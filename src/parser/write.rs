#![cfg_attr(not(test), allow(dead_code))]

use crate::error::MdlError;
use crate::material::{FilterMode, ShadingFlags};
use crate::model::model::Model;
use crate::model::node::{
    NodeRef, TYPE_ATCH, TYPE_BONE, TYPE_CLID, TYPE_EVTS, TYPE_HELP, TYPE_LITE,
};
use crate::model::objects::{
    Attachment, Camera, CollisionShape, CollisionType, EventObject, GeosetAnim, Light,
    ParticleEmitter, ParticleEmitter2, ParticleEmitterUses, RibbonEmitter, TextureAnim,
};
use crate::model::skeleton::{AnimationController, Bone, Helper};
use crate::parser::io::{
    TAG_KATV, TAG_KCRL, TAG_KCTR, TAG_KGAC, TAG_KGAO, TAG_KGRT, TAG_KGSC, TAG_KGTR, TAG_KLAC,
    TAG_KLAE, TAG_KLAI, TAG_KLAS, TAG_KLAV, TAG_KLBC, TAG_KLBI, TAG_KMTA, TAG_KMTF, TAG_KPEE,
    TAG_KPEG, TAG_KPEL, TAG_KPES, TAG_KPEV, TAG_KPLN, TAG_KPLT, TAG_KRAL, TAG_KRCO, TAG_KRHA,
    TAG_KRHB, TAG_KRVS, TAG_KTAR, TAG_KTAS, TAG_KTAT, TAG_KTTR,
};
use byteorder::{LittleEndian, WriteBytesExt};
use std::fs::File;
use std::io::{Cursor, Seek, SeekFrom, Write};
use std::path::Path;

const SUPPORTED_VERSION: u32 = 800;

trait WriteSeek: Write + Seek {}

impl<T: Write + Seek + ?Sized> WriteSeek for T {}

pub fn save_path(path: impl AsRef<Path>, model: &Model) -> Result<(), MdlError> {
    let bytes = serialize(model)?;
    let mut file = File::create(path)?;
    file.write_all(&bytes)?;
    Ok(())
}

#[allow(dead_code)]
pub fn save(file: &mut File, model: &Model) -> Result<(), MdlError> {
    validate_model(model)?;
    write_model(file, model)
}

fn serialize(model: &Model) -> Result<Vec<u8>, MdlError> {
    validate_model(model)?;
    let mut cursor = Cursor::new(Vec::new());
    write_model(&mut cursor, model)?;
    Ok(cursor.into_inner())
}

fn write_model(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    file.write_all(b"MDLX")?;
    write_chunk(file, b"VERS", |out| {
        out.write_u32::<LittleEndian>(SUPPORTED_VERSION)?;
        Ok(())
    })?;
    write_modl(file, model)?;
    write_sequences(file, model)?;
    write_global_sequences(file, model)?;
    write_materials(file, model)?;
    write_textures(file, model)?;
    write_texture_anims(file, model)?;
    write_geosets(file, model)?;
    write_geoset_anims(file, model)?;
    write_bones(file, model)?;
    write_lights(file, model)?;
    write_helpers(file, model)?;
    write_attachments(file, model)?;
    write_pivots(file, model)?;
    write_particle_emitters(file, model)?;
    write_particle_emitters_2(file, model)?;
    write_ribbons(file, model)?;
    write_cameras(file, model)?;
    write_events(file, model)?;
    write_collisions(file, model)?;
    if let Some(data) = &model.mdlvis_data {
        write_chunk(file, b"MDVI", |out| {
            out.write_all(data)?;
            Ok(())
        })?;
    }
    for chunk in &model.unknown_chunks {
        file.write_all(&chunk.fourcc)?;
        file.write_u32::<LittleEndian>(u32::try_from(chunk.data.len()).map_err(|_| {
            MdlError::new("mdx-chunk-too-large")
                .with_arg("fourcc", chunk.fourcc_str())
                .with_arg("size", chunk.data.len())
        })?)?;
        file.write_all(&chunk.data)?;
    }
    Ok(())
}

fn write_chunk<F>(file: &mut dyn WriteSeek, fourcc: &[u8; 4], body: F) -> Result<(), MdlError>
where
    F: FnOnce(&mut dyn WriteSeek) -> Result<(), MdlError>,
{
    file.write_all(fourcc)?;
    let size_pos = file.stream_position()?;
    file.write_u32::<LittleEndian>(0)?;
    let start = file.stream_position()?;
    body(file)?;
    patch_size(file, size_pos, start)
}

fn write_inclusive<F>(file: &mut dyn WriteSeek, body: F) -> Result<(), MdlError>
where
    F: FnOnce(&mut dyn WriteSeek) -> Result<(), MdlError>,
{
    let size_pos = file.stream_position()?;
    file.write_u32::<LittleEndian>(0)?;
    let start = size_pos;
    body(file)?;
    patch_size(file, size_pos, start)
}

fn patch_size(file: &mut dyn WriteSeek, size_pos: u64, start: u64) -> Result<(), MdlError> {
    let end = file.stream_position()?;
    let size = u32::try_from(end - start)
        .map_err(|_| MdlError::new("mdx-chunk-too-large").with_arg("size", end - start))?;
    file.seek(SeekFrom::Start(size_pos))?;
    file.write_u32::<LittleEndian>(size)?;
    file.seek(SeekFrom::Start(end))?;
    Ok(())
}

fn write_padded(file: &mut dyn WriteSeek, text: &str, len: usize) -> Result<(), MdlError> {
    let mut buf = vec![0u8; len];
    let bytes = text.as_bytes();
    let n = bytes.len().min(len.saturating_sub(1));
    buf[..n].copy_from_slice(&bytes[..n]);
    file.write_all(&buf)?;
    Ok(())
}

fn write_vec3(file: &mut dyn WriteSeek, v: [f32; 3]) -> Result<(), MdlError> {
    file.write_f32::<LittleEndian>(v[0])?;
    file.write_f32::<LittleEndian>(v[1])?;
    file.write_f32::<LittleEndian>(v[2])?;
    Ok(())
}

fn write_extent(
    file: &mut dyn WriteSeek,
    radius: f32,
    min: [f32; 3],
    max: [f32; 3],
) -> Result<(), MdlError> {
    file.write_f32::<LittleEndian>(radius)?;
    write_vec3(file, min)?;
    write_vec3(file, max)?;
    Ok(())
}

fn write_controller(
    file: &mut dyn WriteSeek,
    model: &Model,
    tag: u32,
    idx: i32,
    as_int: bool,
) -> Result<(), MdlError> {
    if idx < 0 {
        return Ok(());
    }
    let Some(controller) = model.controllers.get(idx as usize) else {
        return Err(MdlError::new("mdx-invalid-controller-index")
            .with_arg("index", idx)
            .with_arg("count", model.controllers.len()));
    };
    write_controller_data(file, tag, controller, as_int)
}

fn write_controller_data(
    file: &mut dyn WriteSeek,
    tag: u32,
    controller: &AnimationController,
    as_int: bool,
) -> Result<(), MdlError> {
    file.write_u32::<LittleEndian>(tag)?;
    file.write_u32::<LittleEndian>(controller.keyframes.len() as u32)?;
    file.write_u32::<LittleEndian>(controller.interpolation_type)?;
    file.write_i32::<LittleEndian>(controller.global_seq_id)?;
    let with_tans = controller.interpolation_type == 2 || controller.interpolation_type == 3;
    for key in &controller.keyframes {
        file.write_i32::<LittleEndian>(key.frame)?;
        if as_int {
            let value = key.data[0] as i32;
            file.write_i32::<LittleEndian>(value)?;
        } else {
            for value in &key.data {
                file.write_f32::<LittleEndian>(*value)?;
            }
        }
        if with_tans {
            for value in &key.in_tan {
                file.write_f32::<LittleEndian>(*value)?;
            }
            for value in &key.out_tan {
                file.write_f32::<LittleEndian>(*value)?;
            }
        }
    }
    Ok(())
}

