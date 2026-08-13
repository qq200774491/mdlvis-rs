#![cfg_attr(not(test), allow(dead_code))]

use super::skin::skin_geoset;
use super::types::*;
use crate::animation::types::{Pose, TextureAnimPose};
use crate::error::MdlError;
use crate::material::{FilterMode, ShadingFlags};
use crate::model::ids::{GeosetIndex, MaterialIndex, TextureIndex};
use crate::model::model::Model;

pub fn build_scene_packet(model: &Model, pose: &Pose) -> Result<ScenePacket, MdlError> {
    validate_pose(model, pose)?;
    validate_model(model)?;

    let textures = model
        .textures
        .iter()
        .enumerate()
        .map(|(index, texture)| {
            Ok(SceneTextureRequest {
                index: TextureIndex(checked_u32(index, "texture")?),
                filename: texture.filename.clone(),
                replaceable_id: texture.replaceable_id,
                wrap_u: texture.flags.wrap_width,
                wrap_v: texture.flags.wrap_height,
            })
        })
        .collect::<Result<Vec<_>, MdlError>>()?;

    let mut meshes = Vec::with_capacity(model.geosets.len());
    for (geoset_index, geoset) in model.geosets.iter().enumerate() {
        let (positions, normals) = skin_geoset(geoset, pose, geoset_index)?;
        meshes.push(SceneMesh {
            geoset: GeosetIndex(checked_u32(geoset_index, "geoset")?),
            bounds: bounds(&positions),
            positions,
            normals,
            uv_sets: geoset
                .tex_coord_sets
                .iter()
                .map(|set| set.iter().map(|uv| uv.uv).collect())
                .collect(),
            triangles: geoset.faces.iter().map(|face| face.vertices).collect(),
        });
    }

    let mut draws = Vec::new();
    let mut material_layer_offsets = Vec::with_capacity(model.materials.len());
    let mut layer_offset = 0usize;
    for material in &model.materials {
        material_layer_offsets.push(layer_offset);
        layer_offset += material.layers.len();
    }
    for (geoset_index, geoset) in model.geosets.iter().enumerate() {
        let material_index = geoset.material_id.ok_or_else(|| {
            MdlError::new("scene-missing-material").with_arg("geoset", geoset_index)
        })?;
        let material = model.materials.get(material_index).ok_or_else(|| {
            MdlError::new("scene-invalid-material-index")
                .with_arg("geoset", geoset_index)
                .with_arg("index", material_index)
                .with_arg("count", model.materials.len())
        })?;
        let (geoset_color, geoset_alpha) = geoset_anim_values(model, pose, geoset_index);
        for (layer_index, layer) in material.layers.iter().enumerate() {
            let layer_pose_index = material_layer_offsets[material_index] + layer_index;
            let layer_pose = pose.materials.layers.get(layer_pose_index).ok_or_else(|| {
                MdlError::new("scene-invalid-layer-pose-count")
                    .with_arg("index", layer_pose_index)
                    .with_arg("count", pose.materials.layers.len())
            })?;
            let coord_set = layer.extra.coord_id;
            if coord_set as usize >= geoset.tex_coord_sets.len() {
                return Err(MdlError::new("scene-invalid-coord-set")
                    .with_arg("geoset", geoset_index)
                    .with_arg("material", material_index)
                    .with_arg("layer", layer_index)
                    .with_arg("index", coord_set)
                    .with_arg("count", geoset.tex_coord_sets.len()));
            }
            if let Some(texture) = layer_pose.texture_id
                && texture.0 as usize >= model.textures.len()
            {
                return Err(MdlError::new("scene-invalid-texture-index")
                    .with_arg("material", material_index)
                    .with_arg("layer", layer_index)
                    .with_arg("index", texture.0)
                    .with_arg("count", model.textures.len()));
            }
            let texture_transform = match layer.extra.texture_anim_id {
                Some(id) => {
                    texture_transform(pose.materials.texture_anims.get(id.0 as usize).ok_or_else(
                        || {
                            MdlError::new("scene-invalid-texture-anim-index")
                                .with_arg("index", id.0)
                                .with_arg("count", pose.materials.texture_anims.len())
                        },
                    )?)
                }
                None => TextureTransform::default(),
            };
            let source_ordinal = checked_u32(draws.len(), "draw")?;
            draws.push(SceneDraw {
                source_ordinal,
                geoset: GeosetIndex(checked_u32(geoset_index, "geoset")?),
                mesh: checked_u32(geoset_index, "mesh")?,
                material: MaterialIndex(checked_u32(material_index, "material")?),
                layer: checked_u32(layer_index, "layer")?,
                priority_plane: material.priority_plane,
                geoset_color,
                geoset_alpha,
                layer_alpha: layer_pose.alpha,
                texture: layer_pose.texture_id,
                coord_set,
                texture_transform,
                filter_mode: filter_mode(&layer.filter_mode),
                material_state: SceneMaterialState {
                    constant_color: material.flags.constant_color,
                    full_resolution: material.flags.full_resolution,
                    sort_primitives_far_z: material.flags.sort_primitives_far_z,
                },
                render_state: render_state(&layer.shading_flags),
                sort_class: if material.flags.sort_primitives_far_z {
                    SceneSortClass::BackToFrontTriangles
                } else {
                    SceneSortClass::Stable
                },
            });
        }
    }
    ScenePacket::new(pose.frame, meshes, draws, textures)
}

