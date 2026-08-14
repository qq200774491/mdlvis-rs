use crate::error::MdlError;
use crate::parser::load::load;
use crate::renderer::camera::CameraState;
use crate::scene::{ScenePacket, build_scene_packet};
use crate::texture::archive::{StormArchiveSource, War3ArchivePaths};
use crate::texture::loader::TextureLoadResult;
use crate::texture::manager::TextureStatus;
use crate::texture::scene::{
    ResolvedSceneTexture, SceneTextureError, SceneTextureResolver, TextureByteSource,
    TextureSourceError,
};
use egui_wgpu::ScreenDescriptor;
use std::fs::File;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

/// Temporary helper to access the global AppHandler registered in `handler_registry`.
/// Unsafe: returns a mutable reference from a raw pointer. Use only in quick refactor.
pub fn get_global_handler_mut() -> Option<&'static mut crate::app::handler::AppHandler> {
    if let Some(raw) = crate::app::handler_registry::get_raw() {
        unsafe { Some(&mut *(raw as *mut crate::app::handler::AppHandler)) }
    } else {
        None
    }
}

pub struct EventResponse {
    pub repaint: bool,
    pub exit: bool,
}

pub struct App {
    integration: IntegrationState,
    pub(crate) scene_error: Option<MdlError>,
    sticky_load_error: Option<MdlError>,
    last_reported_scene_error: Option<String>,
}

impl App {
    pub(crate) fn new() -> Self {
        Self {
            integration: IntegrationState::default(),
            scene_error: None,
            sticky_load_error: None,
            last_reported_scene_error: None,
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::animation::types::{FrameContext, PlaybackMode};
    use crate::model::ids::TextureIndex;
    use crate::scene::SceneTextureRequest;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    fn tracked(relative: &str) -> crate::model::model::Model {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-data")
            .join(relative);
        let mut file = File::open(path).unwrap();
        load(&mut file).unwrap()
    }

    fn tracked_frame(model: &crate::model::model::Model) -> FrameContext {
        FrameContext {
            sequence: (!model.sequences.is_empty()).then_some(0),
            sequence_time: model
                .sequences
                .first()
                .map_or(0.0, |sequence| f64::from(sequence.start_frame)),
            global_time: 0.0,
            playback: PlaybackMode::Clamp,
            view: Some(crate::animation::types::ViewFrame::default()),
        }
    }

    #[test]
    fn tracked_models_prepare_nonempty_scenes_for_three_frames() {
        for relative in [
            "Nether Blast/Nether Blast I.mdx",
            "Ember Forge  Ember Knight/Ember Forge_opt2.mdx",
        ] {
            let model = tracked(relative);
            let reads = Arc::new(Mutex::new(BTreeMap::<String, usize>::new()));
            let reads_for_source = Arc::clone(&reads);
            let mut resolver = SceneTextureResolver::new(move |path: &str| {
                *reads_for_source
                    .lock()
                    .unwrap()
                    .entry(path.to_string())
                    .or_default() += 1;
                Ok(None)
            });
            for global_time in [0.0, 1.0, 2.0] {
                let mut frame = tracked_frame(&model);
                frame.global_time = global_time;
                let scene =
                    prepare_cpu_scene(&model, frame, &mut resolver, [1.0, 0.0, 0.0]).unwrap();
                assert!(!scene.packet.meshes.is_empty());
                assert!(!scene.packet.draws.is_empty());
                assert_eq!(scene.textures.len(), scene.packet.textures.len());
            }
            assert!(reads.lock().unwrap().values().all(|count| *count == 3));
        }
    }

    #[test]
    fn team_color_re_resolves_without_re_reading_cached_blp() {
        let blp = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("test-data/Ember Forge  Ember Knight/Ember Knight/EmberKnight.blp"),
        )
        .unwrap();
        let reads = Arc::new(Mutex::new(0));
        let reads_for_source = Arc::clone(&reads);
        let mut resolver = SceneTextureResolver::new(move |_path: &str| {
            *reads_for_source.lock().unwrap() += 1;
            Ok(Some(blp.clone()))
        });
        let regular = SceneTextureRequest {
            index: TextureIndex(0),
            filename: "missing.blp".into(),
            replaceable_id: 0,
            wrap_u: false,
            wrap_v: false,
        };
        let team = SceneTextureRequest {
            index: TextureIndex(1),
            filename: String::new(),
            replaceable_id: 1,
            wrap_u: false,
            wrap_v: false,
        };
        let first = resolver
            .resolve_all_canonical(&[regular.clone(), team.clone()], [1.0, 0.0, 0.0])
            .unwrap();
        let second = resolver
            .resolve_all_canonical(&[regular, team], [0.0, 1.0, 0.0])
            .unwrap();
        assert_eq!(*reads.lock().unwrap(), 1, "decoded assets must be cached");
        assert_ne!(first[1].rgba, second[1].rgba);
    }

    #[test]
    fn global_clock_is_independent_and_pauses_without_resume_jump() {
        let start = Instant::now();
        let mut clock = IntegrationClock::default();
        assert_eq!(clock.tick(start, true), 0.0);
        assert_eq!(
            clock.tick(start + std::time::Duration::from_secs(1), true),
            30.0
        );
        assert_eq!(
            clock.tick(start + std::time::Duration::from_secs(8), false),
            30.0
        );
        assert_eq!(
            clock.tick(start + std::time::Duration::from_secs(20), true),
            30.0
        );
        assert_eq!(
            clock.tick(start + std::time::Duration::from_secs(21), true),
            60.0
        );
    }

    #[test]
    fn frame_context_freezes_static_and_maps_loop_and_clamp_explicitly() {
        let view = crate::animation::types::ViewFrame::default();
        let static_frame = frame_context(4, 120.0, false, true, 99.0, view);
        assert_eq!(static_frame.sequence, None);
        assert_eq!(static_frame.sequence_time, 0.0);
        assert_eq!(static_frame.global_time, 0.0);
        assert_eq!(static_frame.playback, PlaybackMode::Clamp);

        let looping = frame_context(4, 120.0, true, true, 99.0, view);
        assert_eq!(looping.sequence, Some(4));
        assert_eq!(looping.sequence_time, 120.0);
        assert_eq!(looping.global_time, 99.0);
        assert_eq!(looping.playback, PlaybackMode::Loop);
        assert_eq!(
            frame_context(4, 120.0, true, false, 99.0, view).playback,
            PlaybackMode::Clamp
        );
    }

    #[test]
    fn camera_basis_is_finite_orthonormal_and_right_handed_near_poles() {
        for pitch in [
            -std::f32::consts::FRAC_PI_2 + 0.0001,
            -0.3,
            0.3,
            std::f32::consts::FRAC_PI_2 - 0.0001,
        ] {
            let camera = CameraState::new(1.2, pitch, 500.0, [2.0, 3.0, 4.0]);
            let view = view_frame(&camera);
            for axis in [view.right, view.up, view.forward] {
                let length = axis.iter().map(|value| value * value).sum::<f32>();
                assert!((length - 1.0).abs() < 1.0e-4);
            }
            assert!(
                view.right
                    .into_iter()
                    .zip(view.up)
                    .map(|(a, b)| a * b)
                    .sum::<f32>()
                    .abs()
                    < 1.0e-4
            );
            let cross = cross3(view.right, view.up);
            assert!(
                cross
                    .into_iter()
                    .zip(view.forward)
                    .map(|(a, b)| (a - b).abs())
                    .sum::<f32>()
                    < 1.0e-4
            );
        }
    }

    #[test]
    fn model_texture_source_rejects_unsafe_paths() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let source = ModelTextureSource {
            root: root.to_path_buf(),
            fatal_error: None,
        };
        for path in ["../Cargo.toml", "/Cargo.toml", "C:/Cargo.toml", "a/./b"] {
            assert_eq!(
                source.find_case_insensitive(path),
                Err(TextureSourceError::Unsupported)
            );
        }
    }

