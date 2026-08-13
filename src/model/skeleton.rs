use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bone {
    pub name: String,
    pub object_id: u32,
    pub parent_id: i32, // -1 means no parent
    pub pivot_point: [f32; 3],
    pub geoset_id: Option<u32>,
    pub geoset_anim_id: Option<u32>,
    pub flags: u32,
    // Animation controller indices (-1 if not animated)
    pub translation_idx: i32,
    pub rotation_idx: i32,
    pub scaling_idx: i32,
    pub visibility_idx: i32,
}

impl Default for Bone {
    fn default() -> Self {
        Self {
            name: String::new(),
            object_id: 0,
            parent_id: -1,
            pivot_point: [0.0, 0.0, 0.0],
            geoset_id: None,
            geoset_anim_id: None,
            flags: 0,
            translation_idx: -1,
            rotation_idx: -1,
            scaling_idx: -1,
            visibility_idx: -1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Helper {
    pub name: String,
    pub object_id: u32,
    pub parent_id: i32, // -1 means no parent
    pub pivot_point: [f32; 3],
    pub flags: u32,
    // Animation controller indices
    pub translation_idx: i32,
    pub rotation_idx: i32,
    pub scaling_idx: i32,
    pub visibility_idx: i32,
}

impl Default for Helper {
    fn default() -> Self {
        Self {
            name: String::new(),
            object_id: 0,
            parent_id: -1,
            pivot_point: [0.0, 0.0, 0.0],
            flags: 0,
            translation_idx: -1,
            rotation_idx: -1,
            scaling_idx: -1,
            visibility_idx: -1,
        }
    }
}

/// Animation controller data (keyframes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationController {
    pub interpolation_type: u32, // 0=None, 1=Linear, 2=Hermite, 3=Bezier
    pub global_seq_id: i32,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keyframe {
    pub frame: i32,
    pub data: Vec<f32>,
    pub in_tan: Vec<f32>,
    pub out_tan: Vec<f32>,
}
