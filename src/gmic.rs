use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum GmicParamType {
    Float { default: f64, min: f64, max: f64 },
    Int { default: i64, min: i64, max: i64 },
    Choice { default: usize, options: Vec<String> },
    Bool { default: bool },
    Color { default_hex: String },
    Text { default: String },
    Separator,
    Note { text: String },
    Button { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmicParameter {
    pub id: usize,
    pub name: String,
    pub param_type: GmicParamType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmicFilter {
    pub id: String,
    pub name: String,
    pub category: String,
    pub subcategory: Option<String>,
    pub command: String,
    pub preview_command: String,
    pub description: String,
    pub author: String,
    pub parameters: Vec<GmicParameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterDatabase {
    pub filters: Vec<GmicFilter>,
    pub categories: Vec<String>,
}

impl FilterDatabase {
    pub fn load_default() -> Self {
        let mut candidates = Vec::new();

        // 1. Embedded definition files in project or executable directory
        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd.join("assets/gmic/update406.gmic"));
            candidates.push(cwd.join("assets/gmic/update.gmic"));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                candidates.push(exe_dir.join("assets/gmic/update406.gmic"));
                candidates.push(exe_dir.join("../assets/gmic/update406.gmic"));
                candidates.push(exe_dir.join("../share/gmic/update406.gmic"));
                candidates.push(exe_dir.join("../share/gmicvideo/assets/gmic/update406.gmic"));
            }
        }

        // 2. User config directory ~/.config/gmic/update*.gmic (sorted descending)
        if let Some(home) = dirs_home() {
            let gmic_dir = home.join(".config/gmic");
            if let Ok(entries) = std::fs::read_dir(&gmic_dir) {
                let mut user_files: Vec<PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                            name.starts_with("update") && name.ends_with(".gmic")
                        } else {
                            false
                        }
                    })
                    .collect();
                user_files.sort_by(|a, b| b.cmp(a));
                candidates.extend(user_files);
            }
        }

        for path in candidates {
            if path.exists() {
                if let Ok(db) = parse_gmic_definitions(&path) {
                    if !db.filters.is_empty() {
                        return db;
                    }
                }
            }
        }

        // Fallback to built-in presets
        get_builtin_filters()
    }

    pub fn find_filter(&self, name_or_cmd: &str) -> Option<&GmicFilter> {
        let clean = sanitize_id(name_or_cmd);
        self.filters.iter().find(|f| {
            f.name.eq_ignore_ascii_case(name_or_cmd)
                || f.command.eq_ignore_ascii_case(name_or_cmd)
                || f.id.eq_ignore_ascii_case(name_or_cmd)
                || sanitize_id(&f.name) == clean
                || sanitize_id(&f.command) == clean
                || f.id == clean
        })
    }
}

pub fn strip_html(input: &str) -> String {
    let re = Regex::new(r"<[^>]+>").unwrap();
    re.replace_all(input, "")
        .trim()
        .trim_matches('_')
        .trim()
        .to_string()
}

