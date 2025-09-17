use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct StreamError(String);

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for StreamError {}

unsafe impl Send for StreamError {}
unsafe impl Sync for StreamError {}

struct PipelineElements {
    appsrc: gst::Element,
    decodebin: gst::Element,
    videoconvert: gst::Element,
    videosink: gst::Element,
    audioconvert: gst::Element,
    audiosink: gst::Element,
}

pub struct VideoStreamer {
    appsrc: gst_app::AppSrc,
    _pipeline: gst::Pipeline,
    chunk_queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    is_eos: Arc<Mutex<bool>>,
}

impl VideoStreamer {
    pub fn new() -> Result<Self, StreamError> {
        Self::init_gstreamer()?;

        let pipeline = gst::Pipeline::new();
        let elements = Self::create_pipeline_elements()?;

        Self::add_elements_to_pipeline(&pipeline, &elements)?;
        Self::link_static_elements(&elements)?;

        Self::setup_dynamic_linking(&elements);
        let appsrc = Self::configure_appsrc(elements.appsrc)?;
        Self::setup_bus_monitoring(&pipeline);

        Self::start_pipeline(&pipeline)?;

        Ok(Self {
            appsrc,
            _pipeline: pipeline,
            chunk_queue: Arc::new(Mutex::new(VecDeque::new())),
            is_eos: Arc::new(Mutex::new(false)),
        })
    }

    fn init_gstreamer() -> Result<(), StreamError> {
        gst::init().map_err(|e| StreamError(format!("Failed to initialize GStreamer: {}", e)))
    }

    fn create_pipeline_elements() -> Result<PipelineElements, StreamError> {
        Ok(PipelineElements {
            appsrc: Self::create_element("appsrc", Some("src"))?,
            decodebin: Self::create_element("decodebin", None)?,
            videoconvert: Self::create_element("videoconvert", None)?,
            videosink: Self::create_element("osxvideosink", None)?,
            audioconvert: Self::create_element("audioconvert", None)?,
            audiosink: Self::create_element("autoaudiosink", None)?,
        })
    }

    fn create_element(factory_name: &str, name: Option<&str>) -> Result<gst::Element, StreamError> {
        let mut builder = gst::ElementFactory::make(factory_name);
        if let Some(element_name) = name {
            builder = builder.name(element_name);
        }
        builder
            .build()
            .map_err(|e| StreamError(format!("Failed to create {}: {}", factory_name, e)))
    }

    fn add_elements_to_pipeline(
        pipeline: &gst::Pipeline,
        elements: &PipelineElements,
    ) -> Result<(), StreamError> {
        pipeline
            .add_many([
                &elements.appsrc,
                &elements.decodebin,
                &elements.videoconvert,
                &elements.videosink,
                &elements.audioconvert,
                &elements.audiosink,
            ])
            .map_err(|e| StreamError(format!("Failed to add elements to pipeline: {}", e)))
    }

    fn link_static_elements(elements: &PipelineElements) -> Result<(), StreamError> {
        gst::Element::link(&elements.appsrc, &elements.decodebin)
            .map_err(|e| StreamError(format!("Failed to link appsrc to decodebin: {}", e)))?;

        gst::Element::link(&elements.videoconvert, &elements.videosink)
            .map_err(|e| StreamError(format!("Failed to link videoconvert to videosink: {}", e)))?;

        gst::Element::link(&elements.audioconvert, &elements.audiosink)
            .map_err(|e| StreamError(format!("Failed to link audioconvert to audiosink: {}", e)))?;

        Ok(())
    }

    fn setup_dynamic_linking(elements: &PipelineElements) {
        let videoconvert = elements.videoconvert.clone();
        let audioconvert = elements.audioconvert.clone();

        elements.decodebin.connect_pad_added(move |_dbin, src_pad| {
            Self::handle_pad_added(src_pad, &videoconvert, &audioconvert);
        });
    }

    fn handle_pad_added(
        src_pad: &gst::Pad,
        videoconvert: &gst::Element,
        audioconvert: &gst::Element,
    ) {
        if src_pad.is_linked() {
            return;
        }

        let caps = match src_pad.current_caps() {
            Some(caps) => caps,
            None => {
                println!("No caps available on pad");
                return;
            }
        };

        let structure = match caps.structure(0) {
            Some(structure) => structure,
            None => return,
        };

        let media_type = structure.name();
        println!("Decodebin pad added: {}", media_type);

        if media_type.starts_with("video/") {
            Self::link_video_pad(src_pad, videoconvert);
        } else if media_type.starts_with("audio/") {
            Self::link_audio_pad(src_pad, audioconvert);
        }
    }

