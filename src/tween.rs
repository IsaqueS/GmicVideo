use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationMode {
    Disabled,
    Random,
    Curve,
}

impl Default for AnimationMode {
    fn default() -> Self {
        AnimationMode::Disabled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EaseType {
    Linear,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    EaseInQuart,
    EaseOutQuart,
    EaseInOutQuart,
    EaseInSine,
    EaseOutSine,
    EaseInOutSine,
    EaseInExpo,
    EaseOutExpo,
    EaseInOutExpo,
    EaseInCirc,
    EaseOutCirc,
    EaseInOutCirc,
    EaseInBack,
    EaseOutBack,
    EaseInOutBack,
    EaseInElastic,
    EaseOutElastic,
    EaseInOutElastic,
    EaseInBounce,
    EaseOutBounce,
    EaseInOutBounce,
    CustomBezier,
}

impl Default for EaseType {
    fn default() -> Self {
        EaseType::Linear
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterTween {
    #[serde(default)]
    pub mode: AnimationMode,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub start_value: f64,
    #[serde(default)]
    pub end_value: f64,
    #[serde(default)]
    pub ease_type: EaseType,
    /// [x1, y1, x2, y2] for cubic bezier easing P0=(0,0), P1, P2, P3=(1,1)
    #[serde(default)]
    pub bezier_points: Option<[f64; 4]>,
    /// Optional arbitrary keyframe curve points: [(t, normalized_val), ...] where t in [0.0, 1.0]
    #[serde(default)]
    pub curve_points: Option<Vec<(f64, f64)>>,
    /// Random seed or frame seed tracking
    #[serde(default)]
    pub random_min: Option<f64>,
    #[serde(default)]
    pub random_max: Option<f64>,
}

impl Default for ParameterTween {
    fn default() -> Self {
        Self {
            mode: AnimationMode::Disabled,
            enabled: false,
            start_value: 0.0,
            end_value: 1.0,
            ease_type: EaseType::Linear,
            bezier_points: Some([0.25, 0.1, 0.25, 1.0]), // default ease
            curve_points: None,
            random_min: None,
            random_max: None,
        }
    }
}

impl ParameterTween {
    /// Calculate parameter value at normalized time t in [0.0, 1.0] and frame index
    pub fn evaluate_at(&self, t: f64, frame_idx: usize, param_min: f64, param_max: f64) -> f64 {
        match self.mode {
            AnimationMode::Disabled => {
                if self.enabled {
                    self.evaluate_curve(t)
                } else {
                    self.start_value
                }
            }
            AnimationMode::Random => {
                let rmin = self.random_min.unwrap_or(self.start_value.min(self.end_value)).max(param_min);
                let rmax = self.random_max.unwrap_or(self.start_value.max(self.end_value)).min(param_max);
                let (lo, hi) = if rmin <= rmax { (rmin, rmax) } else { (rmax, rmin) };
                if (hi - lo).abs() < 1e-6 {
                    return lo;
                }
                // Deterministic pseudo-random based on frame_idx and start_value bits
                let mut hash = (frame_idx as u64)
                    .wrapping_mul(0x9E3779B97F4A7C15)
                    .wrapping_add((self.start_value.to_bits() ^ self.end_value.to_bits()).wrapping_mul(0xBF58476D1CE4E5B9));
                hash = (hash ^ (hash >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
                hash = (hash ^ (hash >> 27)).wrapping_mul(0x94D049BB133111EB);
                hash = hash ^ (hash >> 31);
                let norm = (hash & 0xFFFFFFFFFFFFu64) as f64 / 281474976710655.0;
                lo + (hi - lo) * norm
            }
            AnimationMode::Curve => {
                self.evaluate_curve(t)
            }
        }
    }

    /// Legacy evaluate method
    pub fn evaluate(&self, t: f64) -> f64 {
        self.evaluate_at(t, (t * 1000.0) as usize, -1e9, 1e9)
    }

    fn evaluate_curve(&self, t: f64) -> f64 {
        let clamped_t = t.clamp(0.0, 1.0);
        let factor = match self.ease_type {
            EaseType::CustomBezier => {
                if let Some(pts) = self.bezier_points {
                    evaluate_cubic_bezier(clamped_t, pts[0], pts[1], pts[2], pts[3])
                } else {
                    clamped_t
                }
            }
            _ => {
                if let Some(ref points) = self.curve_points {
                    if !points.is_empty() {
                        evaluate_curve_points(clamped_t, points)
                    } else {
                        evaluate_ease(clamped_t, self.ease_type)
                    }
                } else {
                    evaluate_ease(clamped_t, self.ease_type)
                }
            }
        };

        self.start_value + (self.end_value - self.start_value) * factor
    }
}

pub fn evaluate_ease(t: f64, ease: EaseType) -> f64 {
    let t = t.clamp(0.0, 1.0);
    const PI: f64 = std::f64::consts::PI;

    match ease {
        EaseType::Linear => t,
        EaseType::EaseInQuad => t * t,
        EaseType::EaseOutQuad => t * (2.0 - t),
        EaseType::EaseInOutQuad => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                -1.0 + (4.0 - 2.0 * t) * t
            }
        }
        EaseType::EaseInCubic => t * t * t,
        EaseType::EaseOutCubic => {
            let f = t - 1.0;
            f * f * f + 1.0
        }
        EaseType::EaseInOutCubic => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                let f = 2.0 * t - 2.0;
                0.5 * f * f * f + 1.0
            }
        }
        EaseType::EaseInQuart => t * t * t * t,
        EaseType::EaseOutQuart => {
            let f = t - 1.0;
            1.0 - f * f * f * f
        }
        EaseType::EaseInOutQuart => {
            if t < 0.5 {
                8.0 * t * t * t * t
            } else {
                let f = t - 1.0;
                1.0 - 8.0 * f * f * f * f
            }
        }
        EaseType::EaseInSine => 1.0 - (t * PI / 2.0).cos(),
        EaseType::EaseOutSine => (t * PI / 2.0).sin(),
        EaseType::EaseInOutSine => -0.5 * ((PI * t).cos() - 1.0),
        EaseType::EaseInExpo => {
            if t == 0.0 {
                0.0
            } else {
                (2.0f64).powf(10.0 * (t - 1.0))
            }
        }
        EaseType::EaseOutExpo => {
            if t == 1.0 {
                1.0
            } else {
                1.0 - (2.0f64).powf(-10.0 * t)
            }
        }
        EaseType::EaseInOutExpo => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else if t < 0.5 {
                0.5 * (2.0f64).powf(20.0 * t - 10.0)
            } else {
                -0.5 * (2.0f64).powf(-20.0 * t + 10.0) + 1.0
            }
        }
        EaseType::EaseInCirc => 1.0 - (1.0 - t * t).max(0.0).sqrt(),
        EaseType::EaseOutCirc => (1.0 - (t - 1.0) * (t - 1.0)).max(0.0).sqrt(),
        EaseType::EaseInOutCirc => {
            if t < 0.5 {
                0.5 * (1.0 - (1.0 - 4.0 * t * t).max(0.0).sqrt())
            } else {
                0.5 * ((1.0 - (2.0 * t - 2.0).powi(2)).max(0.0).sqrt() + 1.0)
            }
        }
        EaseType::EaseInBack => {
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            c3 * t * t * t - c1 * t * t
        }
        EaseType::EaseOutBack => {
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            let f = t - 1.0;
            1.0 + c3 * f * f * f + c1 * f * f
        }
        EaseType::EaseInOutBack => {
            let c1 = 1.70158;
            let c2 = c1 * 1.525;
            if t < 0.5 {
                ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t - c2)) / 2.0
            } else {
                ((2.0 * t - 2.0).powi(2) * ((c2 + 1.0) * (t * 2.0 - 2.0) + c2) + 2.0) / 2.0
            }
        }
        EaseType::EaseInElastic => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else {
                let c4 = (2.0 * PI) / 3.0;
                -(2.0f64).powf(10.0 * t - 10.0) * ((t * 10.0 - 10.75) * c4).sin()
            }
        }
        EaseType::EaseOutElastic => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else {
                let c4 = (2.0 * PI) / 3.0;
                (2.0f64).powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
            }
        }
        EaseType::EaseInOutElastic => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else {
                let c5 = (2.0 * PI) / 4.5;
                if t < 0.5 {
                    -0.5 * (2.0f64).powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * c5).sin()
                } else {
                    0.5 * (2.0f64).powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * c5).sin() + 1.0
                }
            }
        }
        EaseType::EaseInBounce => 1.0 - evaluate_ease(1.0 - t, EaseType::EaseOutBounce),
        EaseType::EaseOutBounce => {
            let n1 = 7.5625;
            let d1 = 2.75;
            if t < 1.0 / d1 {
                n1 * t * t
            } else if t < 2.0 / d1 {
                let f = t - 1.5 / d1;
                n1 * f * f + 0.75
            } else if t < 2.5 / d1 {
                let f = t - 2.25 / d1;
                n1 * f * f + 0.9375
            } else {
                let f = t - 2.625 / d1;
                n1 * f * f + 0.984375
            }
        }
        EaseType::EaseInOutBounce => {
            if t < 0.5 {
                (1.0 - evaluate_ease(1.0 - 2.0 * t, EaseType::EaseOutBounce)) / 2.0
            } else {
                (1.0 + evaluate_ease(2.0 * t - 1.0, EaseType::EaseOutBounce)) / 2.0
            }
        }
        EaseType::CustomBezier => evaluate_cubic_bezier(t, 0.25, 0.1, 0.25, 1.0),
    }
}

