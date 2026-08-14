#![allow(dead_code)]

use crate::error::MdlError;
use crate::texture::scene::{TextureByteSource, TextureSourceError};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use wow_mpq::{Archive, Error as MpqError};

const MAX_MEMBER_BYTES: u64 = 64 * 1024 * 1024;
const MAX_MEMBER_PATH_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct War3ArchivePaths {
    pub base: PathBuf,
    pub expansion: Option<PathBuf>,
    pub patch: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveLayer {
    Patch,
    Expansion,
    Base,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveRead {
    pub bytes: Vec<u8>,
    pub layer: ArchiveLayer,
}

#[derive(Debug)]
pub struct StormArchiveSource {
    inner: Arc<Mutex<ArchiveSession>>,
    fatal_error: Option<MdlError>,
}

impl Clone for StormArchiveSource {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            fatal_error: None,
        }
    }
}

impl StormArchiveSource {
    pub fn open(paths: &War3ArchivePaths) -> Result<Self, MdlError> {
        let base = canonical_regular_file(&paths.base, "base")?;
        let expansion = paths
            .expansion
            .as_ref()
            .map(|path| canonical_regular_file(path, "expansion"))
            .transpose()?;
        let patch = paths
            .patch
            .as_ref()
            .map(|path| canonical_regular_file(path, "patch"))
            .transpose()?;

        let mut archives = Vec::new();
        open_layer(&mut archives, ArchiveLayer::Base, &base)?;
        if let Some(path) = expansion.as_deref() {
            open_layer(&mut archives, ArchiveLayer::Expansion, path)?;
        }
        if let Some(path) = patch.as_deref() {
            open_layer(&mut archives, ArchiveLayer::Patch, path)?;
        }

        Ok(Self {
            inner: Arc::new(Mutex::new(ArchiveSession { archives })),
            fatal_error: None,
        })
    }

    pub fn read_member(&self, canonical_path: &str) -> Result<Option<ArchiveRead>, MdlError> {
        let member_name = canonical_member_name(canonical_path)?;
        self.inner
            .lock()
            .map_err(|_| MdlError::new("archive-session-poisoned"))?
            .read_member(&member_name)
    }

    pub fn take_fatal_error(&mut self) -> Option<MdlError> {
        self.fatal_error.take()
    }
}

impl TextureByteSource for StormArchiveSource {
    fn read(&mut self, canonical_path: &str) -> Result<Option<Vec<u8>>, TextureSourceError> {
        match self.read_member(canonical_path) {
            Ok(value) => Ok(value.map(|read| read.bytes)),
            Err(error) => {
                self.fatal_error = Some(error);
                Err(TextureSourceError::Unsupported)
            }
        }
    }
}

#[derive(Debug)]
struct OpenedArchive {
    layer: ArchiveLayer,
    archive: Archive,
}

#[derive(Debug)]
struct ArchiveSession {
    // Opened base -> expansion -> patch. Reverse iteration is the frozen priority.
    archives: Vec<OpenedArchive>,
}

impl ArchiveSession {
    fn read_member(&mut self, member_name: &str) -> Result<Option<ArchiveRead>, MdlError> {
        for archive in self.archives.iter_mut().rev() {
            match archive.archive.find_file(member_name) {
                Ok(None) => continue,
                Ok(Some(info)) => {
                    check_member_size(info.file_size, member_name)?;
                    let bytes = match archive.archive.read_file(member_name) {
                        Ok(bytes) => bytes,
                        Err(MpqError::FileNotFound(_)) => continue,
                        Err(error) => {
                            return Err(map_mpq_error(error, "archive-member-read")
                                .with_arg("path", member_name)
                                .with_arg("layer", format!("{:?}", archive.layer)));
                        }
                    };
                    if bytes.len() as u64 != info.file_size {
                        return Err(MdlError::new("archive-member-short-read")
                            .with_arg("path", member_name)
                            .with_arg("expected", info.file_size)
                            .with_arg("actual", bytes.len()));
                    }
                    return Ok(Some(ArchiveRead {
                        bytes,
                        layer: archive.layer,
                    }));
                }
                Err(error) => {
                    return Err(map_mpq_error(error, "archive-member-lookup")
                        .with_arg("path", member_name)
                        .with_arg("layer", format!("{:?}", archive.layer)));
                }
            }
        }
        Ok(None)
    }
}

