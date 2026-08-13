use crate::error::MdlError;
use crate::model::model::Model;
use crate::parser::geoset::geoset_parse;
use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

const MDX_MAGIC: &[u8; 4] = b"MDLX";
const SUPPORTED_VERSION: u32 = 800;

pub fn load(file: &mut File) -> Result<Model, MdlError> {
    let mut model = Model {
        name: "MDX Model".to_string(),
        ..Model::default()
    };

    file.seek(SeekFrom::Start(0))?;
    let file_len = file.metadata()?.len();

    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if &magic != MDX_MAGIC {
        return Err(
            MdlError::new("unsupported-magic").with_arg("magic", String::from_utf8_lossy(&magic))
        );
    }

    let mut seen_vers = false;

    loop {
        let header_pos = file.stream_position()?;
        let remaining = file_len.saturating_sub(header_pos);
        if remaining == 0 {
            break;
        }
        if remaining < 8 {
            return Err(MdlError::new("mdx-truncated-chunk-header")
                .with_arg("offset", header_pos)
                .with_arg("remaining", remaining));
        }
        let mut chunk_type = [0u8; 4];
        file.read_exact(&mut chunk_type)?;

        let size = file.read_u32::<LittleEndian>()?;
        let start_pos = file.stream_position()?;
        let expected_pos = start_pos + size as u64;
        if expected_pos > file_len {
            return Err(MdlError::new("mdx-truncated-chunk")
                .with_arg("fourcc", String::from_utf8_lossy(&chunk_type))
                .with_arg("size", size)
                .with_arg("remaining", file_len - start_pos));
        }

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
            b"MDVI" => {
                if model.mdlvis_data.is_some() {
                    return Err(MdlError::new("mdx-duplicate-mdvi"));
                }
                let mut data = vec![0u8; size as usize];
                file.read_exact(&mut data)?;
                model.mdlvis_data = Some(data);
            }
            other => crate::parser::chunks::read_unknown_chunk(file, &mut model, *other, size)?,
        }

        // Ensure we're at the correct position after reading the chunk
        let current_pos = file.stream_position()?;
        if current_pos < expected_pos {
            file.seek(SeekFrom::Start(expected_pos))?;
        } else if current_pos > expected_pos {
            return Err(MdlError::new("mdx-chunk-overread")
                .with_arg("fourcc", String::from_utf8_lossy(&chunk_type))
                .with_arg("expected_end", expected_pos)
                .with_arg("actual_end", current_pos));
        }
    }

    if !seen_vers {
        return Err(MdlError::new("missing-vers"));
    }

    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(label: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "mdlvis-rs-mdx-load-{label}-{}-{stamp}.mdx",
            std::process::id()
        ))
    }

    fn mdx_with_chunks(chunks: &[([u8; 4], &[u8])]) -> Vec<u8> {
        let mut bytes = b"MDLX".to_vec();
        bytes.extend_from_slice(b"VERS");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&SUPPORTED_VERSION.to_le_bytes());
        for (fourcc, payload) in chunks {
            bytes.extend_from_slice(fourcc);
            bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            bytes.extend_from_slice(payload);
        }
        bytes
    }

    fn load_bytes(label: &str, bytes: &[u8]) -> Result<Model, MdlError> {
        let path = temp_path(label);
        fs::write(&path, bytes).expect("write temporary MDX");
        let mut file = File::open(&path).expect("open temporary MDX");
        let result = load(&mut file);
        fs::remove_file(path).expect("remove temporary MDX");
        result
    }

    #[test]
    fn mdvi_duplicate_chunks_are_rejected() {
        let bytes = mdx_with_chunks(&[(*b"MDVI", b"first"), (*b"MDVI", b"second")]);
        let err = load_bytes("duplicate-mdvi", &bytes).expect_err("duplicate MDVI must fail");
        assert_eq!(err.key, "mdx-duplicate-mdvi");
    }

    #[test]
    fn mdvi_truncation_and_partial_chunk_headers_are_rejected_without_panicking() {
        let mut truncated_mdvi = mdx_with_chunks(&[]);
        truncated_mdvi.extend_from_slice(b"MDVI");
        truncated_mdvi.extend_from_slice(&4u32.to_le_bytes());
        truncated_mdvi.extend_from_slice(&[1, 2]);
        let result = std::panic::catch_unwind(|| load_bytes("truncated-mdvi", &truncated_mdvi));
        let err = result
            .expect("truncated MDVI must not panic")
            .expect_err("truncated MDVI must fail");
        assert_eq!(err.key, "mdx-truncated-chunk");

        let mut partial_header = mdx_with_chunks(&[]);
        partial_header.extend_from_slice(b"MD");
        let err = load_bytes("partial-header", &partial_header)
            .expect_err("partial chunk header must fail");
        assert_eq!(err.key, "mdx-truncated-chunk-header");
    }

    #[test]
    fn truncated_unknown_chunk_payload_is_rejected_without_panicking() {
        let mut bytes = mdx_with_chunks(&[]);
        bytes.extend_from_slice(b"ZZZZ");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&[1, 2]);
        let result = std::panic::catch_unwind(|| load_bytes("truncated-unknown", &bytes));
        let err = result
            .expect("truncated unknown chunk must not panic")
            .expect_err("truncated unknown chunk must fail");
        assert_eq!(err.key, "mdx-truncated-chunk");
    }
}
