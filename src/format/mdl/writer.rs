use crate::error::MdlError;
use crate::material::{FilterMode, ShadingFlags};
use crate::model::ids::{Extent, TrackId};
use crate::model::model::Model;
use crate::model::node::NodeRef;
use crate::model::objects::{CollisionType, LightType};
use crate::model::skeleton::{AnimationController, Helper};
use std::fmt::Write as FmtWrite;
use std::fs::File;
use std::io::Write;
use std::path::Path;

const SUPPORTED_VERSION: u32 = 800;

pub fn save_path(path: impl AsRef<Path>, model: &Model) -> Result<(), MdlError> {
    let mut file = File::create(path)?;
    save(&mut file, model)
}

pub fn save(writer: &mut impl Write, model: &Model) -> Result<(), MdlError> {
    writer.write_all(to_string(model)?.as_bytes())?;
    Ok(())
}

pub fn to_string(model: &Model) -> Result<String, MdlError> {
    if !model.unknown_chunks.is_empty() {
        return Err(MdlError::new("mdl-unknown-chunks-not-representable")
            .with_arg("count", model.unknown_chunks.len()));
    }
    let mut out = String::new();
    line(&mut out, 0, "// Written by mdlvis-rs.");
    open(&mut out, 0, "Version");
    field(&mut out, 1, "FormatVersion", SUPPORTED_VERSION);
    close(&mut out, 0, false);
    write_model(&mut out, model);
    write_sequences(&mut out, model);
    write_global_sequences(&mut out, model);
    write_textures(&mut out, model);
    write_materials(&mut out, model)?;
    write_texture_anims(&mut out, model)?;
    write_geosets(&mut out, model);
    write_geoset_anims(&mut out, model)?;
    write_bones(&mut out, model)?;
    write_helpers(&mut out, model)?;
    write_lights(&mut out, model)?;
    write_attachments(&mut out, model)?;
    write_pivots(&mut out, model);
    write_particle_emitters(&mut out, model)?;
    write_particle_emitters_2(&mut out, model)?;
    write_ribbons(&mut out, model)?;
    write_cameras(&mut out, model)?;
    write_events(&mut out, model)?;
    write_collisions(&mut out, model)?;
    Ok(out)
}

fn write_model(out: &mut String, model: &Model) {
    open(out, 0, &format!("Model \"{}\"", escaped(&model.name)));
    field(out, 1, "NumGeosets", model.geosets.len());
    field(out, 1, "NumGeosetAnims", model.geoset_anims.len());
    field(out, 1, "NumHelpers", model.helpers.len());
    field(out, 1, "NumLights", model.lights.len());
    field(out, 1, "NumBones", model.bones.len());
    field(out, 1, "NumAttachments", model.attachments.len());
    field(out, 1, "NumParticleEmitters", model.particle_emitters.len());
    field(
        out,
        1,
        "NumParticleEmitters2",
        model.particle_emitters_2.len(),
    );
    field(out, 1, "NumRibbonEmitters", model.ribbons.len());
    field(out, 1, "NumEvents", model.events.len());
    field(out, 1, "BlendTime", model.blend_time);
    write_extent(out, 1, model.extent);
    close(out, 0, false);
}

fn write_sequences(out: &mut String, model: &Model) {
    if model.sequences.is_empty() {
        return;
    }
    open(out, 0, &format!("Sequences {}", model.sequences.len()));
    for sequence in &model.sequences {
        open(out, 1, &format!("Anim \"{}\"", escaped(&sequence.name)));
        vector(
            out,
            2,
            "Interval",
            &[sequence.start_frame, sequence.end_frame],
        );
        if sequence.non_looping {
            flag(out, 2, "NonLooping");
        }
        if let Some(rarity) = sequence.rarity {
            field(out, 2, "Rarity", rarity);
        }
        if sequence.move_speed != 0.0 {
            field(out, 2, "MoveSpeed", sequence.move_speed);
        }
        write_extent(out, 2, sequence.extent);
        close(out, 1, false);
    }
    close(out, 0, false);
}

fn write_global_sequences(out: &mut String, model: &Model) {
    if model.global_sequences.is_empty() {
        return;
    }
    open(
        out,
        0,
        &format!("GlobalSequences {}", model.global_sequences.len()),
    );
    for sequence in &model.global_sequences {
        field(out, 1, "Duration", sequence.duration);
    }
    close(out, 0, false);
}

