use super::syntax::{
    Node, NodeKind, after, block, contains_word, error, named_block, number, parse,
    repeated_blocks, string, vector, word,
};
use crate::error::MdlError;
use crate::material::{FilterMode, Layer, Material, ShadingFlags};
use crate::model::animation::Sequence;
use crate::model::geoset::{Face, Geoset, Normal, TexCoord, Vertex};
use crate::model::ids::{
    Extent, GeosetIndex, GlobalSeqId, MaterialIndex, ObjectId, ParentId, TextureAnimIndex,
    TextureIndex, TrackId,
};
use crate::model::model::Model;
use crate::model::node::{
    NodeFlags, NodeRef, TYPE_ATCH, TYPE_BONE, TYPE_CLID, TYPE_EVTS, TYPE_HELP, TYPE_LITE, TYPE_PRE2,
};
use crate::model::objects::{
    Attachment, Camera, CollisionShape, CollisionType, EventObject, GeosetAnim, GlobalSequence,
    LayerRef, Light, LightType, MaterialFlags, ParticleEmitter, ParticleEmitter2,
    ParticleEmitter2Flags, RibbonEmitter, TextureAnim, TextureFlags,
};
use crate::model::skeleton::{AnimationController, Bone, Helper, Keyframe};
use crate::model::texture::Texture;
use std::fs::File;
use std::io::Read;
use std::path::Path;

const SUPPORTED_VERSION: u32 = 800;

pub fn load_path(path: impl AsRef<Path>) -> Result<Model, MdlError> {
    let mut file = File::open(path)?;
    load(&mut file)
}

pub fn load(reader: &mut impl Read) -> Result<Model, MdlError> {
    let mut source = String::new();
    reader.read_to_string(&mut source)?;
    parse_str(&source)
}

pub fn parse_str(source: &str) -> Result<Model, MdlError> {
    let root = parse(source.strip_prefix('\u{feff}').unwrap_or(source))?;
    let (version, _) =
        named_block(&root, "Version").ok_or_else(|| MdlError::new("mdl-missing-version"))?;
    let format_version = required_number::<u32>(version, "FormatVersion")?;
    if format_version != SUPPORTED_VERSION {
        return Err(MdlError::new("unsupported-version").with_arg("version", format_version));
    }

    let (model_body, model_index) =
        named_block(&root, "Model").ok_or_else(|| MdlError::new("mdl-missing-model"))?;
    let name = root
        .get(model_index + 1)
        .and_then(string)
        .ok_or_else(|| MdlError::new("mdl-missing-model-name"))?;
    let mut model = Model {
        name: name.to_string(),
        blend_time: optional_number(model_body, "BlendTime")?.unwrap_or(0),
        extent: parse_extent(model_body)?,
        ..Model::default()
    };

    parse_sequences(&root, &mut model)?;
    parse_global_sequences(&root, &mut model)?;
    parse_textures(&root, &mut model)?;
    parse_materials(&root, &mut model)?;
    parse_texture_anims(&root, &mut model)?;
    parse_geosets(&root, &mut model)?;
    parse_geoset_anims(&root, &mut model)?;
    parse_bones(&root, &mut model)?;
    parse_helpers(&root, &mut model)?;
    parse_lights(&root, &mut model)?;
    parse_attachments(&root, &mut model)?;
    parse_particle_emitters(&root, &mut model)?;
    parse_particle_emitters_2(&root, &mut model)?;
    parse_ribbons(&root, &mut model)?;
    parse_cameras(&root, &mut model)?;
    parse_events(&root, &mut model)?;
    parse_collisions(&root, &mut model)?;
    parse_pivots(&root, &mut model)?;
    Ok(model)
}

fn parse_sequences(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    let Some((body, _)) = named_block(root, "Sequences") else {
        return Ok(());
    };
    for (anim, label) in repeated_blocks(body, "Anim") {
        let interval = required_vector::<2>(anim, "Interval")?;
        model.sequences.push(Sequence {
            name: label.unwrap_or_default().to_string(),
            start_frame: f32_to_u32(interval[0], anim)?,
            end_frame: f32_to_u32(interval[1], anim)?,
            rarity: optional_number(anim, "Rarity")?,
            non_looping: contains_word(anim, "NonLooping"),
            move_speed: optional_number(anim, "MoveSpeed")?.unwrap_or(0.0),
            extent: parse_extent(anim)?,
        });
    }
    Ok(())
}

