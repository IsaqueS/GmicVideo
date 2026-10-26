//! Real-time playback engine.
//!
//! One engine thread owns the playback state and reacts to commands from the GUI.
//! While playing, a `Session` runs a small pipeline:
//!
//! ```text
//!  ffmpeg decoder ──► queue ──► N processors (G'MIC workers / GPU) ──► results ──► presenter ──► FrameSink
//! ```
//!
//! * Frames are decoded at the chosen preview resolution.
//! * Processors skip frames that would already be late when finished, so a slow effect
//!   lowers the frame rate instead of drifting behind the audio ("drop frames").
//! * The presenter shows each frame at its presentation time. The clock starts when the
//!   first frame is ready and can be corrected by the GUI's audio position (`Sync`).
//! * In "Auto" resolution mode the engine steps the resolution down when frames are dropped
//!   and back up when there is enough headroom.
//! * Effect parameters can change during playback without restarting the pipeline.

use crate::effect::{EffectSettings, PreparedEffect, Timeline};
use crate::frame::Frame;
use crate::frame_sink::{FrameKind, FrameSink};
use crate::gmic::FilterDatabase;
use crate::gmic_worker::GmicWorker;
use crate::gpu::effects::GpuEffectInfo;
use crate::gpu::processor::GpuProcessor;
use crate::server::Output;
use crate::video::{extract_frame, probe_video, scaled_size, spawn_decoder, DecoderOptions, VideoInfo};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::io::BufReader;
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// GPU processors per playback session (more only queue up on the same GPU).
const GPU_PROCESSORS: usize = 2;

/// Resolution steps used by the automatic mode.
const AUTO_LEVELS: [f64; 5] = [0.25, 0.375, 0.5, 0.75, 1.0];

/// Processes one frame of the timeline. Implemented by G'MIC workers and GPU effects.
pub trait FrameProcessor: Send {
    fn process(&mut self, frame: &Frame, effect: &PreparedEffect, timeline: &Timeline, idx: usize) -> Result<Frame, String>;
    fn is_alive(&self) -> bool {
        true
    }
    /// False for throwaway processors that should not be kept in the worker pool.
    fn reusable(&self) -> bool {
        true
    }
    /// True for GPU processors (kept in a separate pool).
    fn is_gpu(&self) -> bool {
        false
    }
    /// False if this processor is specialised for other effects (GPU processors).
    fn handles(&self, _effect: &PreparedEffect) -> bool {
        true
    }
}

impl FrameProcessor for GmicWorker {
    fn process(&mut self, frame: &Frame, effect: &PreparedEffect, timeline: &Timeline, idx: usize) -> Result<Frame, String> {
        let cmd = effect.command_for_frame(timeline, idx);
        GmicWorker::process(self, frame, &cmd, idx as u32)
    }
    fn is_alive(&self) -> bool {
        !self.is_dead()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaybackConfig {
    /// `None` = show the original video.
    #[serde(default)]
    pub effect: Option<EffectSettings>,
    #[serde(default)]
    pub cut_start_secs: Option<f64>,
    #[serde(default)]
    pub cut_end_secs: Option<f64>,
    /// 0 = automatic, otherwise a fraction of the source resolution.
    #[serde(default)]
    pub scale: f64,
    /// Also send the original frames (for the split view).
    #[serde(default)]
    pub show_original: bool,
}

#[derive(Debug)]
pub enum Command {
    Open(PathBuf),
    Configure(PlaybackConfig),
    Play { from_secs: Option<f64> },
    Pause,
    Seek { secs: f64 },
    Sync { secs: f64 },
    SetLoop(bool),
    Close,
}

/// Handle used by the IPC server.
#[derive(Clone)]
pub struct PlaybackHandle {
    tx: Sender<Command>,
}

impl PlaybackHandle {
    pub fn spawn(db: Arc<FilterDatabase>, sink: FrameSink, out: Output) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        std::thread::Builder::new()
            .name("playback".into())
            .spawn(move || Engine::new(db, sink, out).run(rx))
            .expect("spawn playback thread");
        Self { tx }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }
}

// ---------------------------------------------------------------------------------------
// Session: the running pipeline while playing
// ---------------------------------------------------------------------------------------

struct Decoded {
    pts: f64,
    frame: Frame,
}

struct Processed {
    pts: f64,
    original: Option<Frame>,
    effect: Frame,
}

/// Maps presentation time to wall-clock time. Starts when the first frame is shown.
#[derive(Default)]
struct Clock {
    origin: Option<(Instant, f64)>,
}

impl Clock {
    fn media_time(&self, now: Instant) -> Option<f64> {
        self.origin.map(|(t0, p0)| p0 + now.saturating_duration_since(t0).as_secs_f64())
    }
    fn instant_for(&self, pts: f64) -> Option<Instant> {
        self.origin.map(|(t0, p0)| {
            let d = pts - p0;
            if d >= 0.0 { t0 + Duration::from_secs_f64(d) } else { t0.checked_sub(Duration::from_secs_f64(-d)).unwrap_or(t0) }
        })
    }
}

struct Shared {
    stop: AtomicBool,
    clock: Mutex<Clock>,
    queue: Mutex<VecDeque<(u64, Decoded)>>,
    queue_cv: Condvar,
    results: Mutex<BTreeMap<u64, Processed>>,
    results_cv: Condvar,
    effect: RwLock<Option<Arc<PreparedEffect>>>,
    timeline: Timeline,
    frame_dur: f64,
    est_latency: Mutex<f64>,
    decoded_all: AtomicBool,
    in_flight: AtomicUsize,
    last_presented_seq: AtomicU64,
    presented: AtomicU64,
    dropped: AtomicU64,
    last_pts: Mutex<f64>,
    ended: AtomicBool,
    clock_started: AtomicBool,
    show_original: AtomicBool,
    error: Mutex<Option<String>>,
}

impl Shared {
    fn fail(&self, msg: String) {
        let mut e = self.error.lock().unwrap();
        if e.is_none() {
            *e = Some(msg);
        }
        self.stop.store(true, Ordering::SeqCst);
        self.queue_cv.notify_all();
        self.results_cv.notify_all();
    }
}

struct Session {
    shared: Arc<Shared>,
    scale: f64,
    size: (u32, u32),
    decoder: Arc<Mutex<Child>>,
    decoder_thread: Option<JoinHandle<()>>,
    worker_threads: Vec<JoinHandle<Box<dyn FrameProcessor>>>,
    presenter_thread: Option<JoinHandle<()>>,
    started_at: Instant,
}

struct SessionParams<'a> {
    info: &'a VideoInfo,
    timeline: Timeline,
    from_secs: f64,
    scale: f64,
    effect: Option<Arc<PreparedEffect>>,
    processors: Vec<Box<dyn FrameProcessor>>,
    show_original: bool,
    sink: FrameSink,
    out: Output,
    /// Keep the running clock (resolution/cut changes during playback) instead of restarting it.
    clock: Option<(Instant, f64)>,
}