fn open_layer(
    archives: &mut Vec<OpenedArchive>,
    layer: ArchiveLayer,
    path: &Path,
) -> Result<(), MdlError> {
    let archive = Archive::open(path).map_err(|error| {
        map_mpq_error(error, "archive-open-failed")
            .with_arg("layer", format!("{layer:?}"))
            .with_arg("path", path.display())
    })?;
    archives.push(OpenedArchive { layer, archive });
    Ok(())
}

fn check_member_size(size: u64, member_name: &str) -> Result<(), MdlError> {
    if size > MAX_MEMBER_BYTES {
        return Err(MdlError::new("archive-member-too-large")
            .with_arg("path", member_name)
            .with_arg("bytes", size));
    }
    Ok(())
}

fn map_mpq_error(error: MpqError, key: &'static str) -> MdlError {
    MdlError::new(key)
        .with_arg("reason", error.to_string())
        .push_std(error)
}

fn canonical_member_name(canonical_path: &str) -> Result<String, MdlError> {
    if canonical_path.is_empty()
        || canonical_path.len() > MAX_MEMBER_PATH_BYTES
        || !canonical_path.is_ascii()
        || canonical_path.contains('\0')
        || canonical_path.contains('\\')
        || canonical_path.contains(':')
        || canonical_path.starts_with('/')
        || canonical_path.bytes().any(|byte| byte.is_ascii_uppercase())
        || canonical_path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(MdlError::new("archive-unsafe-member-path").with_arg("path", canonical_path));
    }
    Ok(canonical_path.replace('/', "\\"))
}

