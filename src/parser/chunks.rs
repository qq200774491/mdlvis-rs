use crate::error::MdlError;
use crate::model::chunk::UnknownChunk;
use crate::model::ids::{
    Extent, GeosetIndex, GlobalSeqId, MaterialIndex, ObjectId, ParentId, TextureIndex, TrackId,
};
use crate::model::model::Model;
use crate::model::node::{NodeFlags, NodeRef, TYPE_LITE};
use crate::model::objects::{
    Attachment, Camera, CollisionShape, CollisionType, EventObject, GeosetAnim, GlobalSequence,
    Light, LightType, ParticleEmitter, ParticleEmitter2, ParticleEmitter2Flags, RibbonEmitter,
    TextureAnim,
};
use crate::parser::io::{
    TAG_KATV, TAG_KCRL, TAG_KCTR, TAG_KGAC, TAG_KGAO, TAG_KGRT, TAG_KGSC, TAG_KGTR, TAG_KLAC,
    TAG_KLAE, TAG_KLAI, TAG_KLAS, TAG_KLAV, TAG_KLBC, TAG_KLBI, TAG_KP2E, TAG_KP2G, TAG_KP2L,
    TAG_KP2N, TAG_KP2R, TAG_KP2S, TAG_KP2V, TAG_KP2W, TAG_KPEE, TAG_KPEG, TAG_KPEL, TAG_KPES,
    TAG_KPEV, TAG_KPLN, TAG_KPLT, TAG_KRAL, TAG_KRCO, TAG_KRHA, TAG_KRHB, TAG_KRVS, TAG_KTAR,
    TAG_KTAS, TAG_KTAT, TAG_KTTR, read_controller, read_controller_ex, read_cstring,
    read_first_controller, read_vec3, skip_to,
};
use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

fn track(idx: i32) -> TrackId {
    TrackId(idx)
}

fn read_node(file: &mut File, model: &mut Model) -> Result<NodeRef, MdlError> {
    let start = file.stream_position()?;
    let inclusive = file.read_u32::<LittleEndian>()? as u64;
    let node = read_node_body(file, model)?;
    skip_to(file, start + inclusive)?;
    Ok(node)
}

fn read_node_body(file: &mut File, model: &mut Model) -> Result<NodeRef, MdlError> {
    let name = read_cstring(file, 0x50)?;
    let object_id = file.read_u32::<LittleEndian>()?;
    let parent_id = file.read_i32::<LittleEndian>()?;
    let flags = file.read_u32::<LittleEndian>()?;
    let translation = read_controller(file, model, TAG_KGTR, 3)?;
    let rotation = read_controller(file, model, TAG_KGRT, 4)?;
    let scaling = read_controller(file, model, TAG_KGSC, 3)?;
    let visibility = read_first_controller(file, model, &[(TAG_KATV, 1), (TAG_KLAV, 1)])?;
    Ok(NodeRef {
        name,
        object_id: ObjectId(object_id),
        parent_id: ParentId(parent_id),
        flags: NodeFlags::from_bits(flags),
        translation: track(translation),
        rotation: track(rotation),
        scaling: track(scaling),
        visibility: track(visibility),
    })
}

pub fn read_modl(file: &mut File, model: &mut Model, size: u32) -> Result<(), MdlError> {
    let start = file.stream_position()?;
    model.name = read_cstring(file, 0x150)?;
    file.seek(SeekFrom::Current(4))?;
    let bounds_radius = file.read_f32::<LittleEndian>()?;
    let minimum = read_vec3(file)?;
    let maximum = read_vec3(file)?;
    model.extent = Extent {
        bounds_radius,
        minimum,
        maximum,
    };
    model.blend_time = file.read_u32::<LittleEndian>()?;
    skip_to(file, start + size as u64)?;
    println!("Model name: {}", model.name);
    Ok(())
}

pub fn read_global_sequences(
    file: &mut File,
    model: &mut Model,
    size: u32,
) -> Result<(), MdlError> {
    let count = size / 4;
    for _ in 0..count {
        model.global_sequences.push(GlobalSequence {
            duration: file.read_u32::<LittleEndian>()?,
        });
    }
    println!("Loaded {} global sequences", model.global_sequences.len());
    Ok(())
}

