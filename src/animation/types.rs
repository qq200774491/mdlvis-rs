// Animation data types
// Based on Delphi mdlwork.pas types

use nalgebra_glm as glm;

/// Controller item - single keyframe data
/// From TContItem in mdlwork.pas
#[derive(Debug, Clone)]
pub struct ControllerItem {
    pub frame: i32,        // Frame number
    pub data: Vec<f32>,    // Data (translation, rotation, scaling, etc.)
    pub in_tan: Vec<f32>,  // In tangent (for Hermite/Bezier interpolation)
    pub out_tan: Vec<f32>, // Out tangent
}

/// Controller type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ControllerType {
    Translation, // ctTranslation
    Rotation,    // ctRotation
    Scaling,     // ctScaling
    Alpha,       // ctAlpha
    DontInterp,  // Don't interpolate - use nearest frame
    Linear,      // Linear interpolation
    Hermite,     // Hermite (smooth) interpolation
    Bezier,      // Bezier interpolation
}

/// Animation controller
/// From TController in mdlwork.pas
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Controller {
    pub cont_type: ControllerType,  // Type of controller
    pub global_seq_id: i32,         // ID of global sequence (-1 if none)
    pub items: Vec<ControllerItem>, // Keyframes
}

impl Controller {
    /// Get frame data using interpolation
    pub fn get_frame_data(&self, frame: i32) -> Vec<f32> {
        if self.items.is_empty() {
            return vec![0.0; 4]; // Default values
        }

        // Find surrounding keyframes
        let mut before_idx = None;
        let mut after_idx = None;

        for (i, item) in self.items.iter().enumerate() {
            if item.frame <= frame {
                before_idx = Some(i);
            }
            if item.frame >= frame && after_idx.is_none() {
                after_idx = Some(i);
                break;
            }
        }

        // Handle edge cases
        let before_idx = match before_idx {
            Some(idx) => idx,
            None => {
                // Before first frame - use first frame data (base pose)
                return self.items[0].data.clone();
            }
        };

        let after_idx = match after_idx {
            Some(idx) => idx,
            None => {
                // After last frame - use last frame data
                return self.items.last().unwrap().data.clone();
            }
        };

        // Exact frame match
        if before_idx == after_idx {
            return self.items[before_idx].data.clone();
        }

        // Interpolate between frames
        let before = &self.items[before_idx];
        let after = &self.items[after_idx];
        let t = (frame - before.frame) as f32 / (after.frame - before.frame) as f32;

        match self.cont_type {
            ControllerType::DontInterp => before.data.clone(),
            ControllerType::Linear
            | ControllerType::Translation
            | ControllerType::Scaling
            | ControllerType::Alpha => {
                // Linear interpolation
                before
                    .data
                    .iter()
                    .zip(after.data.iter())
                    .map(|(b, a)| b + (a - b) * t)
                    .collect()
            }
            ControllerType::Rotation => {
                // SLERP for quaternions
                if before.data.len() >= 4 && after.data.len() >= 4 {
                    let q1 = glm::quat(
                        before.data[3],
                        before.data[0],
                        before.data[1],
                        before.data[2],
                    );
                    let q2 = glm::quat(after.data[3], after.data[0], after.data[1], after.data[2]);
                    let result = glm::quat_slerp(&q1, &q2, t);
                    vec![result.i, result.j, result.k, result.w]
                } else {
                    before.data.clone()
                }
            }
            ControllerType::Hermite | ControllerType::Bezier => {
                // Hermite/Bezier interpolation using tangents
                // Simplified - can be improved with actual Hermite formula
                let t2 = t * t;
                let t3 = t2 * t;
                let h1 = 2.0 * t3 - 3.0 * t2 + 1.0;
                let h2 = -2.0 * t3 + 3.0 * t2;
                let h3 = t3 - 2.0 * t2 + t;
                let h4 = t3 - t2;

                before
                    .data
                    .iter()
                    .enumerate()
                    .map(|(i, b)| {
                        let a = after.data.get(i).copied().unwrap_or(0.0);
                        let out_t = before.out_tan.get(i).copied().unwrap_or(0.0);
                        let in_t = after.in_tan.get(i).copied().unwrap_or(0.0);
                        h1 * b + h2 * a + h3 * out_t + h4 * in_t
                    })
                    .collect()
            }
        }
    }
}