fn canonical_regular_file(path: &Path, label: &'static str) -> Result<PathBuf, MdlError> {
    if !path.is_absolute() {
        return Err(MdlError::new("archive-path-not-absolute")
            .with_arg("kind", label)
            .with_arg("path", path.display()));
    }
    let canonical = path.canonicalize().map_err(|error| {
        MdlError::new("archive-path-canonicalize")
            .with_arg("kind", label)
            .with_arg("path", path.display())
            .push_std(error)
    })?;
    let metadata = canonical.metadata().map_err(|error| {
        MdlError::new("archive-path-metadata")
            .with_arg("kind", label)
            .with_arg("path", canonical.display())
            .push_std(error)
    })?;
    if !metadata.is_file() {
        return Err(MdlError::new("archive-path-not-file")
            .with_arg("kind", label)
            .with_arg("path", canonical.display()));
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use wow_mpq::{ArchiveBuilder, ListfileOption};

    fn write_archive(dir: &Path, name: &str, members: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join(name);
        let mut builder = ArchiveBuilder::new().listfile_option(ListfileOption::None);
        for (member, bytes) in members {
            builder = builder.add_file_data(bytes.to_vec(), member);
        }
        builder.build(&path).expect("write test mpq");
        path
    }

    fn temp_dir() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "mdlvis-archive-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn patch_then_expansion_then_base_is_the_exact_lookup_order() {
        let dir = temp_dir();
        let base = write_archive(
            &dir,
            "base.mpq",
            &[
                ("textures\\shared.blp", b"base"),
                ("textures\\only-base.blp", b"base-only"),
            ],
        );
        let expansion = write_archive(
            &dir,
            "expansion.mpq",
            &[
                ("textures\\shared.blp", b"expansion"),
                ("textures\\only-expansion.blp", b"expansion-only"),
            ],
        );
        let patch = write_archive(&dir, "patch.mpq", &[("textures\\shared.blp", b"patch")]);
        let source = StormArchiveSource::open(&War3ArchivePaths {
            base,
            expansion: Some(expansion),
            patch: Some(patch),
        })
        .unwrap();

        let shared = source.read_member("textures/shared.blp").unwrap().unwrap();
        assert_eq!(shared.bytes, b"patch");
        assert_eq!(shared.layer, ArchiveLayer::Patch);

        let expansion_only = source
            .read_member("textures/only-expansion.blp")
            .unwrap()
            .unwrap();
        assert_eq!(expansion_only.bytes, b"expansion-only");
        assert_eq!(expansion_only.layer, ArchiveLayer::Expansion);

        let base_only = source
            .read_member("textures/only-base.blp")
            .unwrap()
            .unwrap();
        assert_eq!(base_only.bytes, b"base-only");
        assert_eq!(base_only.layer, ArchiveLayer::Base);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_members_query_all_layers_and_return_none() {
        let dir = temp_dir();
        let source = StormArchiveSource::open(&War3ArchivePaths {
            base: write_archive(&dir, "base.mpq", &[("textures\\present.blp", b"ok")]),
            expansion: Some(write_archive(
                &dir,
                "expansion.mpq",
                &[("textures\\other.blp", b"ok")],
            )),
            patch: Some(write_archive(
                &dir,
                "patch.mpq",
                &[("textures\\third.blp", b"ok")],
            )),
        })
        .unwrap();
        assert_eq!(source.read_member("textures/missing.blp").unwrap(), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn oversize_members_are_stable_errors() {
        let error = check_member_size(MAX_MEMBER_BYTES + 1, "textures\\huge.blp").unwrap_err();
        assert_eq!(error.key, "archive-member-too-large");
        assert!(check_member_size(MAX_MEMBER_BYTES, "textures\\ok.blp").is_ok());
    }

    #[test]
    fn clone_shares_one_session_and_last_drop_keeps_reads_working_until_then() {
        let dir = temp_dir();
        let source = StormArchiveSource::open(&War3ArchivePaths {
            base: write_archive(&dir, "base.mpq", &[("textures\\shared.blp", b"base")]),
            expansion: None,
            patch: None,
        })
        .unwrap();
        let clone = source.clone();
        drop(source);
        let read = clone.read_member("textures/shared.blp").unwrap().unwrap();
        assert_eq!(read.bytes, b"base");
        drop(clone);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unsafe_member_paths_are_stable_errors_without_opening_archives() {
        for path in [
            "",
            "Textures/file.blp",
            "/absolute.blp",
            "c:/drive.blp",
            "a/../b.blp",
            "a/./b.blp",
            "a//b.blp",
            "a\\b.blp",
            "a\0b.blp",
            "中文.blp",
        ] {
            let result = catch_unwind(AssertUnwindSafe(|| canonical_member_name(path)));
            let error = result.expect("unsafe paths must not panic").unwrap_err();
            assert_eq!(error.key, "archive-unsafe-member-path");
        }
    }

    #[test]
    fn relative_and_missing_archive_paths_are_hard_errors() {
        let relative = StormArchiveSource::open(&War3ArchivePaths {
            base: PathBuf::from("war3.mpq"),
            expansion: None,
            patch: None,
        })
        .unwrap_err();
        assert_eq!(relative.key, "archive-path-not-absolute");

        let missing = StormArchiveSource::open(&War3ArchivePaths {
            base: std::env::temp_dir().join("mdlvis-missing-archive.mpq"),
            expansion: None,
            patch: None,
        })
        .unwrap_err();
        assert_eq!(missing.key, "archive-path-canonicalize");
    }

    #[test]
    fn texture_source_maps_hard_errors_and_preserves_them_for_transaction_boundary() {
        let dir = temp_dir();
        let mut source = StormArchiveSource::open(&War3ArchivePaths {
            base: write_archive(&dir, "base.mpq", &[("textures\\shared.blp", b"ok")]),
            expansion: None,
            patch: None,
        })
        .unwrap();
        assert_eq!(
            TextureByteSource::read(&mut source, "Textures/Shared.blp"),
            Err(TextureSourceError::Unsupported)
        );
        assert_eq!(
            source.take_fatal_error().expect("fatal error retained").key,
            "archive-unsafe-member-path"
        );
        assert!(source.take_fatal_error().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[ignore = "requires explicitly configured local Warcraft III MPQs"]
    fn configured_real_archives_read_tracked_builtin_textures() {
        let Some(base) = configured_path("MDLVIS_TEST_WAR3_MPQ") else {
            return;
        };
        let source = StormArchiveSource::open(&War3ArchivePaths {
            base,
            expansion: configured_path("MDLVIS_TEST_WAR3X_MPQ"),
            patch: configured_path("MDLVIS_TEST_WAR3PATCH_MPQ"),
        })
        .expect("configured MPQs open");

        for canonical_path in ["textures/dust3.blp", "textures/lavalump2.blp"] {
            let read = source
                .read_member(canonical_path)
                .unwrap_or_else(|error| panic!("{canonical_path}: {error}"))
                .unwrap_or_else(|| panic!("{canonical_path} missing from configured MPQs"));
            assert!(read.bytes.starts_with(b"BLP1") || read.bytes.starts_with(b"BLP2"));
            eprintln!(
                "archive evidence: {canonical_path} layer={:?} bytes={} fnv1a64={}",
                read.layer,
                read.bytes.len(),
                fnv1a64(&read.bytes)
            );
        }
    }

    fn configured_path(name: &str) -> Option<PathBuf> {
        std::env::var_os(name).map(PathBuf::from)
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x1000_0000_01b3)
        })
    }
}
