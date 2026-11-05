use rodio::Source;
use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use symphonia::core::audio::{SampleBuffer, SignalSpec};
use symphonia::core::codecs::{CODEC_TYPE_MP3, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::stations::{Station, Track, USER_AGENT, fetch_now_playing, metadata_agent};

pub const OUT_RATE: u32 = 44_100;
pub const OUT_CHANNELS: u16 = 2;
const QUEUE_SECONDS: usize = 4;
const ANALYSIS_SAMPLES: usize = 8192;
const READ_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnState {
    Connecting,
    Live,
    Reconnecting,
}

#[derive(Clone, Debug)]
pub struct Connection {
    pub state: ConnState,
    pub attempt: u32,
    pub note: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StreamInfo {
    pub codec: &'static str,
    pub sample_rate: u32,
    pub channels: usize,
    pub bitrate: Option<u32>,
}

struct TuneRequest {
    station: Station,
    generation: u64,
}

struct SampleQueue {
    inner: Mutex<VecDeque<f32>>,
    not_full: Condvar,
    capacity: usize,
}

impl SampleQueue {
    fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(VecDeque::with_capacity(capacity)),
            not_full: Condvar::new(),
            capacity,
        }
    }
}

pub struct SharedAudio {
    generation: AtomicU64,
    running: AtomicBool,
    buffer_fill: AtomicU32,
    played: AtomicU64,
    queue: SampleQueue,
    analysis: Mutex<VecDeque<f32>>,
    connection: Mutex<Connection>,
    stream: Mutex<Option<StreamInfo>>,
    track: Mutex<Option<Track>>,
    target: Mutex<Option<TuneRequest>>,
    target_cv: Condvar,
}

impl SharedAudio {
    fn new() -> Self {
        let capacity = OUT_RATE as usize * OUT_CHANNELS as usize * QUEUE_SECONDS;
        Self {
            generation: AtomicU64::new(0),
            running: AtomicBool::new(true),
            buffer_fill: AtomicU32::new(0),
            played: AtomicU64::new(0),
            queue: SampleQueue::new(capacity),
            analysis: Mutex::new(VecDeque::with_capacity(ANALYSIS_SAMPLES)),
            connection: Mutex::new(Connection {
                state: ConnState::Connecting,
                attempt: 0,
                note: None,
            }),
            stream: Mutex::new(None),
            track: Mutex::new(None),
            target: Mutex::new(None),
            target_cv: Condvar::new(),
        }
    }

    fn is_stale(&self, generation: u64) -> bool {
        !self.running.load(Ordering::Relaxed)
            || self.generation.load(Ordering::Relaxed) != generation
    }

    fn tune(&self, station: Station) {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.clear_queue();
        self.analysis.lock().unwrap().clear();
        *self.track.lock().unwrap() = None;
        *self.stream.lock().unwrap() = None;
        self.set_connection(Connection {
            state: ConnState::Connecting,
            attempt: 0,
            note: None,
        });
        *self.target.lock().unwrap() = Some(TuneRequest {
            station,
            generation,
        });
        self.target_cv.notify_all();
    }

    fn clear_queue(&self) {
        self.queue.inner.lock().unwrap().clear();
        self.buffer_fill.store(0, Ordering::Relaxed);
        self.queue.not_full.notify_all();
    }

    fn set_connection(&self, connection: Connection) {
        *self.connection.lock().unwrap() = connection;
    }

    fn mark_live(&self, codec: &'static str, rate: u32, channels: usize, bitrate: Option<u32>) {
        self.set_connection(Connection {
            state: ConnState::Live,
            attempt: 0,
            note: None,
        });
        *self.stream.lock().unwrap() = Some(StreamInfo {
            codec,
            sample_rate: rate,
            channels,
            bitrate,
        });
    }

    fn push_pcm(&self, data: &[f32], generation: u64) -> bool {
        let mut offset = 0;
        while offset < data.len() {
            if self.is_stale(generation) {
                return false;
            }
            let mut queue = self.queue.inner.lock().unwrap();
            let room = self.queue.capacity.saturating_sub(queue.len());
            if room == 0 {
                let (guard, _) = self
                    .queue
                    .not_full
                    .wait_timeout(queue, Duration::from_millis(120))
                    .unwrap();
                drop(guard);
                continue;
            }
            let take = room.min(data.len() - offset);
            queue.extend(data[offset..offset + take].iter().copied());
            offset += take;
            let fill = queue.len() as f32 / self.queue.capacity as f32;
            drop(queue);
            self.buffer_fill
                .store((fill * 1000.0) as u32, Ordering::Relaxed);
        }
        true
    }

