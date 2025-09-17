mod server;
mod video_streamer;

use server::Server;
use server::{DEFAULT_ENVIRONMENT, ENVIRONMENTS};
use video_streamer::VideoStreamer;

use clap::Parser;
use eframe::egui;
use tokio::sync::mpsc;

#[derive(Parser, Debug)]
#[command(name = "antube")]
#[command(about = "AnTube - Autonomi Video Streamer")]
struct Args {
    /// Network environment (local, autonomi, alpha)
    #[arg(short, long, default_value = "autonomi")]
    network: String,

    /// Data address to stream
    #[arg(short, long)]
    address: Option<String>,

    /// Use default test video (only works with local network)
    #[arg(long)]
    test: bool,
}

#[derive(Debug, Clone)]
struct StreamStatus {
    is_streaming: bool,
    error_message: Option<String>,
    total_bytes_received: usize,
    chunks_received: usize,
}

enum StreamEvent {
    ServerConnected,
    ChunkReceived { size: usize },
    StreamComplete,
    StreamError { error: String },
    VideoStreamerReady,
}

struct AntubeApp {
    server: Option<Server>,
    address_input: String,
    selected_env: String,
    is_connecting: bool,
    stream_status: StreamStatus,
    stream_receiver: Option<mpsc::UnboundedReceiver<StreamEvent>>,
    video_streamer: Option<VideoStreamer>,
    is_playing: bool,
    current_memory_usage: usize,
}

impl AntubeApp {
    fn new(args: Args) -> Self {
        // Use test address if --test flag is provided
        let address = if args.test {
            if args.network == "local" {
                "d8949e2bd7bc0f60d6062510b4f98c9fd92a3bd70567ab9e43f79eb9f8aa24e6".to_string()
            } else {
                println!("Warning: --test flag only works with local network");
                args.address.unwrap_or_default()
            }
        } else {
            args.address.unwrap_or_default()
        };

        let mut app = Self {
            server: None,
            address_input: address,
            selected_env: args.network,
            is_connecting: false,
            stream_status: StreamStatus {
                is_streaming: false,
                error_message: None,
                total_bytes_received: 0,
                chunks_received: 0,
            },
            stream_receiver: None,
            video_streamer: None,
            is_playing: false,
            current_memory_usage: 0,
        };

        // Auto-start streaming if address was provided or test flag used
        if !app.address_input.is_empty() {
            app.connect_and_stream();
        }

        app
    }
}