impl Session {
    fn start(p: SessionParams) -> Result<Session, (String, Vec<Box<dyn FrameProcessor>>)> {
        let (w, h) = scaled_size(p.info.width, p.info.height, p.scale);
        let mut child = match spawn_decoder(&DecoderOptions {
            path: &p.info.path,
            start_secs: Some(p.from_secs),
            end_secs: Some(p.timeline.end_secs),
            fps: p.info.fps_fraction(),
            width: w,
            height: h,
            source_width: p.info.width,
            source_height: p.info.height,
        }) {
            Ok(c) => c,
            Err(e) => return Err((e, p.processors)),
        };
        let stdout = child.stdout.take();
        let _ = child.stderr.take(); // dropped: decoder errors show up as an early end of stream
        let decoder = Arc::new(Mutex::new(child));

        let fps = p.info.fps_num as f64 / p.info.fps_den.max(1) as f64;
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            clock: Mutex::new(Clock { origin: p.clock }),
            queue: Mutex::new(VecDeque::new()),
            queue_cv: Condvar::new(),
            results: Mutex::new(BTreeMap::new()),
            results_cv: Condvar::new(),
            effect: RwLock::new(p.effect),
            timeline: p.timeline,
            frame_dur: 1.0 / fps.max(1.0),
            est_latency: Mutex::new(0.0),
            decoded_all: AtomicBool::new(false),
            in_flight: AtomicUsize::new(0),
            last_presented_seq: AtomicU64::new(0),
            presented: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            last_pts: Mutex::new(p.from_secs),
            ended: AtomicBool::new(false),
            clock_started: AtomicBool::new(p.clock.is_some()),
            show_original: AtomicBool::new(p.show_original),
            error: Mutex::new(None),
        });

        let max_queue = p.processors.len() + 2;

        // Decoder thread.
        let decoder_thread = {
            let shared = shared.clone();
            let from = p.from_secs;
            std::thread::spawn(move || {
                let Some(stdout) = stdout else {
                    shared.decoded_all.store(true, Ordering::SeqCst);
                    return;
                };
                let mut reader = BufReader::with_capacity(1 << 21, stdout);
                let mut n: u64 = 0;
                loop {
                    if shared.stop.load(Ordering::SeqCst) {
                        break;
                    }
                    match Frame::read_gbrp(&mut reader, w, h) {
                        Ok(Some(frame)) => {
                            n += 1;
                            let pts = from + (n - 1) as f64 * shared.frame_dur;
                            // Skip frames that cannot be processed in time anyway, so a slow
                            // effect works on upcoming frames instead of falling behind.
                            let media_now = shared.clock.lock().unwrap().media_time(Instant::now());
                            if let Some(now) = media_now {
                                let est = *shared.est_latency.lock().unwrap();
                                if pts + shared.frame_dur < now + est {
                                    shared.dropped.fetch_add(1, Ordering::Relaxed);
                                    continue;
                                }
                            }
                            let mut q = shared.queue.lock().unwrap();
                            while q.len() >= max_queue && !shared.stop.load(Ordering::SeqCst) {
                                q = shared.queue_cv.wait_timeout(q, Duration::from_millis(20)).unwrap().0;
                            }
                            q.push_back((n, Decoded { pts, frame }));
                            drop(q);
                            shared.queue_cv.notify_all();
                        }
                        Ok(None) | Err(_) => break,
                    }
                }
                shared.decoded_all.store(true, Ordering::SeqCst);
                shared.queue_cv.notify_all();
                shared.results_cv.notify_all();
            })
        };

        // Processor threads.
        let mut worker_threads = Vec::new();
        for mut processor in p.processors {
            let shared = shared.clone();
            worker_threads.push(std::thread::spawn(move || {
                loop {
                    let (seq, item) = {
                        let mut q = shared.queue.lock().unwrap();
                        loop {
                            if shared.stop.load(Ordering::SeqCst) {
                                return processor;
                            }
                            if let Some(item) = pick_frame(&shared, &mut q) {
                                break item;
                            }
                            if shared.decoded_all.load(Ordering::SeqCst) && q.is_empty() {
                                drop(q);
                                shared.results_cv.notify_all();
                                return processor;
                            }
                            q = shared.queue_cv.wait_timeout(q, Duration::from_millis(30)).unwrap().0;
                        }
                    };
                    shared.in_flight.fetch_add(1, Ordering::SeqCst);
                    shared.queue_cv.notify_all(); // room for the decoder

                    let effect = shared.effect.read().unwrap().clone();
                    let started = Instant::now();
                    let result = match &effect {
                        Some(e) => {
                            let idx = shared.timeline.frame_at(item.pts + shared.frame_dur * 0.5);
                            processor.process(&item.frame, e, &shared.timeline, idx)
                        }
                        None => Ok(item.frame.clone()),
                    };
                    let took = started.elapsed().as_secs_f64();
                    {
                        let mut est = shared.est_latency.lock().unwrap();
                        *est = if *est <= 0.0 { took } else { *est * 0.8 + took * 0.2 };
                    }

                    match result {
                        Ok(fx) => {
                            if seq > shared.last_presented_seq.load(Ordering::SeqCst) {
                                let original = shared.show_original.load(Ordering::Relaxed).then_some(item.frame);
                                shared.results.lock().unwrap().insert(seq, Processed { pts: item.pts, original, effect: fx });
                            } else {
                                shared.dropped.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        Err(e) => {
                            if !shared.stop.load(Ordering::SeqCst) {
                                shared.fail(e);
                            }
                        }
                    }
                    shared.in_flight.fetch_sub(1, Ordering::SeqCst);
                    shared.results_cv.notify_all();
                    if !processor.is_alive() {
                        return processor;
                    }
                }
            }));
        }

        // Presenter thread.
        let presenter_thread = {
            let (shared, sink, out) = (shared.clone(), p.sink.clone(), p.out.clone());
            std::thread::spawn(move || present_loop(&shared, &sink, &out))
        };

        Ok(Session {
            shared,
            scale: p.scale,
            size: (w, h),
            decoder,
            decoder_thread: Some(decoder_thread),
            worker_threads,
            presenter_thread: Some(presenter_thread),
            started_at: Instant::now(),
        })
    }

    fn position(&self) -> f64 {
        *self.shared.last_pts.lock().unwrap()
    }

    fn clock(&self) -> Option<(Instant, f64)> {
        self.shared.clock.lock().unwrap().origin
    }

    fn latency(&self) -> f64 {
        *self.shared.est_latency.lock().unwrap()
    }

    fn sync(&self, audio_secs: f64) {
        let mut clock = self.shared.clock.lock().unwrap();
        let now = Instant::now();
        if let Some(media) = clock.media_time(now) {
            let drift = audio_secs - media;
            if drift.abs() > 0.04 && drift.abs() < 5.0 {
                clock.origin = Some((now, audio_secs));
            }
        }
    }

    /// Stops all threads and returns the processors so they can be reused.
    fn stop(mut self) -> (Vec<Box<dyn FrameProcessor>>, Option<String>) {
        self.shared.stop.store(true, Ordering::SeqCst);
        let _ = self.decoder.lock().unwrap().kill();
        self.shared.queue_cv.notify_all();
        self.shared.results_cv.notify_all();
        if let Some(t) = self.decoder_thread.take() {
            let _ = t.join();
        }
        let processors = self.worker_threads.drain(..).filter_map(|t| t.join().ok()).collect();
        if let Some(t) = self.presenter_thread.take() {
            let _ = t.join();
        }
        let _ = self.decoder.lock().unwrap().wait();
        let err = self.shared.error.lock().unwrap().take();
        (processors, err)
    }
}

