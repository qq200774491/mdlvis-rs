use crate::CONFY_APP_NAME;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplaySettings {
    pub show_skeleton: bool,
    pub wireframe_mode: bool,
    pub show_grid: bool,
    pub show_bounding_box: bool,
    pub far_plane: f32,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            show_skeleton: true,
            wireframe_mode: false,
            show_grid: true,
            show_bounding_box: false,
            far_plane: 1000.0,
        }
    }
}

impl DisplaySettings {
    pub fn load() -> Self {
        confy::load("mdlvis-rs", "display").unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = confy::store("mdlvis-rs", "display", self);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorSettings {
    pub team_color: [f32; 3],
    pub skybox_color: [f32; 3],
    pub grid_major_color: [f32; 3],
    pub grid_minor_color: [f32; 3],
    pub bounding_box_color: [f32; 3],
}

impl Default for ColorSettings {
    fn default() -> Self {
        Self {
            team_color: [1.0, 0.0, 0.0],
            skybox_color: [0.02, 0.02, 0.02],
            grid_major_color: [0.06, 0.06, 0.06],
            grid_minor_color: [0.04, 0.04, 0.04],
            bounding_box_color: [1.0, 1.0, 0.0],
        }
    }
}

impl ColorSettings {
    pub fn load() -> Self {
        confy::load("mdlvis-rs", "colors").unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = confy::store("mdlvis-rs", "colors", self);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    En,
    ZhCn,
}

impl Default for Language {
    fn default() -> Self {
        Self::ZhCn
    }
}

impl Language {
    pub const ALL: [Language; 2] = [Language::En, Language::ZhCn];

    pub fn locale(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::ZhCn => "zh-CN",
        }
    }

    pub fn native_name(self) -> &'static str {
        match self {
            Language::En => "English",
            Language::ZhCn => "中文",
        }
    }

    pub fn apply(self) {
        rust_i18n::set_locale(self.locale());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    pub show_texture_panel: bool,
    pub show_display_settings: bool,
    pub show_colors: bool,
    pub show_model_info: bool,
    pub show_geosets: bool,
    pub show_animation: bool,
    pub show_materials: bool,
    #[serde(default)]
    pub language: Language,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            show_texture_panel: false,
            show_display_settings: false,
            show_colors: false,
            show_model_info: false,
            show_geosets: false,
            show_animation: false,
            show_materials: false,
            language: Language::default(),
        }
    }
}

impl UiSettings {
    pub fn load() -> Self {
        confy::load(CONFY_APP_NAME, "ui").unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = confy::store(CONFY_APP_NAME, "ui", self);
    }
}

// Aggregate struct for convenience
pub struct Settings {
    pub display: DisplaySettings,
    pub colors: ColorSettings,
    pub ui: UiSettings,
}

impl Settings {
    pub fn load() -> Self {
        let settings = Self {
            display: DisplaySettings::load(),
            colors: ColorSettings::load(),
            ui: UiSettings::load(),
        };
        settings.ui.language.apply();
        settings
    }
}
