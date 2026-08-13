use crate::error::MdlError;
use crate::scene::types::{SceneDraw, SceneMesh, TextureTransform};

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub(crate) position: [f32; 3],
    pub(crate) normal: [f32; 3],
    pub(crate) uv: [f32; 2],
}

impl Vertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: (size_of::<[f32; 3]>() * 2) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

pub(crate) fn prepare_draw_vertices(
    mesh: &SceneMesh,
    draw: &SceneDraw,
) -> Result<Vec<Vertex>, MdlError> {
    let uv_set = mesh.uv_sets.get(draw.coord_set as usize).ok_or_else(|| {
        MdlError::new("renderer-invalid-coord-set")
            .with_arg("coord_set", draw.coord_set)
            .with_arg("count", mesh.uv_sets.len())
    })?;
    if uv_set.len() != mesh.positions.len() {
        return Err(MdlError::new("renderer-invalid-uv-count")
            .with_arg("expected", mesh.positions.len())
            .with_arg("actual", uv_set.len()));
    }
    let normals = if mesh.normals.is_empty() {
        reconstruct_normals(mesh)?
    } else {
        if mesh.normals.len() != mesh.positions.len() {
            return Err(MdlError::new("renderer-invalid-normal-count")
                .with_arg("expected", mesh.positions.len())
                .with_arg("actual", mesh.normals.len()));
        }
        mesh.normals.clone()
    };
    Ok(mesh
        .positions
        .iter()
        .zip(normals)
        .zip(uv_set)
        .map(|((position, normal), uv)| Vertex {
            position: *position,
            normal,
            uv: *uv,
        })
        .collect())
}

fn reconstruct_normals(mesh: &SceneMesh) -> Result<Vec<[f32; 3]>, MdlError> {
    let mut normals = vec![[0.0_f32; 3]; mesh.positions.len()];
    for (triangle_ordinal, triangle) in mesh.triangles.iter().enumerate() {
        let [a, b, c] = triangle.map(|index| {
            mesh.positions.get(index as usize).copied().ok_or_else(|| {
                MdlError::new("renderer-invalid-triangle-index")
                    .with_arg("triangle", triangle_ordinal)
                    .with_arg("index", index)
                    .with_arg("count", mesh.positions.len())
            })
        });
        let [a, b, c] = [a?, b?, c?];
        let ab = sub(b, a);
        let ac = sub(c, a);
        let weighted = cross(ab, ac);
        for index in triangle {
            let normal = &mut normals[*index as usize];
            for axis in 0..3 {
                normal[axis] += weighted[axis];
            }
        }
    }
    for normal in &mut normals {
        let length_squared = dot(*normal, *normal);
        if length_squared.is_finite() && length_squared > f32::EPSILON {
            let inverse_length = length_squared.sqrt().recip();
            for component in normal {
                *component *= inverse_length;
            }
        } else {
            *normal = [0.0, 0.0, 1.0];
        }
    }
    Ok(normals)
}

pub(crate) fn texture_matrix(transform: TextureTransform) -> Result<[[f32; 4]; 4], MdlError> {
    let [x, y, z, w] = transform.rotation;
    let length_squared = x * x + y * y + z * z + w * w;
    if !length_squared.is_finite() || length_squared <= f32::EPSILON {
        return Err(MdlError::new("renderer-invalid-texture-rotation"));
    }
    let inverse_length = length_squared.sqrt().recip();
    let [x, y, z, w] = [
        x * inverse_length,
        y * inverse_length,
        z * inverse_length,
        w * inverse_length,
    ];
    let rotation = [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + z * w),
            2.0 * (x * z - y * w),
            0.0,
        ],
        [
            2.0 * (x * y - z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + x * w),
            0.0,
        ],
        [
            2.0 * (x * z + y * w),
            2.0 * (y * z - x * w),
            1.0 - 2.0 * (x * x + y * y),
            0.0,
        ],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let scaling = [
        [transform.scaling[0], 0.0, 0.0, 0.0],
        [0.0, transform.scaling[1], 0.0, 0.0],
        [0.0, 0.0, transform.scaling[2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let translation = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [
            transform.translation[0],
            transform.translation[1],
            transform.translation[2],
            1.0,
        ],
    ];
    let matrix = multiply_matrix(multiply_matrix(scaling, rotation), translation);
    if matrix.iter().flatten().all(|value| value.is_finite()) {
        Ok(matrix)
    } else {
        Err(MdlError::new("renderer-non-finite-texture-matrix"))
    }
}

fn multiply_matrix(left: [[f32; 4]; 4], right: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut result = [[0.0; 4]; 4];
    for column in 0..4 {
        for row in 0..4 {
            result[column][row] = (0..4)
                .map(|inner| left[inner][row] * right[column][inner])
                .sum();
        }
    }
    result
}

fn sub(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}