fn parse_global_sequences(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    let Some((body, _)) = named_block(root, "GlobalSequences") else {
        return Ok(());
    };
    for (index, node) in body.iter().enumerate() {
        if word(node) == Some("Duration") {
            model.global_sequences.push(GlobalSequence {
                duration: number(required_next(body, index, "Duration")?)?,
            });
        }
    }
    Ok(())
}

fn parse_textures(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    let Some((body, _)) = named_block(root, "Textures") else {
        return Ok(());
    };
    for (bitmap, _) in repeated_blocks(body, "Bitmap") {
        model.textures.push(Texture {
            filename: optional_string(bitmap, "Image")?.unwrap_or_default(),
            replaceable_id: optional_number(bitmap, "ReplaceableId")?.unwrap_or(0),
            flags: TextureFlags {
                wrap_width: contains_word(bitmap, "WrapWidth"),
                wrap_height: contains_word(bitmap, "WrapHeight"),
            },
            image_data: None,
            width: 0,
            height: 0,
        });
    }
    Ok(())
}

fn parse_materials(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    let Some((body, _)) = named_block(root, "Materials") else {
        return Ok(());
    };
    for (material_body, _) in repeated_blocks(body, "Material") {
        let mut material = Material {
            priority_plane: optional_number(material_body, "PriorityPlane")?.unwrap_or(0),
            flags: MaterialFlags {
                constant_color: contains_word(material_body, "ConstantColor"),
                sort_primitives_far_z: contains_word(material_body, "SortPrimsFarZ"),
                full_resolution: contains_word(material_body, "FullResolution"),
            },
            ..Material::default()
        };
        for (layer_body, _) in repeated_blocks(material_body, "Layer") {
            let filter = optional_word(layer_body, "FilterMode").unwrap_or("None");
            let alpha_track = parse_track(layer_body, "Alpha", 1, model)?;
            let texture_id_track = parse_track(layer_body, "TextureID", 1, model)?;
            material.layers.push(Layer {
                texture_id: optional_number(layer_body, "TextureID")?,
                filter_mode: parse_filter_mode(filter, layer_body)?,
                shading_flags: parse_shading_flags(layer_body),
                alpha: optional_number(layer_body, "Alpha")?.unwrap_or(1.0),
                extra: LayerRef {
                    texture_anim_id: optional_number::<u32>(layer_body, "TVertexAnimId")?
                        .map(TextureAnimIndex),
                    coord_id: optional_number(layer_body, "CoordId")?.unwrap_or(0),
                },
                alpha_track: alpha_track.0,
                texture_id_track: texture_id_track.0,
                enabled: true,
                alpha_override: None,
                filter_mode_override: None,
                shading_flags_override: None,
            });
        }
        model.materials.push(material);
    }
    Ok(())
}

fn parse_texture_anims(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    let Some((body, _)) = named_block(root, "TextureAnims") else {
        return Ok(());
    };
    for (anim, _) in repeated_blocks(body, "TVertexAnim") {
        let translation = parse_track(anim, "Translation", 3, model)?;
        let rotation = parse_track(anim, "Rotation", 4, model)?;
        let scaling = parse_track(anim, "Scaling", 3, model)?;
        model.texture_anims.push(TextureAnim {
            translation,
            rotation,
            scaling,
        });
    }
    Ok(())
}

