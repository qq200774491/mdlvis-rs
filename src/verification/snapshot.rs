use crate::model::model::Model;
use crate::parser::load::load;
use crate::verification::inspect::{inspect_mdx, InspectError, MdxInspection};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status")]
pub enum Count {
    #[serde(rename = "modeled")]
    Modeled { value: usize },
    #[serde(rename = "unmodeled")]
    Unmodeled {
        present: bool,
        estimated: Option<usize>,
    },
}

impl Count {
    fn modeled(value: usize) -> Self {
        Self::Modeled { value }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SequenceSummary {
    pub name: String,
    pub start_frame: u32,
    pub end_frame: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeosetSummary {
    pub vertices: usize,
    pub faces: usize,
    pub vertex_groups: usize,
    pub matrix_groups: usize,
    pub max_bones_per_group: usize,
    pub material_id: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructureSnapshot {
    pub path: String,
    pub file_size: u64,
    pub magic: String,
    pub version: u32,
    pub name: String,
    pub sequences: usize,
    pub geosets: usize,
    pub vertices: usize,
    pub faces: usize,
    pub materials: usize,
    pub layers: usize,
    pub textures: usize,
    pub bones: usize,
    pub helpers: usize,
    pub controllers: usize,
    pub global_sequences: Count,
    pub attachments: Count,
    pub lights: Count,
    pub cameras: Count,
    pub geoset_anims: Count,
    pub texture_anims: Count,
    pub particle_emitters_2: Count,
    pub ribbons: Count,
    pub events: Count,
    pub collisions: Count,
    pub sequence_list: Vec<SequenceSummary>,
    pub geoset_list: Vec<GeosetSummary>,
    pub chunks: Vec<String>,
}

pub fn dump_structure(path: impl AsRef<Path>) -> Result<StructureSnapshot, InspectError> {
    let path = path.as_ref();
    let inspection = inspect_mdx(path)?;
    let mut file = File::open(path)?;
    let model = load(&mut file).map_err(|err| InspectError::Io(err.to_string()))?;
    Ok(from_model(path, &inspection, &model))
}

impl StructureSnapshot {
    pub fn to_pretty_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

fn from_model(path: &Path, inspection: &MdxInspection, model: &Model) -> StructureSnapshot {
    let vertices = model
        .geosets
        .iter()
        .map(|geoset| geoset.vertices.len())
        .sum();
    let faces = model.geosets.iter().map(|geoset| geoset.faces.len()).sum();
    let layers = model
        .materials
        .iter()
        .map(|material| material.layers.len())
        .sum();

    StructureSnapshot {
        path: path.to_string_lossy().into_owned(),
        file_size: inspection.file_size,
        magic: inspection.magic.clone(),
        version: inspection.version,
        name: model.name.trim().to_string(),
        sequences: model.sequences.len(),
        geosets: model.geosets.len(),
        vertices,
        faces,
        materials: model.materials.len(),
        layers,
        textures: model.textures.len(),
        bones: model.bones.len(),
        helpers: model.helpers.len(),
        controllers: model.controllers.len(),
        global_sequences: Count::modeled(model.global_sequences.len()),
        attachments: Count::modeled(model.attachments.len()),
        lights: Count::modeled(model.lights.len()),
        cameras: Count::modeled(model.cameras.len()),
        geoset_anims: Count::modeled(model.geoset_anims.len()),
        texture_anims: Count::modeled(model.texture_anims.len()),
        particle_emitters_2: Count::modeled(model.particle_emitters_2.len()),
        ribbons: Count::modeled(model.ribbons.len()),
        events: Count::modeled(model.events.len()),
        collisions: Count::modeled(model.collisions.len()),
        sequence_list: model
            .sequences
            .iter()
            .map(|sequence| SequenceSummary {
                name: sequence.name.trim_end_matches('\0').trim().to_string(),
                start_frame: sequence.start_frame,
                end_frame: sequence.end_frame,
            })
            .collect(),
        geoset_list: model
            .geosets
            .iter()
            .map(|geoset| GeosetSummary {
                vertices: geoset.vertices.len(),
                faces: geoset.faces.len(),
                vertex_groups: geoset.vertex_groups.len(),
                matrix_groups: geoset.matrix_groups.len(),
                max_bones_per_group: geoset
                    .matrix_groups
                    .iter()
                    .map(|group| group.len())
                    .max()
                    .unwrap_or(0),
                material_id: geoset.material_id,
            })
            .collect(),
        chunks: inspection
            .chunks
            .iter()
            .map(|chunk| chunk.fourcc.clone())
            .collect(),
    }
}