fn write_textures(out: &mut String, model: &Model) {
    if model.textures.is_empty() {
        return;
    }
    open(out, 0, &format!("Textures {}", model.textures.len()));
    for texture in &model.textures {
        open(out, 1, "Bitmap");
        quoted_field(out, 2, "Image", &texture.filename);
        if texture.replaceable_id != 0 {
            field(out, 2, "ReplaceableId", texture.replaceable_id);
        }
        if texture.flags.wrap_width {
            flag(out, 2, "WrapWidth");
        }
        if texture.flags.wrap_height {
            flag(out, 2, "WrapHeight");
        }
        close(out, 1, false);
    }
    close(out, 0, false);
}

fn write_materials(out: &mut String, model: &Model) -> Result<(), MdlError> {
    if model.materials.is_empty() {
        return Ok(());
    }
    open(out, 0, &format!("Materials {}", model.materials.len()));
    for material in &model.materials {
        open(out, 1, "Material");
        if material.flags.constant_color {
            flag(out, 2, "ConstantColor");
        }
        if material.flags.sort_primitives_far_z {
            flag(out, 2, "SortPrimsFarZ");
        }
        if material.flags.full_resolution {
            flag(out, 2, "FullResolution");
        }
        if material.priority_plane != 0 {
            field(out, 2, "PriorityPlane", material.priority_plane);
        }
        for layer in &material.layers {
            open(out, 2, "Layer");
            field(out, 3, "FilterMode", filter_name(&layer.filter_mode));
            for shading in &layer.shading_flags {
                flag(out, 3, shading_name(*shading));
            }
            if layer.texture_id_track >= 0 {
                write_track(out, 3, "TextureID", layer.texture_id_track, 1, model)?;
            } else if let Some(texture_id) = layer.texture_id {
                static_field(out, 3, "TextureID", texture_id);
            }
            if let Some(id) = layer.extra.texture_anim_id {
                field(out, 3, "TVertexAnimId", id.0);
            }
            if layer.extra.coord_id != 0 {
                field(out, 3, "CoordId", layer.extra.coord_id);
            }
            if layer.alpha_track >= 0 {
                write_track(out, 3, "Alpha", layer.alpha_track, 1, model)?;
            } else {
                static_field(out, 3, "Alpha", layer.alpha);
            }
            close(out, 2, false);
        }
        close(out, 1, false);
    }
    close(out, 0, false);
    Ok(())
}

fn write_texture_anims(out: &mut String, model: &Model) -> Result<(), MdlError> {
    if model.texture_anims.is_empty() {
        return Ok(());
    }
    open(
        out,
        0,
        &format!("TextureAnims {}", model.texture_anims.len()),
    );
    for anim in &model.texture_anims {
        open(out, 1, "TVertexAnim");
        write_track_id(out, 2, "Translation", anim.translation, 3, model)?;
        write_track_id(out, 2, "Rotation", anim.rotation, 4, model)?;
        write_track_id(out, 2, "Scaling", anim.scaling, 3, model)?;
        close(out, 1, false);
    }
    close(out, 0, false);
    Ok(())
}

fn write_geosets(out: &mut String, model: &Model) {
    for geoset in &model.geosets {
        open(out, 0, "Geoset");
        write_vectors(
            out,
            1,
            "Vertices",
            geoset.vertices.iter().map(|item| &item.position),
        );
        write_vectors(
            out,
            1,
            "Normals",
            geoset.normals.iter().map(|item| &item.normal),
        );
        write_vectors(
            out,
            1,
            "TVertices",
            geoset.tex_coords.iter().map(|item| &item.uv),
        );
        open(out, 1, "VertexGroup");
        for group in &geoset.vertex_groups {
            line(out, 2, &format!("{group},"));
        }
        close(out, 1, false);
        let index_count = geoset.faces.len() * 3;
        open(out, 1, &format!("Faces 1 {index_count}"));
        open(out, 2, "Triangles");
        let indices = geoset
            .faces
            .iter()
            .flat_map(|face| face.vertices)
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        line(out, 3, &format!("{{ {indices} }},"));
        close(out, 2, false);
        close(out, 1, false);
        let matrix_count: usize = geoset.matrix_groups.iter().map(Vec::len).sum();
        open(
            out,
            1,
            &format!("Groups {} {matrix_count}", geoset.matrix_groups.len()),
        );
        for group in &geoset.matrix_groups {
            vector(out, 2, "Matrices", group);
        }
        close(out, 1, false);
        if let Some(material_id) = geoset.material_id {
            field(out, 1, "MaterialID", material_id);
        }
        field(out, 1, "SelectionGroup", geoset.selection_group);
        if geoset.unselectable {
            flag(out, 1, "Unselectable");
        }
        write_extent(
            out,
            1,
            Extent {
                bounds_radius: geoset.bounds_radius,
                minimum: geoset.minimum_extent,
                maximum: geoset.maximum_extent,
            },
        );
        close(out, 0, false);
    }
}