fn write_node(file: &mut dyn WriteSeek, model: &Model, node: &NodeRef) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        write_padded(out, &node.name, 0x50)?;
        out.write_u32::<LittleEndian>(node.object_id.0)?;
        out.write_i32::<LittleEndian>(node.parent_id.0)?;
        out.write_u32::<LittleEndian>(node.flags.bits())?;
        write_controller(out, model, TAG_KGTR, node.translation.0, false)?;
        write_controller(out, model, TAG_KGRT, node.rotation.0, false)?;
        write_controller(out, model, TAG_KGSC, node.scaling.0, false)?;
        write_controller(out, model, TAG_KATV, node.visibility.0, false)?;
        Ok(())
    })
}

fn bone_as_node(bone: &Bone) -> NodeRef {
    NodeRef {
        name: bone.name.clone(),
        object_id: crate::model::ids::ObjectId(bone.object_id),
        parent_id: crate::model::ids::ParentId(bone.parent_id),
        flags: crate::model::node::NodeFlags::from_bits(bone.flags | TYPE_BONE),
        translation: crate::model::ids::TrackId(bone.translation_idx),
        rotation: crate::model::ids::TrackId(bone.rotation_idx),
        scaling: crate::model::ids::TrackId(bone.scaling_idx),
        visibility: crate::model::ids::TrackId(bone.visibility_idx),
    }
}

fn helper_as_node(helper: &Helper) -> NodeRef {
    NodeRef {
        name: helper.name.clone(),
        object_id: crate::model::ids::ObjectId(helper.object_id),
        parent_id: crate::model::ids::ParentId(helper.parent_id),
        flags: crate::model::node::NodeFlags::from_bits(helper.flags | TYPE_HELP),
        translation: crate::model::ids::TrackId(helper.translation_idx),
        rotation: crate::model::ids::TrackId(helper.rotation_idx),
        scaling: crate::model::ids::TrackId(helper.scaling_idx),
        visibility: crate::model::ids::TrackId(helper.visibility_idx),
    }
}

fn validate_model(model: &Model) -> Result<(), MdlError> {
    for (geoset_index, geoset) in model.geosets.iter().enumerate() {
        for (set_index, tex_coords) in geoset.tex_coord_sets.iter().enumerate() {
            if tex_coords.len() != geoset.vertices.len() {
                return Err(MdlError::new("mdx-invalid-uv-set-size")
                    .with_arg("geoset", geoset_index)
                    .with_arg("set", set_index)
                    .with_arg("expected", geoset.vertices.len())
                    .with_arg("actual", tex_coords.len()));
            }
        }
    }
    for bone in &model.bones {
        validate_node(model, &bone_as_node(bone))?;
    }
    for helper in &model.helpers {
        validate_node(model, &helper_as_node(helper))?;
    }
    for material in &model.materials {
        for layer in &material.layers {
            validate_controller_ref(model, "Layer.Alpha", layer.alpha_track, 1)?;
            validate_controller_ref(model, "Layer.TextureID", layer.texture_id_track, 1)?;
        }
    }
    for anim in &model.texture_anims {
        validate_controller_ref(model, "TextureAnim.Translation", anim.translation.0, 3)?;
        validate_controller_ref(model, "TextureAnim.Rotation", anim.rotation.0, 4)?;
        validate_controller_ref(model, "TextureAnim.Scaling", anim.scaling.0, 3)?;
    }
    for anim in &model.geoset_anims {
        validate_controller_ref(model, "GeosetAnim.Alpha", anim.alpha_track.0, 1)?;
        validate_controller_ref(model, "GeosetAnim.Color", anim.color_track.0, 3)?;
    }
    for light in &model.lights {
        validate_node(model, &light.node)?;
        validate_controller_ref(
            model,
            "Light.AttenuationStart",
            light.attenuation_start_track.0,
            1,
        )?;
        validate_controller_ref(
            model,
            "Light.AttenuationEnd",
            light.attenuation_end_track.0,
            1,
        )?;
        validate_controller_ref(model, "Light.Intensity", light.intensity_track.0, 1)?;
        validate_controller_ref(model, "Light.Color", light.color_track.0, 3)?;
        validate_controller_ref(model, "Light.AmbColor", light.ambient_color_track.0, 3)?;
        validate_controller_ref(
            model,
            "Light.AmbIntensity",
            light.ambient_intensity_track.0,
            1,
        )?;
    }
    for attachment in &model.attachments {
        validate_node(model, &attachment.node)?;
    }
    for emitter in &model.particle_emitters {
        validate_node(model, &emitter.node)?;
        validate_controller_ref(
            model,
            "ParticleEmitter.EmissionRate",
            emitter.emission_rate_track.0,
            1,
        )?;
        validate_controller_ref(model, "ParticleEmitter.Gravity", emitter.gravity_track.0, 1)?;
        validate_controller_ref(
            model,
            "ParticleEmitter.Longitude",
            emitter.longitude_track.0,
            1,
        )?;
        validate_controller_ref(
            model,
            "ParticleEmitter.Latitude",
            emitter.latitude_track.0,
            1,
        )?;
        validate_controller_ref(
            model,
            "ParticleEmitter.LifeSpan",
            emitter.life_span_track.0,
            1,
        )?;
        validate_controller_ref(
            model,
            "ParticleEmitter.InitVelocity",
            emitter.init_velocity_track.0,
            1,
        )?;
    }
    for emitter in &model.particle_emitters_2 {
        validate_node(model, &emitter.node)?;
        validate_controller_ref(model, "ParticleEmitter2.Speed", emitter.speed_track.0, 1)?;
        validate_controller_ref(
            model,
            "ParticleEmitter2.Variation",
            emitter.variation_track.0,
            1,
        )?;
        validate_controller_ref(
            model,
            "ParticleEmitter2.Latitude",
            emitter.latitude_track.0,
            1,
        )?;
        validate_controller_ref(
            model,
            "ParticleEmitter2.Gravity",
            emitter.gravity_track.0,
            1,
        )?;
        validate_controller_ref(
            model,
            "ParticleEmitter2.EmissionRate",
            emitter.emission_rate_track.0,
            1,
        )?;
        validate_controller_ref(model, "ParticleEmitter2.Width", emitter.width_track.0, 1)?;
        validate_controller_ref(model, "ParticleEmitter2.Length", emitter.length_track.0, 1)?;
    }
    for ribbon in &model.ribbons {
        validate_node(model, &ribbon.node)?;
        validate_controller_ref(
            model,
            "RibbonEmitter.HeightAbove",
            ribbon.height_above_track.0,
            1,
        )?;
        validate_controller_ref(
            model,
            "RibbonEmitter.HeightBelow",
            ribbon.height_below_track.0,
            1,
        )?;
        validate_controller_ref(model, "RibbonEmitter.Alpha", ribbon.alpha_track.0, 1)?;
        validate_controller_ref(model, "RibbonEmitter.Color", ribbon.color_track.0, 3)?;
        if !ribbon.texture_slot_track.is_none() {
            return Err(
                MdlError::new("mdx-ribbon-texture-slot-track-not-representable")
                    .with_arg("index", ribbon.texture_slot_track.0),
            );
        }
    }
    for camera in &model.cameras {
        validate_controller_ref(
            model,
            "Camera.TargetTranslation",
            camera.target_translation.0,
            3,
        )?;
        validate_controller_ref(model, "Camera.Rotation", camera.rotation.0, 1)?;
        validate_controller_ref(model, "Camera.Translation", camera.translation.0, 3)?;
    }
    for event in &model.events {
        validate_node(model, &event.node)?;
    }
    for collision in &model.collisions {
        validate_node(model, &collision.node)?;
    }
    for chunk in &model.unknown_chunks {
        if chunk.fourcc == *b"MDVI" {
            return Err(MdlError::new("mdx-mdvi-in-unknown-chunks"));
        }
        u32::try_from(chunk.data.len()).map_err(|_| {
            MdlError::new("mdx-chunk-too-large")
                .with_arg("fourcc", chunk.fourcc_str())
                .with_arg("size", chunk.data.len())
        })?;
    }
    if let Some(data) = &model.mdlvis_data {
        u32::try_from(data.len()).map_err(|_| {
            MdlError::new("mdx-chunk-too-large")
                .with_arg("fourcc", "MDVI")
                .with_arg("size", data.len())
        })?;
    }
    Ok(())
}

