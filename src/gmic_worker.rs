//! Persistent G'MIC worker processes.
//!
//! Starting the `gmic` CLI costs ~300 ms (it parses its whole standard library), and the
//! old pipeline paid that for every frame, plus a PNG encode/decode on each side.
//! A `GmicWorker` starts `gmic` once and runs a small G'MIC script (`WORKER_SCRIPT`) that
//! loops forever: read a command + a raw planar frame from stdin, run the command,
//! write the raw result to stdout.
//!
//! Protocol (all integers little-endian i32):
//!   request:  [width, height, channels(=3), def_len, seed, format] + definition text + planar RGB
//!   response: [1, width, height, 3] + planar RGB                on success
//!             [0, width, height, msg_len] + error message      on failure
//! `format` 0 = uint8 values (cut to 0..255 and rounded), 1 = float32 values (unchanged; used to
//! bake warp maps, see `gpu::warp`).
//!
//! The definition text is `gv_frame :\n  <command line>`; it is (re)defined for every frame.
//! A failing effect only fails that frame; the worker keeps running.

use crate::frame::Frame;
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

/// Written for G'MIC 4 syntax: double-quoted strings are literal, substitutions go outside quotes.
const WORKER_SCRIPT: &str = r#"
gv_worker :
  do
    input raw:-,int32,6
    W={i(0)} H={i(1)} C={i(2)} L={i(3)} S={i(4)} F={i(5)}
    rm
    if !$W||!$H q fi
    input raw:-,uint8,$L
    m {-1,t}
    rm
    if $F input raw:-,float32,$W,$H,1,$C else input raw:-,uint8,$W,$H,1,$C fi
    srand $S
    ok=0
    nomsg=1
    local
      gv_frame
      ok={$!>0}
    onfail
      ok=0 nomsg=0 msg=${}
    done
    if $ok
      keep[0] to_rgb
      if {w}!=$W||{h}!=$H||{d}!=1 resize $W,$H,1,3,3 fi
      (1,$W,$H,3) output[-1] raw:-,int32 rm[-1]
      if $F output raw:-,float32 else cut 0,255 round output raw:-,uint8 fi
      rm
    else
      rm
      if $nomsg msg=No_image_produced fi
      ('E:$msg') (0,$W,$H,{w}) output[-1] raw:-,int32 rm[-1] output raw:-,uint8 rm
    fi
  while 1
"#;

fn worker_script_path() -> Result<PathBuf, String> {
    static PATH: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    PATH.get_or_init(|| {
        let path = std::env::temp_dir().join(format!("gmicvideo-worker-{}.gmic", std::process::id()));
        std::fs::write(&path, WORKER_SCRIPT)
            .map(|_| path)
            .map_err(|e| format!("Failed to write G'MIC worker script: {e}"))
    })
    .clone()
}

/// Lets another thread kill a worker that is busy (e.g. when a render is canceled).
#[derive(Clone)]
pub struct WorkerKillHandle(Arc<Mutex<Child>>);

impl WorkerKillHandle {
    pub fn kill(&self) {
        if let Ok(mut c) = self.0.lock() {
            let _ = c.kill();
        }
    }
}

pub struct GmicWorker {
    child: Arc<Mutex<Child>>,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    stderr_tail: Arc<Mutex<String>>,
    dead: bool,
}

