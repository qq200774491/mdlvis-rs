use crate::material::Material;
use crate::model::animation::Sequence;
use crate::model::chunk::UnknownChunk;
use crate::model::geoset::Geoset;
use crate::model::ids::Extent;
use crate::model::objects::{
    Attachment, Camera, CollisionShape, EventObject, GeosetAnim, GlobalSequence, Light,
    ParticleEmitter, ParticleEmitter2, RibbonEmitter, TextureAnim,
};
use crate::model::skeleton::{AnimationController, Bone, Helper};
use crate::model::texture::Texture;
use serde::{Deserialize, Serialize};

/// Normalized model graph.
///
/// Identified MDX blocks have typed collections even when the current
/// reader cannot fill them. An empty `Vec` means "zero records modeled",
/// not "this fourCC is unknown". Unrecognized fourCCs go in
/// `unknown_chunks` as opaque bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub name: String,
    pub blend_time: u32,
    pub extent: Extent,
    pub geosets: Vec<Geoset>,
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
    pub sequences: Vec<Sequence>,
    pub bones: Vec<Bone>,
    pub helpers: Vec<Helper>,
    pub controllers: Vec<AnimationController>,
    pub pivot_points: Vec<[f32; 3]>,
    pub global_sequences: Vec<GlobalSequence>,
    pub geoset_anims: Vec<GeosetAnim>,
    pub texture_anims: Vec<TextureAnim>,
    pub attachments: Vec<Attachment>,
    pub lights: Vec<Light>,
    pub cameras: Vec<Camera>,
    pub particle_emitters: Vec<ParticleEmitter>,
    pub particle_emitters_2: Vec<ParticleEmitter2>,
    pub ribbons: Vec<RibbonEmitter>,
    pub events: Vec<EventObject>,
    pub collisions: Vec<CollisionShape>,
    /// Payload bytes of the identified MDVI chunk, excluding fourCC and size.
    /// `None` means absent; `Some(vec![])` means a present zero-length chunk.
    pub mdlvis_data: Option<Vec<u8>>,
    pub unknown_chunks: Vec<UnknownChunk>,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            name: String::new(),
            blend_time: 0,
            extent: Extent::default(),
            geosets: Vec::new(),
            materials: Vec::new(),
            textures: Vec::new(),
            sequences: Vec::new(),
            bones: Vec::new(),
            helpers: Vec::new(),
            controllers: Vec::new(),
            pivot_points: Vec::new(),
            global_sequences: Vec::new(),
            geoset_anims: Vec::new(),
            texture_anims: Vec::new(),
            attachments: Vec::new(),
            lights: Vec::new(),
            cameras: Vec::new(),
            particle_emitters: Vec::new(),
            particle_emitters_2: Vec::new(),
            ribbons: Vec::new(),
            events: Vec::new(),
            collisions: Vec::new(),
            mdlvis_data: None,
            unknown_chunks: Vec::new(),
        }
    }
}
