mod effect;
mod frame;
mod frame_sink;
mod gmic;
mod gmic_worker;
mod gpu;
mod playback;
mod renderer;
mod server;
mod thumbnails;
mod tween;
mod video;

use clap::Parser;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser, Debug)]
#[command(name = "gmicvideo", version)]
#[command(about = "Rust-based G'MIC Video Effects Editor with Kirigami UI, FFMPEG & Tweening", long_about = None)]
struct Args {
    /// Start in stdio IPC server mode
    #[arg(long)]
    server: bool,

    /// List all available G'MIC filters and categories
    #[arg(long)]
    list_effects: bool,

    /// Filter search term to query
    #[arg(long)]
    search: Option<String>,

    /// Path to input video for headless batch processing
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Path to output video for headless batch processing
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Filter ID or command name to apply
    #[arg(short, long)]
    filter: Option<String>,

    /// Video export format: h264, h265, vp9, av1, lossless
    #[arg(long, default_value = "h264")]
    format: String,

    /// Encoding CRF quality (lower is better, e.g. 18)
    #[arg(long, default_value = "18")]
    crf: u32,

    /// Find which "Deformations" filters can run on the GPU as baked warp maps and write
    /// assets/gpu/warp_bakeable.json (or the given path)
    #[arg(long, value_name = "OUT", num_args = 0..=1, default_missing_value = "")]
    classify_warps: Option<String>,

    /// Render a preview thumbnail of every effect into assets/thumbnails (or the given dir)
    #[arg(long, value_name = "DIR", num_args = 0..=1, default_missing_value = "")]
    generate_thumbnails: Option<String>,

    /// With --generate-thumbnails: re-render thumbnails that already exist
    #[arg(long)]
    force: bool,

    /// With --generate-thumbnails: seconds allowed per effect
    #[arg(long, default_value = "30")]
    thumbnail_timeout: u64,
}

