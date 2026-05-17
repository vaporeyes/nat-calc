// ABOUTME: Loads optional REPL configuration from the user's home directory.
// ABOUTME: Minimal `key = value` format; controls history line-editing.

use std::path::PathBuf;

/// REPL configuration. Defaults apply when the config file is absent.
#[derive(Debug, Clone)]
pub struct Config {
    /// Enable history navigation / line editing (Up/Down arrows).
    pub history: bool,
    /// Where persistent history is read from and written to.
    pub history_file: PathBuf,
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

impl Default for Config {
    fn default() -> Self {
        let history_file = home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".nat_calc_history");
        Config {
            history: true,
            history_file,
        }
    }
}

/// Load `~/.nat_calc.conf` if present. Unknown keys are ignored; a missing
/// or unreadable file yields defaults (history enabled).
pub fn load() -> Config {
    let mut cfg = Config::default();

    let Some(path) = home_dir().map(|h| h.join(".nat_calc.conf")) else {
        return cfg;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return cfg;
    };

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "history" => cfg.history = parse_bool(value, cfg.history),
            "history_file" => cfg.history_file = expand_tilde(value),
            _ => {}
        }
    }
    cfg
}

fn parse_bool(value: &str, default: bool) -> bool {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => true,
        "false" | "0" | "no" | "off" => false,
        _ => default,
    }
}

fn expand_tilde(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(value)
}
