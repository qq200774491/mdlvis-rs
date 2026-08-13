use super::{dump_structure, inspect_mdx, Count, InspectError};
use crate::error::MdlError;
use crate::parser::load::load;
use std::fs::File;
use std::path::{Path, PathBuf};

fn test_data(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-data")
        .join(rel)
}

fn require_present(count: &Count) {
    match count {
        Count::Unmodeled { present, .. } => assert!(present),
        Count::Modeled { .. } => panic!("expected unmodeled count"),
    }
}

fn assert_repeatable_dump(path: &Path) {
    let first = dump_structure(path).expect("first dump");
    let second = dump_structure(path).expect("second dump");
    assert_eq!(
        first, second,
        "consecutive dumps must be semantically equal"
    );
    let first_json = first.to_pretty_json().expect("serialize first dump");
    let second_json = second.to_pretty_json().expect("serialize second dump");
    assert_eq!(
        first_json, second_json,
        "consecutive dump JSON must be identical"
    );
}

#[test]
fn nether_blast_i_structure() {
    let path = test_data("Nether Blast/Nether Blast I.mdx");
    let inspection = inspect_mdx(&path).expect("inspect Nether Blast I");
    assert_eq!(inspection.magic, "MDLX");
    assert_eq!(inspection.version, 800);
    assert!(inspection.has_chunk("GEOS"));
    assert!(inspection.has_chunk("SEQS"));
    assert!(inspection.has_chunk("GLBS"));
    assert!(inspection.has_chunk("GEOA"));
    assert!(inspection.has_chunk("PRE2"));

    let snap = dump_structure(&path).expect("dump Nether Blast I");
    assert_eq!(snap.sequences, 1);
    assert_eq!(snap.geosets, 1);
    assert_eq!(snap.vertices, 57);
    assert_eq!(snap.faces, 98);
    assert_eq!(snap.bones, 1);
    assert_eq!(snap.helpers, 0);
    assert_eq!(snap.geoset_list[0].max_bones_per_group, 1);
    assert_eq!(snap.sequence_list[0].name, "Birth");
    assert_eq!(snap.sequence_list[0].start_frame, 7633);
    assert_eq!(snap.sequence_list[0].end_frame, 8900);
    let json = snap.to_pretty_json().expect("serialize snapshot");
    assert!(json.contains("\"status\": \"unmodeled\""));
    require_present(&snap.global_sequences);
    require_present(&snap.geoset_anims);
    require_present(&snap.particle_emitters_2);
}

#[test]
fn ember_forge_structure() {
    let path = test_data("Ember Forge  Ember Knight/Ember Forge_opt2.mdx");
    let inspection = inspect_mdx(&path).expect("inspect Ember Forge");
    assert_eq!(inspection.version, 800);
    assert!(inspection.has_chunk("TXAN"));
    assert!(inspection.has_chunk("GEOA"));
    assert!(inspection.has_chunk("CAMS"));

    let snap = dump_structure(&path).expect("dump Ember Forge");
    assert_eq!(snap.sequences, 12);
    assert_eq!(snap.geosets, 11);
    assert_eq!(snap.vertices, 4077);
    assert_eq!(snap.faces, 4524);
    assert_eq!(snap.bones, 41);
    assert_eq!(snap.helpers, 35);
    assert_eq!(snap.materials, 12);
    assert_eq!(snap.layers, 24);
    require_present(&snap.texture_anims);
    require_present(&snap.geoset_anims);
    require_present(&snap.lights);
}

#[test]
fn arthas_local_structure_if_present() {
    let path = test_data("Arthas.mdx");
    if !path.exists() {
        return;
    }
    let snap = dump_structure(&path).expect("dump Arthas");
    assert_eq!(snap.version, 800);
    assert_eq!(snap.sequences, 13);
    assert_eq!(snap.geosets, 4);
    assert_eq!(snap.vertices, 650);
    assert_eq!(snap.faces, 531);
    assert_eq!(snap.bones, 36);
    assert_eq!(snap.helpers, 4);
    require_present(&snap.global_sequences);
}

#[test]
fn qdmr_is_rejected() {
    let path = test_data("QDMR.mdx");
    match inspect_mdx(&path) {
        Err(InspectError::UnsupportedMagic(magic)) => assert_eq!(&magic, b"EMHM"),
        other => panic!("expected unsupported magic, got {other:?}"),
    }
}

fn assert_error_key(err: MdlError, key: &str) {
    assert_eq!(err.key, key, "display={}", err);
}

fn load_path(path: &Path) -> Result<crate::model::model::Model, MdlError> {
    let mut file = File::open(path).expect("open sample");
    load(&mut file)
}

fn write_temp_mdx(label: &str, bytes: &[u8]) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("mdlvis-rs-g0-{}-{}.mdx", label, std::process::id()));
    std::fs::write(&path, bytes).expect("write temp mdx");
    path
}

#[test]
fn nether_blast_i_loads() {
    let path = test_data("Nether Blast/Nether Blast I.mdx");
    let model = load_path(&path).expect("production load must accept VERS 800 MDLX");
    assert_eq!(model.geosets.len(), 1);
    assert_eq!(model.sequences.len(), 1);
}

#[test]
fn qdmr_is_rejected_by_load() {
    let path = test_data("QDMR.mdx");
    let err = load_path(&path).expect_err("production load must reject non-MDLX");
    assert_error_key(err, "unsupported-magic");
}

#[test]
fn load_rejects_missing_vers() {
    // Magic only: no chunks, so VERS is absent without triggering a short read
    // inside a modeled block such as MODL.
    let path = write_temp_mdx("missing-vers", b"MDLX");
    let err = load_path(&path).expect_err("production load must require VERS");
    let _ = std::fs::remove_file(&path);
    assert_error_key(err, "missing-vers");
}

#[test]
fn load_rejects_unsupported_version() {
    let mut bytes = b"MDLX".to_vec();
    bytes.extend_from_slice(b"VERS");
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(&900u32.to_le_bytes());
    let path = write_temp_mdx("vers-900", &bytes);
    let err = load_path(&path).expect_err("production load must reject non-800");
    let _ = std::fs::remove_file(&path);
    assert_error_key(err, "unsupported-version");
}

#[test]
fn structure_dump_is_repeatable() {
    assert_repeatable_dump(&test_data("Nether Blast/Nether Blast I.mdx"));
    assert_repeatable_dump(&test_data("Ember Forge  Ember Knight/Ember Forge_opt2.mdx"));
    let arthas = test_data("Arthas.mdx");
    if arthas.exists() {
        assert_repeatable_dump(&arthas);
    }
}

#[test]
fn loaded_model_exposes_identified_collections() {
    let model = load_path(&test_data("Nether Blast/Nether Blast I.mdx"))
        .expect("VERS 800 MDLX still loads");
    // Parser does not fill these yet. Empty means unparsed, not "chunk absent".
    assert!(model.global_sequences.is_empty());
    assert!(model.geoset_anims.is_empty());
    assert!(model.texture_anims.is_empty());
    assert!(model.attachments.is_empty());
    assert!(model.lights.is_empty());
    assert!(model.particle_emitters_2.is_empty());
    assert!(model.unknown_chunks.is_empty());
    assert!(model.pivot_points.is_empty());
}
