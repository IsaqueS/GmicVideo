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
        self.filters.iter().find(|f| {
            f.name.eq_ignore_ascii_case(name_or_cmd)
                || f.command.eq_ignore_ascii_case(name_or_cmd)
                || f.id.eq_ignore_ascii_case(name_or_cmd)
        })
    }
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
    let param_re = Regex::new(r"^:([^=]+)=(.*)$").map_err(|e| e.to_string())?;

    for line in content.lines() {
        let line = line.trim();
        if !line.starts_with("#@gui") {
            continue;
        }

        if let Some(caps) = tag_re.captures(line) {
            let lang = caps.get(1).map(|m| m.as_str());
            // Prefer default/English tags
            if lang.is_some() && lang != Some("_en") {
                continue;
            }

            let rest = caps.get(2).map(|m| m.as_str()).unwrap_or("");

            // Parameter line for current filter
            if rest.starts_with(':') {
                if let Some(ref mut filter) = current_filter {
                    if let Some(p_caps) = param_re.captures(rest) {
                        let pname = p_caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();
                        let pval = p_caps.get(2).map(|m| m.as_str()).unwrap_or("").trim();

                        let parsed_params = parse_param_definition(pname, pval);
                        for mut p in parsed_params {
                            p.id = filter.parameters.len();
                            if let GmicParamType::Note { ref text } = p.param_type {
                                if filter.description.is_empty() {
                                    filter.description = strip_html(text);
                                }
                            }
                            filter.parameters.push(p);
                        }
                    }
                }
                continue;
            }

            // Category or Subcategory header like: <b>Patterns</b> or _<b>Deformations</b>
            if rest.contains("<b>") || rest.contains("<i>") {
                let clean = rest
                    .replace("<b>", "")
                    .replace("</b>", "")
                    .replace("<i>", "")
                    .replace("</i>", "")
                    .trim_matches('_')
                    .trim()
                    .to_string();

                if !clean.is_empty() && !clean.contains('=') && !clean.contains('{') && !clean.contains('(') {
                    current_category = clean;
                    current_subcategory = None;
                }
                continue;
            }

            // Filter definition line: Filter Name : command, preview_command
            if let Some((name_part, cmd_part)) = rest.split_once(':') {
                let name = name_part.trim();
                if name.is_empty() || name.starts_with('_') {
                    continue;
                }

                // If we had a previous filter, push it
                if let Some(prev) = current_filter.take() {
                    if !prev.command.is_empty() && prev.command != "_none_" {
                        parsed_filters.push(prev);
                    }
                }

                let cmds: Vec<&str> = cmd_part.split(',').map(|s| s.trim()).collect();
                let command = cmds.first().copied().unwrap_or("").to_string();
                let preview = cmds.get(1).copied().unwrap_or(&command).to_string();

                if command.is_empty() || command == "_none_" {
                    continue;
                }

                let id = format!("{}_{}", sanitize_id(&current_category), sanitize_id(name));

                current_filter = Some(GmicFilter {
                    id,
                    name: name.to_string(),
                    category: current_category.clone(),
                    subcategory: current_subcategory.clone(),
                    command,
                    preview_command: preview,
                    description: String::new(),
                    author: String::new(),
                    parameters: Vec::new(),
                });
            }
        }
    }

    if let Some(last) = current_filter.take() {
        if !last.command.is_empty() && last.command != "_none_" {
            parsed_filters.push(last);
        }
    }

    // Let update374.gmic be the primary authority
    let mut final_filters = Vec::new();
    let mut seen_commands = std::collections::HashSet::new();

    for f in parsed_filters {
        if !seen_commands.contains(&f.command) {
            seen_commands.insert(f.command.clone());
            final_filters.push(f);
        }
    }

    // Append any builtin fallbacks that aren't already present
    for f in get_builtin_filters().filters {
        if !seen_commands.contains(&f.command) {
            seen_commands.insert(f.command.clone());
            final_filters.push(f);
        }
    }

    let mut cat_set = std::collections::BTreeSet::new();
    for f in &final_filters {
        cat_set.insert(f.category.clone());
    }

    Ok(FilterDatabase {
        filters: final_filters,
        categories: cat_set.into_iter().collect(),
    })
}

fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect()
}