    fn take_pcm(&self, out: &mut [f32]) -> usize {
        let mut queue = self.queue.inner.lock().unwrap();
        let taken = out.len().min(queue.len());
        for slot in out.iter_mut().take(taken) {
            *slot = queue.pop_front().unwrap_or(0.0);
        }
        let fill = queue.len() as f32 / self.queue.capacity as f32;
        drop(queue);
        self.buffer_fill
            .store((fill * 1000.0) as u32, Ordering::Relaxed);
        if taken > 0 {
            self.played.fetch_add(taken as u64, Ordering::Relaxed);
            self.queue.not_full.notify_one();
        }
        taken
    }

    fn push_analysis(&self, stereo: &[f32]) {
        if stereo.len() < OUT_CHANNELS as usize {
            return;
        }
        let mut analysis = self.analysis.lock().unwrap();
        for frame in stereo.as_chunks::<2>().0 {
            analysis.push_back((frame[0] + frame[1]) * 0.5);
        }
        while analysis.len() > ANALYSIS_SAMPLES {
            analysis.pop_front();
        }
    }

    pub fn connection(&self) -> Connection {
        self.connection.lock().unwrap().clone()
    }

    pub fn track(&self) -> Option<Track> {
        self.track.lock().unwrap().clone()
    }

    pub fn stream_info(&self) -> Option<StreamInfo> {
        self.stream.lock().unwrap().clone()
    }

    pub fn buffer_fill(&self) -> f32 {
        self.buffer_fill.load(Ordering::Relaxed) as f32 / 1000.0
    }

    pub fn played_samples(&self) -> u64 {
        self.played.load(Ordering::Relaxed)
    }

    pub fn analysis_snapshot(&self, len: usize) -> Vec<f32> {
        let analysis = self.analysis.lock().unwrap();
        let start = analysis.len().saturating_sub(len);
        analysis.iter().skip(start).copied().collect()
    }
}

pub struct Engine {
    shared: Arc<SharedAudio>,
}

impl Engine {
    pub fn new() -> Self {
        let shared = Arc::new(SharedAudio::new());
        let worker = shared.clone();
        thread::spawn(move || manager_loop(worker));
        Self { shared }
    }

    pub fn shared(&self) -> &Arc<SharedAudio> {
        &self.shared
    }

    pub fn source(&self) -> LiveSource {
        LiveSource {
            shared: self.shared.clone(),
            local: VecDeque::with_capacity(1024),
        }
    }

    pub fn tune(&self, station: Station) {
        self.shared.tune(station);
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shared.running.store(false, Ordering::Relaxed);
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        self.shared.target_cv.notify_all();
        self.shared.clear_queue();
    }
}

pub struct LiveSource {
    shared: Arc<SharedAudio>,
    local: VecDeque<f32>,
}

impl Iterator for LiveSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.local.is_empty() {
            self.local.resize(1024, 0.0);
            let taken = self.shared.take_pcm(&mut self.local.make_contiguous()[..]);
            self.local.truncate(taken);
            if taken == 0 {
                return Some(0.0);
            }
        }
        self.local.pop_front()
    }
}

impl Source for LiveSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        OUT_CHANNELS
    }

    fn sample_rate(&self) -> u32 {
        OUT_RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[derive(Debug)]
enum StreamError {
    Aborted,
    Closed,
    Failed(String),
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamError::Aborted => write!(f, "已取消"),
            StreamError::Closed => write!(f, "音频流已断开"),
            StreamError::Failed(message) => write!(f, "{message}"),
        }
    }
}