fn validate_node(model: &Model, node: &NodeRef) -> Result<(), MdlError> {
    validate_controller_ref(model, "Node.Translation", node.translation.0, 3)?;
    validate_controller_ref(model, "Node.Rotation", node.rotation.0, 4)?;
    validate_controller_ref(model, "Node.Scaling", node.scaling.0, 3)?;
    validate_controller_ref(model, "Node.Visibility", node.visibility.0, 1)
}

fn validate_controller_ref(
    model: &Model,
    track: &'static str,
    idx: i32,
    elements: usize,
) -> Result<(), MdlError> {
    if idx < 0 {
        return Ok(());
    }
    let controller = model.controllers.get(idx as usize).ok_or_else(|| {
        MdlError::new("mdx-invalid-controller-index")
            .with_arg("track", track)
            .with_arg("index", idx)
            .with_arg("count", model.controllers.len())
    })?;
    if controller.interpolation_type > 3 {
        return Err(MdlError::new("mdx-invalid-interpolation-type")
            .with_arg("track", track)
            .with_arg("value", controller.interpolation_type));
    }
    if controller.global_seq_id >= 0
        && controller.global_seq_id as usize >= model.global_sequences.len()
    {
        return Err(MdlError::new("mdx-invalid-global-sequence-index")
            .with_arg("track", track)
            .with_arg("index", controller.global_seq_id)
            .with_arg("count", model.global_sequences.len()));
    }
    let with_tangents = controller.interpolation_type == 2 || controller.interpolation_type == 3;
    for keyframe in &controller.keyframes {
        if keyframe.data.len() != elements {
            return Err(MdlError::new("mdx-invalid-track-width")
                .with_arg("track", track)
                .with_arg("frame", keyframe.frame)
                .with_arg("expected", elements)
                .with_arg("actual", keyframe.data.len()));
        }
        if with_tangents
            && (keyframe.in_tan.len() != elements || keyframe.out_tan.len() != elements)
        {
            return Err(MdlError::new("mdx-invalid-tangent-width")
                .with_arg("track", track)
                .with_arg("frame", keyframe.frame)
                .with_arg("expected", elements)
                .with_arg("in_tan", keyframe.in_tan.len())
                .with_arg("out_tan", keyframe.out_tan.len()));
        }
    }
    Ok(())
}

fn write_modl(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    write_chunk(file, b"MODL", |out| {
        write_padded(out, &model.name, 0x150)?;
        out.write_u32::<LittleEndian>(0)?;
        write_extent(
            out,
            model.extent.bounds_radius,
            model.extent.minimum,
            model.extent.maximum,
        )?;
        out.write_u32::<LittleEndian>(model.blend_time)?;
        Ok(())
    })
}

fn write_sequences(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.sequences.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"SEQS", |out| {
        for sequence in &model.sequences {
            write_padded(out, &sequence.name, 0x50)?;
            out.write_u32::<LittleEndian>(sequence.start_frame)?;
            out.write_u32::<LittleEndian>(sequence.end_frame)?;
            out.write_f32::<LittleEndian>(sequence.move_speed)?;
            out.write_u32::<LittleEndian>(u32::from(sequence.non_looping))?;
            out.write_f32::<LittleEndian>(sequence.rarity.unwrap_or(0) as f32)?;
            out.write_u32::<LittleEndian>(0)?;
            write_extent(
                out,
                sequence.extent.bounds_radius,
                sequence.extent.minimum,
                sequence.extent.maximum,
            )?;
        }
        Ok(())
    })
}

fn write_global_sequences(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.global_sequences.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"GLBS", |out| {
        for sequence in &model.global_sequences {
            out.write_u32::<LittleEndian>(sequence.duration)?;
        }
        Ok(())
    })
}

fn filter_to_u32(mode: &FilterMode) -> u32 {
    match mode {
        FilterMode::None => 0,
        FilterMode::Transparent => 1,
        FilterMode::Blend => 2,
        FilterMode::Additive => 3,
        FilterMode::AddAlpha => 4,
        FilterMode::Modulate => 5,
        FilterMode::Modulate2x => 6,
    }
}

fn write_materials(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.materials.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"MTLS", |out| {
        for material in &model.materials {
            write_inclusive(out, |mat| {
                mat.write_i32::<LittleEndian>(material.priority_plane)?;
                let mut bits = 0u32;
                if material.flags.constant_color {
                    bits |= 1;
                }
                if material.flags.sort_primitives_far_z {
                    bits |= 16;
                }
                if material.flags.full_resolution {
                    bits |= 32;
                }
                mat.write_u32::<LittleEndian>(bits)?;
                mat.write_all(b"LAYS")?;
                mat.write_u32::<LittleEndian>(material.layers.len() as u32)?;
                for layer in &material.layers {
                    write_inclusive(mat, |lay| {
                        lay.write_u32::<LittleEndian>(filter_to_u32(&layer.filter_mode))?;
                        lay.write_u32::<LittleEndian>(ShadingFlags::get_bits(
                            &layer.shading_flags,
                        ))?;
                        lay.write_u32::<LittleEndian>(layer.texture_id.unwrap_or(0) as u32)?;
                        let txan = layer
                            .extra
                            .texture_anim_id
                            .map(|id| id.0 as i32)
                            .unwrap_or(-1);
                        lay.write_i32::<LittleEndian>(txan)?;
                        lay.write_u32::<LittleEndian>(layer.extra.coord_id)?;
                        lay.write_f32::<LittleEndian>(layer.alpha)?;
                        write_controller(lay, model, TAG_KMTA, layer.alpha_track, false)?;
                        write_controller(lay, model, TAG_KMTF, layer.texture_id_track, true)?;
                        Ok(())
                    })?;
                }
                Ok(())
            })?;
        }
        Ok(())
    })
}

fn write_textures(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.textures.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"TEXS", |out| {
        for texture in &model.textures {
            out.write_u32::<LittleEndian>(texture.replaceable_id)?;
            write_padded(out, &texture.filename, 0x100)?;
            out.write_u32::<LittleEndian>(0)?;
            out.write_u32::<LittleEndian>(texture.flags.bits())?;
        }
        Ok(())
    })
}

fn write_texture_anims(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.texture_anims.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"TXAN", |out| {
        for anim in &model.texture_anims {
            write_tex_anim(out, model, anim)?;
        }
        Ok(())
    })
}

fn write_tex_anim(
    file: &mut dyn WriteSeek,
    model: &Model,
    anim: &TextureAnim,
) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        write_controller(out, model, TAG_KTAT, anim.translation.0, false)?;
        write_controller(out, model, TAG_KTAR, anim.rotation.0, false)?;
        write_controller(out, model, TAG_KTAS, anim.scaling.0, false)?;
        Ok(())
    })
}