/// Takes the next frame worth processing. Frames that would be shown too late are dropped,
/// but the newest frame is always kept so slow effects still update.
fn pick_frame(shared: &Shared, q: &mut VecDeque<(u64, Decoded)>) -> Option<(u64, Decoded)> {
    let media_now = shared.clock.lock().unwrap().media_time(Instant::now());
    if let Some(now) = media_now {
        let est = *shared.est_latency.lock().unwrap();
        while q.len() > 1 {
            let pts = q.front().map(|(_, d)| d.pts).unwrap_or(0.0);
            if pts + shared.frame_dur < now + est {
                q.pop_front();
                shared.dropped.fetch_add(1, Ordering::Relaxed);
            } else {
                break;
            }
        }
    }
    q.pop_front()
}

fn present_loop(shared: &Shared, sink: &FrameSink, out: &Output) {
    loop {
        if shared.stop.load(Ordering::SeqCst) {
            return;
        }
        let mut results = shared.results.lock().unwrap();
        let Some((&seq, first)) = results.iter().next() else {
            let finished = shared.decoded_all.load(Ordering::SeqCst)
                && shared.queue.lock().unwrap().is_empty()
                && shared.in_flight.load(Ordering::SeqCst) == 0;
            if finished {
                shared.ended.store(true, Ordering::SeqCst);
                return;
            }
            let _ = shared.results_cv.wait_timeout(results, Duration::from_millis(20)).unwrap();
            continue;
        };
        let pts = first.pts;
        let now = Instant::now();

        let deadline = {
            let mut clock = shared.clock.lock().unwrap();
            if clock.origin.is_none() {
                clock.origin = Some((now, pts));
            }
            clock.instant_for(pts).unwrap_or(now)
        };

        // Too late and a newer frame is waiting: skip this one.
        if now > deadline + Duration::from_secs_f64(shared.frame_dur) && results.len() > 1 {
            results.remove(&seq);
            shared.last_presented_seq.fetch_max(seq, Ordering::SeqCst);
            shared.dropped.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        if deadline > now {
            let wait = (deadline - now).min(Duration::from_millis(50));
            let _ = shared.results_cv.wait_timeout(results, wait).unwrap();
            continue;
        }

        let item = results.remove(&seq).unwrap();
        drop(results);
        shared.last_presented_seq.fetch_max(seq, Ordering::SeqCst);

        if let Some(orig) = &item.original {
            sink.send(FrameKind::Original, orig, item.pts);
        }
        sink.send(FrameKind::Effect, &item.effect, item.pts);
        *shared.last_pts.lock().unwrap() = item.pts;
        shared.presented.fetch_add(1, Ordering::Relaxed);

        if !shared.clock_started.swap(true, Ordering::SeqCst) {
            out.event("playback_clock_started", serde_json::json!({ "position": item.pts }));
        }
    }
}

// ---------------------------------------------------------------------------------------
// Engine: command handling, still frames, auto resolution
// ---------------------------------------------------------------------------------------

struct Engine {
    db: Arc<FilterDatabase>,
    sink: FrameSink,
    out: Output,
    info: Option<VideoInfo>,
    config: PlaybackConfig,
    effect: Option<Arc<PreparedEffect>>,
    position: f64,
    session: Option<Session>,
    pool: Vec<Box<dyn FrameProcessor>>,
    gpu_pool: Vec<Box<dyn FrameProcessor>>,
    still_worker: Option<GmicWorker>,
    still_gpu: Option<GpuProcessor>,
    /// GPU effect of the running session (`None` = CPU or no effect).
    session_gpu: Option<&'static GpuEffectInfo>,
    still_cache: Option<((i64, u32, u32), Arc<Frame>)>,
    auto_level: usize,
    last_level_change: Instant,
    /// How long the last still frame took (single worker, all threads) at `still_scale`.
    still_secs: Option<(f64, f64)>,
    looping: bool,
    stats_mark: (Instant, u64, u64),
    last_stats: Instant,
}

impl Engine {
    fn new(db: Arc<FilterDatabase>, sink: FrameSink, out: Output) -> Self {
        Self {
            db,
            sink,
            out,
            info: None,
            config: PlaybackConfig::default(),
            effect: None,
            position: 0.0,
            session: None,
            pool: Vec::new(),
            gpu_pool: Vec::new(),
            still_worker: None,
            still_gpu: None,
            session_gpu: None,
            still_cache: None,
            auto_level: 2,
            last_level_change: Instant::now(),
            still_secs: None,
            looping: true,
            stats_mark: (Instant::now(), 0, 0),
            last_stats: Instant::now(),
        }
    }

    fn run(mut self, rx: Receiver<Command>) {
        loop {
            let first = if self.session.is_some() {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            } else {
                match rx.recv() {
                    Ok(c) => Some(c),
                    Err(_) => break,
                }
            };

            // Handle everything that is queued; render at most one still afterwards.
            let mut needs_still = false;
            let mut pending: Vec<Command> = first.into_iter().collect();
            pending.extend(rx.try_iter());
            for cmd in pending {
                match cmd {
                    Command::Close => {
                        self.stop_session();
                        return;
                    }
                    other => needs_still |= self.handle(other),
                }
            }

            if self.session.is_some() {
                self.supervise();
            } else if needs_still {
                self.render_still();
                // Warm up the playback workers while the user is looking at the still,
                // so pressing Play starts immediately.
                if self.effect.is_some() && rx.is_empty() {
                    let warm = if self.gpu_info().is_some() { self.ensure_gpu_pool() } else { self.ensure_pool() };
                    if let Err(e) = warm {
                        self.error(e);
                    }
                }
            }
        }
        self.stop_session();
    }

    /// Returns true if a still frame should be (re)rendered.
    fn handle(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::Open(path) => {
                self.stop_session();
                self.still_cache = None;
                match probe_video(&path) {
                    Ok(info) => {
                        self.auto_level = auto_start_level(&info);
                        self.info = Some(info);
                        self.position = self.timeline().map(|t| t.start_secs).unwrap_or(0.0);
                        self.emit_state();
                        true
                    }
                    Err(e) => {
                        self.info = None;
                        self.error(e);
                        false
                    }
                }
            }
            Command::Configure(cfg) => {
                let old_scale = self.config.scale;
                let old_cut = (self.config.cut_start_secs, self.config.cut_end_secs);
                self.effect = match &cfg.effect {
                    Some(s) => match PreparedEffect::new(&self.db, s.clone()) {
                        Ok(e) => Some(Arc::new(e)),
                        Err(e) => {
                            self.error(e);
                            None
                        }
                    },
                    None => None,
                };
                let mode_changed = self.session.is_some() && !same_gpu(self.session_gpu, self.gpu_info());
                let restart =
                    mode_changed || cfg.scale != old_scale || (cfg.cut_start_secs, cfg.cut_end_secs) != old_cut;
                self.config = cfg;
                if let Some(tl) = self.timeline() {
                    self.position = self.position.clamp(tl.start_secs, tl.end_secs);
                }
                let needs_restart = match &self.session {
                    Some(s) => {
                        s.shared.show_original.store(self.config.show_original, Ordering::Relaxed);
                        if !restart {
                            *s.shared.effect.write().unwrap() = self.effect.clone();
                        }
                        restart
                    }
                    None => return true,
                };
                if needs_restart {
                    self.restart_seamless();
                }
                false
            }
            Command::Play { from_secs } => {
                let pos = from_secs.unwrap_or(self.position);
                self.pick_auto_level_for_play();
                self.start_session(pos);
                false
            }
            Command::Pause => {
                if let Some(s) = &self.session {
                    self.position = s.position();
                }
                self.stop_session();
                self.emit_state();
                false
            }
            Command::Seek { secs } => {
                self.position = match self.timeline() {
                    Some(tl) => secs.clamp(tl.start_secs, tl.end_secs),
                    None => secs.max(0.0),
                };
                if self.session.is_some() {
                    let pos = self.position;
                    self.start_session(pos);
                    false
                } else {
                    true
                }
            }
            Command::Sync { secs } => {
                if let Some(s) = &self.session {
                    s.sync(secs);
                }
                false
            }
            Command::SetLoop(on) => {
                self.looping = on;
                false
            }
            Command::Close => false,
        }
    }

    fn timeline(&self) -> Option<Timeline> {
        self.info
            .as_ref()
            .map(|i| Timeline::new(i.duration_secs, i.fps, self.config.cut_start_secs, self.config.cut_end_secs))
    }

    fn current_scale(&self) -> f64 {
        if self.config.scale > 0.0 { self.config.scale.clamp(0.05, 1.0) } else { AUTO_LEVELS[self.auto_level] }
    }

    fn error(&self, msg: String) {
        self.out.event("playback_error", serde_json::json!({ "message": msg }));
    }

    fn emit_state(&self) {
        let playing = self.session.is_some();
        let (w, h) = self
            .info
            .as_ref()
            .map(|i| scaled_size(i.width, i.height, self.current_scale()))
            .unwrap_or((0, 0));
        self.out.event(
            "playback_state",
            serde_json::json!({
                "playing": playing,
                "position": self.position,
                "scale": self.current_scale(),
                "auto": self.config.scale <= 0.0,
                "width": w,
                "height": h,
                "mode": self.mode_name(),
            }),
        );
    }

    fn mode_name(&self) -> &'static str {
        if self.effect.is_none() {
            "original"
        } else if self.gpu_info().is_some() {
            "gpu"
        } else {
            "cpu"
        }
    }

    /// The GPU version of the current effect, if it exists, a GPU is available and the
    /// current arguments are supported.
    fn gpu_info(&self) -> Option<&'static GpuEffectInfo> {
        let effect = self.effect.as_ref()?;
        if crate::gpu::GpuContext::is_available() { crate::gpu::processor::gpu_info_for(effect) } else { None }
    }

    /// Makes sure `GPU_PROCESSORS` GPU processors for the current effect exist.
    fn ensure_gpu_pool(&mut self) -> Result<(), String> {
        let Some(effect) = self.effect.clone() else { return Ok(()) };
        self.gpu_pool.retain(|p| p.is_alive() && p.handles(&effect));
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        while self.gpu_pool.len() < GPU_PROCESSORS {
            let p = GpuProcessor::for_effect(&effect).ok_or("GPU effect could not be created")?;
            self.gpu_pool.push(Box::new(p.fallback_threads(cores / GPU_PROCESSORS)));
        }
        Ok(())
    }

    /// Makes sure the CPU worker pool exists (workers start in parallel).
    fn ensure_pool(&mut self) -> Result<(), String> {
        self.pool.retain(|p| p.is_alive());
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let wanted = (cores / 2).clamp(1, 8);
        let threads = (cores / wanted).max(1);
        let missing = wanted.saturating_sub(self.pool.len());
        if missing == 0 {
            return Ok(());
        }
        let handles: Vec<_> = (0..missing).map(|_| std::thread::spawn(move || GmicWorker::spawn(threads))).collect();
        for h in handles {
            match h.join() {
                Ok(Ok(w)) => self.pool.push(Box::new(w)),
                Ok(Err(e)) => return Err(e),
                Err(_) => return Err("G'MIC worker start panicked".into()),
            }
        }
        Ok(())
    }

    /// Restarts playback without a visible jump in time: decoding resumes a little ahead of the
    /// running clock (by the expected processing latency) and the clock keeps going.
    fn restart_seamless(&mut self) {
        let Some(s) = &self.session else { return };
        let clock = s.clock();
        let ahead = s.latency().clamp(0.05, 2.0);
        let from = clock
            .and_then(|(t0, p0)| Some(p0 + t0.elapsed().as_secs_f64() + ahead))
            .unwrap_or_else(|| s.position());
        self.start_session_with(from, clock);
    }

    fn start_session(&mut self, from: f64) {
        self.start_session_with(from, None);
    }

    fn start_session_with(&mut self, from: f64, clock: Option<(Instant, f64)>) {
        self.stop_session();
        let Some(info) = self.info.clone() else { return };
        let Some(timeline) = self.timeline() else { return };

        let mut from = from.clamp(timeline.start_secs, timeline.end_secs);
        if from >= timeline.end_secs - timeline.duration_secs().min(0.05) {
            from = timeline.start_secs; // at the end: play from the start
        }

        let gpu = self.gpu_info();
        let gpu_ready = gpu.is_some() && self.ensure_gpu_pool().map_err(|e| eprintln!("[GPU] {e}; using the CPU")).is_ok();
        self.session_gpu = if gpu_ready { gpu } else { None };
        let processors = if gpu_ready {
            std::mem::take(&mut self.gpu_pool)
        } else if self.effect.is_some() {
            if let Err(e) = self.ensure_pool() {
                self.error(e);
                return;
            }
            std::mem::take(&mut self.pool)
        } else {
            // Passthrough still uses the pipeline (for timing); processors are not called.
            vec![Box::new(Passthrough) as Box<dyn FrameProcessor>]
        };

        match Session::start(SessionParams {
            info: &info,
            timeline,
            from_secs: from,
            scale: self.current_scale(),
            effect: self.effect.clone(),
            processors,
            show_original: self.config.show_original,
            sink: self.sink.clone(),
            out: self.out.clone(),
            clock,
        }) {
            Ok(s) => {
                self.position = from;
                self.stats_mark = (Instant::now(), 0, 0);
                self.session = Some(s);
                self.emit_state();
            }
            Err((e, processors)) => {
                self.return_processors(processors);
                self.error(e);
            }
        }
    }

    fn return_processors(&mut self, processors: Vec<Box<dyn FrameProcessor>>) {
        for p in processors {
            if p.is_alive() && p.reusable() {
                if p.is_gpu() { self.gpu_pool.push(p) } else { self.pool.push(p) }
            }
        }
    }

    fn stop_session(&mut self) {
        self.session_gpu = None;
        if let Some(s) = self.session.take() {
            self.position = s.position();
            let (processors, err) = s.stop();
            self.return_processors(processors);
            if let Some(e) = err {
                self.error(e);
            }
        }
    }

    /// Runs while playing: end of stream, errors, stats and automatic resolution.
    fn supervise(&mut self) {
        let Some(s) = &self.session else { return };
        let ended = s.shared.ended.load(Ordering::SeqCst);
        let failed = s.shared.error.lock().unwrap().is_some();
        if ended && !failed && self.looping {
            // Loop inside the engine so the GUI keeps its "playing" state; the new session
            // emits `playback_clock_started`, which re-syncs the audio to the start.
            let start = self.timeline().map(|t| t.start_secs).unwrap_or(0.0);
            self.start_session(start);
            self.out.event("playback_looped", serde_json::json!({ "position": start }));
            return;
        }
        if ended || failed {
            self.stop_session();
            self.emit_state();
            if ended {
                self.out.event("playback_ended", serde_json::json!({ "position": self.position }));
            }
            return;
        }
        if self.last_stats.elapsed() < Duration::from_millis(500) {
            return;
        }
        self.last_stats = Instant::now();

        let presented = s.shared.presented.load(Ordering::Relaxed);
        let dropped = s.shared.dropped.load(Ordering::Relaxed);
        let latency = *s.shared.est_latency.lock().unwrap();
        let position = s.position();
        let workers = s.worker_threads.len().max(1) as f64;
        let fps = 1.0 / s.shared.frame_dur;
        let (scale, size) = (s.scale, s.size);

        let (mark_t, mark_p, mark_d) = self.stats_mark;
        let dt = mark_t.elapsed().as_secs_f64().max(0.001);
        let shown_window = presented.saturating_sub(mark_p);
        let drop_window = dropped.saturating_sub(mark_d);
        self.out.event(
            "playback_stats",
            serde_json::json!({
                "fps": shown_window as f64 / dt,
                "dropped": dropped,
                "presented": presented,
                "latency_ms": latency * 1000.0,
                "scale": scale,
                "width": size.0,
                "height": size.1,
                "position": position,
                "mode": self.mode_name(),
            }),
        );
        if dt < 2.0 {
            return;
        }
        self.stats_mark = (Instant::now(), presented, dropped);

        // Automatic resolution, with a pause between changes to avoid flip-flopping.
        if self.config.scale > 0.0 || self.effect.is_none() {
            return;
        }
        let capacity = if latency > 0.0 { workers / latency } else { f64::INFINITY };
        let total = shown_window + drop_window;
        let drop_ratio = if total > 0 { drop_window as f64 / total as f64 } else { 0.0 };
        let mut new_level = self.auto_level;
        if (drop_ratio > 0.15 || capacity < fps * 0.95) && self.auto_level > 0 {
            new_level -= 1;
        } else if self.auto_level + 1 < AUTO_LEVELS.len() {
            let ratio = AUTO_LEVELS[self.auto_level] / AUTO_LEVELS[self.auto_level + 1];
            if drop_ratio < 0.02 && capacity * ratio * ratio > fps * 1.3 {
                new_level += 1;
            }
        }
        if new_level != self.auto_level && self.last_level_change.elapsed() > Duration::from_secs(3) {
            self.auto_level = new_level;
            self.last_level_change = Instant::now();
            self.restart_seamless();
        }
    }

    /// Picks the starting resolution for automatic mode from the last still-frame timing.
    /// Pool workers use fewer threads than the still worker; assume ~3x slower per frame.
    fn pick_auto_level_for_play(&mut self) {
        if self.config.scale > 0.0 || self.effect.is_none() {
            return;
        }
        let gpu = self.gpu_info().is_some();
        let (Some(info), Some((secs, at_scale))) = (self.info.as_ref(), self.still_secs) else { return };
        let fps = info.fps.max(1.0);
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        // GPU frames are serialised on one device; CPU pool workers use fewer threads each.
        let (workers, per_frame) =
            if gpu { (1.0, secs.max(1e-4)) } else { ((cores / 2).clamp(1, 8) as f64, (secs * 3.0).max(1e-4)) };
        let capacity_at = |level: f64| workers / per_frame * (at_scale / level).powi(2);
        let max_level = if gpu { AUTO_LEVELS.len() - 1 } else { auto_start_level(info).max(self.auto_level) };
        let level = (0..=max_level).rev().find(|&i| capacity_at(AUTO_LEVELS[i]) >= fps * 1.1).unwrap_or(0);
        self.auto_level = level;
        self.last_level_change = Instant::now();
    }

    /// Renders the frame at the current position (when paused or after a seek).
    fn render_still(&mut self) {
        let Some(info) = self.info.clone() else { return };
        let Some(timeline) = self.timeline() else { return };
        let started = Instant::now();
        let scale = self.current_scale();
        let (w, h) = scaled_size(info.width, info.height, scale);
        let pos = self.position;

        let key = ((pos * 1000.0).round() as i64, w, h);
        let source = match &self.still_cache {
            Some((k, f)) if *k == key => f.clone(),
            _ => match extract_frame(&info, pos, w, h) {
                Ok(f) => {
                    let f = Arc::new(f);
                    self.still_cache = Some((key, f.clone()));
                    f
                }
                Err(e) => {
                    self.error(e);
                    return;
                }
            },
        };

        let fx_started = Instant::now();
        // Baked warps need two G'MIC runs to (re)bake; for a single still frame running the
        // filter once on the CPU is faster. Playback reuses the baked map for every frame.
        let gpu = self.gpu_info().filter(|i| !i.static_only);
        if gpu.is_some() {
            let effect = self.effect.clone().unwrap();
            if !self.still_gpu.as_ref().is_some_and(|g| g.is_alive() && g.runs_on_gpu(&effect)) {
                self.still_gpu = GpuProcessor::for_effect(&effect);
            }
        }
        let result = match &self.effect {
            None => Ok((*source).clone()),
            Some(effect) if gpu.is_some() && self.still_gpu.is_some() => {
                let idx = timeline.frame_at(pos);
                FrameProcessor::process(self.still_gpu.as_mut().unwrap(), &source, effect, &timeline, idx)
            }
            Some(effect) => {
                if self.still_worker.as_ref().map(|w| w.is_dead()).unwrap_or(true) {
                    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
                    match GmicWorker::spawn(threads) {
                        Ok(w) => self.still_worker = Some(w),
                        Err(e) => {
                            self.error(e);
                            return;
                        }
                    }
                }
                let idx = timeline.frame_at(pos);
                let worker = self.still_worker.as_mut().unwrap();
                FrameProcessor::process(worker, &source, effect, &timeline, idx)
            }
        };

        if self.effect.is_some() && result.is_ok() {
            self.still_secs = Some((fx_started.elapsed().as_secs_f64(), scale));
        }
        match result {
            Ok(fx) => {
                if self.config.show_original {
                    self.sink.send(FrameKind::Original, &source, pos);
                }
                self.sink.send(FrameKind::Effect, &fx, pos);
                self.out.event(
                    "playback_still",
                    serde_json::json!({
                        "position": pos,
                        "frame_index": timeline.frame_at(pos),
                        "width": w,
                        "height": h,
                        "scale": scale,
                        "elapsed_ms": started.elapsed().as_millis() as u64,
                        "mode": self.mode_name(),
                    }),
                );
            }
            Err(e) => self.error(e),
        }
    }
}

