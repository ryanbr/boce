use std::path::Path;

use sysinfo::{ProcessRefreshKind, RefreshKind, System};

/// Process names that indicate *some* Brave is currently running.
const BRAVE_EXE_NAMES: &[&str] = &[
    "brave.exe",
    "brave",
    "Brave Browser",
    "Brave-Browser",
    "brave-browser",
];

fn fresh_system() -> System {
    System::new_with_specifics(
        RefreshKind::new().with_processes(
            ProcessRefreshKind::new()
                .with_exe(sysinfo::UpdateKind::OnlyIfNotSet)
                .with_cmd(sysinfo::UpdateKind::OnlyIfNotSet)
                .with_cwd(sysinfo::UpdateKind::OnlyIfNotSet),
        ),
    )
}

/// True if *any* Brave process is running (any channel).
pub fn is_brave_running() -> bool {
    let sys = fresh_system();
    sys.processes()
        .values()
        .any(|p| name_matches_brave(p.name().to_string_lossy().as_ref()))
}

/// True if a Brave whose executable path sits under `install_root` is running.
/// On Windows this distinguishes Stable / Beta / Nightly even though they all
/// use "brave.exe" as the process name.
pub fn is_brave_running_for_install(install_root: &Path) -> bool {
    let sys = fresh_system();
    let root_lc = install_root.to_string_lossy().to_lowercase();

    for proc_ in sys.processes().values() {
        let name = proc_.name().to_string_lossy().to_lowercase();
        if !name_matches_brave(&name) {
            continue;
        }
        // Primary: exe() — only works if we can open the process handle.
        if let Some(exe) = proc_.exe()
            && path_starts_with(exe, install_root)
        {
            return true;
        }
        // Fallback 1: inspect the command line. Child renderer/utility
        // processes often have the exe path or `--type=...` args that include
        // the install dir.
        for arg in proc_.cmd() {
            let s = arg.to_string_lossy().to_lowercase();
            if s.contains(&root_lc) {
                return true;
            }
        }
        // Fallback 2: current working directory.
        if let Some(cwd) = proc_.cwd() {
            let s = cwd.to_string_lossy().to_lowercase();
            if s.starts_with(&root_lc) {
                return true;
            }
        }
    }
    false
}

fn name_matches_brave(name: &str) -> bool {
    let n = name.to_lowercase();
    BRAVE_EXE_NAMES
        .iter()
        .any(|b| n == b.to_lowercase() || n.starts_with(&b.to_lowercase()))
}

fn path_starts_with(full: &Path, root: &Path) -> bool {
    // Case-insensitive prefix match — Windows paths aren't case-sensitive.
    let f = full.to_string_lossy().to_lowercase();
    let r = root.to_string_lossy().to_lowercase();
    f.starts_with(&r)
}
