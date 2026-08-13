//! M1-W0-MODEL-01 structure tests. Compiled only for the test bin.

use super::chunk::{UnknownChunk, is_identified_chunk};
use super::ids::{
    Extent, GeosetAnimIndex, GeosetIndex, GlobalSeqId, MaterialIndex, ObjectId, ParentId,
    TextureAnimIndex, TextureIndex, TrackId,
};
use super::model::Model;
use super::node::{NodeFlags, NodeKind, NodeRef, TYPE_ATCH, TYPE_BONE};
use super::objects::{
    LayerRef, LightType, MaterialFlags, ParticleEmitter, ParticleEmitter2, ParticleEmitterUses,
    RibbonEmitter, SequenceExtras, TextureFlags,
};
use super::tracks::{InterpolationType, TrackKind};

#[test]
fn default_model_has_identified_collections() {
    let model = Model::default();
    assert!(model.global_sequences.is_empty());
    assert!(model.geoset_anims.is_empty());
    assert!(model.texture_anims.is_empty());
    assert!(model.attachments.is_empty());
    assert!(model.lights.is_empty());
    assert!(model.cameras.is_empty());
    assert!(model.particle_emitters.is_empty());
    assert!(model.particle_emitters_2.is_empty());
    assert!(model.ribbons.is_empty());
    assert!(model.events.is_empty());
    assert!(model.collisions.is_empty());
    assert!(model.pivot_points.is_empty());
    assert!(model.mdlvis_data.is_none());
    assert!(model.unknown_chunks.is_empty());
    assert_eq!(model.blend_time, 0);
}

#[test]
fn particle_and_ribbon_tracks_default_to_none() {
    let emitter = ParticleEmitter::default();
    assert_eq!(emitter.uses_type, ParticleEmitterUses::Tga);
    for track in [
        emitter.emission_rate_track,
        emitter.gravity_track,
        emitter.longitude_track,
        emitter.latitude_track,
        emitter.life_span_track,
        emitter.init_velocity_track,
    ] {
        assert!(track.is_none());
    }

    let ribbon = RibbonEmitter::default();
    for track in [
        ribbon.height_above_track,
        ribbon.height_below_track,
        ribbon.alpha_track,
        ribbon.color_track,
        ribbon.texture_slot_track,
    ] {
        assert!(track.is_none());
    }
}

#[test]
fn mdlvis_payload_distinguishes_absent_and_empty_chunks() {
    let absent = Model::default();
    let present = Model {
        mdlvis_data: Some(Vec::new()),
        ..Model::default()
    };
    assert_ne!(
        serde_json::to_value(absent).expect("serialize absent MDVI"),
        serde_json::to_value(present).expect("serialize empty MDVI")
    );
}

#[test]
fn reference_sentinels_are_negative() {
    assert!(ParentId::NONE.is_none());
    assert!(TrackId::NONE.is_none());
    assert!(GlobalSeqId::NONE.is_none());
    assert!(!TrackId::from_index(0).is_none());
    assert_ne!(ObjectId(7).0 as usize, 0, "object id is not an array index");
}

#[test]
fn particle_emitter_2_tracks_default_to_none() {
    let emitter = ParticleEmitter2::default();
    for track in [
        emitter.speed_track,
        emitter.variation_track,
        emitter.latitude_track,
        emitter.gravity_track,
        emitter.emission_rate_track,
        emitter.width_track,
        emitter.length_track,
    ] {
        assert!(track.is_none());
    }
}

#[test]
fn node_flags_match_original_low_bits() {
    let flags = NodeFlags::from_bits(1 | 4 | 8 | 16 | TYPE_BONE);
    assert!(flags.dont_inherit_translation());
    assert!(!flags.dont_inherit_scaling());
    assert!(flags.dont_inherit_rotation());
    assert!(flags.billboarded());
    assert!(flags.billboard_lock_x());
    assert!(!flags.billboard_lock_y());
    assert!(!flags.camera_anchored());
    assert_eq!(flags.kind(), NodeKind::Bone);
    assert_eq!(NodeFlags::from_bits(TYPE_ATCH).kind(), NodeKind::Attachment);
    let locked = NodeFlags::from_bits(64);
    assert!(locked.billboard_lock_z());
    assert_eq!(locked.bits(), 64);
    assert_eq!(NodeKind::Bone.type_bits(), TYPE_BONE);
    let _node = NodeRef::default();
}

#[test]
fn interpolation_and_track_kinds() {
    assert_eq!(
        InterpolationType::from_u32(2),
        Some(InterpolationType::Hermite)
    );
    assert!(InterpolationType::Bezier.has_tangents());
    assert!(!InterpolationType::Linear.has_tangents());
    assert_eq!(InterpolationType::None.to_u32(), 0);
    assert_eq!(TrackKind::Rotation.element_count(), 4);
    assert!(TrackKind::Rotation.is_quaternion());
    assert!(!TrackKind::Translation.is_quaternion());
}

#[test]
fn unknown_pocket_rejects_identified_fourcc() {
    assert!(is_identified_chunk(b"GLBS"));
    assert!(is_identified_chunk(b"GEOA"));
    assert!(is_identified_chunk(b"TXAN"));
    assert!(is_identified_chunk(b"ATCH"));
    assert!(!is_identified_chunk(b"FAKE"));
    let pocket = UnknownChunk::new(*b"FAKE", vec![1, 2, 3, 4]);
    assert_eq!(pocket.fourcc_str(), "FAKE");
    assert_eq!(pocket.size(), 4);
}

#[test]
fn texture_wrap_flags() {
    let flags = TextureFlags::from_bits(3);
    assert!(flags.wrap_width);
    assert!(flags.wrap_height);
    assert_eq!(flags.bits(), 3);
}

#[test]
fn extras_and_indexes_exist() {
    let _geoset = GeosetIndex(0);
    let _material = MaterialIndex(0);
    let _texture = TextureIndex(0);
    let _geoset_anim = GeosetAnimIndex(0);
    let _texture_anim = TextureAnimIndex(1);
    assert_eq!(LightType::from_disk(0), Some(LightType::Omnidirectional));
    assert_eq!(LightType::from_disk(2), Some(LightType::Ambient));
    assert_eq!(LightType::from_disk(9), None);
    let material_flags = MaterialFlags {
        constant_color: true,
        sort_primitives_far_z: false,
        full_resolution: false,
    };
    assert!(material_flags.constant_color);
    let layer = LayerRef {
        texture_anim_id: Some(TextureAnimIndex(0)),
        coord_id: 1,
    };
    assert_eq!(layer.coord_id, 1);
    let extras = SequenceExtras {
        move_speed: 200.0,
        extent: Extent::default(),
    };
    assert_eq!(extras.move_speed, 200.0);
}