fn manager_loop(shared: Arc<SharedAudio>) {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(READ_TIMEOUT)
        .user_agent(USER_AGENT)
        .build();
    let mut seen_generation = 0;

    loop {
        if !shared.running.load(Ordering::Relaxed) {
            return;
        }
        let request = {
            let mut target = shared.target.lock().unwrap();
            loop {
                if !shared.running.load(Ordering::Relaxed) {
                    return;
                }
                let ready = target
                    .as_ref()
                    .filter(|request| request.generation != seen_generation)
                    .map(|request| TuneRequest {
                        station: request.station.clone(),
                        generation: request.generation,
                    });
                if let Some(request) = ready {
                    break request;
                }
                let (guard, _) = shared
                    .target_cv
                    .wait_timeout(target, Duration::from_millis(250))
                    .unwrap();
                target = guard;
            }
        };
        seen_generation = request.generation;
        let generation = request.generation;
        spawn_metadata(shared.clone(), request.station.clone(), generation);

        let mut attempt = 0u32;
        loop {
            if shared.is_stale(generation) {
                break;
            }
            if attempt == 0 {
                shared.set_connection(Connection {
                    state: ConnState::Connecting,
                    attempt,
                    note: None,
                });
            }
            match stream_loop(&agent, &shared, &request.station, generation) {
                Err(StreamError::Aborted) => break,
                Ok(()) | Err(StreamError::Closed) => {
                    if shared.is_stale(generation) {
                        break;
                    }
                    attempt += 1;
                    shared.set_connection(Connection {
                        state: ConnState::Reconnecting,
                        attempt,
                        note: Some("连接中断，正在重连".to_string()),
                    });
                }
                Err(StreamError::Failed(message)) => {
                    if shared.is_stale(generation) {
                        break;
                    }
                    attempt += 1;
                    shared.set_connection(Connection {
                        state: ConnState::Reconnecting,
                        attempt,
                        note: Some(message),
                    });
                }
            }
            let backoff_seconds = 1u64 << attempt.saturating_sub(1).min(3);
            let backoff = Duration::from_secs(backoff_seconds);
            if !sleep_while_active(&shared, generation, backoff) {
                break;
            }
            shared.clear_queue();
            shared.set_connection(Connection {
                state: ConnState::Connecting,
                attempt,
                note: None,
            });
        }
    }
}

fn sleep_while_active(shared: &SharedAudio, generation: u64, total: Duration) -> bool {
    let deadline = Instant::now() + total;
    while Instant::now() < deadline {
        if shared.is_stale(generation) {
            return false;
        }
        thread::sleep(Duration::from_millis(100));
    }
    !shared.is_stale(generation)
}

fn spawn_metadata(shared: Arc<SharedAudio>, station: Station, generation: u64) {
    let Some(somafm_id) = station.somafm_id else {
        return;
    };
    thread::spawn(move || {
        let agent = metadata_agent(Duration::from_secs(8));
        loop {
            if shared.is_stale(generation) {
                return;
            }
            if let Ok(track) = fetch_now_playing(&agent, &somafm_id) {
                if shared.is_stale(generation) {
                    return;
                }
                *shared.track.lock().unwrap() = Some(track);
            }
            for _ in 0..40 {
                if shared.is_stale(generation) {
                    return;
                }
                thread::sleep(Duration::from_millis(500));
            }
        }
    });
}

