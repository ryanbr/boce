// Windows-subsystem = "windows" in release builds: no console window is
// created on launch, so double-clicking the exe from Explorer has no flash.
// Trade-off: Ctrl+C in a terminal that launched the GUI won't forward to the
// process (cmd returns to prompt immediately because GUI-subsystem children
// are detached). Close the GUI window to exit.
//
// For CLI subcommands invoked from a terminal we call `AttachConsole` at
// startup so println!/eprintln! output lands in the parent console. That
// output may appear after the prompt on short commands (because cmd didn't
// wait for us) but the information still reaches the user.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod data;
mod filters;
mod gui;
mod install;
mod paths;
mod prefs;
mod process;
mod profile;
mod shields;

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};

use paths::{Channel, ChannelPaths, channel_paths, installed_channels};
use profile::{Profile, list_profiles, resolve_profile_dir};

#[derive(Parser, Debug)]
#[command(
    name = "brave-offline-config-editor",
    version,
    about = "Brave Offline Config Editor — edit Brave's cache, shields, privacy, filters, flags, and raw preferences while Brave is closed.",
    after_help = "\
EXAMPLES:
  brave-offline-config-editor list
      Show installed channels + profiles.

  brave-offline-config-editor --channel nightly clear-cache --dry-run
      Preview what would be deleted from the Default profile of Brave Nightly.

  brave-offline-config-editor --channel nightly clear-cache
      Actually clear the Default profile cache for Nightly.

  brave-offline-config-editor --channel beta clear-cache --all
      Clear cache for every profile under Beta.

  brave-offline-config-editor --channel stable clear-cache --profile \"Profile 1\"
      Clear a specific non-default profile by directory name.

  brave-offline-config-editor --channel beta shields list-keys
      List settable shield keys and their allowed values.

  brave-offline-config-editor --channel beta shields get
      Print current shield values for Beta's Default profile.

  brave-offline-config-editor --channel nightly shields set trackers-and-ads aggressive
      Set Trackers & Ads to Aggressive on Nightly (writes a .bak-<timestamp> first).

  brave-offline-config-editor --channel nightly backup --profile Default
      Manually back up a profile's Preferences file.

NOTE: Close Brave for the target channel before running mutating commands
      (clear-cache, shields set, restore). Use --force only if you are sure."
)]
struct Cli {
    /// Channel to target. Defaults to stable if installed.
    #[arg(long, global = true, value_parser = parse_channel)]
    channel: Option<Channel>,

    /// Skip the "Brave is running" safety check. Only use if you know it's closed.
    #[arg(long, global = true)]
    force: bool,

    #[command(subcommand)]
    cmd: Option<Cmd>,
}

/// Attach to the parent console if this GUI-subsystem process was launched
/// from cmd/PowerShell. That lets println!/eprintln! from CLI subcommands
/// land in the parent terminal. Returns `true` when we attached (i.e. there
/// was a parent console to attach to).
///
/// Launched from Explorer there's no parent console — AttachConsole returns
/// zero and we silently continue. Any stdout writes are dropped, which is
/// fine for GUI-only flows.
#[cfg(target_os = "windows")]
fn attach_parent_console_for_cli() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(target_os = "windows"))]
fn attach_parent_console_for_cli() {}

