//! `fx_blur_gaussian xy_sigma,x_sigma,y_sigma,boundary,channels,value_action`
//!
//! G'MIC: `ac "_fx_blur_gaussian ...",channels,value_action` where `_fx_blur_gaussian` blurs
//! with sigma `xy` on both axes, then extra `y`/`x` blurs. Boundary 0 (transparent) adds an
//! alpha channel blurred with zero boundary; since the alpha is dropped at the end, the
//! visible result equals a Dirichlet blur of the colours.

use super::{arg, channels_supported, gaussian_blur, linear_rgb_pass, GpuEffect};
use crate::gpu::PassChain;

pub fn supports(args: &[f64]) -> bool {
    channels_supported(arg(args, 4, 7.0)) && arg(args, 5, 0.0).round() as i64 != 2
}

pub struct GaussianBlur;

impl GpuEffect for GaussianBlur {
    fn encode(&mut self, chain: &mut PassChain, args: &[f64]) -> Result<(), String> {
        let xy = arg(args, 0, 3.0).max(0.0);
        let sx = arg(args, 1, 0.0).max(0.0);
        let sy = arg(args, 2, 0.0).max(0.0);
        let boundary = arg(args, 3, 3.0).round() as i32;
        let linear = arg(args, 4, 7.0).round() as i64 == 7;

        if linear {
            linear_rgb_pass(chain, true);
        }
        // Successive gaussian blurs combine: sigma² adds up.
        let total_x = (xy * xy + sx * sx).sqrt();
        let total_y = (xy * xy + sy * sy).sqrt();
        gaussian_blur(chain, total_x, total_y, boundary);
        if linear {
            linear_rgb_pass(chain, false);
        }
        Ok(())
    }
}