impl Default for AntubeApp {
    fn default() -> Self {
        Self::new(Args {
            network: DEFAULT_ENVIRONMENT.to_string(),
            address: None,
            test: false,
        })
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
                    StreamEvent::ChunkReceived { size } => {
                        self.stream_status.chunks_received += 1;
                        self.stream_status.total_bytes_received += size;
                        if let Some(streamer) = &self.video_streamer {
                            self.current_memory_usage = streamer.get_memory_usage();
                        }
                    }
                    StreamEvent::VideoStreamerReady => {
                        self.is_playing = true;
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
                            .hint_text("Enter video address..."),
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

                    // Show stream status instead of file controls
                    if self.is_playing {
                        let _ = ui.small_button("🎬");
                        ui.label(
                            egui::RichText::new("Playing")
                                .size(10.0)
                                .color(egui::Color32::GREEN),
                        );
                    } else if self.video_streamer.is_some() {
                        let _ = ui.small_button("📹");
                        ui.label(
                            egui::RichText::new("Ready")
                                .size(10.0)
                                .color(egui::Color32::BLUE),
                        );
                    }

                    // Show status inline on the same row
                    ui.add_space(20.0);
                    if self.is_connecting {
                        ui.add(egui::Spinner::new().size(10.0));
                        ui.label(
                            egui::RichText::new("Connecting...")
                                .size(10.0)
                                .color(egui::Color32::YELLOW),
                        );
                    } else if self.stream_status.is_streaming {
                        ui.add(egui::Spinner::new().size(10.0));
                        ui.label(
                            egui::RichText::new(
                                self.format_file_size(self.stream_status.total_bytes_received)
                                    .to_string(),
                            )
                            .size(10.0)
                            .color(egui::Color32::GREEN),
                        );
                    } else if let Some(_error) = &self.stream_status.error_message {
                        ui.label(
                            egui::RichText::new("Error")
                                .size(10.0)
                                .color(egui::Color32::RED),
                        );
                    } else if self.stream_status.total_bytes_received > 0 {
                        let memory_text =
                            format!("Mem: {}", self.format_file_size(self.current_memory_usage));
                        ui.label(
                            egui::RichText::new(memory_text)
                                .size(10.0)
                                .color(egui::Color32::LIGHT_BLUE),
                        );
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
        // Clear previous state
        self.current_memory_usage = 0;
        self.video_streamer = None;
        self.is_connecting = true;
        self.stream_status = StreamStatus {
            is_streaming: false,
            error_message: None,
            total_bytes_received: 0,
            chunks_received: 0,
        };

        let address = self.address_input.clone();
        let (server_tx, server_rx) = mpsc::unbounded_channel();

        // Spawn server initialization task
        let env = self.selected_env.clone();
        tokio::spawn(async move {
            let result = Server::new(&env).await;
            let _ = server_tx.send(result);
        });

        let (stream_tx, stream_rx) = mpsc::unbounded_channel();
        self.stream_receiver = Some(stream_rx);

        tokio::spawn(Self::run_streaming_task(server_rx, stream_tx, address));
    }

    async fn run_streaming_task(
        server_rx: mpsc::UnboundedReceiver<Result<Server, String>>,
        stream_tx: mpsc::UnboundedSender<StreamEvent>,
        address: String,
    ) {
        let server = match Self::wait_for_server(server_rx, &stream_tx).await {
            Some(server) => server,
            None => return,
        };

        let video_streamer = match Self::create_video_streamer(&stream_tx) {
            Some(streamer) => streamer,
            None => return,
        };

        Self::stream_video_data(server, video_streamer, address, stream_tx).await;
    }

    async fn wait_for_server(
        mut server_rx: mpsc::UnboundedReceiver<Result<Server, String>>,
        stream_tx: &mpsc::UnboundedSender<StreamEvent>,
    ) -> Option<Server> {
        match server_rx.recv().await {
            Some(Ok(server)) => {
                let _ = stream_tx.send(StreamEvent::ServerConnected);
                Some(server)
            }
            Some(Err(error)) => {
                let _ = stream_tx.send(StreamEvent::StreamError { error });
                None
            }
            None => {
                let _ = stream_tx.send(StreamEvent::StreamError {
                    error: "Server initialization failed".to_string(),
                });
                None
            }
        }
    }

    fn create_video_streamer(
        stream_tx: &mpsc::UnboundedSender<StreamEvent>,
    ) -> Option<VideoStreamer> {
        match VideoStreamer::new() {
            Ok(video_streamer) => {
                let _ = stream_tx.send(StreamEvent::VideoStreamerReady);
                Some(video_streamer)
            }
            Err(e) => {
                let _ = stream_tx.send(StreamEvent::StreamError {
                    error: format!("Failed to create video streamer: {e}"),
                });
                None
            }
        }
    }

    async fn stream_video_data(
        server: Server,
        _video_streamer: VideoStreamer, // We'll create a new one after prebuffering
        address: String,
        stream_tx: mpsc::UnboundedSender<StreamEvent>,
    ) {
        let stream = match server.stream_data(&address).await {
            Ok(stream) => stream,
            Err(error) => {
                let _ = stream_tx.send(StreamEvent::StreamError { error });
                return;
            }
        };

        if let Err(e) = Self::process_stream_with_delayed_pipeline(stream, &stream_tx) {
            let _ = stream_tx.send(StreamEvent::StreamError { error: e });
            return;
        }

        let _ = stream_tx.send(StreamEvent::StreamComplete);
    }

    fn process_stream_chunks(
        stream: impl Iterator<Item = Result<bytes::Bytes, String>>,
        video_streamer: &VideoStreamer,
        stream_tx: &mpsc::UnboundedSender<StreamEvent>,
    ) -> Result<(), String> {
        let mut buffer = Vec::new();
        let mut chunk_count = 0;
        const INITIAL_BUFFER_CHUNKS: usize = 3; // Buffer first 3 chunks for better format detection

        for chunk_result in stream {
            let chunk = chunk_result?;
            chunk_count += 1;

            Self::save_chunk_for_testing(&chunk);

            // Buffer initial chunks to give decodebin enough data for format detection
            if chunk_count <= INITIAL_BUFFER_CHUNKS {
                buffer.extend_from_slice(&chunk);
                println!(
                    "Buffering chunk {} of {} bytes (total buffered: {} bytes)",
                    chunk_count,
                    chunk.len(),
                    buffer.len()
                );

                // Send the buffered data as one large chunk after collecting initial chunks
                if chunk_count == INITIAL_BUFFER_CHUNKS {
                    println!(
                        "Pushing initial buffer of {} bytes to help format detection",
                        buffer.len()
                    );
                    Self::push_chunk_to_streamer(&buffer, video_streamer)?;
                    buffer.clear();
                }
            } else {
                // After initial buffering, stream chunks normally
                Self::push_chunk_to_streamer(&chunk, video_streamer)?;
            }

            if stream_tx
                .send(StreamEvent::ChunkReceived { size: chunk.len() })
                .is_err()
            {
                break;
            }
        }

        // Handle case where we have less than INITIAL_BUFFER_CHUNKS
        if !buffer.is_empty() {
            println!("Pushing remaining buffer of {} bytes", buffer.len());
            Self::push_chunk_to_streamer(&buffer, video_streamer)?;
        }

        Ok(())
    }

    fn process_stream_with_prebuffering(
        stream: impl Iterator<Item = Result<bytes::Bytes, String>>,
        video_streamer: &VideoStreamer,
        stream_tx: &mpsc::UnboundedSender<StreamEvent>,
    ) -> Result<(), String> {
        let mut buffer = Vec::new();
        let mut chunk_count = 0;
        const PREBUFFER_SIZE: usize = 40 * 1024 * 1024; // 40MB
        let mut playback_started = false;

        println!(
            "Starting prebuffering - will collect {}MB before starting playback",
            PREBUFFER_SIZE / (1024 * 1024)
        );

        for chunk_result in stream {
            let chunk = chunk_result?;
            chunk_count += 1;

            Self::save_chunk_for_testing(&chunk);

            if !playback_started {
                // Accumulate data until we have enough for reliable format detection
                buffer.extend_from_slice(&chunk);
                println!(
                    "Prebuffering: collected chunk {} ({} bytes), total: {}MB",
                    chunk_count,
                    chunk.len(),
                    buffer.len() / (1024 * 1024)
                );

                if buffer.len() >= PREBUFFER_SIZE {
                    println!(
                        "✅ Prebuffering complete! Starting video playback with {}MB of data",
                        buffer.len() / (1024 * 1024)
                    );

                    // Send all buffered data at once
                    Self::push_chunk_to_streamer(&buffer, video_streamer)?;
                    playback_started = true;
                    buffer.clear(); // Free memory
                }
            } else {
                // After prebuffering, stream chunks normally for continuous playback
                Self::push_chunk_to_streamer(&chunk, video_streamer)?;
            }

            if stream_tx
                .send(StreamEvent::ChunkReceived { size: chunk.len() })
                .is_err()
            {
                break;
            }
        }

        // Handle case where total file size is less than PREBUFFER_SIZE
        if !playback_started && !buffer.is_empty() {
            println!(
                "File smaller than prebuffer size - starting playback with {}MB",
                buffer.len() / (1024 * 1024)
            );
            Self::push_chunk_to_streamer(&buffer, video_streamer)?;
        }

        Ok(())
    }

    fn process_stream_with_delayed_pipeline(
        stream: impl Iterator<Item = Result<bytes::Bytes, String>>,
        stream_tx: &mpsc::UnboundedSender<StreamEvent>,
    ) -> Result<(), String> {
        let mut buffer = Vec::new();
        let mut chunk_count = 0;
        const PREBUFFER_SIZE: usize = 40 * 1024 * 1024; // 40MB

        println!(
            "Starting prebuffering - will collect {}MB before creating video pipeline",
            PREBUFFER_SIZE / (1024 * 1024)
        );

        // First phase: collect all data
        for chunk_result in stream {
            let chunk = chunk_result?;
            chunk_count += 1;

            Self::save_chunk_for_testing(&chunk);
            buffer.extend_from_slice(&chunk);

            println!(
                "Prebuffering: collected chunk {} ({} bytes), total: {}MB",
                chunk_count,
                chunk.len(),
                buffer.len() / (1024 * 1024)
            );

            if stream_tx
                .send(StreamEvent::ChunkReceived { size: chunk.len() })
                .is_err()
            {
                break;
            }
        }

        println!("✅ Data collection complete! Creating video pipeline and starting playback with {}MB of data",
                buffer.len() / (1024 * 1024));

        // Second phase: create pipeline and start playback
        let video_streamer =
            VideoStreamer::new().map_err(|e| format!("Failed to create video streamer: {}", e))?;

        println!("VideoStreamer created, pushing complete buffer");
        Self::push_chunk_to_streamer(&buffer, &video_streamer)?;

        println!("Buffer pushed, signaling end of stream");
        if let Err(e) = video_streamer.signal_end_of_stream() {
            return Err(format!("Failed to signal end of stream: {e}"));
        }

        println!("End of stream signaled successfully");

        // Keep the pipeline alive for playback
        println!("Keeping pipeline alive for playback - press Ctrl+C to exit");
        std::thread::sleep(std::time::Duration::from_secs(120)); // Keep alive for 2 minutes

        Ok(())
    }

    fn save_chunk_for_testing(chunk: &[u8]) {
        use std::fs::File;
        use std::io::Write;
        use std::sync::Mutex;

        static COMPLETE_FILE: Mutex<Option<File>> = Mutex::new(None);

        let mut file_guard = match COMPLETE_FILE.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        if file_guard.is_none() {
            match File::create("/tmp/antube_complete.mp4") {
                Ok(file) => {
                    *file_guard = Some(file);
                    println!("Created complete MP4 file for testing");
                }
                Err(e) => {
                    println!("Failed to create complete file: {e}");
                    return;
                }
            }
        }

        if let Some(ref mut file) = *file_guard {
            if let Err(e) = file.write_all(chunk) {
                println!("Failed to write chunk to complete file: {e}");
            }
            let _ = file.flush();
        }
    }

    fn push_chunk_to_streamer(chunk: &[u8], video_streamer: &VideoStreamer) -> Result<(), String> {
        println!(
            "Received chunk of size: {} bytes, pushing to video streamer",
            chunk.len()
        );

        video_streamer
            .push_chunk(chunk.to_vec())
            .map_err(|e| format!("Failed to push chunk to video streamer: {e}"))?;

        println!("Successfully pushed chunk to video streamer");
        Ok(())
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
                    egui::Color32::from_rgb(10, 10, 10),
                );

                // Draw subtle border
                ui.painter().rect_stroke(
                    response.rect,
                    egui::Rounding::same(4.0),
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 40, 40)),
                );

