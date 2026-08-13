use crate::error::MdlError;
use crate::material::{FilterMode, Layer, Material, ShadingFlags};
use crate::model::animation::Sequence;
use crate::model::model::Model;
use crate::model::skeleton::{AnimationController, Bone, Helper, Keyframe};
use crate::model::texture::Texture;
use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub(crate) fn read_sequences(
    file: &mut File,
    model: &mut Model,
    size: u32,
) -> Result<(), MdlError> {
    // From Delphi: SizeOfSeq = $50 + 13*4 = 80 + 52 = 132 bytes per sequence
    const SEQUENCE_SIZE: u32 = 0x50 + 13 * 4; // 132 bytes

    let count = size / SEQUENCE_SIZE;
    println!("Reading {} sequences from SEQS chunk", count);

    for _ in 0..count {
        let mut name_bytes = [0u8; 0x50]; // 80 bytes for name
        file.read_exact(&mut name_bytes)?;
        let name = String::from_utf8(
            name_bytes
                .iter()
                .take_while(|&&b| b != 0)
                .copied()
                .collect(),
        )
        .unwrap_or_else(|_| "Unknown".to_string());

        let start_frame = file.read_u32::<LittleEndian>()?;
        let end_frame = file.read_u32::<LittleEndian>()?;
        let _move_speed = file.read_f32::<LittleEndian>()?;
        let non_looping_flag = file.read_u32::<LittleEndian>()?;
        let rarity = file.read_f32::<LittleEndian>()?;

        // Skip remaining data (padding + bounds: 4 + 4 + 4*3 + 4*3 = 32 bytes)
        file.seek(SeekFrom::Current(32))?;

        let seq_name = name.trim().to_string();
        println!(
            "  Sequence: '{}' frames {}-{}",
            seq_name, start_frame, end_frame
        );

        model.sequences.push(Sequence {
            name: seq_name,
            start_frame,
            end_frame,
            rarity: Some(rarity as u32),
            non_looping: non_looping_flag != 0,
        });
    }

    Ok(())
}

pub(crate) fn read_textures(file: &mut File, model: &mut Model, size: u32) -> Result<(), MdlError> {
    // From Delphi: TEXSize = $100 + 3*4 = 256 + 12 = 268 bytes per texture
    const TEXTURE_SIZE: u32 = 0x100 + 3 * 4; // 268 bytes

    let count = size / TEXTURE_SIZE;
    println!("Reading {} textures from TEXS chunk", count);

    for _ in 0..count {
        let replaceable_id = file.read_u32::<LittleEndian>()?;

        let mut filename_bytes = [0u8; 0x100]; // 256 bytes for filename
        file.read_exact(&mut filename_bytes)?;
        let filename = String::from_utf8(
            filename_bytes
                .iter()
                .take_while(|&&b| b != 0)
                .copied()
                .collect(),
        )
        .unwrap_or_else(|_| "Unknown".to_string());

        // Skip padding (4 bytes)
        file.seek(SeekFrom::Current(4))?;

        // Read flags
        let _flags = file.read_u32::<LittleEndian>()?;

        let tex_filename = filename.trim().to_string();
        println!(
            "  Texture: '{}', ReplaceableID: {}",
            tex_filename, replaceable_id
        );

        model.textures.push(Texture {
            filename: tex_filename,
            replaceable_id,
            image_data: None, // Will be loaded later if needed
            width: 0,
            height: 0,
        });
    }

    Ok(())
}

// Tag constants for controllers
const TAG_KGTR: u32 = 0x5254474B; // Translation (3 floats)
const TAG_KGRT: u32 = 0x5452474B; // Rotation (4 floats - quaternion)
const TAG_KGSC: u32 = 0x4353474B; // Scaling (3 floats)
const TAG_KLAV: u32 = 0x56414C4B; // Visibility (1 float)

