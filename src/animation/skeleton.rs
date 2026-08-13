// Skeleton bone calculations
// Based on InterpTBone and CalcTBone from mdlDraw.pas (lines 2134-2307)

use super::controller::*;
use super::interpolation::*;
use super::types::*;
use crate::error::MdlError;
use crate::model::ids::{ObjectId, ParentId, TrackId};
use crate::model::model::Model;
use crate::model::node::{NodeFlags, NodeRef};
use nalgebra_glm as glm;
use std::collections::HashMap;

/// Interpolate bone state for given frame
/// Based on InterpTBone procedure (mdlDraw.pas line 2134)
pub fn interp_bone(
    bone: &mut BoneState,
    frame: i32,
    controllers: &[Controller],
    pivot_points: &[glm::Vec3],
) {
    // Bone is ready if it has no parent
    bone.is_ready = bone.parent < 0;

    // Get pivot point for this bone
    let pivot = if (bone.object_id as usize) < pivot_points.len() {
        pivot_points[bone.object_id as usize]
    } else {
        glm::vec3(0.0, 0.0, 0.0)
    };

    // Translation
    if bone.translation_idx < 0 {
        // No translation animation - use pivot point
        bone.abs_vector = pivot;
    } else {
        // Get animated translation
        let data = get_frame_data(controllers, bone.translation_idx, frame);
        bone.abs_vector = glm::vec3(data[0] + pivot.x, data[1] + pivot.y, data[2] + pivot.z);
    }

    // Rotation
    if bone.rotation_idx < 0 {
        // No rotation animation - identity quaternion
        bone.abs_quaternion = glm::quat_identity();
    } else {
        // Get animated rotation (quaternion)
        let data = get_frame_data(controllers, bone.rotation_idx, frame);
        bone.abs_quaternion = glm::quat(data[3], data[0], data[1], data[2]);
    }

    // Scaling
    if bone.scaling_idx < 0 {
        // No scaling animation - uniform scale of 1
        bone.abs_scaling = glm::vec3(1.0, 1.0, 1.0);
    } else {
        // Get animated scaling
        let data = get_frame_data(controllers, bone.scaling_idx, frame);
        bone.abs_scaling = glm::vec3(data[0], data[1], data[2]);
    }

    // Visibility
    if bone.visibility_idx < 0 {
        bone.visible = true;
    } else {
        let data = get_frame_data(controllers, bone.visibility_idx, frame);
        bone.visible = data[0] > 0.2; // Threshold from original code
    }

    // Convert quaternion to rotation matrix
    bone.abs_matrix = quaternion_to_matrix(&bone.abs_quaternion);

    // Apply scaling to matrix (each column scaled by corresponding component)
    bone.abs_matrix = apply_scaling_to_matrix(&bone.abs_matrix, &bone.abs_scaling);

    // TODO: Apply billboard transformation if needed
    // if bone.is_billboarded { ... }
}

/// Calculate absolute transformation from parent
/// Based on CalcAbsolute procedure (mdlDraw.pas line 2195)
pub fn calc_absolute(parent: &BoneState, child: &mut BoneState, pivot_points: &[glm::Vec3]) {
    // Get parent pivot point
    let parent_pivot = if (parent.object_id as usize) < pivot_points.len() {
        pivot_points[parent.object_id as usize]
    } else {
        glm::vec3(0.0, 0.0, 0.0)
    };

    // 1. Multiply rotation matrices
    if !child.is_billboarded {
        child.abs_matrix = mul_matrices(&parent.abs_matrix, &child.abs_matrix);
    } else {
        // For billboarded objects, only apply parent scaling
        let identity = glm::identity::<f32, 3>();
        let scaled = apply_scaling_to_matrix(&identity, &parent.abs_scaling);
        child.abs_matrix = mul_matrices(&scaled, &child.abs_matrix);
    }

    // 2. Transform child position by parent
    // Subtract parent pivot
    let local_pos = child.abs_vector - parent_pivot;

    // Transform by parent matrix
    let transformed = parent.abs_matrix * local_pos;

    // Add parent position
    child.abs_vector = parent.abs_vector + transformed;

    // 3. Combine visibility
    child.visible = child.visible && parent.visible;
}

