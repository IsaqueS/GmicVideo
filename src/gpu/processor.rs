//! `FrameProcessor` that runs a GPU effect.
//!
//! Frames whose (animated) arguments the GPU version does not support, frames of a different
//! effect (the playback engine can swap effects mid-session) and frames after a GPU error are
//! processed by a CPU G'MIC worker instead, started on first use.

use super::effects::{find, GpuEffect, GpuEffectInfo};
use super::{run_frame, GpuContext, Targets};
use crate::effect::{PreparedEffect, Timeline};
use crate::frame::Frame;
use crate::gmic::argument_values;
use crate::gmic_worker::GmicWorker;
use crate::playback::FrameProcessor;
use std::sync::Arc;

pub struct GpuProcessor {
    ctx: Arc<GpuContext>,
    pub info: &'static GpuEffectInfo,
    effect: Box<dyn GpuEffect>,
    targets: Option<Targets>,
    scratch: Vec<u8>,
    fallback: Option<GmicWorker>,
    fallback_threads: usize,
    gpu_failed: bool,
}

impl GpuProcessor {
    /// A GPU processor for `effect`, if a GPU and a GPU version of the filter exist and the
    /// current arguments are supported.
    pub fn for_effect(effect: &PreparedEffect) -> Option<Self> {
        let info = gpu_info_for(effect)?;
        Self::from_info(info)
    }

    /// Uses `worker` (instead of a lazily started one) for frames the GPU cannot handle.
    pub fn with_fallback(mut self, worker: GmicWorker) -> Self {
        self.fallback = Some(worker);
        self
    }

    /// Threads for the lazily started CPU fallback worker.
    pub fn fallback_threads(mut self, threads: usize) -> Self {
        self.fallback_threads = threads.max(1);
        self
    }

    /// True if this processor runs `effect` on the GPU.
    pub fn runs_on_gpu(&self, effect: &PreparedEffect) -> bool {
        find(&effect.filter.command).is_some_and(|i| std::ptr::eq(i, self.info))
            && !(self.info.static_only && effect.is_animated())
    }

    /// True if `effect` would run on the GPU (without creating anything).
    pub fn would_handle(effect: &PreparedEffect) -> bool {
        gpu_info_for(effect).is_some() && GpuContext::is_available()
    }

    /// Processes a frame with explicit numeric arguments (used by tests and benchmarks).
    pub fn process_args(&mut self, frame: &Frame, args: &[f64], seed: u32) -> Result<Frame, String> {
        let (w, h) = (frame.width, frame.height);
        if self.targets.as_ref().map(|t| (t.width, t.height)) != Some((w, h)) {
            self.targets = Some(Targets::new(&self.ctx, w, h));
        }
        self.effect.prepare(&self.ctx, frame, args)?;
        let targets = self.targets.as_ref().unwrap();
        let effect = &mut self.effect;
        run_frame(&self.ctx, targets, frame, &mut self.scratch, seed, |chain| effect.encode(chain, args))
    }

    /// Processor for a registry entry.
    pub fn from_info(info: &'static GpuEffectInfo) -> Option<Self> {
        let ctx = GpuContext::get()?;
        Some(Self {
            ctx,
            info,
            effect: (info.create)(),
            targets: None,
            scratch: Vec::new(),
            fallback: None,
            fallback_threads: 2,
            gpu_failed: false,
        })
    }

    fn process_cpu(&mut self, frame: &Frame, effect: &PreparedEffect, timeline: &Timeline, idx: usize) -> Result<Frame, String> {
        if self.fallback.as_ref().is_none_or(|w| w.is_dead()) {
            self.fallback = Some(GmicWorker::spawn(self.fallback_threads)?);
        }
        let cmd = effect.command_for_frame(timeline, idx);
        self.fallback.as_mut().unwrap().process(frame, &cmd, idx as u32)
    }
}

impl FrameProcessor for GpuProcessor {
    fn process(&mut self, frame: &Frame, effect: &PreparedEffect, timeline: &Timeline, idx: usize) -> Result<Frame, String> {
        if self.gpu_failed || !self.runs_on_gpu(effect) {
            return self.process_cpu(frame, effect, timeline, idx);
        }
        let values = effect.values_at(timeline.t_of(idx), idx);
        let args = argument_values(&effect.filter, &values);
        if !(self.info.supports)(&args) {
            return self.process_cpu(frame, effect, timeline, idx);
        }
        self.effect.set_command_line(&effect.command_for_frame(timeline, idx), idx as u32);
        match self.process_args(frame, &args, idx as u32) {
            Ok(f) => Ok(f),
            Err(e) => {
                eprintln!("[GPU] {} failed ({e}); using the CPU from now on", self.info.name);
                self.gpu_failed = true;
                self.process_cpu(frame, effect, timeline, idx)
            }
        }
    }

    fn is_alive(&self) -> bool {
        !self.gpu_failed
    }

    fn is_gpu(&self) -> bool {
        true
    }

    fn handles(&self, effect: &PreparedEffect) -> bool {
        self.runs_on_gpu(effect)
    }
}

/// The GPU effect info for `effect` with its current base arguments, if any.
pub fn gpu_info_for(effect: &PreparedEffect) -> Option<&'static GpuEffectInfo> {
    let info = find(&effect.filter.command)?;
    let args = argument_values(&effect.filter, &effect.settings.base_values);
    ((info.supports)(&args) && !(info.static_only && effect.is_animated())).then_some(info)
}