pub fn read_texture_anims(file: &mut File, model: &mut Model, size: u32) -> Result<(), MdlError> {
    let end = file.stream_position()? + size as u64;
    while file.stream_position()? < end {
        let start = file.stream_position()?;
        let inclusive = file.read_u32::<LittleEndian>()? as u64;
        let translation = read_controller(file, model, TAG_KTAT, 3)?;
        let rotation = read_controller(file, model, TAG_KTAR, 4)?;
        let scaling = read_controller(file, model, TAG_KTAS, 3)?;
        model.texture_anims.push(TextureAnim {
            translation: track(translation),
            rotation: track(rotation),
            scaling: track(scaling),
        });
        skip_to(file, start + inclusive)?;
    }
    println!("Loaded {} texture anims", model.texture_anims.len());
    Ok(())
}

pub fn read_geoset_anims(file: &mut File, model: &mut Model, size: u32) -> Result<(), MdlError> {
    let end = file.stream_position()? + size as u64;
    while file.stream_position()? < end {
        let start = file.stream_position()?;
        let inclusive = file.read_u32::<LittleEndian>()? as u64;
        let alpha = file.read_f32::<LittleEndian>()?;
        let flags = file.read_u32::<LittleEndian>()?;
        let color = read_vec3(file)?;
        let geoset_id = file.read_i32::<LittleEndian>()?;
        let alpha_track = read_controller(file, model, TAG_KGAO, 1)?;
        let color_track = read_controller(file, model, TAG_KGAC, 3)?;
        model.geoset_anims.push(GeosetAnim {
            geoset_id: if geoset_id >= 0 {
                Some(GeosetIndex(geoset_id as u32))
            } else {
                None
            },
            alpha,
            color,
            drop_shadow: flags & 1 != 0,
            alpha_track: track(alpha_track),
            color_track: track(color_track),
        });
        skip_to(file, start + inclusive)?;
    }
    println!("Loaded {} geoset anims", model.geoset_anims.len());
    Ok(())
}

pub fn read_lights(file: &mut File, model: &mut Model, size: u32) -> Result<(), MdlError> {
    let end = file.stream_position()? + size as u64;
    while file.stream_position()? < end {
        let start = file.stream_position()?;
        let inclusive = file.read_u32::<LittleEndian>()? as u64;
        let mut node = read_node(file, model)?;
        node.flags = NodeFlags::from_bits(node.flags.bits() | TYPE_LITE);
        let light_type = LightType::from_disk(file.read_u32::<LittleEndian>()?)
            .unwrap_or(LightType::Omnidirectional);
        let attenuation_start = file.read_f32::<LittleEndian>()?;
        let attenuation_end = file.read_f32::<LittleEndian>()?;
        let color = read_vec3(file)?;
        let intensity = file.read_f32::<LittleEndian>()?;
        let ambient_color = read_vec3(file)?;
        let ambient_intensity = file.read_f32::<LittleEndian>()?;
        let attenuation_start_track = read_controller(file, model, TAG_KLAS, 1)?;
        let attenuation_end_track = read_controller(file, model, TAG_KLAE, 1)?;
        let intensity_track = read_controller(file, model, TAG_KLAI, 1)?;
        let visibility = read_controller(file, model, TAG_KLAV, 1)?;
        if node.visibility.is_none() {
            node.visibility = track(visibility);
        }
        let color_track = read_controller(file, model, TAG_KLAC, 3)?;
        let ambient_color_track = read_controller(file, model, TAG_KLBC, 3)?;
        let ambient_intensity_track = read_controller(file, model, TAG_KLBI, 1)?;
        model.lights.push(Light {
            node,
            light_type,
            attenuation_start,
            attenuation_end,
            color,
            intensity,
            ambient_color,
            ambient_intensity,
            attenuation_start_track: track(attenuation_start_track),
            attenuation_end_track: track(attenuation_end_track),
            intensity_track: track(intensity_track),
            color_track: track(color_track),
            ambient_color_track: track(ambient_color_track),
            ambient_intensity_track: track(ambient_intensity_track),
        });
        skip_to(file, start + inclusive)?;
    }
    println!("Loaded {} lights", model.lights.len());
    Ok(())
}

