use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub path: PathBuf,
    pub filename: String,
    pub width: u32,
    pub height: u32,
    pub duration_secs: f64,
    pub fps: f64,
    pub frame_count: u64,
    pub has_audio: bool,
    pub codec: String,
}

pub fn probe_video(path: &Path) -> Result<VideoInfo, String> {
    if !path.exists() {
        return Err(format!("Video file not found: {}", path.display()));
    }

    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            path.to_str().ok_or("Invalid path string")?,
        ])
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe exited with error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse ffprobe json: {e}"))?;

    let streams = parsed["streams"]
        .as_array()
        .ok_or("No streams found in video")?;

    let video_stream = streams
        .iter()
        .find(|s| s["codec_type"].as_str() == Some("video"))
        .ok_or("No video stream found in file")?;

    let has_audio = streams
        .iter()
        .any(|s| s["codec_type"].as_str() == Some("audio"));

    let width = video_stream["width"].as_u64().unwrap_or(1920) as u32;
    let height = video_stream["height"].as_u64().unwrap_or(1080) as u32;
    let codec = video_stream["codec_name"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    let fps_str = video_stream["avg_frame_rate"]
        .as_str()
        .or_else(|| video_stream["r_frame_rate"].as_str())
        .unwrap_or("24/1");

    let fps = parse_fraction(fps_str).unwrap_or(24.0);

    let duration_secs = video_stream["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| {
            parsed["format"]["duration"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
        })
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
        fps: if fps > 0.0 { fps } else { 24.0 },
        frame_count: if frame_count > 0 { frame_count } else { 1 },
        has_audio,
        codec,
    })
}

fn parse_fraction(s: &str) -> Option<f64> {
    if let Some((num, den)) = s.split_once('/') {
        let n: f64 = num.parse().ok()?;
        let d: f64 = den.parse().ok()?;
        if d > 0.0 {
            Some(n / d)
        } else {
            None
        }
    } else {
        s.parse::<f64>().ok()
    }
}

pub fn extract_preview_frame(video_path: &Path, time_secs: f64, out_path: &Path) -> Result<(), String> {
    if let Some(parent) = out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-nostats".into(),
        "-loglevel".into(),
        "error".into(),
    ];

    if time_secs > 0.001 {
        args.push("-ss".into());
        args.push(format!("{:.3}", time_secs));
    }

    args.extend([
        "-i".into(),
        video_path.to_str().ok_or("Invalid video path")?.to_string(),
        "-vframes".into(),
        "1".into(),
        "-vf".into(),
        "scale='min(960,iw)':-2".into(),
        "-pix_fmt".into(),
        "rgb24".into(),
        "-q:v".into(),
        "2".into(),
        out_path.to_str().ok_or("Invalid output path")?.to_string(),
    ]);

    let output = Command::new("ffmpeg")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to spawn ffmpeg for preview: {e}"))?;

    if output.status.success() && out_path.exists() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("ffmpeg failed to extract preview frame: {err}"))
    }
}

#[allow(dead_code)]
pub fn extract_all_frames(
    video_path: &Path,
    out_dir: &Path,
    cut_start_secs: Option<f64>,
    cut_end_secs: Option<f64>,
) -> Result<usize, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("Failed to create out dir: {e}"))?;

    let pattern = out_dir.join("frame_%07d.png");
    let mut args: Vec<String> = vec![
        "-y".into(),
        "-loglevel".into(),
        "error".into(),
    ];

    if let Some(start) = cut_start_secs {
        if start > 0.001 {
            args.push("-ss".into());
            args.push(format!("{:.3}", start));
        }
    }

    if let Some(end) = cut_end_secs {
        if let Some(start) = cut_start_secs {
            if end > start + 0.001 {
                args.push("-to".into());
                args.push(format!("{:.3}", end));
            }
        } else if end > 0.001 {
            args.push("-to".into());
            args.push(format!("{:.3}", end));
        }
    }

    args.push("-i".into());
    args.push(video_path.to_str().ok_or("Invalid video path")?.to_string());
    args.push("-vsync".into());
    args.push("0".into());
    args.push("-pix_fmt".into());
    args.push("rgb24".into());
    args.push(pattern.to_str().ok_or("Invalid pattern path")?.to_string());

    let output = Command::new("ffmpeg")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run ffmpeg frame extraction: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg extraction exited with error: {err}"));
    }

    let count = std::fs::read_dir(out_dir)
        .map_err(|e| format!("Failed to read frame dir: {e}"))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "png")
                .unwrap_or(false)
        })
        .count();

    Ok(count)
}