pub fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect()
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub fn parse_gmic_definitions(path: &Path) -> Result<FilterDatabase, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

    let mut parsed_filters = Vec::new();
    let mut current_category = "General".to_string();
    let mut current_subcategory: Option<String> = None;
    let mut current_filter: Option<GmicFilter> = None;

    let tag_re = Regex::new(r"^#@gui(_[a-z]{2})?\s+(.+)$").map_err(|e| e.to_string())?;

    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("#@gui") {
            continue;
        }

        if let Some(caps) = tag_re.captures(trimmed) {
            let body = caps.get(2).map(|m| m.as_str().trim()).unwrap_or("");
            if body.is_empty() {
                continue;
            }

            // Category tag: #@gui Category Name
            if !body.contains(':') {
                let clean = strip_html(body);
                if !clean.is_empty() {
                    current_category = clean;
                    current_subcategory = None;
                }
                continue;
            }

            if body.starts_with(':') {
                let sub = body.trim_start_matches(':').trim();
                if sub.contains('=') {
                    if let Some(ref mut filter) = current_filter {
                        if let Some(param) = parse_parameter_line(sub, filter.parameters.len()) {
                            filter.parameters.extend(param);
                        }
                    }
                } else if !sub.is_empty() {
                    current_subcategory = Some(sub.to_string());
                }
                continue;
            }

            let parts: Vec<&str> = body.splitn(2, ':').collect();
            let name_part = parts[0].trim();

            if name_part.is_empty() {
                continue;
            }

            if parts.len() == 2 && !name_part.starts_with('_') {
                if let Some(f) = current_filter.take() {
                    parsed_filters.push(f);
                }

                let cmds = parts[1].trim();
                let cmd_parts: Vec<&str> = cmds.split(',').collect();
                let command = cmd_parts.first().unwrap_or(&"").trim().to_string();
                let preview_command = cmd_parts.get(1).unwrap_or(&"").trim().to_string();

                let clean_name = strip_html(name_part);
                let id = format!(
                    "{}_{}",
                    sanitize_id(&current_category),
                    sanitize_id(&clean_name)
                );

                current_filter = Some(GmicFilter {
                    id,
                    name: clean_name.clone(),
                    category: current_category.clone(),
                    subcategory: current_subcategory.clone(),
                    command: if command.is_empty() { "fx_unknown".to_string() } else { command },
                    preview_command,
                    description: format!("G'MIC filter: {}", clean_name),
                    author: "G'MIC Team".to_string(),
                    parameters: Vec::new(),
                });
                continue;
            }

            if let Some(ref mut filter) = current_filter {
                if let Some(param) = parse_parameter_line(body, filter.parameters.len()) {
                    filter.parameters.extend(param);
                }
            }
        }
    }

    if let Some(f) = current_filter.take() {
        parsed_filters.push(f);
    }

    let mut categories: Vec<String> = parsed_filters
        .iter()
        .map(|f| f.category.clone())
        .collect();
    categories.sort();
    categories.dedup();

    Ok(FilterDatabase {
        filters: parsed_filters,
        categories,
    })
}

fn parse_parameter_line(line: &str, start_id: usize) -> Option<Vec<GmicParameter>> {
    let (name_part, rest) = line.split_once('=')?;
    let param_name = name_part.trim();

    let clean_name = if param_name.starts_with('_') {
        ""
    } else {
        param_name
    }.to_string();

    let rest = rest.trim();
    // Support G'MIC parameter delimiters: `(...)`, `{...}`, and `[...]`
    let type_end = rest.find(|c| c == '(' || c == '{' || c == '[')?;
    let open_char = rest.as_bytes()[type_end] as char;
    let close_char = match open_char {
        '(' => ')',
        '{' => '}',
        '[' => ']',
        _ => ')',
    };

    let type_name = rest[..type_end]
        .trim()
        .trim_start_matches(|c| c == '~' || c == '_')
        .to_lowercase();

    let after_open = &rest[type_end + 1..];
    // Find matching closing delimiter (accounting for trailing visibility suffixes like `)_0` or `}_1+`)
    let args_str = if let Some(close_idx) = after_open.rfind(close_char) {
        after_open[..close_idx].trim()
    } else if let Some(close_idx) = after_open.rfind(|c| c == ')' || c == '}' || c == ']') {
        after_open[..close_idx].trim()
    } else {
        after_open.trim()
    };

    let mut params = parse_typed_parameter(&type_name, args_str, clean_name);
    for (idx, p) in params.iter_mut().enumerate() {
        p.id = start_id + idx;
    }
    Some(params)
}

