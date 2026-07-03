// whisper_stt.rs

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use pipewire as pw;
use pw::{properties::properties, spa};
use spa::{
    param::format::{MediaSubtype, MediaType},
    param::format_utils,
    pod::Pod,
};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct WhisperModel {
    pub name: &'static str,
    pub url: &'static str,
    pub hash: &'static str,
}

pub const WHISPER_MODELS: &[WhisperModel] = &[
    WhisperModel {
        name: "Base Q8 (78MiB)",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q8_0.bin",
        hash: "7bb89bb49ed6955013b166f1b6a6c04584a20fbe",
    },
    WhisperModel {
        name: "Small Q8 (252MiB)",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q8_0.bin",
        hash: "bcad8a2083f4e53d648d586b7dbc0cd673d8afad",
    },
    WhisperModel {
        name: "Turbo Q5 (574MiB)",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        hash: "e050f7970618a659205450ad97eb95a18d69c9ee",
    },
    WhisperModel {
        name: "Turbo Q8 (874MiB)",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q8_0.bin",
        hash: "01bf15bedffe9f39d65c1b6ff9b687ea91f59e0e",
    },
    WhisperModel {
        name: "Turbo (1.5GiB)",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
        hash: "4af2b29d7ec73d781377bfd1758ca957a807e941",
    },
];

const WHISPER_SAMPLE_RATE: usize = 16_000;
const MAX_DURATION: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct WhisperSttConfig {
    pub model_path: PathBuf,
    pub language: Option<String>,

    pub initial_prompt: Option<String>,
    pub n_threads: i32,

    /// lower values reduce release-time lag but cost more CPU/GPU
    pub partial_decode_interval_ms: u64,

    /// ignore extremely short accidental taps
    pub min_audio_ms: u64,

    /// mic object name from `pw-dump`, None for the default
    pub pipewire_target_object: Option<String>,

    pub use_gpu: bool,
    pub gpu_device: i32,
    pub flash_attn: bool,
}

impl WhisperSttConfig {
    pub fn new(model_path: impl AsRef<Path>) -> Self {
        let n_threads = std::thread::available_parallelism()
            .map(|n| n.get().min(4) as i32)
            .unwrap_or(4);

        Self {
            model_path: model_path.as_ref().to_path_buf(),
            language: None,
            initial_prompt: None,
            n_threads,
            partial_decode_interval_ms: 700,
            min_audio_ms: 250,
            pipewire_target_object: None,
            use_gpu: true,
            gpu_device: 0,
            flash_attn: false,
        }
    }
}

#[derive(Debug)]
pub enum WhisperSttError {
    ModelLoad(String),
    Whisper(String),
    PipeWire(String),
    CaptureInit(String),
    ThreadSpawn(String),
    CaptureThreadPanicked,
    AlreadyRecording,
    NotRecording,
}

impl fmt::Display for WhisperSttError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModelLoad(e) => write!(f, "failed to load whisper model: {e}"),
            Self::Whisper(e) => write!(f, "whisper error: {e}"),
            Self::PipeWire(e) => write!(f, "pipewire error: {e}"),
            Self::CaptureInit(e) => write!(f, "failed to initialize capture: {e}"),
            Self::ThreadSpawn(e) => write!(f, "failed to spawn thread: {e}"),
            Self::CaptureThreadPanicked => write!(f, "capture thread panicked"),
            Self::AlreadyRecording => write!(f, "PTT is already active"),
            Self::NotRecording => write!(f, "PTT is not active"),
        }
    }
}

impl std::error::Error for WhisperSttError {}

struct StopCapture;

struct CaptureSession {
    stop_tx: pw::channel::Sender<StopCapture>,
    capture_thread: Option<JoinHandle<()>>,
    recognizer_thread: Option<JoinHandle<()>>,
    deadline: Instant,
}

pub struct WhisperStt {
    config: WhisperSttConfig,
    ctx: Arc<WhisperContext>,

    active: Option<CaptureSession>,
    finished_recognizers: Vec<JoinHandle<()>>,

    completed_rx: mpsc::Receiver<Result<String, String>>,
    completed_tx: mpsc::Sender<Result<String, String>>,

    last_error: Option<String>,
}

impl WhisperStt {
    pub fn new(model_path: impl AsRef<Path>) -> Result<Self, WhisperSttError> {
        Self::init(WhisperSttConfig::new(model_path))
    }

