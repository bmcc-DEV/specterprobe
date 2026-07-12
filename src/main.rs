use clap::Parser;
use specter_probe::acquisition;
use specter_probe::behavior;
use specter_probe::compat;
use specter_probe::generate;
use specter_probe::genome;
use specter_probe::knowledge;
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

    #[arg(short = 'k', long)]
    knowledge: bool,

    #[arg(long)]
    neo4j: bool,

    #[arg(short = 'c', long)]
    compat: bool,

    #[arg(short = 'd', long)]
    genome: bool,

    #[arg(short = 'e', long)]
    emulator: bool,

    #[arg(short = 'v', long, default_value = "info")]
    verbose: tracing::Level,
}

fn basic_compat_model(name: &str) -> specter_probe::compat::types::CompatOutput {
    use specter_probe::compat::types::*;
    let name_lower = name.to_lowercase();
    CompatOutput {
        gpu: None,
        isp: if name_lower.contains("ch") || name_lower.contains("isp") || name_lower.contains("camera") {
            Some(IspModel {
                resolution: None, input_format: "unknown".into(),
                output_format: "JPEG".into(), black_level: None,
                pipeline: vec![], confidence: 0.2,
            })
        } else { None },
        dsp: if name_lower.contains("dsp") || name_lower.contains("audio") {
            Some(DspModel {
                sample_rate: None, channels: Some(2), bit_depth: Some(16),
                buffer_size: None, ping_pong: false,
                volume_register: None,
                has_modem: name_lower.contains("modem"),
                confidence: 0.3,
            })
        } else { None },
        backend_code: None,
    }
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

    tracing::info!("Specter Probe — EAEA Embedded Architecture Exploration Agent");
    tracing::info!("Firmware: {}", cli.firmware.display());

    let config = acquisition::AcquireConfig {
        firmware_path: cli.firmware.clone(),
        output_dir: cli.output.clone(),
        extract_fs: cli.extract,
        adb_probe: cli.adb,
    };

    let acquire_output = acquisition::acquire(&config)?;
    std::fs::create_dir_all(&cli.output)?;

    let dtb_info = acquire_output.kernel.dtb.as_ref();

    let mut last_lift: Option<lift::types::LiftOutput> = None;
    let mut last_mmio: Option<mmio::types::MmioMap> = None;
    let mut last_behavior: Option<behavior::types::BehaviorOutput> = None;
    let mut last_compat: Option<compat::types::CompatOutput> = None;

    if cli.lift || cli.mmio || cli.behavior || cli.generate || cli.knowledge || cli.compat || cli.genome || cli.emulator {
        for partition in &acquire_output.partitions {
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

            let need_lift = cli.lift || cli.mmio || cli.behavior || cli.generate || cli.knowledge || cli.compat || cli.genome || cli.emulator;
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

            if cli.mmio || cli.behavior || cli.generate || cli.knowledge || cli.compat || cli.genome || cli.emulator {
                if let Some(ref lo) = lift_output {
                    let mmio_map = mmio::discover(&lo.functions, dtb_info);

                    if cli.mmio {
                        let mmio_json = serde_json::to_string_pretty(&mmio_map)?;
                        std::fs::write(cli.output.join(format!("mmio_{}.json", partition.name)), &mmio_json)?;
                        tracing::info!("MMIO Discovery: {} regions found", mmio_map.regions.len());
                    }

                    if cli.behavior || cli.generate || cli.knowledge || cli.compat || cli.genome || cli.emulator {
                        let behavior_output = behavior::model_devices(&mmio_map);
                        let behavior_json = serde_json::to_string_pretty(&behavior_output)?;
                        std::fs::write(cli.output.join(format!("behavior_{}.json", partition.name)), &behavior_json)?;
                        tracing::info!("Behavioral Modeling: {} device models", behavior_output.devices.len());

                        if cli.generate {
                            generate::generate_drivers(&behavior_output, &cli.output)?;
                        }

                        if cli.emulator {
                            generate::generate_emulators(&behavior_output, &cli.output)?;
                        }

                            if cli.compat || cli.genome {
                                let compat_output = compat::detect_all(&lo.functions, &mmio_map);
                                let compat_json = serde_json::to_string_pretty(&compat_output)?;
                                std::fs::write(cli.output.join(format!("compat_{}.json", partition.name)), &compat_json)?;
                                if let Some(ref backend) = compat_output.backend_code {
                                    std::fs::write(cli.output.join(format!("compat_{}.rs", partition.name)), backend)?;
                                }
                                last_compat = Some(compat_output);
                            }

                        if !behavior_output.devices.is_empty() {
                            last_lift = lift_output;
                            last_mmio = Some(mmio_map);
                            last_behavior = Some(behavior_output);
                        }
                    }
                }
            }
        }
    }

    if cli.compat {
        for partition in &acquire_output.partitions {
            if partition.fs_type != "firmware" {
                continue;
            }
            let Some(ref fw_path) = partition.extracted_path else { continue };
            if !fw_path.exists() { continue; }
            let data = match std::fs::read(fw_path) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let data = if data.len() > 10 * 1024 * 1024 {
                data[..10 * 1024 * 1024].to_vec()
            } else {
                data
            };
            let name = &partition.name;
            let lift_output = lift::lift_binary(&data);

            if lift_output.total_instructions == 0 {
                let basic = basic_compat_model(name);
                let json = serde_json::to_string_pretty(&basic)?;
                std::fs::write(cli.output.join(format!("compat_{}.json", name)), &json)?;
            } else {
                let empty_mmio = mmio::types::MmioMap::new();
                let compat_output = compat::detect_all(&lift_output.functions, &empty_mmio);
                let json = serde_json::to_string_pretty(&compat_output)?;
                std::fs::write(cli.output.join(format!("compat_{}.json", name)), &json)?;
                if let Some(ref code) = compat_output.backend_code {
                    std::fs::write(cli.output.join(format!("compat_{}.rs", name)), code)?;
                }
            }
        }
    }

    if cli.knowledge || cli.genome {
        let lift_ref = last_lift.as_ref();
        let mmio_ref = last_mmio.as_ref();
        let beh_ref = last_behavior.as_ref();
        let compat_ref = last_compat.as_ref();

        if let (Some(lift_out), Some(mmio_out), Some(beh_out)) = (lift_ref, mmio_ref, beh_ref) {
            if cli.knowledge || cli.genome {
                let kg = knowledge::build_knowledge_graph(
                    &acquire_output, lift_out, mmio_out, beh_out, cli.neo4j,
                )?;

                if cli.knowledge {
                    let kg_json = serde_json::to_string_pretty(&kg)?;
                    std::fs::write(cli.output.join("knowledge_graph.json"), &kg_json)?;

                    if cli.neo4j {
                        let cypher = knowledge::export::export_cypher(&kg);
                        std::fs::write(cli.output.join("knowledge_graph.cql"), &cypher)?;
                        tracing::info!("CYPHER script written to knowledge_graph.cql");
                    }

                    let functions = knowledge::query::find_all_functions(&kg);
                    let drivers = knowledge::query::find_all_drivers(&kg);
                    let registers = knowledge::query::find_all_registers(&kg);

                    tracing::info!("Knowledge Graph queries:");
                    tracing::info!("  {}", functions.description);
                    tracing::info!("  {}", drivers.description);
                    tracing::info!("  {}", registers.description);
                }

                if cli.genome {
                    if let Some(compat_out) = compat_ref {
                        genome::generate_all(
                            beh_out, compat_out, &kg, lift_out, mmio_out, &cli.output,
                        )?;
                    } else {
                        tracing::warn!("Device Genome requires compat data (use -c)");
                    }
                }
            }
        } else {
            tracing::warn!("Knowledge Graph/Genome requires at least one partition with kernel");
        }
    }

    let json_path = cli.output.join("firmware_manifest.json");
    let json = serde_json::to_string_pretty(&acquire_output)?;
    std::fs::write(&json_path, &json)?;

    tracing::info!("Scan complete. Manifest written to {}", json_path.display());
    println!("{}", json);

    Ok(())
}
