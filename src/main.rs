mod server;
mod video_streamer;

use server::Server;
use server::{DEFAULT_ENVIRONMENT, ENVIRONMENTS};
use video_streamer::VideoStreamer;

use clap::Parser;
use eframe::egui;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

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

type StreamId = u32;

#[derive(Debug, Clone)]
struct StreamInfo {
    id: StreamId,
    address: String,
    environment: String,
    status: StreamStatus,
    created_at: std::time::Instant,
}

#[derive(Debug, Clone)]
enum StreamStatus {
    Connecting,
    Streaming {
        total_bytes_received: usize,
        chunks_received: usize,
        last_update_time: std::time::Instant,
        total_size: usize,
    },
    Completed {
        total_bytes_received: usize,
        chunks_received: usize,
    },
    Error {
        message: String,
    },
}

enum StreamEvent {
    ServerConnected {
        stream_id: StreamId,
        total_size: usize,
    },
    ChunkReceived {
        stream_id: StreamId,
        size: usize,
    },
    StreamComplete {
        stream_id: StreamId,
        video_streamer: VideoStreamer,
    },
    StreamError {
        stream_id: StreamId,
        error: String,
    },
    VideoStreamerReady {
        stream_id: StreamId,
    },
}

struct AntubeApp {
    address_input: String,
    selected_env: String,
    streams: HashMap<StreamId, StreamInfo>,
    video_streamers: HashMap<StreamId, VideoStreamer>,
    stream_receiver: mpsc::UnboundedReceiver<StreamEvent>,
    stream_sender: mpsc::UnboundedSender<StreamEvent>,
    stream_tasks: HashMap<StreamId, JoinHandle<()>>,
    next_stream_id: StreamId,
}

impl AntubeApp {
    fn new(mut args: Args) -> Self {
        // Use test address if --test flag is provided
        let address = if args.test {
            args.network = "local".to_string();
            "d8949e2bd7bc0f60d6062510b4f98c9fd92a3bd70567ab9e43f79eb9f8aa24e6".to_string()
        } else {
            args.address.unwrap_or_default()
        };

        // Create global event channel for all streams
        let (stream_sender, stream_receiver) = mpsc::unbounded_channel();

        let mut app = Self {
            address_input: address,
            selected_env: args.network,
            streams: HashMap::new(),
            video_streamers: HashMap::new(),
            stream_receiver,
            stream_sender,
            stream_tasks: HashMap::new(),
            next_stream_id: 1,
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
        // Request periodic repaints while any stream is active
        let has_active_streams = self.streams.values().any(|stream| {
            matches!(
                stream.status,
                StreamStatus::Connecting | StreamStatus::Streaming { .. }
            )
        });
        if has_active_streams {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Process stream events
        while let Ok(event) = self.stream_receiver.try_recv() {
            match event {
                StreamEvent::ServerConnected {
                    stream_id,
                    total_size,
                } => {
                    if let Some(stream) = self.streams.get_mut(&stream_id) {
                        stream.status = StreamStatus::Streaming {
                            total_bytes_received: 0,
                            chunks_received: 0,
                            last_update_time: std::time::Instant::now(),
                            total_size,
                        };
                        println!("Stream {stream_id} connected, total size: {total_size} bytes");
                    }
                }
                StreamEvent::ChunkReceived { stream_id, size } => {
                    if let Some(stream) = self.streams.get_mut(&stream_id) {
                        if let StreamStatus::Streaming {
                            total_bytes_received,
                            chunks_received,
                            last_update_time,
                            total_size: _,
                        } = &mut stream.status
                        {
                            *chunks_received += 1;
                            *total_bytes_received += size;

                            *last_update_time = std::time::Instant::now();
                        }
                    }
                }
                StreamEvent::VideoStreamerReady { stream_id } => {
                    println!("Stream {stream_id} video streamer ready");
                }
                StreamEvent::StreamComplete {
                    stream_id,
                    video_streamer,
                } => {
                    if let Some(stream) = self.streams.get_mut(&stream_id) {
                        if let StreamStatus::Streaming {
                            total_bytes_received,
                            chunks_received,
                            ..
                        } = &stream.status
                        {
                            stream.status = StreamStatus::Completed {
                                total_bytes_received: *total_bytes_received,
                                chunks_received: *chunks_received,
                            };
                            // Store the VideoStreamer to keep it alive
                            self.video_streamers.insert(stream_id, video_streamer);
                            println!("Stream {stream_id} completed and VideoStreamer stored");
                        }
                    }
                }
                StreamEvent::StreamError { stream_id, error } => {
                    if let Some(stream) = self.streams.get_mut(&stream_id) {
                        stream.status = StreamStatus::Error { message: error };
                        println!("Stream {stream_id} error");
                    }
                }
            }
        }

        // Clean up completed streaming tasks
        let mut finished_tasks = Vec::new();

        for (stream_id, task) in &self.stream_tasks {
            if task.is_finished() {
                finished_tasks.push(*stream_id);
            }
        }

        for stream_id in finished_tasks {
            println!("Cleaning up finished streaming task {stream_id}");
            self.stream_tasks.remove(&stream_id);
            // Note: Keep VideoStreamer alive even after task finishes - user might still be watching
        }

        // Multiple streams UI with scrollable list
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.add_space(10.0);

                // Address input with controls - always at top
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

                    // Add Stream button
                    if ui.button("Add Stream").clicked() && !self.address_input.trim().is_empty() {
                        self.connect_and_stream();
                    }

                    // Clear All button
                    if !self.streams.is_empty() && ui.button("Clear All").clicked() {
                        self.clear_all_streams();
                    }

                    // Auto-focus on startup
                    if self.address_input.is_empty() {
                        response.request_focus();
                    }
                });

                ui.add_space(15.0);

                // Streams header
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Streams ({})", self.streams.len()))
                            .size(16.0)
                            .strong(),
                    );
                });

                ui.add_space(10.0);

                // Scrollable list of streams
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height() - 20.0)
                    .show(ui, |ui| {
                        if self.streams.is_empty() {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    egui::RichText::new("No streams yet. Enter an address above to start streaming.")
                                        .size(14.0)
                                        .color(egui::Color32::GRAY),
                                );
                            });
                        } else {
                            // Sort streams by creation time (newest first)
                            let mut streams: Vec<_> = self.streams.values().collect();
                            streams.sort_by(|a, b| b.created_at.cmp(&a.created_at));

                            for stream in streams {
                                self.show_stream_item(ui, stream);
                                ui.add_space(8.0);
                            }
                        }
                    });
            });
        });
    }
}

