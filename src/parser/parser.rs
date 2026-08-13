use crate::error::MdlError;
use crate::material::{FilterMode, Layer, Material, ShadingFlags};
use crate::model::animation::Sequence;
use crate::model::ids::{Extent, TextureAnimIndex};
use crate::model::model::Model;
use crate::model::objects::{LayerRef, MaterialFlags, TextureFlags};
use crate::model::skeleton::{Bone, Helper};
use crate::model::texture::Texture;
use crate::parser::io::{
    read_controller, read_first_controller, read_vec3, TAG_KATV, TAG_KGRT, TAG_KGSC, TAG_KGTR,
    TAG_KLAV,
};
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
        let move_speed = file.read_f32::<LittleEndian>()?;
        let non_looping_flag = file.read_u32::<LittleEndian>()?;
        let rarity = file.read_f32::<LittleEndian>()?;
        file.seek(SeekFrom::Current(4))?;
        let bounds_radius = file.read_f32::<LittleEndian>()?;
        let minimum = read_vec3(file)?;
        let maximum = read_vec3(file)?;

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
            move_speed,
            extent: Extent {
                bounds_radius,
                minimum,
                maximum,
            },
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
        let flags = file.read_u32::<LittleEndian>()?;

        let tex_filename = filename.trim().to_string();
        println!(
            "  Texture: '{}', ReplaceableID: {}",
            tex_filename, replaceable_id
        );

        model.textures.push(Texture {
            filename: tex_filename,
            replaceable_id,
            flags: TextureFlags::from_bits(flags),
            image_data: None, // Will be loaded later if needed
            width: 0,
            height: 0,
        });
    }

    Ok(())
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
        let flags = file.read_u32::<LittleEndian>()?;

        // Read controllers (these are inside the Node structure)
        let translation_idx = read_controller(file, model, TAG_KGTR, 3)?;
        let rotation_idx = read_controller(file, model, TAG_KGRT, 4)?;
        let scaling_idx = read_controller(file, model, TAG_KGSC, 3)?;
        let visibility_idx = read_first_controller(file, model, &[(TAG_KATV, 1), (TAG_KLAV, 1)])?;

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
            flags,
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
        let flags = file.read_u32::<LittleEndian>()?;

        // Read controllers (these are inside the Node structure)
        let translation_idx = read_controller(file, model, TAG_KGTR, 3)?;
        let rotation_idx = read_controller(file, model, TAG_KGRT, 4)?;
        let scaling_idx = read_controller(file, model, TAG_KGSC, 3)?;
        let visibility_idx = read_first_controller(file, model, &[(TAG_KATV, 1), (TAG_KLAV, 1)])?;

        // Seek to end of Node structure
        file.seek(SeekFrom::Start(node_start + inclusive_size as u64))?;

        // Helper has no additional fields after Node, unlike Bone
        model.helpers.push(Helper {
            name: name.trim().to_string(),
            object_id,
            parent_id,
            pivot_point: [0.0, 0.0, 0.0], // Will be set from PIVT chunk
            flags,
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
        let priority_plane = file.read_i32::<LittleEndian>()?;
        let material_flags = file.read_u32::<LittleEndian>()?;

        // Read LAYS tag
        let mut tag = [0u8; 4];
        file.read_exact(&mut tag)?;

        if &tag != b"LAYS" {
            // Not a valid material, skip to end
            file.seek(SeekFrom::Start(material_end))?;
            continue;
        }

        let layers_count = file.read_u32::<LittleEndian>()?;
        let mut material = Material {
            layers: Vec::new(),
            priority_plane,
            flags: MaterialFlags {
                constant_color: material_flags & 1 != 0,
                sort_primitives_far_z: material_flags & 16 != 0,
                full_resolution: material_flags & 32 != 0,
            },
        };

        // Read each layer
        for _ in 0..layers_count {
            let layer_size = file.read_u32::<LittleEndian>()?;
            let layer_start = file.seek(SeekFrom::Current(0))?;
            let layer_end = layer_start + (layer_size as u64) - 4;

            // Read layer data
            let filter_mode_val = file.read_u32::<LittleEndian>()?;
            let shading_flags_bits = file.read_u32::<LittleEndian>()?;
            let texture_id = file.read_u32::<LittleEndian>()?;
            let texture_animation_id = file.read_i32::<LittleEndian>()?;
            let coord_id = file.read_u32::<LittleEndian>()?;
            let alpha = file.read_f32::<LittleEndian>()?;
            let (alpha_track, texture_id_track) =
                crate::parser::chunks::read_layer_tracks(file, model)?;

            // Parse filter mode using FilterMode::from_u32
            let filter_mode = FilterMode::from_u32(filter_mode_val);

            // Parse shading flags once during loading
            let shading_flags = ShadingFlags::from_bits(shading_flags_bits);

            let layer = Layer {
                texture_id: Some(texture_id as usize),
                filter_mode,
                shading_flags,
                alpha,
                extra: LayerRef {
                    texture_anim_id: if texture_animation_id >= 0 {
                        Some(TextureAnimIndex(texture_animation_id as u32))
                    } else {
                        None
                    },
                    coord_id,
                },
                alpha_track,
                texture_id_track,
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
        model.pivot_points.push([x, y, z]);

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
