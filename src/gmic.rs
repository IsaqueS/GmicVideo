use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
        for path in definition_file_candidates() {
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

/// Possible locations of the G'MIC filter definition file (`update406.gmic`), best first.
fn definition_file_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(custom) = std::env::var("GMIC_VIDEO_DEFINITIONS") {
        candidates.push(PathBuf::from(custom));
    }
    // 1. Bundled definition files next to the executable (AppImage / install prefix)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("assets/gmic/update406.gmic"));
            candidates.push(exe_dir.join("../share/gmicvideo/assets/gmic/update406.gmic"));
            candidates.push(exe_dir.join("../share/gmic/update406.gmic"));
            // target/{debug,release}/GmicVideo -> repo root
            candidates.push(exe_dir.join("../../assets/gmic/update406.gmic"));
        }
    }
    // 2. Project directory (cargo run / tests)
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/gmic/update406.gmic"));
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("assets/gmic/update406.gmic"));
    }

    // 3. User config directory ~/.config/gmic/update*.gmic (newest first)
    if let Some(home) = dirs_home() {
        let gmic_dir = home.join(".config/gmic");
        if let Ok(entries) = std::fs::read_dir(&gmic_dir) {
            let mut user_files: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|name| name.starts_with("update") && name.ends_with(".gmic"))
                        .unwrap_or(false)
                })
                .collect();
            user_files.sort_by(|a, b| b.cmp(a));
            candidates.extend(user_files);
        }
    }
    candidates
}

/// The filter definition file in use, so G'MIC workers can load the exact same filters.
pub fn find_definitions_file() -> Option<PathBuf> {
    static FOUND: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| definition_file_candidates().into_iter().find(|p| p.is_file()))
        .clone()
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

pub fn is_non_effect(category: &str, name: &str, command: &str) -> bool {
    let cat_low = category.trim().to_lowercase();
    let name_low = name.trim().to_lowercase();
    let cmd_low = command.trim().to_lowercase();

    if cat_low == "about" || cat_low == "sobre" || cat_low == "information" || cat_low == "informações" {
        return true;
    }

    if cmd_low == "_none_"
        || cmd_low == "none"
        || cmd_low.is_empty()
        || cmd_low == "fx_unknown"
        || cmd_low == "gui_download_all_data"
        || cmd_low == "gui_layer_info"
    {
        return true;
    }

    if name_low == "about"
        || name_low.starts_with("about ")
        || name_low == "sobre"
        || name_low.starts_with("sobre ")
    {
        return true;
    }

    if name_low == "privacy notice"
        || name_low == "release notes"
        || name_low == "send feedback"
        || name_low == "tribute to sébastien"
        || name_low == "tribute to sebastien"
        || name_low == "what's new?"
        || name_low == "whats new?"
        || name_low == "download external data"
    {
        return true;
    }

    false
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
                    if !is_non_effect(&f.category, &f.name, &f.command) {
                        parsed_filters.push(f);
                    }
                }

                let cmds = parts[1].trim();
                let cmd_parts: Vec<&str> = cmds.split(',').collect();
                let command = cmd_parts.first().unwrap_or(&"").trim().to_string();
                let preview_command = cmd_parts.get(1).unwrap_or(&"").trim().to_string();

                let clean_name = strip_html(name_part);
                if is_non_effect(&current_category, &clean_name, &command) {
                    current_filter = None;
                    continue;
                }

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
        if !is_non_effect(&f.category, &f.name, &f.command) {
            parsed_filters.push(f);
        }
    }

    let mut categories: Vec<String> = parsed_filters
        .iter()
        .map(|f| f.category.clone())
        .filter(|c| !c.trim().eq_ignore_ascii_case("about") && !c.trim().eq_ignore_ascii_case("sobre"))
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

/// Parses a G'MIC `color(...)` default: `r,g,b[,a]`, `#rrggbb`, `#rrggbbaa`, `#rgb` or empty (black).
/// Alpha is only present when the default specifies it (G'MIC then expects 4 arguments).
fn parse_color_tuple(args: &str) -> (i64, i64, i64, Option<i64>) {
    let trimmed = args.trim().trim_matches('"');
    if let Some(hex) = trimmed.strip_prefix('#') {
        let hex = hex.trim();
        let byte = |i: usize| i64::from_str_radix(hex.get(i..i + 2).unwrap_or("0"), 16).unwrap_or(0);
        let nibble = |i: usize| i64::from_str_radix(hex.get(i..i + 1).unwrap_or("0"), 16).unwrap_or(0) * 17;
        return match hex.len() {
            8 => (byte(0), byte(2), byte(4), Some(byte(6))),
            6 => (byte(0), byte(2), byte(4), None),
            4 => (nibble(0), nibble(1), nibble(2), Some(nibble(3))),
            3 => (nibble(0), nibble(1), nibble(2), None),
            _ => (0, 0, 0, None),
        };
    }

    let parts: Vec<i64> = trimmed
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .map(|v| v.round() as i64)
        .collect();
    match parts.len() {
        0 => (0, 0, 0, None),
        1 => (parts[0], parts[0], parts[0], None),
        2 => (parts[0], parts[1], 0, None),
        3 => (parts[0], parts[1], parts[2], None),
        _ => (parts[0], parts[1], parts[2], Some(parts[3])),
    }
}