fn stream_loop(
    agent: &ureq::Agent,
    shared: &Arc<SharedAudio>,
    station: &Station,
    generation: u64,
) -> Result<(), StreamError> {
    let response = match agent.get(&station.url).call() {
        Ok(response) => response,
        Err(ureq::Error::Status(code, _)) => {
            return Err(StreamError::Failed(format!("服务器返回 HTTP {code}")));
        }
        Err(error) => {
            return Err(StreamError::Failed(format!("连接失败: {error}")));
        }
    };

    let reader = InterruptibleReader {
        inner: response.into_reader(),
        shared: shared.clone(),
        generation,
        position: 0,
    };
    let stream = MediaSourceStream::new(Box::new(reader), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions {
                enable_gapless: false,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .map_err(|error| map_symphonia(error, shared, generation))?;
    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| StreamError::Failed("音频流中没有音轨".to_string()))?;
    let track_id = track.id;
    let codec = if track.codec_params.codec == CODEC_TYPE_MP3 {
        "MP3"
    } else {
        "AUDIO"
    };
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|error| StreamError::Failed(format!("无法初始化解码器: {error}")))?;

    let mut converter = PcmConverter::new(OUT_RATE);
    let mut pcm = Vec::with_capacity(8192);
    let mut buffer: Option<DecodeBuffer> = None;
    let mut live = false;

    loop {
        if shared.is_stale(generation) {
            return Err(StreamError::Aborted);
        }
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(error) => return Err(map_symphonia(error, shared, generation)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(error) => return Err(map_symphonia(error, shared, generation)),
        };
        if decoded.frames() == 0 {
            continue;
        }

        let spec = *decoded.spec();
        let frames = decoded.frames();
        let rebuild = match &buffer {
            Some(buffer) => buffer.frames < frames || buffer.spec != spec,
            None => true,
        };
        if rebuild {
            buffer = Some(DecodeBuffer {
                buffer: SampleBuffer::new(frames as u64, spec),
                frames,
                spec,
            });
        }
        let decode_buffer = buffer.as_mut().expect("buffer was just created");
        decode_buffer.buffer.copy_interleaved_ref(decoded);
        let samples = decode_buffer.buffer.samples();
        if samples.is_empty() {
            continue;
        }

        pcm.clear();
        converter.convert(samples, spec.channels.count(), spec.rate, &mut pcm);
        if pcm.is_empty() {
            continue;
        }
        if !live {
            live = true;
            shared.mark_live(codec, spec.rate, spec.channels.count(), station.bitrate);
        }
        shared.push_analysis(&pcm);
        if !shared.push_pcm(&pcm, generation) {
            return Err(StreamError::Aborted);
        }
    }
}

fn map_symphonia(error: SymphoniaError, shared: &SharedAudio, generation: u64) -> StreamError {
    match error {
        SymphoniaError::IoError(error) => {
            if shared.is_stale(generation) || error.kind() == io::ErrorKind::Interrupted {
                StreamError::Aborted
            } else if error.kind() == io::ErrorKind::UnexpectedEof {
                StreamError::Closed
            } else {
                StreamError::Failed(format!("音频流读取失败: {error}"))
            }
        }
        SymphoniaError::DecodeError(message) => StreamError::Failed(format!("解码错误: {message}")),
        SymphoniaError::Unsupported(message) => {
            StreamError::Failed(format!("不支持的音频格式: {message}（目前仅支持 MP3 流）"))
        }
        SymphoniaError::ResetRequired => StreamError::Closed,
        other => StreamError::Failed(other.to_string()),
    }
}

struct DecodeBuffer {
    buffer: SampleBuffer<f32>,
    frames: usize,
    spec: SignalSpec,
}

struct InterruptibleReader {
    inner: Box<dyn Read + Send + Sync>,
    shared: Arc<SharedAudio>,
    generation: u64,
    position: u64,
}

impl Read for InterruptibleReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.shared.is_stale(self.generation) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "stream aborted"));
        }
        let read = self.inner.read(buf)?;
        self.position += read as u64;
        Ok(read)
    }
}

impl Seek for InterruptibleReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        match position {
            SeekFrom::Current(0) => Ok(self.position),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "live streams are not seekable",
            )),
        }
    }
}

impl MediaSource for InterruptibleReader {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

struct PcmConverter {
    out_rate: u32,
    in_rate: u32,
    resampler: Option<LinearResampler>,
    stereo: Vec<f32>,
}

impl PcmConverter {
    fn new(out_rate: u32) -> Self {
        Self {
            out_rate,
            in_rate: out_rate,
            resampler: None,
            stereo: Vec::new(),
        }
    }