    fn link_video_pad(src_pad: &gst::Pad, videoconvert: &gst::Element) {
        let sink_pad = match videoconvert.static_pad("sink") {
            Some(pad) => pad,
            None => {
                println!("Could not get sink pad from videoconvert");
                return;
            }
        };

        match src_pad.link(&sink_pad) {
            Ok(_) => println!("Successfully linked video pad to videoconvert"),
            Err(e) => println!("Failed to link video pad: {:?}", e),
        }
    }

    fn link_audio_pad(src_pad: &gst::Pad, audioconvert: &gst::Element) {
        let sink_pad = match audioconvert.static_pad("sink") {
            Some(pad) => pad,
            None => {
                println!("Could not get sink pad from audioconvert");
                return;
            }
        };

        match src_pad.link(&sink_pad) {
            Ok(_) => println!("Successfully linked audio pad to audioconvert"),
            Err(e) => println!("Failed to link audio pad: {:?}", e),
        }
    }

    fn configure_appsrc(appsrc: gst::Element) -> Result<gst_app::AppSrc, StreamError> {
        let appsrc = appsrc
            .dynamic_cast::<gst_app::AppSrc>()
            .map_err(|_| StreamError("Element is not AppSrc".to_string()))?;

        appsrc.set_format(gst::Format::Bytes);
        appsrc.set_stream_type(gst_app::AppStreamType::Stream);
        appsrc.set_max_bytes(50 * 1024 * 1024u64);

        // Let decodebin auto-detect the format instead of setting caps
        println!("Configured AppSrc without caps for auto-detection");
        Ok(appsrc)
    }

    fn setup_bus_monitoring(pipeline: &gst::Pipeline) {
        let bus = match pipeline.bus() {
            Some(bus) => bus,
            None => return,
        };

        let pipeline_weak = pipeline.downgrade();
        bus.connect_message(None, move |_bus, msg| {
            Self::handle_bus_message(msg, &pipeline_weak);
        });
    }

    fn handle_bus_message(msg: &gst::Message, pipeline_weak: &gst::glib::WeakRef<gst::Pipeline>) {
        use gst::MessageView;
        match msg.view() {
            MessageView::StateChanged(state_changed) => {
                if let Some(pipeline) = pipeline_weak.upgrade() {
                    if msg.src() == Some(pipeline.upcast_ref()) {
                        println!(
                            "Pipeline state: {:?} -> {:?}",
                            state_changed.old(),
                            state_changed.current()
                        );
                    }
                }

                // Also log element state changes to see decodebin activity
                if let Some(element) = msg.src().and_then(|src| src.downcast_ref::<gst::Element>())
                {
                    let element_name = element.name();
                    if element_name.contains("decode") {
                        println!(
                            "Element {} state: {:?} -> {:?}",
                            element_name,
                            state_changed.old(),
                            state_changed.current()
                        );
                    }
                }
            }
            MessageView::Error(error) => {
                println!("Pipeline error: {}", error.error());
                if let Some(debug) = error.debug() {
                    println!("Debug info: {}", debug);
                }
            }
            MessageView::Warning(warning) => {
                println!("Pipeline warning: {}", warning.error());
                if let Some(debug) = warning.debug() {
                    println!("Warning debug: {}", debug);
                }
            }
            MessageView::Eos(_) => {
                println!("Pipeline received End-of-Stream");
            }
            MessageView::StreamStart(_) => {
                println!("Stream started");
            }
            MessageView::ClockProvide(_) => {
                println!("New clock provider available");
            }
            _ => {
                // Log specific message types we care about
                use gst::MessageView;
                match msg.view() {
                    MessageView::Element(_) => println!("Element message: {:?}", msg.view()),
                    MessageView::Buffering(_) => println!("Buffering message: {:?}", msg.view()),
                    MessageView::AsyncDone(_) => println!("Async done: {:?}", msg.view()),
                    MessageView::Latency(_) => println!("Latency message: {:?}", msg.view()),
                    MessageView::Qos(_) => println!("QoS message: {:?}", msg.view()),
                    MessageView::StreamCollection(_) => {
                        println!("Stream collection: {:?}", msg.view())
                    }
                    MessageView::StreamsSelected(_) => {
                        println!("Streams selected: {:?}", msg.view())
                    }
                    _ => {} // Skip other messages to avoid spam
                }
            }
        }
    }

