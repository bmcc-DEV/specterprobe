use crate::acquisition::sparse;
use crate::acquisition::PartitionInfo;
use anyhow::Context;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn process_firmware_dir(
    dir: &Path,
    output_dir: &Path,
) -> anyhow::Result<Vec<PartitionInfo>> {
    scan_firmware_dir_recursive(dir, output_dir, 0)
}

fn scan_firmware_dir_recursive(
    dir: &Path,
    output_dir: &Path,
    depth: usize,
) -> anyhow::Result<Vec<PartitionInfo>> {
    if depth > 8 {
        return Ok(vec![]);
    }

    let mut partitions = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let sub = scan_firmware_dir_recursive(&path, output_dir, depth + 1)?;
            partitions.extend(sub);
            continue;
        }

        if !path.is_file() {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let parts = match file_name {
            n if n == "boot.img" || n == "boot_a.img" || n == "boot_b.img"
                || n == "boot-gki.img" || n == "init_boot.img" => {
                parse_boot_image(&path, output_dir).map(|p| vec![p])?
            }
            n if n == "vendor_boot.img" => {
                parse_boot_image(&path, output_dir).map(|p| vec![p])?
            }
            n if n == "super.img" => parse_super_image(&path, output_dir)?,
            n if n.ends_with(".img") && n != "boot.img" && n != "super.img" => {
                let name = n.trim_end_matches(".img");
                parse_generic_image(&path, output_dir, name).map(|p| vec![p])?
            }
            n if n == "dtb" || n == "dtbo" || n.ends_with(".dtb") || n.ends_with(".dtbo") => {
                vec![PartitionInfo {
                    name: file_name.to_string(),
                    size: fs::metadata(&path)?.len(),
                    fs_type: "dtb".into(),
                    extracted_path: Some(path.clone()),
                }]
            }
            n if n == "kernel" || n.starts_with("Image") || n.starts_with("zImage")
                || n == "EXEC_KERNEL_IMAGE.bin" => {
                vec![PartitionInfo {
                    name: "kernel".into(),
                    size: fs::metadata(&path)?.len(),
                    fs_type: "kernel".into(),
                    extracted_path: Some(path.clone()),
                }]
            }
            n if n.ends_with(".bin") && !n.contains("Gpt_entry")
                && !n.contains("u-boot") && !n.contains("fdl1")
                && !n.contains("lk-") && !n.contains("sml-")
                && !n.contains("teecfg") && !n.contains("tos-") => {
                let name = n.trim_end_matches(".bin");
                vec![PartitionInfo {
                    name: format!("firmware_{}", name),
                    size: fs::metadata(&path)?.len(),
                    fs_type: "firmware".into(),
                    extracted_path: Some(path.clone()),
                }]
            }
            _ => continue,
        };

        partitions.extend(parts);
    }

    Ok(partitions)
}

pub fn process_firmware_zip(
    zip_path: &Path,
    output_dir: &Path,
) -> anyhow::Result<Vec<PartitionInfo>> {
    let temp_dir = tempfile::tempdir()?;
    let status = Command::new("unzip")
        .arg("-o")
        .arg(zip_path)
        .arg("-d")
        .arg(temp_dir.path())
        .status()
        .context("Failed to run unzip — is it installed?")?;

    if !status.success() {
        anyhow::bail!("unzip failed");
    }

    process_firmware_dir(temp_dir.path(), output_dir)
}

pub fn parse_boot_image(
    path: &Path,
    output_dir: &Path,
) -> anyhow::Result<PartitionInfo> {
    let data = fs::read(path).context("Failed to read boot image")?;
    let kernel_dir = output_dir.join("boot_extracted");
    fs::create_dir_all(&kernel_dir)?;

    let part = parse_standard_boot_image(&data, &kernel_dir)
        .or_else(|_| parse_raw_kernel_image(&data, &kernel_dir))
        .unwrap_or_else(|_| PartitionInfo {
            name: "boot".into(),
            size: data.len() as u64,
            fs_type: "raw".into(),
            extracted_path: Some(kernel_dir),
        });

    Ok(part)
}