    fn convert(&mut self, samples: &[f32], channels: usize, rate: u32, out: &mut Vec<f32>) {
        let channels = channels.max(1);
        let frames = samples.len() / channels;
        if frames == 0 {
            return;
        }
        if rate == self.out_rate {
            out.reserve(frames * 2);
            for frame in 0..frames {
                let left = samples[frame * channels];
                let right = if channels >= 2 {
                    samples[frame * channels + 1]
                } else {
                    left
                };
                out.push(left);
                out.push(right);
            }
            return;
        }

        self.stereo.clear();
        self.stereo.reserve(frames * 2);
        for frame in 0..frames {
            let left = samples[frame * channels];
            let right = if channels >= 2 {
                samples[frame * channels + 1]
            } else {
                left
            };
            self.stereo.push(left);
            self.stereo.push(right);
        }
        if self.resampler.is_none() || self.in_rate != rate {
            self.resampler = Some(LinearResampler::new(rate, self.out_rate));
            self.in_rate = rate;
        }
        let resampler = self.resampler.as_mut().unwrap();
        resampler.process(&self.stereo, out);
    }
}

struct LinearResampler {
    step: f64,
    position: f64,
    previous: Option<(f32, f32)>,
}

impl LinearResampler {
    fn new(in_rate: u32, out_rate: u32) -> Self {
        Self {
            step: in_rate as f64 / out_rate as f64,
            position: 0.0,
            previous: None,
        }
    }

    fn process(&mut self, frames: &[f32], out: &mut Vec<f32>) {
        let count = frames.len() / 2;
        if count == 0 {
            return;
        }
        let (mut current, mut index) = match self.previous {
            Some(previous) => (previous, 0),
            None => {
                let first = (frames[0], frames[1]);
                self.previous = Some(first);
                (first, 1)
            }
        };
        loop {
            if index >= count {
                self.previous = Some(current);
                break;
            }
            let next = (frames[index * 2], frames[index * 2 + 1]);
            while self.position < 1.0 {
                let fraction = self.position as f32;
                out.push(current.0 + (next.0 - current.0) * fraction);
                out.push(current.1 + (next.1 - current.1) * fraction);
                self.position += self.step;
            }
            self.position -= 1.0;
            current = next;
            index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stations::builtin_stations;

    #[test]
    fn resampler_downsamples_length() {
        let mut resampler = LinearResampler::new(48_000, 44_100);
        let frames: Vec<f32> = (0..4800)
            .flat_map(|i| [i as f32 / 4800.0, i as f32 / 4800.0])
            .collect();
        let mut out = Vec::new();
        resampler.process(&frames, &mut out);
        let produced = out.len() / 2;
        assert!(
            (produced as i64 - 4410).abs() <= 2,
            "expected about 4410 frames, got {produced}"
        );
    }

    #[test]
    fn converter_downmixes_mono_to_stereo() {
        let mut converter = PcmConverter::new(OUT_RATE);
        let mut out = Vec::new();
        converter.convert(&[0.5, 0.25], 1, OUT_RATE, &mut out);
        assert_eq!(out, vec![0.5, 0.5, 0.25, 0.25]);
    }

    #[test]
    #[ignore = "requires network access"]
    fn live_stream_decodes_audio() {
        let station = builtin_stations().remove(0);
        let generation = 0;
        let shared = Arc::new(SharedAudio::new());
        let worker_shared = shared.clone();
        let worker = thread::spawn(move || {
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(10))
                .timeout_read(READ_TIMEOUT)
                .user_agent(USER_AGENT)
                .build();
            stream_loop(&agent, &worker_shared, &station, generation)
        });

        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline && shared.buffer_fill() <= 0.0 {
            thread::sleep(Duration::from_millis(100));
        }
        let fill = shared.buffer_fill();
        let info = shared.stream_info();
        let connection = shared.connection();
        shared.running.store(false, Ordering::Relaxed);
        let outcome = worker.join();
        eprintln!("fill={fill} info={info:?} connection={connection:?} outcome={outcome:?}");

        assert!(fill > 0.0, "no audio was buffered");
        assert!(info.is_some(), "no stream info was detected");
    }

    #[test]
    #[ignore = "requires an audio output device"]
    fn sink_consumes_the_stream() {
        use rodio::{OutputStream, Sink};

        let (_stream, handle) = OutputStream::try_default().unwrap();
        let sink = Sink::try_new(&handle).unwrap();
        sink.set_volume(0.0);
        let engine = Engine::new();
        sink.append(engine.source());
        sink.play();
        engine.tune(builtin_stations().remove(0));

        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && engine.shared().played_samples() < OUT_RATE as u64 * 2 {
            thread::sleep(Duration::from_millis(100));
        }
        let played = engine.shared().played_samples();
        drop(engine);
        drop(sink);
        assert!(played > 0, "the sink never consumed audio");
    }
}