    pub fn init(config: WhisperSttConfig) -> Result<Self, WhisperSttError> {
        let mut ctx_params = WhisperContextParameters::default();
        ctx_params.use_gpu = config.use_gpu;
        ctx_params.gpu_device = config.gpu_device;
        ctx_params.flash_attn = config.flash_attn;

        let ctx = WhisperContext::new_with_params(&config.model_path, ctx_params)
            .map_err(|e| WhisperSttError::ModelLoad(e.to_string()))?;

        let (completed_tx, completed_rx) = mpsc::channel();

        Ok(Self {
            config,
            ctx: Arc::new(ctx),
            active: None,
            finished_recognizers: Vec::new(),
            completed_rx,
            completed_tx,
            last_error: None,
        })
    }

    /// starts a fresh pw capture stream and a transcription worker
    pub fn ptt_start(&mut self) -> Result<(), WhisperSttError> {
        self.reap_finished_recognizers();

        if self.active.is_some() {
            return Err(WhisperSttError::AlreadyRecording);
        }

        let (audio_tx, audio_rx) = mpsc::channel::<Vec<f32>>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let (stop_tx, stop_rx) = pw::channel::channel::<StopCapture>();

        let recognizer_thread = spawn_recognizer_thread(
            Arc::clone(&self.ctx),
            self.config.clone(),
            audio_rx,
            self.completed_tx.clone(),
        )?;

        let target_object = self.config.pipewire_target_object.clone();

        let capture_thread = thread::Builder::new()
            .name("whisper-stt-pipewire-capture".to_string())
            .spawn(move || {
                pipewire_capture_thread(audio_tx, stop_rx, target_object, ready_tx);
            })
            .map_err(|e| WhisperSttError::ThreadSpawn(e.to_string()))?;

        match ready_rx.recv() {
            Ok(Ok(())) => {
                self.active = Some(CaptureSession {
                    stop_tx,
                    capture_thread: Some(capture_thread),
                    recognizer_thread: Some(recognizer_thread),
                    deadline: Instant::now() + MAX_DURATION,
                });

                Ok(())
            }
            Ok(Err(e)) => {
                let _ = stop_tx.send(StopCapture);
                let _ = capture_thread.join();
                let _ = recognizer_thread.join();

                Err(WhisperSttError::CaptureInit(e))
            }
            Err(e) => {
                let _ = stop_tx.send(StopCapture);
                let _ = capture_thread.join();
                let _ = recognizer_thread.join();

                Err(WhisperSttError::CaptureInit(e.to_string()))
            }
        }
    }

    fn stop_active_capture(&mut self) -> Result<(), WhisperSttError> {
        let Some(mut session) = self.active.take() else {
            return Err(WhisperSttError::NotRecording);
        };

        let _ = session.stop_tx.send(StopCapture);

        let capture_result = if let Some(capture_thread) = session.capture_thread.take() {
            capture_thread
                .join()
                .map_err(|_| WhisperSttError::CaptureThreadPanicked)
        } else {
            Ok(())
        };

        if let Some(recognizer_thread) = session.recognizer_thread.take() {
            self.finished_recognizers.push(recognizer_thread);
        }

        capture_result
    }

    fn drain_completed_transcriptions(&mut self) -> Option<String> {
        let mut latest = None;

        while let Ok(result) = self.completed_rx.try_recv() {
            match result {
                Ok(text) => {
                    let text = normalize_transcript(text);
                    if !text.is_empty() {
                        latest = Some(text);
                    }
                }
                Err(e) => {
                    self.last_error = Some(e);
                }
            }
        }

        latest
    }

    /// stops the pw stream & finalizes recognition asynchronously
    /// poll `take_transcription()` from your main loop to receive transcription
    pub fn ptt_end(&mut self) -> Result<(), WhisperSttError> {
        self.stop_active_capture()
    }

    pub fn take_transcription(&mut self) -> Option<String> {
        self.reap_finished_recognizers();

        let latest = self.drain_completed_transcriptions();

        if latest.is_some() {
            return latest;
        }

        // been recording for too long, force send a stop signal
        if self
            .active
            .as_ref()
            .is_some_and(|session| Instant::now() >= session.deadline)
        {
            if let Err(e) = self.stop_active_capture() {
                self.last_error = Some(e.to_string());
            }
        }

        return None;
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
    }

    pub fn is_recording(&self) -> bool {
        self.active.is_some()
    }

    fn reap_finished_recognizers(&mut self) {
        let mut i = 0;

        while i < self.finished_recognizers.len() {
            if self.finished_recognizers[i].is_finished() {
                let handle = self.finished_recognizers.swap_remove(i);
                let _ = handle.join();
            } else {
                i += 1;
            }
        }
    }
}

impl Drop for WhisperStt {
    fn drop(&mut self) {
        if self.active.is_some() {
            let _ = self.ptt_end();
        }

        for handle in self.finished_recognizers.drain(..) {
            let _ = handle.join();
        }
    }
}