impl GmicWorker {
    /// Starts a worker. `threads` limits G'MIC's internal OpenMP threads
    /// (use 1 when many workers run in parallel, more for a single low-latency worker).
    pub fn spawn(threads: usize) -> Result<Self, String> {
        let gmic_bin = crate::gmic::get_gmic_bin_path();
        let script = worker_script_path()?;

        let mut cmd = Command::new(&gmic_bin);
        cmd.arg("-v").arg("-");
        // Load the same filter definitions the parameter UI was built from.
        if let Some(defs) = crate::gmic::find_definitions_file() {
            cmd.arg(defs);
        }
        cmd.arg(&script)
            .arg("gv_worker")
            .env("OMP_NUM_THREADS", threads.max(1).to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to start G'MIC ({}): {e}", gmic_bin.display()))?;

        let stdin = child.stdin.take().ok_or("Failed to open G'MIC stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to open G'MIC stdout")?;
        let stderr = child.stderr.take();

        // Keep the last few KB of stderr to explain crashes.
        let stderr_tail = Arc::new(Mutex::new(String::new()));
        if let Some(mut err) = stderr {
            let tail = stderr_tail.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = err.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let text = String::from_utf8_lossy(&buf[..n]);
                    if let Ok(mut t) = tail.lock() {
                        t.push_str(&text);
                        if t.len() > 8192 {
                            let cut = t.len() - 4096;
                            let cut = (cut..t.len()).find(|&i| t.is_char_boundary(i)).unwrap_or(0);
                            t.drain(..cut);
                        }
                    }
                }
            });
        }

        Ok(Self {
            child: Arc::new(Mutex::new(child)),
            stdin: Some(stdin),
            stdout: BufReader::with_capacity(1 << 20, stdout),
            stderr_tail,
            dead: false,
        })
    }

    pub fn kill_handle(&self) -> WorkerKillHandle {
        WorkerKillHandle(self.child.clone())
    }

    /// True after an I/O failure (crash or kill). A dead worker must be replaced.
    pub fn is_dead(&self) -> bool {
        self.dead
    }

    /// Runs `command_line` (e.g. `fx_halftone 0,0,5`) on `frame`.
    /// `seed` seeds G'MIC's random generator so random effects are repeatable per frame.
    pub fn process(&mut self, frame: &Frame, command_line: &str, seed: u32) -> Result<Frame, String> {
        let (w, h) = (frame.width, frame.height);
        self.run(w, h, Payload::U8(&frame.data), command_line, seed).map(|r| match r {
            Output::U8(f) => f,
            Output::F32(..) => unreachable!(),
        })
    }

    /// Like `process`, with planar float32 values (3 channels, not clamped or rounded).
    pub fn process_f32(
        &mut self,
        width: u32,
        height: u32,
        planar: &[f32],
        command_line: &str,
        seed: u32,
    ) -> Result<(u32, u32, Vec<f32>), String> {
        assert_eq!(planar.len(), width as usize * height as usize * 3);
        self.run(width, height, Payload::F32(planar), command_line, seed).map(|r| match r {
            Output::F32(w, h, d) => (w, h, d),
            Output::U8(_) => unreachable!(),
        })
    }

    fn run(&mut self, w: u32, h: u32, payload: Payload, command_line: &str, seed: u32) -> Result<Output, String> {
        if self.dead {
            return Err("G'MIC worker is not running".into());
        }
        match self.exchange(w, h, payload, command_line, seed) {
            Ok(res) => res,
            Err(io_err) => {
                self.dead = true;
                let tail = self.stderr_tail.lock().map(|t| t.clone()).unwrap_or_default();
                let tail = clean_stderr(&tail);
                Err(if tail.is_empty() {
                    format!("G'MIC worker stopped: {io_err}")
                } else {
                    format!("G'MIC worker stopped: {io_err}: {tail}")
                })
            }
        }
    }

    /// Outer `Result` = I/O failure (worker is dead); inner = effect failure (worker is fine).
    fn exchange(
        &mut self,
        width: u32,
        height: u32,
        payload: Payload,
        command_line: &str,
        seed: u32,
    ) -> std::io::Result<Result<Output, String>> {
        let definition = format!("gv_frame :\n  {}\n", command_line.replace(['\n', '\r'], " "));
        let stdin = self.stdin.as_mut().ok_or_else(|| std::io::Error::other("stdin closed"))?;
        let is_f32 = matches!(payload, Payload::F32(_));

        let mut header = Vec::with_capacity(24);
        let fields =
            [width as i32, height as i32, 3, definition.len() as i32, (seed & 0x7fff_ffff) as i32, is_f32 as i32];
        for v in fields {
            header.extend_from_slice(&v.to_le_bytes());
        }
        stdin.write_all(&header)?;
        stdin.write_all(definition.as_bytes())?;
        match payload {
            Payload::U8(d) => stdin.write_all(d)?,
            Payload::F32(d) => stdin.write_all(bytemuck::cast_slice(d))?,
        }
        stdin.flush()?;

        let mut resp = [0u8; 16];
        self.stdout.read_exact(&mut resp)?;
        let field = |i: usize| i32::from_le_bytes([resp[i * 4], resp[i * 4 + 1], resp[i * 4 + 2], resp[i * 4 + 3]]);
        let (status, w, h, n) = (field(0), field(1), field(2), field(3));

        if status == 1 {
            if w <= 0 || h <= 0 || n != 3 {
                return Err(std::io::Error::other(format!("bad G'MIC response header {w}x{h}x{n}")));
            }
            if is_f32 {
                let mut out = vec![0f32; w as usize * h as usize * 3];
                self.stdout.read_exact(bytemuck::cast_slice_mut(&mut out))?;
                return Ok(Ok(Output::F32(w as u32, h as u32, out)));
            }
            let mut out = Frame::new(w as u32, h as u32);
            self.stdout.read_exact(&mut out.data)?;
            Ok(Ok(Output::U8(out)))
        } else {
            let len = n.clamp(0, 1 << 20) as usize;
            let mut msg = vec![0u8; len];
            self.stdout.read_exact(&mut msg)?;
            let msg = String::from_utf8_lossy(&msg);
            let msg = msg.strip_prefix("E:").unwrap_or(&msg).replace("No_image_produced", "The effect produced no image");
            Ok(Err(clean_stderr(&msg)))
        }
    }
}

