

pub fn unsparse_proper(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    if data.len() < 28 || data[0..4] != [0x3A, 0xFF, 0x26, 0xED] {
        anyhow::bail!("Not a valid sparse image");
    }

    let major = u16::from_le_bytes([data[4], data[5]]);
    let block_size = u32::from_le_bytes(data[12..16].try_into()?);
    let total_blocks = u32::from_le_bytes(data[16..20].try_into()?);
    let total_chunks = u32::from_le_bytes(data[20..24].try_into()?);

    if major != 1 {
        anyhow::bail!("Unsupported sparse version: {major}");
    }

    let raw_size = total_blocks as u64 * block_size as u64;
    let mut output = vec![0u8; raw_size as usize];
    let mut out_pos: usize = 0;
    let mut offset: usize = 28;

    for _ in 0..total_chunks {
        if offset + 12 > data.len() {
            anyhow::bail!("Truncated sparse data");
        }

        let chunk_type_raw = u16::from_le_bytes([data[offset], data[offset + 1]]);
        let chunk_type = chunk_type_raw & 0x3;
        let chunk_blocks = u32::from_le_bytes(data[offset + 4..offset + 8].try_into()?);
        let chunk_total_size = u32::from_le_bytes(data[offset + 8..offset + 12].try_into()?);
        let chunk_bytes = chunk_blocks as u64 * block_size as u64;

        match chunk_type {
            0 => {
                let data_start = offset + 12;
                let data_end = data_start + chunk_bytes as usize;
                if data_end > data.len() {
                    anyhow::bail!("RAW chunk truncated at offset {offset}");
                }
                let end = (out_pos + chunk_bytes as usize).min(output.len());
                output[out_pos..end].copy_from_slice(&data[data_start..data_start + (end - out_pos)]);
                out_pos += chunk_bytes as usize;
            }
            1 => {
                if offset + 16 > data.len() {
                    anyhow::bail!("FILL chunk truncated");
                }
                let fill_val = u32::from_le_bytes(data[offset + 12..offset + 16].try_into()?);
                let fill_bytes = fill_val.to_le_bytes();
                let end = (out_pos + chunk_bytes as usize).min(output.len());
                for i in out_pos..end {
                    output[i] = fill_bytes[(i - out_pos) % 4];
                }
                out_pos += chunk_bytes as usize;
            }
            2 => {
                out_pos += chunk_bytes as usize;
            }
            3 => {}
            _ => {
                anyhow::bail!("Unknown chunk type: {}", chunk_type_raw & 0x3);
            }
        }

        offset += chunk_total_size as usize;
    }

    Ok(output)
}