/// Evaluates cubic bezier curve y given x in [0.0, 1.0] with control points (x1, y1) and (x2, y2).
/// P0 = (0, 0), P3 = (1, 1).
pub fn evaluate_cubic_bezier(x: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }

    let x1 = x1.clamp(0.0, 1.0);
    let x2 = x2.clamp(0.0, 1.0);

    // Solve for parameter s such that B_x(s) = x using Newton-Raphson + binary search fallback
    let mut s = x;
    for _ in 0..12 {
        let current_x = bezier_coord(s, x1, x2);
        let error = current_x - x;
        if error.abs() < 1e-6 {
            return bezier_coord(s, y1, y2);
        }
        let slope = bezier_derivative(s, x1, x2);
        if slope.abs() < 1e-6 {
            break;
        }
        s -= error / slope;
        s = s.clamp(0.0, 1.0);
    }

    // Binary search refinement
    let mut low = 0.0;
    let mut high = 1.0;
    s = x;
    for _ in 0..16 {
        let current_x = bezier_coord(s, x1, x2);
        if (current_x - x).abs() < 1e-5 {
            break;
        }
        if current_x < x {
            low = s;
        } else {
            high = s;
        }
        s = 0.5 * (low + high);
    }

    bezier_coord(s, y1, y2)
}

