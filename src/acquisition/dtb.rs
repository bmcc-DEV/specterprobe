use crate::acquisition::{
    ClockMap, DmaController, DtbInfo, GpioMap, I2cBus, IrqMap, MmioRegion, SpiBus,
};
use anyhow::Context;
use device_tree_parser::DeviceTreeParser;
use std::convert::TryFrom;
use std::path::Path;

pub fn parse_dtb_file(path: &Path) -> anyhow::Result<DtbInfo> {
    let data = std::fs::read(path).context("Failed to read DTB file")?;
    parse_dtb(&data)
}

pub fn parse_dtb(data: &[u8]) -> anyhow::Result<DtbInfo> {
    let parser = DeviceTreeParser::new(data);
    let tree = parser
        .parse_tree()
        .map_err(|e| anyhow::anyhow!("DTB parse error: {e}"))?;

    let compatible = get_compatible_list(&tree);
    let model = tree
        .properties
        .iter()
        .find(|p| p.name == "model")
        .and_then(|p| match &p.value {
            device_tree_parser::PropertyValue::String(s) => Some(s.to_string()),
            _ => None,
        });

    let mut mmio_regions = Vec::new();
    let mut irqs = Vec::new();
    let mut clocks = Vec::new();
    let mut gpios = Vec::new();
    let mut i2c_buses = Vec::new();
    let mut spi_buses = Vec::new();
    let mut dma_controllers = Vec::new();

    walk_node(
        &tree, "",
        &mut mmio_regions, &mut irqs, &mut clocks,
        &mut gpios, &mut i2c_buses, &mut spi_buses, &mut dma_controllers,
    );

    Ok(DtbInfo {
        compatible,
        model,
        mmio_regions,
        irqs,
        clocks,
        gpios,
        i2c_buses,
        spi_buses,
        dma_controllers,
    })
}

fn get_compatible_list(node: &device_tree_parser::DeviceTreeNode) -> Vec<String> {
    node.properties
        .iter()
        .find(|p| p.name == "compatible")
        .and_then(|p| match &p.value {
            device_tree_parser::PropertyValue::StringList(list) => {
                Some(list.iter().map(|s| (*s).to_string()).collect())
            }
            device_tree_parser::PropertyValue::String(s) => {
                Some(vec![s.to_string()])
            }
            _ => None,
        })
        .unwrap_or_default()
}

fn get_reg(node: &device_tree_parser::DeviceTreeNode) -> Vec<(u64, u64)> {
    let reg_prop = match node.properties.iter().find(|p| p.name == "reg") {
        Some(p) => p,
        None => return vec![],
    };

    match &reg_prop.value {
        device_tree_parser::PropertyValue::U32Array(data) => {
            let mut regions = Vec::new();
            if let Ok(values) = Vec::<u32>::try_from(&reg_prop.value) {
                let mut i = 0;
                while i + 1 < values.len() {
                    regions.push((values[i] as u64, values[i + 1] as u64));
                    i += 2;
                }
            } else if data.len() >= 12 {
                let addr = u64::from_be_bytes([
                    data[0], data[1], data[2], data[3],
                    data[4], data[5], data[6], data[7],
                ]);
                let size = u64::from(u32::from_be_bytes([
                    data[8], data[9], data[10], data[11],
                ]));
                regions.push((addr, size));
            }
            regions
        }
        device_tree_parser::PropertyValue::U64Array(data) => {
            let mut regions = Vec::new();
            for chunk in data.chunks_exact(16) {
                if chunk.len() >= 16 {
                    let addr = u64::from_be_bytes([
                        chunk[0], chunk[1], chunk[2], chunk[3],
                        chunk[4], chunk[5], chunk[6], chunk[7],
                    ]);
                    let size = u64::from_be_bytes([
                        chunk[8], chunk[9], chunk[10], chunk[11],
                        chunk[12], chunk[13], chunk[14], chunk[15],
                    ]);
                    regions.push((addr, size));
                }
            }
            regions
        }
        _ => vec![],
    }
}

fn walk_node(
    node: &device_tree_parser::DeviceTreeNode,
    parent_name: &str,
    mmio_regions: &mut Vec<MmioRegion>,
    irqs: &mut Vec<IrqMap>,
    _clocks: &mut Vec<ClockMap>,
    gpios: &mut Vec<GpioMap>,
    i2c_buses: &mut Vec<I2cBus>,
    spi_buses: &mut Vec<SpiBus>,
    dma_controllers: &mut Vec<DmaController>,
) {
    let node_name = node.name;
    let full_name = if parent_name.is_empty() {
        node_name.to_string()
    } else {
        format!("{}/{}", parent_name, node_name)
    };

    let compat = get_compatible_list(node);
    let regs = get_reg(node);
    let interrupts = node.prop_u32_array("interrupts").unwrap_or_default();

    for &(addr, size) in &regs {
        mmio_regions.push(MmioRegion {
            address: addr,
            size,
            peripheral: Some(full_name.clone()),
            compatible: compat.clone(),
        });
    }

    for &irq in &interrupts {
        irqs.push(IrqMap {
            irq,
            peripheral: full_name.clone(),
            flags: 0,
        });
    }

    if compat.iter().any(|c| c.contains("gpio")) {
        for &(addr, _) in &regs {
            gpios.push(GpioMap {
                bank: gpios.len() as u32,
                base: addr,
                count: 32,
            });
        }
    }

    if compat.iter().any(|c| c.contains("i2c")) {
        for &(addr, _) in &regs {
            i2c_buses.push(I2cBus {
                bus_id: i2c_buses.len() as u32,
                address: addr,
                clock: None,
            });
        }
    }

    if compat.iter().any(|c| c.contains("spi")) {
        for &(addr, _) in &regs {
            spi_buses.push(SpiBus {
                bus_id: spi_buses.len() as u32,
                address: addr,
                chip_select: 0,
            });
        }
    }

    if compat.iter().any(|c| c.contains("dma")) {
        for &(addr, _) in &regs {
            dma_controllers.push(DmaController {
                address: addr,
                channels: 8,
                interrupts: interrupts.clone(),
            });
        }
    }

    for child in &node.children {
        walk_node(
            child,
            &full_name,
            mmio_regions,
            irqs,
            _clocks,
            gpios,
            i2c_buses,
            spi_buses,
            dma_controllers,
        );
    }
}
