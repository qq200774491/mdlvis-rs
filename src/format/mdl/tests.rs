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

const PRE2_STATIC: &str = r#"
Version { FormatVersion 800, }
Model "Static PRE2" { }
ParticleEmitter2 "Smoke" {
    ObjectId 7,
    Blend,
    static Speed 1,
    static Variation 2,
    static Latitude 3,
    static Gravity 4,
    static EmissionRate 5,
    static Width 6,
    static Length 7,
    Head,
}
"#;

const PRE2_ANIMATED: &str = r#"
Version { FormatVersion 800, }
Model "Animated PRE2" { }
GlobalSequences 1 { Duration 2000, }
ParticleEmitter2 "Smoke" {
    ObjectId 7,
    Blend,
    Visibility 1 {
        Linear,
        0: 0.5,
    }
    Speed 1 {
        Linear,
        0: 1,
    }
    Variation 1 {
        Hermite,
        GlobalSeqId 0,
        0: 2,
            InTan 1.5,
            OutTan 2.5,
    }
    Latitude 1 {
        Bezier,
        0: 3,
            InTan 2.5,
            OutTan 3.5,
    }
    Gravity 1 {
        Linear,
        GlobalSeqId 0,
        0: 4,
    }
    EmissionRate 1 {
        Hermite,
        0: 5,
            InTan 4.5,
            OutTan 5.5,
    }
    Width 1 {
        Bezier,
        GlobalSeqId 0,
        0: 6,
            InTan 5.5,
            OutTan 6.5,
    }
    Length 1 {
        Linear,
        0: 7,
    }
    Head,
}
"#;

const PREM_RIBBON_STATIC: &str = r#"
Version { FormatVersion 800, }
Model "Static emitters" { }
ParticleEmitter "TGA particle" {
    ObjectId 10,
    static EmissionRate 1,
    static Gravity 2,
    static Longitude 3,
    static Latitude 4,
    static LifeSpan 5,
    static InitVelocity 6,
    Path "Particles\\Tga.mdl",
    EmitterUsesTGA,
}
ParticleEmitter "MDL particle" {
    ObjectId 11,
    static EmissionRate 7,
    static Gravity 8,
    static Longitude 9,
    static Latitude 10,
    static LifeSpan 11,
    static InitVelocity 12,
    Path "Particles\\Mdl.mdl",
    EmitterUsesMDL,
}
RibbonEmitter "Trail" {
    ObjectId 12,
    Visibility 1 {
        Linear,
        0: 0.75,
    }
    static HeightAbove 13,
    static HeightBelow 14,
    static Alpha 0.5,
    static Color { 0.1, 0.2, 0.3 },
    static TextureSlot 2,
    EmissionRate 15,
    LifeSpan 16,
    Gravity 17,
    Rows 2,
    Columns 3,
    MaterialID 4,
}
"#;

