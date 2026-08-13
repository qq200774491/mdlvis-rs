use super::{dump_structure, inspect_mdx, Count, InspectError};
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
