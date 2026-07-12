use crate::acquisition::ProbeOutput;
use anyhow::Context;
use std::collections::HashMap;
use std::process::Command;

fn run_adb_command(args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("adb")
        .args(args)
        .output()
        .context("Failed to execute adb — is it installed and authorized?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("adb {} failed: {stderr}", args.join(" "));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn run_adb_probe() -> anyhow::Result<ProbeOutput> {
    let properties = match get_all_properties() {
        Ok(props) => props,
        Err(e) => {
            tracing::warn!("ADB getprop failed: {e}");
            HashMap::new()
        }
    };

    let dmesg = match run_adb_command(&["shell", "dmesg"]) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("ADB dmesg failed: {e}");
            String::new()
        }
    };

    let logcat = match run_adb_command(&["logcat", "-d"]) {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("ADB logcat failed: {e}");
            String::new()
        }
    };

    let kernel_version = properties.get("ro.kernel.version").cloned();
    let hardware = properties.get("ro.hardware").cloned();
    let board = properties.get("ro.board.platform").cloned();

    Ok(ProbeOutput {
        properties,
        dmesg,
        logcat,
        kernel_version,
        hardware,
        board,
    })
}

fn get_all_properties() -> anyhow::Result<HashMap<String, String>> {
    let output = run_adb_command(&["shell", "getprop"])?;
    let mut props = HashMap::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with('[') {
            continue;
        }

        if let Some(rest) = line.strip_prefix('[') {
            if let Some(mid) = rest.find("]: [") {
                let key = &rest[..mid];
                let value_start = mid + 4;
                let value = if let Some(end) = rest[value_start..].find(']') {
                    &rest[value_start..value_start + end]
                } else {
                    &rest[value_start..]
                };
                props.insert(key.to_string(), value.to_string());
            }
        }
    }

    Ok(props)
}

impl Default for ProbeOutput {
    fn default() -> Self {
        Self {
            properties: HashMap::new(),
            dmesg: String::new(),
            logcat: String::new(),
            kernel_version: None,
            hardware: None,
            board: None,
        }
    }
}