fn parse_standard_boot_image(data: &[u8], kernel_dir: &std::path::Path) -> anyhow::Result<PartitionInfo> {
    use abootimg_oxide::Header;
    let mut cursor = std::io::Cursor::new(data);
    let header = Header::parse(&mut cursor)?;

    let kernel_pos = header.kernel_position();
    let kernel_size = header.kernel_size() as usize;
    if kernel_pos + kernel_size <= data.len() && kernel_size > 0 {
        fs::write(&kernel_dir.join("kernel"), &data[kernel_pos..kernel_pos + kernel_size])?;
    }

    let ramdisk_pos = header.ramdisk_position();
    let ramdisk_size = header.ramdisk_size() as usize;
    if ramdisk_pos + ramdisk_size <= data.len() && ramdisk_size > 0 {
        fs::write(&kernel_dir.join("ramdisk.img"), &data[ramdisk_pos..ramdisk_pos + ramdisk_size])?;
    }

    let page_size = header.page_size();
    let dtb_start = ramdisk_pos + ramdisk_size;
    let dtb_start_aligned = (dtb_start + page_size - 1) / page_size * page_size;
    if dtb_start_aligned < data.len() {
        let dtb_data = &data[dtb_start_aligned..];
        if dtb_data.len() >= 4 && dtb_data[..4] == [0xd0, 0x0d, 0xfe, 0xed] {
            fs::write(&kernel_dir.join("dtb"), dtb_data)?;
        }
    }

    Ok(PartitionInfo {
        name: "boot".into(),
        size: data.len() as u64,
        fs_type: "bootimage".into(),
        extracted_path: Some(kernel_dir.to_path_buf()),
    })
}

fn parse_raw_kernel_image(data: &[u8], kernel_dir: &std::path::Path) -> anyhow::Result<PartitionInfo> {
    if data.len() < 8 || &data[..8] != b"ANDROID!" {
        anyhow::bail!("not a boot image");
    }
    use std::io::{Cursor, Read, Seek, SeekFrom};
    let mut cur = Cursor::new(data);

    cur.seek(SeekFrom::Start(0x28))?;
    let mut ver_buf = [0u8; 4];
    cur.read_exact(&mut ver_buf)?;
    let version = u32::from_le_bytes(ver_buf);

    if version != 4 {
        anyhow::bail!("unsupported version: {}", version);
    }

    cur.seek(SeekFrom::Start(0))?;
    let mut header = [0u8; 0x100];
    cur.read_exact(&mut header)?;

    let page_size = 4096u64;
    let v3_kernel_off = page_size as usize;
    let kernel_size = u32::from_le_bytes(data[0..4].try_into().unwrap_or([0; 4])) as usize;

    let valid_size = kernel_size.min(data.len().saturating_sub(v3_kernel_off));
    if valid_size > 0 {
        fs::write(&kernel_dir.join("kernel"), &data[v3_kernel_off..v3_kernel_off + valid_size])?;
    }

    Ok(PartitionInfo {
        name: "boot".into(),
        size: data.len() as u64,
        fs_type: "bootimage".into(),
        extracted_path: Some(kernel_dir.to_path_buf()),
    })
}

pub fn parse_sparse_image(
    input: &Path,
    output: &Path,
) -> anyhow::Result<PathBuf> {
    use android_sparse::{Decoder, Reader};

    let data = fs::read(input).context("Failed to read sparse image")?;
    let reader = Reader::new(&data[..])
        .map_err(|e| anyhow::anyhow!("Failed to parse sparse image: {e}"))?;

    let raw_file = fs::File::create(output)?;
    let mut decoder = Decoder::new(raw_file)
        .map_err(|e| anyhow::anyhow!("Failed to create sparse decoder: {e}"))?;

    for block in reader {
        let block = block.map_err(|e| anyhow::anyhow!("Sparse block error: {e}"))?;
        decoder.write_block(&block)
            .map_err(|e| anyhow::anyhow!("Decoder write error: {e}"))?;
    }

    Ok(output.to_path_buf())
}

