//! JSON-over-stdio IPC server used by the Python/QML GUI.
//!
//! Requests: one JSON object per line, `{"id": 7, "action": "...", "payload": {...}}`.
//! Responses carry the same `id`: `{"id": 7, "status": "ok", "data": ...}`.
//! Slow actions (previews, probing) run on their own threads, so a long preview never blocks
//! queue polling or other requests. Unsolicited messages use `{"event": "...", ...}`.

use crate::effect::{EffectSettings, PreparedEffect, Timeline};
use crate::frame::Frame;
use crate::gmic::FilterDatabase;
use crate::frame_sink::FrameSink;
use crate::gmic_worker::GmicWorker;
use crate::playback::{Command as PlaybackCommand, PlaybackConfig, PlaybackHandle};
use crate::renderer::{BatchRenderer, RenderJobRequest, RenderProgress, RenderStage};
use crate::tween::{evaluate_cubic_bezier, evaluate_curve_points, evaluate_ease, EaseType, ParameterTween};
use crate::video::{extract_frame, probe_video, scaled_size, VideoInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Rendering,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderQueueItem {
    pub id: String,
    pub name: String,
    pub video_path: String,
    pub output_path: String,
    pub format: String,
    pub crf: u32,
    pub speed_preset: String,
    pub filter_id: String,
    pub filter_name: String,
    pub status: JobStatus,
    pub progress: RenderProgress,
    pub error: Option<String>,
    pub created_at_secs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewRequest {
    pub video_path: String,
    pub time_secs: f64,
    #[serde(default)]
    pub duration_secs: f64,
    pub filter_id: String,
    #[serde(default)]
    pub base_values: HashMap<usize, f64>,
    #[serde(default)]
    pub tweens: HashMap<usize, ParameterTween>,
    #[serde(default)]
    pub text_values: HashMap<usize, String>,
    #[serde(default)]
    pub cut_start_secs: Option<f64>,
    #[serde(default)]
    pub cut_end_secs: Option<f64>,
    /// 0 (or missing) = automatic, otherwise 0.05..1.0 of the source resolution.
    #[serde(default)]
    pub preview_scale: f64,
    /// Increasing request number; older requests still waiting are skipped.
    #[serde(default)]
    pub seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "payload", rename_all = "snake_case")]
pub enum IpcRequest {
    GetEffects,
    ProbeVideo {
        path: String,
    },
    RenderPreview(PreviewRequest),
    EvaluateCurve {
        ease_type: EaseType,
        bezier_points: Option<[f64; 4]>,
        curve_points: Option<Vec<(f64, f64)>>,
        sample_count: usize,
    },
    QueueRender(RenderJobRequest),
    GetRenderQueue,
    CancelRenderJob {
        job_id: String,
    },
    ClearCompletedJobs,
    // Real-time playback (frames go through the frame pipe, state through events)
    PlaybackOpen {
        path: String,
    },
    PlaybackConfigure(PlaybackConfig),
    PlaybackPlay {
        #[serde(default)]
        from_secs: Option<f64>,
    },
    PlaybackPause,
    PlaybackSeek {
        secs: f64,
    },
    PlaybackSync {
        secs: f64,
    },
    PlaybackSetLoop {
        enabled: bool,
    },
    // Legacy single-job actions
    StartRender(RenderJobRequest),
    GetRenderProgress,
    CancelRender,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum IpcResponse {
    Ok(serde_json::Value),
    Error(String),
}

/// Thread-safe line writer for responses and events.
#[derive(Clone)]
pub struct Output(Arc<Mutex<Box<dyn Write + Send>>>);

impl Output {
    pub fn stdout() -> Self {
        Output(Arc::new(Mutex::new(Box::new(std::io::stdout()))))
    }

    #[allow(dead_code)]
    pub fn discard() -> Self {
        Output(Arc::new(Mutex::new(Box::new(std::io::sink()))))
    }

    /// Output captured in memory (tests).
    #[cfg(test)]
    pub fn memory() -> (Self, Arc<Mutex<Vec<u8>>>) {
        #[derive(Clone)]
        struct Mem(Arc<Mutex<Vec<u8>>>);
        impl Write for Mem {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let buf = Arc::new(Mutex::new(Vec::new()));
        (Output(Arc::new(Mutex::new(Box::new(Mem(buf.clone()))))), buf)
    }

    pub fn send(&self, value: &serde_json::Value) {
        let line = serde_json::to_string(value).unwrap_or_else(|_| "{}".into());
        if let Ok(mut w) = self.0.lock() {
            let _ = writeln!(w, "{line}");
            let _ = w.flush();
        }
    }

    pub fn respond(&self, id: Option<&serde_json::Value>, resp: IpcResponse) {
        let mut v = serde_json::to_value(&resp).unwrap_or_else(|_| serde_json::json!({}));
        if let (Some(id), Some(obj)) = (id, v.as_object_mut()) {
            obj.insert("id".into(), id.clone());
        }
        self.send(&v);
    }

    pub fn event(&self, name: &str, data: serde_json::Value) {
        self.send(&serde_json::json!({ "event": name, "data": data }));
    }
}

/// Still-frame preview: one warm G'MIC worker + a cache of the last decoded frame.
pub struct PreviewService {
    db: Arc<FilterDatabase>,
    worker: Mutex<Option<GmicWorker>>,
    latest_seq: AtomicU64,
    probe_cache: Mutex<HashMap<PathBuf, VideoInfo>>,
    frame_cache: Mutex<Option<((PathBuf, i64, u32, u32), Arc<Frame>)>>,
    out_dir: PathBuf,
    counter: AtomicU64,
}

impl PreviewService {
    pub fn new(db: Arc<FilterDatabase>) -> Self {
        let out_dir = std::env::temp_dir().join(format!("gmic_video_previews_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&out_dir);
        Self {
            db,
            worker: Mutex::new(None),
            latest_seq: AtomicU64::new(0),
            probe_cache: Mutex::new(HashMap::new()),
            frame_cache: Mutex::new(None),
            out_dir,
            counter: AtomicU64::new(0),
        }
    }

    pub fn probe(&self, path: &Path) -> Result<VideoInfo, String> {
        if let Some(info) = self.probe_cache.lock().unwrap().get(path) {
            return Ok(info.clone());
        }
        let info = probe_video(path)?;
        self.probe_cache.lock().unwrap().insert(path.to_path_buf(), info.clone());
        Ok(info)
    }

    /// Resolution used for previews: `0` = automatic (fit within 960 px wide).
    pub fn resolve_scale(info: &VideoInfo, requested: f64) -> f64 {
        if requested > 0.0 && requested.is_finite() {
            requested.clamp(0.05, 1.0)
        } else {
            (960.0 / info.width.max(1) as f64).min(1.0)
        }
    }

    fn source_frame(&self, info: &VideoInfo, time: f64, w: u32, h: u32) -> Result<Arc<Frame>, String> {
        let key = (info.path.clone(), (time * 1000.0).round() as i64, w, h);
        if let Some((k, f)) = self.frame_cache.lock().unwrap().as_ref() {
            if *k == key {
                return Ok(f.clone());
            }
        }
        let frame = Arc::new(extract_frame(info, time, w, h)?);
        *self.frame_cache.lock().unwrap() = Some((key, frame.clone()));
        Ok(frame)
    }

    pub fn render(&self, req: PreviewRequest) -> Result<serde_json::Value, String> {
        let started = Instant::now();
        if req.seq > 0 {
            self.latest_seq.fetch_max(req.seq, Ordering::SeqCst);
        }
        let stale = || req.seq > 0 && req.seq < self.latest_seq.load(Ordering::SeqCst);

        let info = self.probe(Path::new(&req.video_path))?;
        let effect = PreparedEffect::new(
            &self.db,
            EffectSettings {
                filter_id: req.filter_id.clone(),
                base_values: req.base_values.clone(),
                tweens: req.tweens.clone(),
                text_values: req.text_values.clone(),
            },
        )?;
        let scale = Self::resolve_scale(&info, req.preview_scale);
        let (w, h) = scaled_size(info.width, info.height, scale);

        // Same frame index / animation time mapping as the final render.
        let timeline = Timeline::new(info.duration_secs, info.fps, req.cut_start_secs, req.cut_end_secs);
        let idx = timeline.frame_at(req.time_secs);
        let t = timeline.t_of(idx);

        let mut worker_guard = self.worker.lock().unwrap();
        if stale() {
            return Ok(serde_json::json!({ "stale": true }));
        }
        let source = self.source_frame(&info, req.time_secs, w, h)?;
        if worker_guard.as_ref().map(|w| w.is_dead()).unwrap_or(true) {
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            *worker_guard = Some(GmicWorker::spawn(threads)?);
        }
        let cmd = effect.command_for_frame(&timeline, idx);
        let result = worker_guard.as_mut().unwrap().process(&source, &cmd, idx as u32);
        drop(worker_guard);
        let processed = result.map_err(|e| format!("Failed to apply G'MIC effect: {e}"))?;

        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        let raw_path = self.out_dir.join(format!("preview_raw_{n}.ppm"));
        let fx_path = self.out_dir.join(format!("preview_effect_{n}.ppm"));
        source.write_ppm(&raw_path)?;
        processed.write_ppm(&fx_path)?;
        self.cleanup_old(n);

        Ok(serde_json::json!({
            "raw_preview": raw_path.to_string_lossy(),
            "effect_preview": fx_path.to_string_lossy(),
            "time_secs": req.time_secs,
            "normalized_t": t,
            "frame_index": idx,
            "width": w,
            "height": h,
            "scale": scale,
            "elapsed_ms": started.elapsed().as_millis() as u64,
            "seq": req.seq,
        }))
    }

    /// Keeps the last few preview files (the GUI may still be loading the previous one).
    fn cleanup_old(&self, newest: u64) {
        if newest < 4 {
            return;
        }
        let old = newest - 4;
        for kind in ["raw", "effect"] {
            let _ = std::fs::remove_file(self.out_dir.join(format!("preview_{kind}_{old}.ppm")));
        }
    }
}

impl Drop for PreviewService {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.out_dir);
    }
}

pub struct ServerState {
    pub filter_db: Arc<FilterDatabase>,
    pub queue_items: Arc<Mutex<Vec<RenderQueueItem>>>,
    pub cancel_flags: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    pub active_job_id: Arc<Mutex<Option<String>>>,
    pub queue_tx: std::sync::mpsc::Sender<RenderJobRequest>,
    pub current_progress: Arc<Mutex<RenderProgress>>,
    pub preview: Arc<PreviewService>,
    pub playback: PlaybackHandle,
}

impl ServerState {
    pub fn new(out: Output) -> Self {
        let filter_db = Arc::new(FilterDatabase::load_default());
        let queue_items: Arc<Mutex<Vec<RenderQueueItem>>> = Arc::new(Mutex::new(Vec::new()));
        let cancel_flags: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>> = Arc::new(Mutex::new(HashMap::new()));
        let active_job_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let current_progress = Arc::new(Mutex::new(RenderProgress::default()));
        let (queue_tx, queue_rx) = std::sync::mpsc::channel::<RenderJobRequest>();

        // Background FIFO queue worker.
        {
            let (queue, flags, active, db, prog) = (
                queue_items.clone(),
                cancel_flags.clone(),
                active_job_id.clone(),
                filter_db.clone(),
                current_progress.clone(),
            );
            std::thread::spawn(move || {
                while let Ok(job) = queue_rx.recv() {
                    let cancel = flags
                        .lock()
                        .unwrap()
                        .entry(job.id.clone())
                        .or_insert_with(|| Arc::new(AtomicBool::new(false)))
                        .clone();
                    if cancel.load(Ordering::SeqCst) {
                        continue; // canceled while queued
                    }

                    {
                        let mut items = queue.lock().unwrap();
                        if let Some(item) = items.iter_mut().find(|i| i.id == job.id) {
                            item.status = JobStatus::Rendering;
                            item.progress.stage = RenderStage::Extracting;
                            item.progress.percent = 1.0;
                            item.progress.message = "Starting render...".to_string();
                        }
                        *active.lock().unwrap() = Some(job.id.clone());
                    }

                    let job_id = job.id.clone();
                    let (queue_cb, prog_cb) = (queue.clone(), prog.clone());
                    let on_progress = move |p: RenderProgress| {
                        if let Some(item) = queue_cb.lock().unwrap().iter_mut().find(|i| i.id == job_id) {
                            item.progress = p.clone();
                        }
                        *prog_cb.lock().unwrap() = p;
                    };

                    let result = BatchRenderer::run(job.clone(), &db, cancel.clone(), on_progress);

                    let mut items = queue.lock().unwrap();
                    *active.lock().unwrap() = None;
                    flags.lock().unwrap().remove(&job.id);
                    if let Some(item) = items.iter_mut().find(|i| i.id == job.id) {
                        match result {
                            Ok(path) => {
                                item.status = JobStatus::Completed;
                                item.progress.stage = RenderStage::Completed;
                                item.progress.percent = 100.0;
                                item.progress.message = format!("Completed: {}", path.display());
                            }
                            Err(_) if cancel.load(Ordering::SeqCst) => {
                                item.status = JobStatus::Canceled;
                                item.progress.stage = RenderStage::Canceled;
                                item.progress.message = "Render was canceled".to_string();
                            }
                            Err(e) => {
                                item.status = JobStatus::Failed;
                                item.progress.stage = RenderStage::Failed;
                                item.progress.message = format!("Failed: {e}");
                                item.error = Some(e);
                            }
                        }
                    }
                }
            });
        }

        let preview = Arc::new(PreviewService::new(filter_db.clone()));
        let playback = PlaybackHandle::spawn(filter_db.clone(), FrameSink::from_env(), out);
        Self { filter_db, queue_items, cancel_flags, active_job_id, queue_tx, current_progress, preview, playback }
    }

    fn enqueue(&self, mut job: RenderJobRequest) -> String {
        if job.id.is_empty() {
            job.id = format!(
                "job_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
            );
        }
        let filter_name = self
            .filter_db
            .find_filter(&job.filter_id)
            .map(|f| f.name.clone())
            .unwrap_or_else(|| job.filter_id.clone());
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        self.cancel_flags.lock().unwrap().insert(job.id.clone(), Arc::new(AtomicBool::new(false)));
        self.queue_items.lock().unwrap().push(RenderQueueItem {
            id: job.id.clone(),
            name: job.name.clone(),
            video_path: job.video_path.to_string_lossy().to_string(),
            output_path: job.output_path.to_string_lossy().to_string(),
            format: job.format.display_name().to_string(),
            crf: job.crf,
            speed_preset: job.speed_preset.clone(),
            filter_id: job.filter_id.clone(),
            filter_name,
            status: JobStatus::Queued,
            progress: RenderProgress::default(),
            error: None,
            created_at_secs: now_secs,
        });
        let id = job.id.clone();
        let _ = self.queue_tx.send(job);
        id
    }

    fn cancel_job(&self, job_id: &str) {
        if let Some(flag) = self.cancel_flags.lock().unwrap().get(job_id) {
            flag.store(true, Ordering::SeqCst);
        }
        let is_active = self.active_job_id.lock().unwrap().as_deref() == Some(job_id);
        if !is_active {
            let mut items = self.queue_items.lock().unwrap();
            if let Some(item) = items.iter_mut().find(|i| i.id == job_id && i.status == JobStatus::Queued) {
                item.status = JobStatus::Canceled;
                item.progress.stage = RenderStage::Canceled;
                item.progress.message = "Canceled in queue".into();
            }
        }
    }

    /// Returns true for actions that may take long and should not block the request loop.
    fn is_slow(req: &IpcRequest) -> bool {
        matches!(req, IpcRequest::RenderPreview(_) | IpcRequest::ProbeVideo { .. })
    }

    pub fn handle_request(&self, req: IpcRequest) -> IpcResponse {
        match req {
            IpcRequest::GetEffects => {
                let mut v = serde_json::to_value(&*self.filter_db).unwrap_or_default();
                // Mark filters that run on the GPU with their default settings.
                let gpu = crate::gpu::GpuContext::is_available();
                if let Some(filters) = v.get_mut("filters").and_then(|f| f.as_array_mut()) {
                    for (fv, f) in filters.iter_mut().zip(&self.filter_db.filters) {
                        let on_gpu = gpu
                            && crate::gpu::effects::find(&f.command)
                                .map(|info| (info.supports)(&crate::gmic::argument_values(f, &HashMap::new())))
                                .unwrap_or(false);
                        if let Some(o) = fv.as_object_mut() {
                            o.insert("gpu".into(), serde_json::Value::Bool(on_gpu));
                        }
                    }
                }
                IpcResponse::Ok(v)
            }
            IpcRequest::ProbeVideo { path } => match self.preview.probe(Path::new(&path)) {
                Ok(info) => IpcResponse::Ok(serde_json::to_value(info).unwrap_or_default()),
                Err(e) => IpcResponse::Error(e),
            },
            IpcRequest::RenderPreview(req) => match self.preview.render(req) {
                Ok(v) => IpcResponse::Ok(v),
                Err(e) => IpcResponse::Error(e),
            },
            IpcRequest::EvaluateCurve { ease_type, bezier_points, curve_points, sample_count } => {
                let n = sample_count.clamp(10, 10_000);
                let points: Vec<[f64; 2]> = (0..=n)
                    .map(|i| {
                        let t = i as f64 / n as f64;
                        let y = match ease_type {
                            EaseType::CustomBezier => {
                                let pts = bezier_points.unwrap_or([0.25, 0.25, 0.75, 0.75]);
                                evaluate_cubic_bezier(t, pts[0], pts[1], pts[2], pts[3])
                            }
                            _ => match curve_points {
                                Some(ref c) if !c.is_empty() => evaluate_curve_points(t, c),
                                _ => evaluate_ease(t, ease_type),
                            },
                        };
                        [t, y]
                    })
                    .collect();
                IpcResponse::Ok(serde_json::to_value(points).unwrap_or_default())
            }
            IpcRequest::QueueRender(job) => {
                let id = self.enqueue(job);
                IpcResponse::Ok(serde_json::json!({ "status": "queued", "job_id": id }))
            }
            IpcRequest::StartRender(job) => {
                let id = self.enqueue(job);
                IpcResponse::Ok(serde_json::json!({ "status": "started", "job_id": id }))
            }
            IpcRequest::GetRenderQueue => {
                let items = self.queue_items.lock().unwrap();
                IpcResponse::Ok(serde_json::to_value(&*items).unwrap_or_default())
            }
            IpcRequest::CancelRenderJob { job_id } => {
                self.cancel_job(&job_id);
                IpcResponse::Ok(serde_json::json!({ "status": "canceled", "job_id": job_id }))
            }
            IpcRequest::ClearCompletedJobs => {
                let mut items = self.queue_items.lock().unwrap();
                items.retain(|i| i.status == JobStatus::Queued || i.status == JobStatus::Rendering);
                IpcResponse::Ok(serde_json::json!({ "status": "cleared" }))
            }
            IpcRequest::PlaybackOpen { path } => {
                self.playback.send(PlaybackCommand::Open(PathBuf::from(path)));
                IpcResponse::Ok(serde_json::json!({}))
            }
            IpcRequest::PlaybackConfigure(cfg) => {
                self.playback.send(PlaybackCommand::Configure(cfg));
                IpcResponse::Ok(serde_json::json!({}))
            }
            IpcRequest::PlaybackPlay { from_secs } => {
                self.playback.send(PlaybackCommand::Play { from_secs });
                IpcResponse::Ok(serde_json::json!({}))
            }
            IpcRequest::PlaybackPause => {
                self.playback.send(PlaybackCommand::Pause);
                IpcResponse::Ok(serde_json::json!({}))
            }
            IpcRequest::PlaybackSeek { secs } => {
                self.playback.send(PlaybackCommand::Seek { secs });
                IpcResponse::Ok(serde_json::json!({}))
            }
            IpcRequest::PlaybackSync { secs } => {
                self.playback.send(PlaybackCommand::Sync { secs });
                IpcResponse::Ok(serde_json::json!({}))
            }
            IpcRequest::PlaybackSetLoop { enabled } => {
                self.playback.send(PlaybackCommand::SetLoop(enabled));
                IpcResponse::Ok(serde_json::json!({}))
            }
            IpcRequest::GetRenderProgress => {
                let guard = self.current_progress.lock().unwrap();
                IpcResponse::Ok(serde_json::to_value(&*guard).unwrap_or_default())
            }
            IpcRequest::CancelRender => {
                let active = self.active_job_id.lock().unwrap().clone();
                if let Some(id) = active {
                    self.cancel_job(&id);
                }
                let mut guard = self.current_progress.lock().unwrap();
                guard.message = "Canceling render...".into();
                IpcResponse::Ok(serde_json::json!({ "status": "canceling" }))
            }
        }
    }
}

/// Splits a request line into its optional `id` and the action.
pub fn parse_request(line: &str) -> (Option<serde_json::Value>, Result<IpcRequest, String>) {
    let mut value: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return (None, Err(format!("Invalid JSON-IPC request: {e}"))),
    };
    let id = value.as_object_mut().and_then(|o| o.remove("id"));
    let req = serde_json::from_value::<IpcRequest>(value).map_err(|e| format!("Invalid JSON-IPC request: {e}"));
    (id, req)
}

pub fn run_stdio_server() {
    let out = Output::stdout();
    let state = Arc::new(ServerState::new(out.clone()));
    let stdin = std::io::stdin();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (id, req) = parse_request(line);
        match req {
            Err(e) => out.respond(id.as_ref(), IpcResponse::Error(e)),
            Ok(req) if ServerState::is_slow(&req) => {
                let (state, out) = (state.clone(), out.clone());
                std::thread::spawn(move || {
                    let resp = state.handle_request(req);
                    out.respond(id.as_ref(), resp);
                });
            }
            Ok(req) => {
                let resp = state.handle_request(req);
                out.respond(id.as_ref(), resp);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_request_with_and_without_id() {
        let (id, req) = parse_request(r#"{"id": 5, "action": "get_render_queue"}"#);
        assert_eq!(id, Some(serde_json::json!(5)));
        assert!(matches!(req, Ok(IpcRequest::GetRenderQueue)));

        let (id, req) = parse_request(r#"{"action": "probe_video", "payload": {"path": "/x"}}"#);
        assert!(id.is_none());
        assert!(matches!(req, Ok(IpcRequest::ProbeVideo { .. })));

        let (_, req) = parse_request("not json");
        assert!(req.is_err());
    }

    #[test]
    fn preview_request_defaults() {
        let (_, req) = parse_request(
            r#"{"action":"render_preview","payload":{"video_path":"/v.mp4","time_secs":1.0,"filter_id":"x"}}"#,
        );
        match req.unwrap() {
            IpcRequest::RenderPreview(p) => {
                assert_eq!(p.preview_scale, 0.0);
                assert!(p.text_values.is_empty());
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