/// Recursively calculate bone transformation hierarchy
/// Based on CalcTBone procedure (mdlDraw.pas line 2237)
pub fn calc_bone(
    bone_idx: usize,
    bones: &mut [BoneState],
    helpers: &mut [BoneState],
    controllers: &[Controller],
    pivot_points: &[glm::Vec3],
    frame: i32,
) {
    // Check if already calculated
    if bone_idx < bones.len() && bones[bone_idx].is_ready {
        return;
    }
    if bone_idx >= bones.len() {
        let helper_idx = bone_idx - bones.len();
        if helper_idx < helpers.len() && helpers[helper_idx].is_ready {
            return;
        }
    }

    // Get current bone
    let (is_helper, current_idx) = if bone_idx < bones.len() {
        (false, bone_idx)
    } else {
        (true, bone_idx - bones.len())
    };

    let parent_id = if is_helper {
        if current_idx >= helpers.len() {
            return;
        }
        helpers[current_idx].parent
    } else {
        if current_idx >= bones.len() {
            return;
        }
        bones[current_idx].parent
    };

    // No parent - already ready
    if parent_id < 0 {
        if is_helper {
            helpers[current_idx].is_ready = true;
        } else {
            bones[current_idx].is_ready = true;
        }
        return;
    }

    // Calculate parent first (recursive)
    let parent_idx = parent_id as usize;
    calc_bone(parent_idx, bones, helpers, controllers, pivot_points, frame);

    // Get parent bone (this is tricky - need to handle bones vs helpers)
    let parent_bone = if parent_idx < bones.len() {
        bones[parent_idx].clone()
    } else {
        let helper_idx = parent_idx - bones.len();
        if helper_idx < helpers.len() {
            helpers[helper_idx].clone()
        } else {
            return;
        }
    };

    // Calculate absolute transformation
    if is_helper {
        calc_absolute(&parent_bone, &mut helpers[current_idx], pivot_points);
        helpers[current_idx].is_ready = true;
    } else {
        calc_absolute(&parent_bone, &mut bones[current_idx], pivot_points);
        bones[current_idx].is_ready = true;
    }
}

#[derive(Clone)]
struct EvaluatedNode {
    node: NodeRef,
    pivot: [f32; 3],
    local: LocalTransform,
    position: [f32; 3],
    rotation: [f32; 4],
    scaling: [f32; 3],
    sampled_visibility: f32,
    visible: bool,
}

/// Evaluate every non-camera node into a stable ObjectID-ordered pose table.
#[allow(dead_code)]
pub fn evaluate_nodes(model: &Model, frame: &ResolvedFrame) -> Result<Vec<NodePose>, MdlError> {
    let mut nodes = collect_nodes(model);
    nodes.sort_by_key(|node| node.object_id.0);
    for pair in nodes.windows(2) {
        if pair[0].object_id == pair[1].object_id {
            return Err(MdlError::new("animation-duplicate-object-id")
                .with_arg("object_id", pair[0].object_id.0));
        }
    }

    let object_to_node: HashMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.object_id, index))
        .collect();
    for node in &nodes {
        validate_node_flags(node)?;
        if !node.parent_id.is_none()
            && !object_to_node.contains_key(&ObjectId(node.parent_id.0 as u32))
        {
            return Err(MdlError::new("animation-missing-parent-object")
                .with_arg("object_id", node.object_id.0)
                .with_arg("parent_id", node.parent_id.0));
        }
    }
    validate_parent_graph(&nodes, &object_to_node)?;

    let view_rotation = if nodes.iter().any(|node| node.flags.billboarded()) {
        Some(validate_view_frame(frame, &nodes)?)
    } else {
        None
    };
    let mut evaluated = vec![None; nodes.len()];
    for index in 0..nodes.len() {
        evaluate_node(
            index,
            &nodes,
            &object_to_node,
            &mut evaluated,
            model,
            frame,
            view_rotation,
        )?;
    }
    Ok(evaluated
        .into_iter()
        .map(|node| {
            let node = node.expect("all sorted nodes evaluated");
            NodePose {
                object_id: node.node.object_id,
                parent_id: node.node.parent_id,
                local: node.local,
                world: joint_matrix(node.position, node.rotation, node.scaling, node.pivot),
                sampled_visibility: node.sampled_visibility,
                visible: node.visible,
            }
        })
        .collect())
}