pub fn parse_super_image(
    path: &Path,
    output_dir: &Path,
) -> anyhow::Result<Vec<PartitionInfo>> {
    let data = fs::read(path).context("Failed to read super.img")?;
    let ext_dir = output_dir.join("super_extracted");
    fs::create_dir_all(&ext_dir)?;

    let raw_data = unsparse_if_needed(&data, "super.img")?;

    let raw_file_size = raw_data.len() as u64;
    let mut cursor = std::io::Cursor::new(&raw_data);

    match parse_lp_metadata(&mut cursor, raw_file_size) {
        Ok(metadata) => {
            let mut partitions = Vec::new();
            for entry in &metadata.partitions {
                let part_path = ext_dir.join(&entry.name);
                extract_lp_partition(&mut cursor, entry, &part_path)?;
                partitions.push(PartitionInfo {
                    name: entry.name.clone(),
                    size: entry.size,
                    fs_type: "ext4".into(),
                    extracted_path: Some(part_path),
                });
            }
            Ok(partitions)
        }
        Err(e) => {
            tracing::warn!("super.img LP parse failed ({e}), saving as raw");
            let raw_path = ext_dir.join("super.raw");
            fs::write(&raw_path, &raw_data)?;
            Ok(vec![PartitionInfo {
                name: "super".into(),
                size: raw_file_size,
                fs_type: "raw".into(),
                extracted_path: Some(raw_path),
            }])
        }
    }
}

fn parse_generic_image(
    path: &Path,
    output_dir: &Path,
    name: &str,
) -> anyhow::Result<PartitionInfo> {
    let data = fs::read(path)?;

    if is_sparse_image(&data) {
        let raw_path = output_dir.join(format!("{name}.raw"));
        parse_sparse_image(path, &raw_path)?;
        let fs_type = detect_fs_type(&raw_path)?;
        return Ok(PartitionInfo {
            name: name.into(),
            size: raw_path.metadata()?.len(),
            fs_type,
            extracted_path: Some(raw_path),
        });
    }

    let fs_type = detect_fs_type(path)?;
    Ok(PartitionInfo {
        name: name.into(),
        size: data.len() as u64,
        fs_type,
        extracted_path: Some(path.to_path_buf()),
    })
}

fn is_sparse_image(data: &[u8]) -> bool {
    data.len() >= 28 && data[..4] == [0x3A, 0xFF, 0x26, 0xED]
}

fn detect_fs_type(path: &Path) -> anyhow::Result<String> {
    let data = fs::read(path)?;
    if data.len() < 1028 {
        return Ok("raw".into());
    }

    let ext4_magic = &data[0x438..0x43A];
    if ext4_magic == b"\x53\xEF" {
        return Ok("ext4".into());
    }

    let erofs_magic = &data[0..4];
    if erofs_magic == &[0x00, 0x00, 0x00, 0x00] && data.len() > 1024 {
        let maybe_erofs = &data[1024..1032];
        if maybe_erofs.starts_with(b"\xE2\xE1\xF5") {
            return Ok("erofs".into());
        }
    }

    if path.extension().map_or(false, |e| e == "dtb" || e == "dtbo") {
        return Ok("dtb".into());
    }

    Ok("raw".into())
}

pub fn detect_kernel_version(partition: &PartitionInfo) -> anyhow::Result<String> {
    let path = match &partition.extracted_path {
        Some(p) if p.is_dir() => p.join("kernel"),
        Some(p) => p.clone(),
        None => anyhow::bail!("No path for kernel"),
    };

    if !path.exists() {
        anyhow::bail!("Kernel file not found at {}", path.display());
    }

    let data = fs::read(&path)?;
    extract_linux_version(&data)
        .ok_or_else(|| anyhow::anyhow!("Could not detect kernel version"))
}

fn extract_linux_version(data: &[u8]) -> Option<String> {
    let marker = b"Linux version ";
    if let Some(pos) = data.windows(marker.len()).position(|w| w == marker) {
        let start = pos + marker.len();
        let end = data[start..]
            .iter()
            .position(|&b| b == b'\n' || b == b'\0')
            .unwrap_or(64)
            .min(64);
        if end > 0 {
            let version = std::str::from_utf8(&data[start..start + end]).ok()?;
            return Some(version.trim().to_string());
        }
    }
    None
}

// ─── LP Metadata (super.img) Parser ─────────────────────

