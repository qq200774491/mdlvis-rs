use crate::model::ids::Extent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sequence {
    pub name: String,
    pub start_frame: u32,
    pub end_frame: u32,
    pub rarity: Option<u32>,
    pub non_looping: bool,
    pub move_speed: f32,
    pub extent: Extent,
}

impl Default for Sequence {
    fn default() -> Self {
        Self {
            name: String::new(),
            start_frame: 0,
            end_frame: 0,
            rarity: None,
            non_looping: false,
            move_speed: 0.0,
            extent: Extent::default(),
        }
    }
}