fn validate_pose(model: &Model, pose: &Pose) -> Result<(), MdlError> {
    if pose.object_to_pose.len() != pose.nodes.len() {
        return Err(MdlError::new("scene-invalid-pose-map-size")
            .with_arg("nodes", pose.nodes.len())
            .with_arg("map", pose.object_to_pose.len()));
    }
    for (index, node) in pose.nodes.iter().enumerate() {
        if pose.object_to_pose.get(&node.object_id) != Some(&index) {
            return Err(MdlError::new("scene-invalid-pose-map")
                .with_arg("object_id", node.object_id.0)
                .with_arg("index", index));
        }
        if node.world.iter().flatten().any(|value| !value.is_finite())
            || node
                .local
                .translation
                .iter()
                .chain(node.local.rotation.iter())
                .chain(node.local.scaling.iter())
                .any(|value| !value.is_finite())
            || !node.sampled_visibility.is_finite()
        {
            return Err(
                MdlError::new("scene-non-finite-pose-node").with_arg("object_id", node.object_id.0)
            );
        }
    }
    let layer_count: usize = model
        .materials
        .iter()
        .map(|material| material.layers.len())
        .sum();
    if pose.materials.layers.len() != layer_count {
        return Err(MdlError::new("scene-invalid-layer-pose-count")
            .with_arg("expected", layer_count)
            .with_arg("actual", pose.materials.layers.len()));
    }
    if pose.materials.geoset_anims.len() != model.geoset_anims.len() {
        return Err(MdlError::new("scene-invalid-geoset-anim-pose-count")
            .with_arg("expected", model.geoset_anims.len())
            .with_arg("actual", pose.materials.geoset_anims.len()));
    }
    if pose.materials.texture_anims.len() != model.texture_anims.len() {
        return Err(MdlError::new("scene-invalid-texture-anim-pose-count")
            .with_arg("expected", model.texture_anims.len())
            .with_arg("actual", pose.materials.texture_anims.len()));
    }
    for (index, layer) in pose.materials.layers.iter().enumerate() {
        if !layer.alpha.is_finite() {
            return Err(MdlError::new("scene-non-finite-layer-pose").with_arg("index", index));
        }
        if let Some(texture) = layer.texture_id
            && texture.0 as usize >= model.textures.len()
        {
            return Err(MdlError::new("scene-invalid-texture-index")
                .with_arg("layer_pose", index)
                .with_arg("index", texture.0)
                .with_arg("count", model.textures.len()));
        }
    }
    for (index, anim) in pose.materials.geoset_anims.iter().enumerate() {
        if !anim.alpha.is_finite() || anim.color.iter().any(|value| !value.is_finite()) {
            return Err(MdlError::new("scene-non-finite-geoset-anim-pose").with_arg("index", index));
        }
    }
    for (index, anim) in pose.materials.texture_anims.iter().enumerate() {
        if anim
            .translation
            .iter()
            .chain(anim.rotation.iter())
            .chain(anim.scaling.iter())
            .any(|value| !value.is_finite())
        {
            return Err(
                MdlError::new("scene-non-finite-texture-anim-pose").with_arg("index", index)
            );
        }
        let rotation_length: f32 = anim.rotation.iter().map(|value| value * value).sum();
        if !rotation_length.is_finite() || rotation_length <= f32::EPSILON {
            return Err(
                MdlError::new("scene-invalid-texture-anim-quaternion").with_arg("index", index)
            );
        }
    }
    Ok(())
}

