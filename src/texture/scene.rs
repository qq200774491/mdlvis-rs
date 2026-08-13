#![allow(dead_code)] // Staged CPU API; the renderer integration task will consume it.

use crate::model::ids::TextureIndex;
use crate::scene::SceneTextureRequest;
use std::collections::BTreeMap;

const FALLBACK_SIZE: u32 = 8;
const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;
const TEAM_GLOW_HEX: &str = concat!(
    "01010101010101010000000000000000010101010101010100000000000000010101010101010101000000000000000000000000000000000000000000000001",
    "01010101010101010000000000000000000000000000000001000000000000010101010101010101020202020303030303030302020201010101000000000001",
    "0101010101010101010202030405060606060504030202010202010000000000010101010101010101010304060709090a090807050302010302020100000000",
    "0101010101010101030406080a0d0e0f11100f0c0a0706050403020100000000010101010101010107080a0d10121416181715120f0c0a090403020100000000",
    "000001010001030405090f14191e23262624221f1a140d09090602010001010000000101000103050a0f151c23292f3233312e29241c140f0a07030100010100",
    "00000101010204070f141c262f373e4345433e382f251c150c0904010101010000000101010306091017212d39444e5357534d453a2d21190f0b060201010100",
    "000001010104080b131b273543515c6367635b514435271e120d070301010101000001000105090d18202e3d4d5c69707470685d4e3d2d231410090401010101",
    "0000000002050b0e1b243243546471787c7870645442312717110a04010101010000000002060b0f1c2433445566737b7f7a72665643322818120b0401010101",
    "0101010102050b0f19243343526170797b766e625342312716110a04020100000101010102050a0e1822303f4d5a6871746f675c4e3d2e241410090401010000",
    "010101010104090c161e2b3844505c6368635c524536271e120e080301010000010101010103070a1219232f3a454e5458544e453a2d2119100c060301010000",
    "00010101010205080d121b252f38404446433e372f251a140c090502010100000001010100010406090d131b242b303334322e29231c140e0a07030101010000",
    "000101010001030407090d13191e21222422201d19140e09080502010101000000010101000002040607090d121517171b191715130f09060604020000010000",
    "0101010101010101040506080a0c0d0e100f0e0c0a080706010101010101010101010101010101010203040607080a0a0a090807050403020101010101010101",
    "01010101010101010101020203040505050404030201010001010101010101010101010101010101000000010101020203030202020201010101010101010101",
    "01010101010101010101010101010100010101010102020201010101010101010101010101010101010101010101010100000000000000000101010101010101",
    "01010101010101010101010101010101000000000000000001010101010101010101010101010101000000000000000101000000000000000101010101010101",
);
const _: () = assert!(TEAM_GLOW_HEX.len() == 32 * 32 * 2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuColorSpace {
    Srgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuAlphaEncoding {
    Straight,
    Premultiplied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureAddressMode {
    Clamp,
    Repeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFallbackReason {
    Missing,
    Read,
    Decode,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureOrigin {
    Decoded {
        canonical_path: String,
    },
    GeneratedTeamColor,
    GeneratedTeamGlow,
    Fallback {
        canonical_path: Option<String>,
        reason: TextureFallbackReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSceneTexture {
    pub index: TextureIndex,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub color_space: CpuColorSpace,
    pub alpha_encoding: CpuAlphaEncoding,
    pub address_u: TextureAddressMode,
    pub address_v: TextureAddressMode,
    pub origin: TextureOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureSourceError {
    Read,
    Unsupported,
}

pub trait TextureByteSource {
    fn read(&mut self, canonical_path: &str) -> Result<Option<Vec<u8>>, TextureSourceError>;
}

impl<F> TextureByteSource for F
where
    F: FnMut(&str) -> Result<Option<Vec<u8>>, TextureSourceError>,
{
    fn read(&mut self, canonical_path: &str) -> Result<Option<Vec<u8>>, TextureSourceError> {
        self(canonical_path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneTextureError {
    InvalidTeamColor,
    UnsafePath,
    InputTooLarge,
    DimensionOverflow,
    DuplicateTextureIndex,
    CanonicalPathCollision,
}

#[derive(Debug, Clone)]
struct CpuAsset {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

pub struct SceneTextureResolver<S> {
    source: S,
    cache: BTreeMap<String, CpuAsset>,
    spellings: BTreeMap<String, String>,
}

impl<S: TextureByteSource> SceneTextureResolver<S> {
    pub fn new(source: S) -> Self {
        Self {
            source,
            cache: BTreeMap::new(),
            spellings: BTreeMap::new(),
        }
    }

    pub fn resolve(
        &mut self,
        request: &SceneTextureRequest,
        team_color: [f32; 3],
    ) -> Result<ResolvedSceneTexture, SceneTextureError> {
        let team = team_bytes(team_color)?;
        let (address_u, address_v) = address_modes(request);
        match request.replaceable_id {
            1 => {
                let key = format!(
                    "generated/team-color/{:02x}{:02x}{:02x}",
                    team[0], team[1], team[2]
                );
                let asset = self
                    .cache
                    .entry(key)
                    .or_insert_with(|| team_color_asset(team))
                    .clone();
                Ok(resolved(
                    request.index,
                    asset,
                    CpuAlphaEncoding::Straight,
                    address_u,
                    address_v,
                    TextureOrigin::GeneratedTeamColor,
                ))
            }
            2 => {
                let key = format!(
                    "generated/team-glow/{:02x}{:02x}{:02x}",
                    team[0], team[1], team[2]
                );
                let asset = self
                    .cache
                    .entry(key)
                    .or_insert_with(|| team_glow_asset(team_color))
                    .clone();
                Ok(resolved(
                    request.index,
                    asset,
                    CpuAlphaEncoding::Premultiplied,
                    address_u,
                    address_v,
                    TextureOrigin::GeneratedTeamGlow,
                ))
            }
            replaceable_id => {
                let (canonical_path, spelling) = if replaceable_id >= 3 {
                    let mapped = replaceable_path(replaceable_id).to_string();
                    (mapped.clone(), mapped)
                } else {
                    canonical_path(&request.filename)?
                };
                self.record_spelling(&canonical_path, spelling)?;
                if let Some(asset) = self.cache.get(&canonical_path).cloned() {
                    return Ok(resolved(
                        request.index,
                        asset,
                        CpuAlphaEncoding::Straight,
                        address_u,
                        address_v,
                        TextureOrigin::Decoded { canonical_path },
                    ));
                }

                let bytes = match self.source.read(&canonical_path) {
                    Ok(Some(bytes)) => bytes,
                    Ok(None) => {
                        return Ok(fallback(
                            request,
                            address_u,
                            address_v,
                            Some(canonical_path),
                            TextureFallbackReason::Missing,
                        ));
                    }
                    Err(TextureSourceError::Read) => {
                        return Ok(fallback(
                            request,
                            address_u,
                            address_v,
                            Some(canonical_path),
                            TextureFallbackReason::Read,
                        ));
                    }
                    Err(TextureSourceError::Unsupported) => {
                        return Ok(fallback(
                            request,
                            address_u,
                            address_v,
                            Some(canonical_path),
                            TextureFallbackReason::Unsupported,
                        ));
                    }
                };
                if bytes.len() > MAX_SOURCE_BYTES {
                    return Err(SceneTextureError::InputTooLarge);
                }
                if let Err(error) = validate_blp_dimensions(&bytes) {
                    return match error {
                        BlpInputError::Unsupported => Ok(fallback(
                            request,
                            address_u,
                            address_v,
                            Some(canonical_path),
                            TextureFallbackReason::Unsupported,
                        )),
                        BlpInputError::DimensionOverflow => {
                            Err(SceneTextureError::DimensionOverflow)
                        }
                    };
                }
                let decoded = std::panic::catch_unwind(|| {
                    blp::core::decode::decode_to_rgba(&bytes).map(|image| {
                        let width = image.width();
                        let height = image.height();
                        let rgba = image.to_rgba8().into_raw();
                        CpuAsset {
                            width,
                            height,
                            rgba,
                        }
                    })
                });
                let Ok(Ok(asset)) = decoded else {
                    return Ok(fallback(
                        request,
                        address_u,
                        address_v,
                        Some(canonical_path),
                        TextureFallbackReason::Decode,
                    ));
                };
                validate_decoded_size(asset.width, asset.height, asset.rgba.len())?;
                self.cache.insert(canonical_path.clone(), asset.clone());
                Ok(resolved(
                    request.index,
                    asset,
                    CpuAlphaEncoding::Straight,
                    address_u,
                    address_v,
                    TextureOrigin::Decoded { canonical_path },
                ))
            }
        }
    }

    pub fn resolve_all_canonical(
        &mut self,
        requests: &[SceneTextureRequest],
        team_color: [f32; 3],
    ) -> Result<Vec<ResolvedSceneTexture>, SceneTextureError> {
        let mut sorted = requests.iter().collect::<Vec<_>>();
        sorted.sort_by_key(|request| request.index.0);
        if sorted.windows(2).any(|pair| pair[0].index == pair[1].index) {
            return Err(SceneTextureError::DuplicateTextureIndex);
        }
        sorted
            .into_iter()
            .map(|request| self.resolve(request, team_color))
            .collect()
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.spellings.clear();
    }

    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    pub fn source_mut(&mut self) -> &mut S {
        &mut self.source
    }

    fn record_spelling(
        &mut self,
        canonical: &str,
        spelling: String,
    ) -> Result<(), SceneTextureError> {
        if let Some(existing) = self.spellings.get(canonical) {
            if existing != &spelling && existing.eq_ignore_ascii_case(&spelling) {
                return Err(SceneTextureError::CanonicalPathCollision);
            }
        } else {
            self.spellings.insert(canonical.to_string(), spelling);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlpInputError {
    Unsupported,
    DimensionOverflow,
}

fn team_bytes(color: [f32; 3]) -> Result<[u8; 3], SceneTextureError> {
    if color
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return Err(SceneTextureError::InvalidTeamColor);
    }
    Ok(color.map(|value| (value * 255.0).round() as u8))
}

fn address_modes(request: &SceneTextureRequest) -> (TextureAddressMode, TextureAddressMode) {
    (
        if request.wrap_u {
            TextureAddressMode::Repeat
        } else {
            TextureAddressMode::Clamp
        },
        if request.wrap_v {
            TextureAddressMode::Repeat
        } else {
            TextureAddressMode::Clamp
        },
    )
}

fn team_color_asset(color: [u8; 3]) -> CpuAsset {
    let mut rgba = vec![0; 8 * 8 * 4];
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[color[0], color[1], color[2], 255]);
    }
    CpuAsset {
        width: 8,
        height: 8,
        rgba,
    }
}

fn team_glow_asset(color: [f32; 3]) -> CpuAsset {
    let mut rgba = vec![0; 32 * 32 * 4];
    for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
        let alpha = team_glow_alpha(index);
        pixel.copy_from_slice(&[
            (color[0] * f32::from(alpha)).round() as u8,
            (color[1] * f32::from(alpha)).round() as u8,
            (color[2] * f32::from(alpha)).round() as u8,
            alpha,
        ]);
    }
    CpuAsset {
        width: 32,
        height: 32,
        rgba,
    }
}

fn team_glow_alpha(index: usize) -> u8 {
    let bytes = TEAM_GLOW_HEX.as_bytes();
    (hex_nibble(bytes[index * 2]) << 4) | hex_nibble(bytes[index * 2 + 1])
}

fn hex_nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => 0,
    }
}

fn replaceable_path(id: u32) -> &'static str {
    match id {
        11 => "replaceabletextures/cliff/cliff0.blp",
        31 => "replaceabletextures/lordaerontree/lordaeronsummertree.blp",
        32 => "replaceabletextures/ashenvaletree/ashentree.blp",
        33 => "replaceabletextures/barrenstree/barrenstree.blp",
        34 => "replaceabletextures/northrendtree/northtree.blp",
        35 => "replaceabletextures/mushroom/mushroomtree.blp",
        _ => "replaceabletextures/lordaerontree/lordaeronsummertree.blp",
    }
}

fn canonical_path(path: &str) -> Result<(String, String), SceneTextureError> {
    if path.is_empty() || path.contains('\0') {
        return Err(SceneTextureError::UnsafePath);
    }
    let spelling = path.replace('\\', "/");
    if spelling.starts_with('/') || spelling.contains(':') {
        return Err(SceneTextureError::UnsafePath);
    }
    let mut parts = Vec::new();
    for part in spelling.split('/') {
        if part == ".." {
            return Err(SceneTextureError::UnsafePath);
        }
        if !part.is_empty() && part != "." {
            parts.push(part);
        }
    }
    if parts.is_empty() {
        return Err(SceneTextureError::UnsafePath);
    }
    let mut spelling = parts.join("/");
    if !spelling.to_ascii_lowercase().ends_with(".blp") {
        spelling.push_str(".blp");
    }
    Ok((spelling.to_ascii_lowercase(), spelling))
}

fn validate_blp_dimensions(bytes: &[u8]) -> Result<(), BlpInputError> {
    if bytes.len() < 4 || !matches!(&bytes[..4], b"BLP1" | b"BLP2") {
        return Err(BlpInputError::Unsupported);
    }
    if bytes.len() < 20 {
        return Ok(());
    }
    let width = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let height = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    validate_pixel_count(width, height)
        .map(|_| ())
        .map_err(|_| BlpInputError::DimensionOverflow)
}

fn validate_decoded_size(width: u32, height: u32, actual: usize) -> Result<(), SceneTextureError> {
    let expected = validate_pixel_count(width, height)?;
    if actual != expected {
        return Err(SceneTextureError::DimensionOverflow);
    }
    Ok(())
}

fn validate_pixel_count(width: u32, height: u32) -> Result<usize, SceneTextureError> {
    let bytes = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(SceneTextureError::DimensionOverflow)?;
    if width == 0 || height == 0 || bytes > MAX_DECODED_BYTES {
        return Err(SceneTextureError::DimensionOverflow);
    }
    Ok(bytes)
}

fn resolved(
    index: TextureIndex,
    asset: CpuAsset,
    alpha_encoding: CpuAlphaEncoding,
    address_u: TextureAddressMode,
    address_v: TextureAddressMode,
    origin: TextureOrigin,
) -> ResolvedSceneTexture {
    ResolvedSceneTexture {
        index,
        width: asset.width,
        height: asset.height,
        rgba: asset.rgba,
        color_space: CpuColorSpace::Srgb,
        alpha_encoding,
        address_u,
        address_v,
        origin,
    }
}

fn fallback(
    request: &SceneTextureRequest,
    address_u: TextureAddressMode,
    address_v: TextureAddressMode,
    canonical_path: Option<String>,
    reason: TextureFallbackReason,
) -> ResolvedSceneTexture {
    resolved(
        request.index,
        CpuAsset {
            width: FALLBACK_SIZE,
            height: FALLBACK_SIZE,
            rgba: vec![200; (FALLBACK_SIZE * FALLBACK_SIZE * 4) as usize],
        },
        CpuAlphaEncoding::Straight,
        address_u,
        address_v,
        TextureOrigin::Fallback {
            canonical_path,
            reason,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct MemorySource {
        files: BTreeMap<String, Result<Option<Vec<u8>>, TextureSourceError>>,
        reads: BTreeMap<String, usize>,
    }

    impl TextureByteSource for MemorySource {
        fn read(&mut self, path: &str) -> Result<Option<Vec<u8>>, TextureSourceError> {
            *self.reads.entry(path.to_string()).or_default() += 1;
            self.files.get(path).cloned().unwrap_or(Ok(None))
        }
    }

    fn request(index: u32, filename: &str, replaceable_id: u32) -> SceneTextureRequest {
        SceneTextureRequest {
            index: TextureIndex(index),
            filename: filename.to_string(),
            replaceable_id,
            wrap_u: false,
            wrap_v: false,
        }
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
    }

    #[test]
    fn generated_team_color_and_independent_wrap_modes_are_exact() {
        let mut resolver = SceneTextureResolver::new(MemorySource::default());
        for (wrap_u, wrap_v, expected_u, expected_v) in [
            (
                false,
                false,
                TextureAddressMode::Clamp,
                TextureAddressMode::Clamp,
            ),
            (
                true,
                false,
                TextureAddressMode::Repeat,
                TextureAddressMode::Clamp,
            ),
            (
                false,
                true,
                TextureAddressMode::Clamp,
                TextureAddressMode::Repeat,
            ),
            (
                true,
                true,
                TextureAddressMode::Repeat,
                TextureAddressMode::Repeat,
            ),
        ] {
            let mut request = request(1, "", 1);
            request.wrap_u = wrap_u;
            request.wrap_v = wrap_v;
            let resolved = resolver.resolve(&request, [0.5, 0.25, 1.0]).unwrap();
            assert_eq!((resolved.width, resolved.height), (8, 8));
            assert_eq!(&resolved.rgba[..4], &[128, 64, 255, 255]);
            assert!(
                resolved
                    .rgba
                    .chunks_exact(4)
                    .all(|pixel| pixel == [128, 64, 255, 255])
            );
            assert_eq!(resolved.alpha_encoding, CpuAlphaEncoding::Straight);
            assert_eq!(
                (resolved.address_u, resolved.address_v),
                (expected_u, expected_v)
            );
            assert_eq!(resolved.origin, TextureOrigin::GeneratedTeamColor);
        }
        assert_eq!(
            fnv1a64(
                &resolver
                    .resolve(&request(1, "", 1), [0.5, 0.25, 1.0])
                    .unwrap()
                    .rgba
            ),
            7_703_903_293_910_979_493
        );
    }

    #[test]
    fn generated_team_glow_uses_original_premultiplied_alpha_table() {
        let mut resolver = SceneTextureResolver::new(MemorySource::default());
        let resolved = resolver
            .resolve(&request(2, "", 2), [0.5, 0.25, 1.0])
            .unwrap();
        assert_eq!((resolved.width, resolved.height), (32, 32));
        assert_eq!(resolved.alpha_encoding, CpuAlphaEncoding::Premultiplied);
        assert_eq!(&resolved.rgba[..4], &[1, 0, 1, 1]);
        assert!(resolved.rgba.chunks_exact(4).all(|pixel| pixel[3] <= 127));
        assert_eq!(fnv1a64(&resolved.rgba), 7_024_167_445_660_452_048);
        assert_eq!(resolved.origin, TextureOrigin::GeneratedTeamGlow);
    }

    #[test]
    fn invalid_team_colors_are_hard_errors() {
        let mut resolver = SceneTextureResolver::new(MemorySource::default());
        for color in [
            [-0.1, 0.0, 0.0],
            [1.1, 0.0, 0.0],
            [f32::NAN, 0.0, 0.0],
            [f32::INFINITY, 0.0, 0.0],
        ] {
            assert_eq!(
                resolver.resolve(&request(1, "", 1), color),
                Err(SceneTextureError::InvalidTeamColor)
            );
        }
    }

    #[test]
    fn unsafe_paths_are_errors_and_replaceable_ids_map_to_original_keys() {
        let mut resolver = SceneTextureResolver::new(MemorySource::default());
        for path in [
            "",
            "/absolute.blp",
            "\\\\server\\share.blp",
            "C:\\bad.blp",
            "a/../b.blp",
            "a\0b.blp",
        ] {
            assert_eq!(
                resolver.resolve(&request(0, path, 0), [1.0; 3]),
                Err(SceneTextureError::UnsafePath)
            );
        }
        let cases = [
            (11, "replaceabletextures/cliff/cliff0.blp"),
            (
                31,
                "replaceabletextures/lordaerontree/lordaeronsummertree.blp",
            ),
            (32, "replaceabletextures/ashenvaletree/ashentree.blp"),
            (33, "replaceabletextures/barrenstree/barrenstree.blp"),
            (34, "replaceabletextures/northrendtree/northtree.blp"),
            (35, "replaceabletextures/mushroom/mushroomtree.blp"),
            (
                99,
                "replaceabletextures/lordaerontree/lordaeronsummertree.blp",
            ),
        ];
        for (rid, expected) in cases {
            let resolved = resolver
                .resolve(&request(rid, "ignored", rid), [1.0; 3])
                .unwrap();
            assert_eq!(
                resolved.origin,
                TextureOrigin::Fallback {
                    canonical_path: Some(expected.to_string()),
                    reason: TextureFallbackReason::Missing
                }
            );
        }
    }

    #[test]
    fn canonical_batch_is_sorted_and_rejects_duplicate_indices_and_case_collisions() {
        let mut resolver = SceneTextureResolver::new(MemorySource::default());
        let resolved = resolver
            .resolve_all_canonical(
                &[request(5, "Zed", 0), request(2, "Alpha.blp", 0)],
                [1.0; 3],
            )
            .unwrap();
        assert_eq!(
            resolved.iter().map(|item| item.index.0).collect::<Vec<_>>(),
            vec![2, 5]
        );
        assert_eq!(
            resolver.resolve_all_canonical(&[request(1, "a", 0), request(1, "b", 0)], [1.0; 3]),
            Err(SceneTextureError::DuplicateTextureIndex)
        );
        resolver.clear();
        assert!(
            resolver
                .resolve(&request(1, "Textures/Foo.blp", 0), [1.0; 3])
                .is_ok()
        );
        assert_eq!(
            resolver.resolve(&request(2, "textures/foo.blp", 0), [1.0; 3]),
            Err(SceneTextureError::CanonicalPathCollision)
        );
    }

    #[test]
    fn tracked_blp_assets_decode_deterministically_and_successes_are_cached() {
        const EMBER: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/test-data/Ember Forge  Ember Knight/Ember Knight/EmberKnight.blp"
        ));
        const ICON: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/test-data/Ember Forge  Ember Knight/Ember Knight Icons/BTNEmberKnight.blp"
        ));
        let mut source = MemorySource::default();
        source
            .files
            .insert("units/emberknight.blp".into(), Ok(Some(EMBER.to_vec())));
        source
            .files
            .insert("icons/btnemberknight.blp".into(), Ok(Some(ICON.to_vec())));
        let mut resolver = SceneTextureResolver::new(source);

        for (index, path, dimensions, expected_hash) in [
            (
                0,
                "Units\\EmberKnight",
                (512, 512),
                17_545_331_089_151_121_430,
            ),
            (
                1,
                "Icons/BTNEmberKnight.blp",
                (64, 64),
                8_352_639_672_607_133_290,
            ),
        ] {
            let first = resolver
                .resolve(&request(index, path, 0), [1.0; 3])
                .unwrap();
            let second = resolver
                .resolve(&request(index, path, 0), [1.0; 3])
                .unwrap();
            assert_eq!((first.width, first.height), dimensions);
            assert_eq!(first, second);
            assert_eq!(fnv1a64(&first.rgba), expected_hash);
        }
        assert_eq!(resolver.cache_len(), 2);
        assert_eq!(
            resolver.source_mut().reads.values().copied().sum::<usize>(),
            2
        );
    }

    #[test]
    fn fallbacks_are_typed_not_cached_and_recover_when_source_recovers() {
        const ICON: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/test-data/Ember Forge  Ember Knight/Ember Knight Icons/BTNEmberKnight.blp"
        ));
        let request = request(0, "recover", 0);
        let mut resolver = SceneTextureResolver::new(MemorySource::default());
        let first = resolver.resolve(&request, [1.0; 3]).unwrap();
        assert_eq!(first.rgba, vec![200; 8 * 8 * 4]);
        assert_eq!(
            first.origin,
            TextureOrigin::Fallback {
                canonical_path: Some("recover.blp".into()),
                reason: TextureFallbackReason::Missing
            }
        );
        assert_eq!(resolver.cache_len(), 0);

        resolver
            .source_mut()
            .files
            .insert("recover.blp".into(), Ok(Some(ICON.to_vec())));
        let recovered = resolver.resolve(&request, [1.0; 3]).unwrap();
        assert_eq!((recovered.width, recovered.height), (64, 64));
        assert!(matches!(recovered.origin, TextureOrigin::Decoded { .. }));
        assert_eq!(resolver.cache_len(), 1);

        resolver.clear();
        resolver
            .source_mut()
            .files
            .insert("recover.blp".into(), Err(TextureSourceError::Read));
        let read = resolver.resolve(&request, [1.0; 3]).unwrap();
        assert!(matches!(
            read.origin,
            TextureOrigin::Fallback {
                reason: TextureFallbackReason::Read,
                ..
            }
        ));
        resolver
            .source_mut()
            .files
            .insert("recover.blp".into(), Err(TextureSourceError::Unsupported));
        let unsupported = resolver.resolve(&request, [1.0; 3]).unwrap();
        assert!(matches!(
            unsupported.origin,
            TextureOrigin::Fallback {
                reason: TextureFallbackReason::Unsupported,
                ..
            }
        ));
    }

    #[test]
    fn malformed_truncated_and_oversized_inputs_never_panic() {
        let cases = [
            (vec![0; 3], Ok(TextureFallbackReason::Unsupported)),
            (b"BLP1".to_vec(), Ok(TextureFallbackReason::Decode)),
            (
                {
                    let mut bytes = b"BLP1".to_vec();
                    bytes.resize(32, 0);
                    bytes[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
                    bytes[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
                    bytes
                },
                Err(SceneTextureError::DimensionOverflow),
            ),
            (
                vec![0; MAX_SOURCE_BYTES + 1],
                Err(SceneTextureError::InputTooLarge),
            ),
        ];
        for (index, (bytes, expected)) in cases.into_iter().enumerate() {
            let mut source = MemorySource::default();
            source.files.insert("bad.blp".into(), Ok(Some(bytes)));
            let mut resolver = SceneTextureResolver::new(source);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                resolver.resolve(&request(index as u32, "bad", 0), [1.0; 3])
            }));
            let result = result.expect("malformed input must not panic");
            match expected {
                Ok(reason) => assert!(
                    matches!(result.unwrap().origin, TextureOrigin::Fallback { reason: actual, .. } if actual == reason)
                ),
                Err(error) => assert_eq!(result, Err(error)),
            }
        }
    }
}