pub const PNG_MAGIC: &[u8; 8] = &[137, 80, 78, 71, 13, 10, 26, 10];

/// Reads a single, complete PNG file from a streaming reader.
/// Returns Ok(None) cleanly if EOF is reached before the start of a frame.
pub fn read_png_frame<R: std::io::Read>(reader: &mut R) -> Result<Option<Vec<u8>>, String> {
    let mut sig = [0u8; 8];
    let mut first_byte = [0u8; 1];
    match reader.read_exact(&mut first_byte) {
        Ok(()) => {
            sig[0] = first_byte[0];
        }
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Ok(None);
        }
        Err(e) => return Err(format!("Failed to read frame signature: {e}")),
    }

    if let Err(e) = reader.read_exact(&mut sig[1..]) {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            return Ok(None);
        }
        return Err(format!("Incomplete PNG signature: {e}"));
    }

    if &sig != PNG_MAGIC {
        return Err(format!("Invalid PNG magic header: {:?}", &sig));
    }

    let mut frame_bytes = Vec::with_capacity(256 * 1024);
    frame_bytes.extend_from_slice(&sig);

    loop {
        let mut header = [0u8; 8]; // 4 bytes length + 4 bytes chunk type
        reader.read_exact(&mut header).map_err(|e| format!("Failed to read chunk header: {e}"))?;
        frame_bytes.extend_from_slice(&header);

        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let chunk_type = &header[4..8];

        if length > 0 {
            let start = frame_bytes.len();
            frame_bytes.resize(start + length, 0);
            reader
                .read_exact(&mut frame_bytes[start..start + length])
                .map_err(|e| format!("Failed to read chunk data: {e}"))?;
        }

        let mut crc = [0u8; 4];
        reader.read_exact(&mut crc).map_err(|e| format!("Failed to read chunk crc: {e}"))?;
        frame_bytes.extend_from_slice(&crc);

        if chunk_type == b"IEND" {
            break;
        }
    }

    Ok(Some(frame_bytes))
}

/// Spawns an FFmpeg decoder subprocess that streams extracted PNG frames to stdout pipe.
pub fn spawn_video_decoder(
    video_path: &Path,
    cut_start_secs: Option<f64>,
    cut_end_secs: Option<f64>,
) -> Result<std::process::Child, String> {
    let mut args: Vec<String> = vec![
        "-y".into(),
        "-loglevel".into(),
        "error".into(),
    ];

    if let Some(start) = cut_start_secs {
        if start > 0.001 {
            args.push("-ss".into());
            args.push(format!("{:.3}", start));
        }
    }

    if let Some(end) = cut_end_secs {
        if let Some(start) = cut_start_secs {
            if end > start + 0.001 {
                args.push("-to".into());
                args.push(format!("{:.3}", end));
            }
        } else if end > 0.001 {
            args.push("-to".into());
            args.push(format!("{:.3}", end));
        }
    }

    args.push("-i".into());
    args.push(video_path.to_str().ok_or("Invalid video path")?.to_string());
    args.push("-vsync".into());
    args.push("0".into());
    args.push("-pix_fmt".into());
    args.push("rgb24".into());
    args.push("-f".into());
    args.push("image2pipe".into());
    args.push("-vcodec".into());
    args.push("png".into());
    args.push("pipe:1".into());

    Command::new("ffmpeg")
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn ffmpeg decoder: {e}"))
}