fn parse_typed_parameter(type_name: &str, args: &str, clean_name: String) -> Vec<GmicParameter> {
    match type_name {
        "float" | "f" => {
            let parts: Vec<&str> = args.split(',').map(|s| s.trim().trim_end_matches('%')).collect();
            let default = parts.first().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            let min = parts.get(1).and_then(|s| s.parse::<f64>().ok()).unwrap_or(-1000.0);
            let max = parts.get(2).and_then(|s| s.parse::<f64>().ok()).unwrap_or(1000.0);

            vec![GmicParameter {
                id: 0,
                name: if clean_name.is_empty() { "Float Parameter".into() } else { clean_name },
                param_type: GmicParamType::Float { default, min, max },
            }]
        }
        "int" => {
            let parts: Vec<&str> = args.split(',').map(|s| s.trim().trim_end_matches('%')).collect();
            let default = parts.first().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            let min = parts.get(1).and_then(|s| s.parse::<i64>().ok()).unwrap_or(-1000);
            let max = parts.get(2).and_then(|s| s.parse::<i64>().ok()).unwrap_or(1000);

            vec![GmicParameter {
                id: 0,
                name: if clean_name.is_empty() { "Integer Parameter".into() } else { clean_name },
                param_type: GmicParamType::Int { default, min, max },
            }]
        }
        "bool" => {
            let trimmed = args.trim();
            let default_val = trimmed == "1"
                || trimmed.eq_ignore_ascii_case("true")
                || trimmed.starts_with("1,")
                || trimmed.starts_with("true,");
            vec![GmicParameter {
                id: 0,
                name: if clean_name.is_empty() { "Enable".into() } else { clean_name },
                param_type: GmicParamType::Bool { default: default_val },
            }]
        }
        "choice" => {
            let parts = parse_csv_args(args);
            // In G'MIC, choice can be either:
            // 1) choice(default_index, "Opt0", "Opt1", ...)
            // 2) choice("Opt0", "Opt1", ...) -> where default_index is implicitly 0!
            let first_is_index = parts
                .first()
                .map(|s| !s.starts_with('"') && !s.starts_with('\'') && s.parse::<usize>().is_ok())
                .unwrap_or(false);

            let (default_idx, raw_options): (usize, Vec<String>) = if first_is_index {
                let idx = parts.first().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                let opts = parts
                    .into_iter()
                    .skip(1)
                    .map(|s| s.trim_matches('"').trim_matches('\'').to_string())
                    .collect();
                (idx, opts)
            } else {
                let opts = parts
                    .into_iter()
                    .map(|s| s.trim_matches('"').trim_matches('\'').to_string())
                    .collect();
                (0, opts)
            };

            vec![GmicParameter {
                id: 0,
                name: if clean_name.is_empty() { "Selection".into() } else { clean_name },
                param_type: GmicParamType::Choice {
                    default: default_idx,
                    options: raw_options,
                },
            }]
        }
        "point" => {
            // In G'MIC, point(x, y, ...) passes 2 arguments (X and Y percentages, 0..100) to the command
            let parts: Vec<&str> = args.split(',').map(|s| s.trim().trim_end_matches('%')).collect();
            let def_x = parts.first().and_then(|s| s.parse::<f64>().ok()).unwrap_or(50.0);
            let def_y = parts.get(1).and_then(|s| s.parse::<f64>().ok()).unwrap_or(50.0);
            let base_name = if clean_name.is_empty() { "Point".to_string() } else { clean_name };

            vec![
                GmicParameter {
                    id: 0,
                    name: format!("{base_name} X (%)"),
                    param_type: GmicParamType::Float { default: def_x, min: 0.0, max: 100.0 },
                },
                GmicParameter {
                    id: 0,
                    name: format!("{base_name} Y (%)"),
                    param_type: GmicParamType::Float { default: def_y, min: 0.0, max: 100.0 },
                },
            ]
        }
        "color" => {
            let (r, g, b, a_opt) = parse_color_tuple(args);
            let base_name = if clean_name.is_empty() { "Color".to_string() } else { clean_name };
            let mut params = vec![
                GmicParameter {
                    id: 0,
                    name: format!("{base_name} Red"),
                    param_type: GmicParamType::Int { default: r, min: 0, max: 255 },
                },
                GmicParameter {
                    id: 0,
                    name: format!("{base_name} Green"),
                    param_type: GmicParamType::Int { default: g, min: 0, max: 255 },
                },
                GmicParameter {
                    id: 0,
                    name: format!("{base_name} Blue"),
                    param_type: GmicParamType::Int { default: b, min: 0, max: 255 },
                },
            ];
            if let Some(a) = a_opt {
                params.push(GmicParameter {
                    id: 0,
                    name: format!("{base_name} Alpha"),
                    param_type: GmicParamType::Int { default: a, min: 0, max: 255 },
                });
            }
            params
        }
        "value" => {
            let clean_val = args.trim().trim_matches('"').trim_matches('\'');
            if let Ok(default_num) = clean_val.parse::<f64>() {
                vec![GmicParameter {
                    id: 0,
                    name: if clean_name.is_empty() { "Value".into() } else { clean_name },
                    param_type: GmicParamType::Float { default: default_num, min: -1000.0, max: 1000.0 },
                }]
            } else {
                vec![GmicParameter {
                    id: 0,
                    name: if clean_name.is_empty() { "Value".into() } else { clean_name },
                    param_type: GmicParamType::Text { default: clean_val.to_string() },
                }]
            }
        }
        "button" => {
            vec![GmicParameter {
                id: 0,
                name: if clean_name.is_empty() { "Trigger".into() } else { clean_name.clone() },
                param_type: GmicParamType::Button { name: clean_name },
            }]
        }
        "text" | "file" | "filein" | "fileout" | "folder" => {
            // Handle multiline text syntax: text(1, "default") or text(0, "default") vs text("default")
            let parts = parse_csv_args(args);
            let default_str = if parts.len() >= 2
                && (parts[0] == "0" || parts[0] == "1")
                && !parts[0].starts_with('"')
            {
                parts[1..].join(",").trim_matches('"').trim_matches('\'').to_string()
            } else {
                args.trim_matches('"').trim_matches('\'').to_string()
            };

            vec![GmicParameter {
                id: 0,
                name: if clean_name.is_empty() { "Text".into() } else { clean_name },
                param_type: GmicParamType::Text { default: default_str },
            }]
        }
        "separator" | "link" => {
            vec![GmicParameter {
                id: 0,
                name: "".into(),
                param_type: GmicParamType::Separator,
            }]
        }
        "note" => {
            vec![GmicParameter {
                id: 0,
                name: "".into(),
                param_type: GmicParamType::Note { text: args.trim_matches('"').trim_matches('\'').to_string() },
            }]
        }
        _ => Vec::new(),
    }
}