fn collect_nodes(model: &Model) -> Vec<NodeRef> {
    let mut nodes = Vec::with_capacity(
        model.bones.len()
            + model.helpers.len()
            + model.lights.len()
            + model.attachments.len()
            + model.particle_emitters.len()
            + model.particle_emitters_2.len()
            + model.ribbons.len()
            + model.events.len()
            + model.collisions.len(),
    );
    nodes.extend(model.bones.iter().map(|bone| NodeRef {
        name: bone.name.clone(),
        object_id: ObjectId(bone.object_id),
        parent_id: ParentId(bone.parent_id),
        flags: NodeFlags::from_bits(bone.flags),
        translation: TrackId(bone.translation_idx),
        rotation: TrackId(bone.rotation_idx),
        scaling: TrackId(bone.scaling_idx),
        visibility: TrackId(bone.visibility_idx),
    }));
    nodes.extend(model.helpers.iter().map(|helper| NodeRef {
        name: helper.name.clone(),
        object_id: ObjectId(helper.object_id),
        parent_id: ParentId(helper.parent_id),
        flags: NodeFlags::from_bits(helper.flags),
        translation: TrackId(helper.translation_idx),
        rotation: TrackId(helper.rotation_idx),
        scaling: TrackId(helper.scaling_idx),
        visibility: TrackId(helper.visibility_idx),
    }));
    nodes.extend(model.lights.iter().map(|owner| owner.node.clone()));
    nodes.extend(model.attachments.iter().map(|owner| owner.node.clone()));
    nodes.extend(
        model
            .particle_emitters
            .iter()
            .map(|owner| owner.node.clone()),
    );
    nodes.extend(
        model
            .particle_emitters_2
            .iter()
            .map(|owner| owner.node.clone()),
    );
    nodes.extend(model.ribbons.iter().map(|owner| owner.node.clone()));
    nodes.extend(model.events.iter().map(|owner| owner.node.clone()));
    nodes.extend(model.collisions.iter().map(|owner| owner.node.clone()));
    nodes
}

