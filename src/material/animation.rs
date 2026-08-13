use crate::animation::controller::{
    sample_discrete, sample_quaternion, sample_scalar, sample_vec3,
};
use crate::animation::types::{
    GeosetAnimPose, LayerPose, MaterialPose, ResolvedFrame, TextureAnimPose,
};
use crate::error::MdlError;
use crate::model::ids::{TextureIndex, TrackId};
use crate::model::model::Model;

/// Evaluate the material-facing animation channels for one resolved frame.
///
/// Layers are emitted in material-major, layer-minor order. Geoset and texture
/// animations retain their normalized model order.
#[allow(dead_code)]
pub fn evaluate_material_pose(
    model: &Model,
    frame: &ResolvedFrame,
) -> Result<MaterialPose, MdlError> {
    validate_static_references(model)?;

    let mut layers = Vec::with_capacity(
        model
            .materials
            .iter()
            .map(|material| material.layers.len())
            .sum(),
    );
    let mut layer_pose_index = 0;
    for (material_index, material) in model.materials.iter().enumerate() {
        for (layer_index, layer) in material.layers.iter().enumerate() {
            let alpha = sample_scalar(model, TrackId(layer.alpha_track), frame, layer.alpha)?;
            let alpha = if alpha < 0.0 { 1.0 } else { alpha };
            validate_finite_scalar(alpha, "alpha", "material-layer", layer_pose_index)?;

            let static_texture = layer.texture_id.map(|index| index as i32).unwrap_or(-1);
            let texture_id = sample_discrete(
                model,
                TrackId(layer.texture_id_track),
                frame,
                static_texture,
            )?;
            let texture_id = checked_texture_id(model, material_index, layer_index, texture_id)?;

            layers.push(LayerPose { alpha, texture_id });
            layer_pose_index += 1;
        }
    }

    let mut geoset_anims = Vec::with_capacity(model.geoset_anims.len());
    for (anim_index, anim) in model.geoset_anims.iter().enumerate() {
        let mut alpha = sample_scalar(model, anim.alpha_track, frame, anim.alpha)?;
        if anim.alpha_track.is_none() {
            if alpha < 0.0 {
                alpha = 1.0;
            }
        } else if controller_is_dont_interp(model, anim.alpha_track)? && alpha < 0.5 {
            alpha = -1.0;
        }
        validate_finite_scalar(alpha, "alpha", "geoset-anim", anim_index)?;

        let color_bgr = sample_vec3(model, anim.color_track, frame, anim.color)?;
        let mut color = [color_bgr[2], color_bgr[1], color_bgr[0]];
        if anim.color_track.is_none() {
            for channel in &mut color {
                if *channel < 0.0 {
                    *channel = 1.0;
                }
            }
        }
        validate_finite_slice(&color, "color", "geoset-anim", anim_index)?;
        geoset_anims.push(GeosetAnimPose { alpha, color });
    }

    let mut texture_anims = Vec::with_capacity(model.texture_anims.len());
    for (anim_index, anim) in model.texture_anims.iter().enumerate() {
        let translation = sample_vec3(model, anim.translation, frame, [0.0; 3])?;
        let rotation = sample_quaternion(model, anim.rotation, frame, [0.0, 0.0, 0.0, 1.0])?;
        let scaling = sample_vec3(model, anim.scaling, frame, [1.0; 3])?;
        validate_finite_slice(&translation, "translation", "texture-anim", anim_index)?;
        validate_finite_slice(&rotation, "rotation", "texture-anim", anim_index)?;
        validate_finite_slice(&scaling, "scaling", "texture-anim", anim_index)?;
        texture_anims.push(TextureAnimPose {
            translation,
            rotation,
            scaling,
        });
    }

    Ok(MaterialPose {
        layers,
        geoset_anims,
        texture_anims,
    })
}

