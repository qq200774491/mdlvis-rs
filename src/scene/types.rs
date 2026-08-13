#![cfg_attr(not(test), allow(dead_code))]

use crate::animation::types::{PlaybackMode, ResolvedFrame};
use crate::error::MdlError;
use crate::model::ids::{GeosetIndex, MaterialIndex, TextureIndex};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SCENE_PACKET_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenePacket {
    pub schema_version: u32,
    pub frame: SceneFrame,
    pub meshes: Vec<SceneMesh>,
    pub draws: Vec<SceneDraw>,
    pub textures: Vec<SceneTextureRequest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneFrame {
    pub sequence: Option<usize>,
    pub sequence_frame: f64,
    pub global_frame: f64,
    pub playback: ScenePlaybackMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScenePlaybackMode {
    Loop,
    Clamp,
}

impl From<ResolvedFrame> for SceneFrame {
    fn from(frame: ResolvedFrame) -> Self {
        Self {
            sequence: frame.sequence,
            sequence_frame: frame.sequence_frame,
            global_frame: frame.global_frame,
            playback: match frame.playback {
                PlaybackMode::Loop => ScenePlaybackMode::Loop,
                PlaybackMode::Clamp => ScenePlaybackMode::Clamp,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneMesh {
    pub geoset: GeosetIndex,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uv_sets: Vec<Vec<[f32; 2]>>,
    pub triangles: Vec<[u32; 3]>,
    pub bounds: SceneBounds,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub center: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneDraw {
    pub source_ordinal: u32,
    pub geoset: GeosetIndex,
    pub mesh: u32,
    pub material: MaterialIndex,
    pub layer: u32,
    pub priority_plane: i32,
    pub geoset_color: [f32; 3],
    pub geoset_alpha: f32,
    pub layer_alpha: f32,
    pub texture: Option<TextureIndex>,
    pub coord_set: u32,
    pub texture_transform: TextureTransform,
    pub filter_mode: SceneFilterMode,
    pub material_state: SceneMaterialState,
    pub render_state: SceneRenderState,
    pub sort_class: SceneSortClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneTextureRequest {
    pub index: TextureIndex,
    pub filename: String,
    pub replaceable_id: u32,
    pub wrap_u: bool,
    pub wrap_v: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TextureTransform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scaling: [f32; 3],
}

impl Default for TextureTransform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scaling: [1.0; 3],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SceneFilterMode {
    None,
    Transparent,
    Blend,
    Additive,
    AddAlpha,
    Modulate,
    Modulate2x,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderState {
    pub two_sided: bool,
    pub unshaded: bool,
    pub unfogged: bool,
    pub no_depth_test: bool,
    pub no_depth_write: bool,
    pub sphere_env_map: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneMaterialState {
    pub constant_color: bool,
    pub full_resolution: bool,
    pub sort_primitives_far_z: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SceneSortClass {
    Stable,
    BackToFrontTriangles,
}

impl ScenePacket {
    pub fn new(
        frame: ResolvedFrame,
        mut meshes: Vec<SceneMesh>,
        mut draws: Vec<SceneDraw>,
        mut textures: Vec<SceneTextureRequest>,
    ) -> Result<Self, MdlError> {
        for (draw_index, draw) in draws.iter().enumerate() {
            if !meshes.iter().any(|mesh| mesh.geoset == draw.geoset) {
                return Err(MdlError::new("scene-missing-draw-geoset")
                    .with_arg("draw", draw_index)
                    .with_arg("geoset", draw.geoset.0)
                    .with_arg("count", meshes.len()));
            }
            let mesh = meshes.get(draw.mesh as usize).ok_or_else(|| {
                MdlError::new("scene-invalid-mesh-index")
                    .with_arg("draw", draw_index)
                    .with_arg("mesh", draw.mesh)
                    .with_arg("count", meshes.len())
            })?;
            if mesh.geoset != draw.geoset {
                return Err(MdlError::new("scene-draw-mesh-geoset-mismatch")
                    .with_arg("draw", draw_index)
                    .with_arg("mesh", draw.mesh)
                    .with_arg("geoset", draw.geoset.0)
                    .with_arg("mesh_geoset", mesh.geoset.0));
            }
        }
        meshes.sort_by_key(|mesh| mesh.geoset.0);
        for draw in &mut draws {
            let index = meshes
                .iter()
                .position(|mesh| mesh.geoset == draw.geoset)
                .ok_or_else(|| {
                    MdlError::new("scene-missing-draw-geoset")
                        .with_arg("geoset", draw.geoset.0)
                        .with_arg("count", meshes.len())
                })?;
            draw.mesh = u32::try_from(index).map_err(|_| {
                MdlError::new("scene-index-out-of-range")
                    .with_arg("owner", "mesh")
                    .with_arg("index", index)
            })?;
        }
        draws.sort_by_key(|draw| draw.source_ordinal);
        textures.sort_by_key(|texture| texture.index.0);
        let packet = Self {
            schema_version: SCENE_PACKET_SCHEMA_VERSION,
            frame: frame.into(),
            meshes,
            draws,
            textures,
        };
        packet.validate()?;
        Ok(packet)
    }

    pub fn validate(&self) -> Result<(), MdlError> {
        if self.schema_version != SCENE_PACKET_SCHEMA_VERSION {
            return Err(MdlError::new("scene-invalid-schema-version")
                .with_arg("expected", SCENE_PACKET_SCHEMA_VERSION)
                .with_arg("actual", self.schema_version));
        }
        if !self.frame.sequence_frame.is_finite() || !self.frame.global_frame.is_finite() {
            return Err(MdlError::new("scene-non-finite-frame"));
        }

        let mut geosets = HashSet::new();
        for mesh in &self.meshes {
            if !geosets.insert(mesh.geoset) {
                return Err(
                    MdlError::new("scene-duplicate-geoset").with_arg("geoset", mesh.geoset.0)
                );
            }
        }

        let mut texture_indices = HashSet::new();
        for texture in &self.textures {
            if !texture_indices.insert(texture.index) {
                return Err(MdlError::new("scene-duplicate-texture-index")
                    .with_arg("texture", texture.index.0));
            }
        }

        let mut source_ordinals = HashSet::new();
        for draw in &self.draws {
            if !source_ordinals.insert(draw.source_ordinal) {
                return Err(MdlError::new("scene-duplicate-source-ordinal")
                    .with_arg("source_ordinal", draw.source_ordinal));
            }
        }

        if !strictly_increasing(self.meshes.iter().map(|mesh| mesh.geoset.0)) {
            return Err(MdlError::new("scene-non-canonical-mesh-order"));
        }
        if !strictly_increasing(self.draws.iter().map(|draw| draw.source_ordinal)) {
            return Err(MdlError::new("scene-non-canonical-draw-order"));
        }
        if !strictly_increasing(self.textures.iter().map(|texture| texture.index.0)) {
            return Err(MdlError::new("scene-non-canonical-texture-order"));
        }

        for (mesh_index, mesh) in self.meshes.iter().enumerate() {
            validate_mesh(mesh, mesh_index)?;
        }
        for (draw_index, draw) in self.draws.iter().enumerate() {
            if !geosets.contains(&draw.geoset) {
                return Err(MdlError::new("scene-missing-draw-geoset")
                    .with_arg("draw", draw_index)
                    .with_arg("geoset", draw.geoset.0)
                    .with_arg("count", self.meshes.len()));
            }
            let mesh = self.meshes.get(draw.mesh as usize).ok_or_else(|| {
                MdlError::new("scene-invalid-mesh-index")
                    .with_arg("draw", draw_index)
                    .with_arg("mesh", draw.mesh)
                    .with_arg("count", self.meshes.len())
            })?;
            if mesh.geoset != draw.geoset {
                return Err(MdlError::new("scene-draw-mesh-geoset-mismatch")
                    .with_arg("draw", draw_index)
                    .with_arg("mesh", draw.mesh)
                    .with_arg("geoset", draw.geoset.0)
                    .with_arg("mesh_geoset", mesh.geoset.0));
            }
            if draw.coord_set as usize >= mesh.uv_sets.len() {
                return Err(MdlError::new("scene-invalid-coord-set")
                    .with_arg("draw", draw_index)
                    .with_arg("coord_set", draw.coord_set)
                    .with_arg("count", mesh.uv_sets.len()));
            }
            if let Some(texture) = draw.texture
                && !texture_indices.contains(&texture)
            {
                return Err(MdlError::new("scene-missing-texture-request")
                    .with_arg("draw", draw_index)
                    .with_arg("texture", texture.0));
            }
            validate_draw(draw, draw_index)?;
        }
        Ok(())
    }
}

fn validate_mesh(mesh: &SceneMesh, mesh_index: usize) -> Result<(), MdlError> {
    if !mesh.normals.is_empty() && mesh.normals.len() != mesh.positions.len() {
        return Err(MdlError::new("scene-invalid-normal-count")
            .with_arg("mesh", mesh_index)
            .with_arg("expected", mesh.positions.len())
            .with_arg("actual", mesh.normals.len()));
    }
    for (set_index, uv_set) in mesh.uv_sets.iter().enumerate() {
        if uv_set.len() != mesh.positions.len() {
            return Err(MdlError::new("scene-invalid-uv-count")
                .with_arg("mesh", mesh_index)
                .with_arg("set", set_index)
                .with_arg("expected", mesh.positions.len())
                .with_arg("actual", uv_set.len()));
        }
    }
    for (vertex_index, position) in mesh.positions.iter().enumerate() {
        if !position.iter().all(|value| value.is_finite()) {
            return Err(MdlError::new("scene-non-finite-position")
                .with_arg("mesh", mesh_index)
                .with_arg("vertex", vertex_index));
        }
    }
    for (normal_index, normal) in mesh.normals.iter().enumerate() {
        if !normal.iter().all(|value| value.is_finite()) {
            return Err(MdlError::new("scene-non-finite-normal")
                .with_arg("mesh", mesh_index)
                .with_arg("normal", normal_index));
        }
        let length_squared = normal.iter().map(|value| value * value).sum::<f32>();
        if (length_squared.sqrt() - 1.0).abs() > 1.0e-4 {
            return Err(MdlError::new("scene-invalid-normal-length")
                .with_arg("mesh", mesh_index)
                .with_arg("normal", normal_index)
                .with_arg("length", length_squared.sqrt()));
        }
    }
    for (set_index, uv_set) in mesh.uv_sets.iter().enumerate() {
        for (uv_index, uv) in uv_set.iter().enumerate() {
            if !uv.iter().all(|value| value.is_finite()) {
                return Err(MdlError::new("scene-non-finite-uv")
                    .with_arg("mesh", mesh_index)
                    .with_arg("set", set_index)
                    .with_arg("uv", uv_index));
            }
        }
    }
    for (triangle_index, triangle) in mesh.triangles.iter().enumerate() {
        for index in triangle {
            if *index as usize >= mesh.positions.len() {
                return Err(MdlError::new("scene-invalid-triangle-index")
                    .with_arg("mesh", mesh_index)
                    .with_arg("triangle", triangle_index)
                    .with_arg("index", index)
                    .with_arg("count", mesh.positions.len()));
            }
        }
    }
    validate_bounds(mesh.bounds, mesh_index)
}

fn validate_bounds(bounds: SceneBounds, mesh_index: usize) -> Result<(), MdlError> {
    if !bounds.min.iter().all(|value| value.is_finite())
        || !bounds.max.iter().all(|value| value.is_finite())
        || !bounds.center.iter().all(|value| value.is_finite())
        || (0..3).any(|axis| bounds.min[axis] > bounds.max[axis])
        || (0..3).any(|axis| bounds.center[axis] < bounds.min[axis])
        || (0..3).any(|axis| bounds.center[axis] > bounds.max[axis])
    {
        return Err(MdlError::new("scene-invalid-bounds").with_arg("mesh", mesh_index));
    }
    Ok(())
}

fn validate_draw(draw: &SceneDraw, draw_index: usize) -> Result<(), MdlError> {
    if !draw.geoset_alpha.is_finite()
        || !draw.layer_alpha.is_finite()
        || !draw.geoset_color.iter().all(|value| value.is_finite())
    {
        return Err(MdlError::new("scene-non-finite-draw").with_arg("draw", draw_index));
    }
    let transform = draw.texture_transform;
    if !transform.translation.iter().all(|value| value.is_finite())
        || !transform.rotation.iter().all(|value| value.is_finite())
        || !transform.scaling.iter().all(|value| value.is_finite())
    {
        return Err(
            MdlError::new("scene-non-finite-texture-transform").with_arg("draw", draw_index)
        );
    }
    let rotation_length_squared = transform
        .rotation
        .iter()
        .map(|value| value * value)
        .sum::<f32>();
    if !rotation_length_squared.is_finite() || rotation_length_squared <= f32::EPSILON {
        return Err(MdlError::new("scene-invalid-texture-rotation").with_arg("draw", draw_index));
    }
    Ok(())
}

fn strictly_increasing(values: impl IntoIterator<Item = u32>) -> bool {
    let mut previous = None;
    for value in values {
        if previous.is_some_and(|previous| previous >= value) {
            return false;
        }
        previous = Some(value);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::types::{PlaybackMode, ResolvedFrame};
    use crate::model::ids::{GeosetIndex, MaterialIndex, TextureIndex};
    use std::panic::{AssertUnwindSafe, catch_unwind};

    type InvalidCase = (
        &'static str,
        Box<dyn Fn() -> Result<ScenePacket, crate::error::MdlError>>,
    );

    fn bounds() -> SceneBounds {
        SceneBounds {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 0.0],
            center: [0.5, 0.5, 0.0],
        }
    }

    fn mesh(id: u32, uv_sets: usize) -> SceneMesh {
        SceneMesh {
            geoset: GeosetIndex(id),
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uv_sets: (0..uv_sets)
                .map(|set| vec![[set as f32, 0.0], [1.0, 0.0], [0.0, 1.0]])
                .collect(),
            triangles: vec![[0, 1, 2]],
            bounds: bounds(),
        }
    }

    fn draw(ordinal: u32, geoset: u32, coord_set: u32) -> SceneDraw {
        SceneDraw {
            source_ordinal: ordinal,
            geoset: GeosetIndex(geoset),
            mesh: 0,
            material: MaterialIndex(0),
            layer: 0,
            priority_plane: 0,
            geoset_color: [1.0; 3],
            geoset_alpha: 1.0,
            layer_alpha: 1.0,
            texture: Some(TextureIndex(0)),
            coord_set,
            texture_transform: TextureTransform::default(),
            filter_mode: SceneFilterMode::None,
            material_state: SceneMaterialState::default(),
            render_state: SceneRenderState::default(),
            sort_class: SceneSortClass::Stable,
        }
    }

    fn texture(index: u32) -> SceneTextureRequest {
        SceneTextureRequest {
            index: TextureIndex(index),
            filename: format!("texture-{index}.blp"),
            replaceable_id: 0,
            wrap_u: false,
            wrap_v: false,
        }
    }

    #[test]
    fn empty_packet_is_valid_schema_one() {
        let packet = ScenePacket::new(ResolvedFrame::default(), vec![], vec![], vec![]).unwrap();
        assert_eq!(packet.schema_version, SCENE_PACKET_SCHEMA_VERSION);
        assert!(packet.validate().is_ok());
    }

    #[test]
    fn checked_construction_sorts_stably_and_serializes_repeatably() {
        let packet = ScenePacket::new(
            ResolvedFrame {
                sequence: Some(2),
                sequence_frame: 12.5,
                global_frame: 99.25,
                playback: PlaybackMode::Clamp,
                view: None,
            },
            vec![mesh(7, 1), mesh(2, 2)],
            vec![draw(9, 7, 0), {
                let mut value = draw(3, 2, 1);
                value.mesh = 1;
                value
            }],
            vec![texture(5), texture(0)],
        )
        .unwrap();
        assert_eq!(
            packet.meshes.iter().map(|m| m.geoset.0).collect::<Vec<_>>(),
            [2, 7]
        );
        assert_eq!(
            packet
                .draws
                .iter()
                .map(|d| d.source_ordinal)
                .collect::<Vec<_>>(),
            [3, 9]
        );
        assert_eq!(
            packet
                .textures
                .iter()
                .map(|t| t.index.0)
                .collect::<Vec<_>>(),
            [0, 5]
        );
        let once = serde_json::to_string(&packet).unwrap();
        let twice = serde_json::to_string(&packet).unwrap();
        assert_eq!(once, twice);
        let decoded: ScenePacket = serde_json::from_str(&once).unwrap();
        assert_eq!(decoded, packet);
    }

    #[test]
    fn second_uv_set_and_u32_indices_are_preserved() {
        let mut large = mesh(4, 2);
        large.positions = vec![[0.0; 3]; 70_001];
        large.normals.clear();
        large.uv_sets = vec![vec![[0.0; 2]; 70_001], vec![[1.0; 2]; 70_001]];
        large.triangles = vec![[0, 65_536, 70_000]];
        let packet = ScenePacket::new(
            ResolvedFrame::default(),
            vec![large],
            vec![draw(0, 4, 1)],
            vec![texture(0)],
        )
        .unwrap();
        assert_eq!(packet.meshes[0].triangles[0][2], 70_000);
        assert_eq!(packet.draws[0].coord_set, 1);
    }

    #[test]
    fn normals_may_be_empty_or_full_length_only() {
        let mut absent = mesh(0, 1);
        absent.normals.clear();
        assert!(ScenePacket::new(ResolvedFrame::default(), vec![absent], vec![], vec![]).is_ok());
        let mut short = mesh(0, 1);
        short.normals.pop();
        assert_eq!(
            ScenePacket::new(ResolvedFrame::default(), vec![short], vec![], vec![])
                .unwrap_err()
                .key,
            "scene-invalid-normal-count"
        );
    }

    #[test]
    fn invalid_contract_inputs_return_stable_errors_without_panicking() {
        let cases: Vec<InvalidCase> = vec![
            (
                "scene-invalid-schema-version",
                Box::new(|| {
                    let mut packet =
                        ScenePacket::new(ResolvedFrame::default(), vec![], vec![], vec![])?;
                    packet.schema_version = 2;
                    packet.validate()?;
                    Ok(packet)
                }),
            ),
            (
                "scene-non-finite-frame",
                Box::new(|| {
                    ScenePacket::new(
                        ResolvedFrame {
                            sequence_frame: f64::NAN,
                            ..ResolvedFrame::default()
                        },
                        vec![],
                        vec![],
                        vec![],
                    )
                }),
            ),
            (
                "scene-duplicate-geoset",
                Box::new(|| {
                    ScenePacket::new(
                        ResolvedFrame::default(),
                        vec![mesh(0, 1), mesh(0, 1)],
                        vec![],
                        vec![],
                    )
                }),
            ),
            (
                "scene-duplicate-source-ordinal",
                Box::new(|| {
                    ScenePacket::new(
                        ResolvedFrame::default(),
                        vec![mesh(0, 1)],
                        vec![draw(1, 0, 0), draw(1, 0, 0)],
                        vec![texture(0)],
                    )
                }),
            ),
            (
                "scene-duplicate-texture-index",
                Box::new(|| {
                    ScenePacket::new(
                        ResolvedFrame::default(),
                        vec![],
                        vec![],
                        vec![texture(0), texture(0)],
                    )
                }),
            ),
            (
                "scene-invalid-uv-count",
                Box::new(|| {
                    let mut value = mesh(0, 1);
                    value.uv_sets[0].pop();
                    ScenePacket::new(ResolvedFrame::default(), vec![value], vec![], vec![])
                }),
            ),
            (
                "scene-invalid-triangle-index",
                Box::new(|| {
                    let mut value = mesh(0, 1);
                    value.triangles[0][2] = 3;
                    ScenePacket::new(ResolvedFrame::default(), vec![value], vec![], vec![])
                }),
            ),
            (
                "scene-non-finite-position",
                Box::new(|| {
                    let mut value = mesh(0, 1);
                    value.positions[0][0] = f32::NAN;
                    ScenePacket::new(ResolvedFrame::default(), vec![value], vec![], vec![])
                }),
            ),
            (
                "scene-non-finite-normal",
                Box::new(|| {
                    let mut value = mesh(0, 1);
                    value.normals[0][0] = f32::NAN;
                    ScenePacket::new(ResolvedFrame::default(), vec![value], vec![], vec![])
                }),
            ),
            (
                "scene-non-finite-uv",
                Box::new(|| {
                    let mut value = mesh(0, 1);
                    value.uv_sets[0][0][0] = f32::NAN;
                    ScenePacket::new(ResolvedFrame::default(), vec![value], vec![], vec![])
                }),
            ),
            (
                "scene-invalid-normal-length",
                Box::new(|| {
                    let mut value = mesh(0, 1);
                    value.normals[0] = [2.0, 0.0, 0.0];
                    ScenePacket::new(ResolvedFrame::default(), vec![value], vec![], vec![])
                }),
            ),
            (
                "scene-invalid-bounds",
                Box::new(|| {
                    let mut value = mesh(0, 1);
                    value.bounds.min[0] = 2.0;
                    ScenePacket::new(ResolvedFrame::default(), vec![value], vec![], vec![])
                }),
            ),
            (
                "scene-missing-draw-geoset",
                Box::new(|| {
                    ScenePacket::new(
                        ResolvedFrame::default(),
                        vec![],
                        vec![draw(0, 9, 0)],
                        vec![texture(0)],
                    )
                }),
            ),
            (
                "scene-invalid-mesh-index",
                Box::new(|| {
                    let mut value = draw(0, 0, 0);
                    value.mesh = 2;
                    ScenePacket::new(
                        ResolvedFrame::default(),
                        vec![mesh(0, 1)],
                        vec![value],
                        vec![texture(0)],
                    )
                }),
            ),
            (
                "scene-draw-mesh-geoset-mismatch",
                Box::new(|| {
                    let mut value = draw(0, 1, 0);
                    value.mesh = 0;
                    ScenePacket::new(
                        ResolvedFrame::default(),
                        vec![mesh(0, 1), mesh(1, 1)],
                        vec![value],
                        vec![texture(0)],
                    )
                }),
            ),
            (
                "scene-invalid-coord-set",
                Box::new(|| {
                    ScenePacket::new(
                        ResolvedFrame::default(),
                        vec![mesh(0, 0)],
                        vec![draw(0, 0, 0)],
                        vec![texture(0)],
                    )
                }),
            ),
            (
                "scene-missing-texture-request",
                Box::new(|| {
                    ScenePacket::new(
                        ResolvedFrame::default(),
                        vec![mesh(0, 1)],
                        vec![draw(0, 0, 0)],
                        vec![],
                    )
                }),
            ),
            (
                "scene-non-finite-draw",
                Box::new(|| {
                    let mut value = draw(0, 0, 0);
                    value.layer_alpha = f32::INFINITY;
                    ScenePacket::new(
                        ResolvedFrame::default(),
                        vec![mesh(0, 1)],
                        vec![value],
                        vec![texture(0)],
                    )
                }),
            ),
            (
                "scene-invalid-texture-rotation",
                Box::new(|| {
                    let mut value = draw(0, 0, 0);
                    value.texture_transform.rotation = [0.0; 4];
                    ScenePacket::new(
                        ResolvedFrame::default(),
                        vec![mesh(0, 1)],
                        vec![value],
                        vec![texture(0)],
                    )
                }),
            ),
            (
                "scene-non-finite-texture-transform",
                Box::new(|| {
                    let mut value = draw(0, 0, 0);
                    value.texture_transform.translation[0] = f32::NAN;
                    ScenePacket::new(
                        ResolvedFrame::default(),
                        vec![mesh(0, 1)],
                        vec![value],
                        vec![texture(0)],
                    )
                }),
            ),
        ];

        for (key, case) in cases {
            let result = catch_unwind(AssertUnwindSafe(case));
            assert!(result.is_ok(), "{key} panicked");
            assert_eq!(result.unwrap().unwrap_err().key, key);
        }
    }

    #[test]
    fn deserialized_packets_must_keep_canonical_order() {
        let packet = ScenePacket::new(
            ResolvedFrame::default(),
            vec![mesh(0, 1), mesh(1, 1)],
            vec![draw(0, 0, 0), {
                let mut value = draw(1, 1, 0);
                value.mesh = 1;
                value.texture = Some(TextureIndex(1));
                value
            }],
            vec![texture(0), texture(1)],
        )
        .unwrap();
        let mut mesh_order = packet.clone();
        mesh_order.meshes.swap(0, 1);
        assert_eq!(
            mesh_order.validate().unwrap_err().key,
            "scene-non-canonical-mesh-order"
        );
        let mut draw_order = packet.clone();
        draw_order.draws.swap(0, 1);
        assert_eq!(
            draw_order.validate().unwrap_err().key,
            "scene-non-canonical-draw-order"
        );
        let mut texture_order = packet;
        texture_order.textures.swap(0, 1);
        assert_eq!(
            texture_order.validate().unwrap_err().key,
            "scene-non-canonical-texture-order"
        );
    }

    #[test]
    fn tiny_texture_quaternion_and_out_of_bounds_center_are_rejected() {
        let mut tiny = draw(0, 0, 0);
        tiny.texture_transform.rotation = [f32::MIN_POSITIVE, 0.0, 0.0, 0.0];
        assert_eq!(
            ScenePacket::new(
                ResolvedFrame::default(),
                vec![mesh(0, 1)],
                vec![tiny],
                vec![texture(0)],
            )
            .unwrap_err()
            .key,
            "scene-invalid-texture-rotation"
        );

        let mut invalid_center = mesh(0, 1);
        invalid_center.bounds.center[0] = 2.0;
        assert_eq!(
            ScenePacket::new(
                ResolvedFrame::default(),
                vec![invalid_center],
                vec![],
                vec![],
            )
            .unwrap_err()
            .key,
            "scene-invalid-bounds"
        );
    }
}
