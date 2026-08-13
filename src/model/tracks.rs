//! Track interpolation and semantic kinds.
//!
//! These enums document the disk values. Existing `AnimationController`
//! still stores `interpolation_type: u32` so the current parser and
//! animation init path stay unchanged.

#![cfg_attr(not(test), allow(dead_code))]

use serde::{Deserialize, Serialize};

/// Disk interpolation values 0–3 (`DontInterp` / `Linear` / `Hermite` / `Bezier`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum InterpolationType {
    #[default]
    None = 0,
    Linear = 1,
    Hermite = 2,
    Bezier = 3,
}

impl InterpolationType {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Linear),
            2 => Some(Self::Hermite),
            3 => Some(Self::Bezier),
            _ => None,
        }
    }

    pub fn to_u32(self) -> u32 {
        self as u32
    }

    pub fn has_tangents(self) -> bool {
        matches!(self, Self::Hermite | Self::Bezier)
    }
}

/// What a controller samples. Rotation is a quaternion; others are scalars or vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackKind {
    Translation,
    Rotation,
    Scaling,
    Visibility,
    Alpha,
    TextureId,
    Color,
    TextureTranslation,
    TextureRotation,
    TextureScaling,
}

impl TrackKind {
    pub fn element_count(self) -> usize {
        match self {
            Self::Visibility | Self::Alpha | Self::TextureId => 1,
            Self::Color
            | Self::Translation
            | Self::Scaling
            | Self::TextureTranslation
            | Self::TextureScaling => 3,
            Self::Rotation | Self::TextureRotation => 4,
        }
    }

    pub fn is_quaternion(self) -> bool {
        matches!(self, Self::Rotation | Self::TextureRotation)
    }
}