    fn start_pipeline(pipeline: &gst::Pipeline) -> Result<(), StreamError> {
        println!("Starting pipeline");
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| StreamError(format!("Failed to start pipeline: {}", e)))?;
        println!("Pipeline started successfully");
        Ok(())
    }

    pub fn push_chunk(&self, chunk: Vec<u8>) -> Result<(), String> {
        if self.is_stream_ended() {
            return Err("Stream has ended".to_string());
        }

        let chunk_size = chunk.len();
        println!("Pushing chunk of {} bytes directly to AppSrc", chunk_size);

        let buffer = gst::Buffer::from_slice(chunk);
        match self.appsrc.push_buffer(buffer) {
            Ok(_) => {
                println!("Successfully pushed buffer to AppSrc");
                Ok(())
            }
            Err(gst::FlowError::Eos) => {
                println!("AppSrc returned EOS");
                self.signal_end_of_stream()
            }
            Err(e) => Err(format!("Failed to push buffer to AppSrc: {:?}", e)),
        }
    }

    fn is_stream_ended(&self) -> bool {
        self.is_eos.lock().map(|guard| *guard).unwrap_or(false)
    }

    pub fn signal_end_of_stream(&self) -> Result<(), String> {
        if let Ok(mut eos_guard) = self.is_eos.lock() {
            *eos_guard = true;
        }

        self.appsrc
            .end_of_stream()
            .map_err(|e| format!("Failed to signal end of stream: {:?}", e))
            .map(|_| ())
    }

    pub fn get_memory_usage(&self) -> usize {
        self.chunk_queue
            .lock()
            .map(|queue| queue.iter().map(|c| c.len()).sum())
            .unwrap_or(0)
    }
}

impl Drop for VideoStreamer {
    fn drop(&mut self) {
        let _ = self._pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Read;

    #[test]
    fn test_mp4_streaming_with_real_file() {
        println!("Testing MP4 streaming with real file");

        // Read the MP4 file
        let mp4_path = "/Users/anselme/Downloads/pylon.mp4";
        let mut file = File::open(mp4_path).expect("Failed to open MP4 file");

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .expect("Failed to read MP4 file");

        println!("Read {} bytes from MP4 file", buffer.len());

        // Create video streamer
        let streamer = VideoStreamer::new().expect("Failed to create VideoStreamer");

        // Stream the file in 1MB chunks (similar to our main app)
        let chunk_size = 1024 * 1024; // 1MB
        let mut total_pushed = 0;

        for (i, chunk) in buffer.chunks(chunk_size).enumerate() {
            println!("Pushing chunk {} of {} bytes", i, chunk.len());

            match streamer.push_chunk(chunk.to_vec()) {
                Ok(_) => {
                    total_pushed += chunk.len();
                    println!(
                        "Successfully pushed chunk {}, total: {} bytes",
                        i, total_pushed
                    );
                }
                Err(e) => {
                    println!("Failed to push chunk {}: {}", i, e);
                    break;
                }
            }

            // Small delay to simulate network streaming
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        println!(
            "Pushed {} total bytes, signaling end of stream",
            total_pushed
        );

        // Signal end of stream
        match streamer.signal_end_of_stream() {
            Ok(_) => println!("Successfully signaled end of stream"),
            Err(e) => println!("Failed to signal end of stream: {}", e),
        }

        // Let the pipeline process for a bit
        std::thread::sleep(std::time::Duration::from_secs(3));

        println!("Test completed - check if video window appeared");
    }

    #[test]
    fn test_network_assembled_mp4() {
        println!("Testing network-assembled MP4 file");

        // Read the network-assembled MP4 file
        let mp4_path = "/tmp/antube_complete.mp4";
        let mut file = match File::open(mp4_path) {
            Ok(file) => file,
            Err(_) => {
                println!("Network MP4 file not found - run main app first");
                return;
            }
        };

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .expect("Failed to read network MP4");

        println!("Read {} bytes from network MP4 file", buffer.len());

        // Create video streamer
        let streamer = VideoStreamer::new().expect("Failed to create VideoStreamer");

        // Check the first few bytes to see the MP4 headers
        println!("First 32 bytes: {:?}", &buffer[..32.min(buffer.len())]);

        // Stream just the first 4MB to test decodebin recognition
        let test_chunk_size = 4 * 1024 * 1024; // 4MB
        let test_data = &buffer[..test_chunk_size.min(buffer.len())];

        println!(
            "Pushing first {} bytes for format detection",
            test_data.len()
        );

        match streamer.push_chunk(test_data.to_vec()) {
            Ok(_) => println!("Successfully pushed test chunk"),
            Err(e) => println!("Failed to push test chunk: {}", e),
        }

        // Let it process
        std::thread::sleep(std::time::Duration::from_secs(2));

        println!("Network MP4 test completed");
    }
}
