#![cfg_attr(not(test), allow(dead_code))]

use crate::error::MdlError;
use crate::material::{FilterMode, ShadingFlags};
use crate::model::model::Model;
use crate::model::node::{
    NodeRef, TYPE_ATCH, TYPE_BONE, TYPE_CLID, TYPE_EVTS, TYPE_HELP, TYPE_LITE,
};
use crate::model::objects::{
    Attachment, Camera, CollisionShape, CollisionType, EventObject, GeosetAnim, Light,
    ParticleEmitter, ParticleEmitter2, RibbonEmitter, TextureAnim,
};
use crate::model::skeleton::{AnimationController, Bone, Helper};
use crate::parser::io::{
    TAG_KATV, TAG_KCRL, TAG_KCTR, TAG_KGAC, TAG_KGAO, TAG_KGRT, TAG_KGSC, TAG_KGTR, TAG_KLAC,
    TAG_KLAE, TAG_KLAI, TAG_KLAS, TAG_KLAV, TAG_KLBC, TAG_KLBI, TAG_KMTA, TAG_KMTF, TAG_KPEV,
    TAG_KRVS, TAG_KTAR, TAG_KTAS, TAG_KTAT, TAG_KTTR,
};
use byteorder::{LittleEndian, WriteBytesExt};
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

const SUPPORTED_VERSION: u32 = 800;

pub fn save_path(path: impl AsRef<Path>, model: &Model) -> Result<(), MdlError> {
    let mut file = File::create(path)?;
    save(&mut file, model)
}

pub fn save(file: &mut File, model: &Model) -> Result<(), MdlError> {
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
    for chunk in &model.unknown_chunks {
        file.write_all(&chunk.fourcc)?;
        file.write_u32::<LittleEndian>(chunk.data.len() as u32)?;
        file.write_all(&chunk.data)?;
    }
    Ok(())
}

fn write_chunk<F>(file: &mut File, fourcc: &[u8; 4], body: F) -> Result<(), MdlError>
where
    F: FnOnce(&mut File) -> Result<(), MdlError>,
{
    file.write_all(fourcc)?;
    let size_pos = file.stream_position()?;
    file.write_u32::<LittleEndian>(0)?;
    let start = file.stream_position()?;
    body(file)?;
    patch_size(file, size_pos, start)
}

fn write_inclusive<F>(file: &mut File, body: F) -> Result<(), MdlError>
where
    F: FnOnce(&mut File) -> Result<(), MdlError>,
{
    let size_pos = file.stream_position()?;
    file.write_u32::<LittleEndian>(0)?;
    let start = size_pos;
    body(file)?;
    patch_size(file, size_pos, start)
}

fn patch_size(file: &mut File, size_pos: u64, start: u64) -> Result<(), MdlError> {
    let end = file.stream_position()?;
    let size = (end - start) as u32;
    file.seek(SeekFrom::Start(size_pos))?;
    file.write_u32::<LittleEndian>(size)?;
    file.seek(SeekFrom::Start(end))?;
    Ok(())
}

fn write_padded(file: &mut File, text: &str, len: usize) -> Result<(), MdlError> {
    let mut buf = vec![0u8; len];
    let bytes = text.as_bytes();
    let n = bytes.len().min(len.saturating_sub(1));
    buf[..n].copy_from_slice(&bytes[..n]);
    file.write_all(&buf)?;
    Ok(())
}

fn write_vec3(file: &mut File, v: [f32; 3]) -> Result<(), MdlError> {
    file.write_f32::<LittleEndian>(v[0])?;
    file.write_f32::<LittleEndian>(v[1])?;
    file.write_f32::<LittleEndian>(v[2])?;
    Ok(())
}

fn write_extent(
    file: &mut File,
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
    file: &mut File,
    model: &Model,
    tag: u32,
    idx: i32,
    as_int: bool,
) -> Result<(), MdlError> {
    if idx < 0 {
        return Ok(());
    }
    let Some(controller) = model.controllers.get(idx as usize) else {
        return Ok(());
    };
    write_controller_data(file, tag, controller, as_int)
}

fn write_controller_data(
    file: &mut File,
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
            let value = key.data.first().copied().unwrap_or(0.0) as i32;
            file.write_i32::<LittleEndian>(value)?;
        } else {
            for value in &key.data {
                file.write_f32::<LittleEndian>(*value)?;
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
    }
    Ok(())
}