/// Bone transformation state
/// From TBone in mdlwork.pas
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BoneState {
    pub name: String,
    pub object_id: i32,
    pub parent: i32, // Parent bone ID (-1 if root)

    // Controller indices (-1 if no animation)
    pub translation_idx: i32,
    pub rotation_idx: i32,
    pub scaling_idx: i32,
    pub visibility_idx: i32,

    // Billboard flags
    pub is_billboarded: bool,
    pub billboard_lock_x: bool,
    pub billboard_lock_y: bool,
    pub billboard_lock_z: bool,
    pub camera_anchored: bool,

    // Current animated values (computed)
    pub is_ready: bool,            // True if already calculated this frame
    pub abs_quaternion: glm::Quat, // Absolute rotation quaternion
    pub abs_matrix: glm::Mat3,     // Absolute rotation matrix (with scaling)
    pub abs_vector: glm::Vec3,     // Absolute position
    pub abs_scaling: glm::Vec3,    // Absolute scaling
    pub visible: bool,             // Visibility flag
}

impl Default for BoneState {
    fn default() -> Self {
        Self {
            name: String::new(),
            object_id: 0,
            parent: -1,
            translation_idx: -1,
            rotation_idx: -1,
            scaling_idx: -1,
            visibility_idx: -1,
            is_billboarded: false,
            billboard_lock_x: false,
            billboard_lock_y: false,
            billboard_lock_z: false,
            camera_anchored: false,
            is_ready: false,
            abs_quaternion: glm::quat_identity(),
            abs_matrix: glm::identity(),
            abs_vector: glm::vec3(0.0, 0.0, 0.0),
            abs_scaling: glm::vec3(1.0, 1.0, 1.0),
            visible: true,
        }
    }
}

impl BoneState {
    pub fn new(name: String, object_id: i32) -> Self {
        Self {
            name,
            object_id,
            ..Default::default()
        }
    }
}

/// Texture animation data
/// From TTextureAnim in mdlwork.pas
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TextureAnim {
    pub translation_graph: i32,
    pub rotation_graph: i32,
    pub scaling_graph: i32,
}

impl Default for TextureAnim {
    fn default() -> Self {
        Self {
            translation_graph: -1,
            rotation_graph: -1,
            scaling_graph: -1,
        }
    }
}

/// Geoset animation (color/alpha animation for geosets)
/// From mdlwork.pas GeosetAnims
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GeosetAnim {
    pub geoset_id: i32,
    pub alpha: f32,
    pub is_alpha_static: bool,
    pub alpha_graph: i32,
    pub color: glm::Vec3,
    pub is_color_static: bool,
    pub color_graph: i32,
}

impl Default for GeosetAnim {
    fn default() -> Self {
        Self {
            geoset_id: -1,
            alpha: 1.0,
            is_alpha_static: true,
            alpha_graph: -1,
            color: glm::vec3(1.0, 1.0, 1.0),
            is_color_static: true,
            color_graph: -1,
        }
    }
}

use crate::error::MdlError;
use crate::model::ids::{GlobalSeqId, ObjectId, ParentId, TextureIndex};
use crate::model::model::Model;
use std::collections::HashMap;

/// Fractional MDL frame time. Evaluators must not truncate this value.
#[allow(dead_code)]
pub type FrameTime = f64;

/// Sequence-boundary behavior selected by the caller.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PlaybackMode {
    #[default]
    Loop,
    Clamp,
}

/// Camera basis sampled for view-dependent node rules such as billboarding.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub struct ViewFrame {
    pub position: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub forward: [f32; 3],
}

impl Default for ViewFrame {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            right: [1.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            forward: [0.0, 0.0, 1.0],
        }
    }
}

/// Input clocks and optional view basis for one deterministic pose evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub struct FrameContext {
    pub sequence: Option<usize>,
    pub sequence_time: FrameTime,
    pub global_time: FrameTime,
    pub playback: PlaybackMode,
    pub view: Option<ViewFrame>,
}

/// Validated clocks consumed by later track, node, and material evaluators.
/// `global_frame` remains the original independent clock; each controller must
/// pass it through [`resolve_global_frame`] using its own `GlobalSeqId`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub struct ResolvedFrame {
    pub sequence: Option<usize>,
    pub sequence_frame: FrameTime,
    pub global_frame: FrameTime,
    pub playback: PlaybackMode,
    pub view: Option<ViewFrame>,
}

impl Default for ResolvedFrame {
    fn default() -> Self {
        Self {
            sequence: None,
            sequence_frame: 0.0,
            global_frame: 0.0,
            playback: PlaybackMode::Loop,
            view: None,
        }
    }
}

