#![cfg_attr(not(test), allow(dead_code))]

use crate::animation::types::Pose;
use crate::error::MdlError;
use crate::model::geoset::Geoset;
use crate::model::ids::ObjectId;
use std::collections::HashSet;

type SkinnedStreams = (Vec<[f32; 3]>, Vec<[f32; 3]>);

pub(super) fn skin_geoset(
    geoset: &Geoset,
    pose: &Pose,
    geoset_index: usize,
) -> Result<SkinnedStreams, MdlError> {
    if !geoset.normals.is_empty() && geoset.normals.len() != geoset.vertices.len() {
        return Err(MdlError::new("scene-invalid-normal-count")
            .with_arg("geoset", geoset_index)
            .with_arg("expected", geoset.vertices.len())
            .with_arg("actual", geoset.normals.len()));
    }
    if geoset.vertex_groups.is_empty() {
        let positions = geoset
            .vertices
            .iter()
            .enumerate()
            .map(|(index, vertex)| checked_vec(vertex.position, "scene-non-finite-position", index))
            .collect::<Result<Vec<_>, _>>()?;
        let normals = geoset
            .normals
            .iter()
            .enumerate()
            .map(|(index, normal)| normalize(normal.normal, "scene-invalid-normal", index))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok((positions, normals));
    }
    if geoset.vertex_groups.len() != geoset.vertices.len() {
        return Err(MdlError::new("scene-invalid-vertex-group-count")
            .with_arg("geoset", geoset_index)
            .with_arg("expected", geoset.vertices.len())
            .with_arg("actual", geoset.vertex_groups.len()));
    }

    let mut positions = Vec::with_capacity(geoset.vertices.len());
    let mut normals = Vec::with_capacity(geoset.normals.len());
    for (vertex_index, vertex) in geoset.vertices.iter().enumerate() {
        checked_vec(vertex.position, "scene-non-finite-position", vertex_index)?;
        let group_index = geoset.vertex_groups[vertex_index] as usize;
        let group = geoset.matrix_groups.get(group_index).ok_or_else(|| {
            MdlError::new("scene-invalid-matrix-group-index")
                .with_arg("geoset", geoset_index)
                .with_arg("vertex", vertex_index)
                .with_arg("index", group_index)
                .with_arg("count", geoset.matrix_groups.len())
        })?;
        if group.is_empty() {
            return Err(MdlError::new("scene-empty-matrix-group")
                .with_arg("geoset", geoset_index)
                .with_arg("group", group_index));
        }
        let mut seen = HashSet::new();
        let mut position_sum = [0.0; 3];
        let mut normal_sum = [0.0; 3];
        for object_id in group {
            if !seen.insert(*object_id) {
                return Err(MdlError::new("scene-duplicate-skin-object")
                    .with_arg("geoset", geoset_index)
                    .with_arg("group", group_index)
                    .with_arg("object_id", object_id));
            }
            let pose_index = pose
                .object_to_pose
                .get(&ObjectId(*object_id))
                .copied()
                .ok_or_else(|| {
                    MdlError::new("scene-missing-skin-node")
                        .with_arg("geoset", geoset_index)
                        .with_arg("object_id", object_id)
                })?;
            let node = pose.nodes.get(pose_index).ok_or_else(|| {
                MdlError::new("scene-invalid-pose-index")
                    .with_arg("object_id", object_id)
                    .with_arg("index", pose_index)
                    .with_arg("count", pose.nodes.len())
            })?;
            ensure_matrix_finite(node.world, *object_id)?;
            add_assign(
                &mut position_sum,
                transform_point(node.world, vertex.position),
            );
            if !geoset.normals.is_empty() {
                add_assign(
                    &mut normal_sum,
                    transform_normal(node.world, geoset.normals[vertex_index].normal, *object_id)?,
                );
            }
        }
        let divisor = group.len() as f32;
        positions.push(checked_vec(
            scale(position_sum, divisor.recip()),
            "scene-non-finite-skinned-position",
            vertex_index,
        )?);
        if !geoset.normals.is_empty() {
            normals.push(normalize(
                scale(normal_sum, divisor.recip()),
                "scene-invalid-skinned-normal",
                vertex_index,
            )?);
        }
    }
    Ok((positions, normals))
}

fn transform_point(matrix: [[f32; 4]; 4], point: [f32; 3]) -> [f32; 3] {
    [
        matrix[0][0] * point[0] + matrix[0][1] * point[1] + matrix[0][2] * point[2] + matrix[0][3],
        matrix[1][0] * point[0] + matrix[1][1] * point[1] + matrix[1][2] * point[2] + matrix[1][3],
        matrix[2][0] * point[0] + matrix[2][1] * point[1] + matrix[2][2] * point[2] + matrix[2][3],
    ]
}

fn transform_normal(
    matrix: [[f32; 4]; 4],
    normal: [f32; 3],
    object_id: u32,
) -> Result<[f32; 3], MdlError> {
    let [a, b, c] = [matrix[0][0], matrix[0][1], matrix[0][2]];
    let [d, e, f] = [matrix[1][0], matrix[1][1], matrix[1][2]];
    let [g, h, i] = [matrix[2][0], matrix[2][1], matrix[2][2]];
    let cofactors = [
        [e * i - f * h, f * g - d * i, d * h - e * g],
        [c * h - b * i, a * i - c * g, b * g - a * h],
        [b * f - c * e, c * d - a * f, a * e - b * d],
    ];
    let determinant = a * cofactors[0][0] + b * cofactors[0][1] + c * cofactors[0][2];
    if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
        return Err(MdlError::new("scene-singular-normal-matrix").with_arg("object_id", object_id));
    }
    Ok(scale(
        [
            dot(cofactors[0], normal),
            dot(cofactors[1], normal),
            dot(cofactors[2], normal),
        ],
        determinant.recip(),
    ))
}