fn validate_static_references(model: &Model) -> Result<(), MdlError> {
    let mut layer_pose_index = 0;
    for (material_index, material) in model.materials.iter().enumerate() {
        for (layer_index, layer) in material.layers.iter().enumerate() {
            if let Some(texture_index) = layer.texture_id
                && texture_index >= model.textures.len()
            {
                return Err(MdlError::new("animation-invalid-texture-index")
                    .with_arg("material", material_index)
                    .with_arg("layer", layer_index)
                    .with_arg("index", texture_index)
                    .with_arg("count", model.textures.len()));
            }
            if let Some(texture_anim_index) = layer.extra.texture_anim_id
                && texture_anim_index.0 as usize >= model.texture_anims.len()
            {
                return Err(MdlError::new("animation-invalid-texture-anim-index")
                    .with_arg("material", material_index)
                    .with_arg("layer", layer_index)
                    .with_arg("index", texture_anim_index.0)
                    .with_arg("count", model.texture_anims.len()));
            }
            validate_finite_scalar(layer.alpha, "alpha", "material-layer", layer_pose_index)?;
            layer_pose_index += 1;
        }
    }

    for (anim_index, anim) in model.geoset_anims.iter().enumerate() {
        if let Some(geoset_index) = anim.geoset_id
            && geoset_index.0 as usize >= model.geosets.len()
        {
            return Err(MdlError::new("animation-invalid-geoset-index")
                .with_arg("geoset_anim", anim_index)
                .with_arg("index", geoset_index.0)
                .with_arg("count", model.geosets.len()));
        }
        validate_finite_scalar(anim.alpha, "alpha", "geoset-anim", anim_index)?;
        validate_finite_slice(&anim.color, "color", "geoset-anim", anim_index)?;
    }
    Ok(())
}

fn checked_texture_id(
    model: &Model,
    material_index: usize,
    layer_index: usize,
    texture_id: i32,
) -> Result<Option<TextureIndex>, MdlError> {
    if texture_id == -1 {
        return Ok(None);
    }
    if texture_id < -1 || texture_id as usize >= model.textures.len() {
        return Err(MdlError::new("animation-invalid-texture-index")
            .with_arg("material", material_index)
            .with_arg("layer", layer_index)
            .with_arg("index", texture_id)
            .with_arg("count", model.textures.len()));
    }
    Ok(Some(TextureIndex(texture_id as u32)))
}

fn controller_is_dont_interp(model: &Model, track: TrackId) -> Result<bool, MdlError> {
    if track.is_none() {
        return Ok(false);
    }
    model
        .controllers
        .get(track.0 as usize)
        .map(|controller| controller.interpolation_type == 0)
        .ok_or_else(|| {
            MdlError::new("animation-invalid-controller-index")
                .with_arg("index", track.0)
                .with_arg("count", model.controllers.len())
        })
}

fn validate_finite_scalar(
    value: f32,
    channel: &'static str,
    owner: &'static str,
    index: usize,
) -> Result<(), MdlError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(MdlError::new("animation-non-finite-material-value")
            .with_arg("channel", channel)
            .with_arg("owner", owner)
            .with_arg("index", index)
            .with_arg("value", value))
    }
}