impl AntubeApp {
    fn show_stream_item(&self, ui: &mut egui::Ui, stream: &StreamInfo) {
        egui::Frame::none()
            .fill(egui::Color32::from_gray(30))
            .rounding(4.0)
            .inner_margin(egui::style::Margin::same(8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Status indicator
                    self.show_stream_status_indicator(ui, &stream.status);
                    ui.add_space(8.0);

                    // Stream info
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("#{} {}", stream.id, stream.address))
                                    .size(12.0)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(format!("({})", stream.environment))
                                    .size(10.0)
                                    .color(egui::Color32::GRAY),
                            );
                        });

                        // Status details
                        self.show_stream_status_details(ui, &stream.status);
                    });
                });
            });
    }

    fn show_stream_status_indicator(&self, ui: &mut egui::Ui, status: &StreamStatus) {
        match status {
            StreamStatus::Connecting => {
                ui.add(egui::Spinner::new().size(16.0));
            }
            StreamStatus::Streaming { .. } => {
                ui.add(egui::Spinner::new().size(16.0));
            }
            StreamStatus::Completed { .. } => {
                ui.label(
                    egui::RichText::new("✅")
                        .size(16.0)
                        .color(egui::Color32::WHITE),
                );
            }
            StreamStatus::Error { .. } => {
                ui.label("⚠️");
            }
        }
    }

    fn show_stream_status_details(&self, ui: &mut egui::Ui, status: &StreamStatus) {
        match status {
            StreamStatus::Connecting => {
                ui.label(
                    egui::RichText::new("Connecting to network...")
                        .size(11.0)
                        .color(egui::Color32::YELLOW),
                );
            }
            StreamStatus::Streaming {
                total_bytes_received,
                chunks_received,
                total_size,
                ..
            } => {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Streaming:")
                            .size(11.0)
                            .color(egui::Color32::GREEN),
                    );

                    // Show progress with total size
                    let progress_text = format!(
                        "{}/{} streamed",
                        self.format_data_size(*total_bytes_received),
                        self.format_data_size(*total_size)
                    );

                    ui.label(
                        egui::RichText::new(progress_text)
                            .size(11.0)
                            .color(egui::Color32::WHITE),
                    );

                    // Show detailed chunk processing stats
                    let chunk_stats_text = format!("• {chunks_received} chunks processed");

                    ui.label(
                        egui::RichText::new(chunk_stats_text)
                            .size(11.0)
                            .color(egui::Color32::GRAY),
                    );
                });
            }
            StreamStatus::Completed {
                total_bytes_received,
                chunks_received,
            } => {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Completed:")
                            .size(11.0)
                            .color(egui::Color32::GREEN),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{} • {} chunks",
                            self.format_data_size(*total_bytes_received),
                            chunks_received
                        ))
                        .size(11.0)
                        .color(egui::Color32::WHITE),
                    );
                });
            }
            StreamStatus::Error { message } => {
                ui.label(
                    egui::RichText::new(format!("Error: {}", message))
                        .size(11.0)
                        .color(egui::Color32::RED),
                );
            }
        }
    }

    fn connect_and_stream(&mut self) {
        // Create new stream with unique ID
        let stream_id = self.next_stream_id;
        self.next_stream_id += 1;

        let address = self.address_input.clone();
        let environment = self.selected_env.clone();

        // Create stream info
        let stream_info = StreamInfo {
            id: stream_id,
            address: address.clone(),
            environment: environment.clone(),
            status: StreamStatus::Connecting,
            created_at: std::time::Instant::now(),
        };

        // Add to streams map
        self.streams.insert(stream_id, stream_info);

        // Get the shared sender channel
        let stream_tx = self.stream_sender.clone();

        let (server_tx, server_rx) = mpsc::unbounded_channel();

        // Spawn server initialization task
        tokio::spawn(async move {
            let result = Server::new(&environment).await;
            let _ = server_tx.send(result);
        });

        // Start new streaming task and store handle
        let task = tokio::spawn(Self::run_streaming_task(
            stream_id, server_rx, stream_tx, address,
        ));
        self.stream_tasks.insert(stream_id, task);

        println!(
            "Started stream {} for address {}",
            stream_id, self.address_input
        );

        // Clear input for next stream
        self.address_input.clear();
    }

    async fn run_streaming_task(
        stream_id: StreamId,
        server_rx: mpsc::UnboundedReceiver<Result<Server, String>>,
        stream_tx: mpsc::UnboundedSender<StreamEvent>,
        address: String,
    ) {
        let server = match Self::wait_for_server(stream_id, server_rx, &stream_tx).await {
            Some(server) => server,
            None => return,
        };

        Self::stream_video_data(stream_id, server, address, stream_tx).await;
    }

    async fn wait_for_server(
        stream_id: StreamId,
        mut server_rx: mpsc::UnboundedReceiver<Result<Server, String>>,
        stream_tx: &mpsc::UnboundedSender<StreamEvent>,
    ) -> Option<Server> {
        match server_rx.recv().await {
            Some(Ok(server)) => {
                // We'll send ServerConnected with total_size from stream_video_data after getting DataStream
                Some(server)
            }
            Some(Err(error)) => {
                let _ = stream_tx.send(StreamEvent::StreamError { stream_id, error });
                None
            }
            None => {
                let _ = stream_tx.send(StreamEvent::StreamError {
                    stream_id,
                    error: "Server initialization failed".to_string(),
                });
                None
            }
        }
    }

    async fn stream_video_data(
        stream_id: StreamId,
        server: Server,
        address: String,
        stream_tx: mpsc::UnboundedSender<StreamEvent>,
    ) {
        let data_stream = match server.stream_data(&address).await {
            Ok(stream) => stream,
            Err(error) => {
                let _ = stream_tx.send(StreamEvent::StreamError { stream_id, error });
                return;
            }
        };

        // Get total file size from DataStream
        let total_size = data_stream.data_size() as usize;
        println!(
            "Total file size: {} bytes ({:.1} MB)",
            total_size,
            total_size as f64 / (1024.0 * 1024.0)
        );

        // Send ServerConnected event with total size
        let _ = stream_tx.send(StreamEvent::ServerConnected {
            stream_id,
            total_size,
        });

        // Convert DataStream to iterator for processing
        let stream_iter = data_stream.map(|chunk_result| chunk_result.map_err(|e| e.to_string()));

        if let Err(e) = Self::process_stream_with_delayed_pipeline(
            stream_id,
            stream_iter,
            total_size,
            &stream_tx,
        ) {
            let _ = stream_tx.send(StreamEvent::StreamError {
                stream_id,
                error: e,
            });
        }

        // StreamComplete will be sent from process_stream_with_delayed_pipeline with VideoStreamer
    }

    fn process_stream_with_delayed_pipeline(
        stream_id: StreamId,
        stream: impl Iterator<Item = Result<bytes::Bytes, String>>,
        _total_size: usize,
        stream_tx: &mpsc::UnboundedSender<StreamEvent>,
    ) -> Result<(), String> {
        let mut buffer = Vec::new();
        const PREBUFFER_SIZE: usize = 10 * 1024 * 1024; // 10MB

        let mut video_streamer: Option<VideoStreamer> = None;

        let mut playback_started = false;

        println!(
            "Starting prebuffering - will collect up to {}MB before starting video pipeline",
            PREBUFFER_SIZE / (1024 * 1024)
        );

        for chunk_result in stream {
            let chunk = chunk_result?;

            if !playback_started {
                // First phase: collect data until we have enough for reliable format detection
                buffer.extend_from_slice(&chunk);

                // Log progress every 10MB to reduce log spam
                if buffer.len() % (10 * 1024 * 1024) < chunk.len() {
                    println!(
                        "Prebuffering progress: {}MB collected",
                        buffer.len() / (1024 * 1024)
                    );
                }

                // Start playback once we hit PREBUFFER_SIZE OR when we have all the data (whichever comes first)
                if buffer.len() >= PREBUFFER_SIZE {
                    println!("✅ Reached {PREBUFFER_SIZE}MB prebuffer limit! Creating video pipeline and starting playback");

                    // Create pipeline and start playback
                    let streamer = VideoStreamer::new()
                        .map_err(|e| format!("Failed to create video streamer: {}", e))?;

                    println!(
                        "VideoStreamer created, pushing initial buffer of {}MB",
                        buffer.len() / (1024 * 1024)
                    );
                    Self::push_chunk_to_streamer(&buffer, &streamer)?;

                    // Signal that video streamer is ready
                    let _ = stream_tx.send(StreamEvent::VideoStreamerReady { stream_id });

                    video_streamer = Some(streamer);
                    playback_started = true;
                    buffer.clear(); // Free the buffer memory
                }
            } else {
                // Second phase: continue streaming remaining chunks to the pipeline
                if let Some(ref streamer) = video_streamer {
                    Self::push_chunk_to_streamer(&chunk, streamer)?;
                }
            }

            if stream_tx
                .send(StreamEvent::ChunkReceived {
                    stream_id,
                    size: chunk.len(),
                })
                .is_err()
            {
                break;
            }
        }

        // Handle case where total file size is less than PREBUFFER_SIZE
        if !playback_started && !buffer.is_empty() {
            println!(
                "✅ File smaller than {PREBUFFER_SIZE}MB - creating video pipeline with {}MB of data",
                buffer.len() / (1024 * 1024)
            );

            let streamer = VideoStreamer::new()
                .map_err(|e| format!("Failed to create video streamer: {}", e))?;

            Self::push_chunk_to_streamer(&buffer, &streamer)?;

            // Signal that video streamer is ready
            let _ = stream_tx.send(StreamEvent::VideoStreamerReady { stream_id });

            video_streamer = Some(streamer);
        }

        // Signal end of stream and completion
        if let Some(streamer) = video_streamer {
            println!("All chunks processed, signaling end of stream");
            if let Err(e) = streamer.signal_end_of_stream() {
                return Err(format!("Failed to signal end of stream: {e}"));
            }
            println!("End of stream signaled successfully");

            // Send completion event to UI with VideoStreamer to keep it alive
            let _ = stream_tx.send(StreamEvent::StreamComplete {
                stream_id,
                video_streamer: streamer,
            });
            println!("StreamComplete event sent to UI with VideoStreamer");
        }

        Ok(())
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

    fn clear_all_streams(&mut self) {
        println!("Clearing all streams and VideoStreamers");

        // Abort all running streaming tasks
        for (stream_id, task) in self.stream_tasks.drain() {
            println!("Aborting streaming task for stream {}", stream_id);
            task.abort();
        }

        // Clear all streams
        self.streams.clear();

        // Clear all VideoStreamers - this will stop all GStreamer pipelines
        self.video_streamers.clear();

        println!("All streams and VideoStreamers cleared");
    }

    fn format_data_size(&self, bytes: usize) -> String {
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
            .with_title("AnTube")
            .with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "AnTube",
        options,
        Box::new(|_cc| Box::new(AntubeApp::new(args))),
    )
}