pub fn read_attachments(file: &mut File, model: &mut Model, size: u32) -> Result<(), MdlError> {
    let end = file.stream_position()? + size as u64;
    while file.stream_position()? < end {
        let start = file.stream_position()?;
        let inclusive = file.read_u32::<LittleEndian>()? as u64;
        let mut node = read_node(file, model)?;
        let path = read_cstring(file, 0x100)?;
        file.seek(SeekFrom::Current(4))?;
        let attachment_id = file.read_i32::<LittleEndian>()?;
        let visibility = read_controller(file, model, TAG_KATV, 1)?;
        if node.visibility.is_none() {
            node.visibility = track(visibility);
        }
        model.attachments.push(Attachment {
            node,
            path,
            attachment_id,
        });
        skip_to(file, start + inclusive)?;
    }
    println!("Loaded {} attachments", model.attachments.len());
    Ok(())
}

pub fn read_particle_emitters(
    file: &mut File,
    model: &mut Model,
    size: u32,
) -> Result<(), MdlError> {
    let end = file.stream_position()? + size as u64;
    while file.stream_position()? < end {
        let start = file.stream_position()?;
        let inclusive = file.read_u32::<LittleEndian>()? as u64;
        let mut node = read_node(file, model)?;
        let emission_rate = file.read_f32::<LittleEndian>()?;
        let gravity = file.read_f32::<LittleEndian>()?;
        let longitude = file.read_f32::<LittleEndian>()?;
        let latitude = file.read_f32::<LittleEndian>()?;
        let path = read_cstring(file, 0x100)?;
        file.seek(SeekFrom::Current(4))?;
        let life_span = file.read_f32::<LittleEndian>()?;
        let init_velocity = file.read_f32::<LittleEndian>()?;
        let _ = read_controller(file, model, TAG_KPEE, 1)?;
        let _ = read_controller(file, model, TAG_KPEG, 1)?;
        let _ = read_controller(file, model, TAG_KPLN, 1)?;
        let _ = read_controller(file, model, TAG_KPLT, 1)?;
        let _ = read_controller(file, model, TAG_KPEL, 1)?;
        let _ = read_controller(file, model, TAG_KPES, 1)?;
        let visibility = read_controller(file, model, TAG_KPEV, 1)?;
        if node.visibility.is_none() {
            node.visibility = track(visibility);
        }
        model.particle_emitters.push(ParticleEmitter {
            node,
            uses_type: Default::default(),
            emission_rate,
            emission_rate_track: TrackId::NONE,
            gravity,
            gravity_track: TrackId::NONE,
            longitude,
            longitude_track: TrackId::NONE,
            latitude,
            latitude_track: TrackId::NONE,
            life_span,
            life_span_track: TrackId::NONE,
            init_velocity,
            init_velocity_track: TrackId::NONE,
            path,
        });
        skip_to(file, start + inclusive)?;
    }
    println!("Loaded {} particle emitters", model.particle_emitters.len());
    Ok(())
}