/// One G'MIC argument value.
#[derive(Debug, Clone, PartialEq)]
pub enum ArgValue {
    Num(f64),
    Text(String),
}

/// The arguments G'MIC receives for `filter`, in order (separators and notes skipped).
pub fn argument_list(filter: &GmicFilter, values: &HashMap<usize, f64>, texts: &HashMap<usize, String>) -> Vec<ArgValue> {
    let mut args = Vec::new();
    for param in &filter.parameters {
        let v = values.get(&param.id).copied().filter(|v| v.is_finite());
        let arg = match &param.param_type {
            GmicParamType::Separator | GmicParamType::Note { .. } => continue,
            GmicParamType::Float { default, .. } => ArgValue::Num(v.unwrap_or(*default)),
            GmicParamType::Int { default, .. } => ArgValue::Num(v.map(|v| v.round()).unwrap_or(*default as f64)),
            GmicParamType::Choice { default, options } => {
                let max_idx = options.len().saturating_sub(1);
                ArgValue::Num(v.map(|v| (v.round().max(0.0) as usize).min(max_idx)).unwrap_or(*default) as f64)
            }
            GmicParamType::Bool { default } => {
                ArgValue::Num(v.map(|v| if v >= 0.5 { 1.0 } else { 0.0 }).unwrap_or(if *default { 1.0 } else { 0.0 }))
            }
            GmicParamType::Color { default_hex } => ArgValue::Num(v.map(|v| v.round()).unwrap_or_else(|| {
                i64::from_str_radix(default_hex.trim_start_matches('#'), 16).unwrap_or(0) as f64
            })),
            GmicParamType::Text { default } => ArgValue::Text(texts.get(&param.id).unwrap_or(default).clone()),
            GmicParamType::Button { .. } => ArgValue::Num(0.0),
        };
        args.push(arg);
    }
    args
}

/// Numeric arguments only (text arguments become NaN), as used by the GPU effects.
pub fn argument_values(filter: &GmicFilter, values: &HashMap<usize, f64>) -> Vec<f64> {
    argument_list(filter, values, &HashMap::new())
        .into_iter()
        .map(|a| match a {
            ArgValue::Num(v) => v,
            ArgValue::Text(_) => f64::NAN,
        })
        .collect()
}

/// Builds the command line for `filter`, e.g. `fx_halftone 0,0,0,0,5,8,8,5,0.1000`.
/// The arguments are a single comma-separated token; text values are quoted, so values
/// containing spaces (like `" .oO0"`) stay intact.
pub fn build_gmic_command(
    filter: &GmicFilter,
    values: &HashMap<usize, f64>,
    texts: &HashMap<usize, String>,
) -> String {
    let tokens: Vec<String> = argument_list(filter, values, texts)
        .into_iter()
        .map(|a| match a {
            ArgValue::Num(v) => {
                if (v - v.round()).abs() < 1e-5 {
                    format!("{}", v.round() as i64)
                } else {
                    format!("{v:.4}")
                }
            }
            ArgValue::Text(t) => quote_gmic_text(&t),
        })
        .collect();
    if tokens.is_empty() {
        filter.command.clone()
    } else {
        format!("{} {}", filter.command, tokens.join(","))
    }
}

/// Same as `build_gmic_command` with default text values.
#[allow(dead_code)]
pub fn build_gmic_args(filter: &GmicFilter, values: &HashMap<usize, f64>) -> String {
    build_gmic_command(filter, values, &HashMap::new())
}