fn parse_geosets(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    for (body, _) in repeated_blocks(root, "Geoset") {
        let mut geoset = Geoset {
            vertices: parse_vector_list::<3>(body, "Vertices")?
                .into_iter()
                .map(|position| Vertex { position })
                .collect(),
            normals: parse_vector_list::<3>(body, "Normals")?
                .into_iter()
                .map(|normal| Normal { normal })
                .collect(),
            tex_coords: parse_vector_list::<2>(body, "TVertices")?
                .into_iter()
                .map(|uv| TexCoord { uv })
                .collect(),
            vertex_groups: parse_number_list::<u8>(body, "VertexGroup")?,
            ..Geoset::default()
        };
        if let Some((faces, _)) = named_block(body, "Faces") {
            for (triangles, _) in repeated_blocks(faces, "Triangles") {
                let indices = triangles
                    .iter()
                    .find_map(block)
                    .map(number_nodes::<u32>)
                    .transpose()?
                    .unwrap_or_default();
                if indices.len() % 3 != 0 {
                    return Err(
                        MdlError::new("mdl-triangle-index-count").with_arg("actual", indices.len())
                    );
                }
                for indices in indices.chunks_exact(3) {
                    geoset.faces.push(Face {
                        vertices: [indices[0], indices[1], indices[2]],
                    });
                }
            }
        }
        if let Some((groups, _)) = named_block(body, "Groups") {
            for (matrices, _) in repeated_blocks(groups, "Matrices") {
                geoset.matrix_groups.push(number_nodes::<u32>(matrices)?);
            }
        }
        geoset.material_id = optional_number(body, "MaterialID")?;
        geoset.selection_group = optional_number(body, "SelectionGroup")?.unwrap_or(0);
        geoset.unselectable = contains_word(body, "Unselectable");
        let extent = parse_extent(body)?;
        geoset.bounds_radius = extent.bounds_radius;
        geoset.minimum_extent = extent.minimum;
        geoset.maximum_extent = extent.maximum;
        model.geosets.push(geoset);
    }
    Ok(())
}

fn parse_geoset_anims(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    for (body, _) in repeated_blocks(root, "GeosetAnim") {
        let alpha_track = parse_track(body, "Alpha", 1, model)?;
        let color_track = parse_track(body, "Color", 3, model)?;
        model.geoset_anims.push(GeosetAnim {
            geoset_id: optional_number::<u32>(body, "GeosetId")?.map(GeosetIndex),
            alpha: optional_number(body, "Alpha")?.unwrap_or(1.0),
            color: optional_vector(body, "Color")?.unwrap_or([1.0; 3]),
            drop_shadow: contains_word(body, "DropShadow"),
            alpha_track,
            color_track,
        });
    }
    Ok(())
}

fn parse_bones(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    for (body, label) in repeated_blocks(root, "Bone") {
        let node = parse_node(body, label, TYPE_BONE, model)?;
        model.bones.push(Bone {
            name: node.name,
            object_id: node.object_id.0,
            parent_id: node.parent_id.0,
            pivot_point: [0.0; 3],
            geoset_id: optional_optional_id(body, "GeosetId")?,
            geoset_anim_id: optional_optional_id(body, "GeosetAnimId")?,
            flags: node.flags.bits(),
            translation_idx: node.translation.0,
            rotation_idx: node.rotation.0,
            scaling_idx: node.scaling.0,
            visibility_idx: node.visibility.0,
        });
    }
    Ok(())
}

fn parse_helpers(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    for (body, label) in repeated_blocks(root, "Helper") {
        let node = parse_node(body, label, TYPE_HELP, model)?;
        model.helpers.push(Helper {
            name: node.name,
            object_id: node.object_id.0,
            parent_id: node.parent_id.0,
            pivot_point: [0.0; 3],
            flags: node.flags.bits(),
            translation_idx: node.translation.0,
            rotation_idx: node.rotation.0,
            scaling_idx: node.scaling.0,
            visibility_idx: node.visibility.0,
        });
    }
    Ok(())
}