                // Content inside the player
                let content_rect = response.rect.shrink(20.0);
                ui.allocate_ui_at_rect(content_rect, |ui| {
                    ui.centered_and_justified(|ui| {
                        if self.stream_status.is_streaming {
                            ui.vertical_centered(|ui| {
                                ui.add(egui::Spinner::new().size(40.0));
                                ui.add_space(15.0);
                                ui.label(
                                    egui::RichText::new("Streaming video...")
                                        .color(egui::Color32::WHITE)
                                        .size(24.0),
                                );
                                if self.stream_status.total_bytes_received > 0 {
                                    ui.add_space(5.0);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} received",
                                            self.format_file_size(
                                                self.stream_status.total_bytes_received
                                            )
                                        ))
                                        .color(egui::Color32::GRAY)
                                        .size(16.0),
                                    );
                                }
                            });
                        } else if self.video_streamer.is_some() && self.current_memory_usage > 0 {
                            ui.vertical_centered(|ui| {
                                let status_text = if self.is_playing {
                                    "▶ Streaming"
                                } else {
                                    "⏸ Ready to Stream"
                                };
                                ui.label(
                                    egui::RichText::new("🎬")
                                        .color(egui::Color32::WHITE)
                                        .size(80.0),
                                );
                                ui.add_space(15.0);
                                ui.label(
                                    egui::RichText::new(status_text)
                                        .color(egui::Color32::WHITE)
                                        .size(28.0),
                                );
                                ui.add_space(10.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Memory usage: {}",
                                        self.format_file_size(self.current_memory_usage)
                                    ))
                                    .color(egui::Color32::GRAY)
                                    .size(18.0),
                                );
                            });
                        } else if self.is_connecting {
                            ui.vertical_centered(|ui| {
                                ui.add(egui::Spinner::new().size(30.0));
                                ui.add_space(15.0);
                                ui.label(
                                    egui::RichText::new("Connecting to network...")
                                        .color(egui::Color32::YELLOW)
                                        .size(20.0),
                                );
                            });
                        } else {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new("📺")
                                        .color(egui::Color32::GRAY)
                                        .size(80.0),
                                );
                                ui.add_space(15.0);
                                ui.label(
                                    egui::RichText::new("AnTube Player")
                                        .color(egui::Color32::GRAY)
                                        .size(28.0),
                                );
                                ui.add_space(10.0);
                                ui.label(
                                    egui::RichText::new(
                                        "Enter a video address above to start streaming",
                                    )
                                    .color(egui::Color32::DARK_GRAY)
                                    .size(16.0),
                                );
                            });
                        }
                    });
                });
            },
        );
    }

    fn toggle_play_pause(&mut self) {
        // Note: Video playback is now handled automatically by GStreamer
        // when chunks are pushed to the VideoStreamer
        if self.video_streamer.is_some() {
            self.is_playing = !self.is_playing;
            println!("Video streaming state toggled: {}", self.is_playing);
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
    let args = Args::parse();

    println!(
        "Starting AnTube with network: {} and address: {:?}",
        args.network, args.address
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AnTube - Autonomi Video Streamer")
            .with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "AnTube",
        options,
        Box::new(|_cc| Box::new(AntubeApp::new(args))),
    )
}
