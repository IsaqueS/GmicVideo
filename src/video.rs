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

    let output = Command::new("ffmpeg")
        .args([
            "-y",
            "-nostats",
            "-loglevel",
            "error",
            "-ss",
            &format!("{:.3}", time_secs.max(0.0)),
            "-i",
            video_path.to_str().ok_or("Invalid video path")?,
            "-vframes",
            "1",
            "-vf",
            "scale='min(960,iw)':-2",
            "-q:v",
            "2",
            out_path.to_str().ok_or("Invalid output path")?,
        ])
        .output()
        .map_err(|e| format!("Failed to spawn ffmpeg for preview: {e}"))?;

    if output.status.success() && out_path.exists() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("ffmpeg failed to extract preview frame: {err}"))
    }
}

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
