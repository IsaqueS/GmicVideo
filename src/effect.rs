//! Effect settings shared by preview, playback and final render.
//!
//! Everything that turns "which frame of the video is this?" into a G'MIC command line
//! lives here, so preview and render always agree on animated parameter values.

use crate::gmic::{build_gmic_command, FilterDatabase, GmicFilter, GmicParamType};
use crate::tween::{AnimationMode, ParameterTween};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// User-chosen effect configuration (what the GUI sends).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EffectSettings {
    pub filter_id: String,
    #[serde(default)]
    pub base_values: HashMap<usize, f64>,
    #[serde(default)]
    pub tweens: HashMap<usize, ParameterTween>,
    #[serde(default)]
    pub text_values: HashMap<usize, String>,
}

/// The part of the video being processed. Animation time `t` runs 0 → 1 across it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timeline {
    pub start_secs: f64,
    pub end_secs: f64,
    pub fps: f64,
}

impl Timeline {
    pub fn new(duration_secs: f64, fps: f64, cut_start: Option<f64>, cut_end: Option<f64>) -> Self {
        let fps = if fps > 0.0 && fps.is_finite() { fps } else { 24.0 };
        let duration = duration_secs.max(0.0);
        let start = cut_start.unwrap_or(0.0).clamp(0.0, duration);
        let mut end = cut_end.filter(|e| *e > 0.0).unwrap_or(duration).min(duration);
        if end <= start {
            end = duration.max(start);
        }
        Self { start_secs: start, end_secs: end, fps }
    }

    pub fn duration_secs(&self) -> f64 {
        (self.end_secs - self.start_secs).max(0.0)
    }

    pub fn total_frames(&self) -> usize {
        ((self.duration_secs() * self.fps).round() as usize).max(1)
    }

    /// Frame index (within the cut) shown at absolute video time `secs`.
    pub fn frame_at(&self, secs: f64) -> usize {
        let idx = ((secs - self.start_secs) * self.fps).floor().max(0.0) as usize;
        idx.min(self.total_frames() - 1)
    }

    /// Absolute video time of frame `idx`.
    #[allow(dead_code)]
    pub fn time_of(&self, idx: usize) -> f64 {
        self.start_secs + idx as f64 / self.fps
    }

    /// Animation progress 0..1 for frame `idx`.
    pub fn t_of(&self, idx: usize) -> f64 {
        let n = self.total_frames();
        if n > 1 {
            (idx as f64 / (n - 1) as f64).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// A filter resolved from the database together with its settings.
#[derive(Debug, Clone)]
pub struct PreparedEffect {
    pub filter: GmicFilter,
    pub settings: EffectSettings,
}

impl PreparedEffect {
    pub fn new(db: &FilterDatabase, settings: EffectSettings) -> Result<Self, String> {
        let filter = db
            .find_filter(&settings.filter_id)
            .ok_or_else(|| format!("Filter not found: {}", settings.filter_id))?
            .clone();
        Ok(Self { filter, settings })
    }

    /// Parameter values for frame `idx` (tweens applied on top of the base values).
    pub fn values_at(&self, t: f64, idx: usize) -> HashMap<usize, f64> {
        frame_values(&self.filter, &self.settings.base_values, &self.settings.tweens, t, idx)
    }

    /// Full G'MIC command line for frame `idx` of `timeline`.
    pub fn command_for_frame(&self, timeline: &Timeline, idx: usize) -> String {
        let values = self.values_at(timeline.t_of(idx), idx);
        build_gmic_command(&self.filter, &values, &self.settings.text_values)
    }

    /// True if any parameter changes over time (static effects can be cached).
    pub fn is_animated(&self) -> bool {
        self.settings.tweens.values().any(is_tween_active)
    }
}

fn is_tween_active(tween: &ParameterTween) -> bool {
    match tween.mode {
        AnimationMode::Disabled => tween.enabled,
        _ => true,
    }
}

/// Evaluates all tweens for one frame. `t` is the animation progress (0..1), `frame_idx`
/// seeds the Random mode.
pub fn frame_values(
    filter: &GmicFilter,
    base: &HashMap<usize, f64>,
    tweens: &HashMap<usize, ParameterTween>,
    t: f64,
    frame_idx: usize,
) -> HashMap<usize, f64> {
    let mut values = base.clone();
    for (param_id, tween) in tweens {
        if !is_tween_active(tween) {
            continue;
        }
        let (pmin, pmax) = filter
            .parameters
            .iter()
            .find(|p| p.id == *param_id)
            .map(|p| match p.param_type {
                GmicParamType::Float { min, max, .. } => (min, max),
                GmicParamType::Int { min, max, .. } => (min as f64, max as f64),
                _ => (-1e9, 1e9),
            })
            .unwrap_or((-1e9, 1e9));
        // Mix the parameter id into the seed so different parameters get different random sequences.
        let seed_idx = frame_idx.wrapping_add(param_id.wrapping_mul(7919));
        values.insert(*param_id, tween.evaluate_at(t, seed_idx, pmin, pmax));
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tween::EaseType;

    #[test]
    fn timeline_maps_cut_range() {
        let tl = Timeline::new(10.0, 10.0, Some(2.0), Some(4.0));
        assert_eq!(tl.total_frames(), 20);
        assert_eq!(tl.frame_at(2.0), 0);
        assert_eq!(tl.frame_at(3.0), 10);
        assert_eq!(tl.frame_at(100.0), 19);
        assert_eq!(tl.t_of(0), 0.0);
        assert_eq!(tl.t_of(19), 1.0);
        assert!((tl.time_of(10) - 3.0).abs() < 1e-9);
    }

    #[test]
    fn timeline_without_cut_and_bad_values() {
        let tl = Timeline::new(5.0, 0.0, None, None);
        assert_eq!(tl.fps, 24.0);
        assert_eq!(tl.total_frames(), 120);
        let tl = Timeline::new(5.0, 30.0, Some(4.0), Some(1.0));
        assert_eq!(tl.end_secs, 5.0);
    }

    #[test]
    fn preview_and_render_agree_with_cut() {
        let db = FilterDatabase::load_default();
        let vignette = db.find_filter("Halftone").unwrap().clone();
        let pid = vignette
            .parameters
            .iter()
            .find(|p| matches!(p.param_type, GmicParamType::Float { .. }))
            .unwrap()
            .id;
        let mut tweens = HashMap::new();
        tweens.insert(
            pid,
            ParameterTween {
                mode: AnimationMode::Curve,
                enabled: true,
                start_value: 0.0,
                end_value: 100.0,
                ease_type: EaseType::Linear,
                ..Default::default()
            },
        );
        let effect = PreparedEffect {
            filter: vignette,
            settings: EffectSettings { filter_id: "x".into(), tweens, ..Default::default() },
        };
        let tl = Timeline::new(10.0, 10.0, Some(5.0), Some(10.0));
        // Halfway through the cut (t = 7.5s) the value must be ~50, not 75.
        let idx = tl.frame_at(7.5);
        let v = effect.values_at(tl.t_of(idx), idx)[&pid];
        assert!((v - 50.0).abs() < 2.0, "value at middle of cut = {v}");
        assert!(effect.is_animated());
    }
}
