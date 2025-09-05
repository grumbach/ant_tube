mod server;

use server::{DEFAULT_ENVIRONMENT, ENVIRONMENTS};
use server::Server;

use eframe::egui;
use tokio::sync::mpsc;
use bytes::Bytes;

#[derive(Debug, Clone)]
struct StreamStatus {
    is_streaming: bool,
    error_message: Option<String>,
    total_bytes_received: usize,
    chunks_received: usize,
}

enum StreamEvent {
    ServerConnected,
    ChunkReceived { data: Bytes },
    StreamComplete,
    StreamError { error: String },
}

struct AntubeApp {
    server: Option<Server>,
    address_input: String,
    selected_env: String,
    is_connecting: bool,
    stream_status: StreamStatus,
    stream_receiver: Option<mpsc::UnboundedReceiver<StreamEvent>>,
    video_data: Vec<u8>,
    video_temp_path: Option<std::path::PathBuf>,
    is_playing: bool,
}

impl Default for AntubeApp {
    fn default() -> Self {
        Self {
            server: None,
            address_input: String::new(),
            selected_env: DEFAULT_ENVIRONMENT.to_string(),
            is_connecting: false,
            stream_status: StreamStatus {
                is_streaming: false,
                error_message: None,
                total_bytes_received: 0,
                chunks_received: 0,
            },
            stream_receiver: None,
            video_data: Vec::new(),
            video_temp_path: None,
            is_playing: false,
        }
    }
}

impl eframe::App for AntubeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request continuous repaints while streaming
        if self.stream_status.is_streaming || self.is_connecting {
            ctx.request_repaint();
        }

        // Initialize server if needed (lazy initialization)
        if self.server.is_none() && !self.is_connecting && !self.address_input.trim().is_empty() {
            // Only connect when user actually wants to stream
        }

        // Process stream events
        if let Some(receiver) = &mut self.stream_receiver {
            while let Ok(event) = receiver.try_recv() {
                match event {
                    StreamEvent::ServerConnected => {
                        self.is_connecting = false;
                        self.stream_status.is_streaming = true;
                    }
                    StreamEvent::ChunkReceived { data } => {
                        self.stream_status.chunks_received += 1;
                        self.stream_status.total_bytes_received += data.len();
                        self.video_data.extend_from_slice(&data);
                    }
                    StreamEvent::StreamComplete => {
                        self.stream_status.is_streaming = false;
                        self.is_connecting = false;
                    }
                    StreamEvent::StreamError { error } => {
                        self.stream_status.is_streaming = false;
                        self.is_connecting = false;
                        self.stream_status.error_message = Some(error);
                    }
                }
            }
        }

        // Main UI with scroll area to ensure everything fits
        egui::CentralPanel::default().show(ctx, |ui| {
            let available_height = ui.available_height();
            
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);

                // Address input with controls in one row
                ui.horizontal(|ui| {
                    ui.label("Address:");
                    let response = ui.add_sized(
                        [300.0, 22.0],
                        egui::TextEdit::singleline(&mut self.address_input)
                            .hint_text("Enter video address...")
                    );
                    
                    // Environment selector
                    egui::ComboBox::from_label("Env")
                        .selected_text(&self.selected_env)
                        .show_ui(ui, |ui| {
                            for env in ENVIRONMENTS {
                                ui.selectable_value(&mut self.selected_env, env.to_string(), env);
                            }
                        });
                    
                    if ui.button("Stream").clicked() && !self.address_input.trim().is_empty() {
                        self.connect_and_stream();
                    }
                    
                    ui.add_space(15.0);
                    
                    // Only keep essential action buttons
                    let download_enabled = !self.video_data.is_empty();
                    ui.add_enabled_ui(download_enabled, |ui| {
                        if ui.small_button("💾").clicked() {
                            self.save_video_data();
                        }
                    });
                    
                    let play_enabled = !self.video_data.is_empty();
                    ui.add_enabled_ui(play_enabled, |ui| {
                        let button_text = if self.is_playing { "⏸" } else { "▶" };
                        if ui.small_button(button_text).clicked() {
                            self.toggle_play_pause();
                        }
                    });
                    
                    // Show status inline on the same row
                    ui.add_space(20.0);
                    if self.is_connecting {
                        ui.add(egui::Spinner::new().size(10.0));
                        ui.label(egui::RichText::new("Connecting...").size(10.0).color(egui::Color32::YELLOW));
                    } else if self.stream_status.is_streaming {
                        ui.add(egui::Spinner::new().size(10.0));
                        ui.label(egui::RichText::new(format!("{}", self.format_file_size(self.stream_status.total_bytes_received))).size(10.0).color(egui::Color32::GREEN));
                    } else if let Some(_error) = &self.stream_status.error_message {
                        ui.label(egui::RichText::new("Error").size(10.0).color(egui::Color32::RED));
                    } else if self.stream_status.total_bytes_received > 0 {
                        ui.label(egui::RichText::new(format!("{}", self.format_file_size(self.stream_status.total_bytes_received))).size(10.0).color(egui::Color32::LIGHT_BLUE));
                    }
                    
                    // Auto-focus on startup
                    if self.address_input.is_empty() {
                        response.request_focus();
                    }
                });
                
                ui.add_space(5.0);
                
                // Player screen - maximize to fill remaining space
                let remaining_height = available_height - 50.0; // Minimal space for top controls
                self.show_player_screen_full(ui, remaining_height.max(400.0));
            });
        });
    }
}