enum Payload<'a> {
    U8(&'a [u8]),
    F32(&'a [f32]),
}

enum Output {
    U8(Frame),
    F32(u32, u32, Vec<f32>),
}

impl Drop for GmicWorker {
    fn drop(&mut self) {
        // Closing stdin makes the worker loop exit on its own; kill as a fallback.
        drop(self.stdin.take());
        if let Ok(mut c) = self.child.lock() {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
            loop {
                match c.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if std::time::Instant::now() < deadline => {
                        std::thread::sleep(std::time::Duration::from_millis(5))
                    }
                    _ => {
                        let _ = c.kill();
                        let _ = c.wait();
                        break;
                    }
                }
            }
        }
    }
}

/// Strips ANSI colors, the harmless libcurl loader warning, and G'MIC's "*** Error in ..." prefix.
pub fn clean_stderr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Skip "ESC [ ... letter"
            if chars.peek() == Some(&'[') {
                chars.next();
                for c2 in chars.by_ref() {
                    if c2.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    let lines: Vec<String> = out
        .lines()
        .filter(|l| !l.contains("no version information available") && !l.trim().is_empty())
        .map(|l| {
            let l = l.trim();
            match l.rfind("*** ") {
                Some(i) if l.starts_with("[gmic]") || l.starts_with("***") => l[i + 4..].trim().to_string(),
                _ => l.to_string(),
            }
        })
        .collect();
    lines.join(" ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_frame(w: u32, h: u32) -> Frame {
        let mut f = Frame::new(w, h);
        let n = f.plane_len();
        for i in 0..n {
            let (x, y) = ((i % w as usize) as u32, (i / w as usize) as u32);
            f.data[i] = (x * 255 / w) as u8;
            f.data[n + i] = (y * 255 / h) as u8;
            f.data[2 * n + i] = ((x + y) % 256) as u8;
        }
        f
    }

    #[test]
    fn worker_identity_mirror_and_errors() {
        let mut w = GmicWorker::spawn(2).expect("spawn worker");
        let f = test_frame(64, 48);

        let same = w.process(&f, "skip 0", 1).expect("identity");
        assert_eq!(same, f);

        let mirrored = w.process(&f, "mirror x", 1).expect("mirror");
        assert_eq!(mirrored.data[0], f.data[63], "pixel (0,0) must equal original (63,0)");

        let err = w.process(&f, "this_command_does_not_exist 1", 1).unwrap_err();
        assert!(err.contains("this_command_does_not_exist"), "got: {err}");
        assert!(!w.is_dead(), "an effect error must not kill the worker");

        let resized = w.process(&f, "resize 50%,50%", 1).expect("resize");
        assert_eq!((resized.width, resized.height), (64, 48), "output is scaled back to input size");

        let empty = w.process(&f, "rm", 1).unwrap_err();
        assert!(empty.contains("no image"), "got: {empty}");

        // Still alive after errors.
        assert_eq!(w.process(&f, "skip 0", 1).unwrap(), f);
    }

    #[test]
    fn float_mode_keeps_values() {
        let mut w = GmicWorker::spawn(1).expect("spawn worker");
        let data: Vec<f32> = (0..4 * 2 * 3).map(|i| i as f32 * 100.5 - 300.25).collect();
        let (ow, oh, out) = w.process_f32(4, 2, &data, "skip 0", 0).unwrap();
        assert_eq!((ow, oh), (4, 2));
        assert_eq!(out, data, "float values are passed through unclamped");
        let (_, _, out) = w.process_f32(4, 2, &data, "mirror x", 0).unwrap();
        assert_eq!(out[0], data[3]);
        // uint8 mode still works on the same worker afterwards.
        let f = test_frame(8, 8);
        assert_eq!(w.process(&f, "skip 0", 0).unwrap(), f);
    }

    #[test]
    fn seeded_random_is_repeatable() {
        let mut w = GmicWorker::spawn(1).expect("spawn worker");
        let f = test_frame(32, 32);
        let a = w.process(&f, "noise 30", 7).unwrap();
        let b = w.process(&f, "noise 30", 7).unwrap();
        let c = w.process(&f, "noise 30", 8).unwrap();
        assert_eq!(a, b, "same seed -> same output");
        assert_ne!(a, c, "different seed -> different output");
    }

    #[test]
    fn kill_handle_unblocks_worker() {
        let mut w = GmicWorker::spawn(1).expect("spawn worker");
        let handle = w.kill_handle();
        handle.kill();
        let f = test_frame(16, 16);
        assert!(w.process(&f, "skip 0", 0).is_err());
        assert!(w.is_dead());
    }
}
