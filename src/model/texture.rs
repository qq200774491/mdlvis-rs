use serde::{Deserialize, Serialize};

use crate::model::objects::TextureFlags;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Texture {
    pub filename: String,
    pub replaceable_id: u32, // 0 = normal texture, 1 = team color, 2 = team glow, etc.
    pub flags: TextureFlags,
    pub image_data: Option<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}