fn main() {
    let args = Args::parse();

    if args.server {
        server::run_stdio_server();
        return;
    }

    let filter_db = gmic::FilterDatabase::load_default();

    if let Some(out) = args.classify_warps {
        let out = if out.is_empty() { gpu::warp::default_output_path() } else { PathBuf::from(out) };
        let cls = gpu::warp::classify(&filter_db, |name, r| match r {
            Ok((p, ss)) => println!("  warp   {p:5.1} dB  {name}{}", if *ss { " (supersampled)" } else { "" }),
            Err(e) => println!("  no              {name}: {e}"),
        });
        println!("{} of {} deformations are bakeable.", cls.bakeable.len(), cls.bakeable.len() + cls.rejected.len());
        if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_string_pretty(&cls).map_err(|e| e.to_string()).and_then(|j| std::fs::write(&out, j + "\n").map_err(|e| e.to_string())) {
            Ok(()) => println!("Wrote {}", out.display()),
            Err(e) => {
                eprintln!("Failed to write {}: {e}", out.display());
                std::process::exit(1);
            }
        }
        return;
    }

    if let Some(dir) = args.generate_thumbnails {
        let dir = if dir.is_empty() { thumbnails::default_output_dir() } else { PathBuf::from(dir) };
        println!("Rendering thumbnails for {} effects into {}", filter_db.filters.len(), dir.display());
        let res = thumbnails::generate(
            &filter_db,
            &dir,
            std::time::Duration::from_secs(args.thumbnail_timeout),
            args.force,
            |n, total, r| {
                if r.status != "valid" {
                    println!("[{n}/{total}] {:8} {} — {}", r.status, r.name, r.error.chars().take(120).collect::<String>());
                } else if n % 50 == 0 {
                    println!("[{n}/{total}] ...");
                }
            },
        );
        match res {
            Ok(r) => {
                let ok = r.iter().filter(|x| x.status == "valid").count();
                println!("{ok} of {} effects have a thumbnail.", r.len());
            }
            Err(e) => {
                eprintln!("Thumbnail generation failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    if args.list_effects {
        println!("Loaded {} G'MIC filters across {} categories.\n", filter_db.filters.len(), filter_db.categories.len());
        println!("Categories:");
        for cat in &filter_db.categories {
            let count = filter_db.filters.iter().filter(|f| &f.category == cat).count();
            println!("  - {cat} ({count} filters)");
        }
        return;
    }

    if let Some(query) = args.search {
        let q = query.to_lowercase();
        let matches: Vec<_> = filter_db
            .filters
            .iter()
            .filter(|f| f.name.to_lowercase().contains(&q) || f.command.to_lowercase().contains(&q) || f.category.to_lowercase().contains(&q))
            .collect();

        println!("Found {} filters matching '{}':", matches.len(), query);
        for f in matches.iter().take(30) {
            println!("  [{}] {} (command: {}) - {} params", f.category, f.name, f.command, f.parameters.len());
        }
        return;
    }

    // If headless batch render is requested
    if let (Some(input), Some(output), Some(filter_name)) = (args.input, args.output, args.filter) {
        println!("Starting headless render for: {}", input.display());
        let filter = match filter_db.find_filter(&filter_name) {
            Some(f) => f.clone(),
            None => {
                eprintln!("Error: Filter '{}' not found", filter_name);
                std::process::exit(1);
            }
        };

        let format = video::ExportFormat::from_name(&args.format);

        let request = renderer::RenderJobRequest {
            id: "cli_job".to_string(),
            name: format!("CLI: {}", filter.name),
            video_path: input,
            output_path: output,
            format,
            crf: args.crf,
            speed_preset: "medium".to_string(),
            filter_id: filter.id.clone(),
            filter_name: filter.name,
            base_values: std::collections::HashMap::new(),
            tweens: std::collections::HashMap::new(),
            text_values: std::collections::HashMap::new(),
            cut_start_secs: None,
            cut_end_secs: None,
        };

        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        match renderer::BatchRenderer::run(request, &filter_db, cancel, |p| {
            println!("[{:.1}%] {} (FPS: {:.1})", p.percent, p.message, p.fps);
        }) {
            Ok(out) => {
                println!("Successfully rendered: {}", out.display());
            }
            Err(e) => {
                eprintln!("Render failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // Default: Launch the KDE Kirigami User Interface
    launch_kirigami_gui();
}

fn launch_kirigami_gui() {
    let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("gmicvideo"));
    
    // Find gmicvideo_gui.py in:
    // 1. GMIC_VIDEO_GUI_SCRIPT env var
    // 2. Relative to current_exe in AppDir or install prefix (../share/gmicvideo/src_gui/gmicvideo_gui.py)
    // 3. Next to executable (src_gui/gmicvideo_gui.py)
    // 4. Fallback to CARGO_MANIFEST_DIR
    let python_script = if let Ok(custom) = std::env::var("GMIC_VIDEO_GUI_SCRIPT") {
        PathBuf::from(custom)
    } else if let Some(parent) = current_exe.parent() {
        let in_share = parent.join("../share/gmicvideo/src_gui/gmicvideo_gui.py");
        let next_to = parent.join("src_gui/gmicvideo_gui.py");
        if in_share.exists() {
            in_share
        } else if next_to.exists() {
            next_to
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src_gui").join("gmicvideo_gui.py")
        }
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src_gui").join("gmicvideo_gui.py")
    };

    println!("Launching G'MIC Video Editor (Kirigami KDE UI)...");

    let mut cmd = Command::new("python3");
    cmd.arg(&python_script)
       .env("GMIC_VIDEO_BACKEND_BIN", &current_exe);

    for arg in std::env::args().skip(1) {
        cmd.arg(arg);
    }

    let status = cmd.status();

    match status {
        Ok(s) => {
            if !s.success() {
                eprintln!("GUI exited with status: {:?}", s.code());
            }
        }
        Err(e) => {
            eprintln!("Failed to launch Kirigami GUI: {e}");
        }
    }
}
