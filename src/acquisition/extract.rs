use crate::acquisition::{
    DriverInfo, FirmwareCategory, FirmwareFile, HalInfo, PartitionInfo,
};
use anyhow::Context;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct CategorizedFiles {
    pub kernel_configs: Vec<String>,
    pub modules: Vec<String>,
    pub drivers: Vec<DriverInfo>,
    pub hals: Vec<HalInfo>,
    pub firmwares: Vec<FirmwareFile>,
}

pub fn extract_all_partitions(
    partitions: &[PartitionInfo],
    output_dir: &Path,
) -> anyhow::Result<Vec<ExtractedFile>> {
    let mut all_files = Vec::new();
    let base = output_dir.join("fs_extracted");
    fs::create_dir_all(&base)?;

    for partition in partitions {
        let part_dir = base.join(&partition.name);
        fs::create_dir_all(&part_dir)?;

        let Some(ref img_path) = partition.extracted_path else {
            continue;
        };

        if !img_path.exists() {
            tracing::warn!("Image not found: {}", img_path.display());
            continue;
        }

        match partition.fs_type.as_str() {
            "ext4" => {
                extract_ext4(img_path, &part_dir, &mut all_files)?;
            }
            "erofs" => {
                extract_erofs(img_path, &part_dir, &mut all_files)?;
            }
            _ => {
                tracing::debug!(
                    "Skipping extraction for {} (fs_type={})",
                    partition.name, partition.fs_type
                );
            }
        }
    }

    Ok(all_files)
}

fn extract_ext4(
    img_path: &Path,
    output_dir: &Path,
    files: &mut Vec<ExtractedFile>,
) -> anyhow::Result<()> {
    use ext4_view::Ext4;

    let data = fs::read(img_path).context("Failed to read ext4 image")?;
    let ext4 = Ext4::load(Box::new(data))
        .map_err(|e| anyhow::anyhow!("ext4 load error: {e}"))?;

    walk_ext4_dir(&ext4, "/", output_dir, files, 0)?;

    Ok(())
}

fn walk_ext4_dir(
    ext4: &ext4_view::Ext4,
    path_str: &str,
    output_dir: &Path,
    files: &mut Vec<ExtractedFile>,
    depth: usize,
) -> anyhow::Result<()> {
    if depth > 32 {
        return Ok(());
    }

    let entries = match ext4.read_dir(path_str) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let entry_path = entry.path();
        let name = entry_path
            .to_str()
            .ok()
            .and_then(|s| std::path::Path::new(s).file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if name == "." || name == ".." || name.is_empty() {
            continue;
        }

        let metadata = match ext4.metadata(&entry_path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if metadata.is_dir() {
            let sub_path = entry_path.to_str().unwrap_or("/");
            walk_ext4_dir(ext4, sub_path, output_dir, files, depth + 1)?;
        } else if metadata.file_type() == ext4_view::FileType::Regular || metadata.is_symlink() {
            let rel_path = entry_path.to_str().unwrap_or("");
            let rel_path = rel_path.strip_prefix('/').unwrap_or(rel_path);
            let out_path = output_dir.join(rel_path);
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }

            match ext4.read(&entry_path) {
                Ok(data) => {
                    fs::write(&out_path, &data)?;
                    files.push(ExtractedFile {
                        path: out_path,
                        original_path: PathBuf::from(rel_path),
                        size: data.len() as u64,
                    });
                }
                Err(e) => {
                    tracing::warn!("Failed to read ext4 file: {e}");
                }
            }
        }
    }

    Ok(())
}

fn extract_erofs(
    img_path: &Path,
    output_dir: &Path,
    files: &mut Vec<ExtractedFile>,
) -> anyhow::Result<()> {
    use fs_core::BlockRead;
    use fs_erofs::Filesystem;

    struct MemDev(Mutex<Vec<u8>>);

    impl BlockRead for MemDev {
        fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::result::Result<(), fs_core::Error> {
            let v = self.0.lock().unwrap();
            let start = offset as usize;
            let end = start + buf.len();
            if end > v.len() {
                return Err(fs_core::Error::ShortRead {
                    offset,
                    want: buf.len(),
                    got: v.len().saturating_sub(start),
                });
            }
            buf.copy_from_slice(&v[start..end]);
            Ok(())
        }

        fn size_bytes(&self) -> u64 {
            self.0.lock().unwrap().len() as u64
        }
    }

    let data = fs::read(img_path).context("Failed to read EROFS image")?;
    let dev: Arc<dyn BlockRead> = Arc::new(MemDev(Mutex::new(data)));

    let fs = Filesystem::open(dev)
        .map_err(|e| anyhow::anyhow!("erofs open error: {e}"))?;

    let root_inode = fs
        .lookup_path("/")
        .map_err(|e| anyhow::anyhow!("erofs root lookup error: {e}"))?;

    walk_erofs_dir(&fs, &root_inode, "/", output_dir, files, 0)?;

    Ok(())
}

