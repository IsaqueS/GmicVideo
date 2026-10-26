//! FFmpeg / FFprobe helpers: probing, raw-frame decoding and streaming encoding.
//!
//! Frames travel as raw planar `gbrp` bytes (no PNG encode/decode per frame).

use crate::frame::Frame;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    H264,
    H265,
    Vp9,
    Av1,
    Lossless,
}

impl ExportFormat {
    #[allow(dead_code)]
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::H264 | ExportFormat::H265 | ExportFormat::Av1 | ExportFormat::Lossless => "mp4",
            ExportFormat::Vp9 => "webm",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ExportFormat::H264 => "H.264 (MP4)",
            ExportFormat::H265 => "H.265 / HEVC (MP4)",
            ExportFormat::Vp9 => "VP9 (WebM)",
            ExportFormat::Av1 => "AV1 (MP4)",
            ExportFormat::Lossless => "Lossless RGB (MP4)",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "h265" | "hevc" => ExportFormat::H265,
            "vp9" | "webm" => ExportFormat::Vp9,
            "av1" => ExportFormat::Av1,
            "lossless" => ExportFormat::Lossless,
            _ => ExportFormat::H264,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub path: PathBuf,
    pub filename: String,
    /// Display size (after applying rotation metadata).
    pub width: u32,
    pub height: u32,
    pub duration_secs: f64,
    pub fps: f64,
    /// Exact frame rate as a fraction, e.g. 30000/1001.
    pub fps_num: u32,
    pub fps_den: u32,
    pub frame_count: u64,
    pub has_audio: bool,
    pub codec: String,
}

impl VideoInfo {
    pub fn fps_fraction(&self) -> String {
        format!("{}/{}", self.fps_num.max(1), self.fps_den.max(1))
    }
}