/// Validate and resolve the sequence clock without evaluating any tracks.
///
/// A sequence marked `non_looping` always resolves with [`PlaybackMode::Clamp`].
/// This intentionally corrects the legacy UI behavior that ignored the format flag.
#[allow(dead_code)]
pub fn resolve_frame(model: &Model, frame: FrameContext) -> Result<ResolvedFrame, MdlError> {
    validate_frame_time(frame.sequence_time)?;
    validate_frame_time(frame.global_time)?;

    let Some(sequence_index) = frame.sequence else {
        return Ok(ResolvedFrame {
            sequence: None,
            sequence_frame: frame.sequence_time,
            global_frame: frame.global_time,
            playback: frame.playback,
            view: frame.view,
        });
    };
    let sequence = model.sequences.get(sequence_index).ok_or_else(|| {
        MdlError::new("animation-invalid-sequence-index")
            .with_arg("index", sequence_index)
            .with_arg("count", model.sequences.len())
    })?;
    if sequence.start_frame > sequence.end_frame {
        return Err(MdlError::new("animation-invalid-sequence-range")
            .with_arg("index", sequence_index)
            .with_arg("start", sequence.start_frame)
            .with_arg("end", sequence.end_frame));
    }

    let start = FrameTime::from(sequence.start_frame);
    let end = FrameTime::from(sequence.end_frame);
    let playback = if sequence.non_looping {
        PlaybackMode::Clamp
    } else {
        frame.playback
    };
    let sequence_frame = match playback {
        PlaybackMode::Clamp => frame.sequence_time.clamp(start, end),
        PlaybackMode::Loop if start == end => start,
        PlaybackMode::Loop if (start..=end).contains(&frame.sequence_time) => frame.sequence_time,
        PlaybackMode::Loop => start + (frame.sequence_time - start).rem_euclid(end - start),
    };
    Ok(ResolvedFrame {
        sequence: Some(sequence_index),
        sequence_frame,
        global_frame: frame.global_time,
        playback,
        view: frame.view,
    })
}

/// Resolve a controller's independent global-sequence clock.
///
/// A zero-duration sequence returns `0.0` only as a resolution marker.
/// TRACK-02 must select the first keyframe (or the typed default when empty),
/// not perform an ordinary frame-zero interval lookup.
#[allow(dead_code)]
pub fn resolve_global_frame(
    model: &Model,
    global_sequence: GlobalSeqId,
    time: FrameTime,
) -> Result<FrameTime, MdlError> {
    validate_frame_time(time)?;
    if global_sequence.is_none() {
        return Ok(time);
    }
    let index = global_sequence.0 as usize;
    let sequence = model.global_sequences.get(index).ok_or_else(|| {
        MdlError::new("animation-invalid-global-sequence-index")
            .with_arg("index", global_sequence.0)
            .with_arg("count", model.global_sequences.len())
    })?;
    if sequence.duration == 0 {
        Ok(0.0)
    } else {
        Ok(time.rem_euclid(FrameTime::from(sequence.duration)))
    }
}

#[allow(dead_code)]
fn validate_frame_time(time: FrameTime) -> Result<(), MdlError> {
    if time.is_finite() {
        Ok(())
    } else {
        Err(MdlError::new("animation-invalid-frame-time").with_arg("time", time))
    }
}

/// Typed local node transform. Its default is the identity transform.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub struct LocalTransform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scaling: [f32; 3],
}

impl Default for LocalTransform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scaling: [1.0; 3],
        }
    }
}

/// Evaluated state of one node, keyed by stable ObjectID rather than array position.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub struct NodePose {
    pub object_id: ObjectId,
    pub parent_id: ParentId,
    pub local: LocalTransform,
    pub world: [[f32; 4]; 4],
    pub sampled_visibility: f32,
    pub visible: bool,
}

impl Default for NodePose {
    fn default() -> Self {
        Self {
            object_id: ObjectId::default(),
            parent_id: ParentId::NONE,
            local: LocalTransform::default(),
            world: identity_matrix(),
            sampled_visibility: 1.0,
            visible: true,
        }
    }
}

/// Evaluated material-layer values.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub struct LayerPose {
    pub alpha: f32,
    pub texture_id: Option<TextureIndex>,
}

impl Default for LayerPose {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            texture_id: None,
        }
    }
}

/// Evaluated geoset-animation values.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub struct GeosetAnimPose {
    pub alpha: f32,
    pub color: [f32; 3],
}

impl Default for GeosetAnimPose {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            color: [1.0; 3],
        }
    }
}

/// Evaluated texture-animation transform.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub struct TextureAnimPose {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scaling: [f32; 3],
}

impl Default for TextureAnimPose {
    fn default() -> Self {
        let local = LocalTransform::default();
        Self {
            translation: local.translation,
            rotation: local.rotation,
            scaling: local.scaling,
        }
    }
}