fn spawn_recognizer_thread(
    ctx: Arc<WhisperContext>,
    config: WhisperSttConfig,
    audio_rx: mpsc::Receiver<Vec<f32>>,
    completed_tx: mpsc::Sender<Result<String, String>>,
) -> Result<JoinHandle<()>, WhisperSttError> {
    thread::Builder::new()
        .name("whisper-stt-recognizer".to_string())
        .spawn(move || {
            recognizer_thread(ctx, config, audio_rx, completed_tx);
        })
        .map_err(|e| WhisperSttError::ThreadSpawn(e.to_string()))
}

fn recognizer_thread(
    ctx: Arc<WhisperContext>,
    config: WhisperSttConfig,
    audio_rx: mpsc::Receiver<Vec<f32>>,
    completed_tx: mpsc::Sender<Result<String, String>>,
) {
    let partial_stride_samples =
        ms_to_samples(config.partial_decode_interval_ms).max(WHISPER_SAMPLE_RATE / 4);
    let min_samples = ms_to_samples(config.min_audio_ms);

    let mut audio = Vec::<f32>::new();
    let mut last_decoded_len = 0usize;
    let mut latest_partial = String::new();

    while let Ok(chunk) = audio_rx.recv() {
        if chunk.is_empty() {
            continue;
        }

        audio.extend_from_slice(&chunk);

        let enough_new_audio =
            audio.len().saturating_sub(last_decoded_len) >= partial_stride_samples;

        if audio.len() >= min_samples && enough_new_audio {
            match transcribe_audio(&ctx, &config, &audio) {
                Ok(text) => {
                    latest_partial = text;
                    last_decoded_len = audio.len();
                }
                Err(_) => {
                    // Do not fail the session on a speculative decode.
                    // The final decode after PTT end gets reported.
                }
            }
        }
    }

    if audio.len() < min_samples {
        let _ = completed_tx.send(Ok(String::new()));
        return;
    }

    match transcribe_audio(&ctx, &config, &audio) {
        Ok(text) => {
            let _ = completed_tx.send(Ok(text));
        }
        Err(e) if !latest_partial.trim().is_empty() => {
            // Prefer a recent partial over losing the utterance completely.
            let _ = completed_tx.send(Ok(latest_partial));
            let _ = completed_tx.send(Err(e.to_string()));
        }
        Err(e) => {
            let _ = completed_tx.send(Err(e.to_string()));
        }
    }
}

fn transcribe_audio(
    ctx: &WhisperContext,
    config: &WhisperSttConfig,
    audio: &[f32],
) -> Result<String, WhisperSttError> {
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

    params.set_n_threads(config.n_threads);
    params.set_language(config.language.as_deref());
    params.set_no_timestamps(true);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

    if let Some(prompt) = config.initial_prompt.as_deref() {
        params.set_initial_prompt(prompt);
    }

    let mut state = ctx
        .create_state()
        .map_err(|e| WhisperSttError::Whisper(e.to_string()))?;

    state
        .full(params, audio)
        .map_err(|e| WhisperSttError::Whisper(e.to_string()))?;

    let text = state
        .as_iter()
        .map(|segment| segment.to_string())
        .collect::<Vec<_>>()
        .join("");

    Ok(normalize_transcript(text))
}

fn pipewire_capture_thread(
    audio_tx: mpsc::Sender<Vec<f32>>,
    stop_rx: pw::channel::Receiver<StopCapture>,
    target_object: Option<String>,
    ready_tx: mpsc::Sender<Result<(), String>>,
) {
    let mut ready_tx = Some(ready_tx);

    let result = run_pipewire_capture(audio_tx, stop_rx, target_object, &mut ready_tx);

    if let Err(e) = result {
        if let Some(ready_tx) = ready_tx.take() {
            let _ = ready_tx.send(Err(e.to_string()));
        }
    }
}