fn parse_csv_args(input: &str) -> Vec<String> {
    let mut res = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;

    for ch in input.chars() {
        match ch {
            '"' => {
                in_quote = !in_quote;
                current.push(ch);
            }
            ',' if !in_quote => {
                res.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        res.push(current.trim().to_string());
    }
    res
}

fn parse_color_tuple(args: &str) -> (i64, i64, i64, Option<i64>) {
    let parts: Vec<i64> = args
        .split(',')
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .collect();

    let r = *parts.first().unwrap_or(&255);
    let g = *parts.get(1).unwrap_or(&255);
    let b = *parts.get(2).unwrap_or(&255);
    let a = parts.get(3).copied();
    (r, g, b, a)
}

pub fn build_gmic_args(filter: &GmicFilter, values: &HashMap<usize, f64>) -> String {
    let mut arg_tokens = Vec::new();

    for param in &filter.parameters {
        match &param.param_type {
            GmicParamType::Separator | GmicParamType::Note { .. } => {
                // Non-argument parameters are skipped
                continue;
            }
            GmicParamType::Float { default, .. } => {
                let val = values.get(&param.id).copied().unwrap_or(*default);
                if (val - val.round()).abs() < 1e-5 {
                    arg_tokens.push(format!("{}", val.round() as i64));
                } else {
                    arg_tokens.push(format!("{:.4}", val));
                }
            }
            GmicParamType::Int { default, .. } => {
                let val = values
                    .get(&param.id)
                    .map(|v| v.round() as i64)
                    .unwrap_or(*default);
                arg_tokens.push(val.to_string());
            }
            GmicParamType::Choice { default, .. } => {
                let val = values
                    .get(&param.id)
                    .map(|v| v.round() as usize)
                    .unwrap_or(*default);
                arg_tokens.push(val.to_string());
            }
            GmicParamType::Bool { default } => {
                let val = values
                    .get(&param.id)
                    .map(|v| if *v >= 0.5 { 1 } else { 0 })
                    .unwrap_or(if *default { 1 } else { 0 });
                arg_tokens.push(val.to_string());
            }
            GmicParamType::Color { default_hex } => {
                let val = values
                    .get(&param.id)
                    .map(|v| v.round() as i64)
                    .unwrap_or_else(|| {
                        let clean = default_hex.trim_start_matches('#');
                        i64::from_str_radix(clean, 16).unwrap_or(0)
                    });
                arg_tokens.push(val.to_string());
            }
            GmicParamType::Text { default } => {
                arg_tokens.push(format!("\"{}\"", default));
            }
            GmicParamType::Button { .. } => {
                arg_tokens.push("0".into());
            }
        }
    }

    if arg_tokens.is_empty() {
        filter.command.clone()
    } else {
        format!("{} {}", filter.command, arg_tokens.join(","))
    }
}

pub fn get_gmic_bin_path() -> PathBuf {
    if let Ok(val) = std::env::var("GMIC_BIN") {
        let p = PathBuf::from(val);
        if p.exists() {
            return p;
        }
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let candidates = [
                exe_dir.join("bin/gmic"),
                exe_dir.join("gmic"),
                exe_dir.join("../bin/gmic"),
                exe_dir.join("../usr/bin/gmic"),
            ];
            for cand in &candidates {
                if cand.exists() {
                    return cand.to_path_buf();
                }
            }
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let candidates = [
            cwd.join("bin/gmic"),
            cwd.join("packaging/AppDir/usr/bin/gmic"),
        ];
        for cand in &candidates {
            if cand.exists() {
                return cand.to_path_buf();
            }
        }
    }

    PathBuf::from("gmic")
}

/// Executes G'MIC directly on in-memory PNG bytes via stdin/stdout pipe with zero disk I/O.
pub fn execute_gmic_on_bytes(
    input_bytes: &[u8],
    filter: &GmicFilter,
    values: &HashMap<usize, f64>,
) -> Result<Vec<u8>, String> {
    let gmic_cmd = build_gmic_args(filter, values);
    let cmd_parts: Vec<&str> = gmic_cmd.split_whitespace().collect();
    if cmd_parts.is_empty() {
        return Err("Empty G'MIC command".to_string());
    }

    let mut args: Vec<String> = vec![
        "-v".into(),
        "-".into(),
        "-input".into(),
        "png:-".into(),
        "if".into(),
        "iM>255".into(),
        "div".into(),
        "257".into(),
        "round".into(),
        "fi".into(),
    ];

    for p in cmd_parts {
        args.push(p.to_string());
    }

    args.push("-keep[0]".into());
    args.push("-to_rgb".into());
    args.push("-cut".into());
    args.push("0,255".into());
    args.push("-round".into());
    args.push("-output".into());
    args.push("png:-".into());

    let gmic_bin = get_gmic_bin_path();
    let mut child = Command::new(&gmic_bin)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn gmic CLI ({}): {e}", gmic_bin.display()))?;

    let mut child_stdin = child.stdin.take().ok_or("Failed to open child stdin")?;
    let input_vec = input_bytes.to_vec();
    let writer_handle = std::thread::spawn(move || {
        use std::io::Write;
        let _ = child_stdin.write_all(&input_vec);
        drop(child_stdin);
    });

    let mut out_bytes = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        use std::io::Read;
        let _ = stdout.read_to_end(&mut out_bytes);
    }

    let mut err_str = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        use std::io::Read;
        let _ = stderr.read_to_string(&mut err_str);
    }

    let status = child.wait().map_err(|e| format!("Failed to wait on gmic CLI: {e}"))?;
    let _ = writer_handle.join();

    if status.success() && out_bytes.len() > 8 && out_bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]) {
        Ok(out_bytes)
    } else {
        Err(format!("gmic failed: {err_str}"))
    }
}

