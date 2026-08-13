//! Shared node identity, flags, and object-type bits.
//!
//! Low 8 bits are inherit / billboard / camera flags (`MDXReadOBJ`).
//! High bits are the object type (`typHELP` … `typPRE2`).

#![cfg_attr(not(test), allow(dead_code))]

use crate::model::ids::{ObjectId, ParentId, TrackId};
use serde::{Deserialize, Serialize};

pub const TYPE_HELP: u32 = 0;
pub const TYPE_BONE: u32 = 256;
pub const TYPE_LITE: u32 = 512;
pub const TYPE_EVTS: u32 = 1024;
pub const TYPE_ATCH: u32 = 2048;
pub const TYPE_CLID: u32 = 8192;
pub const TYPE_PRE2: u32 = 65536;

const DONT_INHERIT_TRANSLATION: u32 = 1;
const DONT_INHERIT_SCALING: u32 = 2;
const DONT_INHERIT_ROTATION: u32 = 4;
const BILLBOARDED: u32 = 8;
const BILLBOARD_LOCK_X: u32 = 16;
const BILLBOARD_LOCK_Y: u32 = 32;
const BILLBOARD_LOCK_Z: u32 = 64;
const CAMERA_ANCHORED: u32 = 128;
const TYPE_MASK: u32 = !0xFF;

/// Object-type discriminant stored in the high bits of the node flags word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    Helper,
    Bone,
    Light,
    Event,
    Attachment,
    Collision,
    ParticleEmitter2,
    Other(u32),
}

impl NodeKind {
    pub fn from_type_bits(bits: u32) -> Self {
        match bits {
            TYPE_HELP => Self::Helper,
            TYPE_BONE => Self::Bone,
            TYPE_LITE => Self::Light,
            TYPE_EVTS => Self::Event,
            TYPE_ATCH => Self::Attachment,
            TYPE_CLID => Self::Collision,
            TYPE_PRE2 => Self::ParticleEmitter2,
            other => Self::Other(other),
        }
    }

    pub fn type_bits(self) -> u32 {
        match self {
            Self::Helper => TYPE_HELP,
            Self::Bone => TYPE_BONE,
            Self::Light => TYPE_LITE,
            Self::Event => TYPE_EVTS,
            Self::Attachment => TYPE_ATCH,
            Self::Collision => TYPE_CLID,
            Self::ParticleEmitter2 => TYPE_PRE2,
            Self::Other(bits) => bits,
        }
    }
}

/// Packed node flags from the MDX object header.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeFlags(pub u32);

impl NodeFlags {
    pub fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub fn bits(self) -> u32 {
        self.0
    }

    pub fn dont_inherit_translation(self) -> bool {
        self.0 & DONT_INHERIT_TRANSLATION != 0
    }

    pub fn dont_inherit_scaling(self) -> bool {
        self.0 & DONT_INHERIT_SCALING != 0
    }

    pub fn dont_inherit_rotation(self) -> bool {
        self.0 & DONT_INHERIT_ROTATION != 0
    }

    pub fn billboarded(self) -> bool {
        self.0 & BILLBOARDED != 0
    }

    pub fn billboard_lock_x(self) -> bool {
        self.0 & BILLBOARD_LOCK_X != 0
    }

    pub fn billboard_lock_y(self) -> bool {
        self.0 & BILLBOARD_LOCK_Y != 0
    }

    pub fn billboard_lock_z(self) -> bool {
        self.0 & BILLBOARD_LOCK_Z != 0
    }

    pub fn camera_anchored(self) -> bool {
        self.0 & CAMERA_ANCHORED != 0
    }

    pub fn kind(self) -> NodeKind {
        NodeKind::from_type_bits(self.0 & TYPE_MASK)
    }
}

/// Shared node header for every typed object that owns a `TBone` skeleton.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeRef {
    pub name: String,
    pub object_id: ObjectId,
    pub parent_id: ParentId,
    pub flags: NodeFlags,
    pub translation: TrackId,
    pub rotation: TrackId,
    pub scaling: TrackId,
    pub visibility: TrackId,
}
