// SPDX-License-Identifier: GPL-3.0-or-later
/*
 * Copyright (C) 2026 LTP - Linux Test Project
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use anyhow::Context;
use serde::Deserialize;
use std::path::{Path, PathBuf};

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

// ---------------------------------------------------------------------------
// Config struct
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct PatchworkInstance {
    pub url: String,
    pub project: String,
    pub alias: Option<String>,
    pub max_connections: usize,
    pub max_connections_per_host: usize,
    pub timeout_secs: u64,
}

impl PatchworkInstance {
    pub fn display_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.project)
    }
}

impl Default for PatchworkInstance {
    fn default() -> Self {
        Self {
            url: "https://patchwork.kernel.org/api".to_string(),
            project: "ltp".to_string(),
            alias: None,
            max_connections: 8,
            max_connections_per_host: 4,
            timeout_secs: 60,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub port: u16,
    pub max_patches: usize,
    pub checks: bool,
    pub instances: Vec<PatchworkInstance>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 3030,
            max_patches: 500,
            checks: false,
            instances: vec![PatchworkInstance::default()],
        }
    }
}

impl Config {
    /// Load config with priority: ~/.config/ltp-dashboard/config.toml > ./config.toml > defaults.
    pub fn load() -> anyhow::Result<Self> {
        let home_cfg = dirs_home().map(|h| h.join(".config/ltp-dashboard/config.toml"));
        Self::load_impl(home_cfg.as_deref(), Path::new("config.toml"))
    }

    fn load_impl(home_cfg: Option<&Path>, local_cfg: &Path) -> anyhow::Result<Self> {
        if let Some(p) = home_cfg.filter(|p| p.exists()) {
            return Self::load_from(p);
        }
        if local_cfg.exists() {
            return Self::load_from(local_cfg);
        }
        Ok(Self::default())
    }

    fn load_from(path: &Path) -> anyhow::Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
        let p = dir.join(name);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, content).unwrap();
        p
    }

    fn tmpdir() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "ltp-dashboard-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn defaults_are_sane() {
        let cfg = Config::default();
        assert_eq!(cfg.port, 3030);
        assert_eq!(cfg.max_patches, 500);
        assert!(!cfg.checks);
        assert_eq!(cfg.instances.len(), 1);
        assert_eq!(cfg.instances[0].project, "ltp");
    }

    #[test]
    fn no_files_returns_defaults() {
        let tmp = tmpdir();
        let cfg = Config::load_impl(
            Some(&tmp.join("absent.toml")),
            &tmp.join("also-absent.toml"),
        )
        .unwrap();
        assert_eq!(cfg.port, 3030);
        assert_eq!(cfg.max_patches, 500);
    }

    #[test]
    fn full_config_parsed() {
        let tmp = tmpdir();
        let path = write(
            &tmp,
            "config.toml",
            r#"
port = 8080
max_patches = 100
checks = true
claude_config_dir = "/tmp/agents"
ltp_repo = "https://example.com/ltp"
model = "claude-opus-4-6"
"#,
        );
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.max_patches, 100);
        assert!(cfg.checks);
    }

    #[test]
    fn partial_config_fills_defaults() {
        let tmp = tmpdir();
        let path = write(&tmp, "config.toml", "port = 9090\n");
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.port, 9090);
        assert_eq!(cfg.max_patches, 500); // default
        assert!(!cfg.checks); // default
    }

    #[test]
    fn invalid_toml_returns_error() {
        let tmp = tmpdir();
        let path = write(&tmp, "config.toml", "port = [[[not valid\n");
        assert!(Config::load_from(&path).is_err());
    }

    #[test]
    fn home_config_takes_priority_over_local() {
        let tmp = tmpdir();
        let home_cfg = write(&tmp, "home/config.toml", "port = 1111\n");
        let local_cfg = write(&tmp, "local/config.toml", "port = 2222\n");
        let cfg = Config::load_impl(Some(&home_cfg), &local_cfg).unwrap();
        assert_eq!(cfg.port, 1111);
    }

    #[test]
    fn local_config_used_when_home_absent() {
        let tmp = tmpdir();
        let local_cfg = write(&tmp, "local/config.toml", "port = 2222\n");
        let cfg = Config::load_impl(Some(&tmp.join("absent.toml")), &local_cfg).unwrap();
        assert_eq!(cfg.port, 2222);
    }

    #[test]
    fn defaults_when_home_is_none() {
        let tmp = tmpdir();
        let cfg = Config::load_impl(None, &tmp.join("absent.toml")).unwrap();
        assert_eq!(cfg.port, 3030);
    }
}