pub fn execute_gmic_on_image(
    input_image: &Path,
    filter: &GmicFilter,
    values: &HashMap<usize, f64>,
    output_image: &Path,
) -> Result<(), String> {
    if let Some(parent) = output_image.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let gmic_cmd = build_gmic_args(filter, values);
    let cmd_parts: Vec<&str> = gmic_cmd.split_whitespace().collect();
    if cmd_parts.is_empty() {
        return Err("Empty G'MIC command".to_string());
    }

    let mut args: Vec<String> = vec![
        "-input".into(),
        input_image.to_string_lossy().to_string(),
        "if".into(),
        "iM>255".into(),
        "div".into(),
        "257".into(),
        "round".into(),
        "fi".into(),
    ];

    for p in cmd_parts {
        args.push(p.to_string());
    }

    args.push("-keep[0]".into());
    args.push("-to_rgb".into());
    args.push("-cut".into());
    args.push("0,255".into());
    args.push("-round".into());
    args.push("-output".into());
    args.push(output_image.to_string_lossy().to_string());

    let gmic_bin = get_gmic_bin_path();
    let output = Command::new(&gmic_bin)
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to spawn gmic CLI ({}): {e}", gmic_bin.display()))?;

    if output.status.success() && output_image.exists() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("gmic failed: {err}"))
    }
}

