use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub default: Option<DefaultConfig>,
    pub limits: Option<LimitsConfig>,
    pub output: Option<OutputConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DefaultConfig {
    pub logic_only: Option<bool>,
    pub json: Option<bool>,
    pub no_incremental: Option<bool>,
    pub no_pager: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LimitsConfig {
    pub max_file_size: Option<usize>,
    pub max_ast_nodes: Option<usize>,
    pub max_recursion_depth: Option<usize>,
    pub parse_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutputConfig {
    pub color: Option<String>,
}

impl Config {
    /// Load configuration by searching local repo path, then home directory, or returning default.
    pub fn load(repo_path: &Path, custom_path: Option<&Path>) -> Self {
        if let Some(path) = custom_path {
            if let Ok(config) = Self::load_from_file(path) {
                return config;
            }
        }

        // Check local repo directory: .symtracerc or symtrace.toml
        let local_rc = repo_path.join(".symtracerc");
        if local_rc.exists() {
            if let Ok(c) = Self::load_from_file(&local_rc) {
                return c;
            }
        }
        let local_toml = repo_path.join("symtrace.toml");
        if local_toml.exists() {
            if let Ok(c) = Self::load_from_file(&local_toml) {
                return c;
            }
        }

        // Check global user configuration
        if let Some(home) = home_dir() {
            let global_rc = home.join(".symtracerc");
            if global_rc.exists() {
                if let Ok(c) = Self::load_from_file(&global_rc) {
                    return c;
                }
            }
            let global_toml = home.join(".config").join("symtrace").join("symtrace.toml");
            if global_toml.exists() {
                if let Ok(c) = Self::load_from_file(&global_toml) {
                    return c;
                }
            }
        }

        Config::default()
    }

    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

fn home_dir() -> Option<PathBuf> {
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return Some(PathBuf::from(h));
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(p) = std::env::var("USERPROFILE") {
            if !p.is_empty() {
                return Some(PathBuf::from(p));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_toml_config() {
        let toml_str = r#"
[default]
logic_only = true
json = false

[limits]
max_file_size = 10485760
max_ast_nodes = 500000

[output]
color = "always"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default.unwrap().logic_only, Some(true));
        assert_eq!(config.limits.unwrap().max_file_size, Some(10_485_760));
    }
}
