// Animation system module
// Based on original Delphi mdlwork.pas and mdlDraw.pas

pub mod controller;
pub mod interpolation;
pub mod skeleton;
pub mod system;
pub mod types;

use crate::error::MdlError;
use crate::model::model::Model;

pub use system::AnimationSystem;

/// Evaluate one deterministic, CPU-only model pose from explicit frame inputs.
#[allow(dead_code)]
pub fn evaluate_pose(model: &Model, frame: types::FrameContext) -> Result<types::Pose, MdlError> {
    let resolved = types::resolve_frame(model, frame)?;
    let nodes = skeleton::evaluate_nodes(model, &resolved)?;
    let materials = crate::material::evaluate_material_pose(model, &resolved)?;
    let mut pose = types::Pose::from_nodes(resolved, nodes)?;
    pose.materials = materials;
    Ok(pose)
}

#[cfg(test)]
mod pose_integration_tests {
    use super::*;
    use crate::material::{FilterMode, Layer, Material};
    use crate::model::ids::{ObjectId, ParentId};
    use crate::model::node::NodeRef;
    use crate::model::objects::{Attachment, LayerRef};

    fn context() -> types::FrameContext {
        types::FrameContext {
            sequence: None,
            sequence_time: 12.5,
            global_time: 37.25,
            playback: types::PlaybackMode::Loop,
            view: None,
        }
    }

    #[test]
    fn evaluate_pose_combines_nodes_and_materials_without_mutating_model() {
        let mut model = Model::default();
        model.attachments.push(Attachment {
            node: NodeRef {
                object_id: ObjectId(7),
                parent_id: ParentId::NONE,
                ..NodeRef::default()
            },
            ..Attachment::default()
        });
        model.materials.push(Material {
            layers: vec![Layer {
                texture_id: None,
                filter_mode: FilterMode::None,
                shading_flags: Vec::new(),
                alpha: 0.75,
                extra: LayerRef::default(),
                alpha_track: -1,
                texture_id_track: -1,
                enabled: true,
                alpha_override: None,
                filter_mode_override: None,
                shading_flags_override: None,
            }],
            ..Material::default()
        });
        let before = serde_json::to_value(&model).expect("serialize model before evaluation");

        let pose = evaluate_pose(&model, context()).expect("evaluate combined pose");

        assert_eq!(pose.frame.sequence_frame, 12.5);
        assert_eq!(pose.frame.global_frame, 37.25);
        assert_eq!(pose.nodes.len(), 1);
        assert_eq!(pose.object_to_pose[&ObjectId(7)], 0);
        assert_eq!(pose.materials.layers.len(), 1);
        assert_eq!(pose.materials.layers[0].alpha, 0.75);
        assert_eq!(
            serde_json::to_value(&model).expect("serialize model after evaluation"),
            before
        );
    }

    #[test]
    fn evaluate_pose_propagates_frame_and_node_errors() {
        let invalid_frame = types::FrameContext {
            sequence_time: f64::NAN,
            ..context()
        };
        assert_eq!(
            evaluate_pose(&Model::default(), invalid_frame)
                .expect_err("non-finite time must fail")
                .key,
            "animation-invalid-frame-time"
        );

        let duplicate = Model {
            attachments: vec![
                Attachment {
                    node: NodeRef {
                        object_id: ObjectId(3),
                        ..NodeRef::default()
                    },
                    ..Attachment::default()
                },
                Attachment {
                    node: NodeRef {
                        object_id: ObjectId(3),
                        ..NodeRef::default()
                    },
                    ..Attachment::default()
                },
            ],
            ..Model::default()
        };
        assert_eq!(
            evaluate_pose(&duplicate, context())
                .expect_err("duplicate ObjectID must fail")
                .key,
            "animation-duplicate-object-id"
        );
    }
}