pub fn probe_video(path: &Path) -> Result<VideoInfo, String> {
    if !path.exists() {
        return Err(format!("Video file not found: {}", path.display()));
    }

    let output = Command::new("ffprobe")
        .args(["-v", "quiet", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {e}"))?;

    if !output.status.success() {
        return Err(format!("ffprobe exited with error: {}", String::from_utf8_lossy(&output.stderr)));
    }

    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Failed to parse ffprobe json: {e}"))?;

    let streams = parsed["streams"].as_array().ok_or("No streams found in video")?;

    let video_stream = streams
        .iter()
        .find(|s| s["codec_type"].as_str() == Some("video") && s["disposition"]["attached_pic"].as_i64() != Some(1))
        .ok_or("No video stream found in file")?;

    let has_audio = streams.iter().any(|s| s["codec_type"].as_str() == Some("audio"));

    let mut width = video_stream["width"].as_u64().unwrap_or(0) as u32;
    let mut height = video_stream["height"].as_u64().unwrap_or(0) as u32;
    if width == 0 || height == 0 {
        return Err("Could not read the video size".into());
    }

    // Phone videos store rotation as metadata; FFmpeg auto-rotates decoded frames.
    if is_quarter_turn(video_stream) {
        std::mem::swap(&mut width, &mut height);
    }

    let codec = video_stream["codec_name"].as_str().unwrap_or("unknown").to_string();

    let (fps_num, fps_den) = ["avg_frame_rate", "r_frame_rate"]
        .iter()
        .filter_map(|k| video_stream[*k].as_str())
        .find_map(parse_fraction)
        .unwrap_or((24, 1));
    let fps = fps_num as f64 / fps_den as f64;

    let duration_secs = video_stream["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| parsed["format"]["duration"].as_str().and_then(|s| s.parse::<f64>().ok()))
        .unwrap_or(0.0);

    let frame_count = video_stream["nb_frames"]
        .as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| (duration_secs * fps).round() as u64);

    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "video.mp4".to_string());

    Ok(VideoInfo {
        path: path.to_path_buf(),
        filename,
        width,
        height,
        duration_secs,
        fps,
        fps_num,
        fps_den,
        frame_count: frame_count.max(1),
        has_audio,
        codec,
    })
}

fn is_quarter_turn(stream: &serde_json::Value) -> bool {
    let mut rotation: Option<f64> = None;
    if let Some(list) = stream["side_data_list"].as_array() {
        for sd in list {
            if let Some(r) = sd["rotation"].as_f64() {
                rotation = Some(r);
            }
        }
    }
    if rotation.is_none() {
        rotation = stream["tags"]["rotate"].as_str().and_then(|s| s.parse::<f64>().ok());
    }
    match rotation {
        Some(r) => {
            let r = r.rem_euclid(360.0).round() as i64;
            r == 90 || r == 270
        }
        None => false,
    }
}

/// Parses "30000/1001" (or "25") into a reduced, valid fraction.
fn parse_fraction(s: &str) -> Option<(u32, u32)> {
    let (n, d) = match s.split_once('/') {
        Some((n, d)) => (n.trim().parse::<u64>().ok()?, d.trim().parse::<u64>().ok()?),
        None => {
            let f = s.trim().parse::<f64>().ok()?;
            ((f * 1000.0).round() as u64, 1000)
        }
    };
    if n == 0 || d == 0 {
        return None;
    }
    let g = gcd(n, d);
    let (n, d) = (n / g, d / g);
    // Reject nonsense rates like 90000/1 from broken files.
    let fps = n as f64 / d as f64;
    if !(0.5..=240.0).contains(&fps) {
        return None;
    }
    Some((n as u32, d as u32))
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Size of frames when scaling a video by `scale` (0 < scale <= 1). Always even, at least 2.
pub fn scaled_size(width: u32, height: u32, scale: f64) -> (u32, u32) {
    let s = if scale.is_finite() { scale.clamp(0.05, 1.0) } else { 1.0 };
    if s >= 0.999 {
        return (width, height);
    }
    let even = |v: f64| (((v / 2.0).round() as u32) * 2).max(2);
    (even(width as f64 * s), even(height as f64 * s))
}

pub struct DecoderOptions<'a> {
    pub path: &'a Path,
    pub start_secs: Option<f64>,
    pub end_secs: Option<f64>,
    /// Output frame rate as "num/den" (frames are duplicated/dropped to a constant rate).
    pub fps: String,
    /// Output size; differs from the source size for reduced-resolution previews.
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
}

/// Starts FFmpeg decoding to raw `gbrp` frames of exactly `width x height` on stdout.
pub fn spawn_decoder(opts: &DecoderOptions) -> Result<Child, String> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error"]);

    let start = opts.start_secs.filter(|s| *s > 0.0005);
    if let Some(s) = start {
        cmd.arg("-ss").arg(format!("{s:.6}"));
    }
    if let Some(e) = opts.end_secs {
        if e > start.unwrap_or(0.0) + 0.0005 {
            cmd.arg("-to").arg(format!("{e:.6}"));
        }
    }
    cmd.arg("-i").arg(opts.path);

    let mut filters = vec![format!("fps={}", opts.fps)];
    if (opts.width, opts.height) != (opts.source_width, opts.source_height) {
        filters.push(format!("scale={}:{}:flags=area", opts.width, opts.height));
    }

    cmd.args(["-map", "0:v:0", "-an", "-sn", "-dn"])
        .arg("-vf")
        .arg(filters.join(","))
        .args(["-pix_fmt", "gbrp", "-f", "rawvideo", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    cmd.spawn().map_err(|e| format!("Failed to start ffmpeg decoder: {e}"))
}

/// Decodes the single frame shown at `time_secs`, scaled to `width x height`.
pub fn extract_frame(info: &VideoInfo, time_secs: f64, width: u32, height: u32) -> Result<Frame, String> {
    // Seek slightly before the end so the last frame is still found.
    let max_t = (info.duration_secs - 1.0 / info.fps.max(1.0)).max(0.0);
    let t = time_secs.clamp(0.0, max_t);

    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error"]);
    if t > 0.0005 {
        cmd.arg("-ss").arg(format!("{t:.6}"));
    }
    cmd.arg("-i").arg(&info.path).args(["-map", "0:v:0", "-frames:v", "1", "-an", "-sn"]);
    if (width, height) != (info.width, info.height) {
        cmd.arg("-vf").arg(format!("scale={width}:{height}:flags=area"));
    }
    let output = cmd
        .args(["-pix_fmt", "gbrp", "-f", "rawvideo", "pipe:1"])
        .output()
        .map_err(|e| format!("Failed to run ffmpeg: {e}"))?;

    let need = Frame::byte_len(width, height);
    if output.stdout.len() < need {
        return Err(format!(
            "ffmpeg could not decode a frame at {t:.2}s: {}",
            crate::gmic_worker::clean_stderr(&String::from_utf8_lossy(&output.stderr))
        ));
    }
    Ok(Frame::from_gbrp(width, height, &output.stdout[..need]))
}

pub struct EncoderOptions<'a> {
    pub original_video: &'a Path,
    pub out_path: &'a Path,
    pub format: ExportFormat,
    pub width: u32,
    pub height: u32,
    pub fps: String,
    pub crf: u32,
    pub has_audio: bool,
    pub speed_preset: &'a str,
    pub cut_start_secs: Option<f64>,
    pub cut_end_secs: Option<f64>,
}

/// Builds the FFmpeg arguments for the streaming encoder (exposed for tests).
pub fn encoder_args(o: &EncoderOptions) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-nostdin".into(),
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "gbrp".into(),
        "-s".into(),
        format!("{}x{}", o.width, o.height),
        "-framerate".into(),
        o.fps.clone(),
        "-i".into(),
        "pipe:0".into(),
    ];

    if o.has_audio {
        let start = o.cut_start_secs.filter(|s| *s > 0.0005);
        if let Some(s) = start {
            args.push("-ss".into());
            args.push(format!("{s:.6}"));
        }
        if let Some(e) = o.cut_end_secs {
            if e > start.unwrap_or(0.0) + 0.0005 {
                args.push("-to".into());
                args.push(format!("{e:.6}"));
            }
        }
        args.push("-i".into());
        args.push(o.original_video.to_string_lossy().to_string());
        args.extend(["-map".into(), "0:v:0".into(), "-map".into(), "1:a:0?".into()]);
        // WebM only allows Opus/Vorbis audio; MP4 gets AAC.
        if o.format == ExportFormat::Vp9 {
            args.extend(["-c:a".into(), "libopus".into(), "-b:a".into(), "160k".into()]);
        } else {
            args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "192k".into()]);
        }
        args.push("-shortest".into());
    } else {
        args.extend(["-map".into(), "0:v:0".into()]);
    }

    let preset = if o.speed_preset.is_empty() { "medium" } else { o.speed_preset };
    let pad = "pad=ceil(iw/2)*2:ceil(ih/2)*2".to_string();
    let crf = o.crf.to_string();

    let codec_args: Vec<String> = match o.format {
        ExportFormat::H264 => vec![
            "-vf", &pad, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", &crf, "-preset", preset,
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        ExportFormat::H265 => vec![
            "-vf", &pad, "-c:v", "libx265", "-pix_fmt", "yuv420p", "-crf", &crf, "-preset", preset, "-tag:v", "hvc1",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        ExportFormat::Vp9 => {
            let cpu_used = match preset {
                "ultrafast" | "superfast" | "veryfast" => "4",
                "faster" | "fast" => "3",
                "medium" => "2",
                "slow" => "1",
                _ => "0",
            };
            vec![
                "-vf", &pad, "-c:v", "libvpx-vp9", "-pix_fmt", "yuv420p", "-crf", &crf, "-b:v", "0", "-cpu-used",
                cpu_used, "-row-mt", "1",
            ]
            .into_iter()
            .map(String::from)
            .collect()
        }
        ExportFormat::Av1 => {
            let av1_preset = match preset {
                "ultrafast" | "superfast" | "veryfast" => "8",
                "faster" | "fast" => "7",
                "medium" => "6",
                "slow" => "5",
                _ => "4",
            };
            vec!["-vf", &pad, "-c:v", "libsvtav1", "-pix_fmt", "yuv420p", "-crf", &crf, "-preset", av1_preset]
                .into_iter()
                .map(String::from)
                .collect()
        }
        ExportFormat::Lossless => vec!["-c:v", "libx264rgb", "-pix_fmt", "rgb24", "-crf", "0", "-preset", "ultrafast"]
            .into_iter()
            .map(String::from)
            .collect(),
    };
    args.extend(codec_args);

    if o.format != ExportFormat::Vp9 {
        args.extend(["-movflags".into(), "+faststart".into()]);
    }
    args.push(o.out_path.to_string_lossy().to_string());
    args
}

/// Starts FFmpeg reading raw `gbrp` frames from stdin and writing the final video.
pub fn spawn_encoder(o: &EncoderOptions) -> Result<Child, String> {
    if let Some(parent) = o.out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    Command::new("ffmpeg")
        .args(encoder_args(o))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start ffmpeg encoder: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions() {
        assert_eq!(parse_fraction("30000/1001"), Some((30000, 1001)));
        assert_eq!(parse_fraction("60/2"), Some((30, 1)));
        assert_eq!(parse_fraction("25"), Some((25, 1)));
        assert_eq!(parse_fraction("0/0"), None);
        assert_eq!(parse_fraction("90000/1"), None);
    }

    #[test]
    fn scaled_sizes_are_even() {
        assert_eq!(scaled_size(1920, 1080, 1.0), (1920, 1080));
        assert_eq!(scaled_size(1920, 1080, 0.5), (960, 540));
        assert_eq!(scaled_size(971, 357, 0.25), (242, 90));
        let (w, h) = scaled_size(3, 3, 0.1);
        assert!(w >= 2 && h >= 2);
    }

    #[test]
    fn webm_uses_opus_audio() {
        let o = EncoderOptions {
            original_video: Path::new("in.mp4"),
            out_path: Path::new("out.webm"),
            format: ExportFormat::Vp9,
            width: 64,
            height: 64,
            fps: "30/1".into(),
            crf: 30,
            has_audio: true,
            speed_preset: "medium",
            cut_start_secs: None,
            cut_end_secs: None,
        };
        let args = encoder_args(&o);
        assert!(args.windows(2).any(|w| w[0] == "-c:a" && w[1] == "libopus"));
        assert!(!args.iter().any(|a| a == "aac"));
    }

    /// End-to-end: synthesize a short clip, decode it raw, encode it back.
    #[test]
    fn decode_encode_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.mp4");
        let ok = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=160x90:r=10:d=1"])
            .args(["-f", "lavfi", "-i", "sine=d=1", "-shortest", "-pix_fmt", "yuv420p"])
            .arg(&src)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("ffmpeg not available; skipping");
            return;
        }
        let info = probe_video(&src).unwrap();
        assert_eq!((info.width, info.height, info.fps_num, info.fps_den), (160, 90, 10, 1));
        assert!(info.has_audio);

        let mut dec = spawn_decoder(&DecoderOptions {
            path: &src,
            start_secs: None,
            end_secs: None,
            fps: info.fps_fraction(),
            width: 80,
            height: 46,
            source_width: 160,
            source_height: 90,
        })
        .unwrap();
        let mut out = dec.stdout.take().unwrap();
        let mut frames = Vec::new();
        while let Some(f) = Frame::read_gbrp(&mut out, 80, 46).unwrap() {
            frames.push(f);
        }
        dec.wait().unwrap();
        assert_eq!(frames.len(), 10);

        let still = extract_frame(&info, 0.5, 160, 90).unwrap();
        assert_eq!((still.width, still.height), (160, 90));
    }
}
