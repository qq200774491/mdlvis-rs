use crate::app::app::{App, get_global_handler_mut};
use crate::texture::loader::{TextureLoadResult, decode_blp, load_from_file, load_texture};
use crate::texture::manager::TextureStatus;

impl App {
    pub(crate) fn start_texture_load(&mut self, texture_id: usize) {
        let handler = get_global_handler_mut().unwrap();

        if let Some(texture_info) = handler.texture_manager.get_texture(texture_id) {
            // Skip RID textures - they are generated, not loaded
            if texture_info.replaceable_id > 0 {
                println!(
                    "跳过纹理 {}：RID {} 纹理由程序生成，无需加载",
                    texture_id, texture_info.replaceable_id
                );
                return;
            }

            let filename = texture_info.filename.clone();
            let local_path = texture_info.local_path.clone();
            let sender = handler.texture_sender.clone();

            // Update status to loading
            if let Some(info) = handler.texture_manager.get_texture_mut(texture_id) {
                info.status = if local_path.is_some() {
                    TextureStatus::LoadingLocal
                } else {
                    TextureStatus::LoadingRemote
                };
                info.progress = 0.0;
            }

            // Spawn background task using runtime handle
            tokio::spawn(async move {
                println!("🔥正在加载纹理 {}：{}", texture_id, filename);

                let result = if let Some(path) = local_path {
                    // Try local first
                    match load_from_file(&path).await {
                        Ok(data) => decode_blp(&data),
                        Err(local_err) => {
                            println!("本地加载失败（{}），正在尝试远程下载", local_err);
                            load_texture(&filename).await
                        }
                    }
                } else {
                    // Load from remote
                    load_texture(&filename).await
                };

                match result {
                    Ok((rgba_data, width, height)) => {
                        let _ = sender.send(TextureLoadResult::Success {
                            texture_id,
                            rgba_data,
                            width,
                            height,
                        });
                    }
                    Err(e) => {
                        let _ = sender.send(TextureLoadResult::Error {
                            texture_id,
                            error: e.to_string(),
                        });
                    }
                }
            });
        }
    }
}
