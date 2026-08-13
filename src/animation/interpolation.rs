// Interpolation utilities
// Based on mdlDraw.pas interpolation functions

use nalgebra_glm as glm;

/// Convert quaternion to rotation matrix
/// Based on QuaternionToMatrix in mdlDraw.pas (line 1951)
pub fn quaternion_to_matrix(q: &glm::Quat) -> glm::Mat3 {
    // Normalize quaternion
    let q = glm::quat_normalize(q);

    // Delphi code uses x2 = x+x instead of x*2
    let x2 = q.i + q.i;
    let y2 = q.j + q.j;
    let z2 = q.k + q.k;

    let xx = q.i * x2;
    let xy = q.i * y2;
    let xz = q.i * z2;
    let yy = q.j * y2;
    let yz = q.j * z2;
    let zz = q.k * z2;
    let wx = q.w * x2;
    let wy = q.w * y2;
    let wz = q.w * z2;

    // Matrix layout from Delphi:
    // m[0,0]:=1.0-(yy+zz); m[1,0]:=xy-wz;       m[2,0]:=xz+wy;
    // m[0,1]:=xy+wz;       m[1,1]:=1.0-(xx+zz); m[2,1]:=yz-wx;
    // m[0,2]:=xz-wy;       m[1,2]:=yz+wx;       m[2,2]:=1.0-(xx+yy);
    // Delphi uses row-major [row,col], but glm::mat3 is column-major
    // So we need to transpose the matrix (swap rows and columns)

    glm::mat3(
        1.0 - (yy + zz),
        xy - wz,
        xz + wy, // column 0 (was row 0)
        xy + wz,
        1.0 - (xx + zz),
        yz - wx, // column 1 (was row 1)
        xz - wy,
        yz + wx,
        1.0 - (xx + yy), // column 2 (was row 2)
    )
}

/// Multiply two 3x3 matrices
/// Based on MulMatrices in mdlDraw.pas
pub fn mul_matrices(a: &glm::Mat3, b: &glm::Mat3) -> glm::Mat3 {
    a * b
}

/// Apply scaling to rotation matrix
/// From InterpTBone procedure - scales each column by corresponding scale component
pub fn apply_scaling_to_matrix(matrix: &glm::Mat3, scaling: &glm::Vec3) -> glm::Mat3 {
    glm::mat3(
        matrix[(0, 0)] * scaling.x,
        matrix[(1, 0)] * scaling.x,
        matrix[(2, 0)] * scaling.x,
        matrix[(0, 1)] * scaling.y,
        matrix[(1, 1)] * scaling.y,
        matrix[(2, 1)] * scaling.y,
        matrix[(0, 2)] * scaling.z,
        matrix[(1, 2)] * scaling.z,
        matrix[(2, 2)] * scaling.z,
    )
}

/// SLERP (Spherical Linear Interpolation) for quaternions
#[allow(dead_code)]
pub fn quat_slerp(q1: &glm::Quat, q2: &glm::Quat, t: f32) -> glm::Quat {
    glm::quat_slerp(q1, q2, t)
}

/// Linear interpolation for vectors
#[allow(dead_code)]
pub fn lerp_vec3(v1: &glm::Vec3, v2: &glm::Vec3, t: f32) -> glm::Vec3 {
    glm::lerp(v1, v2, t)
}

/// Linear interpolation for scalars
#[allow(dead_code)]
pub fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub(crate) fn linear<const N: usize>(start: [f32; N], end: [f32; N], t: f32) -> [f32; N] {
    std::array::from_fn(|index| lerp_f32(start[index], end[index], t))
}

pub(crate) fn hermite<const N: usize>(
    start: [f32; N],
    start_tangent: [f32; N],
    end: [f32; N],
    end_tangent: [f32; N],
    t: f32,
) -> [f32; N] {
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    std::array::from_fn(|index| {
        start[index] * h00
            + start_tangent[index] * h10
            + end[index] * h01
            + end_tangent[index] * h11
    })
}

pub(crate) fn bezier<const N: usize>(
    start: [f32; N],
    start_control: [f32; N],
    end: [f32; N],
    end_control: [f32; N],
    t: f32,
) -> [f32; N] {
    let inverse = 1.0 - t;
    let f1 = inverse * inverse * inverse;
    let f2 = 3.0 * t * inverse * inverse;
    let f3 = 3.0 * t * t * inverse;
    let f4 = t * t * t;
    std::array::from_fn(|index| {
        start[index] * f1 + start_control[index] * f2 + end_control[index] * f3 + end[index] * f4
    })
}

pub(crate) fn slerp_quaternion(start: [f32; 4], end: [f32; 4], t: f32, threshold: f32) -> [f32; 4] {
    let start = normalize_quaternion(start);
    let mut end = normalize_quaternion(end);
    let mut cosine = dot(start, end);
    if cosine < 0.0 {
        cosine = -cosine;
        end = end.map(|value| -value);
    }
    cosine = cosine.clamp(-1.0, 1.0);
    let (start_scale, end_scale) = if 1.0 - cosine > threshold {
        let omega = cosine.acos();
        let sine = omega.sin();
        (((1.0 - t) * omega).sin() / sine, (t * omega).sin() / sine)
    } else {
        (1.0 - t, t)
    };
    normalize_quaternion(std::array::from_fn(|index| {
        start[index] * start_scale + end[index] * end_scale
    }))
}

pub(crate) fn nested_slerp_quaternion(
    start: [f32; 4],
    start_control: [f32; 4],
    end: [f32; 4],
    end_control: [f32; 4],
    t: f32,
) -> [f32; 4] {
    let values = slerp_quaternion(start, end, t, 1.0e-5);
    let controls = slerp_quaternion(start_control, end_control, t, 1.0e-5);
    slerp_quaternion(values, controls, 2.0 * t * (1.0 - t), 1.0e-5)
}

fn dot(left: [f32; 4], right: [f32; 4]) -> f32 {
    left.into_iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

fn normalize_quaternion(value: [f32; 4]) -> [f32; 4] {
    let length = dot(value, value).sqrt();
    value.map(|component| component / length)
}