fn parse_lights(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    for (body, label) in repeated_blocks(root, "Light") {
        let node = parse_node(body, label, TYPE_LITE, model)?;
        let light_type = if contains_word(body, "Directional") {
            LightType::Directional
        } else if contains_word(body, "Ambient") {
            LightType::Ambient
        } else {
            LightType::Omnidirectional
        };
        let attenuation_start_track = parse_track(body, "AttenuationStart", 1, model)?;
        let attenuation_end_track = parse_track(body, "AttenuationEnd", 1, model)?;
        let intensity_track = parse_track(body, "Intensity", 1, model)?;
        let color_track = parse_track(body, "Color", 3, model)?;
        let ambient_color_track = parse_track(body, "AmbColor", 3, model)?;
        let ambient_intensity_track = parse_track(body, "AmbIntensity", 1, model)?;
        model.lights.push(Light {
            node,
            light_type,
            attenuation_start: optional_number(body, "AttenuationStart")?.unwrap_or(0.0),
            attenuation_end: optional_number(body, "AttenuationEnd")?.unwrap_or(0.0),
            color: optional_vector(body, "Color")?.unwrap_or([1.0; 3]),
            intensity: optional_number(body, "Intensity")?.unwrap_or(0.0),
            ambient_color: optional_vector(body, "AmbColor")?.unwrap_or([0.0; 3]),
            ambient_intensity: optional_number(body, "AmbIntensity")?.unwrap_or(0.0),
            attenuation_start_track,
            attenuation_end_track,
            intensity_track,
            color_track,
            ambient_color_track,
            ambient_intensity_track,
        });
    }
    Ok(())
}

fn parse_attachments(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    for (body, label) in repeated_blocks(root, "Attachment") {
        let node = parse_node(body, label, TYPE_ATCH, model)?;
        model.attachments.push(Attachment {
            node,
            path: optional_string(body, "Path")?.unwrap_or_default(),
            attachment_id: optional_number(body, "AttachmentID")?.unwrap_or(-1),
        });
    }
    Ok(())
}

fn parse_particle_emitters(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    for (body, label) in repeated_blocks(root, "ParticleEmitter") {
        let node = parse_node(body, label, TYPE_HELP, model)?;
        model.particle_emitters.push(ParticleEmitter {
            node,
            emission_rate: optional_number(body, "EmissionRate")?.unwrap_or(0.0),
            gravity: optional_number(body, "Gravity")?.unwrap_or(0.0),
            longitude: optional_number(body, "Longitude")?.unwrap_or(0.0),
            latitude: optional_number(body, "Latitude")?.unwrap_or(0.0),
            life_span: optional_number(body, "LifeSpan")?.unwrap_or(0.0),
            init_velocity: optional_number(body, "InitVelocity")?.unwrap_or(0.0),
            path: optional_string(body, "Path")?.unwrap_or_default(),
        });
    }
    Ok(())
}

fn parse_particle_emitters_2(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    for (body, label) in repeated_blocks(root, "ParticleEmitter2") {
        let node = parse_node(body, label, TYPE_PRE2, model)?;
        let colors = named_block(body, "SegmentColor")
            .map(|(nodes, _)| parse_color_list(nodes))
            .transpose()?
            .unwrap_or([[1.0; 3]; 3]);
        let alpha = optional_vector::<3>(body, "Alpha")?.unwrap_or([255.0; 3]);
        model.particle_emitters_2.push(ParticleEmitter2 {
            node,
            flags: ParticleEmitter2Flags {
                sort_primitives_far_z: contains_word(body, "SortPrimsFarZ"),
                unshaded: contains_word(body, "Unshaded"),
                line_emitter: contains_word(body, "LineEmitter"),
                unfogged: contains_word(body, "Unfogged"),
                model_space: contains_word(body, "ModelSpace"),
                xy_quad: contains_word(body, "XYQuad"),
            },
            speed: optional_number(body, "Speed")?.unwrap_or(0.0),
            speed_track: TrackId::NONE,
            variation: optional_number(body, "Variation")?.unwrap_or(0.0),
            variation_track: TrackId::NONE,
            latitude: optional_number(body, "Latitude")?.unwrap_or(0.0),
            latitude_track: TrackId::NONE,
            gravity: optional_number(body, "Gravity")?.unwrap_or(0.0),
            gravity_track: TrackId::NONE,
            life_span: optional_number(body, "LifeSpan")?.unwrap_or(0.0),
            emission_rate: optional_number(body, "EmissionRate")?.unwrap_or(0.0),
            emission_rate_track: TrackId::NONE,
            width: optional_number(body, "Width")?.unwrap_or(0.0),
            width_track: TrackId::NONE,
            length: optional_number(body, "Length")?.unwrap_or(0.0),
            length_track: TrackId::NONE,
            squirt: contains_word(body, "Squirt"),
            blend_mode: particle_blend_mode(body),
            rows: optional_number(body, "Rows")?.unwrap_or(1),
            columns: optional_number(body, "Columns")?.unwrap_or(1),
            particle_type: if contains_word(body, "Both") {
                2
            } else if contains_word(body, "Tail") {
                1
            } else {
                0
            },
            tail_length: optional_number(body, "TailLength")?.unwrap_or(0.0),
            time: optional_number(body, "Time")?.unwrap_or(0.0),
            segment_color: colors,
            alpha: alpha.map(|value| value.clamp(0.0, 255.0) as u8),
            particle_scaling: optional_vector(body, "ParticleScaling")?.unwrap_or([1.0; 3]),
            texture_id: optional_number::<u32>(body, "TextureID")?.map(TextureIndex),
            replaceable_id: optional_number(body, "ReplaceableId")?.unwrap_or(0),
            priority_plane: optional_number(body, "PriorityPlane")?.unwrap_or(0),
        });
    }
    Ok(())
}