pub fn get_builtin_filters() -> FilterDatabase {
    let mut filters = Vec::new();

    // Halftone
    let mut halftone_params = Vec::new();
    halftone_params.push(GmicParameter {
        id: 0,
        name: "Brightness (%)".into(),
        param_type: GmicParamType::Float { default: 0.0, min: -100.0, max: 100.0 },
    });
    halftone_params.push(GmicParameter {
        id: 1,
        name: "Contrast (%)".into(),
        param_type: GmicParamType::Float { default: 0.0, min: -100.0, max: 100.0 },
    });
    halftone_params.push(GmicParameter {
        id: 2,
        name: "Gamma (%)".into(),
        param_type: GmicParamType::Float { default: 0.0, min: -100.0, max: 100.0 },
    });
    halftone_params.push(GmicParameter {
        id: 3,
        name: "Smoothness".into(),
        param_type: GmicParamType::Float { default: 0.0, min: 0.0, max: 10.0 },
    });
    halftone_params.push(GmicParameter {
        id: 4,
        name: "Number of Tones".into(),
        param_type: GmicParamType::Int { default: 5, min: 2, max: 32 },
    });
    halftone_params.push(GmicParameter {
        id: 5,
        name: "Size for Dark Tones".into(),
        param_type: GmicParamType::Int { default: 8, min: 2, max: 256 },
    });
    halftone_params.push(GmicParameter {
        id: 6,
        name: "Size for Bright Tones".into(),
        param_type: GmicParamType::Int { default: 8, min: 2, max: 256 },
    });
    halftone_params.push(GmicParameter {
        id: 7,
        name: "Shape".into(),
        param_type: GmicParamType::Choice {
            default: 5,
            options: vec![
                "Square".into(),
                "Diamond".into(),
                "Circle".into(),
                "Square (Inv.)".into(),
                "Diamond (Inv.)".into(),
                "Circle (Inv.)".into(),
            ],
        },
    });
    halftone_params.push(GmicParameter {
        id: 8,
        name: "Smoothness".into(),
        param_type: GmicParamType::Float { default: 0.1, min: 0.0, max: 32.0 },
    });

    filters.push(GmicFilter {
        id: "patterns_halftone".into(),
        name: "Halftone".into(),
        category: "Patterns".into(),
        subcategory: None,
        command: "fx_halftone".into(),
        preview_command: "fx_halftone".into(),
        description: "Simulates a printing halftone pattern (dots).".into(),
        author: "David Tschumperle".into(),
        parameters: halftone_params,
    });

    // Warhol
    let mut warhol_params = Vec::new();
    warhol_params.push(GmicParameter {
        id: 0,
        name: "Grid X".into(),
        param_type: GmicParamType::Int { default: 2, min: 1, max: 8 },
    });
    warhol_params.push(GmicParameter {
        id: 1,
        name: "Grid Y".into(),
        param_type: GmicParamType::Int { default: 2, min: 1, max: 8 },
    });
    warhol_params.push(GmicParameter {
        id: 2,
        name: "Smoothness".into(),
        param_type: GmicParamType::Float { default: 2.0, min: 0.0, max: 10.0 },
    });
    warhol_params.push(GmicParameter {
        id: 3,
        name: "Color Variations".into(),
        param_type: GmicParamType::Float { default: 20.0, min: 0.0, max: 100.0 },
    });

    filters.push(GmicFilter {
        id: "artistic_warhol".into(),
        name: "Warhol".into(),
        category: "Artistic".into(),
        subcategory: None,
        command: "warhol".into(),
        preview_command: "warhol".into(),
        description: "Creates Andy Warhol style multi-colored tiles".into(),
        author: "David Tschumperle".into(),
        parameters: warhol_params,
    });

    // Vignette
    let mut vignette_params = Vec::new();
    vignette_params.push(GmicParameter {
        id: 0,
        name: "Strength".into(),
        param_type: GmicParamType::Float { default: 50.0, min: 0.0, max: 100.0 },
    });
    vignette_params.push(GmicParameter {
        id: 1,
        name: "Radius".into(),
        param_type: GmicParamType::Float { default: 70.0, min: 0.0, max: 100.0 },
    });

    filters.push(GmicFilter {
        id: "light_vignette".into(),
        name: "Vignette".into(),
        category: "Lights & Shadows".into(),
        subcategory: None,
        command: "vignette".into(),
        preview_command: "vignette".into(),
        description: "Darkens the edges of the image".into(),
        author: "David Tschumperle".into(),
        parameters: vignette_params,
    });

    let mut categories: Vec<String> = filters.iter().map(|f| f.category.clone()).collect();
    categories.sort();
    categories.dedup();

    FilterDatabase {
        filters,
        categories,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_gmic_binary_version() {
        let bin = get_gmic_bin_path();
        let output = Command::new(&bin)
            .arg("-version")
            .output()
            .expect("gmic binary should be callable");
        assert!(output.status.success(), "gmic -version should succeed");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("4.0.6") || stdout.contains("Version"), "Expected G'MIC version output, got: {}", stdout);
    }

    #[test]
    fn test_filter_database_load_default() {
        let db = FilterDatabase::load_default();
        assert!(!db.filters.is_empty(), "FilterDatabase should have loaded filters");
        assert!(db.filters.len() > 50, "Expected hundreds of filters, got {}", db.filters.len());
        assert!(!db.categories.is_empty(), "Categories should not be empty");
    }

    #[test]
    fn test_filter_ids_and_categories_are_clean_for_thumbnails() {
        let db = FilterDatabase::load_default();
        let warhol = db.find_filter("warhol").expect("Warhol should exist");
        assert_eq!(warhol.id, "artistic_warhol");
        assert_eq!(warhol.category, "Artistic");

        for cat in &db.categories {
            assert!(!cat.contains('<') && !cat.contains('>'), "Category should not contain HTML tags: {}", cat);
            assert!(!cat.starts_with('_'), "Category should not have leading underscore: {}", cat);
        }
    }

    #[test]
    fn test_execute_gmic_on_bytes() {
        let db = FilterDatabase::load_default();
        let filter = db.find_filter("warhol").expect("Warhol filter should exist");

        let icon_path = PathBuf::from("assets/icon/app_icon.png");
        assert!(icon_path.exists(), "app_icon.png should exist");
        let input_bytes = std::fs::read(&icon_path).expect("should read icon bytes");
        let res = execute_gmic_on_bytes(&input_bytes, filter, &HashMap::new());
        assert!(res.is_ok(), "execute_gmic_on_bytes should succeed: {:?}", res.err());
        let out_bytes = res.unwrap();
        assert!(out_bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]), "Should return valid PNG bytes");
    }

    #[test]
    fn test_halftone_effects_execution() {
        let db = FilterDatabase::load_default();
        let icon_path = PathBuf::from("assets/icon/app_icon.png");
        let input_bytes = std::fs::read(&icon_path).expect("should read icon bytes");

        // 1. Test standard Halftone (fx_halftone)
        let halftone = db.find_filter("Halftone").expect("Halftone filter should exist");
        assert_eq!(halftone.command, "fx_halftone");
        let args_str = build_gmic_args(halftone, &HashMap::new());
        assert_eq!(args_str, "fx_halftone 0,0,0,0,5,8,8,5,0.1000");

        let res = execute_gmic_on_bytes(&input_bytes, halftone, &HashMap::new());
        assert!(
            res.is_ok(),
            "Halftone (fx_halftone) execution failed: {:?}",
            res.err()
        );
        let out_bytes = res.unwrap();
        assert!(out_bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]));

        // 2. Test Halftone [Generic] (fx_generic_halftone)
        if let Some(generic_ht) = db.find_filter("Halftone [Generic]") {
            let res_gen = execute_gmic_on_bytes(&input_bytes, generic_ht, &HashMap::new());
            assert!(
                res_gen.is_ok(),
                "Halftone [Generic] execution failed: {:?}",
                res_gen.err()
            );
        }

        // 3. Test Halftone Shapes (iain_halftone_shapes)
        if let Some(shapes_ht) = db.find_filter("Halftone Shapes") {
            let res_shapes = execute_gmic_on_bytes(&input_bytes, shapes_ht, &HashMap::new());
            assert!(
                res_shapes.is_ok(),
                "Halftone Shapes execution failed: {:?}",
                res_shapes.err()
            );
        }
    }
}