pub fn read_particle_emitters_2(
    file: &mut File,
    model: &mut Model,
    size: u32,
) -> Result<(), MdlError> {
    let end = file.stream_position()? + size as u64;
    while file.stream_position()? < end {
        let start = file.stream_position()?;
        let inclusive = file.read_u32::<LittleEndian>()? as u64;
        let mut node = read_node(file, model)?;
        let flags_bits = node.flags.bits();
        let flags = ParticleEmitter2Flags {
            unshaded: flags_bits & 0x8000 != 0,
            sort_primitives_far_z: flags_bits & 0x1_0000 != 0,
            line_emitter: flags_bits & 0x2_0000 != 0,
            unfogged: flags_bits & 0x4_0000 != 0,
            model_space: flags_bits & 0x8_0000 != 0,
            xy_quad: flags_bits & 0x10_0000 != 0,
        };
        let speed = file.read_f32::<LittleEndian>()?;
        let variation = file.read_f32::<LittleEndian>()?;
        let latitude = file.read_f32::<LittleEndian>()?;
        let gravity = file.read_f32::<LittleEndian>()?;
        let life_span = file.read_f32::<LittleEndian>()?;
        let emission_rate = file.read_f32::<LittleEndian>()?;
        let length = file.read_f32::<LittleEndian>()?;
        let width = file.read_f32::<LittleEndian>()?;
        let blend_mode = file.read_u32::<LittleEndian>()?;
        let rows = file.read_u32::<LittleEndian>()?;
        let columns = file.read_u32::<LittleEndian>()?;
        let particle_type = file.read_u32::<LittleEndian>()?;
        let tail_length = file.read_f32::<LittleEndian>()?;
        let time = file.read_f32::<LittleEndian>()?;
        let mut segment_color = [[0.0f32; 3]; 3];
        for color in &mut segment_color {
            *color = read_vec3(file)?;
        }
        let mut alpha = [0u8; 3];
        file.read_exact(&mut alpha)?;
        let particle_scaling = read_vec3(file)?;
        file.seek(SeekFrom::Current(4 * 3 * 4))?;
        let texture_id = file.read_u32::<LittleEndian>()?;
        let squirt = file.read_u32::<LittleEndian>()? != 0;
        let priority_plane = file.read_i32::<LittleEndian>()?;
        let replaceable_id = file.read_u32::<LittleEndian>()?;
        let mut speed_track = TrackId::NONE;
        let mut variation_track = TrackId::NONE;
        let mut latitude_track = TrackId::NONE;
        let mut gravity_track = TrackId::NONE;
        let mut emission_rate_track = TrackId::NONE;
        let mut length_track = TrackId::NONE;
        let mut width_track = TrackId::NONE;
        loop {
            let before = file.stream_position()?;
            let variation = read_controller(file, model, TAG_KP2R, 1)?;
            if variation >= 0 {
                variation_track = track(variation);
            }
            let latitude = read_controller(file, model, TAG_KP2L, 1)?;
            if latitude >= 0 {
                latitude_track = track(latitude);
            }
            let gravity = read_controller(file, model, TAG_KP2G, 1)?;
            if gravity >= 0 {
                gravity_track = track(gravity);
            }
            let visibility = read_controller(file, model, TAG_KP2V, 1)?;
            if visibility >= 0 && node.visibility.is_none() {
                node.visibility = track(visibility);
            }
            let emission_rate_controller = read_controller(file, model, TAG_KP2E, 1)?;
            if emission_rate_controller >= 0 {
                emission_rate_track = track(emission_rate_controller);
            }
            let speed_controller = read_controller(file, model, TAG_KP2S, 1)?;
            if speed_controller >= 0 {
                speed_track = track(speed_controller);
            }
            let length_controller = read_controller(file, model, TAG_KP2N, 1)?;
            if length_controller >= 0 {
                length_track = track(length_controller);
            }
            let width_controller = read_controller(file, model, TAG_KP2W, 1)?;
            if width_controller >= 0 {
                width_track = track(width_controller);
            }
            if file.stream_position()? == before {
                break;
            }
        }
        model.particle_emitters_2.push(ParticleEmitter2 {
            node,
            flags,
            speed,
            speed_track,
            variation,
            variation_track,
            latitude,
            latitude_track,
            gravity,
            gravity_track,
            life_span,
            emission_rate,
            emission_rate_track,
            width,
            width_track,
            length,
            length_track,
            squirt,
            blend_mode,
            rows,
            columns,
            particle_type,
            tail_length,
            time,
            segment_color,
            alpha,
            particle_scaling,
            texture_id: Some(TextureIndex(texture_id)),
            replaceable_id,
            priority_plane,
        });
        skip_to(file, start + inclusive)?;
    }
    println!(
        "Loaded {} particle emitters 2",
        model.particle_emitters_2.len()
    );
    Ok(())
}

pub fn read_ribbons(file: &mut File, model: &mut Model, size: u32) -> Result<(), MdlError> {
    let end = file.stream_position()? + size as u64;
    while file.stream_position()? < end {
        let start = file.stream_position()?;
        let inclusive = file.read_u32::<LittleEndian>()? as u64;
        let mut node = read_node(file, model)?;
        let height_above = file.read_f32::<LittleEndian>()?;
        let height_below = file.read_f32::<LittleEndian>()?;
        let alpha = file.read_f32::<LittleEndian>()?;
        let color = read_vec3(file)?;
        let life_span = file.read_f32::<LittleEndian>()?;
        let texture_slot = file.read_i32::<LittleEndian>()?;
        let emission_rate = file.read_u32::<LittleEndian>()?;
        let rows = file.read_u32::<LittleEndian>()?;
        let columns = file.read_u32::<LittleEndian>()?;
        let material_id = file.read_u32::<LittleEndian>()?;
        let gravity = file.read_f32::<LittleEndian>()?;
        let _ = read_controller(file, model, TAG_KRHA, 1)?;
        let _ = read_controller(file, model, TAG_KRHB, 1)?;
        let _ = read_controller(file, model, TAG_KRAL, 1)?;
        let _ = read_controller(file, model, TAG_KRCO, 3)?;
        let visibility = read_controller(file, model, TAG_KRVS, 1)?;
        if node.visibility.is_none() {
            node.visibility = track(visibility);
        }
        model.ribbons.push(RibbonEmitter {
            node,
            height_above,
            height_above_track: TrackId::NONE,
            height_below,
            height_below_track: TrackId::NONE,
            alpha,
            alpha_track: TrackId::NONE,
            color,
            color_track: TrackId::NONE,
            texture_slot,
            texture_slot_track: TrackId::NONE,
            emission_rate,
            life_span,
            gravity,
            rows,
            columns,
            material_id: Some(MaterialIndex(material_id)),
        });
        skip_to(file, start + inclusive)?;
    }
    println!("Loaded {} ribbons", model.ribbons.len());
    Ok(())
}