fn validate_node_flags(node: &NodeRef) -> Result<(), MdlError> {
    for (set, name) in [
        (node.flags.billboard_lock_x(), "LockX"),
        (node.flags.billboard_lock_y(), "LockY"),
        (node.flags.billboard_lock_z(), "LockZ"),
        (node.flags.camera_anchored(), "CameraAnchored"),
    ] {
        if set {
            return Err(MdlError::new("animation-unsupported-node-flag")
                .with_arg("object_id", node.object_id.0)
                .with_arg("flag", name));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Unvisited,
    Visiting,
    Complete,
}

fn validate_parent_graph(
    nodes: &[NodeRef],
    object_to_node: &HashMap<ObjectId, usize>,
) -> Result<(), MdlError> {
    fn visit(
        index: usize,
        nodes: &[NodeRef],
        object_to_node: &HashMap<ObjectId, usize>,
        states: &mut [VisitState],
    ) -> Result<(), MdlError> {
        match states[index] {
            VisitState::Complete => return Ok(()),
            VisitState::Visiting => {
                return Err(MdlError::new("animation-node-parent-cycle")
                    .with_arg("object_id", nodes[index].object_id.0));
            }
            VisitState::Unvisited => {}
        }
        states[index] = VisitState::Visiting;
        if !nodes[index].parent_id.is_none() {
            let parent = object_to_node[&ObjectId(nodes[index].parent_id.0 as u32)];
            visit(parent, nodes, object_to_node, states)?;
        }
        states[index] = VisitState::Complete;
        Ok(())
    }

    let mut states = vec![VisitState::Unvisited; nodes.len()];
    for index in 0..nodes.len() {
        visit(index, nodes, object_to_node, &mut states)?;
    }
    Ok(())
}

fn validate_view_frame(frame: &ResolvedFrame, nodes: &[NodeRef]) -> Result<[f32; 4], MdlError> {
    let billboard = nodes
        .iter()
        .find(|node| node.flags.billboarded())
        .expect("billboard requested");
    let view = frame.view.ok_or_else(|| {
        MdlError::new("animation-missing-view-frame").with_arg("object_id", billboard.object_id.0)
    })?;
    let basis = [view.right, view.up, view.forward];
    if view.position.iter().any(|value| !value.is_finite())
        || basis.iter().flatten().any(|value| !value.is_finite())
    {
        return Err(invalid_view("finite"));
    }
    if basis
        .iter()
        .any(|axis| (dot3(*axis, *axis) - 1.0).abs() > 1.0e-4)
    {
        return Err(invalid_view("unit"));
    }
    if dot3(view.right, view.up).abs() > 1.0e-4
        || dot3(view.right, view.forward).abs() > 1.0e-4
        || dot3(view.up, view.forward).abs() > 1.0e-4
    {
        return Err(invalid_view("orthogonal"));
    }
    if (dot3(cross(view.right, view.up), view.forward) - 1.0).abs() > 1.0e-4 {
        return Err(invalid_view("handedness"));
    }
    Ok(quaternion_from_columns(view.right, view.up, view.forward))
}

fn invalid_view(reason: &'static str) -> MdlError {
    MdlError::new("animation-invalid-view-frame").with_arg("reason", reason)
}

#[allow(clippy::too_many_arguments)]
fn evaluate_node(
    index: usize,
    nodes: &[NodeRef],
    object_to_node: &HashMap<ObjectId, usize>,
    evaluated: &mut [Option<EvaluatedNode>],
    model: &Model,
    frame: &ResolvedFrame,
    view_rotation: Option<[f32; 4]>,
) -> Result<(), MdlError> {
    if evaluated[index].is_some() {
        return Ok(());
    }
    let node = &nodes[index];
    let parent = if node.parent_id.is_none() {
        None
    } else {
        let parent_index = object_to_node[&ObjectId(node.parent_id.0 as u32)];
        evaluate_node(
            parent_index,
            nodes,
            object_to_node,
            evaluated,
            model,
            frame,
            view_rotation,
        )?;
        evaluated[parent_index].clone()
    };

    let pivot = pivot_for(model, node.object_id)?;
    let local = LocalTransform {
        translation: sample_vec3(model, node.translation, frame, [0.0; 3])?,
        rotation: sample_quaternion(model, node.rotation, frame, [0.0, 0.0, 0.0, 1.0])?,
        scaling: sample_vec3(model, node.scaling, frame, [1.0; 3])?,
    };
    let local_rotation = normalize_quaternion(local.rotation);
    let (parent_position, parent_pivot, parent_rotation, parent_scaling, parent_visible) = parent
        .as_ref()
        .map(|parent| {
            (
                parent.position,
                parent.pivot,
                parent.rotation,
                parent.scaling,
                parent.visible,
            )
        })
        .unwrap_or(([0.0; 3], [0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3], true));
    let inherited_position = if node.flags.dont_inherit_translation() {
        [0.0; 3]
    } else {
        parent_position
    };
    let inherited_rotation = if node.flags.dont_inherit_rotation() {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        parent_rotation
    };
    let inherited_scaling = if node.flags.dont_inherit_scaling() {
        [1.0; 3]
    } else {
        parent_scaling
    };
    let position = if parent.is_none() {
        add3(pivot, local.translation)
    } else {
        add3(
            inherited_position,
            rotate_vector(
                inherited_rotation,
                mul3(
                    inherited_scaling,
                    sub3(add3(pivot, local.translation), parent_pivot),
                ),
            ),
        )
    };
    let rotation = if node.flags.billboarded() {
        normalize_quaternion(quaternion_mul(
            view_rotation.expect("billboard view validated"),
            local_rotation,
        ))
    } else {
        normalize_quaternion(quaternion_mul(inherited_rotation, local_rotation))
    };
    let scaling = mul3(inherited_scaling, local.scaling);
    let sampled_visibility = sample_scalar(model, node.visibility, frame, 1.0)?;
    let visible = sampled_visibility > 0.2 && parent_visible;
    evaluated[index] = Some(EvaluatedNode {
        node: node.clone(),
        pivot,
        local,
        position,
        rotation,
        scaling,
        sampled_visibility,
        visible,
    });
    Ok(())
}

fn pivot_for(model: &Model, object_id: ObjectId) -> Result<[f32; 3], MdlError> {
    if model.pivot_points.is_empty() {
        return Ok([0.0; 3]);
    }
    model
        .pivot_points
        .get(object_id.0 as usize)
        .copied()
        .ok_or_else(|| {
            MdlError::new("animation-missing-pivot-point")
                .with_arg("object_id", object_id.0)
                .with_arg("count", model.pivot_points.len())
        })
}

fn joint_matrix(
    position: [f32; 3],
    rotation: [f32; 4],
    scaling: [f32; 3],
    pivot: [f32; 3],
) -> [[f32; 4]; 4] {
    let rotation = rotation_matrix(rotation);
    let mut world = [[0.0; 4]; 4];
    for row in 0..3 {
        for column in 0..3 {
            world[row][column] = rotation[row][column] * scaling[column];
        }
        world[row][3] = position[row]
            - world[row][0] * pivot[0]
            - world[row][1] * pivot[1]
            - world[row][2] * pivot[2];
    }
    world[3][3] = 1.0;
    world
}

fn rotation_matrix(quaternion: [f32; 4]) -> [[f32; 3]; 3] {
    let [x, y, z, w] = normalize_quaternion(quaternion);
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

fn quaternion_from_columns(right: [f32; 3], up: [f32; 3], forward: [f32; 3]) -> [f32; 4] {
    let matrix = [
        [right[0], up[0], forward[0]],
        [right[1], up[1], forward[1]],
        [right[2], up[2], forward[2]],
    ];
    let trace = matrix[0][0] + matrix[1][1] + matrix[2][2];
    let quaternion = if trace > 0.0 {
        let scale = (trace + 1.0).sqrt() * 2.0;
        [
            (matrix[2][1] - matrix[1][2]) / scale,
            (matrix[0][2] - matrix[2][0]) / scale,
            (matrix[1][0] - matrix[0][1]) / scale,
            0.25 * scale,
        ]
    } else if matrix[0][0] > matrix[1][1] && matrix[0][0] > matrix[2][2] {
        let scale = (1.0 + matrix[0][0] - matrix[1][1] - matrix[2][2]).sqrt() * 2.0;
        [
            0.25 * scale,
            (matrix[0][1] + matrix[1][0]) / scale,
            (matrix[0][2] + matrix[2][0]) / scale,
            (matrix[2][1] - matrix[1][2]) / scale,
        ]
    } else if matrix[1][1] > matrix[2][2] {
        let scale = (1.0 + matrix[1][1] - matrix[0][0] - matrix[2][2]).sqrt() * 2.0;
        [
            (matrix[0][1] + matrix[1][0]) / scale,
            0.25 * scale,
            (matrix[1][2] + matrix[2][1]) / scale,
            (matrix[0][2] - matrix[2][0]) / scale,
        ]
    } else {
        let scale = (1.0 + matrix[2][2] - matrix[0][0] - matrix[1][1]).sqrt() * 2.0;
        [
            (matrix[0][2] + matrix[2][0]) / scale,
            (matrix[1][2] + matrix[2][1]) / scale,
            0.25 * scale,
            (matrix[1][0] - matrix[0][1]) / scale,
        ]
    };
    normalize_quaternion(quaternion)
}

fn quaternion_mul(left: [f32; 4], right: [f32; 4]) -> [f32; 4] {
    let [lx, ly, lz, lw] = left;
    let [rx, ry, rz, rw] = right;
    [
        lw * rx + lx * rw + ly * rz - lz * ry,
        lw * ry - lx * rz + ly * rw + lz * rx,
        lw * rz + lx * ry - ly * rx + lz * rw,
        lw * rw - lx * rx - ly * ry - lz * rz,
    ]
}

fn normalize_quaternion(value: [f32; 4]) -> [f32; 4] {
    let length = value
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    value.map(|component| component / length)
}

fn rotate_vector(rotation: [f32; 4], value: [f32; 3]) -> [f32; 3] {
    let matrix = rotation_matrix(rotation);
    std::array::from_fn(|row| dot3(matrix[row], value))
}

fn add3(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|index| left[index] + right[index])
}

fn sub3(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|index| left[index] - right[index])
}

fn mul3(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|index| left[index] * right[index])
}