const PREM_RIBBON_ANIMATED: &str = r#"
Version { FormatVersion 800, }
Model "Animated emitters" { }
GlobalSequences 1 { Duration 2000, }
ParticleEmitter "Particle" {
    ObjectId 20,
    EmissionRate 1 {
        Linear,
        0: 1,
    }
    Gravity 1 {
        Hermite,
        GlobalSeqId 0,
        0: 2,
            InTan 1.5,
            OutTan 2.5,
    }
    Longitude 1 {
        Bezier,
        0: 3,
            InTan 2.5,
            OutTan 3.5,
    }
    Latitude 1 {
        Linear,
        GlobalSeqId 0,
        0: 4,
    }
    LifeSpan 1 {
        Hermite,
        0: 5,
            InTan 4.5,
            OutTan 5.5,
    }
    InitVelocity 1 {
        Bezier,
        GlobalSeqId 0,
        0: 6,
            InTan 5.5,
            OutTan 6.5,
    }
    Path "Particles\\Animated.mdl",
    EmitterUsesMDL,
}
RibbonEmitter "Ribbon" {
    ObjectId 21,
    Visibility 1 {
        Linear,
        0: 0.5,
    }
    HeightAbove 1 {
        Linear,
        GlobalSeqId 0,
        0: 7,
    }
    HeightBelow 1 {
        Hermite,
        0: 8,
            InTan 7.5,
            OutTan 8.5,
    }
    Alpha 1 {
        Bezier,
        GlobalSeqId 0,
        0: 0.9,
            InTan 0.8,
            OutTan 1,
    }
    Color 1 {
        Hermite,
        0: { 0.1, 0.2, 0.3 },
            InTan { 0, 0.1, 0.2 },
            OutTan { 0.2, 0.3, 0.4 },
    }
    TextureSlot 1 {
        Linear,
        GlobalSeqId 0,
        0: 2,
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
fn particle_emitter_2_static_fields_round_trip_without_tracks() {
    let first = parse_str(PRE2_STATIC).expect("static PRE2 should parse");
    let emitter = &first.particle_emitters_2[0];
    assert_eq!(
        [
            emitter.speed,
            emitter.variation,
            emitter.latitude,
            emitter.gravity,
            emitter.emission_rate,
            emitter.width,
            emitter.length,
        ],
        [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]
    );
    assert!(
        [
            emitter.speed_track,
            emitter.variation_track,
            emitter.latitude_track,
            emitter.gravity_track,
            emitter.emission_rate_track,
            emitter.width_track,
            emitter.length_track,
        ]
        .into_iter()
        .all(|track| track.is_none())
    );

    let text = to_string(&first).expect("static PRE2 should write");
    for field in [
        "Speed",
        "Variation",
        "Latitude",
        "Gravity",
        "EmissionRate",
        "Width",
        "Length",
    ] {
        assert!(text.contains(&format!("static {field} ")));
    }
    let second = parse_str(&text).expect("written static PRE2 should parse");
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
}

#[test]
fn particle_emitter_2_animated_fields_round_trip_as_scalar_tracks() {
    let first = parse_str(PRE2_ANIMATED).expect("animated PRE2 should parse");
    let emitter = &first.particle_emitters_2[0];
    assert!(!emitter.node.visibility.is_none(), "KP2V must stay bound");

    let tracks = [
        (emitter.speed_track, 1, -1, 1.0, None),
        (emitter.variation_track, 2, 0, 2.0, Some((1.5, 2.5))),
        (emitter.latitude_track, 3, -1, 3.0, Some((2.5, 3.5))),
        (emitter.gravity_track, 1, 0, 4.0, None),
        (emitter.emission_rate_track, 2, -1, 5.0, Some((4.5, 5.5))),
        (emitter.width_track, 3, 0, 6.0, Some((5.5, 6.5))),
        (emitter.length_track, 1, -1, 7.0, None),
    ];
    for (id, interpolation, global_seq_id, value, tangents) in tracks {
        let controller = &first.controllers[id.0 as usize];
        assert_eq!(controller.interpolation_type, interpolation);
        assert_eq!(controller.global_seq_id, global_seq_id);
        assert_eq!(controller.keyframes[0].data, vec![value]);
        match tangents {
            Some((in_tan, out_tan)) => {
                assert_eq!(controller.keyframes[0].in_tan, vec![in_tan]);
                assert_eq!(controller.keyframes[0].out_tan, vec![out_tan]);
            }
            None => {
                assert!(controller.keyframes[0].in_tan.is_empty());
                assert!(controller.keyframes[0].out_tan.is_empty());
            }
        }
    }

    let text = to_string(&first).expect("animated PRE2 should write");
    for field in [
        "Speed",
        "Variation",
        "Latitude",
        "Gravity",
        "EmissionRate",
        "Width",
        "Length",
    ] {
        assert!(text.contains(&format!("\n\t{field} 1 {{")));
        assert!(!text.contains(&format!("static {field} ")));
    }
    let second = parse_str(&text).expect("written animated PRE2 should parse");
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
}

#[test]
fn particle_emitter_2_writer_rejects_invalid_tracks_without_touching_targets() {
    let mut invalid_index = parse_str(PRE2_ANIMATED).unwrap();
    invalid_index.particle_emitters_2[0].speed_track.0 = i32::MAX;
    assert_eq!(
        to_string(&invalid_index).unwrap_err().key,
        "mdl-invalid-controller-index"
    );
    assert_rejected_save_preserves_targets(&invalid_index, "index");

    let mut invalid_width = parse_str(PRE2_ANIMATED).unwrap();
    let index = invalid_width.particle_emitters_2[0].speed_track.0 as usize;
    invalid_width.controllers[index].keyframes[0].data.push(2.0);
    assert_eq!(
        to_string(&invalid_width).unwrap_err().key,
        "mdl-invalid-track-width"
    );
    assert_rejected_save_preserves_targets(&invalid_width, "width");

    let mut invalid_tangent = parse_str(PRE2_ANIMATED).unwrap();
    let index = invalid_tangent.particle_emitters_2[0].variation_track.0 as usize;
    invalid_tangent.controllers[index].keyframes[0]
        .out_tan
        .clear();
    assert_eq!(
        to_string(&invalid_tangent).unwrap_err().key,
        "mdl-invalid-tangent-size"
    );
    assert_rejected_save_preserves_targets(&invalid_tangent, "tangent");
}

#[test]
fn particle_and_ribbon_static_fields_round_trip_without_tracks() {
    let first = parse_str(PREM_RIBBON_STATIC).expect("static PREM/RIBB should parse");
    assert_eq!(
        first.particle_emitters[0].uses_type,
        crate::model::objects::ParticleEmitterUses::Tga
    );
    assert_eq!(
        first.particle_emitters[1].uses_type,
        crate::model::objects::ParticleEmitterUses::Mdl
    );
    for (emitter, expected) in first.particle_emitters.iter().zip([
        [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        [7.0, 8.0, 9.0, 10.0, 11.0, 12.0],
    ]) {
        assert_eq!(
            [
                emitter.emission_rate,
                emitter.gravity,
                emitter.longitude,
                emitter.latitude,
                emitter.life_span,
                emitter.init_velocity,
            ],
            expected
        );
        assert!(
            [
                emitter.emission_rate_track,
                emitter.gravity_track,
                emitter.longitude_track,
                emitter.latitude_track,
                emitter.life_span_track,
                emitter.init_velocity_track,
            ]
            .into_iter()
            .all(|track| track.is_none())
        );
    }

    let ribbon = &first.ribbons[0];
    assert_eq!(
        [ribbon.height_above, ribbon.height_below, ribbon.alpha],
        [13.0, 14.0, 0.5]
    );
    assert_eq!(ribbon.color, [0.1, 0.2, 0.3]);
    assert_eq!(ribbon.texture_slot, 2);
    assert!(
        [
            ribbon.height_above_track,
            ribbon.height_below_track,
            ribbon.alpha_track,
            ribbon.color_track,
            ribbon.texture_slot_track,
        ]
        .into_iter()
        .all(|track| track.is_none())
    );
    assert!(!ribbon.node.visibility.is_none());

    let text = to_string(&first).expect("static PREM/RIBB should write");
    assert!(text.contains("EmitterUsesTGA,"));
    assert!(text.contains("EmitterUsesMDL,"));
    for field in [
        "EmissionRate",
        "Gravity",
        "Longitude",
        "Latitude",
        "LifeSpan",
        "InitVelocity",
        "HeightAbove",
        "HeightBelow",
        "Alpha",
        "Color",
        "TextureSlot",
    ] {
        assert!(text.contains(&format!("static {field} ")));
    }
    let second = parse_str(&text).expect("written static PREM/RIBB should parse");
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
}

#[test]
fn particle_and_ribbon_animated_fields_round_trip_with_typed_widths() {
    let first = parse_str(PREM_RIBBON_ANIMATED).expect("animated PREM/RIBB should parse");
    let emitter = &first.particle_emitters[0];
    assert_eq!(
        emitter.uses_type,
        crate::model::objects::ParticleEmitterUses::Mdl
    );
    let scalar_tracks = [
        (emitter.emission_rate_track, 1, -1),
        (emitter.gravity_track, 2, 0),
        (emitter.longitude_track, 3, -1),
        (emitter.latitude_track, 1, 0),
        (emitter.life_span_track, 2, -1),
        (emitter.init_velocity_track, 3, 0),
    ];
    for (id, interpolation, global_seq_id) in scalar_tracks {
        let controller = &first.controllers[id.0 as usize];
        assert_eq!(controller.interpolation_type, interpolation);
        assert_eq!(controller.global_seq_id, global_seq_id);
        assert_eq!(controller.keyframes[0].data.len(), 1);
        if interpolation >= 2 {
            assert_eq!(controller.keyframes[0].in_tan.len(), 1);
            assert_eq!(controller.keyframes[0].out_tan.len(), 1);
        }
    }

    let ribbon = &first.ribbons[0];
    assert!(
        !ribbon.node.visibility.is_none(),
        "visibility must remain a node track"
    );
    for (id, width, interpolation, global_seq_id) in [
        (ribbon.height_above_track, 1, 1, 0),
        (ribbon.height_below_track, 1, 2, -1),
        (ribbon.alpha_track, 1, 3, 0),
        (ribbon.color_track, 3, 2, -1),
        (ribbon.texture_slot_track, 1, 1, 0),
    ] {
        let controller = &first.controllers[id.0 as usize];
        assert_eq!(controller.interpolation_type, interpolation);
        assert_eq!(controller.global_seq_id, global_seq_id);
        assert_eq!(controller.keyframes[0].data.len(), width);
        if interpolation >= 2 {
            assert_eq!(controller.keyframes[0].in_tan.len(), width);
            assert_eq!(controller.keyframes[0].out_tan.len(), width);
        }
    }

    let text = to_string(&first).expect("animated PREM/RIBB should write");
    for field in [
        "EmissionRate",
        "Gravity",
        "Longitude",
        "Latitude",
        "LifeSpan",
        "InitVelocity",
        "HeightAbove",
        "HeightBelow",
        "Alpha",
        "Color",
        "TextureSlot",
    ] {
        assert!(text.contains(&format!("\n\t{field} 1 {{")));
        assert!(!text.contains(&format!("static {field} ")));
    }
    let second = parse_str(&text).expect("written animated PREM/RIBB should parse");
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
}

#[test]
fn particle_and_ribbon_writer_reject_invalid_tracks_without_touching_targets() {
    let mut invalid_index = parse_str(PREM_RIBBON_ANIMATED).unwrap();
    invalid_index.particle_emitters[0].emission_rate_track.0 = i32::MAX;
    assert_eq!(
        to_string(&invalid_index).unwrap_err().key,
        "mdl-invalid-controller-index"
    );
    assert_rejected_save_preserves_targets(&invalid_index, "prem-index");

    let mut invalid_width = parse_str(PREM_RIBBON_ANIMATED).unwrap();
    let index = invalid_width.ribbons[0].color_track.0 as usize;
    invalid_width.controllers[index].keyframes[0].data.pop();
    assert_eq!(
        to_string(&invalid_width).unwrap_err().key,
        "mdl-invalid-track-width"
    );
    assert_rejected_save_preserves_targets(&invalid_width, "ribbon-width");

    let mut invalid_tangent = parse_str(PREM_RIBBON_ANIMATED).unwrap();
    let index = invalid_tangent.particle_emitters[0].life_span_track.0 as usize;
    invalid_tangent.controllers[index].keyframes[0]
        .in_tan
        .clear();
    assert_eq!(
        to_string(&invalid_tangent).unwrap_err().key,
        "mdl-invalid-tangent-size"
    );
    assert_rejected_save_preserves_targets(&invalid_tangent, "prem-tangent");
}

#[test]
fn mdlvis_data_is_rejected_without_touching_targets() {
    let model = Model {
        mdlvis_data: Some(vec![1, 2, 3]),
        ..Model::default()
    };
    assert_eq!(
        to_string(&model).unwrap_err().key,
        "mdl-mdlvis-data-not-representable"
    );
    assert_rejected_save_preserves_targets(&model, "mdlvis-data");
}

fn assert_rejected_save_preserves_targets(model: &Model, case: &str) {
    let prefix = format!(
        "mdlvis-mdl-pre2-{case}-{}-{}",
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

    assert!(super::save_path(&existing_path, model).is_err());
    assert_eq!(std::fs::read(&existing_path).unwrap(), b"keep me");
    assert!(super::save_path(&missing_path, model).is_err());
    assert!(!missing_path.exists());

    std::fs::remove_file(existing_path).unwrap();
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
fn repeated_tvertices_survive_full_mdl_round_trip() {
    let second_set = r#"
    TVertices 3 {
        { 0.25, 0.75 },
        { 0.5, 0.5 },
        { 0.75, 0.25 },
    }
"#;
    let source = MINIMAL.replacen(
        "    VertexGroup {",
        &format!("{second_set}    VertexGroup {{"),
        1,
    );
    let model = parse_str(&source).expect("parse two UV sets");
    assert_eq!(model.geosets[0].tex_coord_sets.len(), 2);
    assert_ne!(
        model.geosets[0].tex_coord_sets[0][0].uv,
        model.geosets[0].tex_coord_sets[1][0].uv
    );

    let text = to_string(&model).expect("write two UV sets");
    assert_eq!(text.matches("\tTVertices 3 {").count(), 2);
    let reparsed = parse_str(&text).expect("reparse two UV sets");
    assert_eq!(
        serde_json::to_value(reparsed).unwrap(),
        serde_json::to_value(&model).unwrap()
    );

    let mut no_uv = model;
    no_uv.geosets[0].tex_coord_sets.clear();
    let text = to_string(&no_uv).expect("write zero UV sets");
    let reparsed = parse_str(&text).expect("reparse zero UV sets");
    assert_eq!(
        serde_json::to_value(reparsed).unwrap(),
        serde_json::to_value(no_uv).unwrap()
    );
}

#[test]
fn invalid_mdl_uv_set_size_preserves_targets_and_damaged_lists_error() {
    let mut model = parse_str(MINIMAL).unwrap();
    model.geosets[0].tex_coord_sets[0].pop();
    let err = to_string(&model).expect_err("short UV set must be rejected");
    assert_eq!(err.key, "mdl-invalid-uv-set-size");
    assert_rejected_save_preserves_targets(&model, "invalid-uv-size");

    let damaged = MINIMAL.replacen("TVertices 3 {", "TVertices 4 {", 1);
    let result = catch_unwind(AssertUnwindSafe(|| parse_str(&damaged)));
    assert!(result.is_ok(), "damaged TVertices list panicked");
    assert!(result.unwrap().is_err(), "damaged TVertices list parsed");
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
fn tracked_ember_forge_geosets_follow_original_tail_and_scan_contract() {
    let path = std::path::Path::new("test-data/Ember Forge  Ember Knight/Ember Forge_opt2.mdx");
    let mut file = File::open(path).expect("tracked Ember Forge sample should exist");
    let model = crate::parser::load::load(&mut file).expect("tracked Ember Forge should load");
    assert_eq!(model.geosets.len(), 11);

    let text = to_string(&model).expect("tracked Ember Forge should write as MDL");
    let blocks = top_level_geoset_blocks(&text);
    assert_eq!(blocks.len(), 11);

    for (index, block) in blocks.iter().enumerate() {
        let minimum = block.find("\tMinimumExtent ").expect("MinimumExtent");
        let maximum = block.find("\tMaximumExtent ").expect("MaximumExtent");
        let radius = block.find("\tBoundsRadius ").expect("BoundsRadius");
        let material = block.find("\tMaterialID ").expect("MaterialID");
        let selection = block.find("\tSelectionGroup ").expect("SelectionGroup");
        assert!(
            minimum < maximum && maximum < radius && radius < material && material < selection,
            "geoset {index} must write bounds before the original parser tail"
        );
        if let Some(unselectable) = block.find("\tUnselectable,") {
            assert!(selection < unselectable);
        }

        let first_close_after_tail = selection
            + block[selection..]
                .find('}')
                .expect("geoset tail must be closed");
        assert_eq!(
            first_close_after_tail,
            block.rfind('}').unwrap(),
            "geoset {index} tail must lead directly to the top-level close"
        );
    }
}

fn top_level_geoset_blocks(text: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut search_from = 0;
    while let Some(relative) = text[search_from..].find("\nGeoset {\n") {
        let start = search_from + relative + 1;
        let open = start + "Geoset ".len();
        let mut depth = 0;
        for (relative, character) in text[open..].char_indices() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let end = open + relative + 1;
                        blocks.push(&text[start..end]);
                        search_from = end;
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    blocks
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