pub fn read_cameras(file: &mut File, model: &mut Model, size: u32) -> Result<(), MdlError> {
    let end = file.stream_position()? + size as u64;
    while file.stream_position()? < end {
        let start = file.stream_position()?;
        let inclusive = file.read_u32::<LittleEndian>()? as u64;
        let name = read_cstring(file, 0x50)?;
        let position = read_vec3(file)?;
        let field_of_view = file.read_f32::<LittleEndian>()?;
        let far_clip = file.read_f32::<LittleEndian>()?;
        let near_clip = file.read_f32::<LittleEndian>()?;
        let target_position = read_vec3(file)?;
        let target_translation = read_controller(file, model, TAG_KCTR, 3)?;
        let rotation = read_controller(file, model, TAG_KCRL, 1)?;
        let translation = read_controller(file, model, TAG_KTTR, 3)?;
        model.cameras.push(Camera {
            name,
            position,
            field_of_view,
            far_clip,
            near_clip,
            target_position,
            translation: track(translation),
            rotation: track(rotation),
            target_translation: track(target_translation),
        });
        skip_to(file, start + inclusive)?;
    }
    println!("Loaded {} cameras", model.cameras.len());
    Ok(())
}

pub fn read_events(file: &mut File, model: &mut Model, size: u32) -> Result<(), MdlError> {
    let end = file.stream_position()? + size as u64;
    while file.stream_position()? < end {
        let node = read_node(file, model)?;
        let mut kevt = [0u8; 4];
        file.read_exact(&mut kevt)?;
        let count = file.read_u32::<LittleEndian>()? as usize;
        let global_seq = file.read_i32::<LittleEndian>()?;
        let mut tracks = Vec::with_capacity(count);
        for _ in 0..count {
            tracks.push(file.read_i32::<LittleEndian>()?);
        }
        let _ = kevt;
        model.events.push(EventObject {
            node,
            global_seq_id: GlobalSeqId(global_seq),
            tracks,
        });
    }
    println!("Loaded {} events", model.events.len());
    Ok(())
}

pub fn read_collisions(file: &mut File, model: &mut Model, size: u32) -> Result<(), MdlError> {
    let end = file.stream_position()? + size as u64;
    while file.stream_position()? < end {
        let node = read_node(file, model)?;
        let kind_raw = file.read_u32::<LittleEndian>()?;
        let (kind, vertices, bounds_radius) = if kind_raw == 0 {
            let a = read_vec3(file)?;
            let b = read_vec3(file)?;
            (CollisionType::Box, vec![a, b], 0.0)
        } else {
            let center = read_vec3(file)?;
            let radius = file.read_f32::<LittleEndian>()?;
            (CollisionType::Sphere, vec![center], radius)
        };
        model.collisions.push(CollisionShape {
            node,
            kind,
            vertices,
            bounds_radius,
        });
    }
    println!("Loaded {} collisions", model.collisions.len());
    Ok(())
}

pub fn read_unknown_chunk(
    file: &mut File,
    model: &mut Model,
    fourcc: [u8; 4],
    size: u32,
) -> Result<(), MdlError> {
    let mut data = vec![0u8; size as usize];
    file.read_exact(&mut data)?;
    model.unknown_chunks.push(UnknownChunk::new(fourcc, data));
    Ok(())
}

pub fn read_layer_tracks(file: &mut File, model: &mut Model) -> Result<(i32, i32), MdlError> {
    let alpha = read_controller(file, model, crate::parser::io::TAG_KMTA, 1)?;
    let texture = read_controller_ex(file, model, crate::parser::io::TAG_KMTF, 1, true)?;
    Ok((alpha, texture))
}
