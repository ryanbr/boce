use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::paths::ChannelPaths;

#[derive(Debug, Clone)]
pub struct Profile {
    /// Directory name (e.g. "Default", "Profile 1").
    pub dir_name: String,
    /// Human-readable name from Local State, if known.
    pub display_name: Option<String>,
    /// Full path to the profile directory (contains `Preferences`).
    pub path: PathBuf,
}

impl Profile {
    pub fn prefs_path(&self) -> PathBuf {
        self.path.join("Preferences")
    }
}

/// Enumerate profiles by reading `Local State`, falling back to a directory scan.
pub fn list_profiles(paths: &ChannelPaths) -> Result<Vec<Profile>> {
    let local_state = paths.user_data.join("Local State");
    let mut profiles = Vec::new();

    if local_state.is_file() {
        let raw = std::fs::read_to_string(&local_state)
            .with_context(|| format!("reading {}", local_state.display()))?;
        let v: Value = serde_json::from_str(&raw)
            .with_context(|| format!("parsing {}", local_state.display()))?;
        if let Some(info_cache) = v
            .get("profile")
            .and_then(|p| p.get("info_cache"))
            .and_then(|c| c.as_object())
        {
            for (dir_name, info) in info_cache {
                let dir = paths.user_data.join(dir_name);
                if !dir.is_dir() {
                    continue;
                }
                let display = info
                    .get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string());
                profiles.push(Profile {
                    dir_name: dir_name.clone(),
                    display_name: display,
                    path: dir,
                });
            }
        }
    }

    if profiles.is_empty() {
        // Fallback: scan for directories that contain a Preferences file.
        if let Ok(entries) = std::fs::read_dir(&paths.user_data) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() && p.join("Preferences").is_file() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    profiles.push(Profile {
                        dir_name: name,
                        display_name: None,
                        path: p,
                    });
                }
            }
        }
    }

    profiles.sort_by(|a, b| a.dir_name.cmp(&b.dir_name));
    Ok(profiles)
}

/// Look up a profile by its directory name (case-insensitive on Windows semantics).
pub fn find_profile(paths: &ChannelPaths, name: &str) -> Result<Profile> {
    let all = list_profiles(paths)?;
    all.into_iter()
        .find(|p| p.dir_name.eq_ignore_ascii_case(name))
        .with_context(|| format!("profile '{}' not found", name))
}

pub fn resolve_profile_dir(paths: &ChannelPaths, name: Option<&str>) -> Result<Profile> {
    match name {
        Some(n) => find_profile(paths, n),
        None => find_profile(paths, "Default"),
    }
}

#[allow(dead_code)]
pub fn pretty_profile_label(p: &Profile) -> String {
    match &p.display_name {
        Some(name) => format!("{} ({})", p.dir_name, name),
        None => p.dir_name.clone(),
    }
}

#[allow(dead_code)]
pub fn is_profile_dir(p: &Path) -> bool {
    p.is_dir() && p.join("Preferences").is_file()
}