fn parse_ribbons(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    for (body, label) in repeated_blocks(root, "RibbonEmitter") {
        let node = parse_node(body, label, TYPE_HELP, model)?;
        model.ribbons.push(RibbonEmitter {
            node,
            height_above: optional_number(body, "HeightAbove")?.unwrap_or(0.0),
            height_below: optional_number(body, "HeightBelow")?.unwrap_or(0.0),
            alpha: optional_number(body, "Alpha")?.unwrap_or(1.0),
            color: optional_vector(body, "Color")?.unwrap_or([1.0; 3]),
            texture_slot: optional_number(body, "TextureSlot")?.unwrap_or(0),
            emission_rate: optional_number(body, "EmissionRate")?.unwrap_or(0),
            life_span: optional_number(body, "LifeSpan")?.unwrap_or(0.0),
            gravity: optional_number(body, "Gravity")?.unwrap_or(0.0),
            rows: optional_number(body, "Rows")?.unwrap_or(1),
            columns: optional_number(body, "Columns")?.unwrap_or(1),
            material_id: optional_number::<u32>(body, "MaterialID")?.map(MaterialIndex),
        });
    }
    Ok(())
}

fn parse_cameras(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    for (body, label) in repeated_blocks(root, "Camera") {
        let target = named_block(body, "Target").map(|(nodes, _)| nodes);
        let translation = parse_track(body, "Translation", 3, model)?;
        let rotation = parse_track(body, "Rotation", 1, model)?;
        let target_translation = if let Some(nodes) = target {
            parse_track(nodes, "Translation", 3, model)?
        } else {
            TrackId::NONE
        };
        model.cameras.push(Camera {
            name: label.unwrap_or_default().to_string(),
            position: optional_vector(body, "Position")?.unwrap_or([0.0; 3]),
            field_of_view: optional_number(body, "FieldOfView")?.unwrap_or(0.0),
            far_clip: optional_number(body, "FarClip")?.unwrap_or(0.0),
            near_clip: optional_number(body, "NearClip")?.unwrap_or(0.0),
            target_position: target
                .map(|nodes| optional_vector(nodes, "Position"))
                .transpose()?
                .flatten()
                .unwrap_or([0.0; 3]),
            translation,
            rotation,
            target_translation,
        });
    }
    Ok(())
}

fn parse_events(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    for (body, label) in repeated_blocks(root, "EventObject") {
        let node = parse_node(body, label, TYPE_EVTS, model)?;
        let event_track = named_block(body, "EventTrack").map(|(nodes, _)| nodes);
        let tracks = event_track
            .map(event_frames)
            .transpose()?
            .unwrap_or_default();
        model.events.push(EventObject {
            node,
            global_seq_id: GlobalSeqId(
                event_track
                    .map(|nodes| optional_number(nodes, "GlobalSeqId"))
                    .transpose()?
                    .flatten()
                    .unwrap_or(-1),
            ),
            tracks,
        });
    }
    Ok(())
}

fn event_frames(nodes: &[Node]) -> Result<Vec<i32>, MdlError> {
    nodes
        .iter()
        .enumerate()
        .filter(|(index, node)| {
            matches!(node.kind, NodeKind::Number(_))
                && (index == &0 || word(&nodes[*index - 1]) != Some("GlobalSeqId"))
        })
        .map(|(_, node)| number(node))
        .collect()
}