fn write_geoset_anims(out: &mut String, model: &Model) -> Result<(), MdlError> {
    for anim in &model.geoset_anims {
        open(out, 0, "GeosetAnim");
        if anim.drop_shadow {
            flag(out, 1, "DropShadow");
        }
        if anim.alpha_track.is_none() {
            static_field(out, 1, "Alpha", anim.alpha);
        } else {
            write_track_id(out, 1, "Alpha", anim.alpha_track, 1, model)?;
        }
        if anim.color_track.is_none() {
            static_vector(out, 1, "Color", &anim.color);
        } else {
            write_track_id(out, 1, "Color", anim.color_track, 3, model)?;
        }
        if let Some(id) = anim.geoset_id {
            field(out, 1, "GeosetId", id.0);
        }
        close(out, 0, false);
    }
    Ok(())
}

fn write_bones(out: &mut String, model: &Model) -> Result<(), MdlError> {
    for bone in &model.bones {
        open(out, 0, &format!("Bone \"{}\"", escaped(&bone.name)));
        write_node(
            out,
            1,
            &NodeRef {
                name: bone.name.clone(),
                object_id: crate::model::ids::ObjectId(bone.object_id),
                parent_id: crate::model::ids::ParentId(bone.parent_id),
                flags: crate::model::node::NodeFlags::from_bits(bone.flags),
                translation: TrackId(bone.translation_idx),
                rotation: TrackId(bone.rotation_idx),
                scaling: TrackId(bone.scaling_idx),
                visibility: TrackId(bone.visibility_idx),
            },
            model,
        )?;
        optional_id(out, 1, "GeosetId", bone.geoset_id);
        optional_id(out, 1, "GeosetAnimId", bone.geoset_anim_id);
        close(out, 0, false);
    }
    Ok(())
}

fn write_helpers(out: &mut String, model: &Model) -> Result<(), MdlError> {
    for helper in &model.helpers {
        open(out, 0, &format!("Helper \"{}\"", escaped(&helper.name)));
        write_node(out, 1, &helper_node(helper), model)?;
        close(out, 0, false);
    }
    Ok(())
}

fn write_lights(out: &mut String, model: &Model) -> Result<(), MdlError> {
    for light in &model.lights {
        open(out, 0, &format!("Light \"{}\"", escaped(&light.node.name)));
        write_node(out, 1, &light.node, model)?;
        flag(
            out,
            1,
            match light.light_type {
                LightType::Omnidirectional => "Omnidirectional",
                LightType::Directional => "Directional",
                LightType::Ambient => "Ambient",
            },
        );
        write_static_or_track(
            out,
            1,
            "AttenuationStart",
            light.attenuation_start,
            light.attenuation_start_track,
            1,
            model,
        )?;
        write_static_or_track(
            out,
            1,
            "AttenuationEnd",
            light.attenuation_end,
            light.attenuation_end_track,
            1,
            model,
        )?;
        write_static_vector_or_track(out, 1, "Color", &light.color, light.color_track, model)?;
        write_static_or_track(
            out,
            1,
            "Intensity",
            light.intensity,
            light.intensity_track,
            1,
            model,
        )?;
        write_static_vector_or_track(
            out,
            1,
            "AmbColor",
            &light.ambient_color,
            light.ambient_color_track,
            model,
        )?;
        write_static_or_track(
            out,
            1,
            "AmbIntensity",
            light.ambient_intensity,
            light.ambient_intensity_track,
            1,
            model,
        )?;
        close(out, 0, false);
    }
    Ok(())
}

