use crate::error::MdlError;
use crate::model::model::Model;
use crate::parser::geoset::geoset_parse;
use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub fn load(file: &mut File) -> Result<Model, MdlError> {
    let mut model = Model::default();
    model.name = "MDX Model".to_string();

    file.seek(SeekFrom::Start(4))?;

    loop {
        let mut chunk_type = [0u8; 4];
        if file.read_exact(&mut chunk_type).is_err() {
            break;
        }

        let size = file.read_u32::<LittleEndian>()?;
        let start_pos = file.seek(SeekFrom::Current(0))?;

        match &chunk_type {
            b"VERS" => {
                // Version chunk
                let version = file.read_u32::<LittleEndian>()?;
                println!("MDX Version: {}", version);
            }
            b"MODL" => {
                // Model header - skip 8 bytes, then read 336 bytes for name
                file.seek(SeekFrom::Current(8))?;
                let mut name_bytes = [0u8; 336];
                file.read_exact(&mut name_bytes)?;
                model.name =
                    String::from_utf8(name_bytes.into_iter().take_while(|&b| b != 0).collect())
                        .unwrap_or_else(|_| "Unknown".to_string());
                println!("Model name: {}", model.name.trim());
            }
            b"GEOS" => {
                // Geosets - this chunk contains multiple geosets
                println!("Reading GEOS chunk, size: {}", size);
                geoset_parse(file, &mut model, size)?;
                println!("Loaded {} geosets", model.geosets.len());
            }
            b"SEQS" => {
                // Sequences
                crate::parser::parser::read_sequences(file, &mut model, size)?;
                println!("Loaded {} sequences", model.sequences.len());
            }
            b"TEXS" => {
                // Textures
                crate::parser::parser::read_textures(file, &mut model, size)?;
                println!("Loaded {} textures", model.textures.len());
            }
            b"BONE" => {
                // Bones
                crate::parser::parser::read_bones(file, &mut model, size)?;
            }
            b"HELP" => {
                // Helpers
                crate::parser::parser::read_helpers(file, &mut model, size)?;
            }
            b"PIVT" => {
                // Pivot points
                crate::parser::parser::read_pivots(file, &mut model, size)?;
            }
            b"MTLS" => {
                // Materials
                crate::parser::parser::read_materials(file, &mut model, size)?;
            }
            _ => {
                // Skip unknown chunk
                file.seek(SeekFrom::Current(size as i64))?;
            }
        }

        // Ensure we're at the correct position after reading the chunk
        let current_pos = file.seek(SeekFrom::Current(0))?;
        let expected_pos = start_pos + size as u64;
        if current_pos < expected_pos {
            file.seek(SeekFrom::Start(expected_pos))?;
        }
    }

    Ok(model)
}