fn walk_erofs_dir(
    fs: &fs_erofs::Filesystem,
    inode: &fs_erofs::Inode,
    path_str: &str,
    output_dir: &Path,
    files: &mut Vec<ExtractedFile>,
    depth: usize,
) -> anyhow::Result<()> {
    if depth > 32 {
        return Ok(());
    }

    if !inode.is_dir() {
        return Ok(());
    }

    let entries = fs
        .read_dir(inode)
        .map_err(|e| anyhow::anyhow!("erofs read_dir error: {e}"))?;

    for entry in &entries {
        let name = String::from_utf8_lossy(&entry.name).to_string();
        if name == "." || name == ".." || name.is_empty() {
            continue;
        }

        let child_path = if path_str == "/" {
            format!("/{name}")
        } else {
            format!("{path_str}/{name}")
        };

        let child_inode: fs_erofs::Inode = match fs.read_inode(entry.nid) {
            Ok(i) => i,
            Err(_) => continue,
        };

        if child_inode.is_dir() {
            walk_erofs_dir(fs, &child_inode, &child_path, output_dir, files, depth + 1)?;
        } else if child_inode.is_regular_file() {
            let rel_path = child_path.strip_prefix('/').unwrap_or(&child_path);
            let out_path = output_dir.join(rel_path);
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut data = vec![0u8; child_inode.size as usize];
            match fs.read_file(&child_inode, 0, &mut data) {
                Ok(()) => {
                    fs::write(&out_path, &data)?;
                    files.push(ExtractedFile {
                        path: out_path,
                        original_path: PathBuf::from(&child_path),
                        size: data.len() as u64,
                    });
                }
                Err(e) => {
                    tracing::warn!("Failed to read erofs file: {e}");
                }
            }
        }
    }

    Ok(())
}

pub struct ExtractedFile {
    pub path: PathBuf,
    pub original_path: PathBuf,
    pub size: u64,
}

pub fn categorize_files(files: &[ExtractedFile]) -> CategorizedFiles {
    let mut kernel_configs = Vec::new();
    let mut modules = Vec::new();
    let mut drivers: Vec<DriverInfo> = Vec::new();
    let mut hals: Vec<HalInfo> = Vec::new();
    let mut firmwares: Vec<FirmwareFile> = Vec::new();

    for file in files {
        let path_str = file.path.to_string_lossy().to_lowercase();
        let name = file
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if name == ".config" || path_str.contains("proc/config.gz") {
            if let Ok(data) = fs::read_to_string(&file.path) {
                kernel_configs.extend(
                    data.lines()
                        .filter(|l| l.starts_with("CONFIG_"))
                        .map(|l| l.to_string()),
                );
            }
        }

        if path_str.ends_with(".ko") {
            modules.push(name.to_string());
            drivers.push(DriverInfo {
                name: name.trim_end_matches(".ko").to_string(),
                module: Some(name.to_string()),
                device_type: "kernel_module".into(),
                dt_compatible: vec![],
            });
        }

        if path_str.contains("/lib/firmware/") || path_str.contains("/etc/firmware/") {
            if let Ok(data) = fs::read(&file.path) {
                let hash = {
                    let mut hasher = Sha256::new();
                    hasher.update(&data);
                    format!("{:x}", hasher.finalize())
                };
                firmwares.push(FirmwareFile {
                    path: file.path.clone(),
                    size: file.size,
                    sha256: hash,
                    category: FirmwareCategory::Firmware,
                });
            }
        }

        if path_str.contains("vendor/etc/vintf/") || path_str.contains("manifest.xml") {
            hals.push(HalInfo {
                name: name.to_string(),
                version: "unknown".into(),
                interfaces: vec![],
            });
        }

        if path_str.contains("vendor/lib") || path_str.contains("vendor/lib64") {
            if path_str.ends_with(".so") {
                let is_hal = path_str.contains("hw/")
                    || path_str.contains("camera")
                    || path_str.contains("audio")
                    || path_str.contains("sensors")
                    || path_str.contains("gatekeeper")
                    || path_str.contains("keymaster")
                    || path_str.contains("graphics")
                    || path_str.contains("media");

                if is_hal {
                    hals.push(HalInfo {
                        name: name.to_string(),
                        version: "vendor".into(),
                        interfaces: vec![path_str.clone()],
                    });
                } else {
                    drivers.push(DriverInfo {
                        name: name.to_string(),
                        module: None,
                        device_type: "shared_library".into(),
                        dt_compatible: vec![],
                    });
                }
            }
        }
    }

    CategorizedFiles {
        kernel_configs,
        modules,
        drivers,
        hals,
        firmwares,
    }
}
