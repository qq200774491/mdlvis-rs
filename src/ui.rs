use crate::material::{FilterMode, ShadingFlags};
use crate::model::model::Model;
use crate::settings::Settings;

pub struct Ui {
    show_geosets: Vec<bool>,
    selected_sequence: usize,
    current_frame: f32,
    is_playing: bool,
    is_looping: bool,
    use_animation: bool,
    last_update_time: f64,
    last_frame_time: f64,
}

impl Ui {
    pub fn new() -> Self {
        Self {
            show_geosets: Vec::new(),
            selected_sequence: 0,
            current_frame: 0.0,
            is_playing: false,
            is_looping: true,
            use_animation: false,
            last_update_time: 0.0,
            last_frame_time: 0.0,
        }
    }

    /// Reset animation state when a new model is loaded
    pub fn reset_animation(&mut self, model: &Option<Model>) {
        self.selected_sequence = 0;
        self.is_playing = false;
        self.use_animation = false; // Back to original parsed data
        self.last_update_time = 0.0;
        self.last_frame_time = 0.0;

        // Set current_frame to start of first sequence
        if let Some(model) = model {
            if !model.sequences.is_empty() {
                self.current_frame = model.sequences[0].start_frame as f32;
            } else {
                self.current_frame = 0.0;
            }
        } else {
            self.current_frame = 0.0;
        }
    }

