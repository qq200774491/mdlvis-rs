//! Identified MDX object types that are not yet filled by the parser.
//!
//! Empty `Vec`s on `Model` mean "recognized, zero records so far", not
//! "this chunk does not exist". Presence of the on-disk chunk is still
//! reported by the verification inspector until a reader lands.

#![cfg_attr(not(test), allow(dead_code))]

use crate::model::ids::{
    Extent, GeosetIndex, GlobalSeqId, MaterialIndex, TextureAnimIndex, TextureIndex, TrackId,
};
use crate::model::node::NodeRef;
use serde::{Deserialize, Serialize};

/// `GLBS` record: duration in frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalSequence {
    pub duration: u32,
}

/// `GEOA` record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GeosetAnim {
    pub geoset_id: Option<GeosetIndex>,
    pub alpha: f32,
    pub color: [f32; 3],
    pub drop_shadow: bool,
    pub alpha_track: TrackId,
    pub color_track: TrackId,
}

/// `TXAN` record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextureAnim {
    pub translation: TrackId,
    pub rotation: TrackId,
    pub scaling: TrackId,
}

/// Texture wrap bits from `TEXS` (bit0 width, bit1 height).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureFlags {
    pub wrap_width: bool,
    pub wrap_height: bool,
}

impl TextureFlags {
    pub fn from_bits(bits: u32) -> Self {
        Self {
            wrap_width: bits & 1 != 0,
            wrap_height: bits & 2 != 0,
        }
    }

    pub fn bits(self) -> u32 {
        u32::from(self.wrap_width) | (u32::from(self.wrap_height) << 1)
    }
}

/// Material header bits currently discarded after the layer loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialFlags {
    pub constant_color: bool,
    pub sort_primitives_far_z: bool,
    pub full_resolution: bool,
}

/// Extra layer fields the current reader skips (`TVertexAnimID`, `CoordID`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerRef {
    pub texture_anim_id: Option<TextureAnimIndex>,
    pub coord_id: u32,
}

/// `ATCH` record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub node: NodeRef,
    pub path: String,
    pub attachment_id: i32,
}

/// Disk light type (0 omnidirectional, 1 directional, 2 ambient).
/// Original `MDXReadLights` stores `ReadLong+1`; this crate keeps the disk value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum LightType {
    #[default]
    Omnidirectional = 0,
    Directional = 1,
    Ambient = 2,
}

impl LightType {
    pub fn from_disk(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Omnidirectional),
            1 => Some(Self::Directional),
            2 => Some(Self::Ambient),
            _ => None,
        }
    }
}

/// `LITE` record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Light {
    pub node: NodeRef,
    pub light_type: LightType,
    pub attenuation_start: f32,
    pub attenuation_end: f32,
    pub color: [f32; 3],
    pub intensity: f32,
    pub ambient_color: [f32; 3],
    pub ambient_intensity: f32,
    pub attenuation_start_track: TrackId,
    pub attenuation_end_track: TrackId,
    pub intensity_track: TrackId,
    pub color_track: TrackId,
    pub ambient_color_track: TrackId,
    pub ambient_intensity_track: TrackId,
}

/// `PREM` record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ParticleEmitter {
    pub node: NodeRef,
    pub emission_rate: f32,
    pub gravity: f32,
    pub longitude: f32,
    pub latitude: f32,
    pub life_span: f32,
    pub init_velocity: f32,
    pub path: String,
}

/// `PRE2` emitter flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticleEmitter2Flags {
    pub sort_primitives_far_z: bool,
    pub unshaded: bool,
    pub line_emitter: bool,
    pub unfogged: bool,
    pub model_space: bool,
    pub xy_quad: bool,
}

/// `PRE2` record. Filter/blend stays a raw disk integer until the particle
/// reader lands; it is not the material `FilterMode`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ParticleEmitter2 {
    pub node: NodeRef,
    pub flags: ParticleEmitter2Flags,
    pub speed: f32,
    pub variation: f32,
    pub latitude: f32,
    pub gravity: f32,
    pub life_span: f32,
    pub emission_rate: f32,
    pub width: f32,
    pub length: f32,
    pub squirt: bool,
    pub blend_mode: u32,
    pub rows: u32,
    pub columns: u32,
    pub particle_type: u32,
    pub tail_length: f32,
    pub time: f32,
    pub segment_color: [[f32; 3]; 3],
    pub alpha: [u8; 3],
    pub particle_scaling: [f32; 3],
    pub texture_id: Option<TextureIndex>,
    pub replaceable_id: u32,
    pub priority_plane: i32,
}

/// `RIBB` record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RibbonEmitter {
    pub node: NodeRef,
    pub height_above: f32,
    pub height_below: f32,
    pub alpha: f32,
    pub color: [f32; 3],
    pub texture_slot: i32,
    pub emission_rate: u32,
    pub life_span: f32,
    pub gravity: f32,
    pub rows: u32,
    pub columns: u32,
    pub material_id: Option<MaterialIndex>,
}

/// `CAMS` record. Cameras are not skeleton nodes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub name: String,
    pub position: [f32; 3],
    pub field_of_view: f32,
    pub far_clip: f32,
    pub near_clip: f32,
    pub target_position: [f32; 3],
    pub translation: TrackId,
    pub rotation: TrackId,
    pub target_translation: TrackId,
}

/// `EVTS` record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EventObject {
    pub node: NodeRef,
    pub global_seq_id: GlobalSeqId,
    pub tracks: Vec<i32>,
}

/// `CLID` shape kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CollisionType {
    #[default]
    Box,
    Sphere,
}

/// `CLID` record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CollisionShape {
    pub node: NodeRef,
    pub kind: CollisionType,
    pub vertices: Vec<[f32; 3]>,
    pub bounds_radius: f32,
}

/// Sequence fields the current reader still drops (move speed, extent).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SequenceExtras {
    pub move_speed: f32,
    pub extent: Extent,
}
