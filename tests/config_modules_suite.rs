use tui_visualizer::audio::AudioFeatures;
use tui_visualizer::control_matrix::{ControlMatrix, ControlMatrixError, ControlState};
use tui_visualizer::preset_graph::{GraphOp, PresetGraph, PresetGraphError};
use tui_visualizer::theme_pack::{ThemePackError, ThemePackManifest};

fn sample_audio() -> AudioFeatures {
    AudioFeatures {
        rms: 1.0,
        bands: [0.9, 0.8, 0.7, 0.4, 0.3, 0.2, 0.1, 0.0],
        onset: 0.65,
        beat: true,
        beat_strength: 0.9,
        centroid: 0.45,
        flatness: 0.25,
    }
}

#[test]
fn preset_graph_parses_and_compiles() {
    let text = r#"
        node intro 0
        node drop 3
        node outro 7
        edge intro drop on_beat
        edge drop outro beat_ge 0.70
    "#;

    let graph = PresetGraph::parse(text).expect("graph parse should succeed");
    let ir = graph.compile().expect("graph compile should succeed");
    assert_eq!(ir.nodes.len(), 3);
    assert_eq!(ir.adjacency[0].len(), 1);
    assert_eq!(ir.adjacency[0][0].op, GraphOp::OnBeat);
}

#[test]
fn preset_graph_compile_rejects_unknown_refs() {
    let text = r#"
        node a 0
        edge a missing always
    "#;

    let graph = PresetGraph::parse(text).expect("graph parse should succeed");
    let err = graph.compile().expect_err("compile should fail for invalid ref");
    assert!(matches!(err, PresetGraphError::UnknownNodeRef { .. }));
}

#[test]
fn preset_graph_compile_rejects_cycles() {
    let text = r#"
        node a 1
        node b 2
        edge a b always
        edge b a always
    "#;

    let graph = PresetGraph::parse(text).expect("graph parse should succeed");
    let err = graph.compile().expect_err("compile should fail for cycle");
    assert!(matches!(err, PresetGraphError::CycleDetected { .. }));
}

#[test]
fn preset_graph_parse_rejects_invalid_op_arguments() {
    let text = r#"
        node a 0
        node b 1
        edge a b on_beat 0.5
    "#;

    let err = PresetGraph::parse(text).expect_err("extra argument for on_beat must fail");
    assert!(matches!(err, PresetGraphError::Parse { .. }));
}

#[test]
fn preset_graph_parse_rejects_out_of_range_chance() {
    let text = r#"
        node a 0
        node b 1
        edge a b chance 1.5
    "#;

    let err = PresetGraph::parse(text).expect_err("chance outside [0,1] must fail");
    assert!(matches!(err, PresetGraphError::Parse { .. }));
}

#[test]
fn control_matrix_routes_and_clamps() {
    let text = r#"
        route zoom bass smoothstep 1.0 2.0 0.0 0.0 1.0
        route hue centroid linear 1.0 1.0 0.0 0.0 1.0
    "#;
    let matrix = ControlMatrix::parse(text).expect("matrix parse should succeed");
    let mut state = ControlState::default();
    let out = matrix.evaluate(&sample_audio(), &mut state);

    let zoom = out.get("zoom").copied().expect("zoom output should exist");
    let hue = out.get("hue").copied().expect("hue output should exist");
    assert!((0.0..=1.0).contains(&zoom));
    assert!((0.0..=1.0).contains(&hue));
}

#[test]
fn control_matrix_applies_smoothing() {
    let text = "route zoom rms linear 0.5 1.0 0.0 0.0 1.0";
    let matrix = ControlMatrix::parse(text).expect("matrix parse should succeed");
    let mut state = ControlState::default();

    let mut hot = AudioFeatures::default();
    hot.rms = 1.0;
    let first = matrix
        .evaluate(&hot, &mut state)
        .get("zoom")
        .copied()
        .expect("zoom output should exist");
    assert!((first - 1.0).abs() < 1e-6);

    let mut cold = AudioFeatures::default();
    cold.rms = 0.0;
    let second = matrix
        .evaluate(&cold, &mut state)
        .get("zoom")
        .copied()
        .expect("zoom output should exist");
    assert!((second - 0.5).abs() < 1e-6);
}