fn strip_html(input: &str) -> String {
    let re = Regex::new(r"<[^>]+>").unwrap();
    re.replace_all(input, "").trim().to_string()
}

fn parse_color_tuple(args: &str) -> (i64, i64, i64, Option<i64>) {
    let clean = args.trim().trim_matches('"').trim_matches('\'');
    if clean.starts_with('#') {
        let hex = clean.trim_start_matches('#');
        if hex.len() >= 6 {
            let r = i64::from_str_radix(&hex[0..2], 16).unwrap_or(255);
            let g = i64::from_str_radix(&hex[2..4], 16).unwrap_or(255);
            let b = i64::from_str_radix(&hex[4..6], 16).unwrap_or(255);
            let a = if hex.len() >= 8 {
                i64::from_str_radix(&hex[6..8], 16).ok()
            } else {
                None
            };
            return (r, g, b, a);
        }
    }
    let parts = parse_csv_args(clean);
    let r = parts.first().and_then(|s| s.parse::<i64>().ok()).unwrap_or(255);
    let g = parts.get(1).and_then(|s| s.parse::<i64>().ok()).unwrap_or(255);
    let b = parts.get(2).and_then(|s| s.parse::<i64>().ok()).unwrap_or(255);
    let a = parts.get(3).and_then(|s| s.parse::<i64>().ok());
    (r, g, b, a)
}