fn write_attachments(out: &mut String, model: &Model) -> Result<(), MdlError> {
    for attachment in &model.attachments {
        open(
            out,
            0,
            &format!("Attachment \"{}\"", escaped(&attachment.node.name)),
        );
        write_node(out, 1, &attachment.node, model)?;
        if !attachment.path.is_empty() {
            quoted_field(out, 1, "Path", &attachment.path);
        }
        field(out, 1, "AttachmentID", attachment.attachment_id);
        close(out, 0, false);
    }
    Ok(())
}

fn write_pivots(out: &mut String, model: &Model) {
    if model.pivot_points.is_empty() {
        return;
    }
    open(out, 0, &format!("PivotPoints {}", model.pivot_points.len()));
    for pivot in &model.pivot_points {
        vector_value(out, 1, pivot);
    }
    close(out, 0, false);
}

fn write_particle_emitters(out: &mut String, model: &Model) -> Result<(), MdlError> {
    for emitter in &model.particle_emitters {
        open(
            out,
            0,
            &format!("ParticleEmitter \"{}\"", escaped(&emitter.node.name)),
        );
        write_node(out, 1, &emitter.node, model)?;
        static_field(out, 1, "EmissionRate", emitter.emission_rate);
        static_field(out, 1, "Gravity", emitter.gravity);
        static_field(out, 1, "Longitude", emitter.longitude);
        static_field(out, 1, "Latitude", emitter.latitude);
        static_field(out, 1, "LifeSpan", emitter.life_span);
        static_field(out, 1, "InitVelocity", emitter.init_velocity);
        quoted_field(out, 1, "Path", &emitter.path);
        close(out, 0, false);
    }
    Ok(())
}

fn write_particle_emitters_2(out: &mut String, model: &Model) -> Result<(), MdlError> {
    for emitter in &model.particle_emitters_2 {
        open(
            out,
            0,
            &format!("ParticleEmitter2 \"{}\"", escaped(&emitter.node.name)),
        );
        write_node(out, 1, &emitter.node, model)?;
        flag(out, 1, particle_blend_name(emitter.blend_mode)?);
        static_field(out, 1, "Speed", emitter.speed);
        static_field(out, 1, "Variation", emitter.variation);
        static_field(out, 1, "Latitude", emitter.latitude);
        static_field(out, 1, "Gravity", emitter.gravity);
        static_field(out, 1, "EmissionRate", emitter.emission_rate);
        static_field(out, 1, "Width", emitter.width);
        static_field(out, 1, "Length", emitter.length);
        open(out, 1, "SegmentColor");
        for color in &emitter.segment_color {
            vector(out, 2, "Color", color);
        }
        close(out, 1, true);
        vector(out, 1, "Alpha", &emitter.alpha);
        vector(out, 1, "ParticleScaling", &emitter.particle_scaling);
        field(out, 1, "Rows", emitter.rows);
        field(out, 1, "Columns", emitter.columns);
        if let Some(id) = emitter.texture_id {
            field(out, 1, "TextureID", id.0);
        }
        field(out, 1, "Time", emitter.time);
        field(out, 1, "LifeSpan", emitter.life_span);
        field(out, 1, "TailLength", emitter.tail_length);
        if emitter.replaceable_id != 0 {
            field(out, 1, "ReplaceableId", emitter.replaceable_id);
        }
        if emitter.priority_plane != 0 {
            field(out, 1, "PriorityPlane", emitter.priority_plane);
        }
        for (enabled, name) in [
            (emitter.flags.sort_primitives_far_z, "SortPrimsFarZ"),
            (emitter.flags.unshaded, "Unshaded"),
            (emitter.flags.line_emitter, "LineEmitter"),
            (emitter.flags.unfogged, "Unfogged"),
            (emitter.flags.model_space, "ModelSpace"),
            (emitter.flags.xy_quad, "XYQuad"),
            (emitter.squirt, "Squirt"),
        ] {
            if enabled {
                flag(out, 1, name);
            }
        }
        flag(
            out,
            1,
            match emitter.particle_type {
                0 => "Head",
                1 => "Tail",
                _ => "Both",
            },
        );
        close(out, 0, false);
    }
    Ok(())
}