const LP_METADATA_GEOMETRY_MAGIC: u32 = 0x616C5030;
const LP_METADATA_HEADER_MAGIC: u32 = 0x414C504D;
const LP_SECTOR_SIZE: u64 = 512;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct LpMetadataGeometry {
    magic: u32,
    struct_size: u32,
    checksum: u32,
    metadata_max_size: u32,
    metadata_slot_count: u32,
    logical_block_size: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct LpMetadataHeader {
    magic: u32,
    major_version: u32,
    minor_version: u32,
    header_size: u32,
    _reserved: [u8; 20],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct LpMetadataPartitionEntry {
    name_offset: u32,
    attributes: u32,
    first_extent_index: u32,
    num_extents: u32,
    group_index: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct LpMetadataExtentEntry {
    num_sectors: u64,
    target_type: u32,
    target_data: u32,
    target_source: u64,
}

#[derive(Debug)]
struct LpMetadata {
    _block_size: u32,
    partitions: Vec<LpPartitionInfo>,
}

#[derive(Debug)]
struct LpPartitionInfo {
    name: String,
    size: u64,
    extents: Vec<LpExtent>,
}

#[derive(Debug)]
struct LpExtent {
    num_sectors: u64,
    target_type: u32,
    _physical_partition: u32,
    physical_sector: u64,
}

fn read_struct<T: Copy, R: Read + Seek>(file: &mut R, offset: u64) -> anyhow::Result<T> {
    file.seek(SeekFrom::Start(offset))?;
    let size = std::mem::size_of::<T>();
    let mut buf = vec![0u8; size];
    file.read_exact(&mut buf)?;
    let val: T = unsafe { std::ptr::read(buf.as_ptr() as *const T) };
    Ok(val)
}

fn parse_lp_geometry<R: Read + Seek>(file: &mut R, file_size: u64) -> anyhow::Result<LpMetadataGeometry> {
    let geometry_size = std::mem::size_of::<LpMetadataGeometry>() as u64;

    for &sector_offset in &[-2i64, -1i64] {
        let pos = (file_size as i64) + (sector_offset * LP_SECTOR_SIZE as i64);
        if pos < 0 {
            continue;
        }
        let pos = pos as u64;
        if pos + geometry_size > file_size {
            continue;
        }

        let geometry: LpMetadataGeometry = match read_struct(file, pos) {
            Ok(g) => g,
            Err(_) => continue,
        };
        if geometry.magic == LP_METADATA_GEOMETRY_MAGIC {
            return Ok(geometry);
        }
    }

    anyhow::bail!("No valid LP geometry found")
}

fn parse_lp_metadata<R: Read + Seek>(file: &mut R, file_size: u64) -> anyhow::Result<LpMetadata> {
    let geometry = parse_lp_geometry(file, file_size)?;
    let block_size = geometry.logical_block_size;
    let metadata_max_size = geometry.metadata_max_size as u64;
    let slot_count = geometry.metadata_slot_count.max(1);

    let slot_size = if block_size > 0 && metadata_max_size > 0 {
        (metadata_max_size + block_size as u64 - 1) / block_size as u64 * block_size as u64
    } else {
        65536
    };

    for slot in 0..slot_count {
        let slot_offset_from_end = slot_size * (slot + 1) as u64 + LP_SECTOR_SIZE * 2;
        if slot_offset_from_end > file_size {
            continue;
        }
        let slot_start = file_size - slot_offset_from_end;

        let header: LpMetadataHeader = match read_struct(file, slot_start) {
            Ok(h) => h,
            Err(_) => continue,
        };

        if header.magic != LP_METADATA_HEADER_MAGIC {
            continue;
        }

        let partitions_offset = slot_start + header.header_size as u64 + 4;
        let entry_size = std::mem::size_of::<LpMetadataPartitionEntry>() as u64;
        let extent_size = std::mem::size_of::<LpMetadataExtentEntry>() as u64;

        file.seek(SeekFrom::Start(partitions_offset - 4))?;
        let mut num_parts_buf = [0u8; 4];
        file.read_exact(&mut num_parts_buf)?;
        let num_partitions = u32::from_le_bytes(num_parts_buf) as usize;

        if num_partitions > 128 {
            continue;
        }

        file.seek(SeekFrom::Start(partitions_offset + num_partitions as u64 * entry_size))?;
        let mut num_exts_buf = [0u8; 4];
        if file.read_exact(&mut num_exts_buf).is_err() {
            continue;
        }
        let num_extents = u32::from_le_bytes(num_exts_buf) as usize;
        if num_extents > 512 {
            continue;
        }

        let mut partitions = Vec::new();

        for i in 0..num_partitions {
            let entry_offset = partitions_offset + i as u64 * entry_size;
            let entry: LpMetadataPartitionEntry = match read_struct(file, entry_offset) {
                Ok(e) => e,
                Err(_) => break,
            };

            let name_offset = partitions_offset + entry.name_offset as u64;
            file.seek(SeekFrom::Start(name_offset))?;
            let mut name_bytes = vec![0u8; 128];
            let n = file.read(&mut name_bytes)?;
            name_bytes.truncate(n);
            let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(name_bytes.len());
            let name = String::from_utf8_lossy(&name_bytes[..name_end]).to_string();

            if name.is_empty() {
                continue;
            }

            let mut extents = Vec::new();
            for j in 0..entry.num_extents as usize {
                let ext_idx = entry.first_extent_index as usize + j;
                let ext_offset = partitions_offset
                    + num_partitions as u64 * entry_size
                    + 4
                    + ext_idx as u64 * extent_size;

                let ext_entry: LpMetadataExtentEntry = match read_struct(file, ext_offset) {
                    Ok(e) => e,
                    Err(_) => break,
                };
                extents.push(LpExtent {
                    num_sectors: ext_entry.num_sectors,
                    target_type: ext_entry.target_type,
                    _physical_partition: ext_entry.target_data,
                    physical_sector: ext_entry.target_source,
                });
            }

            let total_size: u64 = extents.iter().map(|e| e.num_sectors * LP_SECTOR_SIZE).sum();

            partitions.push(LpPartitionInfo {
                name,
                size: total_size,
                extents,
            });
        }

        if !partitions.is_empty() {
            return Ok(LpMetadata {
                _block_size: block_size,
                partitions,
            });
        }
    }

    anyhow::bail!("No valid LP metadata found in any slot")
}

fn extract_lp_partition<R: Read + Seek>(
    file: &mut R,
    partition: &LpPartitionInfo,
    output_path: &Path,
) -> anyhow::Result<()> {
    let mut out = fs::File::create(output_path)?;
    let chunk_size: u64 = 1024 * 1024;

    for extent in &partition.extents {
        if extent.target_type != 0 {
            continue;
        }

        let physical_offset = extent.physical_sector * LP_SECTOR_SIZE;
        let extent_bytes = extent.num_sectors * LP_SECTOR_SIZE;
        let mut remaining = extent_bytes;
        let mut offset = physical_offset;

        while remaining > 0 {
            let read_size = remaining.min(chunk_size) as usize;
            file.seek(SeekFrom::Start(offset))?;
            let mut buf = vec![0u8; read_size];
            file.read_exact(&mut buf)?;
            out.write_all(&buf)?;
            offset += read_size as u64;
            remaining -= read_size as u64;
        }
    }

    Ok(())
}

fn unsparse_if_needed(data: &[u8], name: &str) -> anyhow::Result<Vec<u8>> {
    if !is_sparse_image(data) {
        return Ok(data.to_vec());
    }
    let size_hint = estimate_raw_size(data);
    if size_hint > 2_000_000_000 {
        tracing::warn!("{} is {}GB sparse -> too large for in-memory unsparse, saving raw", name, size_hint / 1_000_000_000);
        return Ok(data.to_vec());
    }
    tracing::info!("{} is sparse ({:.1}MB raw), converting...", name, size_hint as f64 / 1_000_000.0);
    sparse::unsparse_proper(data)
}

fn estimate_raw_size(data: &[u8]) -> u64 {
    if data.len() < 24 {
        return 0;
    }
    let block_size = u32::from_le_bytes(data[12..16].try_into().unwrap_or([0; 4])) as u64;
    let total_blocks = u32::from_le_bytes(data[16..20].try_into().unwrap_or([0; 4])) as u64;
    block_size * total_blocks
}