/// Evaluated material channels, indexed in their normalized model order.
#[derive(Debug, Clone, Default, PartialEq)]
#[allow(dead_code)]
pub struct MaterialPose {
    pub layers: Vec<LayerPose>,
    pub geoset_anims: Vec<GeosetAnimPose>,
    pub texture_anims: Vec<TextureAnimPose>,
}

/// Deterministic output of the future public entry point:
/// `evaluate_pose(model: &Model, frame: FrameContext) -> Result<Pose, MdlError>`.
///
/// This contract is frozen here; track, hierarchy, and material evaluation land
/// in dependent work packages rather than a placeholder implementation.
#[derive(Debug, Clone, Default, PartialEq)]
#[allow(dead_code)]
pub struct Pose {
    pub frame: ResolvedFrame,
    pub nodes: Vec<NodePose>,
    pub object_to_pose: HashMap<ObjectId, usize>,
    pub materials: MaterialPose,
}

impl Pose {
    /// Construct the stable ObjectID-ordered node table and its lookup index.
    #[allow(dead_code)]
    pub fn from_nodes(frame: ResolvedFrame, mut nodes: Vec<NodePose>) -> Result<Self, MdlError> {
        nodes.sort_by_key(|node| node.object_id.0);
        if let Some(duplicate) = nodes
            .windows(2)
            .find(|pair| pair[0].object_id == pair[1].object_id)
        {
            return Err(MdlError::new("animation-duplicate-object-id")
                .with_arg("object_id", duplicate[0].object_id.0));
        }
        let object_to_pose = nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.object_id, index))
            .collect();
        Ok(Self {
            frame,
            nodes,
            object_to_pose,
            materials: MaterialPose::default(),
        })
    }
}

#[allow(dead_code)]
const fn identity_matrix() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

#[cfg(test)]
mod pose_contract_tests {
    use super::*;
    use crate::model::animation::Sequence;
    use crate::model::ids::{GlobalSeqId, ObjectId, ParentId};
    use crate::model::model::Model;
    use crate::model::objects::GlobalSequence;

    fn sequence(start_frame: u32, end_frame: u32, non_looping: bool) -> Sequence {
        Sequence {
            start_frame,
            end_frame,
            non_looping,
            ..Sequence::default()
        }
    }

    fn context(sequence: Option<usize>, time: FrameTime, playback: PlaybackMode) -> FrameContext {
        FrameContext {
            sequence,
            sequence_time: time,
            global_time: 12_345.75,
            playback,
            view: None,
        }
    }

    #[test]
    fn frame_time_keeps_fractional_values_and_none_is_unbounded() {
        let frame = resolve_frame(
            &Model::default(),
            context(None, 12.75_f64, PlaybackMode::Loop),
        )
        .unwrap();
        assert_eq!(frame.sequence, None);
        assert_eq!(frame.sequence_frame, 12.75);
        assert_eq!(frame.global_frame, 12_345.75);
    }

    #[test]
    fn looping_preserves_inclusive_end_and_wraps_multiple_or_negative_periods() {
        let model = Model {
            sequences: vec![sequence(100, 200, false)],
            ..Model::default()
        };
        for (input, expected) in [
            (100.0, 100.0),
            (200.0, 200.0),
            (450.0, 150.0),
            (-50.0, 150.0),
        ] {
            let frame = resolve_frame(&model, context(Some(0), input, PlaybackMode::Loop)).unwrap();
            assert_eq!(frame.sequence_frame, expected);
            assert_eq!(frame.global_frame, 12_345.75);
        }
    }

    #[test]
    fn clamp_non_looping_and_zero_length_sequences_are_resolved() {
        let model = Model {
            sequences: vec![
                sequence(100, 200, false),
                sequence(300, 400, true),
                sequence(500, 500, false),
            ],
            ..Model::default()
        };
        assert_eq!(
            resolve_frame(&model, context(Some(0), -1.0, PlaybackMode::Clamp))
                .unwrap()
                .sequence_frame,
            100.0
        );
        let forced = resolve_frame(&model, context(Some(1), 900.0, PlaybackMode::Loop)).unwrap();
        assert_eq!(forced.playback, PlaybackMode::Clamp);
        assert_eq!(forced.sequence_frame, 400.0);
        for input in [-1000.0, 500.0, 1000.0] {
            assert_eq!(
                resolve_frame(&model, context(Some(2), input, PlaybackMode::Loop))
                    .unwrap()
                    .sequence_frame,
                500.0
            );
        }
    }

