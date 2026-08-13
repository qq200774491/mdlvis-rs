use serde::{Deserialize, Serialize};

/// Filter mode for material layers
/// Mapping according to MDX specification:
/// 0 = None
/// 1 = Transparent
/// 2 = Blend
/// 3 = Additive
/// 4 = AddAlpha
/// 5 = Modulate
/// 6 = Modulate2x
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterMode {
    None,        // 0
    Transparent, // 1
    Blend,       // 2
    Additive,    // 3
    AddAlpha,    // 4
    Modulate,    // 5
    Modulate2x,  // 6
}

impl FilterMode {
    /// Parse FilterMode from u32 value (MDX binary format)
    pub fn from_u32(value: u32) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Transparent,
            2 => Self::Blend,
            3 => Self::Additive,
            4 => Self::AddAlpha,
            5 => Self::Modulate,
            6 => Self::Modulate2x,
            _ => {
                eprintln!("未知过滤模式：{}，已使用“无”", value);
                Self::None
            }
        }
    }

    /// Convert FilterMode to f32 for shader uniform
    pub fn to_f32(&self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Transparent => 1.0,
            Self::Blend => 2.0,
            Self::Additive => 3.0,
            Self::AddAlpha => 4.0,
            Self::Modulate => 5.0,
            Self::Modulate2x => 6.0,
        }
    }

    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Self::None => "无",
            Self::Transparent => "透明",
            Self::Blend => "混合",
            Self::Additive => "加法",
            Self::AddAlpha => "加法透明",
            Self::Modulate => "调制",
            Self::Modulate2x => "双倍调制",
        }
    }
}