// Reads a controller chunk if present, returns controller index or -1 if not found
fn read_controller(
    file: &mut File,
    model: &mut Model,
    expected_tag: u32,
    element_size: usize,
) -> Result<i32, MdlError> {
    let pos_before = file.stream_position()?;

    // Try to read tag
    let tag = match file.read_u32::<LittleEndian>() {
        Ok(t) => t,
        Err(_) => {
            file.seek(SeekFrom::Start(pos_before))?;
            return Ok(-1);
        }
    };

    // If tag doesn't match, rewind and return -1 (static)
    if tag != expected_tag {
        file.seek(SeekFrom::Start(pos_before))?;
        return Ok(-1);
    }

    // Tag matches, read controller data
    let keyframe_count = file.read_u32::<LittleEndian>()? as usize;
    let interpolation_type = file.read_u32::<LittleEndian>()?;
    let global_seq_id = file.read_i32::<LittleEndian>()?;

    // Create AnimationController
    let controller_idx = model.controllers.len() as i32;
    let mut keyframes = Vec::with_capacity(keyframe_count);

    // Read keyframes
    for _ in 0..keyframe_count {
        let frame = file.read_i32::<LittleEndian>()?;

        // Read data values
        let mut data = Vec::with_capacity(element_size);
        for _ in 0..element_size {
            data.push(file.read_f32::<LittleEndian>()?);
        }

        // Read tangents if Hermite (2) or Bezier (3)
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

    // Debug: print first controller info
    if model.controllers.len() == 1 {
        println!(
            "  First controller: {} keyframes, interp_type={}, global_seq={}",
            keyframe_count, interpolation_type, global_seq_id
        );
        if !model.controllers[0].keyframes.is_empty() {
            let kf = &model.controllers[0].keyframes[0];
            println!("    First keyframe: frame={}, data={:?}", kf.frame, kf.data);
        }
    }

    Ok(controller_idx)
}

pub(crate) fn read_bones(file: &mut File, model: &mut Model, size: u32) -> Result<(), MdlError> {
    let start_pos = file.stream_position()?;
    let end_pos = start_pos + size as u64;

    while file.stream_position()? < end_pos {
        let node_start = file.stream_position()?;

        if node_start >= end_pos {
            break;
        }

        // Read Node.inclusiveSize - this is the size of the Node structure INCLUDING this u32
        let inclusive_size = file.read_u32::<LittleEndian>()?;

        // Read Node fields
        let mut name_bytes = [0u8; 0x50]; // 80 bytes for name
        file.read_exact(&mut name_bytes)?;
        let name = String::from_utf8(
            name_bytes
                .iter()
                .take_while(|&&b| b != 0)
                .copied()
                .collect(),
        )
        .unwrap_or_else(|_| "Unknown".to_string());

        let object_id = file.read_u32::<LittleEndian>()?;
        let parent_id = file.read_i32::<LittleEndian>()?;
        let _flags = file.read_u32::<LittleEndian>()?;

        // Read controllers (these are inside the Node structure)
        let translation_idx = read_controller(file, model, TAG_KGTR, 3)?;
        let rotation_idx = read_controller(file, model, TAG_KGRT, 4)?;
        let scaling_idx = read_controller(file, model, TAG_KGSC, 3)?;
        let visibility_idx = read_controller(file, model, TAG_KLAV, 1)?;

        // Seek to end of Node structure
        file.seek(SeekFrom::Start(node_start + inclusive_size as u64))?;

        // Now read Bone-specific fields (AFTER Node structure)
        let geoset_id = file.read_i32::<LittleEndian>()?;
        let geoset_anim_id = file.read_i32::<LittleEndian>()?;

        model.bones.push(Bone {
            name: name.trim().to_string(),
            object_id,
            parent_id,
            pivot_point: [0.0, 0.0, 0.0], // Will be set from PIVT chunk
            geoset_id: if geoset_id >= 0 {
                Some(geoset_id as u32)
            } else {
                None
            },
            geoset_anim_id: if geoset_anim_id >= 0 {
                Some(geoset_anim_id as u32)
            } else {
                None
            },
            translation_idx,
            rotation_idx,
            scaling_idx,
            visibility_idx,
        });
    }

    println!(
        "Loaded {} bones, {} controllers",
        model.bones.len(),
        model.controllers.len()
    );
    Ok(())
}

pub(crate) fn read_helpers(file: &mut File, model: &mut Model, size: u32) -> Result<(), MdlError> {
    let start_pos = file.stream_position()?;
    let end_pos = start_pos + size as u64;

    while file.stream_position()? < end_pos {
        let node_start = file.stream_position()?;

        if node_start >= end_pos {
            break;
        }

        // Read Node.inclusiveSize
        let inclusive_size = file.read_u32::<LittleEndian>()?;

        // Read Node fields
        let mut name_bytes = [0u8; 0x50]; // 80 bytes for name
        file.read_exact(&mut name_bytes)?;
        let name = String::from_utf8(
            name_bytes
                .iter()
                .take_while(|&&b| b != 0)
                .copied()
                .collect(),
        )
        .unwrap_or_else(|_| "Unknown".to_string());

        let object_id = file.read_u32::<LittleEndian>()?;
        let parent_id = file.read_i32::<LittleEndian>()?;
        let _flags = file.read_u32::<LittleEndian>()?;

        // Read controllers (these are inside the Node structure)
        let translation_idx = read_controller(file, model, TAG_KGTR, 3)?;
        let rotation_idx = read_controller(file, model, TAG_KGRT, 4)?;
        let scaling_idx = read_controller(file, model, TAG_KGSC, 3)?;
        let visibility_idx = read_controller(file, model, TAG_KLAV, 1)?;

        // Seek to end of Node structure
        file.seek(SeekFrom::Start(node_start + inclusive_size as u64))?;

        // Helper has no additional fields after Node, unlike Bone
        model.helpers.push(Helper {
            name: name.trim().to_string(),
            object_id,
            parent_id,
            pivot_point: [0.0, 0.0, 0.0], // Will be set from PIVT chunk
            translation_idx,
            rotation_idx,
            scaling_idx,
            visibility_idx,
        });
    }

    println!("Loaded {} helpers", model.helpers.len());
    Ok(())
}

pub(crate) fn read_materials(
    file: &mut File,
    model: &mut Model,
    size: u32,
) -> Result<(), MdlError> {
    let start_pos = file.seek(SeekFrom::Current(0))?;
    let end_pos = start_pos + size as u64;

    // Each material has inclusiveSize at the start
    while file.seek(SeekFrom::Current(0))? < end_pos {
        let material_size = file.read_u32::<LittleEndian>()?;
        let material_start = file.seek(SeekFrom::Current(0))?;
        let material_end = material_start + (material_size as u64) - 4; // -4 because we already read size

        // Skip priority plane and flags
        file.seek(SeekFrom::Current(8))?;

        // Read LAYS tag
        let mut tag = [0u8; 4];
        file.read_exact(&mut tag)?;

        if &tag != b"LAYS" {
            // Not a valid material, skip to end
            file.seek(SeekFrom::Start(material_end))?;
            continue;
        }

        let layers_count = file.read_u32::<LittleEndian>()?;
        let mut material = Material::default();

        // Read each layer
        for _ in 0..layers_count {
            let layer_size = file.read_u32::<LittleEndian>()?;
            let layer_start = file.seek(SeekFrom::Current(0))?;
            let layer_end = layer_start + (layer_size as u64) - 4;

            // Read layer data
            let filter_mode_val = file.read_u32::<LittleEndian>()?;
            let shading_flags_bits = file.read_u32::<LittleEndian>()?;
            let texture_id = file.read_u32::<LittleEndian>()?;
            let _texture_animation_id = file.read_u32::<LittleEndian>()?;
            let _coord_id = file.read_u32::<LittleEndian>()?;
            let alpha = file.read_f32::<LittleEndian>()?;

            // Parse filter mode using FilterMode::from_u32
            let filter_mode = FilterMode::from_u32(filter_mode_val);

            // Parse shading flags once during loading
            let shading_flags = ShadingFlags::from_bits(shading_flags_bits);

            let layer = Layer {
                texture_id: Some(texture_id as usize),
                filter_mode,
                shading_flags,
                alpha,
                // Initialize runtime fields
                enabled: true,
                alpha_override: None,
                filter_mode_override: None,
                shading_flags_override: None,
            };
            material.layers.push(layer);

            // Skip to end of layer (may contain optional track chunks KMTF, KMTA, etc.)
            file.seek(SeekFrom::Start(layer_end))?;
        }

        if let Some(layer) = material.layers.first() {
            if let Some(tex_id) = layer.texture_id {
                println!(
                    "  Material {}: texture_id = {}, filter_mode = {:?}, alpha = {}",
                    model.materials.len(),
                    tex_id,
                    layer.filter_mode,
                    layer.alpha
                );
            }
        }

        model.materials.push(material);

        // Seek to end of material
        file.seek(SeekFrom::Start(material_end))?;
    }

    println!("Loaded {} materials", model.materials.len());

    Ok(())
}

pub(crate) fn read_pivots(file: &mut File, model: &mut Model, size: u32) -> Result<(), MdlError> {
    let count = size / (4 * 3); // Each pivot point is 3 floats

    for i in 0..count as usize {
        let x = file.read_f32::<LittleEndian>()?;
        let y = file.read_f32::<LittleEndian>()?;
        let z = file.read_f32::<LittleEndian>()?;

        // Assign to bones first, then helpers
        if i < model.bones.len() {
            model.bones[i].pivot_point = [x, y, z];
        } else {
            let helper_idx = i - model.bones.len();
            if helper_idx < model.helpers.len() {
                model.helpers[helper_idx].pivot_point = [x, y, z];
            }
        }
    }

    println!(
        "Loaded {} pivot points ({} bones + {} helpers)",
        count,
        model.bones.len(),
        model.helpers.len()
    );
    Ok(())
}