fn parse_collisions(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    for (body, label) in repeated_blocks(root, "CollisionShape") {
        let node = parse_node(body, label, TYPE_CLID, model)?;
        model.collisions.push(CollisionShape {
            node,
            kind: if contains_word(body, "Sphere") {
                CollisionType::Sphere
            } else {
                CollisionType::Box
            },
            vertices: parse_vector_list(body, "Vertices")?,
            bounds_radius: optional_number(body, "BoundsRadius")?.unwrap_or(0.0),
        });
    }
    Ok(())
}

fn parse_pivots(root: &[Node], model: &mut Model) -> Result<(), MdlError> {
    let Some((body, _)) = named_block(root, "PivotPoints") else {
        return Ok(());
    };
    model.pivot_points = vector_nodes(body)?;
    for bone in &mut model.bones {
        bone.pivot_point = model
            .pivot_points
            .get(bone.object_id as usize)
            .copied()
            .unwrap_or([0.0; 3]);
    }
    for helper in &mut model.helpers {
        helper.pivot_point = model
            .pivot_points
            .get(helper.object_id as usize)
            .copied()
            .unwrap_or([0.0; 3]);
    }
    Ok(())
}

fn parse_node(
    body: &[Node],
    label: Option<&str>,
    type_bits: u32,
    model: &mut Model,
) -> Result<NodeRef, MdlError> {
    let mut flags = type_bits;
    for (name, bit) in [
        ("DontInheritTranslation", 1),
        ("DontInheritScaling", 2),
        ("DontInheritRotation", 4),
        ("Billboarded", 8),
        ("BillboardedLockX", 16),
        ("BillboardedLockY", 32),
        ("BillboardedLockZ", 64),
        ("CameraAnchored", 128),
    ] {
        if contains_word(body, name) {
            flags |= bit;
        }
    }
    Ok(NodeRef {
        name: label.unwrap_or_default().to_string(),
        object_id: ObjectId(optional_number(body, "ObjectId")?.unwrap_or(0)),
        parent_id: ParentId(optional_number(body, "Parent")?.unwrap_or(-1)),
        flags: NodeFlags::from_bits(flags),
        translation: parse_track(body, "Translation", 3, model)?,
        rotation: parse_track(body, "Rotation", 4, model)?,
        scaling: parse_track(body, "Scaling", 3, model)?,
        visibility: parse_track(body, "Visibility", 1, model)?,
    })
}

fn parse_track(
    body: &[Node],
    name: &str,
    expected_elements: usize,
    model: &mut Model,
) -> Result<TrackId, MdlError> {
    let Some(track) = track_block(body, name) else {
        return Ok(TrackId::NONE);
    };
    let interpolation_type = if contains_word(track, "DontInterp") {
        0
    } else if contains_word(track, "Linear") {
        1
    } else if contains_word(track, "Hermite") {
        2
    } else if contains_word(track, "Bezier") {
        3
    } else {
        return Err(MdlError::new("mdl-missing-interpolation").with_arg("track", name));
    };
    let mut keyframes = Vec::new();
    let mut index = 0;
    while index + 2 < track.len() {
        if !matches!(track[index].kind, NodeKind::Number(_))
            || !matches!(track[index + 1].kind, NodeKind::Colon)
        {
            index += 1;
            continue;
        }
        let frame = number(&track[index])?;
        let data = value_numbers(&track[index + 2])?;
        if data.len() != expected_elements {
            return Err(MdlError::new("mdl-invalid-track-size")
                .with_arg("track", name)
                .with_arg("expected", expected_elements)
                .with_arg("actual", data.len()));
        }
        index += 3;
        while index < track.len() && matches!(track[index].kind, NodeKind::Comma) {
            index += 1;
        }
        let mut in_tan = Vec::new();
        let mut out_tan = Vec::new();
        if interpolation_type == 2 || interpolation_type == 3 {
            if word_at(track, index) == Some("InTan") {
                in_tan = value_numbers(required_next(track, index, "InTan")?)?;
                index += 2;
                while index < track.len() && matches!(track[index].kind, NodeKind::Comma) {
                    index += 1;
                }
            }
            if word_at(track, index) == Some("OutTan") {
                out_tan = value_numbers(required_next(track, index, "OutTan")?)?;
                index += 2;
            }
            if in_tan.len() != expected_elements || out_tan.len() != expected_elements {
                return Err(MdlError::new("mdl-invalid-tangent-size")
                    .with_arg("track", name)
                    .with_arg("expected", expected_elements));
            }
        }
        keyframes.push(Keyframe {
            frame,
            data,
            in_tan,
            out_tan,
        });
    }
    let id = TrackId::from_index(model.controllers.len());
    model.controllers.push(AnimationController {
        interpolation_type,
        global_seq_id: optional_number(track, "GlobalSeqId")?.unwrap_or(-1),
        keyframes,
    });
    Ok(id)
}

