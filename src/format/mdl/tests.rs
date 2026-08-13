use super::{parse_str, to_string};
use crate::model::chunk::UnknownChunk;
use crate::model::model::Model;
use std::fs::File;
use std::panic::{AssertUnwindSafe, catch_unwind};

const MINIMAL: &str = r#"
Version {
    FormatVersion 800,
}
Model "Round Trip" {
    NumGeosets 1,
    NumBones 1,
    NumEvents 1,
    BlendTime 150,
    MinimumExtent { -1, -2, -3 },
    MaximumExtent { 4, 5, 6 },
    BoundsRadius 7,
}
Sequences 1 {
    Anim "Stand" {
        Interval { 0, 1000 },
        NonLooping,
        MoveSpeed 12.5,
        MinimumExtent { -1, -1, -1 },
        MaximumExtent { 1, 1, 1 },
        BoundsRadius 2,
    }
}
GlobalSequences 1 {
    Duration 3333,
}
Textures 1 {
    Bitmap {
        Image "Textures\\Test.blp",
        WrapWidth,
    }
}
Materials 1 {
    Material {
        PriorityPlane 1,
        Layer {
            FilterMode Blend,
            TwoSided,
            static TextureID 0,
            Alpha 2 {
                Linear,
                0: 0,
                1000: 1,
            }
        }
    }
}
Geoset {
    Vertices 3 {
        { 0, 0, 0 },
        { 1, 0, 0 },
        { 0, 1, 0 },
    }
    Normals 3 {
        { 0, 0, 1 },
        { 0, 0, 1 },
        { 0, 0, 1 },
    }
    TVertices 3 {
        { 0, 0 },
        { 1, 0 },
        { 0, 1 },
    }
    VertexGroup {
        0,
        0,
        0,
    }
    Faces 1 3 {
        Triangles {
            { 0, 1, 2 },
        }
    }
    Groups 1 1 {
        Matrices { 0 },
    }
    MaterialID 0,
    SelectionGroup 0,
    MinimumExtent { 0, 0, 0 },
    MaximumExtent { 1, 1, 0 },
    BoundsRadius 1,
}
Bone "Root" {
    ObjectId 0,
    Translation 2 {
        Hermite,
        0: { 0, 0, 0 },
            InTan { 0, 0, 0 },
            OutTan { 0, 0, 0 },
        1000: { 1, 2, 3 },
            InTan { 0, 0, 0 },
            OutTan { 0, 0, 0 },
    }
    GeosetId 0,
    GeosetAnimId None,
}
PivotPoints 1 {
    { 0, 0, 0 },
}
EventObject "SNDXTEST" {
    ObjectId 1,
    EventTrack 1 {
        GlobalSeqId 0,
        500,
    }
}
TextureAnims 1 {
    TVertexAnim {
        Translation 1 {
            Linear,
            0: { 0, 0, 0 },
        }
    }
}
GeosetAnim {
    static Alpha 0.5,
    static Color { 1, 0.5, 0.25 },
    GeosetId 0,
}
Light "Lamp" {
    ObjectId 2,
    Omnidirectional,
    static AttenuationStart 10,
    static AttenuationEnd 100,
    static Color { 1, 0.5, 0.25 },
    static Intensity 2,
    static AmbColor { 0.1, 0.2, 0.3 },
    static AmbIntensity 0.5,
}
Camera "View" {
    Position { 1, 2, 3 },
    FieldOfView 0.7,
    FarClip 1000,
    NearClip 10,
    Target {
        Position { 0, 0, 0 },
    }
}
"#;

#[test]
fn minimal_model_round_trips_semantically() {
    let first = parse_str(MINIMAL).expect("minimal MDL should parse");
    let text = to_string(&first).expect("minimal model should write");
    assert!(text.contains("FormatVersion 800,"));
    let second = parse_str(&text).expect("written MDL should parse");

    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
}