fn write_ribbons(out: &mut String, model: &Model) -> Result<(), MdlError> {
    for ribbon in &model.ribbons {
        open(
            out,
            0,
            &format!("RibbonEmitter \"{}\"", escaped(&ribbon.node.name)),
        );
        write_node(out, 1, &ribbon.node, model)?;
        static_field(out, 1, "HeightAbove", ribbon.height_above);
        static_field(out, 1, "HeightBelow", ribbon.height_below);
        static_field(out, 1, "Alpha", ribbon.alpha);
        static_vector(out, 1, "Color", &ribbon.color);
        static_field(out, 1, "TextureSlot", ribbon.texture_slot);
        field(out, 1, "EmissionRate", ribbon.emission_rate);
        field(out, 1, "LifeSpan", ribbon.life_span);
        field(out, 1, "Gravity", ribbon.gravity);
        field(out, 1, "Rows", ribbon.rows);
        field(out, 1, "Columns", ribbon.columns);
        if let Some(id) = ribbon.material_id {
            field(out, 1, "MaterialID", id.0);
        }
        close(out, 0, false);
    }
    Ok(())
}

fn write_cameras(out: &mut String, model: &Model) -> Result<(), MdlError> {
    for camera in &model.cameras {
        open(out, 0, &format!("Camera \"{}\"", escaped(&camera.name)));
        vector(out, 1, "Position", &camera.position);
        write_track_id(out, 1, "Translation", camera.translation, 3, model)?;
        write_track_id(out, 1, "Rotation", camera.rotation, 1, model)?;
        field(out, 1, "FieldOfView", camera.field_of_view);
        field(out, 1, "FarClip", camera.far_clip);
        field(out, 1, "NearClip", camera.near_clip);
        open(out, 1, "Target");
        vector(out, 2, "Position", &camera.target_position);
        write_track_id(out, 2, "Translation", camera.target_translation, 3, model)?;
        close(out, 1, false);
        close(out, 0, false);
    }
    Ok(())
}

fn write_events(out: &mut String, model: &Model) -> Result<(), MdlError> {
    for event in &model.events {
        open(
            out,
            0,
            &format!("EventObject \"{}\"", escaped(&event.node.name)),
        );
        write_node(out, 1, &event.node, model)?;
        open(out, 1, &format!("EventTrack {}", event.tracks.len()));
        if !event.global_seq_id.is_none() {
            field(out, 2, "GlobalSeqId", event.global_seq_id.0);
        }
        for frame in &event.tracks {
            line(out, 2, &format!("{frame},"));
        }
        close(out, 1, false);
        close(out, 0, false);
    }
    Ok(())
}

fn write_collisions(out: &mut String, model: &Model) -> Result<(), MdlError> {
    for shape in &model.collisions {
        open(
            out,
            0,
            &format!("CollisionShape \"{}\"", escaped(&shape.node.name)),
        );
        write_node(out, 1, &shape.node, model)?;
        flag(
            out,
            1,
            match shape.kind {
                CollisionType::Box => "Box",
                CollisionType::Sphere => "Sphere",
            },
        );
        write_vectors(out, 1, "Vertices", shape.vertices.iter());
        if matches!(shape.kind, CollisionType::Sphere) {
            field(out, 1, "BoundsRadius", shape.bounds_radius);
        }
        close(out, 0, false);
    }
    Ok(())
}

fn write_node(
    out: &mut String,
    indent: usize,
    node: &NodeRef,
    model: &Model,
) -> Result<(), MdlError> {
    field(out, indent, "ObjectId", node.object_id.0);
    if !node.parent_id.is_none() {
        field(out, indent, "Parent", node.parent_id.0);
    }
    for (enabled, name) in [
        (
            node.flags.dont_inherit_translation(),
            "DontInheritTranslation",
        ),
        (node.flags.dont_inherit_scaling(), "DontInheritScaling"),
        (node.flags.dont_inherit_rotation(), "DontInheritRotation"),
        (node.flags.billboarded(), "Billboarded"),
        (node.flags.billboard_lock_x(), "BillboardedLockX"),
        (node.flags.billboard_lock_y(), "BillboardedLockY"),
        (node.flags.billboard_lock_z(), "BillboardedLockZ"),
        (node.flags.camera_anchored(), "CameraAnchored"),
    ] {
        if enabled {
            flag(out, indent, name);
        }
    }
    write_track_id(out, indent, "Translation", node.translation, 3, model)?;
    write_track_id(out, indent, "Rotation", node.rotation, 4, model)?;
    write_track_id(out, indent, "Scaling", node.scaling, 3, model)?;
    write_track_id(out, indent, "Visibility", node.visibility, 1, model)?;
    Ok(())
}

