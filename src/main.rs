use clap::Parser;
use specter_probe::acquisition;
use specter_probe::behavior;
use specter_probe::generate;
use specter_probe::lift;
use specter_probe::mmio;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "specter-probe", about = "EAEA — Embedded Architecture Exploration Agent")]
struct Cli {
    #[arg(short, long, default_value = ".")]
    firmware: std::path::PathBuf,

    #[arg(short, long, default_value = "./output")]
    output: std::path::PathBuf,

    #[arg(short = 'x', long)]
    extract: bool,

    #[arg(short = 'a', long)]
    adb: bool,

    #[arg(short = 'l', long)]
    lift: bool,

    #[arg(short = 'm', long)]
    mmio: bool,

    #[arg(short = 'b', long)]
    behavior: bool,

    #[arg(short = 'g', long)]
    generate: bool,

    #[arg(short = 'v', long, default_value = "info")]
    verbose: tracing::Level,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .parse(format!("specter_probe={}", cli.verbose))
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Specter Probe — EAEA Firmware Acquisition");
    tracing::info!("Firmware: {}", cli.firmware.display());

    let config = acquisition::AcquireConfig {
        firmware_path: cli.firmware.clone(),
        output_dir: cli.output.clone(),
        extract_fs: cli.extract,
        adb_probe: cli.adb,
    };

    let output = acquisition::acquire(&config)?;
    std::fs::create_dir_all(&cli.output)?;

    let dtb_info = output
        .kernel
        .dtb
        .as_ref();

    if cli.lift || cli.mmio || cli.behavior || cli.generate {
        for partition in &output.partitions {
            let kernel_path = match &partition.extracted_path {
                Some(p) if p.is_dir() => p.join("kernel"),
                Some(p) => p.clone(),
                None => continue,
            };

            if !kernel_path.exists() {
                continue;
            }

            let data = match std::fs::read(&kernel_path) {
                Ok(d) => d,
                Err(_) => continue,
            };

            let need_lift = cli.lift || cli.mmio || cli.behavior || cli.generate;
            let lift_output = if need_lift {
                let lo = lift::lift_binary(&data);
                Some(lo)
            } else {
                None
            };

            if cli.lift {
                if let Some(ref lo) = lift_output {
                    tracing::info!("Lifting {} from {}", partition.name, kernel_path.display());

                    let lift_json = serde_json::to_string_pretty(&lo)?;
                    std::fs::write(cli.output.join(format!("lift_{}.json", partition.name)), &lift_json)?;
                    std::fs::write(cli.output.join(format!("lift_{}.ll", partition.name)), &lo.ir_text)?;

                    tracing::info!(
                        "Lift complete: {} functions, {} instructions",
                        lo.lifted_functions, lo.total_instructions,
                    );
                }
            }

            if cli.mmio || cli.behavior || cli.generate {
                if let Some(ref lo) = lift_output {
                    let mmio_map = mmio::discover(&lo.functions, dtb_info);

                    if cli.mmio {
                        let mmio_json = serde_json::to_string_pretty(&mmio_map)?;
                        std::fs::write(cli.output.join(format!("mmio_{}.json", partition.name)), &mmio_json)?;
                        tracing::info!("MMIO Discovery: {} regions found", mmio_map.regions.len());
                    }

                    if cli.behavior || cli.generate {
                        let behavior_output = behavior::model_devices(&mmio_map);
                        let behavior_json = serde_json::to_string_pretty(&behavior_output)?;
                        std::fs::write(cli.output.join(format!("behavior_{}.json", partition.name)), &behavior_json)?;
                        tracing::info!("Behavioral Modeling: {} device models", behavior_output.devices.len());

                        if cli.generate {
                            generate::generate_drivers(&behavior_output, &cli.output)?;
                        }
                    }
                }
            }
        }
    }

    let json_path = cli.output.join("firmware_manifest.json");
    let json = serde_json::to_string_pretty(&output)?;
    std::fs::write(&json_path, &json)?;

    tracing::info!("Scan complete. Manifest written to {}", json_path.display());
    println!("{}", json);

    Ok(())
}