fn parse_param_definition(name: &str, val: &str) -> Vec<GmicParameter> {
    let clean_name = name
        .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ' || c == ':')
        .trim()
        .to_string();

    let open_idx = match val.find(|c| c == '(' || c == '{') {
        Some(idx) => idx,
        None => return Vec::new(),
    };
    let close_idx = match val.rfind(|c| c == ')' || c == '}') {
        Some(idx) => idx,
        None => return Vec::new(),
    };
    if close_idx <= open_idx {
        return Vec::new();
    }

    let ptype_raw = &val[..open_idx];
    let ptype = ptype_raw
        .trim()
        .trim_matches(|c| c == '_' || c == '~' || c == '=' || c == ' ')
        .to_ascii_lowercase();
    let args = val[open_idx + 1..close_idx].trim();

    match ptype.as_str() {
        "float" => {
            let parts = parse_csv_args(args);
            let default = parts.first().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            let min = parts.get(1).and_then(|s| s.parse::<f64>().ok()).unwrap_or(default - 100.0);
            let max = parts.get(2).and_then(|s| s.parse::<f64>().ok()).unwrap_or(default + 100.0);
            vec![GmicParameter {
                id: 0,
                name: if clean_name.is_empty() { "Amount".into() } else { clean_name },
                param_type: GmicParamType::Float { default, min, max },
            }]
        }
        "int" => {
            let parts = parse_csv_args(args);
            let default = parts.first().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            let min = parts.get(1).and_then(|s| s.parse::<i64>().ok()).unwrap_or(default - 100);
            let max = parts.get(2).and_then(|s| s.parse::<i64>().ok()).unwrap_or(default + 100);
            vec![GmicParameter {
                id: 0,
                name: if clean_name.is_empty() { "Value".into() } else { clean_name },
                param_type: GmicParamType::Int { default, min, max },
            }]
        }
        "bool" => {
            let default = args.parse::<i32>().unwrap_or(0) != 0 || args.eq_ignore_ascii_case("true");
            vec![GmicParameter {
                id: 0,
                name: if clean_name.is_empty() { "Enable".into() } else { clean_name },
                param_type: GmicParamType::Bool { default },
            }]
        }
        "choice" => {
            let parts = parse_csv_args(args);
            let mut opts = Vec::new();
            let mut default = 0;
            for (idx, p) in parts.iter().enumerate() {
                let opt_str = p.trim().trim_matches('"').trim_matches('\'').to_string();
                if idx == 0 {
                    if let Ok(d) = opt_str.parse::<usize>() {
                        default = d;
                        continue;
                    }
                }
                opts.push(opt_str);
            }
            vec![GmicParameter {
                id: 0,
                name: if clean_name.is_empty() { "Option".into() } else { clean_name },
                param_type: GmicParamType::Choice { default, options: opts },
            }]
        }
        "point" => {
            let parts = parse_csv_args(args);
            let x_def = parts.first().and_then(|s| s.parse::<f64>().ok()).unwrap_or(50.0);
            let y_def = parts.get(1).and_then(|s| s.parse::<f64>().ok()).unwrap_or(50.0);
            let base_name = if clean_name.is_empty() { "Point".to_string() } else { clean_name };
            vec![
                GmicParameter {
                    id: 0,
                    name: format!("{base_name} X"),
                    param_type: GmicParamType::Float { default: x_def, min: -100.0, max: 200.0 },
                },
                GmicParameter {
                    id: 0,
                    name: format!("{base_name} Y"),
                    param_type: GmicParamType::Float { default: y_def, min: -100.0, max: 200.0 },
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
            let default_num = args.parse::<f64>().unwrap_or(0.0);
            vec![GmicParameter {
                id: 0,
                name: if clean_name.is_empty() { "Value".into() } else { clean_name },
                param_type: GmicParamType::Float { default: default_num, min: -1000.0, max: 1000.0 },
            }]
        }
        "button" => {
            vec![GmicParameter {
                id: 0,
                name: if clean_name.is_empty() { "Trigger".into() } else { clean_name.clone() },
                param_type: GmicParamType::Button { name: clean_name },
            }]
        }
        "text" | "file" | "filein" | "fileout" | "folder" => {
            vec![GmicParameter {
                id: 0,
                name: if clean_name.is_empty() { "Text".into() } else { clean_name },
                param_type: GmicParamType::Text { default: args.trim_matches('"').trim_matches('\'').to_string() },
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

/// Builds G'MIC CLI argument string for a filter given the parameter values map
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
                let (r, g, b, _) = parse_color_tuple(default_hex);
                arg_tokens.push(format!("{r},{g},{b}"));
            }
            GmicParamType::Text { default } => {
                arg_tokens.push(format!("\"{}\"", default));
            }
            GmicParamType::Button { .. } => {
                let val = values.get(&param.id).map(|v| v.round() as i64).unwrap_or(0);
                arg_tokens.push(val.to_string());
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
            cwd.join("assets/bin/gmic"),
        ];
        for cand in &candidates {
            if cand.exists() {
                return cand.to_path_buf();
            }
        }
    }

    PathBuf::from("gmic")
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
    ];

    for p in cmd_parts {
        args.push(p.to_string());
    }

    args.push("-keep[0]".into());
    args.push("-to_rgb".into());
    args.push("-cut".into());
    args.push("0,255".into());
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
        param_type: GmicParamType::Float { default: 0.0, min: 0.0, max: 20.0 },
    });
    halftone_params.push(GmicParameter {
        id: 4,
        name: "Number of Tones".into(),
        param_type: GmicParamType::Int { default: 5, min: 2, max: 20 },
    });
    halftone_params.push(GmicParameter {
        id: 5,
        name: "Size for Dark Tones".into(),
        param_type: GmicParamType::Float { default: 8.0, min: 1.0, max: 50.0 },
    });
    halftone_params.push(GmicParameter {
        id: 6,
        name: "Size for Bright Tones".into(),
        param_type: GmicParamType::Float { default: 8.0, min: 1.0, max: 50.0 },
    });
    halftone_params.push(GmicParameter {
        id: 7,
        name: "Shape".into(),
        param_type: GmicParamType::Choice {
            default: 0,
            options: vec!["Circle (Inv.)".into(), "Circle".into(), "Diamond".into(), "Square".into(), "Line".into()],
        },
    });
    halftone_params.push(GmicParameter {
        id: 8,
        name: "Dot Smoothness".into(),
        param_type: GmicParamType::Float { default: 0.1, min: 0.0, max: 1.0 },
    });

    filters.push(GmicFilter {
        id: "patterns_halftone".into(),
        name: "Halftone".into(),
        category: "Patterns".into(),
        subcategory: None,
        command: "fx_halftone".into(),
        preview_command: "fx_halftone_preview".into(),
        description: "Simulates a printing halftone pattern (dots) with customizable dot size and tones.".into(),
        author: "David Tschumperlé".into(),
        parameters: halftone_params,
    });

    let categories = filters.iter().map(|f| f.category.clone()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();

    FilterDatabase { filters, categories }
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
}