/// Wraps a text value in double quotes for G'MIC. In G'MIC 4, double-quoted strings are
/// literal (no `$`/`{}` substitution); only `"` and `\` need escaping, and newlines are
/// written as `\n` because the command must stay on one line.
pub fn quote_gmic_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Locates the bundled G'MIC 4.0.6 CLI (falls back to `gmic` on PATH).
pub fn get_gmic_bin_path() -> PathBuf {
    static FOUND: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| {
            let mut candidates = Vec::new();
            if let Ok(val) = std::env::var("GMIC_BIN") {
                candidates.push(PathBuf::from(val));
            }
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(exe_dir) = exe_path.parent() {
                    candidates.push(exe_dir.join("bin/gmic"));
                    candidates.push(exe_dir.join("gmic"));
                    candidates.push(exe_dir.join("../bin/gmic"));
                    candidates.push(exe_dir.join("../usr/bin/gmic"));
                    // target/{debug,release}/GmicVideo -> repo root
                    candidates.push(exe_dir.join("../../bin/gmic"));
                }
            }
            candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bin/gmic"));
            if let Ok(cwd) = std::env::current_dir() {
                candidates.push(cwd.join("bin/gmic"));
            }
            candidates
                .into_iter()
                .find(|p| p.is_file())
                .unwrap_or_else(|| PathBuf::from("gmic"))
        })
        .clone()
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
    use std::process::Command;

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

    fn test_frame() -> crate::frame::Frame {
        let mut f = crate::frame::Frame::new(256, 192);
        for (i, v) in f.data.iter_mut().enumerate() {
            *v = ((i * 37) % 251) as u8;
        }
        f
    }

    #[test]
    fn test_halftone_effects_execution() {
        let db = FilterDatabase::load_default();
        let mut worker = crate::gmic_worker::GmicWorker::spawn(4).expect("spawn worker");
        let frame = test_frame();

        // 1. Standard Halftone (fx_halftone)
        let halftone = db.find_filter("Halftone").expect("Halftone filter should exist");
        assert_eq!(halftone.command, "fx_halftone");
        let cmd = build_gmic_args(halftone, &HashMap::new());
        assert_eq!(cmd, "fx_halftone 0,0,0,0,5,8,8,5,0.1000");
        worker.process(&frame, &cmd, 0).expect("Halftone (fx_halftone) execution failed");

        // 2. Halftone [Generic] and Halftone Shapes
        for name in ["Halftone [Generic]", "Halftone Shapes"] {
            if let Some(f) = db.find_filter(name) {
                let cmd = build_gmic_args(f, &HashMap::new());
                worker
                    .process(&frame, &cmd, 0)
                    .unwrap_or_else(|e| panic!("{name} execution failed: {e}"));
            }
        }
    }

    #[test]
    fn test_warhol_execution() {
        let db = FilterDatabase::load_default();
        let filter = db.find_filter("warhol").expect("Warhol filter should exist");
        let mut worker = crate::gmic_worker::GmicWorker::spawn(4).expect("spawn worker");
        let out = worker
            .process(&test_frame(), &build_gmic_args(filter, &HashMap::new()), 0)
            .expect("Warhol should run");
        assert_eq!((out.width, out.height), (256, 192));
    }

    #[test]
    fn test_text_parameters_keep_spaces() {
        let db = FilterDatabase::load_default();
        let mut worker = crate::gmic_worker::GmicWorker::spawn(4).expect("spawn worker");
        let frame = test_frame();

        for name in ["Ascii Art", "Polar Transform", "User-Defined"] {
            let filter = db.find_filter(name).unwrap_or_else(|| panic!("{name} should exist"));
            let cmd = build_gmic_args(filter, &HashMap::new());
            assert!(cmd.contains(' '), "{name}: {cmd}");
            assert_eq!(cmd.split_once(' ').unwrap().0, filter.command);
            worker
                .process(&frame, &cmd, 0)
                .unwrap_or_else(|e| panic!("{name} failed with command `{cmd}`: {e}"));
        }
    }

    #[test]
    fn test_text_values_override_defaults_and_are_escaped() {
        let db = FilterDatabase::load_default();
        let filter = db.find_filter("Ascii Art").expect("Ascii Art should exist");
        let text_param = filter
            .parameters
            .iter()
            .find(|p| matches!(p.param_type, GmicParamType::Text { .. }))
            .expect("text parameter");
        let mut texts = HashMap::new();
        texts.insert(text_param.id, "a \"b\" c".to_string());
        let cmd = build_gmic_command(filter, &HashMap::new(), &texts);
        assert!(cmd.contains(r#""a \"b\" c""#), "{cmd}");
        assert_eq!(quote_gmic_text("x\ny"), r#""x\ny""#);
    }

    #[test]
    fn test_color_defaults() {
        assert_eq!(parse_color_tuple("#000000ff"), (0, 0, 0, Some(255)));
        assert_eq!(parse_color_tuple("#ff8000"), (255, 128, 0, None));
        assert_eq!(parse_color_tuple("#fff"), (255, 255, 255, None));
        assert_eq!(parse_color_tuple("10,20,30"), (10, 20, 30, None));
        assert_eq!(parse_color_tuple("10,20,30,40"), (10, 20, 30, Some(40)));
        assert_eq!(parse_color_tuple(""), (0, 0, 0, None));

        // Vignette's color(#000000ff) must give black + alpha = 4 arguments.
        let db = FilterDatabase::load_default();
        let vignette = db.find_filter("Vignette").unwrap();
        assert_eq!(build_gmic_args(vignette, &HashMap::new()), "fx_vignette 70,70,95,0,0,0,255");
    }

    #[test]
    fn test_choice_values_are_clamped() {
        let db = FilterDatabase::load_default();
        let halftone = db.find_filter("Halftone").unwrap();
        let shape = halftone
            .parameters
            .iter()
            .find(|p| matches!(p.param_type, GmicParamType::Choice { .. }))
            .unwrap();
        let mut values = HashMap::new();
        values.insert(shape.id, 99.0);
        let cmd = build_gmic_args(halftone, &values);
        assert!(!cmd.contains(",99,"), "{cmd}");
    }
}
