use crate::error::MdlError;
use std::path::Path;

const TEXTURE_BASE_URL: &str = "https://github.com/WarRaft/War3.mpq/raw/refs/heads/main/lowercase";

pub enum TextureLoadResult {
    Success {
        texture_id: usize,
        rgba_data: Vec<u8>,
        width: u32,
        height: u32,
    },
    Error {
        texture_id: usize,
        error: String,
    },
}

/// Load texture from local file
pub async fn load_from_file(path: &Path) -> Result<Vec<u8>, MdlError> {
    let data = tokio::fs::read(path).await?;
    Ok(data)
}

/// Download a texture from the GitHub repository
pub async fn download_texture(path: &str) -> Result<Vec<u8>, MdlError> {
    // Convert path to lowercase and replace backslashes with forward slashes
    let normalized_path = path.to_lowercase().replace('\\', "/");

    // Remove leading slash if present
    let normalized_path = normalized_path.trim_start_matches('/');

    let url = format!("{}/{}", TEXTURE_BASE_URL, normalized_path);

    // Use async HTTP client
    let response = reqwest::get(&url).await.map_err(|e| {
        MdlError::new("network-error")
            .with_arg("msg", format!("Failed to download from {}: {}", url, e))
    })?;

    if !response.status().is_success() {
        return Err(MdlError::new("network-error")
            .with_arg("msg", format!("HTTP {} from {}", response.status(), url)));
    }

    let bytes = response.bytes().await.map_err(|e| {
        MdlError::new("network-error").with_arg(
            "msg",
            format!("Failed to read response from {}: {}", url, e),
        )
    })?;
    Ok(bytes.to_vec())
}

/// Load and decode a BLP texture
pub fn decode_blp(data: &[u8]) -> Result<(Vec<u8>, u32, u32), MdlError> {
    // Use blp crate to decode to RGBA
    let img = blp::core::decode::decode_to_rgba(data)?;

    let width = img.width();
    let height = img.height();

    // Convert to RGBA8
    let rgba_img = img.to_rgba8();
    let rgba_data = rgba_img.into_raw();

    Ok((rgba_data, width, height))
}

/// Download and decode a texture from the repository
pub async fn load_texture(path: &str) -> Result<(Vec<u8>, u32, u32), MdlError> {
    let blp_data = download_texture(path).await?;
    decode_blp(&blp_data)
}