fn write_track_id(
    out: &mut String,
    indent: usize,
    name: &str,
    id: TrackId,
    elements: usize,
    model: &Model,
) -> Result<(), MdlError> {
    if id.is_none() {
        return Ok(());
    }
    write_track(out, indent, name, id.0, elements, model)
}

fn write_track(
    out: &mut String,
    indent: usize,
    name: &str,
    index: i32,
    elements: usize,
    model: &Model,
) -> Result<(), MdlError> {
    let controller = model.controllers.get(index as usize).ok_or_else(|| {
        MdlError::new("mdl-invalid-controller-index")
            .with_arg("track", name)
            .with_arg("index", index)
    })?;
    validate_controller(name, elements, controller)?;
    open(
        out,
        indent,
        &format!("{name} {}", controller.keyframes.len()),
    );
    flag(
        out,
        indent + 1,
        match controller.interpolation_type {
            0 => "DontInterp",
            1 => "Linear",
            2 => "Hermite",
            3 => "Bezier",
            value => {
                return Err(MdlError::new("mdl-invalid-interpolation")
                    .with_arg("track", name)
                    .with_arg("value", value));
            }
        },
    );
    if controller.global_seq_id >= 0 {
        field(out, indent + 1, "GlobalSeqId", controller.global_seq_id);
    }
    for keyframe in &controller.keyframes {
        track_value(out, indent + 1, keyframe.frame, &keyframe.data);
        if controller.interpolation_type == 2 || controller.interpolation_type == 3 {
            vector(out, indent + 2, "InTan", &keyframe.in_tan);
            vector(out, indent + 2, "OutTan", &keyframe.out_tan);
        }
    }
    close(out, indent, false);
    Ok(())
}

fn validate_controller(
    name: &str,
    elements: usize,
    controller: &AnimationController,
) -> Result<(), MdlError> {
    let tangents = controller.interpolation_type == 2 || controller.interpolation_type == 3;
    for keyframe in &controller.keyframes {
        if keyframe.data.len() != elements {
            return Err(MdlError::new("mdl-invalid-track-width")
                .with_arg("track", name)
                .with_arg("expected", elements)
                .with_arg("actual", keyframe.data.len()));
        }
        if tangents && (keyframe.in_tan.len() != elements || keyframe.out_tan.len() != elements) {
            return Err(MdlError::new("mdl-invalid-tangent-size")
                .with_arg("track", name)
                .with_arg("frame", keyframe.frame));
        }
    }
    Ok(())
}

fn write_static_or_track(
    out: &mut String,
    indent: usize,
    name: &str,
    value: f32,
    track: TrackId,
    elements: usize,
    model: &Model,
) -> Result<(), MdlError> {
    if track.is_none() {
        static_field(out, indent, name, value);
        Ok(())
    } else {
        write_track_id(out, indent, name, track, elements, model)
    }
}

fn write_static_vector_or_track<const N: usize>(
    out: &mut String,
    indent: usize,
    name: &str,
    value: &[f32; N],
    track: TrackId,
    model: &Model,
) -> Result<(), MdlError> {
    if track.is_none() {
        static_vector(out, indent, name, value);
        Ok(())
    } else {
        write_track_id(out, indent, name, track, N, model)
    }
}

fn helper_node(helper: &Helper) -> NodeRef {
    NodeRef {
        name: helper.name.clone(),
        object_id: crate::model::ids::ObjectId(helper.object_id),
        parent_id: crate::model::ids::ParentId(helper.parent_id),
        flags: crate::model::node::NodeFlags::from_bits(helper.flags),
        translation: TrackId(helper.translation_idx),
        rotation: TrackId(helper.rotation_idx),
        scaling: TrackId(helper.scaling_idx),
        visibility: TrackId(helper.visibility_idx),
    }
}

fn optional_id(out: &mut String, indent: usize, name: &str, value: Option<u32>) {
    match value {
        Some(value) => field(out, indent, name, value),
        None => field(out, indent, name, "None"),
    }
}