fn write_node(file: &mut File, model: &Model, node: &NodeRef) -> Result<(), MdlError> {
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

fn write_modl(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_sequences(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_global_sequences(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_materials(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_textures(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_texture_anims(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_tex_anim(file: &mut File, model: &Model, anim: &TextureAnim) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        write_controller(out, model, TAG_KTAT, anim.translation.0, false)?;
        write_controller(out, model, TAG_KTAR, anim.rotation.0, false)?;
        write_controller(out, model, TAG_KTAS, anim.scaling.0, false)?;
        Ok(())
    })
}

fn write_geosets(file: &mut File, model: &Model) -> Result<(), MdlError> {
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
                geo.write_u32::<LittleEndian>(1)?;
                geo.write_all(b"UVBS")?;
                geo.write_u32::<LittleEndian>(geoset.tex_coords.len() as u32)?;
                for uv in &geoset.tex_coords {
                    geo.write_f32::<LittleEndian>(uv.uv[0])?;
                    geo.write_f32::<LittleEndian>(uv.uv[1])?;
                }
                Ok(())
            })?;
        }
        Ok(())
    })
}

fn write_geoset_anims(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_geoset_anim(file: &mut File, model: &Model, anim: &GeosetAnim) -> Result<(), MdlError> {
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

fn write_bones(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_helpers(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_lights(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_light(file: &mut File, model: &Model, light: &Light) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        let mut node = light.node.clone();
        node.flags = crate::model::node::NodeFlags::from_bits(node.flags.bits() | TYPE_LITE);
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

fn write_attachments(file: &mut File, model: &Model) -> Result<(), MdlError> {
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
    file: &mut File,
    model: &Model,
    attachment: &Attachment,
) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        let mut node = attachment.node.clone();
        node.flags = crate::model::node::NodeFlags::from_bits(node.flags.bits() | TYPE_ATCH);
        write_node(out, model, &node)?;
        write_padded(out, &attachment.path, 0x100)?;
        out.write_u32::<LittleEndian>(0)?;
        out.write_i32::<LittleEndian>(attachment.attachment_id)?;
        write_controller(out, model, TAG_KATV, attachment.node.visibility.0, false)?;
        Ok(())
    })
}

fn write_pivots(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_particle_emitters(file: &mut File, model: &Model) -> Result<(), MdlError> {
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
    file: &mut File,
    model: &Model,
    emitter: &ParticleEmitter,
) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        write_node(out, model, &emitter.node)?;
        out.write_f32::<LittleEndian>(emitter.emission_rate)?;
        out.write_f32::<LittleEndian>(emitter.gravity)?;
        out.write_f32::<LittleEndian>(emitter.longitude)?;
        out.write_f32::<LittleEndian>(emitter.latitude)?;
        write_padded(out, &emitter.path, 0x100)?;
        out.write_u32::<LittleEndian>(0)?;
        out.write_f32::<LittleEndian>(emitter.life_span)?;
        out.write_f32::<LittleEndian>(emitter.init_velocity)?;
        write_controller(out, model, TAG_KPEV, emitter.node.visibility.0, false)?;
        Ok(())
    })
}

fn write_particle_emitters_2(file: &mut File, model: &Model) -> Result<(), MdlError> {
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
    file: &mut File,
    model: &Model,
    emitter: &ParticleEmitter2,
) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        write_node(out, model, &emitter.node)?;
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
            crate::parser::io::TAG_KP2V,
            emitter.node.visibility.0,
            false,
        )?;
        Ok(())
    })
}

fn write_ribbons(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_ribbon(file: &mut File, model: &Model, ribbon: &RibbonEmitter) -> Result<(), MdlError> {
    write_inclusive(file, |out| {
        write_node(out, model, &ribbon.node)?;
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
        write_controller(out, model, TAG_KRVS, ribbon.node.visibility.0, false)?;
        Ok(())
    })
}

fn write_cameras(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_camera(file: &mut File, model: &Model, camera: &Camera) -> Result<(), MdlError> {
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

fn write_events(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_event(file: &mut File, model: &Model, event: &EventObject) -> Result<(), MdlError> {
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

fn write_collisions(file: &mut File, model: &Model) -> Result<(), MdlError> {
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

fn write_collision(file: &mut File, model: &Model, shape: &CollisionShape) -> Result<(), MdlError> {
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