fn same_gpu(a: Option<&'static GpuEffectInfo>, b: Option<&'static GpuEffectInfo>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => std::ptr::eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn auto_start_level(info: &VideoInfo) -> usize {
    // Start at the largest level that fits within ~960 px wide.
    let target = (960.0 / info.width.max(1) as f64).min(1.0);
    AUTO_LEVELS.iter().rposition(|&l| l <= target + 1e-6).unwrap_or(0)
}

struct Passthrough;

impl FrameProcessor for Passthrough {
    fn process(&mut self, frame: &Frame, _: &PreparedEffect, _: &Timeline, _: usize) -> Result<Frame, String> {
        Ok(frame.clone())
    }
    fn reusable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_sink::tests::{parse, Recorder};
    use std::process::Command as Proc;

    fn make_clip(dir: &std::path::Path, secs: u32, size: &str) -> Option<PathBuf> {
        let src = dir.join("clip.mp4");
        Proc::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("testsrc2=s={size}:r=25:d={secs}"))
            .args(["-pix_fmt", "yuv420p"])
            .arg(&src)
            .status()
            .ok()?
            .success()
            .then_some(src)
    }

    /// Filter database with "halftone_cpu_test": Halftone without its GPU version.
    fn test_db() -> Arc<FilterDatabase> {
        let mut db = FilterDatabase::load_default();
        let mut f = db.find_filter("Halftone").unwrap().clone();
        f.id = "halftone_cpu_test".into();
        f.name = "Halftone CPU test".into();
        f.command = format!("{} ", f.command); // not matched by the GPU registry
        db.filters.insert(0, f);
        Arc::new(db)
    }

    fn engine_with_recorder() -> (PlaybackHandle, Recorder) {
        let rec = Recorder::default();
        let sink = FrameSink::new(Some(Box::new(rec.clone())));
        let out = Output::discard();
        (PlaybackHandle::spawn(test_db(), sink, out), rec)
    }

    #[test]
    fn plays_original_in_real_time_and_ends() {
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = make_clip(dir.path(), 2, "160x90") else { return };
        let (h, rec) = engine_with_recorder();
        h.send(Command::Open(src));
        h.send(Command::SetLoop(false));
        h.send(Command::Configure(PlaybackConfig { scale: 1.0, ..Default::default() }));
        let t0 = Instant::now();
        h.send(Command::Play { from_secs: Some(0.0) });
        std::thread::sleep(Duration::from_millis(3000));
        h.send(Command::Close);
        let frames = parse(&rec.0.lock().unwrap());
        // Initial still + 50 frames of playback.
        assert!(frames.len() >= 45, "got {} frames", frames.len());
        let pts: Vec<f64> = frames.iter().skip(1).map(|f| f.3).collect();
        assert!(pts.windows(2).all(|w| w[1] > w[0]), "frames must be in order");
        assert!(t0.elapsed() >= Duration::from_millis(2000));
    }

    #[test]
    fn loops_back_to_cut_start() {
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = make_clip(dir.path(), 2, "160x90") else { return };
        let (h, rec) = engine_with_recorder();
        h.send(Command::Open(src));
        h.send(Command::Configure(PlaybackConfig {
            scale: 1.0,
            cut_start_secs: Some(0.5),
            cut_end_secs: Some(1.0),
            ..Default::default()
        }));
        h.send(Command::Play { from_secs: None });
        // The first message can be the still rendered right after Open (before the cut arrived).
        let pts_now = || parse(&rec.0.lock().unwrap()).iter().skip(1).map(|f| f.3).collect::<Vec<f64>>();
        // Wait for a loop (generous timeout: ffmpeg start-up is slow when tests run in parallel).
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_secs(6) && !pts_now().windows(2).any(|w| w[1] < w[0]) {
            std::thread::sleep(Duration::from_millis(100));
        }
        h.send(Command::Close);
        let pts = pts_now();
        assert!(pts.iter().all(|&p| (0.5 - 1e-6..=1.0).contains(&p)), "stays within the cut: {pts:?}");
        let restarts = pts.windows(2).filter(|w| w[1] < w[0]).count();
        assert!(restarts >= 1, "should loop back to the cut start, pts: {pts:?}");
    }

    #[test]
    fn slow_effect_drops_frames_but_keeps_up() {
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = make_clip(dir.path(), 3, "320x180") else { return };
        let (h, rec) = engine_with_recorder();
        h.send(Command::Open(src));
        h.send(Command::SetLoop(false));
        // A deliberately slow CPU-only effect at full resolution.
        let mut base = std::collections::HashMap::new();
        base.insert(0usize, 0.0);
        h.send(Command::Configure(PlaybackConfig {
            effect: Some(EffectSettings { filter_id: "halftone_cpu_test".into(), base_values: base, ..Default::default() }),
            scale: 1.0,
            ..Default::default()
        }));
        h.send(Command::Play { from_secs: Some(0.0) });
        std::thread::sleep(Duration::from_millis(4500));
        h.send(Command::Close);
        let frames = parse(&rec.0.lock().unwrap());
        assert!(frames.len() >= 3, "some frames must be shown, got {}", frames.len());
        let max_pts = frames.iter().map(|f| f.3).fold(0.0, f64::max);
        assert!(max_pts > 2.0, "playback must keep up with the clock (max pts {max_pts})");
    }

    #[test]
    fn gpu_effect_plays_on_gpu_and_switches_to_cpu() {
        if !crate::gpu::GpuContext::is_available() {
            eprintln!("no GPU; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = make_clip(dir.path(), 3, "320x180") else { return };
        let rec = Recorder::default();
        let sink = FrameSink::new(Some(Box::new(rec.clone())));
        let (out, events) = Output::memory();
        let h = PlaybackHandle::spawn(test_db(), sink, out);
        h.send(Command::Open(src));
        h.send(Command::SetLoop(false));
        let config = |id: &str| PlaybackConfig {
            effect: Some(EffectSettings { filter_id: id.into(), ..Default::default() }),
            scale: 1.0,
            ..Default::default()
        };
        h.send(Command::Configure(config("Halftone")));
        h.send(Command::Play { from_secs: Some(0.0) });
        std::thread::sleep(Duration::from_millis(1500));
        let gpu_frames = parse(&rec.0.lock().unwrap()).len();
        // Switching to an effect without a GPU version must restart on the CPU workers.
        h.send(Command::Configure(config("halftone_cpu_test")));
        std::thread::sleep(Duration::from_millis(1500));
        h.send(Command::Close);
        let log = String::from_utf8(events.lock().unwrap().clone()).unwrap();
        let modes: Vec<String> = log
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter(|v| v["event"] == "playback_stats")
            .filter_map(|v| v["data"]["mode"].as_str().map(String::from))
            .collect();
        // Frame counts are only a sanity check: other tests load the CPU/GPU in parallel.
        assert!(gpu_frames >= 5, "GPU halftone should play, got {gpu_frames} frames");
        assert_eq!(modes.first().map(String::as_str), Some("gpu"), "{modes:?}\n{log}");
        assert_eq!(modes.last().map(String::as_str), Some("cpu"), "{modes:?}");
    }

    #[test]
    fn seek_while_paused_renders_still_at_scale() {
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = make_clip(dir.path(), 2, "320x180") else { return };
        let (h, rec) = engine_with_recorder();
        h.send(Command::Open(src));
        h.send(Command::Configure(PlaybackConfig {
            effect: Some(EffectSettings { filter_id: "Vignette".into(), ..Default::default() }),
            scale: 0.5,
            show_original: true,
            ..Default::default()
        }));
        h.send(Command::Seek { secs: 1.0 });
        std::thread::sleep(Duration::from_millis(2500));
        h.send(Command::Close);
        let frames = parse(&rec.0.lock().unwrap());
        let last = frames.iter().rev().take(2).collect::<Vec<_>>();
        assert_eq!(last.len(), 2);
        assert!(last.iter().any(|f| f.0 == 1), "original frame sent for split view");
        assert!(last.iter().all(|f| f.1 == 160 && f.2 == 90), "{last:?}");
        assert!((last[0].3 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn auto_start_levels() {
        let mk = |w| VideoInfo {
            path: PathBuf::new(),
            filename: String::new(),
            width: w,
            height: w * 9 / 16,
            duration_secs: 1.0,
            fps: 25.0,
            fps_num: 25,
            fps_den: 1,
            frame_count: 25,
            has_audio: false,
            codec: String::new(),
        };
        assert_eq!(AUTO_LEVELS[auto_start_level(&mk(640))], 1.0);
        assert_eq!(AUTO_LEVELS[auto_start_level(&mk(1920))], 0.5);
        assert_eq!(AUTO_LEVELS[auto_start_level(&mk(3840))], 0.25);
    }
}
