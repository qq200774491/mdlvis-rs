use crate::renderer::renderer::Renderer;
use crate::texture::manager::{TextureManager, TextureStatus};

pub struct TexturePanel {
    // Panel state is now in Settings
    viewer_texture_id: Option<usize>,
    error_info_texture_id: Option<usize>,
}

impl TexturePanel {
    pub fn new() -> Self {
        Self {
            viewer_texture_id: None,
            error_info_texture_id: None,
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        texture_manager: &TextureManager,
        renderer: &mut Renderer,
        show_panel: &mut bool,
    ) -> Option<Vec<usize>> {
        if !*show_panel {
            return None;
        }

        let mut load_requests = Vec::new();

        // Show texture viewer if requested
        if let Some(texture_id) = self.viewer_texture_id {
            self.show_texture_viewer(ctx, texture_manager, renderer, texture_id);
        }

        // Show error info if requested
        if let Some(texture_id) = self.error_info_texture_id {
            self.show_error_info(ctx, texture_manager, texture_id);
        }

        egui::Window::new(crate::i18n::t("window.textures"))
            .default_width(400.0)
            .default_height(600.0)
            .resizable(true)
            .open(show_panel)
            .show(ctx, |ui| {
                // Header with statistics
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::t_args(
                        "texture.total",
                        [("count", texture_manager.textures.len().into())],
                    ));
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::GREEN,
                        crate::i18n::t_args(
                            "texture.loaded",
                            [("count", texture_manager.loaded_count().into())],
                        ),
                    );
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        crate::i18n::t_args(
                            "texture.loading",
                            [("count", texture_manager.loading_count().into())],
                        ),
                    );
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::RED,
                        crate::i18n::t_args(
                            "texture.errors",
                            [("count", texture_manager.error_count().into())],
                        ),
                    );
                });

                ui.separator();

                // Buttons
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::t("texture.load-missing")).clicked() {
                        for (id, texture) in texture_manager.textures.iter().enumerate() {
                            if !texture.is_loaded() && !texture.is_loading() {
                                load_requests.push(id);
                            }
                        }
                    }

                    if ui.button(crate::i18n::t("texture.retry-failed")).clicked() {
                        for (id, texture) in texture_manager.textures.iter().enumerate() {
                            if texture.has_error() {
                                load_requests.push(id);
                            }
                        }
                    }
                });

                ui.separator();

                // Texture list
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for texture in &texture_manager.textures {
                            ui.group(|ui| {
                                ui.set_min_width(ui.available_width());

                                // Header with ID and status indicator
                                ui.horizontal(|ui| {
                                    // Status circle
                                    let radius = 6.0;
                                    let (rect, _response) = ui.allocate_exact_size(
                                        egui::vec2(radius * 2.0, radius * 2.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().circle_filled(
                                        rect.center(),
                                        radius,
                                        texture.status_color(),
                                    );

                                    // Texture ID
                                    ui.label(format!("ID: {}", texture.texture_id));

                                    // Replaceable indicator - always show if RID != 0
                                    // All RID textures use same yellow/gold color
                                    if texture.replaceable_id == 1 {
                                        ui.colored_label(egui::Color32::GOLD, crate::i18n::t("material.rid-team-color"));
                                    } else if texture.replaceable_id == 2 {
                                        ui.colored_label(egui::Color32::GOLD, crate::i18n::t("material.rid-team-glow"));
                                    } else if texture.replaceable_id > 0 {
                                        ui.colored_label(
                                            egui::Color32::GOLD,
                                            crate::i18n::t_args("material.rid", [("rid", texture.replaceable_id.into())]),
                                        );
                                    }
                                });

                                // Filename
                                if !texture.filename.is_empty() {
                                    ui.label(egui::RichText::new(&texture.filename).small());
                                }

                                // Local path if found
                                if let Some(local_path) = &texture.local_path {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "📁 {}",
                                            local_path.display()
                                        ))
                                            .small()
                                            .color(egui::Color32::DARK_GREEN),
                                    );
                                }

                                // Status
                                ui.horizontal(|ui| {
                                    ui.label(crate::i18n::t("texture.status"));
                                    ui.colored_label(texture.status_color(), texture.status_text());
                                });

                                // Progress bar
                                if texture.is_loading() {
                                    ui.add(
                                        egui::ProgressBar::new(texture.progress)
                                            .show_percentage()
                                            .animate(true),
                                    );
                                }

                                // Action buttons
                                ui.horizontal(|ui| {
                                    // Show button for loaded textures
                                    if texture.is_loaded() {
                                        if ui.button(crate::i18n::t("texture.view")).clicked() {
                                            self.viewer_texture_id = Some(texture.texture_id);
                                        }
                                    }

                                    // Don't show Load/Retry buttons for RID textures - they are generated, not loaded
                                    if texture.replaceable_id == 0 {
                                        if !texture.is_loaded() && !texture.is_loading() {
                                            if ui.button(crate::i18n::t("texture.load")).clicked() {
                                                load_requests.push(texture.texture_id);
                                            }
                                        }

                                        if texture.has_error() {
                                            if ui.button(crate::i18n::t("texture.retry")).clicked() {
                                                load_requests.push(texture.texture_id);
                                            }
                                            if ui.button(crate::i18n::t("texture.details")).clicked() {
                                                self.error_info_texture_id =
                                                    Some(texture.texture_id);
                                            }
                                        }
                                    }
                                });
                            });
                            ui.add_space(4.0);
                        }
                    });
            });

        if load_requests.is_empty() {
            None
        } else {
            Some(load_requests)
        }
    }

    fn show_texture_viewer(
        &mut self,
        ctx: &egui::Context,
        _texture_manager: &TextureManager,
        renderer: &mut Renderer,
        texture_id: usize,
    ) {
        let mut is_open = true;

        egui::Window::new(crate::i18n::t_args("window.texture-viewer", [("id", texture_id.into())]))
            .default_width(512.0)
            .default_height(512.0)
            .resizable(true)
            .open(&mut is_open)
            .show(ctx, |ui| {
                ui.heading(crate::i18n::t("texture.viewer-heading"));

                // Try to get egui texture ID from renderer
                if let Some(egui_texture_id) = renderer.get_egui_texture_id(texture_id) {
                    // Calculate size to fit in window while maintaining aspect ratio
                    let available_size = ui.available_size();
                    let max_size = available_size.min_elem().min(512.0);

                    ui.image(egui::ImageSource::Texture(egui::load::SizedTexture::new(
                        egui_texture_id,
                        egui::vec2(max_size, max_size),
                    )));
                } else {
                    ui.label(crate::i18n::t("texture.not-available"));
                    ui.label(crate::i18n::t("texture.load-first"));
                }
            });

        if !is_open {
            self.viewer_texture_id = None;
        }
    }

    fn show_error_info(
        &mut self,
        ctx: &egui::Context,
        texture_manager: &TextureManager,
        texture_id: usize,
    ) {
        let mut is_open = true;

        if let Some(texture) = texture_manager.textures.get(texture_id) {
            egui::Window::new(crate::i18n::t_args("window.texture-error", [("id", texture_id.into())]))
                .default_width(400.0)
                .resizable(true)
                .open(&mut is_open)
                .show(ctx, |ui| {
                    ui.heading(crate::i18n::t("texture.error-heading"));
                    ui.separator();

                    ui.label(crate::i18n::t("texture.id"));
                    ui.label(format!("  {}", texture.texture_id));
                    ui.add_space(8.0);

                    if !texture.filename.is_empty() {
                        ui.label(crate::i18n::t("texture.filename"));
                        ui.label(format!("  {}", texture.filename));
                        ui.add_space(8.0);
                    }

                    if let Some(local_path) = &texture.local_path {
                        ui.label(crate::i18n::t("texture.tried-path"));
                        ui.label(format!("  {}", local_path.display()));
                        ui.add_space(8.0);
                    }

                    if texture.replaceable_id > 0 {
                        ui.label(crate::i18n::t("texture.replaceable-id"));
                        ui.label(format!("  {}", texture.replaceable_id));
                        ui.add_space(8.0);
                    }

                    ui.label(crate::i18n::t("texture.error"));
                    let error_msg = match &texture.status {
                        TextureStatus::Error(msg) => msg.clone(),
                        _ => crate::i18n::t("texture.unknown-error"),
                    };
                    ui.colored_label(egui::Color32::RED, format!("  {}", error_msg));
                    ui.add_space(8.0);

                    ui.separator();
                    ui.label(crate::i18n::t("texture.suggestions"));
                    ui.label(crate::i18n::t("texture.suggestion.exists"));
                    ui.label(crate::i18n::t("texture.suggestion.format"));
                    ui.label(crate::i18n::t("texture.suggestion.corrupt"));
                });
        }

        if !is_open {
            self.error_info_texture_id = None;
        }
    }
}