fn validate_finite_slice(
    values: &[f32],
    channel: &'static str,
    owner: &'static str,
    index: usize,
) -> Result<(), MdlError> {
    for value in values {
        validate_finite_scalar(*value, channel, owner, index)?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::animation::types::PlaybackMode;
    use crate::material::{FilterMode, Layer, Material};
    use crate::model::geoset::Geoset;
    use crate::model::ids::{GeosetIndex, TextureAnimIndex};
    use crate::model::objects::{GeosetAnim, LayerRef, TextureAnim};
    use crate::model::skeleton::{AnimationController, Keyframe};
    use crate::model::texture::Texture;

    fn frame(time: f64) -> ResolvedFrame {
        ResolvedFrame {
            sequence: None,
            sequence_frame: time,
            global_frame: time,
            playback: PlaybackMode::Loop,
            view: None,
        }
    }

    fn key(frame: i32, data: &[f32]) -> Keyframe {
        Keyframe {
            frame,
            data: data.to_vec(),
            in_tan: Vec::new(),
            out_tan: Vec::new(),
        }
    }

    fn controller(interpolation_type: u32, keys: Vec<Keyframe>) -> AnimationController {
        AnimationController {
            interpolation_type,
            global_seq_id: -1,
            keyframes: keys,
        }
    }

    fn layer(alpha: f32, texture_id: Option<usize>) -> Layer {
        Layer {
            texture_id,
            filter_mode: FilterMode::None,
            shading_flags: Vec::new(),
            alpha,
            extra: LayerRef::default(),
            alpha_track: -1,
            texture_id_track: -1,
            enabled: true,
            alpha_override: None,
            filter_mode_override: None,
            shading_flags_override: None,
        }
    }

    fn texture() -> Texture {
        Texture {
            filename: String::new(),
            replaceable_id: 0,
            flags: Default::default(),
            image_data: None,
            width: 0,
            height: 0,
        }
    }

    #[test]
    fn evaluates_layers_in_material_major_layer_minor_order() {
        let mut model = Model::default();
        model.textures = vec![texture(), texture(), texture()];
        model.materials = vec![
            Material {
                layers: vec![layer(0.1, Some(0)), layer(0.2, Some(1))],
                ..Material::default()
            },
            Material {
                layers: vec![layer(0.3, Some(2))],
                ..Material::default()
            },
        ];

        let pose = evaluate_material_pose(&model, &frame(0.0)).expect("evaluate materials");
        assert_eq!(pose.layers.len(), 3);
        assert_eq!(pose.layers[0].alpha, 0.1);
        assert_eq!(pose.layers[1].alpha, 0.2);
        assert_eq!(pose.layers[2].alpha, 0.3);
        assert_eq!(pose.layers[2].texture_id, Some(TextureIndex(2)));
    }

    #[test]
    fn samples_kmta_and_kmtf_and_maps_minus_one_to_none() {
        let mut model = Model::default();
        model.textures = vec![texture(), texture()];
        model.controllers = vec![
            controller(1, vec![key(0, &[0.0]), key(10, &[1.0])]),
            controller(0, vec![key(0, &[-1.0]), key(10, &[1.0])]),
        ];
        let mut animated = layer(0.75, Some(0));
        animated.alpha_track = 0;
        animated.texture_id_track = 1;
        model.materials = vec![Material {
            layers: vec![animated],
            ..Material::default()
        }];

        let none = evaluate_material_pose(&model, &frame(5.0)).expect("sample first KMTF key");
        assert_eq!(none.layers[0].alpha, 0.5);
        assert_eq!(none.layers[0].texture_id, None);

        let texture = evaluate_material_pose(&model, &frame(10.0)).expect("sample second KMTF key");
        assert_eq!(texture.layers[0].texture_id, Some(TextureIndex(1)));
    }

    #[test]
    fn applies_original_negative_alpha_and_geoa_dont_interp_rules() {
        let mut model = Model::default();
        model.materials = vec![Material {
            layers: vec![layer(-0.25, None)],
            ..Material::default()
        }];
        model.geosets.push(Geoset::default());
        model.controllers = vec![controller(0, vec![key(0, &[0.4])])];
        model.geoset_anims = vec![
            GeosetAnim {
                geoset_id: Some(GeosetIndex(0)),
                alpha: -0.5,
                color: [-0.1, 0.25, 0.5],
                ..GeosetAnim::default()
            },
            GeosetAnim {
                geoset_id: Some(GeosetIndex(0)),
                alpha: 1.0,
                alpha_track: TrackId(0),
                ..GeosetAnim::default()
            },
        ];

        let pose = evaluate_material_pose(&model, &frame(0.0)).expect("evaluate alpha rules");
        assert_eq!(pose.layers[0].alpha, 1.0);
        assert_eq!(pose.geoset_anims[0].alpha, 1.0);
        assert_eq!(pose.geoset_anims[0].color, [0.5, 0.25, 1.0]);
        assert_eq!(pose.geoset_anims[1].alpha, -1.0);
    }

    #[test]
    fn samples_geoa_color_as_bgr_to_rgb() {
        let mut model = Model::default();
        model.geosets.push(Geoset::default());
        model.controllers = vec![controller(0, vec![key(0, &[0.1, 0.2, 0.9])])];
        model.geoset_anims = vec![GeosetAnim {
            geoset_id: Some(GeosetIndex(0)),
            color_track: TrackId(0),
            ..GeosetAnim::default()
        }];

        let pose = evaluate_material_pose(&model, &frame(0.0)).expect("evaluate GEOA color");
        assert_eq!(pose.geoset_anims[0].color, [0.9, 0.2, 0.1]);
    }

    #[test]
    fn samples_txan_translation_rotation_and_scaling() {
        let mut model = Model::default();
        model.controllers = vec![
            controller(0, vec![key(0, &[1.0, 2.0, 3.0])]),
            controller(0, vec![key(0, &[0.0, 0.0, 0.0, 1.0])]),
            controller(0, vec![key(0, &[2.0, 3.0, 4.0])]),
        ];
        model.texture_anims = vec![TextureAnim {
            translation: TrackId(0),
            rotation: TrackId(1),
            scaling: TrackId(2),
        }];
        let mut animated = layer(1.0, None);
        animated.extra.texture_anim_id = Some(TextureAnimIndex(0));
        model.materials = vec![Material {
            layers: vec![animated],
            ..Material::default()
        }];

        let pose = evaluate_material_pose(&model, &frame(0.0)).expect("evaluate TXAN");
        assert_eq!(pose.texture_anims[0].translation, [1.0, 2.0, 3.0]);
        assert_eq!(pose.texture_anims[0].rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(pose.texture_anims[0].scaling, [2.0, 3.0, 4.0]);
    }

    #[test]
    fn rejects_invalid_material_references_and_texture_values() {
        let mut bad_texture = Model::default();
        bad_texture.materials = vec![Material {
            layers: vec![layer(1.0, Some(0))],
            ..Material::default()
        }];
        assert_eq!(
            evaluate_material_pose(&bad_texture, &frame(0.0))
                .expect_err("static texture must be in range")
                .key,
            "animation-invalid-texture-index"
        );

        let mut bad_texture_anim = Model::default();
        let mut animated = layer(1.0, None);
        animated.extra.texture_anim_id = Some(TextureAnimIndex(0));
        bad_texture_anim.materials = vec![Material {
            layers: vec![animated],
            ..Material::default()
        }];
        assert_eq!(
            evaluate_material_pose(&bad_texture_anim, &frame(0.0))
                .expect_err("texture animation reference must be in range")
                .key,
            "animation-invalid-texture-anim-index"
        );

        let mut bad_geoset = Model::default();
        bad_geoset.geoset_anims = vec![GeosetAnim {
            geoset_id: Some(GeosetIndex(0)),
            ..GeosetAnim::default()
        }];
        assert_eq!(
            evaluate_material_pose(&bad_geoset, &frame(0.0))
                .expect_err("geoset reference must be in range")
                .key,
            "animation-invalid-geoset-index"
        );

        let mut bad_dynamic = Model::default();
        bad_dynamic.textures.push(texture());
        bad_dynamic.controllers = vec![controller(0, vec![key(0, &[2.0])])];
        let mut dynamic = layer(1.0, None);
        dynamic.texture_id_track = 0;
        bad_dynamic.materials = vec![Material {
            layers: vec![dynamic],
            ..Material::default()
        }];
        assert_eq!(
            evaluate_material_pose(&bad_dynamic, &frame(0.0))
                .expect_err("dynamic texture must be in range")
                .key,
            "animation-invalid-texture-index"
        );
    }

    #[test]
    fn rejects_non_finite_static_values_and_propagates_track_errors() {
        let mut non_finite = Model::default();
        non_finite.materials = vec![Material {
            layers: vec![layer(f32::NAN, None)],
            ..Material::default()
        }];
        assert_eq!(
            evaluate_material_pose(&non_finite, &frame(0.0))
                .expect_err("non-finite alpha must fail")
                .key,
            "animation-non-finite-material-value"
        );

        let mut bad_width = Model::default();
        bad_width.controllers = vec![controller(0, vec![key(0, &[1.0, 2.0])])];
        bad_width.texture_anims = vec![TextureAnim {
            translation: TrackId(0),
            ..TextureAnim::default()
        }];
        assert_eq!(
            evaluate_material_pose(&bad_width, &frame(0.0))
                .expect_err("TRACK validation must propagate")
                .key,
            "animation-invalid-track-width"
        );
    }
}