fn particle_blend_name(value: u32) -> Result<&'static str, MdlError> {
    match value {
        0 => Ok("Blend"),
        1 => Ok("Additive"),
        2 => Ok("Modulate"),
        3 => Ok("Modulate2x"),
        4 => Ok("AlphaKey"),
        _ => Err(MdlError::new("mdl-invalid-particle-blend-mode").with_arg("value", value)),
    }
}

fn filter_name(mode: &FilterMode) -> &'static str {
    match mode {
        FilterMode::None => "None",
        FilterMode::Transparent => "Transparent",
        FilterMode::Blend => "Blend",
        FilterMode::Additive => "Additive",
        FilterMode::AddAlpha => "AddAlpha",
        FilterMode::Modulate => "Modulate",
        FilterMode::Modulate2x => "Modulate2x",
    }
}

fn shading_name(flag: ShadingFlags) -> &'static str {
    match flag {
        ShadingFlags::Unshaded => "Unshaded",
        ShadingFlags::SphereEnvMap => "SphereEnvMap",
        ShadingFlags::TwoSided => "TwoSided",
        ShadingFlags::Unfogged => "Unfogged",
        ShadingFlags::NoDepthTest => "NoDepthTest",
        ShadingFlags::NoDepthSet => "NoDepthSet",
    }
}

fn write_extent(out: &mut String, indent: usize, extent: Extent) {
    vector(out, indent, "MinimumExtent", &extent.minimum);
    vector(out, indent, "MaximumExtent", &extent.maximum);
    field(out, indent, "BoundsRadius", extent.bounds_radius);
}

fn write_vectors<'a, T, const N: usize>(out: &mut String, indent: usize, name: &str, values: T)
where
    T: IntoIterator<Item = &'a [f32; N]>,
{
    let values: Vec<&[f32; N]> = values.into_iter().collect();
    open(out, indent, &format!("{name} {}", values.len()));
    for value in values {
        vector_value(out, indent + 1, value);
    }
    close(out, indent, false);
}

fn track_value(out: &mut String, indent: usize, frame: i32, values: &[f32]) {
    tabs(out, indent);
    let _ = write!(out, "{frame}: ");
    if values.len() == 1 {
        let _ = writeln!(out, "{},", values[0]);
    } else {
        let _ = writeln!(out, "{{ {} }},", joined(values));
    }
}

fn vector<T: std::fmt::Display>(out: &mut String, indent: usize, name: &str, values: &[T]) {
    tabs(out, indent);
    let _ = writeln!(out, "{name} {{ {} }},", joined(values));
}

fn static_vector<T: std::fmt::Display>(out: &mut String, indent: usize, name: &str, values: &[T]) {
    tabs(out, indent);
    let _ = writeln!(out, "static {name} {{ {} }},", joined(values));
}

fn vector_value<T: std::fmt::Display>(out: &mut String, indent: usize, values: &[T]) {
    tabs(out, indent);
    let _ = writeln!(out, "{{ {} }},", joined(values));
}

fn joined<T: std::fmt::Display>(values: &[T]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn open(out: &mut String, indent: usize, header: &str) {
    tabs(out, indent);
    let _ = writeln!(out, "{header} {{");
}

fn close(out: &mut String, indent: usize, comma: bool) {
    tabs(out, indent);
    let _ = writeln!(out, "}}{}", if comma { "," } else { "" });
}

fn field(out: &mut String, indent: usize, name: &str, value: impl std::fmt::Display) {
    tabs(out, indent);
    let _ = writeln!(out, "{name} {value},");
}

fn static_field(out: &mut String, indent: usize, name: &str, value: impl std::fmt::Display) {
    tabs(out, indent);
    let _ = writeln!(out, "static {name} {value},");
}

fn quoted_field(out: &mut String, indent: usize, name: &str, value: &str) {
    tabs(out, indent);
    let _ = writeln!(out, "{name} \"{}\",", escaped(value));
}

fn flag(out: &mut String, indent: usize, name: &str) {
    tabs(out, indent);
    let _ = writeln!(out, "{name},");
}

fn line(out: &mut String, indent: usize, value: &str) {
    tabs(out, indent);
    let _ = writeln!(out, "{value}");
}

fn tabs(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push('\t');
    }
}

fn escaped(value: &str) -> String {
    value.replace('"', "\\\"")
}
