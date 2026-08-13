use crate::model::ids::TextureIndex;
use crate::scene::types::{SceneFilterMode, SceneRenderState, SceneSortClass};

#[derive(Debug, Clone)]
pub struct GeosetRenderInfo {
    pub index_start: u32,
    pub index_count: u32,
    pub material_id: Option<usize>,
    #[allow(dead_code)]
    pub vertices: Vec<[f32; 3]>,
    #[allow(dead_code)]
    pub faces: Vec<Vec<u32>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BlendFactor {
    Zero,
    One,
    SrcAlpha,
    OneMinusSrcAlpha,
    Src,
    Dst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScenePipelineState {
    pub blend: Option<(BlendFactor, BlendFactor)>,
    pub alpha_cutoff: bool,
    pub depth_write: bool,
    pub depth_always: bool,
    pub cull_back: bool,
}

impl ScenePipelineState {
    pub(crate) fn from_scene(filter: SceneFilterMode, state: SceneRenderState) -> Self {
        let (blend, alpha_cutoff, filter_depth_write) = match filter {
            SceneFilterMode::None => (None, false, true),
            SceneFilterMode::Transparent => (None, true, true),
            SceneFilterMode::Blend => (
                Some((BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha)),
                false,
                false,
            ),
            SceneFilterMode::Additive => (Some((BlendFactor::One, BlendFactor::One)), false, false),
            SceneFilterMode::AddAlpha => (
                Some((BlendFactor::SrcAlpha, BlendFactor::One)),
                false,
                false,
            ),
            SceneFilterMode::Modulate => {
                (Some((BlendFactor::Dst, BlendFactor::Zero)), false, false)
            }
            SceneFilterMode::Modulate2x => {
                (Some((BlendFactor::Dst, BlendFactor::Src)), false, false)
            }
        };
        Self {
            blend,
            alpha_cutoff,
            depth_write: filter_depth_write && !state.no_depth_write,
            depth_always: state.no_depth_test,
            cull_back: !state.two_sided,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PreparedDraw {
    pub source_ordinal: u32,
    pub priority_plane: i32,
    pub pass_rank: u8,
    pub index_start: u32,
    pub index_count: u32,
    pub texture: Option<TextureIndex>,
    pub texture_slot: Option<u32>,
    pub bounds_center: [f32; 3],
    pub sort_class: SceneSortClass,
    pub triangle_ordinal: u32,
    pub geoset: u32,
    pub pipeline: ScenePipelineState,
    pub uniform_offset: u32,
}

pub(crate) fn pass_rank(filter: SceneFilterMode) -> u8 {
    match filter {
        SceneFilterMode::None | SceneFilterMode::Transparent => 0,
        SceneFilterMode::Blend => 1,
        SceneFilterMode::Additive
        | SceneFilterMode::AddAlpha
        | SceneFilterMode::Modulate
        | SceneFilterMode::Modulate2x => 2,
    }
}