#[test]
fn unsupported_and_damaged_inputs_return_errors_without_panicking() {
    for source in [
        "Version { FormatVersion 900, } Model \"x\" { BlendTime 0, }",
        "Version { FormatVersion 800, } Model \"unterminated {",
        "Version { FormatVersion 800, } Model \"x\" { BlendTime @, }",
        "Version { FormatVersion 800, } Model \"x\" {",
    ] {
        let result = catch_unwind(AssertUnwindSafe(|| parse_str(source)));
        assert!(result.is_ok(), "parser panicked for {source:?}");
        assert!(result.unwrap().is_err(), "invalid MDL parsed: {source:?}");
    }
}

#[test]
fn invalid_track_widths_return_errors_without_panicking() {
    for source in [
        r#"Version { FormatVersion 800, } Model "x" { } Bone "b" {
            Translation 1 { Hermite,
                0: { 1, 2 },
                InTan { 3, 4 },
                OutTan { 5, 6 },
            }
        }"#,
        r#"Version { FormatVersion 800, } Model "x" { } Bone "b" {
            Rotation 1 { Bezier,
                0: { 1, 2, 3 },
                InTan { 4, 5, 6 },
                OutTan { 7, 8, 9 },
            }
        }"#,
        r#"Version { FormatVersion 800, } Model "x" { } Bone "b" {
            Visibility 1 { Linear,
                0: { 1, 2 },
            }
        }"#,
        r#"Version { FormatVersion 800, } Model "x" { } Bone "b" {
            Translation 1 { Hermite,
                0: { 1, 2, 3 },
                InTan { 4, 5 },
                OutTan { 6, 7, 8 },
            }
        }"#,
        r#"Version { FormatVersion 800, } Model "x" { } Bone "b" {
            Rotation 1 { Bezier,
                0: { 1, 2, 3, 4 },
                InTan { 5, 6, 7, 8 },
                OutTan { 9, 10, 11 },
            }
        }"#,
    ] {
        let result = catch_unwind(AssertUnwindSafe(|| parse_str(source)));
        assert!(result.is_ok(), "parser panicked for {source:?}");
        assert!(result.unwrap().is_err(), "invalid MDL parsed: {source:?}");
    }
}

#[test]
fn accepts_utf8_bom() {
    let model = parse_str("\u{feff}Version { FormatVersion 800, } Model \"bom\" { }")
        .expect("UTF-8 BOM should be accepted");
    assert_eq!(model.name, "bom");
}

#[test]
fn save_path_does_not_overwrite_on_serialization_error() {
    let prefix = format!(
        "mdlvis-mdl-save-safety-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let existing_path = std::env::temp_dir().join(format!("{prefix}-existing.mdl"));
    let missing_path = std::env::temp_dir().join(format!("{prefix}-missing.mdl"));
    let _ = std::fs::remove_file(&missing_path);
    std::fs::write(&existing_path, b"keep me").unwrap();
    let mut model = Model::default();
    model
        .unknown_chunks
        .push(UnknownChunk::new(*b"ZZZZ", vec![1, 2, 3]));

    assert!(super::save_path(&existing_path, &model).is_err());
    assert_eq!(std::fs::read(&existing_path).unwrap(), b"keep me");

    assert!(super::save_path(&missing_path, &model).is_err());
    assert!(!missing_path.exists());

    std::fs::remove_file(existing_path).unwrap();
}

#[test]
fn arthas_fixture_parses_when_available() {
    let path = std::path::Path::new("test-data/Arthas.mdl");
    if !path.exists() {
        return;
    }
    let mut file = File::open(path).unwrap();
    let model = super::load(&mut file).expect("Arthas.mdl should parse");
    assert_eq!(model.name, "Arthas");
    assert_eq!(model.geosets.len(), 4);
    assert_eq!(model.bones.len(), 36);
    assert_eq!(model.helpers.len(), 4);
    assert_eq!(model.events.len(), 13);

    let text = to_string(&model).expect("parsed Arthas model should write");
    let reopened = parse_str(&text).expect("written Arthas model should parse");
    assert_eq!(
        serde_json::to_value(model).unwrap(),
        serde_json::to_value(reopened).unwrap()
    );
}
