use crate::error::MdlError;
use crate::model::model::Model;
use crate::parser::geoset::geoset_parse;
use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

const MDX_MAGIC: &[u8; 4] = b"MDLX";
const SUPPORTED_VERSION: u32 = 800;

pub fn load(file: &mut File) -> Result<Model, MdlError> {
    let mut model = Model::default();
    model.name = "MDX Model".to_string();

    file.seek(SeekFrom::Start(0))?;

    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if &magic != MDX_MAGIC {
        return Err(
            MdlError::new("unsupported-magic").with_arg("magic", String::from_utf8_lossy(&magic))
        );
    }

    let mut seen_vers = false;

    loop {
        let mut chunk_type = [0u8; 4];
        if file.read_exact(&mut chunk_type).is_err() {
            break;
        }

        let size = file.read_u32::<LittleEndian>()?;
        let start_pos = file.seek(SeekFrom::Current(0))?;

        match &chunk_type {
            b"VERS" => {
                if size < 4 {
                    return Err(MdlError::new("missing-vers"));
                }
                let version = file.read_u32::<LittleEndian>()?;
                if version != SUPPORTED_VERSION {
                    return Err(MdlError::new("unsupported-version").with_arg("version", version));
                }
                seen_vers = true;
                println!("MDX Version: {}", version);
            }
            b"MODL" => crate::parser::chunks::read_modl(file, &mut model, size)?,
            b"GEOS" => {
                println!("Reading GEOS chunk, size: {}", size);
                geoset_parse(file, &mut model, size)?;
                println!("Loaded {} geosets", model.geosets.len());
            }
            b"SEQS" => {
                crate::parser::parser::read_sequences(file, &mut model, size)?;
                println!("Loaded {} sequences", model.sequences.len());
            }
            b"TEXS" => {
                crate::parser::parser::read_textures(file, &mut model, size)?;
                println!("Loaded {} textures", model.textures.len());
            }
            b"BONE" => crate::parser::parser::read_bones(file, &mut model, size)?,
            b"HELP" => crate::parser::parser::read_helpers(file, &mut model, size)?,
            b"PIVT" => crate::parser::parser::read_pivots(file, &mut model, size)?,
            b"MTLS" => crate::parser::parser::read_materials(file, &mut model, size)?,
            b"GLBS" => crate::parser::chunks::read_global_sequences(file, &mut model, size)?,
            b"TXAN" => crate::parser::chunks::read_texture_anims(file, &mut model, size)?,
            b"GEOA" => crate::parser::chunks::read_geoset_anims(file, &mut model, size)?,
            b"LITE" => crate::parser::chunks::read_lights(file, &mut model, size)?,
            b"ATCH" => crate::parser::chunks::read_attachments(file, &mut model, size)?,
            b"PREM" => crate::parser::chunks::read_particle_emitters(file, &mut model, size)?,
            b"PRE2" => crate::parser::chunks::read_particle_emitters_2(file, &mut model, size)?,
            b"RIBB" => crate::parser::chunks::read_ribbons(file, &mut model, size)?,
            b"CAMS" => crate::parser::chunks::read_cameras(file, &mut model, size)?,
            b"EVTS" => crate::parser::chunks::read_events(file, &mut model, size)?,
            b"CLID" => crate::parser::chunks::read_collisions(file, &mut model, size)?,
            other => crate::parser::chunks::read_unknown_chunk(file, &mut model, *other, size)?,
        }

        // Ensure we're at the correct position after reading the chunk
        let current_pos = file.seek(SeekFrom::Current(0))?;
        let expected_pos = start_pos + size as u64;
        if current_pos < expected_pos {
            file.seek(SeekFrom::Start(expected_pos))?;
        }
    }

    if !seen_vers {
        return Err(MdlError::new("missing-vers"));
    }

    Ok(model)
}
