use crate::error::MdlError;
use crate::model::model::Model;
use crate::model::skeleton::{AnimationController, Keyframe};
use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub const TAG_KGTR: u32 = 0x5254_474B;
pub const TAG_KGRT: u32 = 0x5452_474B;
pub const TAG_KGSC: u32 = 0x4353_474B;
pub const TAG_KATV: u32 = 0x5654_414B;
pub const TAG_KLAV: u32 = 0x5641_4C4B;
pub const TAG_KMTA: u32 = 0x4154_4D4B;
pub const TAG_KMTF: u32 = 0x4654_4D4B;
pub const TAG_KTAT: u32 = 0x5441_544B;
pub const TAG_KTAR: u32 = 0x5241_544B;
pub const TAG_KTAS: u32 = 0x5341_544B;
pub const TAG_KGAO: u32 = 0x4F41_474B;
pub const TAG_KGAC: u32 = 0x4341_474B;
pub const TAG_KCTR: u32 = 0x5254_434B;
pub const TAG_KCRL: u32 = 0x4C52_434B;
pub const TAG_KTTR: u32 = 0x5254_544B;
pub const TAG_KLAS: u32 = 0x5341_4C4B;
pub const TAG_KLAE: u32 = 0x4541_4C4B;
pub const TAG_KLAI: u32 = 0x4941_4C4B;
pub const TAG_KLAC: u32 = 0x4341_4C4B;
pub const TAG_KLBC: u32 = 0x4342_4C4B;
pub const TAG_KLBI: u32 = 0x4942_4C4B;
pub const TAG_KPEE: u32 = 0x4545_504B;
pub const TAG_KPEG: u32 = 0x4745_504B;
pub const TAG_KPLN: u32 = 0x4E4C_504B;
pub const TAG_KPLT: u32 = 0x544C_504B;
pub const TAG_KPEL: u32 = 0x4C45_504B;
pub const TAG_KPES: u32 = 0x5345_504B;
pub const TAG_KPEV: u32 = 0x5645_504B;
pub const TAG_KP2R: u32 = 0x5232_504B;
pub const TAG_KP2L: u32 = 0x4C32_504B;
pub const TAG_KP2G: u32 = 0x4732_504B;
pub const TAG_KP2V: u32 = 0x5632_504B;
pub const TAG_KP2E: u32 = 0x4532_504B;
pub const TAG_KP2S: u32 = 0x5332_504B;
pub const TAG_KP2N: u32 = 0x4E32_504B;
pub const TAG_KP2W: u32 = 0x5732_504B;
pub const TAG_KRVS: u32 = 0x5356_524B;
pub const TAG_KRHA: u32 = 0x4148_524B;
pub const TAG_KRHB: u32 = 0x4248_524B;
pub const TAG_KRAL: u32 = 0x4C41_524B;
pub const TAG_KRCO: u32 = 0x4F43_524B;

pub fn read_cstring(file: &mut File, len: usize) -> Result<String, MdlError> {
    let mut bytes = vec![0u8; len];
    file.read_exact(&mut bytes)?;
    Ok(
        String::from_utf8(bytes.into_iter().take_while(|&b| b != 0).collect())
            .unwrap_or_else(|_| "Unknown".to_string())
            .trim()
            .to_string(),
    )
}

pub fn read_vec3(file: &mut File) -> Result<[f32; 3], MdlError> {
    Ok([
        file.read_f32::<LittleEndian>()?,
        file.read_f32::<LittleEndian>()?,
        file.read_f32::<LittleEndian>()?,
    ])
}

pub fn skip_to(file: &mut File, pos: u64) -> Result<(), MdlError> {
    file.seek(SeekFrom::Start(pos))?;
    Ok(())
}

pub fn read_controller(
    file: &mut File,
    model: &mut Model,
    expected_tag: u32,
    element_size: usize,
) -> Result<i32, MdlError> {
    read_controller_ex(file, model, expected_tag, element_size, false)
}

/// Reads a controller if the next fourCC matches `expected_tag`.
/// Returns `-1` when the tag is absent (static value).
/// `as_int` is for TextureID tracks (`KMTF`).
pub fn read_controller_ex(
    file: &mut File,
    model: &mut Model,
    expected_tag: u32,
    element_size: usize,
    as_int: bool,
) -> Result<i32, MdlError> {
    let pos_before = file.stream_position()?;
    let tag = match file.read_u32::<LittleEndian>() {
        Ok(tag) => tag,
        Err(_) => {
            file.seek(SeekFrom::Start(pos_before))?;
            return Ok(-1);
        }
    };
    if tag != expected_tag {
        file.seek(SeekFrom::Start(pos_before))?;
        return Ok(-1);
    }

    let keyframe_count = file.read_u32::<LittleEndian>()? as usize;
    let interpolation_type = file.read_u32::<LittleEndian>()?;
    let global_seq_id = file.read_i32::<LittleEndian>()?;
    let controller_idx = model.controllers.len() as i32;
    let mut keyframes = Vec::with_capacity(keyframe_count);

    for _ in 0..keyframe_count {
        let frame = file.read_i32::<LittleEndian>()?;
        let mut data = Vec::with_capacity(element_size);
        for _ in 0..element_size {
            if as_int {
                data.push(file.read_i32::<LittleEndian>()? as f32);
            } else {
                data.push(file.read_f32::<LittleEndian>()?);
            }
        }
        let (in_tan, out_tan) = if interpolation_type == 2 || interpolation_type == 3 {
            let mut in_tan = Vec::with_capacity(element_size);
            let mut out_tan = Vec::with_capacity(element_size);
            for _ in 0..element_size {
                in_tan.push(file.read_f32::<LittleEndian>()?);
            }
            for _ in 0..element_size {
                out_tan.push(file.read_f32::<LittleEndian>()?);
            }
            (in_tan, out_tan)
        } else {
            (Vec::new(), Vec::new())
        };
        keyframes.push(Keyframe {
            frame,
            data,
            in_tan,
            out_tan,
        });
    }

    model.controllers.push(AnimationController {
        interpolation_type,
        global_seq_id,
        keyframes,
    });
    Ok(controller_idx)
}

pub fn read_first_controller(
    file: &mut File,
    model: &mut Model,
    tags: &[(u32, usize)],
) -> Result<i32, MdlError> {
    for &(tag, size) in tags {
        let idx = read_controller(file, model, tag, size)?;
        if idx >= 0 {
            return Ok(idx);
        }
    }
    Ok(-1)
}
