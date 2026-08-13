use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vertex {
    pub position: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Normal {
    pub normal: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TexCoord {
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Face {
    pub vertices: [u32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Geoset {
    pub vertices: Vec<Vertex>,
    pub normals: Vec<Normal>,
    pub tex_coord_sets: Vec<Vec<TexCoord>>,
    pub faces: Vec<Face>,
    pub material_id: Option<usize>,
    pub selection_group: usize,
    pub unselectable: bool,
    pub bounds_radius: f32,
    pub minimum_extent: [f32; 3],
    pub maximum_extent: [f32; 3],
    // Animation data
    pub vertex_groups: Vec<u8>, // GNDX: Index into matrix_groups for each vertex
    pub matrix_groups: Vec<Vec<u32>>, // MTGC+MATS: Groups of bone indices
}

impl Default for Geoset {
    fn default() -> Self {
        Self {
            vertices: Vec::new(),
            normals: Vec::new(),
            tex_coord_sets: Vec::new(),
            faces: Vec::new(),
            material_id: None,
            selection_group: 0,
            unselectable: false,
            bounds_radius: 0.0,
            minimum_extent: [0.0; 3],
            maximum_extent: [0.0; 3],
            vertex_groups: Vec::new(),
            matrix_groups: Vec::new(),
        }
    }
}