fn validate_model(model: &Model) -> Result<(), MdlError> {
    for (geoset_index, geoset) in model.geosets.iter().enumerate() {
        if !geoset.normals.is_empty() && geoset.normals.len() != geoset.vertices.len() {
            return Err(MdlError::new("scene-invalid-normal-count")
                .with_arg("geoset", geoset_index)
                .with_arg("expected", geoset.vertices.len())
                .with_arg("actual", geoset.normals.len()));
        }
        for (set_index, set) in geoset.tex_coord_sets.iter().enumerate() {
            if set.len() != geoset.vertices.len() {
                return Err(MdlError::new("scene-invalid-uv-count")
                    .with_arg("geoset", geoset_index)
                    .with_arg("set", set_index)
                    .with_arg("expected", geoset.vertices.len())
                    .with_arg("actual", set.len()));
            }
        }
        for (face_index, face) in geoset.faces.iter().enumerate() {
            for vertex in face.vertices {
                if vertex as usize >= geoset.vertices.len() {
                    return Err(MdlError::new("scene-invalid-face-index")
                        .with_arg("geoset", geoset_index)
                        .with_arg("face", face_index)
                        .with_arg("index", vertex)
                        .with_arg("count", geoset.vertices.len()));
                }
            }
        }
        let material_index = geoset.material_id.ok_or_else(|| {
            MdlError::new("scene-missing-material").with_arg("geoset", geoset_index)
        })?;
        let material = model.materials.get(material_index).ok_or_else(|| {
            MdlError::new("scene-invalid-material-index")
                .with_arg("geoset", geoset_index)
                .with_arg("index", material_index)
                .with_arg("count", model.materials.len())
        })?;
        for (layer_index, layer) in material.layers.iter().enumerate() {
            if layer.extra.coord_id as usize >= geoset.tex_coord_sets.len() {
                return Err(MdlError::new("scene-invalid-coord-set")
                    .with_arg("geoset", geoset_index)
                    .with_arg("material", material_index)
                    .with_arg("layer", layer_index)
                    .with_arg("index", layer.extra.coord_id)
                    .with_arg("count", geoset.tex_coord_sets.len()));
            }
        }
    }
    for (index, geoset_anim) in model.geoset_anims.iter().enumerate() {
        if let Some(geoset) = geoset_anim.geoset_id
            && geoset.0 as usize >= model.geosets.len()
        {
            return Err(MdlError::new("scene-invalid-geoset-anim-reference")
                .with_arg("index", index)
                .with_arg("geoset", geoset.0)
                .with_arg("count", model.geosets.len()));
        }
    }
    for material in &model.materials {
        for layer in &material.layers {
            if let Some(texture_anim) = layer.extra.texture_anim_id
                && texture_anim.0 as usize >= model.texture_anims.len()
            {
                return Err(MdlError::new("scene-invalid-texture-anim-reference")
                    .with_arg("index", texture_anim.0)
                    .with_arg("count", model.texture_anims.len()));
            }
        }
    }
    Ok(())
}

fn geoset_anim_values(model: &Model, pose: &Pose, geoset_index: usize) -> ([f32; 3], f32) {
    let mut result = ([1.0; 3], 1.0);
    for (index, anim) in model.geoset_anims.iter().enumerate() {
        if anim.geoset_id == Some(GeosetIndex(geoset_index as u32)) {
            let sampled = pose.materials.geoset_anims[index];
            result = (sampled.color, sampled.alpha);
        }
    }
    result
}

fn texture_transform(pose: &TextureAnimPose) -> TextureTransform {
    TextureTransform {
        translation: pose.translation,
        rotation: pose.rotation,
        scaling: pose.scaling,
    }
}

