use std::collections::HashMap;

pub fn guess_register_name(offset: u32, classification: Option<&str>) -> Option<String> {
    let class = classification.unwrap_or("");

    let names: HashMap<u32, Vec<&str>> = [
        (0x00, vec!["status", "version", "id", "tx", "data"]),
        (0x04, vec!["control", "command", "enable", "rx", "dir"]),
        (0x08, vec!["data", "tx_data", "dout", "config"]),
        (0x0C, vec!["rx_data", "din", "status2", "baud"]),
        (0x10, vec!["irq_mask", "intr_mask", "dma_src"]),
        (0x14, vec!["irq_status", "intr_status", "dma_dst"]),
        (0x18, vec!["dma_count", "fifo", "timeout"]),
        (0x1C, vec!["dma_ctrl", "reset", "scratch"]),
        (0x20, vec!["dma_next", "addr_low"]),
        (0x24, vec!["addr_high", "dma_cfg"]),
        (0x28, vec!["polling_status", "rx_desc"]),
        (0x30, vec!["irq_clear", "eoi"]),
        (0x34, vec!["clock_div", "clk"]),
        (0x40, vec!["fifo_data", "burst"]),
        (0x44, vec!["fifo_status", "threshold"]),
        (0x50, vec!["power_ctrl", "pm"]),
        (0x54, vec!["wakeup", "sleep"]),
        (0x60, vec!["test", "debug"]),
        (0x64, vec!["debug2", "scratch2"]),
        (0xFC, vec!["revision", "id2"]),
    ]
    .into_iter()
    .collect();

    if let Some(candidates) = names.get(&offset) {
        for cand in candidates {
            if class.contains("gpio") && ["dir", "data", "enable", "irq_mask", "irq_status"].contains(cand) {
                return Some(cand.to_string());
            }
            if class.contains("uart") && ["tx", "rx", "baud", "status", "control"].contains(cand) {
                return Some(cand.to_string());
            }
            if class.contains("i2c") && ["data", "control", "status", "clock"].contains(cand) {
                return Some(cand.to_string());
            }
            if class.contains("spi") && ["data", "control", "status", "config"].contains(cand) {
                return Some(cand.to_string());
            }
            if class.contains("dma") && ["dma_src", "dma_dst", "dma_count", "dma_ctrl"].contains(cand) {
                return Some(cand.to_string());
            }
            if class.contains("ethernet") || class.contains("eth") {
                if ["status", "control", "tx_data", "rx_data", "rx_desc"].contains(cand) {
                    return Some(cand.to_string());
                }
            }
        }
        return Some(candidates[0].to_string());
    }

    None
}
