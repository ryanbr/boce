//! Detection of installed Brave *binaries* (distinct from profile dirs).
//!
//! On Windows, Brave follows the Chromium updater layout:
//!
//!   <install_root>\Application\brave.exe        (launcher stub)
//!   <install_root>\Application\<version>\...    (actual browser assets)
//!   <install_root>\Application\Last Version     (text file with current version)
//!
//! Install roots:
//!   Per-user:  %LOCALAPPDATA%\BraveSoftware\Brave-Browser[-Beta|-Nightly|-Dev]
//!   System:    %ProgramFiles%\BraveSoftware\Brave-Browser[-Beta|-Nightly|-Dev]
//!              %ProgramFiles(x86)%\BraveSoftware\...  (legacy 32-bit installers)
//!
//! On Linux, packaged installs live at /opt/brave.com/<channel>/.
//! On macOS, at /Applications/Brave Browser[ Beta|...].app.

use std::path::{Path, PathBuf};

use crate::paths::Channel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // variants constructed per-platform; some unused in certain cfgs
pub enum InstallScope {
    PerUser,
    System,
    Unknown,
}

impl InstallScope {
    pub fn label(self) -> &'static str {
        match self {
            InstallScope::PerUser => "per-user",
            InstallScope::System => "system",
            InstallScope::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // application_dir is useful metadata even when not printed
pub struct Installation {
    pub channel: Channel,
    pub scope: InstallScope,
    /// Directory containing brave.exe (the Application\ dir on Windows).
    pub application_dir: PathBuf,
    /// Absolute path to the brave launcher executable.
    pub exe: PathBuf,
    /// Resolved version string, e.g. "1.70.117". None if we couldn't determine it.
    pub version: Option<String>,
}

#[allow(dead_code)] // used only in the Windows candidate_roots
fn channel_dir_name(ch: Channel) -> &'static str {
    match ch {
        Channel::Stable => "Brave-Browser",
        Channel::Beta => "Brave-Browser-Beta",
        Channel::Nightly => "Brave-Browser-Nightly",
        Channel::Dev => "Brave-Browser-Dev",
    }
}

#[cfg(target_os = "windows")]
fn candidate_roots(ch: Channel) -> Vec<(PathBuf, InstallScope)> {
    let mut out = Vec::new();
    let name = channel_dir_name(ch);
    if let Some(local) = dirs::data_local_dir() {
        out.push((local.join("BraveSoftware").join(name), InstallScope::PerUser));
    }
    for env_var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Ok(pf) = std::env::var(env_var) {
            out.push((
                PathBuf::from(pf).join("BraveSoftware").join(name),
                InstallScope::System,
            ));
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn candidate_roots(ch: Channel) -> Vec<(PathBuf, InstallScope)> {
    let subdir = match ch {
        Channel::Stable => "brave",
        Channel::Beta => "brave-beta",
        Channel::Nightly => "brave-nightly",
        Channel::Dev => "brave-dev",
    };
    vec![(PathBuf::from("/opt/brave.com").join(subdir), InstallScope::System)]
}

#[cfg(target_os = "macos")]
fn candidate_roots(ch: Channel) -> Vec<(PathBuf, InstallScope)> {
    let app = match ch {
        Channel::Stable => "Brave Browser.app",
        Channel::Beta => "Brave Browser Beta.app",
        Channel::Nightly => "Brave Browser Nightly.app",
        Channel::Dev => "Brave Browser Dev.app",
    };
    vec![(
        PathBuf::from("/Applications").join(app).join("Contents/MacOS"),
        InstallScope::System,
    )]
}

/// Given an install root, locate the Application directory + launcher exe.
#[cfg(target_os = "windows")]
fn resolve_exe(root: &Path) -> Option<(PathBuf, PathBuf)> {
    let app = root.join("Application");
    let exe = app.join("brave.exe");
    if exe.is_file() {
        Some((app, exe))
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn resolve_exe(root: &Path) -> Option<(PathBuf, PathBuf)> {
    for name in ["brave", "brave-browser", "brave-browser-beta", "brave-browser-nightly"] {
        let exe = root.join(name);
        if exe.is_file() {
            return Some((root.to_path_buf(), exe));
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn resolve_exe(root: &Path) -> Option<(PathBuf, PathBuf)> {
    for name in ["Brave Browser", "Brave Browser Beta", "Brave Browser Nightly", "Brave Browser Dev"] {
        let exe = root.join(name);
        if exe.is_file() {
            return Some((root.to_path_buf(), exe));
        }
    }
    None
}

/// Read the version string from a Chromium-style Application dir.
/// Strategy:
///   1. Read `Last Version` (a tiny text file written by the updater).
///   2. Fall back to the highest-sorted versioned subdirectory.
fn detect_version(application_dir: &Path) -> Option<String> {
    let last = application_dir.join("Last Version");
    if let Ok(text) = std::fs::read_to_string(&last) {
        let trimmed = text.trim();
        if !trimmed.is_empty() && looks_like_version(trimmed) {
            return Some(trimmed.to_string());
        }
    }

    let mut versions: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(application_dir) {
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if looks_like_version(&name) {
                versions.push(name);
            }
        }
    }
    versions.sort_by(|a, b| compare_versions(a, b));
    versions.pop()
}

fn looks_like_version(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let pa: Vec<u64> = a.split('.').filter_map(|p| p.parse().ok()).collect();
    let pb: Vec<u64> = b.split('.').filter_map(|p| p.parse().ok()).collect();
    pa.cmp(&pb)
}

pub fn detect_for_channel(ch: Channel) -> Vec<Installation> {
    let mut out: Vec<Installation> = Vec::new();
    for (root, scope) in candidate_roots(ch) {
        let Some((app_dir, exe)) = resolve_exe(&root) else {
            continue;
        };
        // %ProgramFiles% and %ProgramW6432% resolve to the same directory on
        // 64-bit Windows; canonicalize and skip if we've already recorded this exe.
        let key = std::fs::canonicalize(&exe).unwrap_or_else(|_| exe.clone());
        let already_seen = out.iter().any(|i| {
            std::fs::canonicalize(&i.exe).unwrap_or_else(|_| i.exe.clone()) == key
        });
        if already_seen {
            continue;
        }
        let version = detect_version(&app_dir);
        out.push(Installation {
            channel: ch,
            scope,
            application_dir: app_dir,
            exe,
            version,
        });
    }
    out
}

pub fn detect_all() -> Vec<Installation> {
    let mut out = Vec::new();
    for ch in Channel::ALL {
        out.extend(detect_for_channel(ch));
    }
    out
}
