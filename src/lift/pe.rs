use std::io::{Read, Seek, SeekFrom};

const DOS_MAGIC: u16 = 0x5A4D;
const PE_MAGIC: u32 = 0x00004550;
const NT_OPTIONAL_HDR64_MAGIC: u16 = 0x020B;
const IMAGE_FILE_MACHINE_ARM64: u16 = 0xAA64;

#[derive(Debug)]
pub struct PeInfo {
    pub entry_point_rva: u32,
    pub sections: Vec<PeSection>,
    pub is_arm64: bool,
}

#[derive(Debug)]
pub struct PeSection {
    pub name: String,
    pub virtual_address: u32,
    pub virtual_size: u32,
    pub raw_data_offset: u32,
    pub raw_data_size: u32,
    pub characteristics: u32,
}

pub fn parse_pe<R: Read + Seek>(reader: &mut R) -> anyhow::Result<PeInfo> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    let magic = u16::from_le_bytes(buf);
    if magic != DOS_MAGIC {
        anyhow::bail!("Not a PE image (bad DOS magic)");
    }

    reader.seek(SeekFrom::Start(0x3C))?;
    reader.read_exact(&mut buf)?;
    let pe_offset = u16::from_le_bytes(buf) as u64;

    reader.seek(SeekFrom::Start(pe_offset))?;
    let mut pe_buf = [0u8; 4];
    reader.read_exact(&mut pe_buf)?;
    let pe_magic = u32::from_le_bytes(pe_buf);
    if pe_magic != PE_MAGIC {
        anyhow::bail!("Not a PE image (bad PE magic)");
    }

    let mut coff = [0u8; 20];
    reader.read_exact(&mut coff)?;
    let machine = u16::from_le_bytes(coff[0..2].try_into().unwrap());
    let num_sections = u16::from_le_bytes(coff[2..4].try_into().unwrap());

    let opt_header_start = reader.stream_position()?;

    let mut opt_magic = [0u8; 2];
    reader.read_exact(&mut opt_magic)?;
    let opt_magic_val = u16::from_le_bytes(opt_magic);

    if opt_magic_val != NT_OPTIONAL_HDR64_MAGIC {
        anyhow::bail!("Not a PE32+ image (bad optional header magic)");
    }

    let entry_point_rva;
    let opt_header_size;

    {
        let mut opt_hdr = vec![0u8; 106];
        reader.read_exact(&mut opt_hdr)?;
        entry_point_rva = u32::from_le_bytes(opt_hdr[16..20].try_into().unwrap());
        let size_of_optional_header =
            u16::from_le_bytes(coff[16..18].try_into().unwrap()) as u64;
        opt_header_size = size_of_optional_header;
    }

    let section_table_offset = opt_header_start + opt_header_size;

    reader.seek(SeekFrom::Start(section_table_offset))?;

    let mut sections = Vec::new();
    for _ in 0..num_sections {
        let mut sec_buf = [0u8; 40];
        if reader.read_exact(&mut sec_buf).is_err() {
            break;
        }

        let name_bytes = &sec_buf[0..8];
        let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(8);
        let name = String::from_utf8_lossy(&name_bytes[..name_end]).to_string();

        let virtual_size = u32::from_le_bytes(sec_buf[8..12].try_into().unwrap());
        let virtual_address = u32::from_le_bytes(sec_buf[12..16].try_into().unwrap());
        let raw_data_size = u32::from_le_bytes(sec_buf[16..20].try_into().unwrap());
        let raw_data_offset = u32::from_le_bytes(sec_buf[20..24].try_into().unwrap());
        let characteristics = u32::from_le_bytes(sec_buf[36..40].try_into().unwrap());

        sections.push(PeSection {
            name,
            virtual_address,
            virtual_size,
            raw_data_offset,
            raw_data_size,
            characteristics,
        });
    }

    Ok(PeInfo {
        entry_point_rva,
        sections,
        is_arm64: machine == IMAGE_FILE_MACHINE_ARM64,
    })
}

pub fn extract_arm64_code<R: Read + Seek>(
    reader: &mut R,
    pe: &PeInfo,
) -> anyhow::Result<Vec<u8>> {
    for section in &pe.sections {
        let is_text = section.name == ".text"
            || section.name == ".TEXT"
            || section.name == "CODE"
            || section.name.contains("text");

        if is_text && section.raw_data_size > 0 {
            reader.seek(SeekFrom::Start(section.raw_data_offset as u64))?;
            let mut code = vec![0u8; section.raw_data_size as usize];
            reader.read_exact(&mut code)?;
            return Ok(code);
        }
    }

    anyhow::bail!("No executable section (.text) found");
}