impl AntubeApp {
    fn connect_and_stream(&mut self) {
        // Clear previous data and set connecting state
        self.video_data.clear();
        self.is_connecting = true;
        self.stream_status = StreamStatus {
            is_streaming: false,
            error_message: None,
            total_bytes_received: 0,
            chunks_received: 0,
        };
        
        let address = self.address_input.clone();
        let (server_tx, mut server_rx) = mpsc::unbounded_channel();
        
        // Spawn server initialization task
        let env = self.selected_env.clone();
        tokio::spawn(async move {
            let result = Server::new(&env).await;
            let _ = server_tx.send(result);
        });
        
        let (stream_tx, stream_rx) = mpsc::unbounded_channel();
        self.stream_receiver = Some(stream_rx);
        
        // Spawn the connection and streaming task
        tokio::spawn(async move {
            // Wait for server to initialize
            match server_rx.recv().await {
                Some(Ok(server)) => {
                    let _ = stream_tx.send(StreamEvent::ServerConnected);
                    
                    // Start streaming
                    match server.stream_data(&address).await {
                        Ok(stream) => {
                            for chunk_result in stream {
                                match chunk_result {
                                    Ok(chunk) => {
                                        if stream_tx.send(StreamEvent::ChunkReceived { data: chunk }).is_err() {
                                            break;
                                        }
                                    }
                                    Err(error) => {
                                        let _ = stream_tx.send(StreamEvent::StreamError { error });
                                        return;
                                    }
                                }
                            }
                            let _ = stream_tx.send(StreamEvent::StreamComplete);
                        }
                        Err(error) => {
                            let _ = stream_tx.send(StreamEvent::StreamError { error });
                        }
                    }
                }
                Some(Err(error)) => {
                    let _ = stream_tx.send(StreamEvent::StreamError { error });
                }
                None => {
                    let _ = stream_tx.send(StreamEvent::StreamError { 
                        error: "Server initialization failed".to_string() 
                    });
                }
            }
        });
    }
    
