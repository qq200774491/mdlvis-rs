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
                eprintln!("Unknown filter mode: {}, using None", value);
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

    pub fn name(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Transparent => "Transparent",
            Self::Blend => "Blend",
            Self::Additive => "Additive",
            Self::AddAlpha => "AddAlpha",
            Self::Modulate => "Modulate",
            Self::Modulate2x => "Modulate2x",
        }
    }

    pub fn label(&self) -> String {
        crate::i18n::t(match self {
            Self::None => "filter.none",
            Self::Transparent => "filter.transparent",
            Self::Blend => "filter.blend",
            Self::Additive => "filter.additive",
            Self::AddAlpha => "filter.add-alpha",
            Self::Modulate => "filter.modulate",
            Self::Modulate2x => "filter.modulate-2x",
        })
    }
}
