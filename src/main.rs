mod gmic;
mod renderer;
mod server;
mod tween;
mod video;

use clap::Parser;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser, Debug)]
#[command(name = "gmicvideo")]
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
}

fn main() {
    let args = Args::parse();

    if args.server {
        server::run_stdio_server();
        return;
    }

    let filter_db = gmic::FilterDatabase::load_default();

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

        let format = match args.format.to_lowercase().as_str() {
            "h265" | "hevc" => video::ExportFormat::H265,
            "vp9" => video::ExportFormat::Vp9,
            "av1" => video::ExportFormat::Av1,
            "lossless" => video::ExportFormat::Lossless,
            _ => video::ExportFormat::H264,
        };

        let renderer = renderer::BatchRenderer::new();
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
            cut_start_secs: None,
            cut_end_secs: None,
        };

        match renderer.run(request, &filter_db, |p| {
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