/// Spawns an FFmpeg streaming encoder subprocess that reads PNG frames from stdin pipe with zero disk files.
pub fn spawn_video_streaming_encoder(
    original_video: &Path,
    out_path: &Path,
    format: ExportFormat,
    fps: f64,
    crf: u32,
    has_audio: bool,
    speed_preset: &str,
    cut_start_secs: Option<f64>,
    cut_end_secs: Option<f64>,
) -> Result<std::process::Child, String> {
    if let Some(parent) = out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-nostats".into(),
        "-loglevel".into(),
        "error".into(),
        "-f".into(),
        "image2pipe".into(),
        "-vcodec".into(),
        "png".into(),
        "-framerate".into(),
        format!("{:.3}", fps),
        "-i".into(),
        "pipe:0".into(),
    ];

    if has_audio {
        if let Some(start) = cut_start_secs {
            if start > 0.001 {
                args.push("-ss".into());
                args.push(format!("{:.3}", start));
            }
        }
        if let Some(end) = cut_end_secs {
            if let Some(start) = cut_start_secs {
                if end > start + 0.001 {
                    args.push("-to".into());
                    args.push(format!("{:.3}", end));
                }
            } else if end > 0.001 {
                args.push("-to".into());
                args.push(format!("{:.3}", end));
            }
        }
        args.push("-i".into());
        args.push(original_video.to_string_lossy().to_string());
        args.push("-map".into());
        args.push("0:v:0".into());
        args.push("-map".into());
        args.push("1:a?".into());
        args.push("-c:a".into());
        args.push("aac".into());
        args.push("-shortest".into());
    }

    let clean_preset = if speed_preset.is_empty() { "medium" } else { speed_preset };
    let pad_filter = "pad=ceil(iw/2)*2:ceil(ih/2)*2";

    match format {
        ExportFormat::H264 => {
            args.extend([
                "-vf".into(),
                pad_filter.into(),
                "-c:v".into(),
                "libx264".into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
                "-crf".into(),
                crf.to_string(),
                "-preset".into(),
                clean_preset.into(),
            ]);
        }
        ExportFormat::H265 => {
            args.extend([
                "-vf".into(),
                pad_filter.into(),
                "-c:v".into(),
                "libx265".into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
                "-crf".into(),
                crf.to_string(),
                "-preset".into(),
                clean_preset.into(),
            ]);
        }
        ExportFormat::Vp9 => {
            let cpu_used = match clean_preset {
                "ultrafast" | "superfast" | "veryfast" => "4",
                "faster" | "fast" => "3",
                "medium" => "2",
                "slow" => "1",
                _ => "0",
            };
            args.extend([
                "-vf".into(),
                pad_filter.into(),
                "-c:v".into(),
                "libvpx-vp9".into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
                "-crf".into(),
                crf.to_string(),
                "-b:v".into(),
                "0".into(),
                "-cpu-used".into(),
                cpu_used.into(),
            ]);
        }
        ExportFormat::Av1 => {
            let av1_preset = match clean_preset {
                "ultrafast" | "superfast" | "veryfast" => "8",
                "faster" | "fast" => "7",
                "medium" => "6",
                "slow" => "5",
                _ => "4",
            };
            args.extend([
                "-vf".into(),
                pad_filter.into(),
                "-c:v".into(),
                "libsvtav1".into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
                "-crf".into(),
                crf.to_string(),
                "-preset".into(),
                av1_preset.into(),
            ]);
        }
        ExportFormat::Lossless => {
            args.extend([
                "-c:v".into(),
                "libx264rgb".into(),
                "-crf".into(),
                "0".into(),
                "-preset".into(),
                "ultrafast".into(),
            ]);
        }
    }

    args.extend([
        "-progress".into(),
        "pipe:1".into(),
        "-stats_period".into(),
        "0.2".into(),
    ]);

    args.push(out_path.to_string_lossy().to_string());

    Command::new("ffmpeg")
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn ffmpeg encoder: {e}"))
}