    fn show_player_screen_full(&mut self, ui: &mut egui::Ui, max_height: f32) {
        // Use all available space - player goes to bottom of window
        let available_width = ui.available_width();
        let player_height = max_height;
        let player_size = egui::Vec2::new(available_width, player_height);
        
        ui.allocate_ui_with_layout(
            player_size,
            egui::Layout::centered_and_justified(egui::Direction::TopDown),
            |ui| {
                let (_rect, response) = ui.allocate_exact_size(player_size, egui::Sense::click());
                
                // Draw the player background - fill entire area
                ui.painter().rect_filled(
                    response.rect,
                    egui::Rounding::same(4.0),
                    egui::Color32::from_rgb(10, 10, 10)
                );
                
                // Draw subtle border
                ui.painter().rect_stroke(
                    response.rect,
                    egui::Rounding::same(4.0),
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 40, 40))
                );
                
                // Content inside the player
                let content_rect = response.rect.shrink(20.0);
                ui.allocate_ui_at_rect(content_rect, |ui| {
                    ui.centered_and_justified(|ui| {
                        if self.stream_status.is_streaming {
                            ui.vertical_centered(|ui| {
                                ui.add(egui::Spinner::new().size(40.0));
                                ui.add_space(15.0);
                                ui.label(egui::RichText::new("Streaming video...")
                                    .color(egui::Color32::WHITE)
                                    .size(24.0));
                                if self.stream_status.total_bytes_received > 0 {
                                    ui.add_space(5.0);
                                    ui.label(egui::RichText::new(format!("{} received", 
                                        self.format_file_size(self.stream_status.total_bytes_received)))
                                        .color(egui::Color32::GRAY)
                                        .size(16.0));
                                }
                            });
                        } else if !self.video_data.is_empty() {
                            ui.vertical_centered(|ui| {
                                let status_text = if self.is_playing { "▶ Playing" } else { "⏸ Ready to Play" };
                                ui.label(egui::RichText::new("🎬")
                                    .color(egui::Color32::WHITE)
                                    .size(80.0));
                                ui.add_space(15.0);
                                ui.label(egui::RichText::new(status_text)
                                    .color(egui::Color32::WHITE)
                                    .size(28.0));
                                ui.add_space(10.0);
                                ui.label(egui::RichText::new(format!("File size: {}", 
                                    self.format_file_size(self.video_data.len())))
                                    .color(egui::Color32::GRAY)
                                    .size(18.0));
                            });
                        } else if self.is_connecting {
                            ui.vertical_centered(|ui| {
                                ui.add(egui::Spinner::new().size(30.0));
                                ui.add_space(15.0);
                                ui.label(egui::RichText::new("Connecting to network...")
                                    .color(egui::Color32::YELLOW)
                                    .size(20.0));
                            });
                        } else {
                            ui.vertical_centered(|ui| {
                                ui.label(egui::RichText::new("📺")
                                    .color(egui::Color32::GRAY)
                                    .size(80.0));
                                ui.add_space(15.0);
                                ui.label(egui::RichText::new("AnTube Player")
                                    .color(egui::Color32::GRAY)
                                    .size(28.0));
                                ui.add_space(10.0);
                                ui.label(egui::RichText::new("Enter a video address above to start streaming")
                                    .color(egui::Color32::DARK_GRAY)
                                    .size(16.0));
                            });
                        }
                    });
                });
            }
        );
    }
    
    fn toggle_play_pause(&mut self) {
        if !self.video_data.is_empty() {
            self.is_playing = !self.is_playing;
            
            if self.is_playing {
                // Create temp file and play
                self.play_in_system_player();
            } else {
                // For now, we can't actually pause system player
                // This is a limitation of the current approach
                println!("Pause requested (system player control limited)");
            }
        }
    }
    
    fn play_in_system_player(&mut self) {
        if self.video_data.is_empty() {
            return;
        }
        
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("antube_temp_{}.mp4", 
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()));
        
        match std::fs::write(&temp_file, &self.video_data) {
            Ok(_) => {
                self.video_temp_path = Some(temp_file.clone());
                
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open")
                        .arg(&temp_file)
                        .spawn();
                }
                
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("cmd")
                        .args(["/C", "start", "", &temp_file.to_string_lossy()])
                        .spawn();
                }
                
                #[cfg(target_os = "linux")]
                {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(&temp_file)
                        .spawn();
                }
            }
            Err(e) => {
                println!("Failed to create temp file: {}", e);
            }
        }
    }
    
    fn save_video_data(&self) {
        if self.video_data.is_empty() {
            return;
        }
        
        let address_short = if self.address_input.len() > 8 {
            &self.address_input[..8]
        } else {
            &self.address_input
        };
        let suggested_filename = format!("antube_{}.mp4", address_short);
        
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Video Files", &["mp4", "mov", "avi", "mkv", "webm"])
            .add_filter("All Files", &["*"])
            .set_file_name(&suggested_filename)
            .set_title("Save Downloaded Video")
            .save_file() 
        {
            match std::fs::write(&path, &self.video_data) {
                Ok(_) => {
                    println!("✅ Video saved to: {}", path.display());
                }
                Err(e) => {
                    println!("❌ Failed to save: {}", e);
                }
            }
        }
    }
    
    fn format_file_size(&self, bytes: usize) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
        let mut size = bytes as f64;
        let mut unit_index = 0;
        
        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }
        
        if unit_index == 0 {
            format!("{} {}", bytes, UNITS[unit_index])
        } else {
            format!("{:.1} {}", size, UNITS[unit_index])
        }
    }
}

#[tokio::main]
async fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AnTube - Autonomi Video Streamer")
            .with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "AnTube",
        options,
        Box::new(|_cc| Box::new(AntubeApp::default())),
    )
}
