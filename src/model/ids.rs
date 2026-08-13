//! Stable identifiers and index newtypes for the M1 `Model` contract.
//!
//! Array position is not an identity. Object IDs may be sparse. Track,
//! parent, and global-sequence references use the original-file sentinels:
//! parent / track / global-seq `< 0` means "none".

#![cfg_attr(not(test), allow(dead_code))]

use serde::{Deserialize, Serialize};

/// MDX node ObjectID. Unique within one model; not an array index.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectId(pub u32);

/// Parent ObjectID. `NONE` (`-1`) means a root node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ParentId(pub i32);

impl ParentId {
    pub const NONE: Self = Self(-1);

    pub fn is_none(self) -> bool {
        self.0 < 0
    }
}

impl Default for ParentId {
    fn default() -> Self {
        Self::NONE
    }
}

/// Index into `Model.controllers`. `NONE` (`-1`) means a static value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId(pub i32);

impl TrackId {
    pub const NONE: Self = Self(-1);

    pub fn is_none(self) -> bool {
        self.0 < 0
    }

    pub fn from_index(index: usize) -> Self {
        Self(index as i32)
    }
}

impl Default for TrackId {
    fn default() -> Self {
        Self::NONE
    }
}

/// Index into `Model.global_sequences`. `< 0` means the track uses sequence time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GlobalSeqId(pub i32);

impl GlobalSeqId {
    pub const NONE: Self = Self(-1);

    pub fn is_none(self) -> bool {
        self.0 < 0
    }
}

impl Default for GlobalSeqId {
    fn default() -> Self {
        Self::NONE
    }
}

/// Index into `Model.geosets`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GeosetIndex(pub u32);

/// Index into `Model.materials`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MaterialIndex(pub u32);

/// Index into `Model.textures`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextureIndex(pub u32);

/// Index into `Model.geoset_anims`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GeosetAnimIndex(pub u32);

/// Index into `Model.texture_anims`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextureAnimIndex(pub u32);

/// Inclusive axis-aligned bounds used by MODL / SEQS / GEOS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Extent {
    pub bounds_radius: f32,
    pub minimum: [f32; 3],
    pub maximum: [f32; 3],
}
