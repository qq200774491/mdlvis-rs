use crate::CONFY_APP_NAME;
use crate::i18n;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    ZhCn,
    EnUs,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::ZhCn, Language::EnUs];

    pub fn from_tag(tag: &str) -> Self {
        match i18n::normalize_locale(tag) {
            "en-US" => Language::EnUs,
            _ => Language::ZhCn,
        }
    }

    pub fn locale(self) -> &'static str {
        match self {
            Language::ZhCn => "zh-CN",
            Language::EnUs => "en-US",
        }
    }

    pub fn native_name(self) -> String {
        match self {
            Language::ZhCn => i18n::t("locale.zh-CN"),
            Language::EnUs => i18n::t("locale.en-US"),
        }
    }

    pub fn apply(self) {
        i18n::set_locale(self.locale());
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
    #[serde(default = "default_locale")]
    pub locale: String,
}

fn default_locale() -> String {
    i18n::DEFAULT_LOCALE.to_string()
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
            locale: default_locale(),
        }
    }
}

impl UiSettings {
    pub fn load() -> Self {
        let mut settings: UiSettings = confy::load(CONFY_APP_NAME, "ui").unwrap_or_default();
        settings.locale = i18n::normalize_locale(&settings.locale).to_string();
        settings
    }

    pub fn save(&self) {
        let _ = confy::store(CONFY_APP_NAME, "ui", self);
    }

    pub fn language(&self) -> Language {
        Language::from_tag(&self.locale)
    }

    pub fn set_language(&mut self, language: Language) {
        self.locale = language.locale().to_string();
        language.apply();
    }
}

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
        settings.ui.language().apply();
        settings
    }
}
