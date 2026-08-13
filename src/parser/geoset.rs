use crate::error::MdlError;
use crate::model::geoset::{Face, Geoset, Normal, TexCoord, Vertex};
use crate::model::model::Model;
use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub fn geoset_parse(file: &mut File, model: &mut Model, geos_size: u32) -> Result<(), MdlError> {
    let start_pos = file.stream_position()?;
    let end_pos = start_pos + geos_size as u64;

    while file.stream_position()? < end_pos {
        let geoset_start = file.stream_position()?;

        // Each geoset has inclusiveSize field
        let inclusive_size = file.read_u32::<LittleEndian>()?;
        let geoset_end = geoset_start + inclusive_size as u64;

        if inclusive_size < 4 || geoset_end > end_pos {
            return Err(MdlError::new("mdx-invalid-geoset-size")
                .with_arg("size", inclusive_size)
                .with_arg("remaining", end_pos - geoset_start));
        }

        let mut geoset = Geoset::default();
        let mut indices = Vec::new();
        let mut tag = [0u8; 4];

        // Read all chunks within this geoset (order not guaranteed)
        while file.stream_position()? < geoset_end {
            // Check if we have at least 4 bytes left for a tag
            if geoset_end - file.stream_position()? < 4 {
                break;
            }

            file.read_exact(&mut tag)?;

            match &tag {
                b"VRTX" => {
                    let count = file.read_u32::<LittleEndian>()? as usize;
                    for _ in 0..count {
                        let x = file.read_f32::<LittleEndian>()?;
                        let y = file.read_f32::<LittleEndian>()?;
                        let z = file.read_f32::<LittleEndian>()?;
                        geoset.vertices.push(Vertex {
                            position: [x, y, z],
                        });
                    }
                }
                b"NRMS" => {
                    let count = file.read_u32::<LittleEndian>()? as usize;
                    for _ in 0..count {
                        let x = file.read_f32::<LittleEndian>()?;
                        let y = file.read_f32::<LittleEndian>()?;
                        let z = file.read_f32::<LittleEndian>()?;
                        geoset.normals.push(Normal { normal: [x, y, z] });
                    }
                }
                b"PTYP" => {
                    let count = file.read_u32::<LittleEndian>()?;
                    file.seek(SeekFrom::Current((count * 4) as i64))?;
                }
                b"PCNT" => {
                    let count = file.read_u32::<LittleEndian>()?;
                    file.seek(SeekFrom::Current((count * 4) as i64))?;
                }
                b"PVTX" => {
                    let count = file.read_u32::<LittleEndian>()? as usize;
                    for _ in 0..count {
                        let index = file.read_u16::<LittleEndian>()?;
                        indices.push(index as u32);
                    }
                }
                b"GNDX" => {
                    // Vertex Groups: byte array mapping each vertex to a matrix group
                    let count = file.read_u32::<LittleEndian>()? as usize;
                    geoset.vertex_groups.reserve(count);
                    for _ in 0..count {
                        geoset.vertex_groups.push(file.read_u8()?);
                    }
                }
                b"MTGC" => {
                    // Matrix Group Counts: how many bones in each group
                    let num_groups = file.read_u32::<LittleEndian>()? as usize;
                    geoset.matrix_groups.reserve(num_groups);
                    for _ in 0..num_groups {
                        let group_size = file.read_u32::<LittleEndian>()? as usize;
                        geoset.matrix_groups.push(Vec::with_capacity(group_size));
                    }
                }
                b"MATS" => {
                    // Matrix Groups Data: bone indices for each group
                    let total_count = file.read_u32::<LittleEndian>()? as usize;
                    let mut indices = Vec::with_capacity(total_count);
                    for _ in 0..total_count {
                        indices.push(file.read_u32::<LittleEndian>()?);
                    }

                    // Distribute indices into matrix_groups
                    let mut idx = 0;
                    let expected: usize = geoset.matrix_groups.iter().map(|g| g.capacity()).sum();
                    if expected != total_count {
                        eprintln!(
                            "[mdlvis-rs] MATS count mismatch: total_count={}, sum(capacity)={}",
                            total_count, expected
                        );
                    }
                    for group in &mut geoset.matrix_groups {
                        let cap = group.capacity();
                        for _ in 0..cap {
                            if idx < indices.len() {
                                group.push(indices[idx]);
                                idx += 1;
                            }
                        }
                    }

                    // After MATS comes MaterialID as a plain long field
                    let material_id = file.read_u32::<LittleEndian>()?;
                    geoset.material_id = Some(material_id as usize);

                    // Skip SelectionGroup
                    let _selection_group = file.read_u32::<LittleEndian>()?;
                    geoset.selection_group = _selection_group as usize;

                    // Read Selectable
                    let selectable = file.read_u32::<LittleEndian>()?;
                    geoset.unselectable = selectable == 4; // 4 = Unselectable

                    // Read BoundsRadius
                    geoset.bounds_radius = file.read_f32::<LittleEndian>()?;

                    // Read MinExt (3 floats)
                    geoset.minimum_extent[0] = file.read_f32::<LittleEndian>()?;
                    geoset.minimum_extent[1] = file.read_f32::<LittleEndian>()?;
                    geoset.minimum_extent[2] = file.read_f32::<LittleEndian>()?;

                    // Read MaxExt (3 floats)
                    geoset.maximum_extent[0] = file.read_f32::<LittleEndian>()?;
                    geoset.maximum_extent[1] = file.read_f32::<LittleEndian>()?;
                    geoset.maximum_extent[2] = file.read_f32::<LittleEndian>()?;
                    // Skip nanim and animations array
                    let nanim = file.read_u32::<LittleEndian>()?;
                    file.seek(SeekFrom::Current((nanim * 28) as i64))?; // Each animation is 7 floats
                }
                b"UVAS" => {
                    ensure_remaining(file, geoset_end, 4, "mdx-truncated-uv-set")?;
                    let uvas_count = file.read_u32::<LittleEndian>()? as usize;
                    let remaining = geoset_end.saturating_sub(file.stream_position()?);
                    if uvas_count > (remaining / 8) as usize {
                        return Err(MdlError::new("mdx-invalid-uv-set-count")
                            .with_arg("count", uvas_count)
                            .with_arg("remaining", remaining));
                    }
                    geoset.tex_coord_sets.reserve(uvas_count);
                    for set_index in 0..uvas_count {
                        ensure_remaining(file, geoset_end, 8, "mdx-truncated-uv-set")?;
                        file.read_exact(&mut tag)?;
                        if &tag != b"UVBS" {
                            return Err(
                                MdlError::new("mdx-invalid-uv-set-tag").with_arg("set", set_index)
                            );
                        }
                        let uvbs_count = file.read_u32::<LittleEndian>()? as usize;
                        let byte_count = uvbs_count.checked_mul(8).ok_or_else(|| {
                            MdlError::new("mdx-invalid-uv-set-size")
                                .with_arg("set", set_index)
                                .with_arg("actual", uvbs_count)
                        })?;
                        ensure_remaining(
                            file,
                            geoset_end,
                            byte_count as u64,
                            "mdx-truncated-uv-set",
                        )?;
                        let mut tex_coords = Vec::with_capacity(uvbs_count);
                        for _ in 0..uvbs_count {
                            let u = file.read_f32::<LittleEndian>()?;
                            let v = file.read_f32::<LittleEndian>()?;
                            tex_coords.push(TexCoord { uv: [u, v] });
                        }
                        geoset.tex_coord_sets.push(tex_coords);
                    }
                }
                _ => {
                    // Not a known chunk tag - could be materialId, selectionGroup, etc.
                    // Treat as u32 value
                    file.seek(SeekFrom::Current(-4))?; // Go back
                    let _val = file.read_u32::<LittleEndian>()?;
                }
            }
        }

        for (set_index, tex_coords) in geoset.tex_coord_sets.iter().enumerate() {
            if tex_coords.len() != geoset.vertices.len() {
                return Err(MdlError::new("mdx-invalid-uv-set-size")
                    .with_arg("set", set_index)
                    .with_arg("expected", geoset.vertices.len())
                    .with_arg("actual", tex_coords.len()));
            }
        }

        // Group indices into triangles
        for chunk in indices.chunks(3) {
            if chunk.len() == 3 {
                geoset.faces.push(Face {
                    vertices: [chunk[0], chunk[1], chunk[2]],
                });
            }
        }

        if !geoset.vertices.is_empty() {
            println!(
                "  Geoset {}: {} vertices, {} faces, {} vertex groups, {} matrix groups",
                model.geosets.len(),
                geoset.vertices.len(),
                geoset.faces.len(),
                geoset.vertex_groups.len(),
                geoset.matrix_groups.len()
            );
            model.geosets.push(geoset);
        }

        // Seek to end of geoset using inclusiveSize
        file.seek(SeekFrom::Start(geoset_start + inclusive_size as u64))?;
    }

    Ok(())
}

fn ensure_remaining(
    file: &mut File,
    end: u64,
    required: u64,
    key: &'static str,
) -> Result<(), MdlError> {
    let position = file.stream_position()?;
    let remaining = end.saturating_sub(position);
    if required > remaining {
        return Err(MdlError::new(key)
            .with_arg("required", required)
            .with_arg("remaining", remaining));
    }
    Ok(())
}