fn write_geosets(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.geosets.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"GEOS", |out| {
        for geoset in &model.geosets {
            write_inclusive(out, |geo| {
                geo.write_all(b"VRTX")?;
                geo.write_u32::<LittleEndian>(geoset.vertices.len() as u32)?;
                for vertex in &geoset.vertices {
                    write_vec3(geo, vertex.position)?;
                }

                geo.write_all(b"NRMS")?;
                geo.write_u32::<LittleEndian>(geoset.vertices.len() as u32)?;
                for i in 0..geoset.vertices.len() {
                    let normal = geoset
                        .normals
                        .get(i)
                        .map(|n| n.normal)
                        .unwrap_or([0.0, 0.0, 1.0]);
                    write_vec3(geo, normal)?;
                }

                geo.write_all(b"PTYP")?;
                geo.write_u32::<LittleEndian>(1)?;
                geo.write_u32::<LittleEndian>(4)?;
                geo.write_all(b"PCNT")?;
                geo.write_u32::<LittleEndian>(1)?;
                let index_count = geoset.faces.len() * 3;
                geo.write_u32::<LittleEndian>(index_count as u32)?;
                geo.write_all(b"PVTX")?;
                geo.write_u32::<LittleEndian>(index_count as u32)?;
                for face in &geoset.faces {
                    for index in face.vertices {
                        geo.write_u16::<LittleEndian>(index as u16)?;
                    }
                }

                geo.write_all(b"GNDX")?;
                geo.write_u32::<LittleEndian>(geoset.vertices.len() as u32)?;
                for i in 0..geoset.vertices.len() {
                    geo.write_u8(*geoset.vertex_groups.get(i).unwrap_or(&0))?;
                }

                geo.write_all(b"MTGC")?;
                geo.write_u32::<LittleEndian>(geoset.matrix_groups.len() as u32)?;
                for group in &geoset.matrix_groups {
                    geo.write_u32::<LittleEndian>(group.len() as u32)?;
                }
                geo.write_all(b"MATS")?;
                let mats: usize = geoset.matrix_groups.iter().map(|g| g.len()).sum();
                geo.write_u32::<LittleEndian>(mats as u32)?;
                for group in &geoset.matrix_groups {
                    for bone in group {
                        geo.write_u32::<LittleEndian>(*bone)?;
                    }
                }

                geo.write_u32::<LittleEndian>(geoset.material_id.unwrap_or(0) as u32)?;
                geo.write_u32::<LittleEndian>(geoset.selection_group as u32)?;
                geo.write_u32::<LittleEndian>(if geoset.unselectable { 4 } else { 0 })?;
                write_extent(
                    geo,
                    geoset.bounds_radius,
                    geoset.minimum_extent,
                    geoset.maximum_extent,
                )?;
                geo.write_u32::<LittleEndian>(0)?;

                geo.write_all(b"UVAS")?;
                geo.write_u32::<LittleEndian>(geoset.tex_coord_sets.len() as u32)?;
                for tex_coords in &geoset.tex_coord_sets {
                    geo.write_all(b"UVBS")?;
                    geo.write_u32::<LittleEndian>(tex_coords.len() as u32)?;
                    for uv in tex_coords {
                        geo.write_f32::<LittleEndian>(uv.uv[0])?;
                        geo.write_f32::<LittleEndian>(uv.uv[1])?;
                    }
                }
                Ok(())
            })?;
        }
        Ok(())
    })
}

fn write_geoset_anims(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.geoset_anims.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"GEOA", |out| {
        for anim in &model.geoset_anims {
            write_geoset_anim(out, model, anim)?;
        }
        Ok(())
    })
}

fn write_geoset_anim(
    file: &mut dyn WriteSeek,
    model: &Model,
    anim: &GeosetAnim,
) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        out.write_f32::<LittleEndian>(anim.alpha)?;
        let mut flags = 0u32;
        if anim.drop_shadow {
            flags |= 1;
        }
        flags |= 2;
        out.write_u32::<LittleEndian>(flags)?;
        write_vec3(out, anim.color)?;
        out.write_i32::<LittleEndian>(anim.geoset_id.map(|id| id.0 as i32).unwrap_or(-1))?;
        write_controller(out, model, TAG_KGAO, anim.alpha_track.0, false)?;
        write_controller(out, model, TAG_KGAC, anim.color_track.0, false)?;
        Ok(())
    })
}

fn write_bones(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.bones.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"BONE", |out| {
        for bone in &model.bones {
            write_node(out, model, &bone_as_node(bone))?;
            out.write_i32::<LittleEndian>(bone.geoset_id.map(|id| id as i32).unwrap_or(-1))?;
            out.write_i32::<LittleEndian>(bone.geoset_anim_id.map(|id| id as i32).unwrap_or(-1))?;
        }
        Ok(())
    })
}

fn write_helpers(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.helpers.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"HELP", |out| {
        for helper in &model.helpers {
            write_node(out, model, &helper_as_node(helper))?;
        }
        Ok(())
    })
}

fn write_lights(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.lights.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"LITE", |out| {
        for light in &model.lights {
            write_light(out, model, light)?;
        }
        Ok(())
    })
}

fn write_light(file: &mut dyn WriteSeek, model: &Model, light: &Light) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        let mut node = light.node.clone();
        node.flags = crate::model::node::NodeFlags::from_bits(node.flags.bits() | TYPE_LITE);
        node.visibility = crate::model::ids::TrackId::NONE;
        write_node(out, model, &node)?;
        out.write_u32::<LittleEndian>(light.light_type as u32)?;
        out.write_f32::<LittleEndian>(light.attenuation_start)?;
        out.write_f32::<LittleEndian>(light.attenuation_end)?;
        write_vec3(out, light.color)?;
        out.write_f32::<LittleEndian>(light.intensity)?;
        write_vec3(out, light.ambient_color)?;
        out.write_f32::<LittleEndian>(light.ambient_intensity)?;
        write_controller(out, model, TAG_KLAS, light.attenuation_start_track.0, false)?;
        write_controller(out, model, TAG_KLAE, light.attenuation_end_track.0, false)?;
        write_controller(out, model, TAG_KLAI, light.intensity_track.0, false)?;
        write_controller(out, model, TAG_KLAV, light.node.visibility.0, false)?;
        write_controller(out, model, TAG_KLAC, light.color_track.0, false)?;
        write_controller(out, model, TAG_KLBC, light.ambient_color_track.0, false)?;
        write_controller(out, model, TAG_KLBI, light.ambient_intensity_track.0, false)?;
        Ok(())
    })
}

fn write_attachments(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.attachments.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"ATCH", |out| {
        for attachment in &model.attachments {
            write_attachment(out, model, attachment)?;
        }
        Ok(())
    })
}

fn write_attachment(
    file: &mut dyn WriteSeek,
    model: &Model,
    attachment: &Attachment,
) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        let mut node = attachment.node.clone();
        node.flags = crate::model::node::NodeFlags::from_bits(node.flags.bits() | TYPE_ATCH);
        node.visibility = crate::model::ids::TrackId::NONE;
        write_node(out, model, &node)?;
        write_padded(out, &attachment.path, 0x100)?;
        out.write_u32::<LittleEndian>(0)?;
        out.write_i32::<LittleEndian>(attachment.attachment_id)?;
        write_controller(out, model, TAG_KATV, attachment.node.visibility.0, false)?;
        Ok(())
    })
}

fn write_pivots(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.pivot_points.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"PIVT", |out| {
        for point in &model.pivot_points {
            write_vec3(out, *point)?;
        }
        Ok(())
    })
}

fn write_particle_emitters(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.particle_emitters.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"PREM", |out| {
        for emitter in &model.particle_emitters {
            write_particle_emitter(out, model, emitter)?;
        }
        Ok(())
    })
}