    #[test]
    fn global_time_is_independent_and_resolves_duration_none_and_errors() {
        let model = Model {
            global_sequences: vec![
                GlobalSequence { duration: 1000 },
                GlobalSequence { duration: 0 },
            ],
            ..Model::default()
        };
        assert_eq!(
            resolve_global_frame(&model, GlobalSeqId(0), 2500.5).unwrap(),
            500.5
        );
        assert_eq!(
            resolve_global_frame(&model, GlobalSeqId(0), -250.0).unwrap(),
            750.0
        );
        assert_eq!(
            resolve_global_frame(&model, GlobalSeqId(1), 2500.5).unwrap(),
            0.0
        );
        assert_eq!(
            resolve_global_frame(&model, GlobalSeqId::NONE, 2500.5).unwrap(),
            2500.5
        );
        assert_eq!(
            resolve_global_frame(&model, GlobalSeqId(2), 1.0)
                .unwrap_err()
                .key,
            "animation-invalid-global-sequence-index"
        );
        for invalid in [FrameTime::NAN, FrameTime::INFINITY, FrameTime::NEG_INFINITY] {
            assert_eq!(
                resolve_global_frame(&model, GlobalSeqId::NONE, invalid)
                    .unwrap_err()
                    .key,
                "animation-invalid-frame-time"
            );
        }
    }

    #[test]
    fn invalid_sequence_contexts_return_stable_error_keys() {
        let model = Model {
            sequences: vec![sequence(200, 100, false)],
            ..Model::default()
        };
        assert_eq!(
            resolve_frame(&model, context(Some(1), 0.0, PlaybackMode::Loop))
                .unwrap_err()
                .key,
            "animation-invalid-sequence-index"
        );
        assert_eq!(
            resolve_frame(&model, context(Some(0), 0.0, PlaybackMode::Loop))
                .unwrap_err()
                .key,
            "animation-invalid-sequence-range"
        );
        for (sequence_time, global_time) in [
            (FrameTime::NAN, 0.0),
            (FrameTime::INFINITY, 0.0),
            (0.0, FrameTime::NEG_INFINITY),
        ] {
            let mut invalid = context(None, sequence_time, PlaybackMode::Loop);
            invalid.global_time = global_time;
            assert_eq!(
                resolve_frame(&Model::default(), invalid).unwrap_err().key,
                "animation-invalid-frame-time"
            );
        }
    }

    #[test]
    fn typed_pose_defaults_are_identity_values() {
        let local = LocalTransform::default();
        assert_eq!(local.translation, [0.0; 3]);
        assert_eq!(local.rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(local.scaling, [1.0; 3]);

        let node = NodePose::default();
        assert_eq!(node.world, identity_matrix());
        assert_eq!(node.sampled_visibility, 1.0);
        assert!(node.visible);

        let layer = LayerPose::default();
        assert_eq!(layer.alpha, 1.0);
        assert_eq!(layer.texture_id, None);
        let geoset = GeosetAnimPose::default();
        assert_eq!(geoset.alpha, 1.0);
        assert_eq!(geoset.color, [1.0; 3]);
        let texture = TextureAnimPose::default();
        assert_eq!(texture.translation, [0.0; 3]);
        assert_eq!(texture.rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(texture.scaling, [1.0; 3]);
    }

    #[test]
    fn pose_from_nodes_sorts_sparse_object_ids_and_rebuilds_the_index() {
        let nodes = [7, 42, 3]
            .map(|id| NodePose {
                object_id: ObjectId(id),
                parent_id: ParentId::NONE,
                ..NodePose::default()
            })
            .to_vec();
        let pose = Pose::from_nodes(ResolvedFrame::default(), nodes).unwrap();
        assert_eq!(
            pose.nodes
                .iter()
                .map(|node| node.object_id.0)
                .collect::<Vec<_>>(),
            vec![3, 7, 42]
        );
        for (index, object_id) in [3, 7, 42].into_iter().enumerate() {
            assert_eq!(pose.object_to_pose[&ObjectId(object_id)], index);
        }
    }

    #[test]
    fn pose_from_nodes_rejects_duplicate_object_ids() {
        let nodes = vec![
            NodePose {
                object_id: ObjectId(42),
                ..NodePose::default()
            },
            NodePose {
                object_id: ObjectId(7),
                ..NodePose::default()
            },
            NodePose {
                object_id: ObjectId(42),
                ..NodePose::default()
            },
        ];
        let error = Pose::from_nodes(ResolvedFrame::default(), nodes).unwrap_err();
        assert_eq!(error.key, "animation-duplicate-object-id");
        assert_eq!(error.args["object_id"], "42");
    }
}
