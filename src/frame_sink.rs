//! Delivery of displayable frames to the GUI.
//!
//! The GUI passes the write end of a pipe to the backend (env `GMIC_VIDEO_FRAME_FD`).
//! Each message is a 32-byte header followed by `width * height * 4` RGBX bytes:
//!
//! | offset | type | field                                  |
//! |--------|------|----------------------------------------|
//! | 0      | u32  | magic `0x52465647` ("GVFR")            |
//! | 4      | u32  | kind: 0 = effect frame, 1 = original   |
//! | 8      | u32  | width                                  |
//! | 12     | u32  | height                                 |
//! | 16     | u64  | sequence number (increases per frame)  |
//! | 24     | f64  | presentation time in seconds           |
//!
//! Keeping frames off the JSON channel means a 1080p stream (~250 MB/s) never delays
//! request/response traffic.

use crate::frame::Frame;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const FRAME_MAGIC: u32 = 0x5246_5647;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    Effect = 0,
    Original = 1,
}

/// Where displayable frames go. Cheap to clone.
#[derive(Clone)]
pub struct FrameSink {
    inner: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
    seq: Arc<AtomicU64>,
    scratch: Arc<Mutex<Vec<u8>>>,
}

impl FrameSink {
    pub fn new(writer: Option<Box<dyn Write + Send>>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(writer)),
            seq: Arc::new(AtomicU64::new(0)),
            scratch: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Uses the pipe passed by the GUI, or discards frames when there is none.
    pub fn from_env() -> Self {
        #[cfg(unix)]
        if let Some(fd) = std::env::var("GMIC_VIDEO_FRAME_FD").ok().and_then(|v| v.parse::<i32>().ok()) {
            use std::os::fd::FromRawFd;
            if fd > 2 {
                // SAFETY: the GUI created this descriptor for us and nothing else in the
                // process uses it; ownership moves into the File exactly once.
                let file = unsafe { std::fs::File::from_raw_fd(fd) };
                return Self::new(Some(Box::new(file)));
            }
        }
        Self::new(None)
    }

    /// Sends a planar frame. Returns false if the GUI end is gone.
    pub fn send(&self, kind: FrameKind, frame: &Frame, pts_secs: f64) -> bool {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        let Some(writer) = guard.as_mut() else { return false };

        let mut buf = self.scratch.lock().unwrap();
        frame.to_rgba_into(&mut buf);
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;

        let mut header = [0u8; 32];
        header[0..4].copy_from_slice(&FRAME_MAGIC.to_le_bytes());
        header[4..8].copy_from_slice(&(kind as u32).to_le_bytes());
        header[8..12].copy_from_slice(&frame.width.to_le_bytes());
        header[12..16].copy_from_slice(&frame.height.to_le_bytes());
        header[16..24].copy_from_slice(&seq.to_le_bytes());
        header[24..32].copy_from_slice(&pts_secs.to_le_bytes());

        let ok = writer.write_all(&header).and_then(|_| writer.write_all(&buf)).and_then(|_| writer.flush()).is_ok();
        if !ok {
            *guard = None; // GUI closed the pipe; stop trying.
        }
        ok
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A sink that records frames in memory (used by playback tests).
    #[derive(Clone, Default)]
    pub struct Recorder(pub Arc<Mutex<Vec<u8>>>);

    impl Write for Recorder {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Parses recorded messages into (kind, width, height, pts).
    pub fn parse(bytes: &[u8]) -> Vec<(u32, u32, u32, f64)> {
        let mut out = Vec::new();
        let mut pos = 0;
        while pos + 32 <= bytes.len() {
            let u32_at = |o: usize| u32::from_le_bytes(bytes[pos + o..pos + o + 4].try_into().unwrap());
            assert_eq!(u32_at(0), FRAME_MAGIC);
            let (kind, w, h) = (u32_at(4), u32_at(8), u32_at(12));
            let pts = f64::from_le_bytes(bytes[pos + 24..pos + 32].try_into().unwrap());
            out.push((kind, w, h, pts));
            pos += 32 + (w * h * 4) as usize;
        }
        out
    }

    #[test]
    fn header_layout() {
        let rec = Recorder::default();
        let sink = FrameSink::new(Some(Box::new(rec.clone())));
        let f = Frame::new(4, 2);
        assert!(sink.send(FrameKind::Original, &f, 1.5));
        let bytes = rec.0.lock().unwrap().clone();
        assert_eq!(bytes.len(), 32 + 4 * 2 * 4);
        assert_eq!(parse(&bytes), vec![(1, 4, 2, 1.5)]);
    }
}