fn normalize(value: [f32; 3], key: &'static str, index: usize) -> Result<[f32; 3], MdlError> {
    checked_vec(value, key, index)?;
    let length = dot(value, value).sqrt();
    if !length.is_finite() || length <= f32::EPSILON {
        return Err(MdlError::new(key).with_arg("index", index));
    }
    Ok(scale(value, length.recip()))
}

fn checked_vec(value: [f32; 3], key: &'static str, index: usize) -> Result<[f32; 3], MdlError> {
    if value.iter().all(|component| component.is_finite()) {
        Ok(value)
    } else {
        Err(MdlError::new(key).with_arg("index", index))
    }
}

fn ensure_matrix_finite(matrix: [[f32; 4]; 4], object_id: u32) -> Result<(), MdlError> {
    if matrix.iter().flatten().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(MdlError::new("scene-non-finite-skin-matrix").with_arg("object_id", object_id))
    }
}

fn add_assign(target: &mut [f32; 3], value: [f32; 3]) {
    for axis in 0..3 {
        target[axis] += value[axis];
    }
}

fn scale(value: [f32; 3], factor: f32) -> [f32; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::types::NodePose;
    use crate::model::geoset::{Normal, Vertex};
    use crate::model::ids::ParentId;

    fn node(id: u32, world: [[f32; 4]; 4]) -> NodePose {
        NodePose {
            object_id: ObjectId(id),
            parent_id: ParentId::NONE,
            world,
            ..NodePose::default()
        }
    }

    fn matrix(scale: [f32; 3], translation: [f32; 3]) -> [[f32; 4]; 4] {
        [
            [scale[0], 0.0, 0.0, translation[0]],
            [0.0, scale[1], 0.0, translation[1]],
            [0.0, 0.0, scale[2], translation[2]],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }

    fn pose(nodes: Vec<NodePose>) -> Pose {
        Pose::from_nodes(Default::default(), nodes).unwrap()
    }

    fn geoset(group: Vec<u32>) -> Geoset {
        Geoset {
            vertices: vec![Vertex {
                position: [1.0, 0.0, 0.0],
            }],
            normals: vec![Normal {
                normal: [1.0, 1.0, 0.0],
            }],
            vertex_groups: vec![0],
            matrix_groups: vec![group],
            ..Geoset::default()
        }
    }

    #[test]
    fn sparse_object_ids_and_four_influences_are_equal_weighted() {
        let two = skin_geoset(
            &geoset(vec![7, 42]),
            &pose(vec![
                node(7, matrix([1.0; 3], [2.0, 0.0, 0.0])),
                node(42, matrix([1.0; 3], [4.0, 0.0, 0.0])),
            ]),
            0,
        )
        .unwrap();
        assert_eq!(two.0[0], [4.0, 0.0, 0.0]);

        let four = skin_geoset(
            &geoset(vec![7, 42, 3, 99]),
            &pose(vec![
                node(7, matrix([1.0; 3], [0.0, 0.0, 0.0])),
                node(42, matrix([1.0; 3], [2.0, 0.0, 0.0])),
                node(3, matrix([1.0; 3], [4.0, 0.0, 0.0])),
                node(99, matrix([1.0; 3], [6.0, 0.0, 0.0])),
            ]),
            0,
        )
        .unwrap();
        assert_eq!(four.0[0], [4.0, 0.0, 0.0]);
    }

    #[test]
    fn nonuniform_scale_uses_inverse_transpose_for_normals() {
        let result = skin_geoset(
            &geoset(vec![7]),
            &pose(vec![node(7, matrix([2.0, 1.0, 1.0], [0.0; 3]))]),
            0,
        )
        .unwrap();
        let expected = [0.4472136, 0.8944272, 0.0];
        for (actual, expected) in result.1[0].iter().zip(expected) {
            assert!((*actual - expected).abs() < 1.0e-5);
        }
    }

    #[test]
    fn invalid_skin_inputs_error_without_panicking() {
        let mut bad_group_count = geoset(vec![7]);
        bad_group_count.vertex_groups.clear();
        bad_group_count.vertex_groups.push(0);
        bad_group_count.vertices.push(Vertex { position: [0.0; 3] });
        bad_group_count.normals.push(Normal {
            normal: [0.0, 0.0, 1.0],
        });
        let mut bad_group_index = geoset(vec![7]);
        bad_group_index.vertex_groups[0] = 1;
        let cases = [
            geoset(vec![]),
            geoset(vec![7, 7]),
            geoset(vec![99]),
            bad_group_count,
            bad_group_index,
        ];
        let expected = [
            "scene-empty-matrix-group",
            "scene-duplicate-skin-object",
            "scene-missing-skin-node",
            "scene-invalid-vertex-group-count",
            "scene-invalid-matrix-group-index",
        ];
        for (geoset, expected) in cases.into_iter().zip(expected) {
            let result = std::panic::catch_unwind(|| {
                skin_geoset(&geoset, &pose(vec![node(7, matrix([1.0; 3], [0.0; 3]))]), 0)
            });
            assert!(result.is_ok());
            assert_eq!(result.unwrap().unwrap_err().key, expected);
        }

        let singular = skin_geoset(
            &geoset(vec![7]),
            &pose(vec![node(7, matrix([0.0, 1.0, 1.0], [0.0; 3]))]),
            0,
        )
        .unwrap_err();
        assert_eq!(singular.key, "scene-singular-normal-matrix");
    }
}