fn track_block<'a>(nodes: &'a [Node], name: &str) -> Option<&'a [Node]> {
    nodes.iter().enumerate().find_map(|(index, node)| {
        if word(node) != Some(name)
            || !nodes
                .get(index + 1)
                .is_some_and(|node| matches!(node.kind, NodeKind::Number(_)))
        {
            return None;
        }
        nodes.get(index + 2).and_then(block)
    })
}

fn parse_extent(nodes: &[Node]) -> Result<Extent, MdlError> {
    Ok(Extent {
        bounds_radius: optional_number(nodes, "BoundsRadius")?.unwrap_or(0.0),
        minimum: optional_vector(nodes, "MinimumExtent")?.unwrap_or([0.0; 3]),
        maximum: optional_vector(nodes, "MaximumExtent")?.unwrap_or([0.0; 3]),
    })
}

fn parse_filter_mode(value: &str, nodes: &[Node]) -> Result<FilterMode, MdlError> {
    match value {
        "None" => Ok(FilterMode::None),
        "Transparent" => Ok(FilterMode::Transparent),
        "Blend" => Ok(FilterMode::Blend),
        "Additive" => Ok(FilterMode::Additive),
        "AddAlpha" => Ok(FilterMode::AddAlpha),
        "Modulate" => Ok(FilterMode::Modulate),
        "Modulate2x" => Ok(FilterMode::Modulate2x),
        _ => Err(error(
            "mdl-invalid-filter-mode",
            nodes.first().map_or(1, |node| node.line),
            nodes.first().map_or(1, |node| node.column),
        )
        .with_arg("mode", value)),
    }
}

fn parse_shading_flags(nodes: &[Node]) -> Vec<ShadingFlags> {
    [
        ("Unshaded", ShadingFlags::Unshaded),
        ("SphereEnvMap", ShadingFlags::SphereEnvMap),
        ("TwoSided", ShadingFlags::TwoSided),
        ("Unfogged", ShadingFlags::Unfogged),
        ("NoDepthTest", ShadingFlags::NoDepthTest),
        ("NoDepthSet", ShadingFlags::NoDepthSet),
    ]
    .into_iter()
    .filter_map(|(name, flag)| contains_word(nodes, name).then_some(flag))
    .collect()
}

fn particle_blend_mode(nodes: &[Node]) -> u32 {
    ["Blend", "Additive", "Modulate", "Modulate2x", "AlphaKey"]
        .iter()
        .position(|name| contains_word(nodes, name))
        .unwrap_or(0) as u32
}

fn parse_color_list(nodes: &[Node]) -> Result<[[f32; 3]; 3], MdlError> {
    let colors: Vec<[f32; 3]> = repeated_blocks(nodes, "Color")
        .map(|(body, _)| {
            let holder = Node {
                kind: NodeKind::Block(body.to_vec()),
                line: body.first().map_or(1, |node| node.line),
                column: body.first().map_or(1, |node| node.column),
            };
            vector(&holder)
        })
        .collect::<Result<_, _>>()?;
    colors.try_into().map_err(|values: Vec<_>| {
        MdlError::new("mdl-segment-color-size").with_arg("actual", values.len())
    })
}

fn required_next<'a>(nodes: &'a [Node], index: usize, field: &str) -> Result<&'a Node, MdlError> {
    nodes
        .get(index + 1)
        .ok_or_else(|| MdlError::new("mdl-missing-value").with_arg("field", field))
}