#[test]
fn control_matrix_rejects_duplicate_controls() {
    let text = r#"
        route zoom rms linear 1.0 1.0 0.0 0.0 1.0
        route zoom onset linear 1.0 1.0 0.0 0.0 1.0
    "#;
    let err = ControlMatrix::parse(text).expect_err("duplicate control should fail");
    assert!(matches!(err, ControlMatrixError::DuplicateControl(_)));
}

#[test]
fn control_matrix_rejects_invalid_bounds() {
    let text = "route zoom rms linear 1.0 1.0 0.0 2.0 1.0";
    let err = ControlMatrix::parse(text).expect_err("min > max should fail");
    assert!(matches!(err, ControlMatrixError::InvalidBounds { .. }));
}

#[test]
fn control_matrix_rejects_unknown_feature() {
    let text = "route zoom unknown_feature linear 1.0 1.0 0.0 0.0 1.0";
    let err = ControlMatrix::parse(text).expect_err("unknown feature should fail");
    assert!(matches!(err, ControlMatrixError::Parse { .. }));
}

#[test]
fn control_matrix_round_trip_text() {
    let text = "route zoom bass smoothstep 0.4 1.2 0.1 0.0 2.0";
    let matrix = ControlMatrix::parse(text).expect("matrix parse should succeed");
    let serialized = matrix.to_text();
    let parsed = ControlMatrix::parse(&serialized).expect("matrix reparse should succeed");
    assert_eq!(parsed.routes(), matrix.routes());
}

#[test]
fn theme_pack_parses_manifest() {
    let text = r#"
        name=Neon Drift
        tags=night,energetic
        presets=1,5,8
        transition.min_beats=8
        transition.max_beats=24
        transition.crossfade_ms=450
        defaults.intensity=0.85
        defaults.zoom=1.2
    "#;
    let pack = ThemePackManifest::parse(text).expect("theme pack parse should succeed");
    assert_eq!(pack.name, "Neon Drift");
    assert_eq!(pack.tags, vec!["night".to_string(), "energetic".to_string()]);
    assert_eq!(pack.preset_indices, vec![1, 5, 8]);
}

#[test]
fn theme_pack_requires_presets() {
    let text = r#"
        name=No Presets
        tags=test
        transition.min_beats=4
        transition.max_beats=8
        transition.crossfade_ms=120
        defaults.intensity=1.0
        defaults.zoom=1.0
    "#;
    let err = ThemePackManifest::parse(text).expect_err("missing presets should fail");
    assert!(matches!(err, ThemePackError::MissingField("presets")));
}

#[test]
fn theme_pack_rejects_invalid_transition_bounds() {
    let text = r#"
        name=Bad Transition
        tags=test
        presets=2,4
        transition.min_beats=12
        transition.max_beats=4
        transition.crossfade_ms=200
        defaults.intensity=1.0
        defaults.zoom=1.0
    "#;
    let err = ThemePackManifest::parse(text).expect_err("invalid transition should fail");
    assert!(matches!(
        err,
        ThemePackError::InvalidValue {
            field: "transition.max_beats",
            ..
        }
    ));
}

#[test]
fn theme_pack_rejects_duplicate_preset_index() {
    let text = r#"
        name=Duplicate Preset
        tags=test
        presets=2,2
        transition.min_beats=4
        transition.max_beats=8
        transition.crossfade_ms=200
        defaults.intensity=1.0
        defaults.zoom=1.0
    "#;
    let err = ThemePackManifest::parse(text).expect_err("duplicate preset index should fail");
    assert!(matches!(err, ThemePackError::DuplicatePresetIndex(2)));
}

#[test]
fn theme_pack_rejects_unknown_key() {
    let text = r#"
        name=Unknown Key
        tags=test
        presets=1
        transition.min_beats=4
        transition.max_beats=8
        transition.crossfade_ms=120
        defaults.intensity=1.0
        defaults.zoom=1.0
        defaults.extra=1
    "#;
    let err = ThemePackManifest::parse(text).expect_err("unknown key should fail");
    assert!(matches!(err, ThemePackError::Parse { .. }));
}