fn parse_channel(s: &str) -> Result<Channel, String> {
    match s.to_lowercase().as_str() {
        "stable" | "release" => Ok(Channel::Stable),
        "beta" => Ok(Channel::Beta),
        "nightly" => Ok(Channel::Nightly),
        "dev" => Ok(Channel::Dev),
        other => Err(format!("unknown channel '{}'", other)),
    }
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Launch the graphical interface (also the default if no subcommand given).
    Gui,

    /// List installed Brave channels and profiles.
    List,

    /// Clear browser cache (not cookies/history) for one or all profiles.
    ClearCache {
        /// Profile directory name. Omit for Default. Use --all for every profile.
        #[arg(long)]
        profile: Option<String>,
        /// Clear cache for every profile in the selected channel.
        #[arg(long, conflicts_with = "profile")]
        all: bool,
        /// Show what would be cleared without deleting anything.
        #[arg(long)]
        dry_run: bool,
    },

    /// Read/write Brave shield settings in the per-profile Preferences file.
    #[command(subcommand)]
    Shields(ShieldsCmd),

    /// Create a timestamped backup of a profile's Preferences file.
    Backup {
        #[arg(long)]
        profile: Option<String>,
    },

    /// Restore a Preferences.bak-<stamp> file back over Preferences.
    Restore {
        /// Full path to the .bak-<stamp> file.
        backup: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum ShieldsCmd {
    /// Print the allowlist of keys we know how to read/write.
    ListKeys,
    /// Print current values for one key or all known keys.
    Get {
        #[arg(long)]
        profile: Option<String>,
        /// Key name (see `list-keys`). Omit to print all.
        key: Option<String>,
    },
    /// Set a key to a symbolic value (e.g. `ads-default block`).
    Set {
        #[arg(long)]
        profile: Option<String>,
        key: String,
        value: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Default to GUI when no subcommand is supplied.
    let cmd = match cli.cmd {
        Some(c) => c,
        None => return gui::run(),
    };
    // Anything other than the explicit Gui subcommand is CLI output; attach
    // to the parent console (if any) so println! lands in the terminal.
    if !matches!(cmd, Cmd::Gui) {
        attach_parent_console_for_cli();
    }

    // Mutating commands must refuse to run if Brave is up — Brave rewrites
    // Preferences on shutdown and will clobber our edits otherwise.
    let mutates = matches!(
        cmd,
        Cmd::ClearCache { .. } | Cmd::Shields(ShieldsCmd::Set { .. }) | Cmd::Restore { .. }
    );
    if mutates && !cli.force && process::is_brave_running() {
        return Err(anyhow!(
            "Brave appears to be running. Close it first, or pass --force to override."
        ));
    }

    match cmd {
        Cmd::Gui => gui::run(),
        Cmd::List => cmd_list(),
        Cmd::ClearCache {
            profile,
            all,
            dry_run,
        } => {
            let paths = select_channel(cli.channel)?;
            cmd_clear_cache(&paths, profile.as_deref(), all, dry_run)
        }
        Cmd::Shields(ShieldsCmd::ListKeys) => {
            cmd_shields_list_keys();
            Ok(())
        }
        Cmd::Shields(ShieldsCmd::Get { profile, key }) => {
            let paths = select_channel(cli.channel)?;
            let prof = resolve_profile_dir(&paths, profile.as_deref())?;
            cmd_shields_get(&paths, &prof, key.as_deref())
        }
        Cmd::Shields(ShieldsCmd::Set {
            profile,
            key,
            value,
        }) => {
            let paths = select_channel(cli.channel)?;
            let prof = resolve_profile_dir(&paths, profile.as_deref())?;
            cmd_shields_set(&paths, &prof, &key, &value)
        }
        Cmd::Backup { profile } => {
            let paths = select_channel(cli.channel)?;
            let prof = resolve_profile_dir(&paths, profile.as_deref())?;
            let bkp = prefs::backup_file(&prof.prefs_path())?;
            println!("backup: {}", bkp.display());
            Ok(())
        }
        Cmd::Restore { backup } => {
            let restored = prefs::restore_backup(&backup)?;
            println!("restored: {}", restored.display());
            Ok(())
        }
    }
}

fn select_channel(explicit: Option<Channel>) -> Result<ChannelPaths> {
    if let Some(ch) = explicit {
        let p = channel_paths(ch)
            .ok_or_else(|| anyhow!("could not resolve paths for channel {:?}", ch))?;
        if !p.exists() {
            return Err(anyhow!(
                "channel {} not installed at {}",
                ch.name(),
                p.user_data.display()
            ));
        }
        return Ok(p);
    }
    let installed = installed_channels();
    if installed.is_empty() {
        return Err(anyhow!("no Brave installation found"));
    }
    if let Some(stable) = installed.iter().find(|p| p.channel == Channel::Stable) {
        return Ok(stable.clone());
    }
    Ok(installed[0].clone())
}

fn cmd_list() -> Result<()> {
    let installs = install::detect_all();
    let profile_roots = installed_channels();

    if installs.is_empty() && profile_roots.is_empty() {
        println!("No Brave installation or profile directory found.");
        return Ok(());
    }

    if installs.is_empty() {
        println!("Installations: none detected.");
    } else {
        println!("Installations:");
        for ins in &installs {
            let ver = ins.version.as_deref().unwrap_or("unknown");
            println!(
                "  [{}] v{} ({})",
                ins.channel.name(),
                ver,
                ins.scope.label()
            );
            println!("      exe: {}", ins.exe.display());
        }
    }

    println!();
    if profile_roots.is_empty() {
        println!("Profiles: none found.");
    } else {
        println!("Profiles (User Data):");
        for p in &profile_roots {
            println!("  [{}] {}", p.channel.name(), p.user_data.display());
            match list_profiles(p) {
                Ok(profiles) if profiles.is_empty() => println!("    (no profiles)"),
                Ok(profiles) => {
                    for prof in profiles {
                        match &prof.display_name {
                            Some(name) => println!("    - {} : {}", prof.dir_name, name),
                            None => println!("    - {}", prof.dir_name),
                        }
                    }
                }
                Err(e) => println!("    error listing profiles: {}", e),
            }
        }
    }
    Ok(())
}

fn cmd_clear_cache(
    paths: &ChannelPaths,
    profile: Option<&str>,
    all: bool,
    dry_run: bool,
) -> Result<()> {
    let targets: Vec<Profile> = if all {
        list_profiles(paths)?
    } else {
        vec![resolve_profile_dir(paths, profile)?]
    };

    if targets.is_empty() {
        return Err(anyhow!("no profiles to clear"));
    }

    // The CLI `clear-cache` subcommand is a shortcut for the "cache" category
    // of the Delete Data flow. All other categories (history / cookies /
    // passwords / autofill) require explicit user intent and live only in the
    // GUI's Delete Data tab.
    let cat = data::lookup("cache").ok_or_else(|| anyhow!("internal: cache category missing"))?;

    let mut total: u64 = 0;
    for prof in &targets {
        let report = data::clear(paths, prof, cat, dry_run)?;
        let action = if dry_run { "would free" } else { "freed" };
        println!(
            "[{}] {} {} across {} path(s)",
            prof.dir_name,
            action,
            data::format_bytes(report.bytes_freed),
            report.touched_paths.len()
        );
        for pth in &report.touched_paths {
            println!("    cleared: {}", pth.display());
        }
        for (pth, err) in &report.skipped {
            println!("    skipped: {} ({})", pth.display(), err);
        }
        total += report.bytes_freed;
    }
    if targets.len() > 1 {
        let action = if dry_run { "would free" } else { "freed" };
        println!("total {}: {}", action, data::format_bytes(total));
    }
    Ok(())
}

fn cmd_shields_list_keys() {
    println!("Known shield keys (name — description — allowed values):");
    for k in shields::KEYS {
        let vals: Vec<&str> = k.values.iter().map(|v| v.symbol).collect();
        println!("  {} — {} — [{}]", k.name, k.desc, vals.join(", "));
        println!("      paths: {}", shields::display_paths(k));
    }
}

fn cmd_shields_get(paths: &ChannelPaths, prof: &Profile, key: Option<&str>) -> Result<()> {
    let prefs_root = prefs::read_prefs(&prof.prefs_path())
        .with_context(|| format!("reading {}", prof.prefs_path().display()))?;
    let ls_root = prefs::read_prefs(&paths.local_state_path()).ok();

    let keys: Vec<&shields::ShieldKey> = match key {
        Some(name) => vec![shields::lookup(name)?],
        None => shields::KEYS.iter().collect(),
    };

    for k in keys {
        let root = match k.file {
            shields::PrefFile::Preferences => Some(&prefs_root),
            shields::PrefFile::LocalState => ls_root.as_ref(),
        };
        match root.and_then(|r| shields::current_symbol(r, k)) {
            Some(sym) => println!("{} = {}", k.name, sym),
            None => println!("{} = <default>", k.name),
        }
    }
    Ok(())
}

fn cmd_shields_set(paths: &ChannelPaths, prof: &Profile, key: &str, value: &str) -> Result<()> {
    let k = shields::lookup(key)?;
    let sv = shields::lookup_symbol(k, value)?;

    let target_path = match k.file {
        shields::PrefFile::Preferences => prof.prefs_path(),
        shields::PrefFile::LocalState => paths.local_state_path(),
    };
    let file_label = match k.file {
        shields::PrefFile::Preferences => "Preferences",
        shields::PrefFile::LocalState => "Local State",
    };

    let mut root = prefs::read_prefs(&target_path)?;
    let before = shields::current_symbol(&root, k);
    shields::apply_value(&mut root, sv)?;
    let backup = prefs::write_prefs(&target_path, &root)?;

    let disk = prefs::read_prefs(&target_path)?;
    let persisted = shields::verify_value(&disk, sv);

    println!("profile: {}", prof.dir_name);
    println!("key:     {} ({})", k.name, file_label);
    println!("before:  {}", before.unwrap_or("<default>"));
    println!("after:   {}", sv.symbol);
    println!("backup:  {}", backup.display());
    if persisted {
        println!("status:  verified on disk");
    } else {
        println!(
            "status:  WARNING — values did not persist. Close every Brave window (including tray) and retry."
        );
    }
    Ok(())
}