fn bounds(positions: &[[f32; 3]]) -> SceneBounds {
    if positions.is_empty() {
        return SceneBounds {
            min: [0.0; 3],
            max: [0.0; 3],
            center: [0.0; 3],
        };
    }
    let mut min = positions[0];
    let mut max = positions[0];
    for position in &positions[1..] {
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    SceneBounds {
        min,
        max,
        center: std::array::from_fn(|axis| (min[axis] + max[axis]) * 0.5),
    }
}

fn checked_u32(value: usize, owner: &'static str) -> Result<u32, MdlError> {
    u32::try_from(value).map_err(|_| {
        MdlError::new("scene-index-out-of-range")
            .with_arg("owner", owner)
            .with_arg("index", value)
    })
}

fn filter_mode(value: &FilterMode) -> SceneFilterMode {
    match value {
        FilterMode::None => SceneFilterMode::None,
        FilterMode::Transparent => SceneFilterMode::Transparent,
        FilterMode::Blend => SceneFilterMode::Blend,
        FilterMode::Additive => SceneFilterMode::Additive,
        FilterMode::AddAlpha => SceneFilterMode::AddAlpha,
        FilterMode::Modulate => SceneFilterMode::Modulate,
        FilterMode::Modulate2x => SceneFilterMode::Modulate2x,
    }
}

fn render_state(flags: &[ShadingFlags]) -> SceneRenderState {
    SceneRenderState {
        two_sided: flags.contains(&ShadingFlags::TwoSided),
        unshaded: flags.contains(&ShadingFlags::Unshaded),
        unfogged: flags.contains(&ShadingFlags::Unfogged),
        no_depth_test: flags.contains(&ShadingFlags::NoDepthTest),
        no_depth_write: flags.contains(&ShadingFlags::NoDepthSet),
        sphere_env_map: flags.contains(&ShadingFlags::SphereEnvMap),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::types::{GeosetAnimPose, LayerPose, MaterialPose, Pose, TextureAnimPose};
    use crate::material::{Layer, Material};
    use crate::model::geoset::{Face, Geoset, Normal, TexCoord, Vertex};
    use crate::model::ids::{GeosetIndex, TextureAnimIndex};
    use crate::model::objects::{GeosetAnim, LayerRef, MaterialFlags, TextureAnim};
    use crate::model::texture::Texture;

    fn texture() -> Texture {
        Texture {
            filename: "Textures/test.blp".to_string(),
            replaceable_id: 1,
            flags: crate::model::objects::TextureFlags {
                wrap_width: true,
                wrap_height: false,
            },
            image_data: None,
            width: 0,
            height: 0,
        }
    }

    fn layer(coord_id: u32, texture_anim_id: Option<u32>) -> Layer {
        Layer {
            texture_id: Some(0),
            filter_mode: FilterMode::Blend,
            shading_flags: vec![
                ShadingFlags::TwoSided,
                ShadingFlags::Unshaded,
                ShadingFlags::Unfogged,
                ShadingFlags::NoDepthTest,
                ShadingFlags::NoDepthSet,
                ShadingFlags::SphereEnvMap,
            ],
            alpha: 1.0,
            extra: LayerRef {
                texture_anim_id: texture_anim_id.map(TextureAnimIndex),
                coord_id,
            },
            alpha_track: -1,
            texture_id_track: -1,
            enabled: false,
            alpha_override: Some(0.0),
            filter_mode_override: Some(FilterMode::None),
            shading_flags_override: Some(Vec::new()),
        }
    }

    fn geoset() -> Geoset {
        Geoset {
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
            tex_coord_sets: vec![
                vec![TexCoord { uv: [0.0, 0.0] }; 3],
                vec![TexCoord { uv: [0.5, 0.5] }; 3],
            ],
            faces: vec![Face {
                vertices: [0, 1, 2],
            }],
            material_id: Some(0),
            ..Geoset::default()
        }
    }

    fn model() -> Model {
        Model {
            geosets: vec![geoset()],
            materials: vec![Material {
                layers: vec![layer(1, Some(0))],
                priority_plane: 3,
                flags: MaterialFlags {
                    constant_color: true,
                    sort_primitives_far_z: true,
                    full_resolution: true,
                },
            }],
            textures: vec![texture()],
            geoset_anims: vec![
                GeosetAnim {
                    geoset_id: None,
                    ..GeosetAnim::default()
                },
                GeosetAnim {
                    geoset_id: Some(GeosetIndex(0)),
                    ..GeosetAnim::default()
                },
                GeosetAnim {
                    geoset_id: Some(GeosetIndex(0)),
                    ..GeosetAnim::default()
                },
            ],
            texture_anims: vec![TextureAnim::default()],
            ..Model::default()
        }
    }

    fn pose() -> Pose {
        Pose {
            materials: MaterialPose {
                layers: vec![LayerPose {
                    alpha: 0.4,
                    texture_id: Some(TextureIndex(0)),
                }],
                geoset_anims: vec![
                    GeosetAnimPose {
                        color: [9.0; 3],
                        alpha: 9.0,
                    },
                    GeosetAnimPose {
                        color: [0.1, 0.2, 0.3],
                        alpha: 0.5,
                    },
                    GeosetAnimPose {
                        color: [0.7, 0.8, 0.9],
                        alpha: 0.25,
                    },
                ],
                texture_anims: vec![TextureAnimPose {
                    translation: [1.0, 2.0, 3.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scaling: [2.0, 3.0, 4.0],
                }],
            },
            ..Pose::default()
        }
    }

    #[test]
    fn empty_and_identity_scene_packets_are_stable() {
        let empty = build_scene_packet(&Model::default(), &Pose::default()).unwrap();
        assert!(empty.meshes.is_empty() && empty.draws.is_empty());

        let packet = build_scene_packet(&model(), &pose()).unwrap();
        assert_eq!(packet.meshes.len(), 1);
        assert_eq!(packet.draws.len(), 1);
        assert_eq!(packet.meshes[0].positions[1], [1.0, 0.0, 0.0]);
        assert_eq!(packet.meshes[0].normals[0], [0.0, 0.0, 1.0]);
        assert_eq!(packet.meshes[0].uv_sets[1][0], [0.5, 0.5]);
        assert_eq!(packet.draws[0].coord_set, 1);
        assert_eq!(packet.draws[0].geoset_color, [0.7, 0.8, 0.9]);
        assert_eq!(packet.draws[0].geoset_alpha, 0.25);
        assert_eq!(packet.draws[0].layer_alpha, 0.4);
        assert_eq!(
            packet.draws[0].texture_transform.translation,
            [1.0, 2.0, 3.0]
        );
        assert!(packet.draws[0].render_state.two_sided);
        assert!(packet.draws[0].render_state.unshaded);
        assert!(packet.draws[0].render_state.unfogged);
        assert!(packet.draws[0].render_state.no_depth_test);
        assert!(packet.draws[0].render_state.no_depth_write);
        assert!(packet.draws[0].render_state.sphere_env_map);
        assert_eq!(packet.draws[0].filter_mode, SceneFilterMode::Blend);
        assert!(packet.draws[0].material_state.constant_color);
        assert_eq!(packet.textures[0].replaceable_id, 1);
        assert!(packet.textures[0].wrap_u);
    }

    #[test]
    fn maps_all_seven_filter_modes_without_runtime_overrides() {
        let source = [
            FilterMode::None,
            FilterMode::Transparent,
            FilterMode::Blend,
            FilterMode::Additive,
            FilterMode::AddAlpha,
            FilterMode::Modulate,
            FilterMode::Modulate2x,
        ];
        let expected = [
            SceneFilterMode::None,
            SceneFilterMode::Transparent,
            SceneFilterMode::Blend,
            SceneFilterMode::Additive,
            SceneFilterMode::AddAlpha,
            SceneFilterMode::Modulate,
            SceneFilterMode::Modulate2x,
        ];
        assert_eq!(source.map(|value| filter_mode(&value)), expected);
    }

    #[test]
    fn build_is_repeatable_and_does_not_mutate_inputs() {
        let model = model();
        let pose = pose();
        let model_before = serde_json::to_value(&model).unwrap();
        let pose_before = pose.clone();
        let first = build_scene_packet(&model, &pose).unwrap();
        let second = build_scene_packet(&model, &pose).unwrap();
        assert_eq!(first, second);
        assert_eq!(serde_json::to_value(&model).unwrap(), model_before);
        assert_eq!(pose, pose_before);
    }

    #[test]
    fn invalid_references_and_shapes_error_without_panicking() {
        let cases: Vec<(&str, Model, Pose)> = vec![
            (
                "scene-missing-material",
                {
                    let mut value = model();
                    value.geosets[0].material_id = None;
                    value
                },
                pose(),
            ),
            (
                "scene-invalid-coord-set",
                {
                    let mut value = model();
                    value.materials[0].layers[0].extra.coord_id = 2;
                    value
                },
                pose(),
            ),
            (
                "scene-invalid-face-index",
                {
                    let mut value = model();
                    value.geosets[0].faces[0].vertices[2] = 3;
                    value
                },
                pose(),
            ),
            (
                "scene-invalid-uv-count",
                {
                    let mut value = model();
                    value.geosets[0].tex_coord_sets[0].pop();
                    value
                },
                pose(),
            ),
            (
                "scene-invalid-normal-count",
                {
                    let mut value = model();
                    value.geosets[0].normals.pop();
                    value
                },
                pose(),
            ),
            (
                "scene-invalid-geoset-anim-reference",
                {
                    let mut value = model();
                    value.geoset_anims[0].geoset_id = Some(GeosetIndex(9));
                    value
                },
                pose(),
            ),
            (
                "scene-invalid-texture-anim-reference",
                {
                    let mut value = model();
                    value.materials[0].layers[0].extra.texture_anim_id = Some(TextureAnimIndex(9));
                    value
                },
                pose(),
            ),
            ("scene-invalid-texture-index", model(), {
                let mut value = pose();
                value.materials.layers[0].texture_id = Some(TextureIndex(9));
                value
            }),
            ("scene-invalid-pose-map-size", model(), {
                let mut value = pose();
                value
                    .object_to_pose
                    .insert(crate::model::ids::ObjectId(7), 0);
                value
            }),
            ("scene-non-finite-layer-pose", model(), {
                let mut value = pose();
                value.materials.layers[0].alpha = f32::NAN;
                value
            }),
            ("scene-invalid-texture-anim-quaternion", model(), {
                let mut value = pose();
                value.materials.texture_anims[0].rotation = [0.0; 4];
                value
            }),
        ];
        for (key, model, pose) in cases {
            let result = std::panic::catch_unwind(|| build_scene_packet(&model, &pose));
            assert!(result.is_ok(), "{key} panicked");
            assert_eq!(result.unwrap().unwrap_err().key, key);
        }
    }

    #[test]
    fn preserves_u32_indices_above_u16_range() {
        let mut large = model();
        large.geosets[0].vertices = vec![Vertex { position: [0.0; 3] }; 70_001];
        large.geosets[0].normals.clear();
        large.geosets[0].tex_coord_sets = vec![vec![TexCoord { uv: [0.0; 2] }; 70_001]];
        large.geosets[0].faces = vec![Face {
            vertices: [0, 65_536, 70_000],
        }];
        large.materials[0].layers[0].extra.coord_id = 0;
        let packet = build_scene_packet(&large, &pose()).unwrap();
        assert_eq!(packet.meshes[0].triangles[0], [0, 65_536, 70_000]);
    }

    fn load_tracked(relative: &str) -> Model {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-data")
            .join(relative);
        let mut file = std::fs::File::open(path).unwrap();
        crate::parser::load::load(&mut file).unwrap()
    }

    fn tracked_pose(model: &Model) -> Result<Pose, MdlError> {
        crate::animation::evaluate_pose(
            model,
            crate::animation::types::FrameContext {
                sequence: Some(0),
                sequence_time: model.sequences[0].start_frame as f64,
                global_time: 0.0,
                playback: crate::animation::types::PlaybackMode::Clamp,
                view: Some(crate::animation::types::ViewFrame::default()),
            },
        )
    }

    fn assert_tracked_structure(model: &Model, pose: &Pose) {
        let packet = build_scene_packet(model, pose).unwrap();
        assert_eq!(packet.meshes.len(), model.geosets.len());
        assert_eq!(packet.textures.len(), model.textures.len());
        assert_eq!(
            packet.draws.len(),
            model
                .geosets
                .iter()
                .map(|geoset| model.materials[geoset.material_id.unwrap()].layers.len())
                .sum::<usize>()
        );
        assert_eq!(packet, build_scene_packet(model, pose).unwrap());
    }

    #[test]
    fn tracked_nether_builds_stable_scene_structure() {
        let model = load_tracked("Nether Blast/Nether Blast I.mdx");
        let pose = tracked_pose(&model).unwrap();
        assert_tracked_structure(&model, &pose);
    }

    #[test]
    fn tracked_ember_builds_stable_scene_structure() {
        let model = load_tracked("Ember Forge  Ember Knight/Ember Forge_opt2.mdx");
        let pose = tracked_pose(&model).unwrap();
        assert_tracked_structure(&model, &pose);
    }
}
