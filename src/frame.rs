//! In-memory video frames.
//!
//! Frames are stored as **planar RGB** (all red bytes, then all green, then all blue).
//! This is the layout G'MIC uses internally, so frames can be streamed to the G'MIC
//! workers without any per-pixel reordering. FFmpeg's matching planar format is
//! `gbrp`, which only differs in plane order (G, B, R).

use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Planar RGB: `[R plane][G plane][B plane]`, each `width * height` bytes.
    pub data: Vec<u8>,
}

impl Frame {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; width as usize * height as usize * 3],
        }
    }

    pub fn plane_len(&self) -> usize {
        self.width as usize * self.height as usize
    }

    pub fn byte_len(width: u32, height: u32) -> usize {
        width as usize * height as usize * 3
    }

    pub fn plane(&self, c: usize) -> &[u8] {
        let n = self.plane_len();
        &self.data[c * n..(c + 1) * n]
    }

    /// Reads one `gbrp` frame (FFmpeg planar order G, B, R) from a stream.
    /// Returns `Ok(None)` on a clean end of stream.
    pub fn read_gbrp<R: Read>(reader: &mut R, width: u32, height: u32) -> std::io::Result<Option<Frame>> {
        let n = width as usize * height as usize;
        let mut gbr = vec![0u8; n * 3];
        if !read_full_or_eof(reader, &mut gbr)? {
            return Ok(None);
        }
        Ok(Some(Self::from_gbrp(width, height, &gbr)))
    }

    pub fn from_gbrp(width: u32, height: u32, gbr: &[u8]) -> Frame {
        let n = width as usize * height as usize;
        let mut data = Vec::with_capacity(n * 3);
        data.extend_from_slice(&gbr[2 * n..3 * n]); // R
        data.extend_from_slice(&gbr[0..n]); // G
        data.extend_from_slice(&gbr[n..2 * n]); // B
        Frame { width, height, data }
    }

    /// Writes the frame in FFmpeg `gbrp` plane order.
    pub fn write_gbrp<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(self.plane(1))?;
        writer.write_all(self.plane(2))?;
        writer.write_all(self.plane(0))
    }

    #[allow(dead_code)]
    /// Builds a planar frame from interleaved RGBA bytes.
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Frame {
        let n = width as usize * height as usize;
        let mut data = vec![0u8; n * 3];
        let (r, rest) = data.split_at_mut(n);
        let (g, b) = rest.split_at_mut(n);
        for (i, px) in rgba.chunks_exact(4).take(n).enumerate() {
            r[i] = px[0];
            g[i] = px[1];
            b[i] = px[2];
        }
        Frame { width, height, data }
    }

    /// Interleaves the frame into RGBA bytes (alpha = 255).
    pub fn to_rgba_into(&self, out: &mut Vec<u8>) {
        let n = self.plane_len();
        out.clear();
        out.resize(n * 4, 255);
        let (r, g, b) = (self.plane(0), self.plane(1), self.plane(2));
        for (i, px) in out.chunks_exact_mut(4).enumerate() {
            px[0] = r[i];
            px[1] = g[i];
            px[2] = b[i];
        }
    }

    #[allow(dead_code)]
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.to_rgba_into(&mut out);
        out
    }

    /// Saves as binary PPM (P6). Qt reads PPM natively, and writing it is nearly free.
    pub fn write_ppm(&self, path: &Path) -> Result<(), String> {
        let n = self.plane_len();
        let mut buf = Vec::with_capacity(n * 3 + 32);
        buf.extend_from_slice(format!("P6\n{} {}\n255\n", self.width, self.height).as_bytes());
        let (r, g, b) = (self.plane(0), self.plane(1), self.plane(2));
        for i in 0..n {
            buf.push(r[i]);
            buf.push(g[i]);
            buf.push(b[i]);
        }
        let tmp = path.with_extension("ppm.part");
        std::fs::write(&tmp, &buf).map_err(|e| format!("Failed to write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("Failed to write {}: {e}", path.display()))
    }

    /// Peak signal-to-noise ratio between two frames of the same size (dB).
    #[allow(dead_code)]
    pub fn psnr(&self, other: &Frame) -> f64 {
        assert_eq!((self.width, self.height), (other.width, other.height));
        let mse: f64 = self
            .data
            .iter()
            .zip(&other.data)
            .map(|(&a, &b)| {
                let d = a as f64 - b as f64;
                d * d
            })
            .sum::<f64>()
            / self.data.len().max(1) as f64;
        if mse <= 1e-12 {
            f64::INFINITY
        } else {
            10.0 * (255.0f64 * 255.0 / mse).log10()
        }
    }
}

/// Fills `buf` completely. Returns `Ok(false)` if the stream ended before any byte was read.
pub fn read_full_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => {
                if filled == 0 {
                    return Ok(false);
                }
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    format!("stream ended mid-frame ({filled} of {} bytes)", buf.len()),
                ));
            }
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gbrp_round_trip() {
        let mut f = Frame::new(3, 2);
        for (i, v) in f.data.iter_mut().enumerate() {
            *v = i as u8;
        }
        let mut gbrp = Vec::new();
        f.write_gbrp(&mut gbrp).unwrap();
        let back = Frame::read_gbrp(&mut std::io::Cursor::new(gbrp), 3, 2).unwrap().unwrap();
        assert_eq!(f, back);
    }

    #[test]
    fn rgba_round_trip() {
        let mut f = Frame::new(4, 3);
        for (i, v) in f.data.iter_mut().enumerate() {
            *v = (i * 7) as u8;
        }
        let rgba = f.to_rgba();
        assert_eq!(rgba.len(), 4 * 3 * 4);
        assert_eq!(Frame::from_rgba(4, 3, &rgba), f);
    }

    #[test]
    fn eof_handling() {
        let mut empty = std::io::Cursor::new(Vec::<u8>::new());
        assert!(Frame::read_gbrp(&mut empty, 2, 2).unwrap().is_none());
        let mut partial = std::io::Cursor::new(vec![1u8; 5]);
        assert!(Frame::read_gbrp(&mut partial, 2, 2).is_err());
    }
}