fn dot3(left: [f32; 3], right: [f32; 3]) -> f32 {
    left.into_iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

#[cfg(test)]
mod stable_node_pose_tests {
    use super::*;
    use crate::animation::types::{PlaybackMode, ResolvedFrame, ViewFrame};
    use crate::model::ids::{ObjectId, ParentId};
    use crate::model::model::Model;
    use crate::model::node::{NodeFlags, NodeRef};
    use crate::model::objects::{
        Attachment, CollisionShape, EventObject, Light, ParticleEmitter, ParticleEmitter2,
        RibbonEmitter,
    };
    use crate::model::skeleton::{AnimationController, Bone, Helper, Keyframe};

    fn frame(view: Option<ViewFrame>) -> ResolvedFrame {
        ResolvedFrame {
            sequence: None,
            sequence_frame: 0.0,
            global_frame: 0.0,
            playback: PlaybackMode::Clamp,
            view,
        }
    }

    fn node(id: u32, parent: i32, flags: u32) -> NodeRef {
        NodeRef {
            name: format!("node-{id}"),
            object_id: ObjectId(id),
            parent_id: ParentId(parent),
            flags: NodeFlags::from_bits(flags),
            ..NodeRef::default()
        }
    }

    fn helper(id: u32, parent: i32, flags: u32) -> Helper {
        Helper {
            name: format!("helper-{id}"),
            object_id: id,
            parent_id: parent,
            flags,
            ..Helper::default()
        }
    }

    fn bone(id: u32, parent: i32, flags: u32) -> Bone {
        Bone {
            name: format!("bone-{id}"),
            object_id: id,
            parent_id: parent,
            flags,
            ..Bone::default()
        }
    }

    fn key(data: &[f32]) -> Keyframe {
        Keyframe {
            frame: 0,
            data: data.to_vec(),
            in_tan: Vec::new(),
            out_tan: Vec::new(),
        }
    }

    fn controller(data: &[f32]) -> AnimationController {
        AnimationController {
            interpolation_type: 1,
            global_seq_id: -1,
            keyframes: vec![key(data)],
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1.0e-5, "{actual} != {expected}");
    }

    fn point(world: [[f32; 4]; 4], value: [f32; 3]) -> [f32; 3] {
        [
            world[0][0] * value[0] + world[0][1] * value[1] + world[0][2] * value[2] + world[0][3],
            world[1][0] * value[0] + world[1][1] * value[1] + world[1][2] * value[2] + world[1][3],
            world[2][0] * value[0] + world[2][1] * value[1] + world[2][2] * value[2] + world[2][3],
        ]
    }

    #[test]
    fn all_nine_node_owners_share_one_sorted_graph_and_cameras_are_excluded() {
        let mut model = Model::default();
        model.bones.push(bone(90, -1, 0));
        model.helpers.push(helper(80, -1, 0));
        model.lights.push(Light {
            node: node(70, -1, 0),
            ..Light::default()
        });
        model.attachments.push(Attachment {
            node: node(60, -1, 0),
            ..Attachment::default()
        });
        model.particle_emitters.push(ParticleEmitter {
            node: node(50, -1, 0),
            ..ParticleEmitter::default()
        });
        model.particle_emitters_2.push(ParticleEmitter2 {
            node: node(40, -1, 0),
            ..ParticleEmitter2::default()
        });
        model.ribbons.push(RibbonEmitter {
            node: node(30, -1, 0),
            ..RibbonEmitter::default()
        });
        model.events.push(EventObject {
            node: node(20, -1, 0),
            ..EventObject::default()
        });
        model.collisions.push(CollisionShape {
            node: node(10, -1, 0),
            ..CollisionShape::default()
        });
        model.cameras.push(Default::default());

        let poses = evaluate_nodes(&model, &frame(None)).unwrap();
        assert_eq!(
            poses
                .iter()
                .map(|pose| pose.object_id.0)
                .collect::<Vec<_>>(),
            vec![10, 20, 30, 40, 50, 60, 70, 80, 90]
        );
    }

    #[test]
    fn pivots_are_indexed_by_sparse_object_id_and_empty_means_zero() {
        let mut model = Model {
            helpers: vec![helper(7, -1, 0)],
            ..Model::default()
        };
        let zero = evaluate_nodes(&model, &frame(None)).unwrap();
        assert_eq!(point(zero[0].world, [0.0; 3]), [0.0; 3]);

        model.pivot_points = vec![[0.0; 3]; 8];
        model.pivot_points[7] = [3.0, 4.0, 5.0];
        let sparse = evaluate_nodes(&model, &frame(None)).unwrap();
        assert_eq!(point(sparse[0].world, [3.0, 4.0, 5.0]), [3.0, 4.0, 5.0]);

        model.pivot_points.pop();
        assert_eq!(
            evaluate_nodes(&model, &frame(None)).unwrap_err().key,
            "animation-missing-pivot-point"
        );
    }

    #[test]
    fn root_and_child_use_joint_pivot_rotation_scaling_matrices() {
        let mut model = Model {
            helpers: vec![helper(0, -1, 0), helper(1, 0, 0)],
            pivot_points: vec![[1.0, 0.0, 0.0], [3.0, 0.0, 0.0]],
            controllers: vec![
                controller(&[2.0, 0.0, 0.0]),
                controller(&[0.0, 0.0, 1.0, 0.0]),
                controller(&[2.0, 2.0, 2.0]),
                controller(&[1.0, 0.0, 0.0]),
            ],
            ..Model::default()
        };
        model.helpers[0].translation_idx = 0;
        model.helpers[0].rotation_idx = 1;
        model.helpers[0].scaling_idx = 2;
        model.helpers[1].translation_idx = 3;

        let poses = evaluate_nodes(&model, &frame(None)).unwrap();
        assert_eq!(poses[0].local.translation, [2.0, 0.0, 0.0]);
        assert_eq!(poses[0].local.scaling, [2.0; 3]);
        let root_pivot = point(poses[0].world, [1.0, 0.0, 0.0]);
        assert_close(root_pivot[0], 3.0);
        assert_close(root_pivot[1], 0.0);
        let child_pivot = point(poses[1].world, [3.0, 0.0, 0.0]);
        assert_close(child_pivot[0], -3.0);
        assert_close(child_pivot[1], 0.0);
    }

    #[test]
    fn dont_inherit_translation_rotation_and_scaling_are_independent() {
        let mut base = Model {
            helpers: vec![helper(0, -1, 0), helper(1, 0, 0)],
            pivot_points: vec![[0.0; 3], [1.0, 0.0, 0.0]],
            controllers: vec![
                controller(&[5.0, 0.0, 0.0]),
                controller(&[0.0, 0.0, 1.0, 0.0]),
                controller(&[2.0, 2.0, 2.0]),
            ],
            ..Model::default()
        };
        base.helpers[0].translation_idx = 0;
        base.helpers[0].rotation_idx = 1;
        base.helpers[0].scaling_idx = 2;

        for (flag, expected_pivot) in [
            (1, [-2.0, 0.0, 0.0]),
            (4, [7.0, 0.0, 0.0]),
            (2, [4.0, 0.0, 0.0]),
        ] {
            let mut model = base.clone();
            model.helpers[1].flags = flag;
            let poses = evaluate_nodes(&model, &frame(None)).unwrap();
            let actual = point(poses[1].world, [1.0, 0.0, 0.0]);
            for axis in 0..3 {
                assert_close(actual[axis], expected_pivot[axis]);
            }
        }
    }

    #[test]
    fn visibility_threshold_and_parent_chain_are_applied() {
        let mut model = Model {
            helpers: vec![helper(0, -1, 0), helper(1, 0, 0), helper(2, 1, 0)],
            controllers: vec![controller(&[0.2]), controller(&[1.0]), controller(&[0.9])],
            ..Model::default()
        };
        for (helper, track) in model.helpers.iter_mut().zip(0..) {
            helper.visibility_idx = track;
        }
        let poses = evaluate_nodes(&model, &frame(None)).unwrap();
        assert_eq!(poses[0].sampled_visibility, 0.2);
        assert!(!poses[0].visible);
        assert!(!poses[1].visible);
        assert!(!poses[2].visible);
    }

    #[test]
    fn full_billboard_uses_view_rotation_but_inherits_parent_position_and_scale() {
        let mut model = Model {
            helpers: vec![helper(0, -1, 0), helper(1, 0, 8)],
            pivot_points: vec![[0.0; 3], [1.0, 0.0, 0.0]],
            controllers: vec![
                controller(&[5.0, 0.0, 0.0]),
                controller(&[0.0, 0.0, 1.0, 0.0]),
                controller(&[2.0, 2.0, 2.0]),
            ],
            ..Model::default()
        };
        model.helpers[0].translation_idx = 0;
        model.helpers[0].rotation_idx = 1;
        model.helpers[0].scaling_idx = 2;
        let view = ViewFrame {
            position: [0.0, 0.0, 10.0],
            right: [0.0, 1.0, 0.0],
            up: [-1.0, 0.0, 0.0],
            forward: [0.0, 0.0, 1.0],
        };
        let poses = evaluate_nodes(&model, &frame(Some(view))).unwrap();
        let pivot = point(poses[1].world, [1.0, 0.0, 0.0]);
        for (actual, expected) in pivot.into_iter().zip([3.0, 0.0, 0.0]) {
            assert_close(actual, expected);
        }
        let right = point(poses[1].world, [2.0, 0.0, 0.0]);
        for (actual, expected) in right.into_iter().zip([3.0, 2.0, 0.0]) {
            assert_close(actual, expected);
        }
    }

    #[test]
    fn unsupported_billboard_modes_are_stable_errors() {
        for flag in [8_u32, 16, 32, 64, 128] {
            let model = Model {
                helpers: vec![helper(0, -1, flag)],
                ..Model::default()
            };
            let error = evaluate_nodes(&model, &frame(None)).unwrap_err();
            assert_eq!(
                error.key,
                if flag == 8 {
                    "animation-missing-view-frame"
                } else {
                    "animation-unsupported-node-flag"
                }
            );
        }
    }

    #[test]
    fn invalid_billboard_view_basis_reports_stable_reason_order() {
        let model = Model {
            helpers: vec![helper(7, -1, 8)],
            ..Model::default()
        };
        for (view, reason) in [
            (
                ViewFrame {
                    right: [f32::NAN, 0.0, 0.0],
                    ..ViewFrame::default()
                },
                "finite",
            ),
            (
                ViewFrame {
                    right: [2.0, 0.0, 0.0],
                    ..ViewFrame::default()
                },
                "unit",
            ),
            (
                ViewFrame {
                    up: [1.0, 0.0, 0.0],
                    ..ViewFrame::default()
                },
                "orthogonal",
            ),
            (
                ViewFrame {
                    forward: [0.0, 0.0, -1.0],
                    ..ViewFrame::default()
                },
                "handedness",
            ),
        ] {
            let error = evaluate_nodes(&model, &frame(Some(view))).unwrap_err();
            assert_eq!(error.key, "animation-invalid-view-frame");
            assert_eq!(error.args["reason"], reason);
        }
    }

    #[test]
    fn duplicate_missing_parent_and_cycle_are_rejected() {
        let duplicate = Model {
            bones: vec![bone(7, -1, 0)],
            helpers: vec![helper(7, -1, 0)],
            ..Model::default()
        };
        let duplicate = evaluate_nodes(&duplicate, &frame(None)).unwrap_err();
        assert_eq!(duplicate.key, "animation-duplicate-object-id");
        assert_eq!(duplicate.args["object_id"], "7");
        let missing = Model {
            helpers: vec![helper(7, 42, 0)],
            ..Model::default()
        };
        let missing = evaluate_nodes(&missing, &frame(None)).unwrap_err();
        assert_eq!(missing.key, "animation-missing-parent-object");
        assert_eq!(missing.args["object_id"], "7");
        assert_eq!(missing.args["parent_id"], "42");
        let cycle = Model {
            helpers: vec![helper(7, 42, 0), helper(42, 7, 0)],
            ..Model::default()
        };
        let cycle = evaluate_nodes(&cycle, &frame(None)).unwrap_err();
        assert_eq!(cycle.key, "animation-node-parent-cycle");
        assert_eq!(cycle.args["object_id"], "7");
    }

    #[test]
    fn typed_sampler_errors_are_forwarded() {
        let mut model = Model {
            helpers: vec![helper(0, -1, 0)],
            ..Model::default()
        };
        model.helpers[0].translation_idx = 0;
        assert_eq!(
            evaluate_nodes(&model, &frame(None)).unwrap_err().key,
            "animation-invalid-controller-index"
        );
    }
}