#[allow(dead_code)]
pub fn encode_video<F>(
    frames_dir: &Path,
    original_video: &Path,
    out_path: &Path,
    format: ExportFormat,
    fps: f64,
    crf: u32,
    has_audio: bool,
    speed_preset: &str,
    cut_start_secs: Option<f64>,
    cut_end_secs: Option<f64>,
    mut on_progress: Option<F>,
) -> Result<PathBuf, String>
where
    F: FnMut(usize, f64),
{
    if let Some(parent) = out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let input_pattern = frames_dir.join("frame_%07d.png");

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-nostats".into(),
        "-loglevel".into(),
        "error".into(),
        "-framerate".into(),
        format!("{:.3}", fps),
        "-i".into(),
        input_pattern.to_string_lossy().to_string(),
    ];

    if has_audio {
        if let Some(start) = cut_start_secs {
            if start > 0.001 {
                args.push("-ss".into());
                args.push(format!("{:.3}", start));
            }
        }
        if let Some(end) = cut_end_secs {
            if let Some(start) = cut_start_secs {
                if end > start + 0.001 {
                    args.push("-to".into());
                    args.push(format!("{:.3}", end));
                }
            } else if end > 0.001 {
                args.push("-to".into());
                args.push(format!("{:.3}", end));
            }
        }
        args.push("-i".into());
        args.push(original_video.to_string_lossy().to_string());
        args.push("-map".into());
        args.push("0:v:0".into());
        args.push("-map".into());
        args.push("1:a?".into());
        args.push("-c:a".into());
        args.push("aac".into());
    }

    if has_audio {
        args.push("-shortest".into());
    }

    let clean_preset = if speed_preset.is_empty() { "medium" } else { speed_preset };
    let pad_filter = "pad=ceil(iw/2)*2:ceil(ih/2)*2";

    match format {
        ExportFormat::H264 => {
            args.extend([
                "-vf".into(),
                pad_filter.into(),
                "-c:v".into(),
                "libx264".into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
                "-crf".into(),
                crf.to_string(),
                "-preset".into(),
                clean_preset.into(),
            ]);
        }
        ExportFormat::H265 => {
            args.extend([
                "-vf".into(),
                pad_filter.into(),
                "-c:v".into(),
                "libx265".into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
                "-crf".into(),
                crf.to_string(),
                "-preset".into(),
                clean_preset.into(),
            ]);
        }
        ExportFormat::Vp9 => {
            let cpu_used = match clean_preset {
                "ultrafast" | "superfast" | "veryfast" => "4",
                "faster" | "fast" => "3",
                "medium" => "2",
                "slow" => "1",
                _ => "0",
            };
            args.extend([
                "-vf".into(),
                pad_filter.into(),
                "-c:v".into(),
                "libvpx-vp9".into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
                "-crf".into(),
                crf.to_string(),
                "-b:v".into(),
                "0".into(),
                "-cpu-used".into(),
                cpu_used.into(),
            ]);
        }
        ExportFormat::Av1 => {
            let av1_preset = match clean_preset {
                "ultrafast" | "superfast" | "veryfast" => "8",
                "faster" | "fast" => "7",
                "medium" => "6",
                "slow" => "5",
                _ => "4",
            };
            args.extend([
                "-vf".into(),
                pad_filter.into(),
                "-c:v".into(),
                "libsvtav1".into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
                "-crf".into(),
                crf.to_string(),
                "-preset".into(),
                av1_preset.into(),
            ]);
        }
        ExportFormat::Lossless => {
            args.extend([
                "-c:v".into(),
                "libx264rgb".into(),
                "-crf".into(),
                "0".into(),
                "-preset".into(),
                "ultrafast".into(),
            ]);
        }
    }

    args.extend([
        "-progress".into(),
        "pipe:1".into(),
        "-stats_period".into(),
        "0.2".into(),
    ]);

    args.push(out_path.to_string_lossy().to_string());

    let mut child = Command::new("ffmpeg")
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn ffmpeg encoder: {e}"))?;

    let mut stderr_opt = child.stderr.take();
    let stderr_handle = std::thread::spawn(move || {
        let mut err_str = String::new();
        if let Some(ref mut r) = stderr_opt {
            use std::io::Read;
            let _ = r.read_to_string(&mut err_str);
        }
        err_str
    });

    if let Some(stdout) = child.stdout.take() {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stdout);
        let mut cur_frame: usize = 0;
        let mut cur_fps: f64 = 0.0;

        for line in reader.lines().flatten() {
            if let Some(val) = line.strip_prefix("frame=") {
                cur_frame = val.trim().parse().unwrap_or(cur_frame);
            } else if let Some(val) = line.strip_prefix("fps=") {
                cur_fps = val.trim().parse().unwrap_or(cur_fps);
            } else if line.starts_with("progress=") {
                if let Some(ref mut cb) = on_progress {
                    cb(cur_frame, cur_fps);
                }
            }
        }
    }

    let status = child.wait().map_err(|e| format!("Failed to wait on ffmpeg encoder: {e}"))?;
    let stderr_str = stderr_handle.join().unwrap_or_default();

    if status.success() && out_path.exists() {
        Ok(out_path.to_path_buf())
    } else {
        Err(format!("ffmpeg encoding failed: {stderr_str}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_png_frame_from_bytes() {
        let icon_path = PathBuf::from("assets/icon/app_icon.png");
        assert!(icon_path.exists());
        let bytes = std::fs::read(&icon_path).expect("read icon");
        let mut cursor = std::io::Cursor::new(bytes);
        let frame = read_png_frame(&mut cursor).expect("read frame");
        assert!(frame.is_some(), "Should read valid frame");
        let next_frame = read_png_frame(&mut cursor).expect("read second frame");
        assert!(next_frame.is_none(), "Should be EOF after single image");
    }
}