    #[test]
    fn source_preflight_reports_case_collision_and_oversize_as_hard_errors() {
        let root = std::env::temp_dir().join(format!(
            "mdlvis-integrate-source-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let source = ModelTextureSource {
            root: root.canonicalize().unwrap(),
            fatal_error: None,
        };
        let collision =
            unique_case_match("ONE.blp", vec![root.join("One.blp"), root.join("one.blp")])
                .unwrap_err();
        assert_eq!(collision.key, "scene-texture-case-collision");

        let oversized = root.join("large.blp");
        let file = File::create(&oversized).unwrap();
        file.set_len(64 * 1024 * 1024 + 1).unwrap();
        let error = source
            .preflight(&[SceneTextureRequest {
                index: TextureIndex(0),
                filename: "large.blp".into(),
                replaceable_id: 0,
                wrap_u: false,
                wrap_v: false,
            }])
            .unwrap_err();
        assert_eq!(error.key, "scene-texture-input-too-large");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn viewer_source_prefers_model_local_then_archive() {
        use crate::texture::archive::{StormArchiveSource, War3ArchivePaths};
        use wow_mpq::{ArchiveBuilder, ListfileOption};

        let root = std::env::temp_dir().join(format!(
            "mdlvis-archive-source-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("local.blp"), b"from-model").unwrap();
        let mpq = root.join("base.mpq");
        ArchiveBuilder::new()
            .listfile_option(ListfileOption::None)
            .add_file_data(b"from-archive".to_vec(), "local.blp")
            .add_file_data(b"archive-only".to_vec(), "missing.blp")
            .build(&mpq)
            .unwrap();

        let mut source = ViewerTextureSource {
            local: ModelTextureSource {
                root: root.canonicalize().unwrap(),
                fatal_error: None,
            },
            archive: Some(
                StormArchiveSource::open(&War3ArchivePaths {
                    base: mpq,
                    expansion: None,
                    patch: None,
                })
                .unwrap(),
            ),
            fatal_error: None,
        };
        assert_eq!(
            TextureByteSource::read(&mut source, "local.blp").unwrap(),
            Some(b"from-model".to_vec())
        );
        assert_eq!(
            TextureByteSource::read(&mut source, "missing.blp").unwrap(),
            Some(b"archive-only".to_vec())
        );
        assert_eq!(TextureByteSource::read(&mut source, "absent.blp").unwrap(), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unconfigured_archive_keeps_existing_missing_fallback() {
        let mut source = ViewerTextureSource {
            local: ModelTextureSource {
                root: Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf(),
                fatal_error: None,
            },
            archive: None,
            fatal_error: None,
        };
        assert_eq!(TextureByteSource::read(&mut source, "absent.blp").unwrap(), None);
    }

    #[test]
    fn failed_generation_does_not_replace_active_generation() {
        let mut state = IntegrationState::default();
        let first = state.begin_load();
        state.publish_load(
            first,
            SceneTextureResolver::new(ViewerTextureSource {
                local: ModelTextureSource {
                    root: PathBuf::from("first"),
                    fatal_error: None,
                },
                archive: None,
                fatal_error: None,
            }),
        );
        let _failed = state.begin_load();
        assert_eq!(state.active_generation, first);
        state.clock.global_frame = 123.0;
        state.clock = IntegrationClock::default();
        assert_eq!(state.clock.global_frame, 0.0);
    }

    #[test]
    fn load_error_stays_visible_across_old_scene_success_until_new_load_succeeds() {
        let mut app = App::new();
        app.record_load_error(MdlError::new("load-b-failed"));
        for _ in 0..3 {
            app.scene_error = None;
            assert_eq!(app.sticky_load_error.as_ref().unwrap().key, "load-b-failed");
        }
        app.sticky_load_error = None;
        assert!(app.sticky_load_error.is_none());
    }
}

#[derive(Default)]
struct IntegrationClock {
    global_frame: f64,
    last_tick: Option<Instant>,
    was_playing: bool,
}

impl IntegrationClock {
    fn tick(&mut self, now: Instant, playing: bool) -> f64 {
        if playing
            && self.was_playing
            && let Some(previous) = self.last_tick
        {
            self.global_frame += now.duration_since(previous).as_secs_f64() * 30.0;
        }
        self.last_tick = Some(now);
        self.was_playing = playing;
        self.global_frame
    }
}

struct IntegrationState {
    load_generation: u64,
    active_generation: u64,
    clock: IntegrationClock,
    textures: Option<SceneTextureResolver<ViewerTextureSource>>,
    time_origin: Instant,
}

impl Default for IntegrationState {
    fn default() -> Self {
        Self {
            load_generation: 0,
            active_generation: 0,
            clock: IntegrationClock::default(),
            textures: None,
            time_origin: Instant::now(),
        }
    }
}

struct PreparedCpuScene {
    packet: ScenePacket,
    textures: Vec<ResolvedSceneTexture>,
}

impl IntegrationState {
    fn begin_load(&mut self) -> u64 {
        self.load_generation = self.load_generation.wrapping_add(1);
        self.load_generation
    }

    fn publish_load(
        &mut self,
        generation: u64,
        resolver: SceneTextureResolver<ViewerTextureSource>,
    ) {
        self.active_generation = generation;
        self.textures = Some(resolver);
    }
}

#[derive(Debug, Clone)]
struct ModelTextureSource {
    root: PathBuf,
    fatal_error: Option<MdlError>,
}

impl ModelTextureSource {
    fn new(model_path: &Path) -> Result<Self, MdlError> {
        let parent = model_path
            .parent()
            .ok_or_else(|| MdlError::new("scene-texture-missing-model-directory"))?;
        let root = parent.canonicalize().map_err(|error| {
            MdlError::new("scene-texture-model-directory")
                .with_arg("path", parent.display())
                .push_std(error)
        })?;
        Ok(Self {
            root,
            fatal_error: None,
        })
    }

    #[cfg(test)]
    fn find_case_insensitive(&self, logical: &str) -> Result<Option<PathBuf>, TextureSourceError> {
        self.locate(logical)
            .map_err(|_| TextureSourceError::Unsupported)
    }

    fn locate(&self, logical: &str) -> Result<Option<PathBuf>, MdlError> {
        let normalized = logical.replace('\\', "/");
        let path = Path::new(logical);
        if path.is_absolute()
            || normalized
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == "..")
            || path.components().any(|component| {
                !matches!(component, Component::Normal(_))
                    || component
                        .as_os_str()
                        .to_str()
                        .is_none_or(|segment| segment.contains('\0'))
            })
        {
            return Err(MdlError::new("scene-texture-unsafe-path").with_arg("path", logical));
        }

        let mut current = self.root.clone();
        for component in path.components() {
            let Component::Normal(expected) = component else {
                return Err(MdlError::new("scene-texture-unsafe-path").with_arg("path", logical));
            };
            let expected = expected.to_string_lossy();
            let entries = std::fs::read_dir(&current).map_err(|error| {
                MdlError::new("scene-texture-read-directory")
                    .with_arg("path", current.display())
                    .push_std(error)
            })?;
            let matches = entries
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&expected)
                })
                .map(|entry| entry.path())
                .collect::<Vec<_>>();
            let Some(found) = unique_case_match(logical, matches)? else {
                return Ok(None);
            };
            current = found;
        }

        let canonical = current
            .canonicalize()
            .map_err(|error| MdlError::new("scene-texture-canonicalize").push_std(error))?;
        if !canonical.starts_with(&self.root) {
            return Err(MdlError::new("scene-texture-root-escape").with_arg("path", logical));
        }
        Ok(Some(canonical))
    }

    fn preflight(&self, requests: &[crate::scene::SceneTextureRequest]) -> Result<(), MdlError> {
        const MAX_BYTES: u64 = 64 * 1024 * 1024;
        for request in requests {
            let Some(path) = logical_texture_path(request)? else {
                continue;
            };
            let Some(found) = self.locate(&path)? else {
                continue;
            };
            let metadata = found.metadata().map_err(|error| {
                MdlError::new("scene-texture-metadata")
                    .with_arg("path", found.display())
                    .push_std(error)
            })?;
            if !metadata.is_file() {
                return Err(MdlError::new("scene-texture-not-file").with_arg("path", path));
            }
            if metadata.len() > MAX_BYTES {
                return Err(MdlError::new("scene-texture-input-too-large")
                    .with_arg("path", path)
                    .with_arg("bytes", metadata.len()));
            }
        }
        Ok(())
    }
}

fn unique_case_match(logical: &str, matches: Vec<PathBuf>) -> Result<Option<PathBuf>, MdlError> {
    match matches.as_slice() {
        [] => Ok(None),
        [only] => Ok(Some(only.clone())),
        _ => Err(MdlError::new("scene-texture-case-collision").with_arg("path", logical)),
    }
}

fn logical_texture_path(
    request: &crate::scene::SceneTextureRequest,
) -> Result<Option<String>, MdlError> {
    let value = match request.replaceable_id {
        1 | 2 => return Ok(None),
        11 => "replaceabletextures/cliff/cliff0.blp".to_string(),
        31 => "replaceabletextures/lordaerontree/lordaeronsummertree.blp".to_string(),
        32 => "replaceabletextures/ashenvaletree/ashentree.blp".to_string(),
        33 => "replaceabletextures/barrenstree/barrenstree.blp".to_string(),
        34 => "replaceabletextures/northrendtree/northtree.blp".to_string(),
        35 => "replaceabletextures/mushroom/mushroomtree.blp".to_string(),
        value if value >= 3 => {
            "replaceabletextures/lordaerontree/lordaeronsummertree.blp".to_string()
        }
        _ => request.filename.replace('\\', "/"),
    };
    if value.is_empty() || value.contains('\0') || value.starts_with('/') || value.contains(':') {
        return Err(MdlError::new("scene-texture-unsafe-path").with_arg("path", value));
    }
    let mut parts = Vec::new();
    for part in value.split('/') {
        if part == ".." {
            return Err(MdlError::new("scene-texture-unsafe-path").with_arg("path", value));
        }
        if !part.is_empty() && part != "." {
            parts.push(part);
        }
    }
    if parts.is_empty() {
        return Err(MdlError::new("scene-texture-unsafe-path").with_arg("path", value));
    }
    let mut path = parts.join("/").to_ascii_lowercase();
    if !path.ends_with(".blp") {
        path.push_str(".blp");
    }
    Ok(Some(path))
}

impl TextureByteSource for ModelTextureSource {
    fn read(&mut self, canonical_path: &str) -> Result<Option<Vec<u8>>, TextureSourceError> {
        const MAX_BYTES: u64 = 64 * 1024 * 1024;
        let Some(path) = (match self.locate(canonical_path) {
            Ok(value) => value,
            Err(error) => {
                self.fatal_error = Some(error);
                return Err(TextureSourceError::Unsupported);
            }
        }) else {
            return Ok(None);
        };
        let metadata = path.metadata().map_err(|_| TextureSourceError::Read)?;
        if !metadata.is_file() || metadata.len() > MAX_BYTES {
            if metadata.len() > MAX_BYTES {
                self.fatal_error = Some(
                    MdlError::new("scene-texture-input-too-large")
                        .with_arg("path", canonical_path)
                        .with_arg("bytes", metadata.len()),
                );
            }
            return Err(TextureSourceError::Unsupported);
        }
        let bytes = std::fs::read(path).map_err(|_| TextureSourceError::Read)?;
        if bytes.len() as u64 > MAX_BYTES {
            self.fatal_error = Some(
                MdlError::new("scene-texture-input-too-large")
                    .with_arg("path", canonical_path)
                    .with_arg("bytes", bytes.len()),
            );
            return Err(TextureSourceError::Unsupported);
        }
        Ok(Some(bytes))
    }
}

#[derive(Clone)]
struct ViewerTextureSource {
    local: ModelTextureSource,
    archive: Option<StormArchiveSource>,
    fatal_error: Option<MdlError>,
}

impl ViewerTextureSource {
    fn new(model_path: &Path) -> Result<Self, MdlError> {
        Ok(Self {
            local: ModelTextureSource::new(model_path)?,
            archive: configured_archive_source()?,
            fatal_error: None,
        })
    }

    fn preflight(&self, requests: &[crate::scene::SceneTextureRequest]) -> Result<(), MdlError> {
        self.local.preflight(requests)
    }
}

impl TextureByteSource for ViewerTextureSource {
    fn read(&mut self, canonical_path: &str) -> Result<Option<Vec<u8>>, TextureSourceError> {
        match self.local.read(canonical_path) {
            Ok(Some(bytes)) => return Ok(Some(bytes)),
            Ok(None) => {}
            Err(error) => {
                if let Some(fatal) = self.local.fatal_error.take() {
                    self.fatal_error = Some(fatal);
                }
                return Err(error);
            }
        }

        let Some(archive) = self.archive.as_mut() else {
            return Ok(None);
        };
        match archive.read(canonical_path) {
            Ok(value) => Ok(value),
            Err(error) => {
                if let Some(fatal) = archive.take_fatal_error() {
                    self.fatal_error = Some(fatal);
                }
                Err(error)
            }
        }
    }
}

fn configured_archive_source() -> Result<Option<StormArchiveSource>, MdlError> {
    let Some(base) = configured_absolute_path("MDLVIS_WAR3_MPQ")? else {
        if configured_absolute_path("MDLVIS_WAR3X_MPQ")?.is_some()
            || configured_absolute_path("MDLVIS_WAR3PATCH_MPQ")?.is_some()
        {
            return Err(MdlError::new("archive-base-required")
                .with_arg("hint", "set MDLVIS_WAR3_MPQ to an absolute war3.mpq path"));
        }
        return Ok(None);
    };
    StormArchiveSource::open(&War3ArchivePaths {
        base,
        expansion: configured_absolute_path("MDLVIS_WAR3X_MPQ")?,
        patch: configured_absolute_path("MDLVIS_WAR3PATCH_MPQ")?,
    })
    .map(Some)
}

fn configured_absolute_path(name: &str) -> Result<Option<PathBuf>, MdlError> {
    let Some(value) = std::env::var_os(name) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(MdlError::new("archive-path-not-absolute")
            .with_arg("kind", name)
            .with_arg("path", path.display()));
    }
    Ok(Some(path))
}

fn prepare_model_cpu_scene(
    model: &crate::model::model::Model,
    frame: crate::animation::types::FrameContext,
    resolver: &mut SceneTextureResolver<ViewerTextureSource>,
    team_color: [f32; 3],
) -> Result<PreparedCpuScene, MdlError> {
    let pose = crate::animation::evaluate_pose(model, frame)?;
    let packet = build_scene_packet(model, &pose)?;
    resolver.source_mut().preflight(&packet.textures)?;
    let prepared = match resolver
        .resolve_all_canonical(&packet.textures, team_color)
        .map(|textures| PreparedCpuScene { packet, textures })
        .map_err(texture_error)
    {
        Ok(prepared) => prepared,
        Err(error) => {
            return Err(resolver.source_mut().fatal_error.take().unwrap_or(error));
        }
    };
    if let Some(error) = resolver.source_mut().fatal_error.take() {
        return Err(error);
    }
    Ok(prepared)
}

fn texture_error(error: SceneTextureError) -> MdlError {
    MdlError::new("scene-texture-resolution").with_arg("reason", format!("{error:?}"))
}

fn normalize3(value: [f32; 3]) -> [f32; 3] {
    let length = value
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    value.map(|component| component / length)
}

fn cross3(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn view_frame(camera: &CameraState) -> crate::animation::types::ViewFrame {
    let position = [
        camera.target[0] + camera.distance * camera.yaw.cos() * camera.pitch.cos(),
        camera.target[1] + camera.distance * camera.yaw.sin() * camera.pitch.cos(),
        camera.target[2] + camera.distance * camera.pitch.sin(),
    ];
    let forward = normalize3(std::array::from_fn(|axis| {
        camera.target[axis] - position[axis]
    }));
    let horizontal_right = [camera.yaw.sin(), -camera.yaw.cos(), 0.0];
    let right_candidate = cross3([0.0, 0.0, 1.0], forward);
    let right = if right_candidate
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        > 1.0e-8
    {
        normalize3(right_candidate)
    } else {
        normalize3(horizontal_right)
    };
    let up = normalize3(cross3(forward, right));
    crate::animation::types::ViewFrame {
        position,
        right,
        up,
        forward,
    }
}

fn frame_context(
    selected_sequence: usize,
    sequence_time: f32,
    use_animation: bool,
    is_looping: bool,
    global_time: f64,
    view: crate::animation::types::ViewFrame,
) -> crate::animation::types::FrameContext {
    crate::animation::types::FrameContext {
        sequence: use_animation.then_some(selected_sequence),
        sequence_time: if use_animation {
            f64::from(sequence_time)
        } else {
            0.0
        },
        global_time: if use_animation { global_time } else { 0.0 },
        playback: if use_animation && is_looping {
            crate::animation::types::PlaybackMode::Loop
        } else {
            crate::animation::types::PlaybackMode::Clamp
        },
        view: Some(view),
    }
}

#[cfg(test)]
fn prepare_cpu_scene<S: TextureByteSource>(
    model: &crate::model::model::Model,
    frame: crate::animation::types::FrameContext,
    resolver: &mut SceneTextureResolver<S>,
    team_color: [f32; 3],
) -> Result<PreparedCpuScene, MdlError> {
    let pose = crate::animation::evaluate_pose(model, frame)?;
    let packet = build_scene_packet(model, &pose)?;
    let textures = resolver
        .resolve_all_canonical(&packet.textures, team_color)
        .map_err(texture_error)?;
    Ok(PreparedCpuScene { packet, textures })
}

impl App {
    pub fn handle_event(&mut self, event: &winit::event::WindowEvent) -> EventResponse {
        let handler = get_global_handler_mut().unwrap();
        let window = handler.window.as_ref().unwrap();

        let egui_state = &mut handler.egui_state.as_mut().unwrap();

        let egui_response = egui_state.on_window_event(&window, event);

        // For keyboard and some events, if egui consumed it, don't process further
        let egui_wants_input = egui_response.consumed;

        // Handle window events
        match event {
            winit::event::WindowEvent::CloseRequested => {
                return EventResponse {
                    repaint: false,
                    exit: true,
                };
            }
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                if egui_wants_input {
                    return EventResponse {
                        repaint: egui_response.repaint,
                        exit: false,
                    };
                }
                if event.logical_key
                    == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
                {
                    return EventResponse {
                        repaint: false,
                        exit: true,
                    };
                }
            }
            winit::event::WindowEvent::Resized(size) => {
                handler.renderer.as_mut().unwrap().resize(*size);
            }
            winit::event::WindowEvent::MouseInput { state, button, .. } => {
                // Don't handle mouse input if egui wants the pointer
                if handler.egui_wants_pointer {
                    return EventResponse {
                        repaint: egui_response.repaint,
                        exit: false,
                    };
                }
                let is_pressed = *state == winit::event::ElementState::Pressed;
                handler
                    .camera_controller
                    .on_mouse_button(*button, is_pressed);
            }
            winit::event::WindowEvent::ModifiersChanged(modifiers) => {
                let shift = modifiers.state().shift_key();
                let alt = modifiers.state().alt_key();
                let control = modifiers.state().control_key();
                handler.camera_controller.on_modifiers(shift, alt, control);
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                // Don't handle mouse movement if egui wants the pointer
                if handler.egui_wants_pointer {
                    return EventResponse {
                        repaint: egui_response.repaint,
                        exit: false,
                    };
                }
                handler.current_cursor_pos = Some((position.x, position.y));
                handler
                    .camera_controller
                    .on_mouse_move((position.x, position.y));
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => {
                // Don't handle mouse wheel if egui wants the pointer
                if handler.egui_wants_pointer {
                    return EventResponse {
                        repaint: egui_response.repaint,
                        exit: false,
                    };
                }
                match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => {
                        // Real mouse wheel - simple zoom
                        handler.camera_controller.simple_zoom(*y);
                    }
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        // Trackpad scroll (two fingers) - handle like PanGesture
                        let control = handler.camera_controller.is_control_pressed();
                        let shift = handler.camera_controller.is_shift_pressed();
                        handler.camera_controller.on_pan_gesture(
                            pos.x as f32 * 0.05,
                            -pos.y as f32 * 0.05,
                            control,
                            shift,
                        );
                    }
                }
            }
            winit::event::WindowEvent::PanGesture { delta, phase, .. } => {
                // Don't handle pan gesture if egui wants the pointer
                if handler.egui_wants_pointer {
                    return EventResponse {
                        repaint: egui_response.repaint,
                        exit: false,
                    };
                }
                // Two-finger swipe gesture - ONLY WAY to control camera with trackpad:
                // - No modifiers: rotate around grid center (0,0,0)
                // - Shift: pan (move target)
                // - Control: zoom (change distance)
                use winit::event::TouchPhase;
                if matches!(phase, TouchPhase::Moved) {
                    let control = handler.camera_controller.is_control_pressed();
                    let shift = handler.camera_controller.is_shift_pressed();
                    handler
                        .camera_controller
                        .on_pan_gesture(delta.x, -delta.y, control, shift);
                }
            }
            _ => {}
        }

        EventResponse {
            repaint: false,
            exit: false,
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let handler = get_global_handler_mut().unwrap();

        while let Ok(result) = handler.texture_receiver.try_recv() {
            if self.integration.active_generation != 0 {
                continue;
            }
            match result {
                TextureLoadResult::Success {
                    texture_id,
                    rgba_data,
                    width,
                    height,
                } => {
                    handler
                        .renderer
                        .as_mut()
                        .unwrap()
                        .load_texture_from_rgba(&rgba_data, width, height, texture_id);

                    // Update texture manager status
                    if let Some(texture_info) = handler.texture_manager.get_texture_mut(texture_id)
                    {
                        texture_info.status = TextureStatus::Loaded;
                        texture_info.width = width;
                        texture_info.height = height;
                        texture_info.progress = 1.0;
                    }
                }
                TextureLoadResult::Error { texture_id, error } => {
                    // Update texture manager status to error ONLY if not already loaded
                    if let Some(texture_info) = handler.texture_manager.get_texture_mut(texture_id)
                    {
                        // Don't overwrite successful load with error from background task
                        if !texture_info.is_loaded() {
                            texture_info.status = TextureStatus::Error(error);
                            texture_info.progress = 0.0;
                        } else {
                            println!(
                                "Ignoring error for texture {} - already loaded successfully",
                                texture_id
                            );
                        }
                    }
                }
            }
        }

        let window = handler.window.as_ref().unwrap();

        let egui_state = handler.egui_state.as_mut().unwrap();
        let raw_input = egui_state.take_egui_input(&window);
        let egui_ctx = egui_state.egui_ctx().clone();

        // Get camera orientation for axis gizmo
        let (camera_yaw, camera_pitch) =
            handler.renderer.as_mut().unwrap().camera.get_orientation();

        // Update animation playback BEFORE UI (so current_frame is up-to-date)
        let (_, _, was_playing, _, was_using_animation) = handler.ui.scene_playback();
        let global_time = self
            .integration
            .clock
            .tick(Instant::now(), was_playing && was_using_animation);
        let current_time = self.integration.time_origin.elapsed().as_secs_f64();
        handler.ui.animate(&handler.model, current_time);

        let mut reset_camera = false;
        let mut current_frame = 0.0;
        let mut show_geosets = Vec::new();
        let mut colors_changed = false;
        let mut open_model = false;
        let mut texture_load_requests: Vec<usize> = Vec::new();
        let mut use_animation = false;
        let mut language_changed = false;

        let full_output = egui_ctx.run(raw_input, |ctx| {
            let (
                reset_camera_ui,
                current_frame_ui,
                show_geosets_ui,
                colors_changed_ui,
                open_model_ui,
                use_animation_ui,
                language_changed_ui,
            ) = handler.ui.show(
                ctx,
                &mut handler.model,
                camera_yaw,
                camera_pitch,
                &mut handler.settings,
                &mut handler.renderer.as_mut().unwrap(),
            );

            reset_camera = reset_camera_ui;
            current_frame = current_frame_ui;
            show_geosets = show_geosets_ui;
            colors_changed = colors_changed_ui;
            open_model = open_model_ui;
            use_animation = use_animation_ui;
            language_changed = language_changed_ui;

            // Show texture panel
            if let Some(requests) = handler.texture_panel.show(
                ctx,
                &handler.texture_manager,
                &mut handler.renderer.as_mut().unwrap(),
                &mut handler.settings.ui.show_texture_panel,
            ) {
                texture_load_requests = requests;
            }

            if let Some(error) = self.sticky_load_error.as_ref().or(self.scene_error.as_ref()) {
                egui::Window::new("Scene error")
                    .collapsible(false)
                    .resizable(true)
                    .show(ctx, |ui| {
                        ui.label(error.to_string());
                    });
            }
            if self.integration.active_generation != 0
                && handler.settings.ui.show_texture_panel
            {
                egui::Window::new("Scene textures")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.label("Scene textures are resolved from the model directory and are read-only here.");
                    });
            }
        });