fn write_particle_emitter(
    file: &mut dyn WriteSeek,
    model: &Model,
    emitter: &ParticleEmitter,
) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        let mut node = emitter.node.clone();
        node.visibility = crate::model::ids::TrackId::NONE;
        let uses_bits = match emitter.uses_type {
            ParticleEmitterUses::Tga => 0x1_0000,
            ParticleEmitterUses::Mdl => 0x8000,
        };
        node.flags = crate::model::node::NodeFlags::from_bits(
            (node.flags.bits() & !(0x8000 | 0x1_0000)) | uses_bits,
        );
        write_node(out, model, &node)?;
        out.write_f32::<LittleEndian>(emitter.emission_rate)?;
        out.write_f32::<LittleEndian>(emitter.gravity)?;
        out.write_f32::<LittleEndian>(emitter.longitude)?;
        out.write_f32::<LittleEndian>(emitter.latitude)?;
        write_padded(out, &emitter.path, 0x100)?;
        out.write_u32::<LittleEndian>(0)?;
        out.write_f32::<LittleEndian>(emitter.life_span)?;
        out.write_f32::<LittleEndian>(emitter.init_velocity)?;
        write_controller(out, model, TAG_KPEE, emitter.emission_rate_track.0, false)?;
        write_controller(out, model, TAG_KPEG, emitter.gravity_track.0, false)?;
        write_controller(out, model, TAG_KPLN, emitter.longitude_track.0, false)?;
        write_controller(out, model, TAG_KPLT, emitter.latitude_track.0, false)?;
        write_controller(out, model, TAG_KPEL, emitter.life_span_track.0, false)?;
        write_controller(out, model, TAG_KPES, emitter.init_velocity_track.0, false)?;
        write_controller(out, model, TAG_KPEV, emitter.node.visibility.0, false)?;
        Ok(())
    })
}

fn write_particle_emitters_2(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.particle_emitters_2.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"PRE2", |out| {
        for emitter in &model.particle_emitters_2 {
            write_particle_emitter_2(out, model, emitter)?;
        }
        Ok(())
    })
}

fn write_particle_emitter_2(
    file: &mut dyn WriteSeek,
    model: &Model,
    emitter: &ParticleEmitter2,
) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        let mut node = emitter.node.clone();
        node.visibility = crate::model::ids::TrackId::NONE;
        write_node(out, model, &node)?;
        out.write_f32::<LittleEndian>(emitter.speed)?;
        out.write_f32::<LittleEndian>(emitter.variation)?;
        out.write_f32::<LittleEndian>(emitter.latitude)?;
        out.write_f32::<LittleEndian>(emitter.gravity)?;
        out.write_f32::<LittleEndian>(emitter.life_span)?;
        out.write_f32::<LittleEndian>(emitter.emission_rate)?;
        out.write_f32::<LittleEndian>(emitter.length)?;
        out.write_f32::<LittleEndian>(emitter.width)?;
        out.write_u32::<LittleEndian>(emitter.blend_mode)?;
        out.write_u32::<LittleEndian>(emitter.rows)?;
        out.write_u32::<LittleEndian>(emitter.columns)?;
        out.write_u32::<LittleEndian>(emitter.particle_type)?;
        out.write_f32::<LittleEndian>(emitter.tail_length)?;
        out.write_f32::<LittleEndian>(emitter.time)?;
        for color in &emitter.segment_color {
            write_vec3(out, *color)?;
        }
        out.write_all(&emitter.alpha)?;
        write_vec3(out, emitter.particle_scaling)?;
        for _ in 0..12 {
            out.write_u32::<LittleEndian>(0)?;
        }
        out.write_u32::<LittleEndian>(emitter.texture_id.map(|id| id.0).unwrap_or(0))?;
        out.write_u32::<LittleEndian>(u32::from(emitter.squirt))?;
        out.write_i32::<LittleEndian>(emitter.priority_plane)?;
        out.write_u32::<LittleEndian>(emitter.replaceable_id)?;
        write_controller(
            out,
            model,
            crate::parser::io::TAG_KP2S,
            emitter.speed_track.0,
            false,
        )?;
        write_controller(
            out,
            model,
            crate::parser::io::TAG_KP2R,
            emitter.variation_track.0,
            false,
        )?;
        write_controller(
            out,
            model,
            crate::parser::io::TAG_KP2L,
            emitter.latitude_track.0,
            false,
        )?;
        write_controller(
            out,
            model,
            crate::parser::io::TAG_KP2G,
            emitter.gravity_track.0,
            false,
        )?;
        write_controller(
            out,
            model,
            crate::parser::io::TAG_KP2E,
            emitter.emission_rate_track.0,
            false,
        )?;
        write_controller(
            out,
            model,
            crate::parser::io::TAG_KP2V,
            emitter.node.visibility.0,
            false,
        )?;
        write_controller(
            out,
            model,
            crate::parser::io::TAG_KP2N,
            emitter.length_track.0,
            false,
        )?;
        write_controller(
            out,
            model,
            crate::parser::io::TAG_KP2W,
            emitter.width_track.0,
            false,
        )?;
        Ok(())
    })
}

fn write_ribbons(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.ribbons.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"RIBB", |out| {
        for ribbon in &model.ribbons {
            write_ribbon(out, model, ribbon)?;
        }
        Ok(())
    })
}

fn write_ribbon(
    file: &mut dyn WriteSeek,
    model: &Model,
    ribbon: &RibbonEmitter,
) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        let mut node = ribbon.node.clone();
        node.visibility = crate::model::ids::TrackId::NONE;
        write_node(out, model, &node)?;
        out.write_f32::<LittleEndian>(ribbon.height_above)?;
        out.write_f32::<LittleEndian>(ribbon.height_below)?;
        out.write_f32::<LittleEndian>(ribbon.alpha)?;
        write_vec3(out, ribbon.color)?;
        out.write_f32::<LittleEndian>(ribbon.life_span)?;
        out.write_i32::<LittleEndian>(ribbon.texture_slot)?;
        out.write_u32::<LittleEndian>(ribbon.emission_rate)?;
        out.write_u32::<LittleEndian>(ribbon.rows)?;
        out.write_u32::<LittleEndian>(ribbon.columns)?;
        out.write_u32::<LittleEndian>(ribbon.material_id.map(|id| id.0).unwrap_or(0))?;
        out.write_f32::<LittleEndian>(ribbon.gravity)?;
        write_controller(out, model, TAG_KRHA, ribbon.height_above_track.0, false)?;
        write_controller(out, model, TAG_KRHB, ribbon.height_below_track.0, false)?;
        write_controller(out, model, TAG_KRAL, ribbon.alpha_track.0, false)?;
        write_controller(out, model, TAG_KRCO, ribbon.color_track.0, false)?;
        write_controller(out, model, TAG_KRVS, ribbon.node.visibility.0, false)?;
        Ok(())
    })
}

fn write_cameras(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.cameras.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"CAMS", |out| {
        for camera in &model.cameras {
            write_camera(out, model, camera)?;
        }
        Ok(())
    })
}

fn write_camera(file: &mut dyn WriteSeek, model: &Model, camera: &Camera) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        write_padded(out, &camera.name, 0x50)?;
        write_vec3(out, camera.position)?;
        out.write_f32::<LittleEndian>(camera.field_of_view)?;
        out.write_f32::<LittleEndian>(camera.far_clip)?;
        out.write_f32::<LittleEndian>(camera.near_clip)?;
        write_vec3(out, camera.target_position)?;
        write_controller(out, model, TAG_KCTR, camera.target_translation.0, false)?;
        write_controller(out, model, TAG_KCRL, camera.rotation.0, false)?;
        write_controller(out, model, TAG_KTTR, camera.translation.0, false)?;
        Ok(())
    })
}

fn write_events(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.events.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"EVTS", |out| {
        for event in &model.events {
            write_event(out, model, event)?;
        }
        Ok(())
    })
}

fn write_event(
    file: &mut dyn WriteSeek,
    model: &Model,
    event: &EventObject,
) -> Result<(), MdlError> {
    let mut node = event.node.clone();
    node.flags = crate::model::node::NodeFlags::from_bits(node.flags.bits() | TYPE_EVTS);
    write_node(file, model, &node)?;
    file.write_all(b"KEVT")?;
    file.write_u32::<LittleEndian>(event.tracks.len() as u32)?;
    file.write_i32::<LittleEndian>(event.global_seq_id.0)?;
    for track in &event.tracks {
        file.write_i32::<LittleEndian>(*track)?;
    }
    Ok(())
}

