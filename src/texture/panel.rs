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

        egui::Window::new("纹理")
            .default_width(400.0)
            .default_height(600.0)
            .resizable(true)
            .open(show_panel)
            .show(ctx, |ui| {
                // Header with statistics
                ui.horizontal(|ui| {
                    ui.label(format!("总数：{}", texture_manager.textures.len()));
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::GREEN,
                        format!("已加载：{}", texture_manager.loaded_count()),
                    );
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("加载中：{}", texture_manager.loading_count()),
                    );
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::RED,
                        format!("错误：{}", texture_manager.error_count()),
                    );
                });

                ui.separator();

                // Buttons
                ui.horizontal(|ui| {
                    if ui.button("加载全部缺失纹理").clicked() {
                        for (id, texture) in texture_manager.textures.iter().enumerate() {
                            if !texture.is_loaded() && !texture.is_loading() {
                                load_requests.push(id);
                            }
                        }
                    }

                    if ui.button("重试失败项").clicked() {
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
                                        ui.colored_label(
                                            egui::Color32::GOLD,
                                            "[RID：1 队伍颜色]",
                                        );
                                    } else if texture.replaceable_id == 2 {
                                        ui.colored_label(egui::Color32::GOLD, "[RID：2 队伍辉光]");
                                    } else if texture.replaceable_id > 0 {
                                        ui.colored_label(
                                            egui::Color32::GOLD,
                                            format!("[RID：{}]", texture.replaceable_id),
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
                                        egui::RichText::new(format!("📁 {}", local_path.display()))
                                            .small()
                                            .color(egui::Color32::DARK_GREEN),
                                    );
                                }

                                // Status
                                ui.horizontal(|ui| {
                                    ui.label("状态：");
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
                                        if ui.button("👁 查看").clicked() {
                                            self.viewer_texture_id = Some(texture.texture_id);
                                        }
                                    }

                                    // Don't show Load/Retry buttons for RID textures - they are generated, not loaded
                                    if texture.replaceable_id == 0 {
                                        if !texture.is_loaded() && !texture.is_loading() {
                                            if ui.button("加载").clicked() {
                                                load_requests.push(texture.texture_id);
                                            }
                                        }

                                        if texture.has_error() {
                                            if ui.button("重试").clicked() {
                                                load_requests.push(texture.texture_id);
                                            }
                                            if ui.button("⚠ 详情").clicked() {
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

        egui::Window::new(format!("🖼 纹理查看器 - ID：{}", texture_id))
            .default_width(512.0)
            .default_height(512.0)
            .resizable(true)
            .open(&mut is_open)
            .show(ctx, |ui| {
                ui.heading("纹理查看器");

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
                    ui.label("⚠ 纹理尚未加载或不可用");
                    ui.label("请先在“纹理”窗口中加载该纹理");
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
            egui::Window::new(format!("⚠ 纹理错误 - ID：{}", texture_id))
                .default_width(400.0)
                .resizable(true)
                .open(&mut is_open)
                .show(ctx, |ui| {
                    ui.heading("纹理加载错误");
                    ui.separator();

                    ui.label("纹理 ID：");
                    ui.label(format!("  {}", texture.texture_id));
                    ui.add_space(8.0);

                    if !texture.filename.is_empty() {
                        ui.label("文件名：");
                        ui.label(format!("  {}", texture.filename));
                        ui.add_space(8.0);
                    }

                    if let Some(local_path) = &texture.local_path {
                        ui.label("尝试的路径：");
                        ui.label(format!("  {}", local_path.display()));
                        ui.add_space(8.0);
                    }

                    if texture.replaceable_id > 0 {
                        ui.label("可替换纹理 ID：");
                        ui.label(format!("  {}", texture.replaceable_id));
                        ui.add_space(8.0);
                    }

                    ui.label("错误：");
                    let error_msg = match &texture.status {
                        TextureStatus::Error(msg) => msg.clone(),
                        _ => "未知错误".to_string(),
                    };
                    ui.colored_label(egui::Color32::RED, format!("  {}", error_msg));
                    ui.add_space(8.0);

                    ui.separator();
                    ui.label("💡 建议：");
                    ui.label("  • 检查模型目录中是否存在该文件");
                    ui.label("  • 确认文件格式受支持（.blp）");
                    ui.label("  • 确认文件没有损坏");
                });
        }

        if !is_open {
            self.error_info_texture_id = None;
        }
    }
}
