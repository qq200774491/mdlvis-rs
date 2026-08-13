use crate::material::{FilterMode, MaterialUniform, ShadingFlags};
use crate::model::model::Model;
use crate::renderer::geoset_render_info::GeosetRenderInfo;
use crate::renderer::geoset_render_info::ScenePipelineState;
use crate::renderer::renderer::Renderer;
use egui_wgpu::ScreenDescriptor;

pub(crate) struct ScenePassBindings<'a> {
    pub camera: &'a wgpu::BindGroup,
    pub white_texture: &'a wgpu::BindGroup,
    pub textures: &'a [wgpu::BindGroup],
    pub legacy_material: &'a wgpu::BindGroup,
    pub scene_material: &'a wgpu::BindGroup,
}

pub(crate) fn record_prepared_scene<'pass>(
    render_pass: &mut wgpu::RenderPass<'pass>,
    scene: &'pass crate::renderer::renderer::PreparedScene,
    gpu: &'pass crate::renderer::renderer::SceneGpu,
    pipelines: &'pass [(ScenePipelineState, wgpu::RenderPipeline)],
    bindings: ScenePassBindings<'pass>,
    show_geosets: &[bool],
) -> Result<(), crate::error::MdlError> {
    if scene.draws.is_empty() || scene.vertices.is_empty() || scene.indices.is_empty() {
        return Ok(());
    }
    render_pass.set_bind_group(0, bindings.camera, &[]);
    render_pass.set_vertex_buffer(0, gpu.vertex_buffer.slice(..));
    render_pass.set_index_buffer(gpu.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
    for draw in &scene.draws {
        if show_geosets
            .get(draw.geoset as usize)
            .is_some_and(|visible| !visible)
        {
            continue;
        }
        let pipeline = pipelines
            .iter()
            .find(|(state, _)| *state == draw.pipeline)
            .map(|(_, pipeline)| pipeline)
            .ok_or_else(|| crate::error::MdlError::new("renderer-missing-scene-pipeline"))?;
        let texture_bind_group = match draw.texture_slot {
            Some(slot) => bindings.textures.get(slot as usize).ok_or_else(|| {
                crate::error::MdlError::new("renderer-missing-scene-texture-slot")
                    .with_arg("slot", slot)
            })?,
            None => bindings.white_texture,
        };
        render_pass.set_pipeline(pipeline);
        render_pass.set_bind_group(1, texture_bind_group, &[]);
        render_pass.set_bind_group(2, bindings.legacy_material, &[]);
        render_pass.set_bind_group(3, bindings.scene_material, &[draw.uniform_offset]);
        render_pass.draw_indexed(
            draw.index_start..draw.index_start + draw.index_count,
            0,
            0..1,
        );
    }
    Ok(())
}

impl Renderer {
    pub fn render(
        &mut self,
        model_opt: Option<&Model>,
        show_skeleton: bool,
        show_grid: bool,
        show_bounding_box: bool,
        wireframe_mode: bool,
        far_plane: f32,
        show_geosets: &Vec<bool>,
        paint_jobs: Vec<egui::ClippedPrimitive>,
        textures_delta: egui::TexturesDelta,
        screen_descriptor: ScreenDescriptor,
    ) -> Result<(), wgpu::SurfaceError> {
        // Skip rendering if window size is invalid (minimized, not ready, etc.)
        if self.config.width == 0 || self.config.height == 0 {
            return Ok(());
        }

        // Calculate viewport dimensions (no left panel anymore)
        let viewport_width = self.config.width as f32;
        let viewport_height = self.config.height as f32;

        // Update camera matrix with correct aspect ratio for viewport
        let aspect = viewport_width / viewport_height;
        let proj = nalgebra_glm::perspective(aspect, 45.0_f32.to_radians(), 0.1, far_plane);

        let eye = nalgebra_glm::vec3(
            self.camera.target[0]
                + self.camera.distance * self.camera.yaw.cos() * self.camera.pitch.cos(),
            self.camera.target[1]
                + self.camera.distance * self.camera.yaw.sin() * self.camera.pitch.cos(),
            self.camera.target[2] + self.camera.distance * self.camera.pitch.sin(),
        );
        let center = nalgebra_glm::vec3(
            self.camera.target[0],
            self.camera.target[1],
            self.camera.target[2],
        );
        let up = nalgebra_glm::vec3(0.0, 0.0, 1.0); // Z-up coordinate system
        let view = nalgebra_glm::look_at(&eye, &center, &up);

        let view_proj = proj * view;
        self.view_proj_matrix = view_proj; // Store for axis label projection
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(view_proj.as_slice()),
        );
        let scene_refresh_error = self
            .refresh_scene_view(crate::renderer::renderer::SceneView {
                eye: eye.into(),
                forward: (center - eye).into(),
            })
            .err();
        self.scene_error = scene_refresh_error;
        let scene_ready = self.scene_error.is_none();

        // Helper closure to update material uniform for specific geoset
        // Works with optional model (model_opt): if None, returns defaults
        let update_material_uniform =
            |geoset: &GeosetRenderInfo, layer_index: usize| -> MaterialUniform {
                let (filter_mode, replaceable_id, layer_alpha, shading_flags) =
                    if let Some(mat_id) = geoset.material_id {
                        if let Some(m) = model_opt {
                            if mat_id < m.materials.len() {
                                let material = &m.materials[mat_id];
                                if layer_index < material.layers.len() {
                                    let layer = &material.layers[layer_index];
                                    let rid = if let Some(tex_id) = layer.texture_id {
                                        if tex_id < m.textures.len() {
                                            m.textures[tex_id].replaceable_id
                                        } else {
                                            0
                                        }
                                    } else {
                                        0
                                    };
                                    // Use get_filter_mode() to respect overrides!
                                    let filter_mode = layer.get_filter_mode();
                                    // Use layer methods to get effective values (with overrides)
                                    let flags = layer.get_shading_flags();
                                    let shading_bits = ShadingFlags::get_bits(&flags);
                                    let alpha = layer.get_alpha();

                                    (filter_mode, rid, alpha, shading_bits)
                                } else {
                                    (FilterMode::None, 0, 1.0, 0)
                                }
                            } else {
                                (FilterMode::None, 0, 1.0, 0)
                            }
                        } else {
                            (FilterMode::None, 0, 1.0, 0)
                        }
                    } else {
                        (FilterMode::None, 0, 1.0, 0)
                    };

                MaterialUniform::new(
                    self.team_color,
                    replaceable_id,
                    wireframe_mode,
                    filter_mode,
                    layer_alpha,
                    shading_flags,
                )
            };

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d {
                width: self.config.width,
                height: self.config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        let mut scene_record_error = None;
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: self.skybox_color[0] as f64,
                            g: self.skybox_color[1] as f64,
                            b: self.skybox_color[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            // Set viewport and scissor for full screen (no left panel)
            render_pass.set_viewport(0.0, 0.0, viewport_width, viewport_height, 0.0, 1.0);

            render_pass.set_scissor_rect(0, 0, viewport_width as u32, viewport_height as u32);

            // Draw axes and grid first
            if show_grid {
                render_pass.set_pipeline(&self.line_pipeline);
                render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.line_vertex_buffer.slice(..));
                render_pass.draw(0..self.num_lines, 0..1);
            }

            // Scene state was validated by update_scene. Pipeline resources are built exhaustively.
            let scene_mode = self.scene_prepared.is_some();
            if scene_ready {
                scene_record_error = self.record_scene_pass(&mut render_pass, show_geosets).err();
            }

            // Draw model in two passes: opaque first (with depth write), then transparent (without depth write)
            if let Some(model) = model_opt.filter(|_| !scene_mode) {
                if self.num_indices > 0 {
                    render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                    render_pass
                        .set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

                    // Helper function to check if material uses team glow (ReplaceableID=2)

                    // Helper closure to render a geoset with a specific layer
                    let render_geoset_layer =
                        |render_pass: &mut wgpu::RenderPass,
                         geoset: &GeosetRenderInfo,
                         layer_index: usize| {
                            // Determine texture bind group for this specific layer
                            let texture_bind_group = if let Some(mat_id) = geoset.material_id {
                                if mat_id < model.materials.len() {
                                    let material = &model.materials[mat_id];
                                    if layer_index < material.layers.len() {
                                        let layer = &material.layers[layer_index];
                                        if let Some(tex_id) = layer.texture_id {
                                            if tex_id < self.texture_bind_groups.len() {
                                                &self.texture_bind_groups[tex_id]
                                            } else {
                                                &self.texture_bind_groups[0]
                                            }
                                        } else {
                                            &self.texture_bind_groups[0]
                                        }
                                    } else {
                                        &self.texture_bind_groups[0]
                                    }
                                } else {
                                    &self.texture_bind_groups[0]
                                }
                            } else {
                                &self.texture_bind_groups[0]
                            };

                            // Update material uniform for this specific geoset
                            let material_uniform = update_material_uniform(geoset, layer_index);

                            self.queue.write_buffer(
                                &self.material_buffer,
                                0,
                                bytemuck::cast_slice(&[material_uniform]),
                            );

                            render_pass.set_bind_group(1, texture_bind_group, &[]);
                            render_pass.set_bind_group(2, &self.material_bind_group, &[]);
                            render_pass.draw_indexed(
                                geoset.index_start..(geoset.index_start + geoset.index_count),
                                0,
                                0..1,
                            );
                        };

                    // PASS 1: Render opaque materials with depth write enabled
                    let opaque_pipeline = if wireframe_mode {
                        &self.wireframe_pipeline
                    } else {
                        &self.render_pipeline
                    };
                    render_pass.set_pipeline(opaque_pipeline);

                    for (geoset_idx, geoset) in self.geosets.iter().enumerate() {
                        // Skip if geoset is hidden via UI
                        if geoset_idx < show_geosets.len() && !show_geosets[geoset_idx] {
                            continue;
                        }

                        if let Some(mat_id) = geoset.material_id {
                            if mat_id < model.materials.len() {
                                let material = &model.materials[mat_id];
                                // Render ALL layers, not just first
                                for (layer_idx, layer) in material.layers.iter().enumerate() {
                                    // Skip if layer is disabled in UI
                                    if !layer.is_enabled() {
                                        continue;
                                    }

                                    // Get current filter mode (may be overridden by user)
                                    let current_filter_mode = layer.get_filter_mode();

                                    // Render None and Transparent layers in first pass
                                    // Transparent uses alpha testing (discard in shader), not blending
                                    if current_filter_mode == FilterMode::None
                                        || current_filter_mode == FilterMode::Transparent
                                    {
                                        render_geoset_layer(&mut render_pass, geoset, layer_idx);
                                    }
                                }
                            }
                        }
                    }

                    // PASS 2: Render transparent materials (blend mode: SRC_ALPHA, ONE_MINUS_SRC_ALPHA)
                    let transparent_pipeline = if wireframe_mode {
                        &self.wireframe_transparent_pipeline
                    } else {
                        &self.transparent_pipeline
                    };
                    render_pass.set_pipeline(transparent_pipeline);

                    for (geoset_idx, geoset) in self.geosets.iter().enumerate() {
                        // Skip if geoset is hidden via UI
                        if geoset_idx < show_geosets.len() && !show_geosets[geoset_idx] {
                            continue;
                        }

                        if let Some(mat_id) = geoset.material_id {
                            if mat_id < model.materials.len() {
                                let material = &model.materials[mat_id];
                                // Render ALL layers, not just first
                                for (layer_idx, layer) in material.layers.iter().enumerate() {
                                    // Skip if layer is disabled in UI
                                    if !layer.is_enabled() {
                                        continue;
                                    }

                                    // Get current filter mode (may be overridden by user)
                                    let current_filter_mode = layer.get_filter_mode();

                                    // Render ONLY Blend layers in second pass
                                    // Transparent is rendered in first pass with alpha testing
                                    if current_filter_mode == FilterMode::Blend {
                                        render_geoset_layer(&mut render_pass, geoset, layer_idx);
                                    }
                                }
                            }
                        }
                    }

                    // PASS 3: Render additive materials (blend mode: ONE, ONE)
                    let additive_pipeline = if wireframe_mode {
                        &self.wireframe_additive_pipeline
                    } else {
                        &self.additive_pipeline
                    };
                    render_pass.set_pipeline(additive_pipeline);

                    for (geoset_idx, geoset) in self.geosets.iter().enumerate() {
                        // Skip if geoset is hidden via UI
                        if geoset_idx < show_geosets.len() && !show_geosets[geoset_idx] {
                            continue;
                        }

                        if let Some(mat_id) = geoset.material_id {
                            if mat_id < model.materials.len() {
                                let material = &model.materials[mat_id];
                                // Render ALL layers, not just first
                                for (layer_idx, layer) in material.layers.iter().enumerate() {
                                    // Skip if layer is disabled in UI
                                    if !layer.is_enabled() {
                                        continue;
                                    }

                                    // Get current filter mode (may be overridden by user)
                                    let current_filter_mode = layer.get_filter_mode();

                                    // Render additive layers in third pass
                                    if current_filter_mode == FilterMode::Additive
                                        || current_filter_mode == FilterMode::AddAlpha
                                    {
                                        render_geoset_layer(&mut render_pass, geoset, layer_idx);
                                    }
                                }
                            }
                        }
                    }
                }

                // Draw skeleton on top (only if model is present)
                if model_opt.is_some() && show_skeleton && self.num_skeleton_lines > 0 {
                    render_pass.set_pipeline(&self.line_pipeline);
                    render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, self.skeleton_vertex_buffer.slice(..));
                    render_pass.draw(0..(self.num_skeleton_lines * 2), 0..1);
                }

                // Draw bounding boxes (only if model is present)
                if model_opt.is_some() && show_bounding_box && self.num_bounding_box_lines > 0 {
                    render_pass.set_pipeline(&self.line_pipeline);
                    render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, self.bounding_box_vertex_buffer.slice(..));
                    render_pass.draw(0..(self.num_bounding_box_lines * 2), 0..1);
                }
            }
        }

        // Render egui properly
        for (id, image_delta) in &textures_delta.set {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }

        let screen_desc = screen_descriptor;

        // Update egui buffers before rendering
        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_desc,
        );

        {
            let mut egui_rpass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui render pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                })
                .forget_lifetime(); // This is the key!

            self.egui_renderer
                .render(&mut egui_rpass, &paint_jobs, &screen_desc);
        }
        if scene_record_error.is_some() {
            self.scene_error = scene_record_error;
        }

        for id in &textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    pub(crate) fn record_scene_pass<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        show_geosets: &[bool],
    ) -> Result<(), crate::error::MdlError> {
        let (Some(scene), Some(gpu)) = (&self.scene_prepared, &self.scene_gpu) else {
            return Ok(());
        };
        record_prepared_scene(
            render_pass,
            scene,
            gpu,
            &self.scene_pipelines,
            ScenePassBindings {
                camera: &self.camera_bind_group,
                white_texture: &self
                    .scene_texture_resources
                    .as_ref()
                    .ok_or_else(|| crate::error::MdlError::new("renderer-missing-scene-textures"))?
                    .white_bind_group,
                textures: self
                    .scene_texture_resources
                    .as_ref()
                    .ok_or_else(|| crate::error::MdlError::new("renderer-missing-scene-textures"))?
                    .bind_groups
                    .as_slice(),
                legacy_material: &self.material_bind_group,
                scene_material: &gpu.uniform_bind_group,
            },
            show_geosets,
        )
    }
}