    /// Update animation playback - advances current_frame based on time
    /// Should be called every frame BEFORE show()
    pub fn animate(&mut self, model: &Option<Model>, current_time: f64) {
        if !self.is_playing {
            return;
        }

        let Some(model) = model else { return };
        if model.sequences.is_empty() || self.selected_sequence >= model.sequences.len() {
            return;
        }

        let seq = &model.sequences[self.selected_sequence];

        // Initialize timing on first frame
        if self.last_update_time == 0.0 {
            self.last_update_time = current_time;
            self.last_frame_time = current_time;
            return;
        }

        // Calculate delta time
        let delta_time = current_time - self.last_update_time;
        self.last_update_time = current_time;

        // Advance frame (30 fps)
        let frame_delta = delta_time * 30.0;
        self.current_frame += frame_delta as f32;

        // Handle looping
        if self.current_frame >= seq.end_frame as f32 {
            if self.is_looping && !seq.non_looping {
                // Loop back to start
                self.current_frame =
                    seq.start_frame as f32 + (self.current_frame - seq.end_frame as f32);
            } else {
                // Stop at end
                self.current_frame = seq.end_frame as f32;
                self.is_playing = false;
            }
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        model: &mut Option<Model>,
        camera_yaw: f32,
        camera_pitch: f32,
        settings: &mut Settings,
        renderer: &mut crate::renderer::renderer::Renderer,
    ) -> (bool, f32, Vec<bool>, bool, bool, bool) {
        // reset_camera, current_frame, show_geosets, colors_changed, open_model, use_animation
        let mut reset_camera = false;
        let mut colors_changed = false;
        let mut open_model = false;

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                // Open Model button
                if ui.button("📁 打开模型").clicked() {
                    open_model = true;
                }

                ui.separator();
                ui.label("📋 窗口：");

                if ui
                    .button(if settings.ui.show_texture_panel {
                        "✅ 纹理"
                    } else {
                        "⬜ 纹理"
                    })
                    .clicked()
                {
                    settings.ui.show_texture_panel = !settings.ui.show_texture_panel;
                    settings.ui.save();
                }

                if ui
                    .button(if settings.ui.show_display_settings {
                        "✅ 显示设置"
                    } else {
                        "⬜ 显示设置"
                    })
                    .clicked()
                {
                    settings.ui.show_display_settings = !settings.ui.show_display_settings;
                    settings.ui.save();
                }

                if ui
                    .button(if settings.ui.show_colors {
                        "✅ 颜色"
                    } else {
                        "⬜ 颜色"
                    })
                    .clicked()
                {
                    settings.ui.show_colors = !settings.ui.show_colors;
                    settings.ui.save();
                }

                if ui
                    .button(if settings.ui.show_model_info {
                        "✅ 模型信息"
                    } else {
                        "⬜ 模型信息"
                    })
                    .clicked()
                {
                    settings.ui.show_model_info = !settings.ui.show_model_info;
                    settings.ui.save();
                }

                if ui
                    .button(if settings.ui.show_geosets {
                        "✅ 几何体"
                    } else {
                        "⬜ 几何体"
                    })
                    .clicked()
                {
                    settings.ui.show_geosets = !settings.ui.show_geosets;
                    settings.ui.save();
                }

                if ui
                    .button(if settings.ui.show_materials {
                        "✅ 材质"
                    } else {
                        "⬜ 材质"
                    })
                    .clicked()
                {
                    settings.ui.show_materials = !settings.ui.show_materials;
                    settings.ui.save();
                }

                if ui
                    .button(if settings.ui.show_animation {
                        "✅ 动画"
                    } else {
                        "⬜ 动画"
                    })
                    .clicked()
                {
                    settings.ui.show_animation = !settings.ui.show_animation;
                    settings.ui.save();
                }
            });
        });

        // Show windows based on UI settings
        if settings.ui.show_display_settings {
            reset_camera = self.show_display_settings_window(ctx, settings);
        }

        if settings.ui.show_colors {
            colors_changed = self.show_colors_window(ctx, settings);
        }

        if settings.ui.show_model_info {
            self.show_model_info_window(ctx, model, &mut settings.ui);
        }

        if settings.ui.show_geosets {
            self.show_geosets_window(ctx, model, &mut settings.ui);
        }

        if settings.ui.show_materials {
            self.show_materials_window(ctx, model, &mut settings.ui, renderer);
        }

        if settings.ui.show_animation {
            self.show_animation_window(ctx, model, &mut settings.ui);
        }

        // Draw axis gizmo in bottom-right corner (Blender-style)
        let gizmo_size = 100.0;
        let gizmo_margin = 20.0;

        // Get screen size - use available_rect which gives actual rendering area
        let screen_rect = ctx.viewport_rect();

        // Calculate bottom-right corner position
        let gizmo_x = screen_rect.max.x - gizmo_size - gizmo_margin;
        let gizmo_y = screen_rect.max.y - gizmo_size - gizmo_margin;
        let center = egui::pos2(gizmo_x + gizmo_size / 2.0, gizmo_y + gizmo_size / 2.0);
        let radius = gizmo_size / 2.8;
        let circle_radius = 11.0; // Radius of circles at axis ends

        // Calculate axis directions based on camera orientation
        let x_angle = -camera_yaw;
        let x_dir = egui::vec2(x_angle.cos(), -x_angle.sin()) * radius;
        let x_end = center + x_dir;

        let y_angle = -camera_yaw + std::f32::consts::FRAC_PI_2;
        let y_dir = egui::vec2(y_angle.cos(), -y_angle.sin()) * radius * camera_pitch.cos();
        let y_end = center + y_dir;

        let z_dir = egui::vec2(0.0, -camera_pitch.sin() * radius);
        let z_end = center + z_dir;

        // Get painter directly from ctx
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("axis_gizmo_painter"),
        ));
        let font_id = egui::FontId::proportional(14.0);

        // Draw circle background with darker, more professional look
        painter.circle_filled(
            center,
            gizmo_size / 2.0,
            egui::Color32::from_rgba_premultiplied(40, 40, 42, 220),
        );
        painter.circle_stroke(
            center,
            gizmo_size / 2.0,
            egui::Stroke::new(1.5, egui::Color32::from_gray(70)),
        );

        // Calculate depth
        let x_depth = (-camera_yaw).sin();
        let y_depth = (-camera_yaw - std::f32::consts::FRAC_PI_2).sin();
        let z_depth = camera_pitch.sin();

        // Blender-style colors (more saturated)
        let x_color = egui::Color32::from_rgb(220, 38, 38); // Bright red
        let y_color = egui::Color32::from_rgb(102, 204, 102); // Bright green
        let z_color = egui::Color32::from_rgb(64, 128, 255); // Bright blue

        // Sort and draw axes (back to front)
        let mut axes = vec![
            (x_depth, x_color, x_end, "X"),
            (y_depth, y_color, y_end, "Y"),
            (z_depth, z_color, z_end, "Z"),
        ];
        axes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        for (depth, color, end, label) in axes {
            if depth > 0.0 {
                // Front-facing axis - bright and bold
                // Draw line with gradient effect (thicker at base)
                painter.line_segment([center, end], egui::Stroke::new(3.5, color));

                // Draw circle at the end
                painter.circle_filled(end, circle_radius, color);

                // Draw label in white on the circle
                painter.text(
                    end,
                    egui::Align2::CENTER_CENTER,
                    label,
                    font_id.clone(),
                    egui::Color32::WHITE,
                );
            } else {
                // Back-facing axis - darker and thinner
                let darker = egui::Color32::from_rgba_premultiplied(
                    (color.r() as f32 * 0.4) as u8,
                    (color.g() as f32 * 0.4) as u8,
                    (color.b() as f32 * 0.4) as u8,
                    180,
                );
                painter.line_segment([center, end], egui::Stroke::new(2.0, darker));

                // Draw smaller circle at the end
                painter.circle_filled(end, circle_radius * 0.7, darker);
            }
        }

        (
            reset_camera,
            self.current_frame,
            self.show_geosets.clone(),
            colors_changed,
            open_model,
            self.use_animation,
        )
    }

    fn show_display_settings_window(
        &mut self,
        ctx: &egui::Context,
        settings: &mut Settings,
    ) -> bool {
        let mut reset_camera = false;

        egui::Window::new("🎨 显示设置")
            .default_width(300.0)
            .resizable(true)
            .open(&mut settings.ui.show_display_settings)
            .show(ctx, |ui| {
                let mut changed = false;

                changed |= ui
                    .checkbox(&mut settings.display.show_skeleton, "显示骨骼")
                    .changed();
                changed |= ui
                    .checkbox(&mut settings.display.wireframe_mode, "线框模式")
                    .changed();
                changed |= ui
                    .checkbox(&mut settings.display.show_grid, "显示网格")
                    .changed();
                changed |= ui
                    .checkbox(&mut settings.display.show_bounding_box, "显示包围盒")
                    .changed();

                ui.separator();
                ui.label("远裁剪面（视距）：");
                changed |= ui
                    .add(
                        egui::Slider::new(&mut settings.display.far_plane, 100.0..=5000.0)
                            .suffix(" 单位")
                            .logarithmic(true),
                    )
                    .changed();

                if changed {
                    settings.display.save();
                }

                ui.separator();

                if ui.button("重置相机").clicked() {
                    reset_camera = true;
                }
            });

        if !settings.ui.show_display_settings {
            settings.ui.save();
        }

        reset_camera
    }

    fn show_colors_window(&mut self, ctx: &egui::Context, settings: &mut Settings) -> bool {
        let mut colors_changed = false;

        egui::Window::new("🌈 颜色")
            .default_width(300.0)
            .resizable(true)
            .open(&mut settings.ui.show_colors)
            .show(ctx, |ui| {
                let mut changed = false;

                ui.label("队伍颜色：");
                changed |= ui
                    .color_edit_button_rgb(&mut settings.colors.team_color)
                    .changed();

                ui.label("天空盒颜色：");
                changed |= ui
                    .color_edit_button_rgb(&mut settings.colors.skybox_color)
                    .changed();

                ui.label("网格主线颜色：");
                changed |= ui
                    .color_edit_button_rgb(&mut settings.colors.grid_major_color)
                    .changed();

                ui.label("网格辅线颜色：");
                changed |= ui
                    .color_edit_button_rgb(&mut settings.colors.grid_minor_color)
                    .changed();

                ui.label("包围盒颜色：");
                changed |= ui
                    .color_edit_button_rgb(&mut settings.colors.bounding_box_color)
                    .changed();

                ui.separator();

                if ui.button("恢复默认颜色").clicked() {
                    settings.colors = crate::settings::ColorSettings::default();
                    changed = true;
                }

                if changed {
                    settings.colors.save();
                    colors_changed = true;
                }
            });

        if !settings.ui.show_colors {
            settings.ui.save();
        }

        colors_changed
    }

    fn show_model_info_window(
        &mut self,
        ctx: &egui::Context,
        model: &Option<Model>,
        ui_settings: &mut crate::settings::UiSettings,
    ) {
        egui::Window::new("ℹ️ 模型信息")
            .default_width(300.0)
            .resizable(true)
            .open(&mut ui_settings.show_model_info)
            .show(ctx, |ui| {
                if let Some(model) = model {
                    ui.label(format!("名称：{}", model.name));
                    ui.separator();

                    ui.label(format!("几何体：{}", model.geosets.len()));
                    let total_verts: usize = model.geosets.iter().map(|g| g.vertices.len()).sum();
                    let total_faces: usize = model.geosets.iter().map(|g| g.faces.len()).sum();
                    let total_uvs: usize = model.geosets.iter().map(|g| g.tex_coords.len()).sum();
                    ui.label(format!("  顶点总数：{}", total_verts));
                    ui.label(format!("  面总数：{}", total_faces));
                    ui.label(format!("  UV 总数：{}", total_uvs));

                    ui.separator();
                    ui.label(format!("材质：{}", model.materials.len()));
                    ui.label(format!("纹理：{}", model.textures.len()));
                    ui.label(format!("动画序列：{}", model.sequences.len()));
                    ui.label(format!("骨骼：{}", model.bones.len()));
                    ui.label(format!("辅助节点：{}", model.helpers.len()));
                } else {
                    ui.label("尚未加载模型");
                }
            });

        if !ui_settings.show_model_info {
            ui_settings.save();
        }
    }

    fn show_geosets_window(
        &mut self,
        ctx: &egui::Context,
        model: &Option<Model>,
        ui_settings: &mut crate::settings::UiSettings,
    ) {
        egui::Window::new("📦 几何体")
            .default_width(300.0)
            .resizable(true)
            .open(&mut ui_settings.show_geosets)
            .show(ctx, |ui| {
                if let Some(model) = model {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (i, geoset) in model.geosets.iter().enumerate() {
                            if self.show_geosets.len() <= i {
                                self.show_geosets.push(true);
                            }

                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.show_geosets[i], format!("#{}", i));
                                ui.label(format!(
                                    "{} 个顶点，{} 个面",
                                    geoset.vertices.len(),
                                    geoset.faces.len()
                                ));
                            });
                        }
                    });
                } else {
                    ui.label("尚未加载模型");
                }
            });

        if !ui_settings.show_geosets {
            ui_settings.save();
        }
    }

    fn show_materials_window(
        &mut self,
        ctx: &egui::Context,
        model: &mut Option<Model>,
        ui_settings: &mut crate::settings::UiSettings,
        renderer: &mut crate::renderer::renderer::Renderer,
    ) {
        egui::Window::new("🎨 材质")
            .default_width(400.0)
            .resizable(true)
            .open(&mut ui_settings.show_materials)
            .show(ctx, |ui| {
                if let Some(model) = model {
                    // Save immutable references before mutable iteration
                    let textures = &model.textures;

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (mat_id, material) in model.materials.iter_mut().enumerate() {
                            // Use CollapsingHeader for each material
                            let header_id = egui::Id::new(("material_header", mat_id));
                            egui::CollapsingHeader::new(format!(
                                "🎨 材质 #{}（{} 个图层）",
                                mat_id,
                                material.layers.len()
                            ))
                            .id_salt(header_id)
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // JSON copy button
                                    if ui
                                        .button("📋 JSON")
                                        .on_hover_text("将材质信息以 JSON 格式复制到剪贴板")
                                        .clicked()
                                    {
                                        // Build JSON representation
                                        let mut json = format!(
                                            "{{\n  \"material_id\": {},\n  \"layers\": [\n",
                                            mat_id
                                        );

                                        for (layer_id, layer) in material.layers.iter().enumerate()
                                        {
                                            json.push_str(&format!("    {{\n"));
                                            json.push_str(&format!(
                                                "      \"layer_id\": {},\n",
                                                layer_id
                                            ));

                                            if let Some(tex_id) = layer.texture_id {
                                                json.push_str(&format!(
                                                    "      \"texture_id\": {},\n",
                                                    tex_id
                                                ));

                                                if let Some(texture_info) = textures.get(tex_id) {
                                                    json.push_str(&format!(
                                                        "      \"filename\": \"{}\",\n",
                                                        texture_info.filename
                                                    ));
                                                    json.push_str(&format!(
                                                        "      \"replaceable_id\": {},\n",
                                                        texture_info.replaceable_id
                                                    ));
                                                }
                                            } else {
                                                json.push_str("      \"texture_id\": null,\n");
                                            }

                                            json.push_str(&format!(
                                                "      \"filter_mode\": \"{:?}\",\n",
                                                layer.filter_mode
                                            ));

                                            // Add shading flags as array of names (already parsed)
                                            if !layer.shading_flags.is_empty() {
                                                let flags_json: Vec<String> = layer
                                                    .shading_flags
                                                    .iter()
                                                    .map(|f| format!("\"{:?}\"", f))
                                                    .collect();
                                                json.push_str(&format!(
                                                    "      \"shading_flags\": [{}],\n",
                                                    flags_json.join(", ")
                                                ));
                                            } else {
                                                json.push_str("      \"shading_flags\": [],\n");
                                            }

                                            json.push_str(&format!(
                                                "      \"alpha\": {:.2}\n",
                                                layer.alpha
                                            ));

                                            if layer_id < material.layers.len() - 1 {
                                                json.push_str("    },\n");
                                            } else {
                                                json.push_str("    }\n");
                                            }
                                        }

                                        json.push_str("  ]\n}");

                                        // Copy to clipboard
                                        ctx.copy_text(json);
                                    }
                                });

                                ui.label(format!("图层数：{}", material.layers.len()));

                                // No need to initialize - data is in the model now

                                for (layer_id, layer) in material.layers.iter_mut().enumerate() {
                                    ui.separator();

                                    // Layer header with checkbox - edit model directly
                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut layer.enabled, "");
                                        ui.label(
                                            egui::RichText::new(format!("图层 #{}", layer_id))
                                                .strong(),
                                        );
                                    });

                                    ui.add_enabled_ui(layer.enabled, |ui| {
                                        if let Some(tex_id) = layer.texture_id {
                                            // Texture preview with collapsing header
                                            // Use unique ID based on material and layer to avoid conflicts
                                            let header_id = egui::Id::new((
                                                "texture_preview",
                                                mat_id,
                                                layer_id,
                                            ));

                                            // Build header text with RID if present
                                            let header_text = if let Some(texture_info) =
                                                textures.get(tex_id)
                                            {
                                                if texture_info.replaceable_id == 1 {
                                                    format!(
                                                        "    📎 纹理 #{} [RID：1 队伍颜色]",
                                                        tex_id
                                                    )
                                                } else if texture_info.replaceable_id == 2 {
                                                    format!(
                                                        "    📎 纹理 #{} [RID：2 队伍辉光]",
                                                        tex_id
                                                    )
                                                } else if texture_info.replaceable_id > 0 {
                                                    format!(
                                                        "    📎 纹理 #{} [RID：{}]",
                                                        tex_id, texture_info.replaceable_id
                                                    )
                                                } else {
                                                    format!("    📎 纹理 #{}", tex_id)
                                                }
                                            } else {
                                                format!("    📎 纹理 #{}", tex_id)
                                            };

                                            egui::CollapsingHeader::new(header_text)
                                                .id_salt(header_id)
                                                .default_open(false)
                                                .show(ui, |ui| {
                                                    // Don't use indent - CollapsingHeader already has proper indentation
                                                    if let Some(texture_info) = textures.get(tex_id)
                                                    {
                                                        // Show texture info
                                                        if !texture_info.filename.is_empty() {
                                                            ui.label(
                                                                egui::RichText::new(
                                                                    &texture_info.filename,
                                                                )
                                                                .small(),
                                                            );
                                                        }

                                                        // Show RID
                                                        if texture_info.replaceable_id == 1 {
                                                            ui.colored_label(
                                                                egui::Color32::GOLD,
                                                                "[RID：1 队伍颜色]",
                                                            );
                                                        } else if texture_info.replaceable_id == 2 {
                                                            ui.colored_label(
                                                                egui::Color32::GOLD,
                                                                "[RID：2 队伍辉光]",
                                                            );
                                                        } else if texture_info.replaceable_id > 0 {
                                                            ui.colored_label(
                                                                egui::Color32::GOLD,
                                                                format!(
                                                                    "[RID：{}]",
                                                                    texture_info.replaceable_id
                                                                ),
                                                            );
                                                        }

                                                        // Show texture preview
                                                        if let Some(egui_tex_id) =
                                                            renderer.get_egui_texture_id(tex_id)
                                                        {
                                                            ui.image(egui::ImageSource::Texture(
                                                                egui::load::SizedTexture {
                                                                    id: egui_tex_id,
                                                                    size: egui::vec2(128.0, 128.0),
                                                                },
                                                            ));
                                                        }
                                                    }
                                                });
                                        } else {
                                            ui.label("    纹理 ID：无");
                                        }

                                        // Filter Mode with collapsible checkboxes in column
                                        let current_filter_mode = layer
                                            .filter_mode_override
                                            .as_ref()
                                            .unwrap_or(&layer.filter_mode);

                                        let filter_name = current_filter_mode.name();

                                        let filter_header_id =
                                            egui::Id::new(("filter_mode", mat_id, layer_id));
                                        egui::CollapsingHeader::new(format!(
                                            "    过滤模式：{}",
                                            filter_name
                                        ))
                                        .id_salt(filter_header_id)
                                        .default_open(false)
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.add_space(20.0);

                                                // Reset button
                                                if ui
                                                    .small_button("↺")
                                                    .on_hover_text("恢复原始值")
                                                    .clicked()
                                                {
                                                    layer.filter_mode_override = None;
                                                }

                                                if layer.filter_mode_override.is_some() {
                                                    ui.label(
                                                        egui::RichText::new("（已修改）")
                                                            .small()
                                                            .weak(),
                                                    );
                                                }
                                            });

                                            ui.horizontal(|ui| {
                                                ui.add_space(20.0);
                                                ui.vertical(|ui| {
                                                    let current_mode = layer
                                                        .filter_mode_override
                                                        .clone()
                                                        .unwrap_or(layer.filter_mode.clone());

                                                    if ui
                                                        .radio(
                                                            matches!(
                                                                current_mode,
                                                                FilterMode::None
                                                            ),
                                                            "无",
                                                        )
                                                        .clicked()
                                                    {
                                                        layer.filter_mode_override =
                                                            Some(FilterMode::None);
                                                    }
                                                    if ui
                                                        .radio(
                                                            matches!(
                                                                current_mode,
                                                                FilterMode::Transparent
                                                            ),
                                                            "透明",
                                                        )
                                                        .clicked()
                                                    {
                                                        layer.filter_mode_override =
                                                            Some(FilterMode::Transparent);
                                                    }
                                                    if ui
                                                        .radio(
                                                            matches!(
                                                                current_mode,
                                                                FilterMode::Blend
                                                            ),
                                                            "混合",
                                                        )
                                                        .clicked()
                                                    {
                                                        layer.filter_mode_override =
                                                            Some(FilterMode::Blend);
                                                    }
                                                    if ui
                                                        .radio(
                                                            matches!(
                                                                current_mode,
                                                                FilterMode::Additive
                                                            ),
                                                            "加法",
                                                        )
                                                        .clicked()
                                                    {
                                                        layer.filter_mode_override =
                                                            Some(FilterMode::Additive);
                                                    }
                                                    if ui
                                                        .radio(
                                                            matches!(
                                                                current_mode,
                                                                FilterMode::AddAlpha
                                                            ),
                                                            "加法透明",
                                                        )
                                                        .clicked()
                                                    {
                                                        layer.filter_mode_override =
                                                            Some(FilterMode::AddAlpha);
                                                    }
                                                    if ui
                                                        .radio(
                                                            matches!(
                                                                current_mode,
                                                                FilterMode::Modulate
                                                            ),
                                                            "调制",
                                                        )
                                                        .clicked()
                                                    {
                                                        layer.filter_mode_override =
                                                            Some(FilterMode::Modulate);
                                                    }
                                                    if ui
                                                        .radio(
                                                            matches!(
                                                                current_mode,
                                                                FilterMode::Modulate2x
                                                            ),
                                                            "双倍调制",
                                                        )
                                                        .clicked()
                                                    {
                                                        layer.filter_mode_override =
                                                            Some(FilterMode::Modulate2x);
                                                    }
                                                });
                                            });
                                        });

                                        // Shading flags with active checkboxes in column (collapsible)
                                        let current_shading_flags = layer
                                            .shading_flags_override
                                            .as_ref()
                                            .unwrap_or(&layer.shading_flags);

                                        // Build bitmask (Unshaded, SphereEnvMap, TwoSided, Unfogged, NoDepthTest, NoDepthSet)
                                        let shading_mask = format!(
                                            "{}{}{}{}{}{}",
                                            if current_shading_flags
                                                .contains(&ShadingFlags::Unshaded)
                                            {
                                                "1"
                                            } else {
                                                "0"
                                            },
                                            if current_shading_flags
                                                .contains(&ShadingFlags::SphereEnvMap)
                                            {
                                                "1"
                                            } else {
                                                "0"
                                            },
                                            if current_shading_flags
                                                .contains(&ShadingFlags::TwoSided)
                                            {
                                                "1"
                                            } else {
                                                "0"
                                            },
                                            if current_shading_flags
                                                .contains(&ShadingFlags::Unfogged)
                                            {
                                                "1"
                                            } else {
                                                "0"
                                            },
                                            if current_shading_flags
                                                .contains(&ShadingFlags::NoDepthTest)
                                            {
                                                "1"
                                            } else {
                                                "0"
                                            },
                                            if current_shading_flags
                                                .contains(&ShadingFlags::NoDepthSet)
                                            {
                                                "1"
                                            } else {
                                                "0"
                                            },
                                        );

                                        let shading_header_id =
                                            egui::Id::new(("shading", mat_id, layer_id));
                                        egui::CollapsingHeader::new(format!(
                                            "    着色标志 {}",
                                            shading_mask
                                        ))
                                        .id_salt(shading_header_id)
                                        .default_open(false)
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.add_space(20.0);

                                                // Reset button
                                                if ui
                                                    .small_button("↺")
                                                    .on_hover_text("恢复原始值")
                                                    .clicked()
                                                {
                                                    layer.shading_flags_override = None;
                                                }

                                                if layer.shading_flags_override.is_some() {
                                                    ui.label(
                                                        egui::RichText::new("（已修改）")
                                                            .small()
                                                            .weak(),
                                                    );
                                                }
                                            });

                                            ui.horizontal(|ui| {
                                                ui.add_space(20.0);
                                                ui.vertical(|ui| {
                                                    // Get current flags (either override or original)
                                                    let mut current_flags = layer
                                                        .shading_flags_override
                                                        .clone()
                                                        .unwrap_or_else(|| {
                                                            layer.shading_flags.clone()
                                                        });

                                                    let mut changed = false;

                                                    // All possible shading flags
                                                    let all_flags = [
                                                        ShadingFlags::Unshaded,
                                                        ShadingFlags::SphereEnvMap,
                                                        ShadingFlags::TwoSided,
                                                        ShadingFlags::Unfogged,
                                                        ShadingFlags::NoDepthTest,
                                                        ShadingFlags::NoDepthSet,
                                                    ];

                                                    for flag in &all_flags {
                                                        let mut is_set =
                                                            current_flags.contains(flag);
                                                        if ui
                                                            .checkbox(&mut is_set, flag.name())
                                                            .changed()
                                                        {
                                                            if is_set {
                                                                if !current_flags.contains(flag) {
                                                                    current_flags.push(*flag);
                                                                    changed = true;
                                                                }
                                                            } else {
                                                                current_flags.retain(|f| f != flag);
                                                                changed = true;
                                                            }
                                                        }
                                                    }

                                                    if changed {
                                                        layer.shading_flags_override =
                                                            Some(current_flags);
                                                    }
                                                });
                                            });
                                        }); // end CollapsingHeader for Shading

                                        // Alpha slider with reset button
                                        ui.horizontal(|ui| {
                                            ui.label("    不透明度：");

                                            // Get current alpha (either override or original)
                                            let mut current_alpha =
                                                layer.alpha_override.unwrap_or(layer.alpha);

                                            if ui
                                                .add(
                                                    egui::Slider::new(
                                                        &mut current_alpha,
                                                        0.0..=1.0,
                                                    )
                                                    .step_by(0.01)
                                                    .show_value(true),
                                                )
                                                .changed()
                                            {
                                                layer.alpha_override = Some(current_alpha);
                                            }

                                            // Reset button
                                            if ui
                                                .small_button("↺")
                                                .on_hover_text("恢复原始值")
                                                .clicked()
                                            {
                                                layer.alpha_override = None;
                                            }

                                            // Show original value if overridden
                                            if layer.alpha_override.is_some() {
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "（原始值：{:.2}）",
                                                        layer.alpha
                                                    ))
                                                    .small()
                                                    .weak(),
                                                );
                                            }
                                        });
                                    }); // end add_enabled_ui
                                }
                            }); // end CollapsingHeader
                        }
                    });
                } else {
                    ui.label("尚未加载模型");
                }
            });

        if !ui_settings.show_materials {
            ui_settings.save();
        }
    }

    fn show_animation_window(
        &mut self,
        ctx: &egui::Context,
        model: &Option<Model>,
        ui_settings: &mut crate::settings::UiSettings,
    ) {
        egui::Window::new("🎬 动画")
            .default_width(350.0)
            .default_height(500.0)
            .resizable(true)
            .open(&mut ui_settings.show_animation)
            .show(ctx, |ui| {
                if let Some(model) = model {
                    if !model.sequences.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label("序列：");
                            ui.separator();

                            // Control buttons
                            ui.add_enabled_ui(!self.is_playing, |ui| {
                                if ui.button("▶ 播放").clicked() {
                                    self.is_playing = true;
                                    self.use_animation = true; // Enable animated transforms

                                    // Starting playback
                                    if self.selected_sequence < model.sequences.len() {
                                        let seq = &model.sequences[self.selected_sequence];
                                        // Reset to start if out of range
                                        if self.current_frame < seq.start_frame as f32
                                            || self.current_frame >= seq.end_frame as f32
                                        {
                                            self.current_frame = seq.start_frame as f32;
                                        }
                                    }
                                    self.last_update_time = 0.0; // Will be initialized on next update
                                    self.last_frame_time = 0.0;
                                }
                            });

                            ui.add_enabled_ui(self.is_playing, |ui| {
                                if ui.button("⏸ 暂停").clicked() {
                                    self.is_playing = false;
                                    self.last_update_time = 0.0;
                                }
                            });

                            // Stop button - stops and resets to start of sequence
                            let seq = &model.sequences[self.selected_sequence];
                            let can_stop =
                                self.is_playing || self.current_frame > seq.start_frame as f32;
                            ui.add_enabled_ui(can_stop, |ui| {
                                if ui.button("⏹ 停止").clicked() {
                                    self.is_playing = false;
                                    self.last_update_time = 0.0;
                                    self.current_frame = seq.start_frame as f32;
                                }
                            });

                            ui.separator();

                            // Reset button - disables animation, returns to original parsed data
                            if ui.button("🔄 重置").clicked() {
                                self.is_playing = false;
                                self.use_animation = false; // Disable animated transforms
                                self.last_update_time = 0.0;
                                self.current_frame =
                                    model.sequences[self.selected_sequence].start_frame as f32;
                            }

                            let loop_button = if self.is_looping {
                                "🔁 循环"
                            } else {
                                "➡ 单次"
                            };
                            if ui.button(loop_button).clicked() {
                                self.is_looping = !self.is_looping;
                            }
                        });

                        ui.separator();

                        // Sequences list - full width, flexible height
                        ui.label("动画列表：");
                        let available_height = ui.available_height() - 200.0; // Reserve space for details below
                        egui::ScrollArea::vertical()
                            .max_height(available_height.max(150.0))
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                for (i, seq) in model.sequences.iter().enumerate() {
                                    let is_selected = i == self.selected_sequence;
                                    let response = ui.selectable_label(is_selected, &seq.name);

                                    if response.clicked() {
                                        self.selected_sequence = i;
                                        self.current_frame = seq.start_frame as f32;
                                        self.is_playing = false;
                                        self.last_update_time = 0.0;
                                    }
                                }
                            });

                        ui.separator();

                        // Show sequence details (without border)
                        let seq = &model.sequences[self.selected_sequence];
                        ui.label(format!("动画：{}", seq.name));
                        ui.label(format!("帧范围：{} - {}", seq.start_frame, seq.end_frame));
                        ui.label(format!(
                            "时长：{} 帧（30 FPS 时为 {:.1} 秒）",
                            seq.end_frame - seq.start_frame,
                            (seq.end_frame - seq.start_frame) as f32 / 30.0
                        ));

                        // Show current state
                        let state_text = if self.is_playing {
                            format!("▶ 正在播放 - 第 {:.0} 帧", self.current_frame)
                        } else {
                            format!("⏸ 已暂停 - 第 {:.0} 帧", self.current_frame)
                        };
                        ui.label(egui::RichText::new(state_text).strong());

                        if seq.non_looping {
                            ui.label("⚠ 不循环");
                        }

                        if let Some(rarity) = seq.rarity {
                            ui.label(format!("稀有度：{}", rarity));
                        }

                        ui.separator();

                        // Frame slider
                        ui.horizontal(|ui| {
                            ui.label("帧：");
                            let frame_range = seq.start_frame as f32..=seq.end_frame as f32;
                            let slider_response = ui.add(
                                egui::Slider::new(&mut self.current_frame, frame_range).integer(),
                            );

                            // Only react to ACTUAL user interaction, not programmatic updates
                            if slider_response.drag_started() {
                                // User started dragging - pause animation
                                self.is_playing = false;
                                self.last_update_time = 0.0;
                            }

                            ui.label(format!("{:.0}", self.current_frame));
                        });
                    } else {
                        ui.label("模型中没有动画");
                    }
                } else {
                    ui.label("尚未加载模型");
                }
            });

        if !ui_settings.show_animation {
            ui_settings.save();
        }
    }
}