fn run_pipewire_capture(
    audio_tx: mpsc::Sender<Vec<f32>>,
    stop_rx: pw::channel::Receiver<StopCapture>,
    target_object: Option<String>,
    ready_tx: &mut Option<mpsc::Sender<Result<(), String>>>,
) -> Result<(), WhisperSttError> {
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)
        .map_err(|e| WhisperSttError::PipeWire(e.to_string()))?;

    let context = pw::context::ContextRc::new(&mainloop, None)
        .map_err(|e| WhisperSttError::PipeWire(e.to_string()))?;

    let core = context
        .connect_rc(None)
        .map_err(|e| WhisperSttError::PipeWire(e.to_string()))?;

    let _stop_receiver = stop_rx.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |_| {
            mainloop.quit();
        }
    });

    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Communication",
        *pw::keys::APP_NAME => "WhisperStt",
    };

    if let Some(target_object) = target_object {
        props.insert(*pw::keys::TARGET_OBJECT, target_object);
    }

    let stream = pw::stream::StreamBox::new(&core, "WhisperStt microphone capture", props)
        .map_err(|e| WhisperSttError::PipeWire(e.to_string()))?;

    let user_data = AudioCaptureUserData::default();
    let audio_tx_for_callback = audio_tx.clone();

    let _listener = stream
        .add_local_listener_with_user_data(user_data)
        .param_changed(|_, user_data, id, param| {
            let Some(param) = param else {
                return;
            };

            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }

            let Ok((media_type, media_subtype)) = format_utils::parse_format(param) else {
                return;
            };

            if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                return;
            }

            let _ = user_data.format.parse(param);
        })
        .process(move |stream, user_data| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };

            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }

            let data = &mut datas[0];

            let chunk = data.chunk();

            let offset = chunk.offset() as usize;
            let size = chunk.size() as usize;

            let Some(bytes) = data.data() else {
                return;
            };

            if offset >= bytes.len() {
                return;
            }

            let end = offset.saturating_add(size).min(bytes.len());
            let bytes = &bytes[offset..end];

            let channels = (user_data.format.channels() as usize).max(1);
            let input_rate = {
                let rate = user_data.format.rate() as usize;
                if rate == 0 { 48_000 } else { rate }
            };

            let resampled = user_data
                .resampler
                .push_interleaved_f32le_mono_16k(bytes, channels, input_rate);

            if !resampled.is_empty() {
                let _ = audio_tx_for_callback.send(resampled);
            }
        })
        .register()
        .map_err(|e| WhisperSttError::PipeWire(e.to_string()))?;

    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);

    let obj = pw::spa::pod::Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pw::spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };

    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .map_err(|e| WhisperSttError::PipeWire(e.to_string()))?
    .0
    .into_inner();

    let pod = Pod::from_bytes(&values).ok_or_else(|| {
        WhisperSttError::PipeWire("failed to parse serialized PipeWire pod".to_string())
    })?;

    let mut params = [pod];

    stream
        .connect(
            spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .map_err(|e| WhisperSttError::PipeWire(e.to_string()))?;

    if let Some(ready_tx) = ready_tx.take() {
        let _ = ready_tx.send(Ok(()));
    }

    mainloop.run();

    Ok(())
}

#[derive(Default)]
struct AudioCaptureUserData {
    format: spa::param::audio::AudioInfoRaw,
    resampler: StreamingResampler,
}

#[derive(Default)]
struct StreamingResampler {
    pending: Vec<f32>,
    position: f64,
    input_rate: usize,
}

impl StreamingResampler {
    fn push_interleaved_f32le_mono_16k(
        &mut self,
        bytes: &[u8],
        channels: usize,
        input_rate: usize,
    ) -> Vec<f32> {
        if channels == 0 || input_rate == 0 {
            return Vec::new();
        }

        if self.input_rate != input_rate {
            self.pending.clear();
            self.position = 0.0;
            self.input_rate = input_rate;
        }

        let frame_bytes = channels * std::mem::size_of::<f32>();
        if frame_bytes == 0 {
            return Vec::new();
        }

        let frames = bytes.len() / frame_bytes;
        if frames == 0 {
            return Vec::new();
        }

        let mut mono = Vec::with_capacity(frames);

        for frame in 0..frames {
            let frame_start = frame * frame_bytes;
            let mut sum = 0.0f32;

            for ch in 0..channels {
                let sample_start = frame_start + ch * 4;

                let sample = f32::from_le_bytes([
                    bytes[sample_start],
                    bytes[sample_start + 1],
                    bytes[sample_start + 2],
                    bytes[sample_start + 3],
                ]);

                sum += sample;
            }

            mono.push(sum / channels as f32);
        }

        self.pending.extend_from_slice(&mono);

        let step = input_rate as f64 / WHISPER_SAMPLE_RATE as f64;
        let mut out = Vec::with_capacity(
            ((self.pending.len() as f64 - self.position) / step).max(0.0) as usize,
        );

        while self.position + 1.0 < self.pending.len() as f64 {
            let i = self.position.floor() as usize;
            let frac = (self.position - i as f64) as f32;

            let a = self.pending[i];
            let b = self.pending[i + 1];

            out.push(a + (b - a) * frac);

            self.position += step;
        }

        let drop_count = self.position.floor() as usize;
        if drop_count > 0 {
            self.pending.drain(..drop_count);
            self.position -= drop_count as f64;
        }

        out
    }
}

fn ms_to_samples(ms: u64) -> usize {
    ((ms as usize) * WHISPER_SAMPLE_RATE) / 1000
}

fn normalize_transcript(text: String) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
