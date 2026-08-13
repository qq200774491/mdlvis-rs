use crate::model::model::Model;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum TextureStatus {
    NotLoaded,
    LoadingLocal,
    LoadingRemote,
    Loaded,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct TextureInfo {
    pub texture_id: usize,
    pub filename: String,
    pub replaceable_id: u32,
    pub status: TextureStatus,
    pub progress: f32, // 0.0 to 1.0
    pub local_path: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
}

impl TextureInfo {
    pub fn new(texture_id: usize, filename: String, replaceable_id: u32) -> Self {
        Self {
            texture_id,
            filename,
            replaceable_id,
            status: TextureStatus::NotLoaded,
            progress: 0.0,
            local_path: None,
            width: 0,
            height: 0,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(
            self.status,
            TextureStatus::LoadingLocal | TextureStatus::LoadingRemote
        )
    }

    pub fn is_loaded(&self) -> bool {
        matches!(self.status, TextureStatus::Loaded)
    }

    pub fn has_error(&self) -> bool {
        matches!(self.status, TextureStatus::Error(_))
    }

    pub fn status_text(&self) -> String {
        match &self.status {
            TextureStatus::NotLoaded => crate::i18n::t("texture.status.not-loaded"),
            TextureStatus::LoadingLocal => crate::i18n::t("texture.status.loading-local"),
            TextureStatus::LoadingRemote => crate::i18n::t("texture.status.loading-remote"),
            TextureStatus::Loaded => crate::i18n::t_args(
                "texture.status.loaded",
                [
                    ("width", self.width.into()),
                    ("height", self.height.into()),
                ],
            ),
            TextureStatus::Error(err) => crate::i18n::t_args(
                "texture.status.local-error",
                [("error", err.as_str().into())],
            ),
        }
    }

    pub fn status_color(&self) -> egui::Color32 {
        match &self.status {
            TextureStatus::NotLoaded => egui::Color32::GRAY,
            TextureStatus::LoadingLocal | TextureStatus::LoadingRemote => egui::Color32::YELLOW,
            TextureStatus::Loaded => egui::Color32::GREEN,
            TextureStatus::Error(_) => egui::Color32::RED,
        }
    }
}

pub struct TextureManager {
    pub textures: Vec<TextureInfo>,
    model_directory: Option<PathBuf>,
}

impl TextureManager {
    pub fn new() -> Self {
        Self {
            textures: Vec::new(),
            model_directory: None,
        }
    }

    pub fn set_model_path(&mut self, model_path: &Path) {
        self.model_directory = model_path.parent().map(|p| p.to_path_buf());
    }

    pub fn init_from_model(&mut self, model: &Model) {
        self.textures.clear();

        for (id, texture) in model.textures.iter().enumerate() {
            let mut info = TextureInfo::new(id, texture.filename.clone(), texture.replaceable_id);

            // Check if already loaded
            if texture.image_data.is_some() {
                info.status = TextureStatus::Loaded;
                info.width = texture.width;
                info.height = texture.height;
            }

            self.textures.push(info);
        }
    }

    /// Try to find texture locally in model directory
    /// Case-insensitive search with automatic .blp extension
    pub fn find_local_path(&self, filename: &str) -> Option<PathBuf> {
        if let Some(model_dir) = &self.model_directory {
            // Normalize filename: lowercase, forward slashes
            let normalized = filename.to_lowercase().replace('\\', "/");

            // Add .blp extension if not present
            let with_extension = if !normalized.ends_with(".blp") {
                format!("{}.blp", normalized)
            } else {
                normalized.clone()
            };

            // Extract just the filename without path
            let filename_only = Path::new(&with_extension)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(&with_extension);

            // Directories to search in
            let search_dirs = vec![
                model_dir.clone(),          // Model directory
                model_dir.join("textures"), // textures/ subdirectory
                model_dir.join("Textures"),
                model_dir.join("TEXTURES"),
                model_dir.join(".."), // Parent directory
            ];

            // Search in each directory
            for search_dir in search_dirs {
                if !search_dir.exists() {
                    continue;
                }

                // Try to read directory
                if let Ok(entries) = std::fs::read_dir(&search_dir) {
                    for entry in entries.flatten() {
                        if let Ok(file_name) = entry.file_name().into_string() {
                            // Case-insensitive comparison
                            if file_name.to_lowercase() == filename_only {
                                return Some(entry.path());
                            }
                        }
                    }
                }

                // Also try direct path (in case of exact match)
                let direct = search_dir.join(filename_only);
                if direct.exists() {
                    return Some(direct);
                }
            }
        }
        None
    }

    pub fn get_texture(&self, id: usize) -> Option<&TextureInfo> {
        self.textures.get(id)
    }

    pub fn get_texture_mut(&mut self, id: usize) -> Option<&mut TextureInfo> {
        self.textures.get_mut(id)
    }

    pub fn loading_count(&self) -> usize {
        self.textures.iter().filter(|t| t.is_loading()).count()
    }

    pub fn loaded_count(&self) -> usize {
        self.textures.iter().filter(|t| t.is_loaded()).count()
    }

    pub fn error_count(&self) -> usize {
        self.textures.iter().filter(|t| t.has_error()).count()
    }
}
