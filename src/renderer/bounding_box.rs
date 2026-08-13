use crate::model::model::Model;
use crate::renderer::line_vertex::LineVertex;
use crate::renderer::renderer::Renderer;
use wgpu::util::DeviceExt;

impl Renderer {
    pub(crate) fn generate_bounding_box_lines(&mut self, model: &Model) {
        let bbox_color = [1.0, 1.0, 0.0]; // Yellow for bounding box - will be updated from settings
        self.generate_bounding_box_lines_with_color(model, bbox_color);
    }

    pub(crate) fn generate_bounding_box_lines_with_color(
        &mut self,
        model: &Model,
        bbox_color: [f32; 3],
    ) {
        let mut bbox_vertices = Vec::new();

        // Calculate overall model bounding box
        let mut overall_min = [f32::INFINITY, f32::INFINITY, f32::INFINITY];
        let mut overall_max = [f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
        let mut has_valid_bbox = false;

        for geoset in &model.geosets {
            let min = geoset.minimum_extent;
            let max = geoset.maximum_extent;

            // Skip if bounding box is invalid
            if min[0] >= max[0] || min[1] >= max[1] || min[2] >= max[2] {
                continue;
            }

            // Update overall bounds
            for i in 0..3 {
                overall_min[i] = overall_min[i].min(min[i]);
                overall_max[i] = overall_max[i].max(max[i]);
            }
            has_valid_bbox = true;
        }

        if has_valid_bbox {
            // Create wireframe cube for the overall model bounding box
            let vertices = [
                // Bottom face (Z = min[2])
                [overall_min[0], overall_min[1], overall_min[2]], // 0: min corner
                [overall_max[0], overall_min[1], overall_min[2]], // 1: +X
                [overall_max[0], overall_max[1], overall_min[2]], // 2: +X+Y
                [overall_min[0], overall_max[1], overall_min[2]], // 3: +Y
                // Top face (Z = max[2])
                [overall_min[0], overall_min[1], overall_max[2]], // 4: +Z
                [overall_max[0], overall_min[1], overall_max[2]], // 5: +X+Z
                [overall_max[0], overall_max[1], overall_max[2]], // 6: +X+Y+Z (max corner)
                [overall_min[0], overall_max[1], overall_max[2]], // 7: +Y+Z
            ];

            // Bottom face edges
            for i in 0..4 {
                let next = (i + 1) % 4;
                bbox_vertices.push(LineVertex {
                    position: vertices[i],
                    color: bbox_color,
                });
                bbox_vertices.push(LineVertex {
                    position: vertices[next],
                    color: bbox_color,
                });
            }

            // Top face edges
            for i in 4..8 {
                let next = 4 + ((i - 4 + 1) % 4);
                bbox_vertices.push(LineVertex {
                    position: vertices[i],
                    color: bbox_color,
                });
                bbox_vertices.push(LineVertex {
                    position: vertices[next],
                    color: bbox_color,
                });
            }

            // Vertical edges connecting bottom to top
            for i in 0..4 {
                bbox_vertices.push(LineVertex {
                    position: vertices[i],
                    color: bbox_color,
                });
                bbox_vertices.push(LineVertex {
                    position: vertices[i + 4],
                    color: bbox_color,
                });
            }

            let box_size = [
                overall_max[0] - overall_min[0],
                overall_max[1] - overall_min[1],
                overall_max[2] - overall_min[2],
            ];

            println!(
                "Model bounding box: min={:?}, max={:?}, size={:?}",
                overall_min, overall_max, box_size
            );
        }

        if !bbox_vertices.is_empty() {
            self.bounding_box_vertex_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Bounding Box Vertex Buffer"),
                        contents: bytemuck::cast_slice(&bbox_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
            self.num_bounding_box_lines = (bbox_vertices.len() / 2) as u32;
            println!(
                "Generated {} bounding box lines for overall model ({} geosets)",
                self.num_bounding_box_lines,
                model.geosets.len()
            );
        } else {
            self.num_bounding_box_lines = 0;
            println!("No valid bounding boxes found in geosets");
        }
    }
}
