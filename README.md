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

- **VideoStreamer**: GStreamer pipeline (AppSrc → DecodeB → VideoSink)
- **Circular Buffer**: Automatic memory management with 50MB limit
- **Real-time Processing**: Streams video as chunks arrive from the network
- **No Disk I/O**: Everything processed in memory

## Requirements

- GStreamer 1.14+ (installed via `brew install gstreamer` on macOS)
- Rust 1.70+
- Autonomi network access