        // Update egui pointer state for next frame
        handler.egui_wants_pointer = egui_ctx.wants_pointer_input();

        // Process texture load requests
        // Scene textures are resolved synchronously from the model directory. Legacy panel
        // requests must not start remote/background tasks that could overwrite scene resources.
        let _ = texture_load_requests;

        // Handle Open Model button
        if open_model {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(&crate::i18n::t("dialog.open-filter"), &["mdx"])
                .pick_file()
            {
                if let Some(path_str) = path.to_str() {
                    handler.pending_model_path = Some(path_str.to_string());
                }
            }
        }

        // Handle reset camera button
        if language_changed {
            if let Some(window) = handler.window.as_ref() {
                window.set_title(&crate::i18n::t("app.window-title"));
            }
        }

        if reset_camera {
            handler.camera_controller.reset();
        }

        // Update renderer colors if they changed
        if colors_changed {
            handler
                .renderer
                .as_mut()
                .unwrap()
                .update_colors(&handler.settings, None);
        }

        egui_state.handle_platform_output(&window, full_output.platform_output);

        let paint_jobs = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [window.inner_size().width, window.inner_size().height],
            pixels_per_point: window.scale_factor() as f32,
        };

        let show_skeleton = handler.settings.display.show_skeleton;
        let show_grid = handler.settings.display.show_grid;
        let show_bounding_box = handler.settings.display.show_bounding_box;
        let wireframe_mode = handler.settings.display.wireframe_mode;
        let far_plane = handler.settings.display.far_plane;

        // Sync camera state to renderer
        handler.renderer.as_mut().unwrap().camera = handler.camera_controller.state().clone();

        let (selected_sequence, ui_frame, _is_playing, is_looping, ui_uses_animation) =
            handler.ui.scene_playback();
        debug_assert_eq!(current_frame, ui_frame);
        debug_assert_eq!(use_animation, ui_uses_animation);
        let frame = frame_context(
            selected_sequence,
            ui_frame,
            ui_uses_animation
                && handler
                    .model
                    .as_ref()
                    .is_some_and(|m| !m.sequences.is_empty()),
            is_looping,
            global_time,
            view_frame(handler.camera_controller.state()),
        );

        let scene_update = match (&handler.model, &self.integration.textures) {
            (Some(model), Some(active_resolver)) => {
                let mut candidate = active_resolver.clone();
                prepare_model_cpu_scene(
                    model,
                    frame,
                    &mut candidate,
                    handler.settings.colors.team_color,
                )
                .and_then(|prepared| {
                    handler
                        .renderer
                        .as_mut()
                        .expect("renderer initialized")
                        .update_scene_with_textures(&prepared.packet, &prepared.textures)?;
                    Ok(Some(candidate))
                })
            }
            _ => Ok(None),
        };
        let scene_update_ok = scene_update.is_ok();
        match scene_update {
            Ok(candidate) => {
                if let Some(candidate) = candidate {
                    self.integration.textures = Some(candidate);
                }
            }
            Err(error) => self.record_scene_error(error),
        }

        let render_result = handler.renderer.as_mut().unwrap().render(
            None,
            show_skeleton,
            show_grid,
            show_bounding_box,
            wireframe_mode,
            far_plane,
            &show_geosets,
            paint_jobs,
            full_output.textures_delta,
            screen_descriptor,
        );
        if let Some(error) = handler
            .renderer
            .as_ref()
            .and_then(|renderer| renderer.scene_error.clone())
        {
            self.record_scene_error(error);
        } else if render_result.is_ok() && scene_update_ok {
            self.scene_error = None;
            self.last_reported_scene_error = None;
        }
        render_result
    }

    pub(crate) fn record_scene_error(&mut self, error: MdlError) {
        let text = error.to_string();
        if self.last_reported_scene_error.as_deref() != Some(&text) {
            eprintln!("Scene integration error: {text}");
            self.last_reported_scene_error = Some(text);
        }
        self.scene_error = Some(error);
    }

    pub(crate) fn record_load_error(&mut self, error: MdlError) {
        self.record_scene_error(error.clone());
        self.sticky_load_error = Some(error);
    }

    pub async fn load_model(&mut self, path: &str) -> Result<(), MdlError> {
        println!("Loading model: {}", path);

        let handler = get_global_handler_mut().unwrap();
        let generation = self.integration.begin_load();
        let mut file = File::open(path)?;
        let model = load(&mut file)?;
        let model_path = Path::new(path);
        let mut resolver = SceneTextureResolver::new(ViewerTextureSource::new(model_path)?);
        let initial_frame = crate::animation::types::FrameContext {
            sequence: None,
            sequence_time: 0.0,
            global_time: 0.0,
            playback: crate::animation::types::PlaybackMode::Clamp,
            view: Some(view_frame(handler.camera_controller.state())),
        };
        let prepared = prepare_model_cpu_scene(
            &model,
            initial_frame,
            &mut resolver,
            handler.settings.colors.team_color,
        )?;
        handler
            .renderer
            .as_mut()
            .expect("renderer initialized")
            .update_scene_with_textures(&prepared.packet, &prepared.textures)?;

        handler.model_path = Some(path.to_string());
        handler.texture_manager.set_model_path(model_path);
        handler.texture_manager.init_from_model(&model);
        handler.model = Some(model);
        handler.ui.reset_animation(&handler.model);
        self.integration.publish_load(generation, resolver);
        self.integration.clock = IntegrationClock::default();
        self.scene_error = None;
        self.sticky_load_error = None;
        self.last_reported_scene_error = None;

        println!("Model loaded successfully");

        Ok(())
    }
}