fn write_collisions(file: &mut dyn WriteSeek, model: &Model) -> Result<(), MdlError> {
    if model.collisions.is_empty() {
        return Ok(());
    }
    write_chunk(file, b"CLID", |out| {
        for shape in &model.collisions {
            write_collision(out, model, shape)?;
        }
        Ok(())
    })
}

fn write_collision(
    file: &mut dyn WriteSeek,
    model: &Model,
    shape: &CollisionShape,
) -> Result<(), MdlError> {
    let mut node = shape.node.clone();
    node.flags = crate::model::node::NodeFlags::from_bits(node.flags.bits() | TYPE_CLID);
    write_node(file, model, &node)?;
    match shape.kind {
        CollisionType::Box => {
            file.write_u32::<LittleEndian>(0)?;
            let a = shape.vertices.first().copied().unwrap_or([0.0; 3]);
            let b = shape.vertices.get(1).copied().unwrap_or(a);
            write_vec3(file, a)?;
            write_vec3(file, b)?;
        }
        CollisionType::Sphere => {
            file.write_u32::<LittleEndian>(2)?;
            let center = shape.vertices.first().copied().unwrap_or([0.0; 3]);
            write_vec3(file, center)?;
            file.write_f32::<LittleEndian>(shape.bounds_radius)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::chunk::UnknownChunk;
    use crate::model::geoset::{Face, Geoset, Normal, TexCoord, Vertex};
    use crate::model::ids::{MaterialIndex, TextureIndex, TrackId};
    use crate::model::node::{NodeFlags, NodeRef};
    use crate::model::objects::{
        GlobalSequence, ParticleEmitter, ParticleEmitter2, ParticleEmitterUses, RibbonEmitter,
        TextureAnim,
    };
    use crate::model::skeleton::Keyframe;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn keyframe(data: Vec<f32>) -> Keyframe {
        Keyframe {
            frame: 100,
            data,
            in_tan: Vec::new(),
            out_tan: Vec::new(),
        }
    }

    fn controller(global_seq_id: i32, data: Vec<f32>) -> AnimationController {
        AnimationController {
            interpolation_type: 1,
            global_seq_id,
            keyframes: vec![keyframe(data)],
        }
    }

    fn model_with_translation(controller: AnimationController) -> Model {
        let mut model = Model {
            name: "MDX writer regression".to_string(),
            controllers: vec![controller],
            ..Model::default()
        };
        model.texture_anims.push(TextureAnim {
            translation: TrackId(0),
            rotation: TrackId::NONE,
            scaling: TrackId::NONE,
        });
        model
    }

    fn temp_path(label: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "mdlvis-rs-mdx-writer-{label}-{}-{stamp}.mdx",
            std::process::id()
        ))
    }

    fn round_trip(model: &Model) -> Model {
        let path = temp_path("round-trip");
        save_path(&path, model).expect("write MDX");
        let mut file = File::open(&path).expect("open written MDX");
        let loaded = crate::parser::load::load(&mut file).expect("reload written MDX");
        fs::remove_file(path).expect("remove temporary MDX");
        loaded
    }

    fn model_with_uv_sets(tex_coord_sets: Vec<Vec<TexCoord>>) -> Model {
        Model {
            name: "multi UV regression".to_string(),
            geosets: vec![Geoset {
                vertices: vec![
                    Vertex {
                        position: [0.0, 0.0, 0.0],
                    },
                    Vertex {
                        position: [1.0, 0.0, 0.0],
                    },
                    Vertex {
                        position: [0.0, 1.0, 0.0],
                    },
                ],
                normals: vec![
                    Normal {
                        normal: [0.0, 0.0, 1.0]
                    };
                    3
                ],
                tex_coord_sets,
                faces: vec![Face {
                    vertices: [0, 1, 2],
                }],
                material_id: Some(0),
                vertex_groups: vec![0, 0, 0],
                ..Geoset::default()
            }],
            ..Model::default()
        }
    }

    #[test]
    fn zero_one_and_two_uv_sets_survive_full_mdx_round_trip() {
        let first = vec![
            TexCoord { uv: [0.0, 0.0] },
            TexCoord { uv: [1.0, 0.0] },
            TexCoord { uv: [0.0, 1.0] },
        ];
        let second = vec![
            TexCoord { uv: [0.25, 0.75] },
            TexCoord { uv: [0.5, 0.5] },
            TexCoord { uv: [0.75, 0.25] },
        ];

        for sets in [vec![], vec![first.clone()], vec![first, second]] {
            let model = model_with_uv_sets(sets);
            let reloaded = round_trip(&model);
            assert_models_equal(&reloaded, &model, "UVAS/UVBS round trip");
        }
    }

    #[test]
    fn invalid_uv_set_size_does_not_overwrite_or_create_mdx_targets() {
        let mut model = model_with_uv_sets(vec![vec![
            TexCoord { uv: [0.0, 0.0] },
            TexCoord { uv: [1.0, 0.0] },
        ]]);
        let err = serialize(&model).expect_err("UV set is shorter than vertices");
        assert_eq!(err.key, "mdx-invalid-uv-set-size");

        let existing = temp_path("invalid-uv-existing");
        let missing = temp_path("invalid-uv-missing");
        let _ = fs::remove_file(&missing);
        fs::write(&existing, b"keep me").unwrap();
        assert!(save_path(&existing, &model).is_err());
        assert_eq!(fs::read(&existing).unwrap(), b"keep me");
        assert!(save_path(&missing, &model).is_err());
        assert!(!missing.exists());
        fs::remove_file(existing).unwrap();

        model.geosets[0].tex_coord_sets.clear();
        assert!(serialize(&model).is_ok(), "zero UV sets are valid");
    }

    #[test]
    fn damaged_uv_chunks_return_errors_without_panicking() {
        let model = model_with_uv_sets(vec![vec![
            TexCoord { uv: [0.0, 0.0] },
            TexCoord { uv: [1.0, 0.0] },
            TexCoord { uv: [0.0, 1.0] },
        ]]);
        let bytes = serialize(&model).expect("serialize valid UV set");
        let geos = bytes.windows(4).position(|tag| tag == b"GEOS").unwrap();
        let geoset_start = geos + 8;
        let uvas = bytes.windows(4).position(|tag| tag == b"UVAS").unwrap();
        let uvbs = uvas + 8;

        let mut cases = Vec::new();
        let mut invalid_size = bytes.clone();
        invalid_size[geoset_start..geoset_start + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        cases.push(invalid_size);
        let mut invalid_tag = bytes.clone();
        invalid_tag[uvbs..uvbs + 4].copy_from_slice(b"NOPE");
        cases.push(invalid_tag);
        cases.push(bytes[..uvbs + 10].to_vec());

        for (index, damaged) in cases.into_iter().enumerate() {
            let path = temp_path(&format!("damaged-uv-{index}"));
            fs::write(&path, damaged).unwrap();
            let result = std::panic::catch_unwind(|| {
                let mut file = File::open(&path).unwrap();
                crate::parser::load::load(&mut file)
            });
            fs::remove_file(path).unwrap();
            assert!(result.is_ok(), "damaged UV chunk panicked");
            assert!(result.unwrap().is_err(), "damaged UV chunk parsed");
        }
    }

    fn scalar_controllers(count: usize) -> Vec<AnimationController> {
        (1..=count)
            .map(|value| controller(-1, vec![value as f32]))
            .collect()
    }

    fn tracked_sample(relative: &str) -> Model {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-data")
            .join(relative);
        let mut file = File::open(path).expect("open tracked VERS 800 sample");
        crate::parser::load::load(&mut file).expect("load tracked sample")
    }

    fn assert_models_equal(actual: &Model, expected: &Model, context: &str) {
        let actual_value = serde_json::to_value(actual).expect("serialize actual Model");
        let expected_value = serde_json::to_value(expected).expect("serialize expected Model");
        for (field, expected_field) in expected_value.as_object().expect("Model is an object") {
            if actual_value.get(field) != Some(expected_field) {
                let detail = if field == "controllers" {
                    let actual_controllers = actual_value[field]
                        .as_array()
                        .expect("controllers are an array");
                    let expected_controllers =
                        expected_field.as_array().expect("controllers are an array");
                    let first_difference = actual_controllers
                        .iter()
                        .zip(expected_controllers)
                        .position(|(left, right)| left != right);
                    let expected_detail = first_difference
                        .and_then(|index| expected_controllers.get(index))
                        .cloned();
                    let actual_detail = first_difference
                        .and_then(|index| actual_controllers.get(index))
                        .cloned();
                    format!(
                        "controller counts {} != {}, first difference {:?}: actual {:?}, expected {:?}",
                        actual_controllers.len(),
                        expected_controllers.len(),
                        first_difference,
                        actual_detail,
                        expected_detail
                    )
                } else {
                    String::new()
                };
                panic!("{context}: round-trip changed Model field {field}: {detail}");
            }
        }
        assert_eq!(actual_value, expected_value, "{context}");
    }

    #[test]
    fn controller_global_sequence_ids_survive_full_model_round_trip() {
        let mut model = model_with_translation(controller(-1, vec![1.0, 2.0, 3.0]));
        model
            .global_sequences
            .push(GlobalSequence { duration: 1_000 });
        model.controllers.push(controller(0, vec![4.0, 5.0, 6.0]));
        model.texture_anims.push(TextureAnim {
            translation: TrackId(1),
            rotation: TrackId::NONE,
            scaling: TrackId::NONE,
        });
        model
            .unknown_chunks
            .push(UnknownChunk::new(*b"ZZZZ", vec![1, 2, 3, 4]));

        let first_loaded = round_trip(&model);
        let loaded = round_trip(&first_loaded);
        assert_eq!(first_loaded.controllers[0].global_seq_id, -1);
        assert_eq!(first_loaded.controllers[1].global_seq_id, 0);
        assert_eq!(
            serde_json::to_value(&loaded).expect("serialize loaded Model"),
            serde_json::to_value(&first_loaded).expect("serialize original loaded Model")
        );
    }

    #[test]
    fn tracked_model_preserves_pre2_animated_fields() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-data")
            .join("Nether Blast/Nether Blast I.mdx");
        let mut file = File::open(path).expect("open tracked VERS 800 sample");
        let original = crate::parser::load::load(&mut file).expect("load tracked sample");
        assert!(original.particle_emitters_2.iter().all(|emitter| {
            emitter.emission_rate_track.0 >= 0
                && emitter.speed_track.is_none()
                && emitter.variation_track.is_none()
                && emitter.latitude_track.is_none()
                && emitter.gravity_track.is_none()
                && emitter.width_track.is_none()
                && emitter.length_track.is_none()
        }));
        let reloaded = round_trip(&original);

        assert_eq!(original.controllers.len(), 20);
        assert_eq!(reloaded.controllers.len(), 20);
        let original_value = serde_json::to_value(&original).expect("serialize original Model");
        let reloaded_value = serde_json::to_value(&reloaded).expect("serialize reloaded Model");
        for (field, expected) in original_value.as_object().expect("Model is an object") {
            assert_eq!(
                reloaded_value.get(field),
                Some(expected),
                "round-trip changed Model field {field}"
            );
        }
        assert_eq!(reloaded_value, original_value,);
    }

    #[test]
    fn all_pre2_animated_fields_survive_full_model_round_trip() {
        let mut model = Model {
            name: "PRE2 animated fields".to_string(),
            controllers: (1..=8)
                .map(|value| controller(-1, vec![value as f32]))
                .collect(),
            ..Model::default()
        };
        let mut emitter = ParticleEmitter2 {
            texture_id: Some(TextureIndex(0)),
            speed_track: TrackId(0),
            variation_track: TrackId(1),
            latitude_track: TrackId(2),
            gravity_track: TrackId(3),
            emission_rate_track: TrackId(4),
            length_track: TrackId(6),
            width_track: TrackId(7),
            ..ParticleEmitter2::default()
        };
        emitter.node.visibility = TrackId(5);
        model.particle_emitters_2.push(emitter);

        let reloaded = round_trip(&model);

        assert_eq!(
            serde_json::to_value(&reloaded).expect("serialize reloaded Model"),
            serde_json::to_value(&model).expect("serialize original Model")
        );
    }

    #[test]
    fn pre2_animated_fields_are_validated() {
        let mut invalid_index = Model::default();
        invalid_index.particle_emitters_2.push(ParticleEmitter2 {
            speed_track: TrackId(999),
            ..ParticleEmitter2::default()
        });
        let err = serialize(&invalid_index).expect_err("PRE2 controller index must be valid");
        assert_eq!(err.key, "mdx-invalid-controller-index");

        let mut invalid_width = Model {
            controllers: vec![controller(-1, vec![1.0, 2.0])],
            ..Model::default()
        };
        invalid_width.particle_emitters_2.push(ParticleEmitter2 {
            emission_rate_track: TrackId(0),
            ..ParticleEmitter2::default()
        });
        let err = serialize(&invalid_width).expect_err("PRE2 track width must be scalar");
        assert_eq!(err.key, "mdx-invalid-track-width");
    }

    #[test]
    fn invalid_controller_index_is_rejected() {
        let mut model = Model::default();
        model.texture_anims.push(TextureAnim {
            translation: TrackId(999),
            rotation: TrackId::NONE,
            scaling: TrackId::NONE,
        });

        let err = serialize(&model).expect_err("out-of-range controller must fail");
        assert_eq!(err.key, "mdx-invalid-controller-index");
    }

    #[test]
    fn save_path_does_not_touch_targets_when_validation_fails() {
        let mut model = Model::default();
        model.texture_anims.push(TextureAnim {
            translation: TrackId(999),
            rotation: TrackId::NONE,
            scaling: TrackId::NONE,
        });
        let existing = temp_path("existing");
        let missing = temp_path("missing");
        fs::write(&existing, b"keep me").expect("create existing target");
        let _ = fs::remove_file(&missing);

        assert!(save_path(&existing, &model).is_err());
        assert_eq!(
            fs::read(&existing).expect("read existing target"),
            b"keep me"
        );
        assert!(save_path(&missing, &model).is_err());
        assert!(!missing.exists());

        fs::remove_file(existing).expect("remove existing target");
    }

    #[test]
    fn invalid_controller_data_and_tangent_widths_are_rejected() {
        let data_error = model_with_translation(controller(-1, vec![1.0, 2.0]));
        let err = serialize(&data_error).expect_err("short translation data must fail");
        assert_eq!(err.key, "mdx-invalid-track-width");

        let mut tangent_controller = controller(-1, vec![1.0, 2.0, 3.0]);
        tangent_controller.interpolation_type = 2;
        tangent_controller.keyframes[0].in_tan = vec![0.0, 0.0];
        tangent_controller.keyframes[0].out_tan = vec![0.0, 0.0, 0.0];
        let tangent_error = model_with_translation(tangent_controller);
        let err = serialize(&tangent_error).expect_err("short translation tangent must fail");
        assert_eq!(err.key, "mdx-invalid-tangent-width");
    }

    #[test]
    fn invalid_interpolation_type_is_rejected_without_touching_targets() {
        let mut invalid_controller = controller(-1, vec![1.0, 2.0, 3.0]);
        invalid_controller.interpolation_type = 99;
        let model = model_with_translation(invalid_controller);
        let existing = temp_path("invalid-interpolation-existing");
        let missing = temp_path("invalid-interpolation-missing");
        fs::write(&existing, b"keep me").expect("create existing target");
        let _ = fs::remove_file(&missing);

        let result = std::panic::catch_unwind(|| serialize(&model));
        let err = result
            .expect("invalid interpolation must not panic")
            .expect_err("invalid interpolation must fail");
        assert_eq!(err.key, "mdx-invalid-interpolation-type");
        assert!(save_path(&existing, &model).is_err());
        assert_eq!(
            fs::read(&existing).expect("read existing target"),
            b"keep me"
        );
        assert!(save_path(&missing, &model).is_err());
        assert!(!missing.exists());

        fs::remove_file(existing).expect("remove existing target");
    }

    #[test]
    fn tracked_prem_and_ribbon_models_survive_first_full_model_round_trip() {
        for relative in [
            "Ember Forge  Ember Knight/Ember Forge_opt2.mdx",
            "Ember Forge  Ember Knight/Ember Knight/Ember Knight_opt2.mdx",
            "Ember Forge  Ember Knight/Fire_Stream.mdx",
            "Nether Blast/Nether Blast III.mdx",
        ] {
            let original = tracked_sample(relative);
            let reloaded = round_trip(&original);
            assert_models_equal(&reloaded, &original, relative);
        }
    }

    #[test]
    fn all_prem_and_ribbon_animated_fields_survive_full_model_round_trip() {
        let mut model = Model {
            name: "PREM and RIBB animated fields".to_string(),
            controllers: scalar_controllers(12),
            ..Model::default()
        };
        model.controllers[10] = controller(-1, vec![0.1, 0.2, 0.3]);

        let mut particle = ParticleEmitter {
            node: NodeRef {
                name: "Particle".to_string(),
                flags: NodeFlags::from_bits(0x1000),
                visibility: TrackId(6),
                ..NodeRef::default()
            },
            uses_type: ParticleEmitterUses::Mdl,
            emission_rate_track: TrackId(0),
            gravity_track: TrackId(1),
            longitude_track: TrackId(2),
            latitude_track: TrackId(3),
            life_span_track: TrackId(4),
            init_velocity_track: TrackId(5),
            path: "Particles\\Animated.mdl".to_string(),
            ..ParticleEmitter::default()
        };
        particle.node.object_id.0 = 1;
        model.particle_emitters.push(particle);

        let mut ribbon = RibbonEmitter {
            node: NodeRef {
                name: "Ribbon".to_string(),
                flags: NodeFlags::from_bits(0x4000),
                visibility: TrackId(11),
                ..NodeRef::default()
            },
            height_above_track: TrackId(7),
            height_below_track: TrackId(8),
            alpha_track: TrackId(9),
            color_track: TrackId(10),
            material_id: Some(MaterialIndex(0)),
            ..RibbonEmitter::default()
        };
        ribbon.node.object_id.0 = 2;
        model.ribbons.push(ribbon);

        let reloaded = round_trip(&model);
        assert_eq!(
            serde_json::to_value(&reloaded).expect("serialize reloaded Model"),
            serde_json::to_value(&model).expect("serialize original Model")
        );
    }

    #[test]
    fn prem_and_ribbon_animated_fields_are_fully_validated() {
        let mut invalid_index = Model::default();
        invalid_index.particle_emitters.push(ParticleEmitter {
            emission_rate_track: TrackId(999),
            ..ParticleEmitter::default()
        });
        let err = serialize(&invalid_index).expect_err("PREM controller index must be valid");
        assert_eq!(err.key, "mdx-invalid-controller-index");

        let mut invalid_width = Model {
            controllers: vec![controller(-1, vec![1.0, 2.0])],
            ..Model::default()
        };
        invalid_width.ribbons.push(RibbonEmitter {
            color_track: TrackId(0),
            ..RibbonEmitter::default()
        });
        let err = serialize(&invalid_width).expect_err("RIBB color track must have width three");
        assert_eq!(err.key, "mdx-invalid-track-width");

        let mut invalid_tangent = controller(-1, vec![1.0]);
        invalid_tangent.interpolation_type = 2;
        invalid_tangent.keyframes[0].in_tan.clear();
        invalid_tangent.keyframes[0].out_tan = vec![0.0];
        let mut invalid_tangent_model = Model {
            controllers: vec![invalid_tangent],
            ..Model::default()
        };
        invalid_tangent_model.ribbons.push(RibbonEmitter {
            alpha_track: TrackId(0),
            ..RibbonEmitter::default()
        });
        let err = serialize(&invalid_tangent_model).expect_err("RIBB tangent width must be valid");
        assert_eq!(err.key, "mdx-invalid-tangent-width");

        let mut invalid_global = Model {
            controllers: vec![controller(0, vec![1.0])],
            ..Model::default()
        };
        invalid_global.particle_emitters.push(ParticleEmitter {
            gravity_track: TrackId(0),
            ..ParticleEmitter::default()
        });
        let err = serialize(&invalid_global).expect_err("PREM global sequence must exist");
        assert_eq!(err.key, "mdx-invalid-global-sequence-index");

        let mut invalid_interpolation_controller = controller(-1, vec![1.0]);
        invalid_interpolation_controller.interpolation_type = 99;
        let mut invalid_interpolation = Model {
            controllers: vec![invalid_interpolation_controller],
            ..Model::default()
        };
        invalid_interpolation
            .particle_emitters
            .push(ParticleEmitter {
                life_span_track: TrackId(0),
                ..ParticleEmitter::default()
            });
        let err = serialize(&invalid_interpolation).expect_err("PREM interpolation must be valid");
        assert_eq!(err.key, "mdx-invalid-interpolation-type");
    }

    #[test]
    fn dynamic_ribbon_texture_slot_is_rejected_without_touching_targets() {
        let mut model = Model {
            controllers: scalar_controllers(1),
            ..Model::default()
        };
        model.ribbons.push(RibbonEmitter {
            texture_slot_track: TrackId(0),
            ..RibbonEmitter::default()
        });
        let existing = temp_path("dynamic-texture-slot-existing");
        let missing = temp_path("dynamic-texture-slot-missing");
        fs::write(&existing, b"keep me").expect("create existing target");
        let _ = fs::remove_file(&missing);

        let err = serialize(&model).expect_err("dynamic texture slot cannot be represented in MDX");
        assert_eq!(err.key, "mdx-ribbon-texture-slot-track-not-representable");
        assert!(save_path(&existing, &model).is_err());
        assert_eq!(
            fs::read(&existing).expect("read existing target"),
            b"keep me"
        );
        assert!(save_path(&missing, &model).is_err());
        assert!(!missing.exists());

        fs::remove_file(existing).expect("remove existing target");
    }

    #[test]
    fn mdlvis_payload_preserves_absent_empty_and_arbitrary_bytes() {
        for payload in [None, Some(Vec::new()), Some(vec![0, 1, 2, 0xff, 0, 7])] {
            let model = Model {
                mdlvis_data: payload.clone(),
                ..Model::default()
            };
            assert_eq!(round_trip(&model).mdlvis_data, payload);
        }
    }

    #[test]
    fn identified_mdvi_cannot_be_written_from_unknown_chunk_pocket() {
        let mut model = Model::default();
        model
            .unknown_chunks
            .push(UnknownChunk::new(*b"MDVI", vec![1, 2, 3]));
        let err = serialize(&model).expect_err("MDVI belongs to mdlvis_data");
        assert_eq!(err.key, "mdx-mdvi-in-unknown-chunks");
    }

    #[test]
    fn mdvi_and_unknown_chunks_keep_separate_payloads() {
        let model = Model {
            mdlvis_data: Some(vec![1, 2, 3]),
            unknown_chunks: vec![UnknownChunk::new(*b"ZZZZ", vec![4, 5, 6])],
            ..Model::default()
        };
        let reloaded = round_trip(&model);
        assert_eq!(reloaded.mdlvis_data, model.mdlvis_data);
        assert_eq!(reloaded.unknown_chunks, model.unknown_chunks);
    }
}
