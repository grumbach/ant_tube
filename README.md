# AnTube - Autonomi Video Streamer

A real-time video streaming application using GStreamer for the Autonomi network.

## Features

- **Real-time streaming**: Video plays as chunks arrive (no disk buffering)
- **Memory efficient**: Maximum 50MB memory usage with circular buffer
- **Chunk-based**: Processes 4MB chunks efficiently
- **Multiple networks**: Supports local, autonomi, and alpha environments

## Usage

### GUI Mode (Interactive)
```bash
cargo run
```

### CLI Mode (Fast Launch)
```bash
# Stream with specific network and address
cargo run -- --network local --address "your_data_address_here"

# Use different network environments
cargo run -- --network autonomi --address "your_data_address"
cargo run -- --network alpha --address "your_data_address"

# Just set network (address can be entered in GUI)
cargo run -- --network local
```

### Command Line Options

- `-n, --network <NETWORK>`: Network environment (local, autonomi, alpha) [default: autonomi]
- `-a, --address <ADDRESS>`: Data address to stream
- `-h, --help`: Show help information

## Examples

```bash
# Quick test with local network
cargo run -- -n local -a "abcdef123456789"

# Production streaming on autonomi
cargo run -- --network autonomi --address "real_video_address_here"

# Development testing
cargo run -- --network alpha --address "test_address"
```

## Architecture

AnTube uses a multi-layered architecture that separates download tasks from video playback to ensure continuous streaming:

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                                AntubeApp (Main UI)                              │
├─────────────────────────────────────────────────────────────────────────────────┤
│  streams: HashMap<StreamId, StreamInfo>         // UI state & metadata         │
│  stream_tasks: HashMap<StreamId, JoinHandle>    // Download task handles       │
│  video_streamers: HashMap<StreamId, VideoStreamer> // Live video pipelines    │
└─────────────────────────────────────────────────────────────────────────────────┘
                                    ▲
                                    │ StreamEvents
                                    │ (status updates)
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                            Event Channel (MPSC)                                │
│  ServerConnected, ChunkReceived, StreamComplete{VideoStreamer}, StreamError    │
└─────────────────────────────────────────────────────────────────────────────────┘
                                    ▲
                                    │ Events
                    ┌───────────────┴───────────────┐
                    │                               │
                    ▼                               ▼
┌─────────────────────────────┐           ┌─────────────────────────────┐
│    Stream Task (Tokio)      │           │    VideoStreamer           │
│  ┌─────────────────────────┐│           │  ┌─────────────────────────┐│
│  │  1. Server Init         ││           │  │   GStreamer Pipeline    ││
│  │  2. VideoStreamer Init  ││           │  │                         ││
│  │  3. Download Loop       ││           │  │  ┌─────┐ ┌──────────┐   ││
│  │  4. Send Completion     ││           │  │  │AppSrc│→│DecodeB in│   ││
│  └─────────────────────────┘│           │  │  └─────┘ └──────────┘   ││
│             │                │           │  │      ▼       ▼         ││
│             ▼                │           │  │ ┌─────────┐ ┌────────┐  ││
│  ┌─────────────────────────┐ │           │  │ │VideoConv│ │AudioConv│ ││
│  │ process_stream_with_    │ │           │  │ └─────────┘ └────────┘  ││
│  │ delayed_pipeline()      │ │           │  │      ▼         ▼       ││
│  │                         │ │           │  │ ┌─────────┐ ┌────────┐  ││
│  │ ┌─────────────────────┐ │ │           │  │ │VideoSink│ │AudioSink│ ││
│  │ │ 1. Prebuffer 40MB   │ │ │           │  │ │(Window) │ │(Speaker)│ ││
│  │ │ 2. Create pipeline  │ │ │           │  │ └─────────┘ └────────┘  ││
│  │ │ 3. Stream chunks    │ │ │           │  └─────────────────────────┘│
│  │ │ 4. Signal EOS       │ │ │           │              │              │
│  │ │ 5. Send VideoStr    │ │ │           │              ▲              │
│  │ └─────────────────────┘ │ │           │         Raw Video/Audio     │
│  └─────────────────────────┘ │           │         Bytes to Display    │
│                               │           └─────────────────────────────┘
└─────────────────────────────┘
         │
         ▼
┌─────────────────────────────┐
│       Autonomi Network      │
│                             │
│ ┌─────────┐ ┌─────────────┐ │
│ │ Server  │ │   Chunks    │ │
│ │         │ │ (bytes::    │ │
│ │ Client  │ │  Bytes)     │ │
│ └─────────┘ └─────────────┘ │
└─────────────────────────────┘
```

### Data Flow
1. **📥 Raw video bytes**: Network → Stream Task → VideoStreamer → GStreamer
2. **📊 Status updates**: Stream Task → Event Channel → AntubeApp UI
3. **🎬 VideoStreamer ownership**: Task creates → Task transfers to App → App keeps alive
4. **🧹 Task cleanup**: App removes finished JoinHandle, keeps VideoStreamer running

### Component Lifecycle
- **Stream Task**: `[Spawned] → [Downloads] → [Completes] → [Cleaned up]`
- **VideoStreamer**: `[Created] → [Receives data] → [Plays video] → [Stays alive until cleared]`
- **GStreamer**: `[Pipeline started] → [Decodes/plays] → [Continues until VideoStreamer dropped]`

### Key Design Principles
- **Separation of Concerns**: Download tasks are ephemeral, video playback is persistent
- **Memory Management**: 40MB prebuffering with 5MB GStreamer internal limits
- **No Disk I/O**: Everything processed in memory for optimal performance
- **Real-time Processing**: Video starts playing before download completes
- **Lifecycle Independence**: Video continues playing after download finishes

## Requirements

- GStreamer 1.14+ (installed via `brew install gstreamer` on macOS)
- Rust 1.70+
- Autonomi network access