fn required_number<T>(nodes: &[Node], name: &str) -> Result<T, MdlError>
where
    T: std::str::FromStr,
{
    optional_number(nodes, name)?
        .ok_or_else(|| MdlError::new("mdl-missing-value").with_arg("field", name))
}

fn optional_number<T>(nodes: &[Node], name: &str) -> Result<Option<T>, MdlError>
where
    T: std::str::FromStr,
{
    let Some(index) = nodes.iter().position(|node| word(node) == Some(name)) else {
        return Ok(None);
    };
    let Some(node) = nodes.get(index + 1) else {
        return Ok(None);
    };
    if matches!(node.kind, NodeKind::Block(_)) {
        return Ok(None);
    }
    if nodes
        .get(index + 2)
        .is_some_and(|candidate| matches!(candidate.kind, NodeKind::Block(_)))
    {
        return Ok(None);
    }
    number(node).map(Some)
}

fn optional_optional_id(nodes: &[Node], name: &str) -> Result<Option<u32>, MdlError> {
    let Some(node) = after(nodes, name) else {
        return Ok(None);
    };
    if matches!(word(node), Some("None" | "Multiple")) {
        Ok(None)
    } else {
        number(node).map(Some)
    }
}

fn optional_string(nodes: &[Node], name: &str) -> Result<Option<String>, MdlError> {
    let Some(node) = after(nodes, name) else {
        return Ok(None);
    };
    string(node)
        .map(|value| Some(value.to_string()))
        .ok_or_else(|| error("mdl-expected-string", node.line, node.column))
}

fn optional_word<'a>(nodes: &'a [Node], name: &str) -> Option<&'a str> {
    after(nodes, name).and_then(word)
}

fn optional_vector<const N: usize>(
    nodes: &[Node],
    name: &str,
) -> Result<Option<[f32; N]>, MdlError> {
    let Some(node) = after(nodes, name) else {
        return Ok(None);
    };
    if !matches!(node.kind, NodeKind::Block(_)) {
        return Ok(None);
    }
    vector(node).map(Some)
}

fn required_vector<const N: usize>(nodes: &[Node], name: &str) -> Result<[f32; N], MdlError> {
    optional_vector(nodes, name)?
        .ok_or_else(|| MdlError::new("mdl-missing-vector").with_arg("field", name))
}

fn parse_vector_list<const N: usize>(
    nodes: &[Node],
    name: &str,
) -> Result<Vec<[f32; N]>, MdlError> {
    let Some((body, _)) = named_block(nodes, name) else {
        return Ok(Vec::new());
    };
    vector_nodes(body)
}

fn vector_nodes<const N: usize>(nodes: &[Node]) -> Result<Vec<[f32; N]>, MdlError> {
    nodes
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::Block(_)))
        .map(vector)
        .collect()
}

fn parse_number_list<T>(nodes: &[Node], name: &str) -> Result<Vec<T>, MdlError>
where
    T: std::str::FromStr,
{
    let Some((body, _)) = named_block(nodes, name) else {
        return Ok(Vec::new());
    };
    number_nodes(body)
}

fn number_nodes<T>(nodes: &[Node]) -> Result<Vec<T>, MdlError>
where
    T: std::str::FromStr,
{
    nodes
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::Number(_)))
        .map(number)
        .collect()
}

fn value_numbers(node: &Node) -> Result<Vec<f32>, MdlError> {
    if let Some(nodes) = block(node) {
        number_nodes(nodes)
    } else {
        number(node).map(|value| vec![value])
    }
}

fn f32_to_u32(value: f32, nodes: &[Node]) -> Result<u32, MdlError> {
    if value < 0.0 || value > u32::MAX as f32 || value.fract() != 0.0 {
        return Err(error(
            "mdl-number-out-of-range",
            nodes.first().map_or(1, |node| node.line),
            nodes.first().map_or(1, |node| node.column),
        )
        .with_arg("value", value));
    }
    Ok(value as u32)
}

fn word_at(nodes: &[Node], index: usize) -> Option<&str> {
    nodes.get(index).and_then(word)
}