fn bezier_coord(s: f64, p1: f64, p2: f64) -> f64 {
    let one_minus_s = 1.0 - s;
    3.0 * one_minus_s * one_minus_s * s * p1
        + 3.0 * one_minus_s * s * s * p2
        + s * s * s
}

fn bezier_derivative(s: f64, p1: f64, p2: f64) -> f64 {
    let one_minus_s = 1.0 - s;
    3.0 * one_minus_s * one_minus_s * p1
        + 6.0 * one_minus_s * s * (p2 - p1)
        + 3.0 * s * s * (1.0 - p2)
}

/// Piecewise linear or Catmull-Rom interpolation through sorted keyframe points [(t, val), ...]
pub fn evaluate_curve_points(t: f64, points: &[(f64, f64)]) -> f64 {
    if points.is_empty() {
        return t;
    }
    if points.len() == 1 {
        return points[0].1;
    }

    if t <= points[0].0 {
        return points[0].1;
    }
    if t >= points[points.len() - 1].0 {
        return points[points.len() - 1].1;
    }

    // Find interval [p_i, p_{i+1}]
    for i in 0..points.len() - 1 {
        let (t0, y0) = points[i];
        let (t1, y1) = points[i + 1];
        if t >= t0 && t <= t1 {
            let span = t1 - t0;
            if span.abs() < 1e-9 {
                return y0;
            }
            let fraction = (t - t0) / span;
            // Smooth Hermite / Catmull-Rom interpolation if we have neighbor points
            let ym1 = if i > 0 { points[i - 1].1 } else { y0 - (y1 - y0) };
            let yp2 = if i + 2 < points.len() { points[i + 2].1 } else { y1 + (y1 - y0) };

            return cubic_hermite(ym1, y0, y1, yp2, fraction);
        }
    }

    points[points.len() - 1].1
}

fn cubic_hermite(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let a = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
    let b = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
    let c = -0.5 * p0 + 0.5 * p2;
    let d = p1;

    a * t * t * t + b * t * t + c * t + d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear() {
        assert_eq!(evaluate_ease(0.0, EaseType::Linear), 0.0);
        assert_eq!(evaluate_ease(0.5, EaseType::Linear), 0.5);
        assert_eq!(evaluate_ease(1.0, EaseType::Linear), 1.0);
    }

    #[test]
    fn test_bezier() {
        let v0 = evaluate_cubic_bezier(0.0, 0.42, 0.0, 0.58, 1.0);
        let v1 = evaluate_cubic_bezier(1.0, 0.42, 0.0, 0.58, 1.0);
        assert!((v0 - 0.0).abs() < 1e-4);
        assert!((v1 - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_tween_evaluation() {
        let tween = ParameterTween {
            enabled: true,
            start_value: 10.0,
            end_value: 50.0,
            ease_type: EaseType::Linear,
            bezier_points: None,
            curve_points: None,
        };
        assert_eq!(tween.evaluate(0.0), 10.0);
        assert_eq!(tween.evaluate(0.5), 30.0);
        assert_eq!(tween.evaluate(1.0), 50.0);
    }
}
