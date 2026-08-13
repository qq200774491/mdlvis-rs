use super::{FilterMode, ShadingFlags};
use crate::model::objects::{LayerRef, MaterialFlags};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Material {
    pub layers: Vec<Layer>,
    pub priority_plane: i32,
    pub flags: MaterialFlags,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            layers: Vec::new(),
            priority_plane: 0,
            flags: MaterialFlags::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub texture_id: Option<usize>,
    pub filter_mode: FilterMode,
    pub shading_flags: Vec<ShadingFlags>,
    pub alpha: f32,
    pub extra: LayerRef,
    pub alpha_track: i32,
    pub texture_id_track: i32,

    // Runtime overrides (not serialized, only for UI)
    #[serde(skip)]
    pub enabled: bool,
    #[serde(skip)]
    pub alpha_override: Option<f32>,
    #[serde(skip)]
    pub filter_mode_override: Option<FilterMode>,
    #[serde(skip)]
    pub shading_flags_override: Option<Vec<ShadingFlags>>,
}

impl Layer {
    /// Get effective enabled state (for rendering)
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get effective alpha value (override or original)
    pub fn get_alpha(&self) -> f32 {
        self.alpha_override.unwrap_or(self.alpha)
    }

    /// Get effective filter mode (override or original)
    pub fn get_filter_mode(&self) -> FilterMode {
        self.filter_mode_override
            .clone()
            .unwrap_or(self.filter_mode.clone())
    }

    /// Get effective shading flags (override or original)
    pub fn get_shading_flags(&self) -> Vec<ShadingFlags> {
        self.shading_flags_override
            .clone()
            .unwrap_or(self.shading_flags.clone())
    }
}
