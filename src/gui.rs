use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use eframe::egui;
use serde_json::Value;

use crate::data;
use crate::filters;
use crate::install::{self, Installation};
use crate::paths::{Channel, ChannelPaths, channel_paths};
use crate::prefs;
use crate::process;
use crate::profile::{self, Profile};
use crate::shields::{self, ShieldKey, ShieldValue};

#[derive(PartialEq, Eq)]
enum Tab {
    // Brave-analogous tabs, in Brave's sidebar order.
    GettingStarted,
    Shields,
    Filters,
    Privacy,
    DeleteData,
    System,
    // Tool-specific / power-user tabs.
    Flags,
    Preferences,
    Backups,
}

pub struct App {
    installs: Vec<Installation>,
    selected_channel: Option<Channel>,

    paths: Option<ChannelPaths>,
    profiles: Vec<Profile>,
    selected_profile: Option<String>,

    // Delete Data view state.
    data_profile_checked: HashMap<String, bool>,
    data_category_checked: HashMap<String, bool>,
    data_dialog_open: bool,
    data_dialog_size_bytes: u64,
    data_dialog_profiles: Vec<String>,
    data_dialog_categories: Vec<&'static str>,
    last_data_report: Vec<String>,

    // Shields view state.
    shield_current: HashMap<String, Option<&'static str>>,
    shield_pending: HashMap<String, String>,

    // Backups view state.
    backup_files: Vec<PathBuf>,
    diff_output: Vec<String>,
    diff_from: Option<PathBuf>,

    // Raw Preferences editor state.
    prefs_buffer: String,
    prefs_loaded_from: Option<PathBuf>,
    prefs_parse_error: Option<String>,
    prefs_dirty: bool,
    prefs_search: String,

    // Flags view state.
    flags_original: Vec<String>,
    flags_working: Vec<String>,
    flags_add_buffer: String,

    // Filters view state.
    filter_catalog: Vec<filters::CatalogEntry>,
    filter_regional_current: HashMap<String, bool>,
    filter_regional_pending: HashMap<String, bool>,
    filter_subscriptions_current: Vec<filters::Subscription>,
    filter_subscription_toggles: HashMap<String, bool>,
    filter_subscription_removes: Vec<String>,
    filter_sub_add_buffer: String,
    filter_custom_original: String,
    filter_custom_working: String,
    filter_search: String,

    // Getting Started tab state.
    gs_profile_name_original: String,
    gs_profile_name_working: String,
    gs_startup_original: Option<i64>,
    gs_startup_working: Option<i64>,

    brave_running: bool,
    last_process_poll: Option<std::time::Instant>,
    force: bool,
    tab: Tab,
    status: String,
    error: Option<String>,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            installs: Vec::new(),
            selected_channel: None,
            paths: None,
            profiles: Vec::new(),
            selected_profile: None,
            data_profile_checked: HashMap::new(),
            data_category_checked: {
                let mut m = HashMap::new();
                for c in data::CATEGORIES {
                    m.insert(c.id.to_string(), c.default_checked);
                }
                m
            },
            data_dialog_open: false,
            data_dialog_size_bytes: 0,
            data_dialog_profiles: Vec::new(),
            data_dialog_categories: Vec::new(),
            last_data_report: Vec::new(),
            shield_current: HashMap::new(),
            shield_pending: HashMap::new(),
            backup_files: Vec::new(),
            diff_output: Vec::new(),
            diff_from: None,
            prefs_buffer: String::new(),
            prefs_loaded_from: None,
            prefs_parse_error: None,
            prefs_dirty: false,
            prefs_search: String::new(),
            flags_original: Vec::new(),
            flags_working: Vec::new(),
            flags_add_buffer: String::new(),
            filter_catalog: Vec::new(),
            filter_regional_current: HashMap::new(),
            filter_regional_pending: HashMap::new(),
            filter_subscriptions_current: Vec::new(),
            filter_subscription_toggles: HashMap::new(),
            filter_subscription_removes: Vec::new(),
            filter_sub_add_buffer: String::new(),
            filter_custom_original: String::new(),
            filter_custom_working: String::new(),
            filter_search: String::new(),
            gs_profile_name_original: String::new(),
            gs_profile_name_working: String::new(),
            gs_startup_original: None,
            gs_startup_working: None,
            brave_running: false,
            last_process_poll: None,
            force: false,
            tab: Tab::GettingStarted,
            status: String::new(),
            error: None,
        };
        app.refresh_all();
        app
    }

    fn refresh_all(&mut self) {
        self.installs = install::detect_all();
        if self.selected_channel.is_none()
            && let Some(first) = self.installs.first()
        {
            self.selected_channel = Some(first.channel);
        }
        self.reload_channel();
        self.refresh_brave_running();
    }

    fn refresh_brave_running(&mut self) {
        self.brave_running = match self.current_install() {
            Some(ins) => {
                let root = ins.application_dir.parent().unwrap_or(&ins.application_dir);
                process::is_brave_running_for_install(root)
            }
            None => process::is_brave_running(),
        };
    }

    fn current_install(&self) -> Option<&install::Installation> {
        let ch = self.selected_channel?;
        self.installs.iter().find(|i| i.channel == ch)
    }

    fn reload_channel(&mut self) {
        self.paths = self.selected_channel.and_then(channel_paths);
        self.profiles.clear();
        self.selected_profile = None;
        self.data_profile_checked.clear();
        self.data_dialog_open = false;
        self.last_data_report.clear();
        self.flags_original.clear();
        self.flags_working.clear();
        self.flags_add_buffer.clear();
        self.filter_catalog.clear();
        self.filter_regional_current.clear();
        self.filter_regional_pending.clear();
        self.filter_subscriptions_current.clear();
        self.filter_subscription_toggles.clear();
        self.filter_subscription_removes.clear();
        self.filter_sub_add_buffer.clear();
        self.filter_custom_original.clear();
        self.filter_custom_working.clear();

        if let Some(p) = &self.paths {
            match profile::list_profiles(p) {
                Ok(list) => {
                    let default_data_profile = self
                        .selected_profile
                        .clone()
                        .or_else(|| list.first().map(|p| p.dir_name.clone()));
                    for prof in &list {
                        let default = Some(&prof.dir_name) == default_data_profile.as_ref();
                        self.data_profile_checked
                            .insert(prof.dir_name.clone(), default);
                    }
                    if let Some(first) = list.first() {
                        self.selected_profile = Some(first.dir_name.clone());
                    }
                    self.profiles = list;
                }
                Err(e) => self.error = Some(format!("listing profiles: {}", e)),
            }

            // Filter catalog (non-fatal if missing).
            if let Ok(catalog) = filters::load_catalog(&p.user_data) {
                self.filter_catalog = catalog;
            }

            if let Ok(root) = prefs::read_prefs(&p.local_state_path()) {
                if let Some(v) = prefs::get_path(&root, "browser.enabled_labs_experiments")
                    && let Some(arr) = v.as_array()
                {
                    let list: Vec<String> = arr
                        .iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect();
                    self.flags_original = list.clone();
                    self.flags_working = list;
                }
                if let Some(dict) =
                    prefs::get_path(&root, filters::PREF_REGIONAL).and_then(|v| v.as_object())
                {
                    for (uuid, entry) in dict {
                        if let Some(en) = entry.get("enabled").and_then(|b| b.as_bool()) {
                            self.filter_regional_current.insert(uuid.clone(), en);
                        }
                    }
                }
                self.filter_subscriptions_current = filters::read_subscriptions(&root);
                self.filter_custom_original = filters::read_custom_filters(&root);
                self.filter_custom_working = self.filter_custom_original.clone();
            }
        }
        self.reload_profile_views();
    }

    fn reload_profile_views(&mut self) {
        self.shield_current.clear();
        self.shield_pending.clear();
        self.backup_files.clear();
        self.diff_output.clear();
        self.diff_from = None;
        self.prefs_buffer.clear();
        self.prefs_loaded_from = None;
        self.prefs_parse_error = None;
        self.prefs_dirty = false;

        self.gs_profile_name_original.clear();
        self.gs_profile_name_working.clear();
        self.gs_startup_original = None;
        self.gs_startup_working = None;

        let Some(prof) = self.current_profile().cloned() else {
            return;
        };

        let prefs_root = prefs::read_prefs(&prof.prefs_path()).ok();
        let local_state_root = self
            .paths
            .as_ref()
            .and_then(|p| prefs::read_prefs(&p.local_state_path()).ok());
        if prefs_root.is_none() {
            self.error = Some("could not read Preferences".to_string());
        }
        for k in shields::KEYS {
            let root = match k.file {
                shields::PrefFile::Preferences => prefs_root.as_ref(),
                shields::PrefFile::LocalState => local_state_root.as_ref(),
            };
            let sym = root.and_then(|r| shields::current_symbol(r, k));
            self.shield_current.insert(k.name.to_string(), sym);
        }

        if let Some(r) = prefs_root.as_ref() {
            let name = prefs::get_path(r, "profile.name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.gs_profile_name_original = name.clone();
            self.gs_profile_name_working = name;
            let startup = prefs::get_path(r, "session.restore_on_startup").and_then(|v| v.as_i64());
            self.gs_startup_original = startup;
            self.gs_startup_working = startup;
        }

        if let Ok(entries) = std::fs::read_dir(&prof.path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.contains(".bak-") {
                    self.backup_files.push(entry.path());
                }
            }
            self.backup_files.sort();
        }
    }

    fn current_profile(&self) -> Option<&Profile> {
        let name = self.selected_profile.as_deref()?;
        self.profiles.iter().find(|p| p.dir_name == name)
    }

    fn installed_version(&self, ch: Channel) -> Option<&str> {
        self.installs
            .iter()
            .find(|i| i.channel == ch)
            .and_then(|i| i.version.as_deref())
    }

    fn can_mutate(&self) -> bool {
        self.force || !self.brave_running
    }

    fn disabled_reason(&self) -> String {
        let ch = self.selected_channel.map(|c| c.name()).unwrap_or("(none)");
        format!(
            "Disabled because Brave {} is running. Close it, tick 'Force', or click Recheck.",
            ch
        )
    }

    fn set_status(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.error = None;
    }

    fn set_error(&mut self, msg: impl Into<String>) {
        self.error = Some(msg.into());
    }

    fn do_apply_shields(&mut self) {
        if !self.can_mutate() {
            self.set_error("Brave is running; close it or tick 'Force' first");
            return;
        }
        let Some(prof) = self.current_profile().cloned() else {
            self.set_error("no profile selected");
            return;
        };

        let mut changes: Vec<(&'static ShieldKey, &'static ShieldValue)> = Vec::new();
        for k in shields::KEYS {
            let Some(sym) = self.shield_pending.get(k.name) else {
                continue;
            };
            if sym.is_empty() || sym == "(keep)" {
                continue;
            }
            match shields::lookup_symbol(k, sym) {
                Ok(sv) => changes.push((k, sv)),
                Err(e) => {
                    self.set_error(e.to_string());
                    return;
                }
            }
        }
        if changes.is_empty() {
            self.set_error("no pending changes");
            return;
        }

        let mut prefs_changes: Vec<(&'static ShieldKey, &'static ShieldValue)> = Vec::new();
        let mut ls_changes: Vec<(&'static ShieldKey, &'static ShieldValue)> = Vec::new();
        for (k, sv) in &changes {
            match k.file {
                shields::PrefFile::Preferences => prefs_changes.push((k, sv)),
                shields::PrefFile::LocalState => ls_changes.push((k, sv)),
            }
        }

        let mut report_lines = Vec::new();
        let mut any_mismatch = false;

        if !prefs_changes.is_empty() {
            let path = prof.prefs_path();
            match self.apply_file(&path, &prefs_changes) {
                Ok((backup, mismatches)) => {
                    if mismatches.is_empty() {
                        report_lines.push(format!(
                            "Preferences: {} change(s) verified; backup: {}",
                            prefs_changes.len(),
                            backup.display()
                        ));
                    } else {
                        any_mismatch = true;
                        report_lines.push(format!(
                            "Preferences: mismatches for [{}]; backup: {}",
                            mismatches.join(", "),
                            backup.display()
                        ));
                    }
                }
                Err(e) => {
                    self.set_error(format!("Preferences write: {}", e));
                    return;
                }
            }
        }

        if !ls_changes.is_empty() {
            let Some(ls_path) = self.paths.as_ref().map(|p| p.local_state_path()) else {
                self.set_error("no channel selected (cannot write Local State)");
                return;
            };
            match self.apply_file(&ls_path, &ls_changes) {
                Ok((backup, mismatches)) => {
                    if mismatches.is_empty() {
                        report_lines.push(format!(
                            "Local State: {} change(s) verified; backup: {}",
                            ls_changes.len(),
                            backup.display()
                        ));
                    } else {
                        any_mismatch = true;
                        report_lines.push(format!(
                            "Local State: mismatches for [{}]; backup: {}",
                            mismatches.join(", "),
                            backup.display()
                        ));
                    }
                }
                Err(e) => {
                    self.set_error(format!("Local State write: {}", e));
                    return;
                }
            }
        }

        let summary = report_lines.join(" | ");
        if any_mismatch {
            self.set_error(format!(
                "{} — Brave is likely still running and overwrote the file. \
                 Close every Brave {} window (including tray) and retry.",
                summary,
                self.selected_channel.map(|c| c.name()).unwrap_or("")
            ));
        } else {
            self.set_status(summary);
        }

        self.shield_pending.clear();
        self.reload_profile_views();
    }

    fn apply_file(
        &self,
        path: &std::path::Path,
        changes: &[(&'static ShieldKey, &'static ShieldValue)],
    ) -> Result<(PathBuf, Vec<String>)> {
        let mut root = prefs::read_prefs(path)?;
        for (k, sv) in changes {
            shields::apply_value(&mut root, sv)
                .map_err(|e| anyhow::anyhow!("set {}: {}", k.name, e))?;
        }
        let backup = prefs::write_prefs(path, &root)?;
        let disk = prefs::read_prefs(path)?;
        let mut mismatches = Vec::new();
        for (k, sv) in changes {
            if !shields::verify_value(&disk, sv) {
                mismatches.push(k.name.to_string());
            }
        }
        Ok((backup, mismatches))
    }

    fn do_backup(&mut self) {
        let Some(prof) = self.current_profile().cloned() else {
            self.set_error("no profile selected");
            return;
        };
        match prefs::backup_file(&prof.prefs_path()) {
            Ok(p) => {
                self.set_status(format!("backup: {}", p.display()));
                self.reload_profile_views();
            }
            Err(e) => self.set_error(format!("backup: {}", e)),
        }
    }

    fn do_diff_backup(&mut self, backup: PathBuf) {
        self.diff_output.clear();
        self.diff_from = None;
        let Some(prof) = self.current_profile().cloned() else {
            self.set_error("no profile selected");
            return;
        };
        let current_path = prof.prefs_path();
        let a = match prefs::read_prefs(&backup) {
            Ok(v) => v,
            Err(e) => {
                self.set_error(format!("read backup: {}", e));
                return;
            }
        };
        let b = match prefs::read_prefs(&current_path) {
            Ok(v) => v,
            Err(e) => {
                self.set_error(format!("read current: {}", e));
                return;
            }
        };
        let diffs = prefs::diff_values(&a, &b);
        if diffs.is_empty() {
            self.diff_output
                .push("(identical — no differences)".to_string());
        } else {
            self.diff_output
                .push(format!("{} difference(s):", diffs.len()));
            for d in diffs.iter().take(300) {
                let l = d
                    .left
                    .as_ref()
                    .map(|v| {
                        let s = v.to_string();
                        if s.len() > 120 {
                            format!("{}…", &s[..120])
                        } else {
                            s
                        }
                    })
                    .unwrap_or_else(|| "<missing>".to_string());
                let r = d
                    .right
                    .as_ref()
                    .map(|v| {
                        let s = v.to_string();
                        if s.len() > 120 {
                            format!("{}…", &s[..120])
                        } else {
                            s
                        }
                    })
                    .unwrap_or_else(|| "<missing>".to_string());
                self.diff_output.push(format!(
                    "{}\n    backup : {}\n    current: {}",
                    d.path, l, r
                ));
            }
            if diffs.len() > 300 {
                self.diff_output
                    .push(format!("… and {} more", diffs.len() - 300));
            }
        }
        self.diff_from = Some(backup);
        self.set_status(format!("diffed ({} differences)", diffs.len()));
    }

    fn do_restore(&mut self, path: PathBuf) -> Result<()> {
        if !self.can_mutate() {
            self.set_error("Brave is running; close it or tick 'Force' first");
            return Ok(());
        }
        match prefs::restore_backup(&path) {
            Ok(p) => {
                self.set_status(format!("restored: {}", p.display()));
                self.reload_profile_views();
            }
            Err(e) => self.set_error(format!("restore: {}", e)),
        }
        Ok(())
    }

    fn do_load_preferences(&mut self) {
        let Some(prof) = self.current_profile().cloned() else {
            self.set_error("no profile selected");
            return;
        };
        self.load_file_into_editor(prof.prefs_path());
    }

    /// Populate the Preferences-tab buffer from `path` and switch to that tab.
    /// Subsequent Save writes *back* to `path` (same as if the user had
    /// clicked Load Preferences on the current profile) — editing a backup
    /// updates the backup file, not the active Preferences. Use the Backup
    /// tab's Restore button afterwards to copy the (now-edited) backup over
    /// the active Preferences.
    fn load_file_into_editor(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                self.prefs_buffer = text;
                self.prefs_loaded_from = Some(path.clone());
                self.prefs_parse_error = None;
                self.prefs_dirty = false;
                self.tab = Tab::Preferences;
                self.set_status(format!(
                    "loaded {} ({} bytes)",
                    path.display(),
                    self.prefs_buffer.len()
                ));
            }
            Err(e) => self.set_error(format!("read {}: {}", path.display(), e)),
        }
    }

    fn do_prettify_preferences(&mut self) {
        if self.prefs_buffer.is_empty() {
            self.set_error("nothing loaded");
            return;
        }
        match serde_json::from_str::<Value>(&self.prefs_buffer) {
            Ok(v) => match serde_json::to_string_pretty(&v) {
                Ok(pretty) => {
                    self.prefs_buffer = pretty;
                    self.prefs_parse_error = None;
                    self.prefs_dirty = true;
                    self.set_status("reformatted (click Save to persist)");
                }
                Err(e) => self.set_error(format!("serialize: {}", e)),
            },
            Err(e) => {
                self.prefs_parse_error = Some(e.to_string());
                self.set_error(format!("invalid JSON: {}", e));
            }
        }
    }

    fn do_minify_preferences(&mut self) {
        if self.prefs_buffer.is_empty() {
            self.set_error("nothing loaded");
            return;
        }
        match serde_json::from_str::<Value>(&self.prefs_buffer) {
            Ok(v) => match serde_json::to_string(&v) {
                Ok(min) => {
                    self.prefs_buffer = min;
                    self.prefs_parse_error = None;
                    self.prefs_dirty = true;
                    self.set_status("minified (click Save to persist)");
                }
                Err(e) => self.set_error(format!("serialize: {}", e)),
            },
            Err(e) => {
                self.prefs_parse_error = Some(e.to_string());
                self.set_error(format!("invalid JSON: {}", e));
            }
        }
    }

    fn do_save_preferences(&mut self) {
        if !self.can_mutate() {
            self.set_error("Brave is running; close it or tick 'Force' first");
            return;
        }
        let Some(path) = self.prefs_loaded_from.clone() else {
            self.set_error("nothing loaded");
            return;
        };
        let parsed: Value = match serde_json::from_str(&self.prefs_buffer) {
            Ok(v) => v,
            Err(e) => {
                self.prefs_parse_error = Some(e.to_string());
                self.set_error(format!("invalid JSON — not saving: {}", e));
                return;
            }
        };
        if !parsed.is_object() {
            self.set_error("top-level value must be a JSON object — not saving");
            return;
        }
        self.prefs_parse_error = None;
        match prefs::write_prefs(&path, &parsed) {
            Ok(bkp) => {
                self.prefs_dirty = false;
                self.set_status(format!(
                    "saved {} (backup: {})",
                    path.display(),
                    bkp.display()
                ));
                self.reload_profile_views();
            }
            Err(e) => self.set_error(format!("write: {}", e)),
        }
    }

    fn do_estimate_data_size(&mut self) {
        let Some(paths) = self.paths.clone() else {
            self.set_error("no channel selected");
            return;
        };
        let profs: Vec<Profile> = self
            .profiles
            .iter()
            .filter(|p| *self.data_profile_checked.get(&p.dir_name).unwrap_or(&false))
            .cloned()
            .collect();
        let cats: Vec<&'static data::DataCategory> = data::CATEGORIES
            .iter()
            .filter(|c| *self.data_category_checked.get(c.id).unwrap_or(&false))
            .collect();
        if profs.is_empty() {
            self.set_error("tick at least one profile");
            return;
        }
        if cats.is_empty() {
            self.set_error("tick at least one data category");
            return;
        }
        let mut total = 0u64;
        self.last_data_report.clear();
        for p in &profs {
            let mut sub = 0u64;
            for c in &cats {
                let s = data::size_of(&paths, p, c);
                self.last_data_report.push(format!(
                    "[{}] {}: {}",
                    p.dir_name,
                    c.label,
                    data::format_bytes(s)
                ));
                sub += s;
            }
            self.last_data_report.push(format!(
                "[{}] subtotal: {}",
                p.dir_name,
                data::format_bytes(sub)
            ));
            total += sub;
        }
        self.last_data_report
            .push(format!("TOTAL would free: {}", data::format_bytes(total)));
        self.set_status(format!(
            "estimate: {} across {} profile(s)",
            data::format_bytes(total),
            profs.len()
        ));
    }

    fn prepare_delete_confirmation(&mut self) {
        let Some(paths) = self.paths.clone() else {
            self.set_error("no channel selected");
            return;
        };
        let profs: Vec<String> = self
            .profiles
            .iter()
            .filter(|p| *self.data_profile_checked.get(&p.dir_name).unwrap_or(&false))
            .map(|p| p.dir_name.clone())
            .collect();
        let cats: Vec<&'static str> = data::CATEGORIES
            .iter()
            .filter(|c| *self.data_category_checked.get(c.id).unwrap_or(&false))
            .map(|c| c.id)
            .collect();
        if profs.is_empty() {
            self.set_error("tick at least one profile");
            return;
        }
        if cats.is_empty() {
            self.set_error("tick at least one data category");
            return;
        }
        let mut total = 0u64;
        for p in &self.profiles {
            if !profs.contains(&p.dir_name) {
                continue;
            }
            for c in data::CATEGORIES {
                if cats.contains(&c.id) {
                    total += data::size_of(&paths, p, c);
                }
            }
        }
        self.data_dialog_size_bytes = total;
        self.data_dialog_profiles = profs;
        self.data_dialog_categories = cats;
        self.data_dialog_open = true;
    }

    fn do_delete_data(&mut self) {
        if !self.can_mutate() {
            self.set_error("Brave is running; close it or tick 'Force' first");
            return;
        }
        let Some(paths) = self.paths.clone() else {
            self.set_error("no channel selected");
            return;
        };
        let profs: Vec<Profile> = self
            .profiles
            .iter()
            .filter(|p| self.data_dialog_profiles.contains(&p.dir_name))
            .cloned()
            .collect();
        let cats: Vec<&'static data::DataCategory> = data::CATEGORIES
            .iter()
            .filter(|c| self.data_dialog_categories.contains(&c.id))
            .collect();

        self.last_data_report.clear();
        let mut total = 0u64;
        let mut had_errors = false;

        for p in &profs {
            let mut sub = 0u64;
            for c in &cats {
                match data::clear(&paths, p, c, false) {
                    Ok(report) => {
                        sub += report.bytes_freed;
                        self.last_data_report.push(format!(
                            "[{}] {}: freed {} ({} paths)",
                            p.dir_name,
                            c.label,
                            data::format_bytes(report.bytes_freed),
                            report.touched_paths.len()
                        ));
                        for (pth, err) in report.skipped {
                            had_errors = true;
                            self.last_data_report.push(format!(
                                "    skipped {} — {}",
                                pth.display(),
                                err
                            ));
                        }
                    }
                    Err(e) => {
                        had_errors = true;
                        self.last_data_report
                            .push(format!("[{}] {}: ERROR {}", p.dir_name, c.label, e));
                    }
                }
            }
            self.last_data_report.push(format!(
                "[{}] subtotal: {}",
                p.dir_name,
                data::format_bytes(sub)
            ));
            total += sub;
        }
        self.last_data_report
            .push(format!("TOTAL freed: {}", data::format_bytes(total)));

        if had_errors {
            self.set_error(format!(
                "freed {} with some errors — see details below",
                data::format_bytes(total)
            ));
        } else {
            self.set_status(format!(
                "freed {} across {} profile(s)",
                data::format_bytes(total),
                profs.len()
            ));
        }
    }

    fn do_add_flag(&mut self) {
        let raw = self.flags_add_buffer.trim();
        if raw.is_empty() {
            return;
        }
        let normalized = if raw.contains('@') {
            raw.to_string()
        } else {
            format!("{}@1", raw)
        };
        if let Some(name) = normalized.split('@').next() {
            let prefix = format!("{}@", name);
            self.flags_working.retain(|e| !e.starts_with(&prefix));
        }
        self.flags_working.push(normalized);
        self.flags_add_buffer.clear();
    }

    fn do_apply_flags(&mut self) {
        if !self.can_mutate() {
            self.set_error("Brave is running; close it or tick 'Force' first");
            return;
        }
        let Some(ls_path) = self.paths.as_ref().map(|p| p.local_state_path()) else {
            self.set_error("no channel selected");
            return;
        };
        let mut root = match prefs::read_prefs(&ls_path) {
            Ok(r) => r,
            Err(e) => {
                self.set_error(format!("read Local State: {}", e));
                return;
            }
        };
        let arr: Vec<serde_json::Value> = self
            .flags_working
            .iter()
            .map(|s| serde_json::Value::String(s.clone()))
            .collect();
        if let Err(e) = prefs::set_path(
            &mut root,
            "browser.enabled_labs_experiments",
            serde_json::Value::Array(arr),
        ) {
            self.set_error(format!("set flags: {}", e));
            return;
        }
        let backup = match prefs::write_prefs(&ls_path, &root) {
            Ok(b) => b,
            Err(e) => {
                self.set_error(format!("write Local State: {}", e));
                return;
            }
        };
        let verified = prefs::read_prefs(&ls_path)
            .ok()
            .and_then(|r| {
                prefs::get_path(&r, "browser.enabled_labs_experiments")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .collect::<Vec<_>>()
                    })
            })
            .map(|disk| disk == self.flags_working)
            .unwrap_or(false);
        if verified {
            self.flags_original = self.flags_working.clone();
            self.set_status(format!(
                "{} flag(s) written; backup: {}",
                self.flags_working.len(),
                backup.display()
            ));
        } else {
            self.set_error(format!(
                "write succeeded but values did NOT persist. Close every Brave {} window \
                 (including tray) and retry. Backup at {}",
                self.selected_channel.map(|c| c.name()).unwrap_or(""),
                backup.display()
            ));
        }
    }

    fn do_apply_filters(&mut self) {
        if !self.can_mutate() {
            self.set_error("Brave is running; close it or tick 'Force' first");
            return;
        }
        let Some(ls_path) = self.paths.as_ref().map(|p| p.local_state_path()) else {
            self.set_error("no channel selected");
            return;
        };
        let mut root = match prefs::read_prefs(&ls_path) {
            Ok(r) => r,
            Err(e) => {
                self.set_error(format!("read Local State: {}", e));
                return;
            }
        };
        let mut regional_applied = 0;
        for (uuid, v) in &self.filter_regional_pending {
            if let Err(e) = filters::write_regional_enabled(&mut root, uuid, *v) {
                self.set_error(format!("write regional {}: {}", uuid, e));
                return;
            }
            regional_applied += 1;
        }
        let mut sub_applied = 0;
        for url in &self.filter_subscription_removes {
            if let Err(e) = filters::remove_subscription(&mut root, url) {
                self.set_error(format!("remove sub {}: {}", url, e));
                return;
            }
            sub_applied += 1;
        }
        for (url, v) in &self.filter_subscription_toggles {
            if self.filter_subscription_removes.contains(url) {
                continue;
            }
            if let Err(e) = filters::write_subscription_enabled(&mut root, url, *v) {
                self.set_error(format!("write sub {}: {}", url, e));
                return;
            }
            sub_applied += 1;
        }
        let custom_changed = self.filter_custom_working != self.filter_custom_original;
        if custom_changed
            && let Err(e) = filters::write_custom_filters(&mut root, &self.filter_custom_working)
        {
            self.set_error(format!("write custom_filters: {}", e));
            return;
        }
        let backup = match prefs::write_prefs(&ls_path, &root) {
            Ok(b) => b,
            Err(e) => {
                self.set_error(format!("write Local State: {}", e));
                return;
            }
        };
        let disk = match prefs::read_prefs(&ls_path) {
            Ok(r) => r,
            Err(e) => {
                self.set_error(format!("verify read: {}", e));
                return;
            }
        };
        let mut verified = true;
        for (uuid, want) in &self.filter_regional_pending {
            if filters::read_regional_enabled(&disk, uuid) != Some(*want) {
                verified = false;
                break;
            }
        }
        if verified
            && custom_changed
            && filters::read_custom_filters(&disk) != self.filter_custom_working
        {
            verified = false;
        }
        if verified {
            for (uuid, v) in self.filter_regional_pending.drain() {
                self.filter_regional_current.insert(uuid, v);
            }
            self.filter_subscription_toggles.clear();
            self.filter_subscription_removes.clear();
            self.filter_custom_original = self.filter_custom_working.clone();
            self.filter_subscriptions_current = filters::read_subscriptions(&disk);
            self.set_status(format!(
                "filters applied ({} regional, {} sub ops, custom {}); backup: {}",
                regional_applied,
                sub_applied,
                if custom_changed {
                    "updated"
                } else {
                    "unchanged"
                },
                backup.display()
            ));
        } else {
            self.set_error(format!(
                "write succeeded but values did NOT persist. Close every Brave {} \
                 window (including tray) and retry. Backup at {}",
                self.selected_channel.map(|c| c.name()).unwrap_or(""),
                backup.display()
            ));
        }
    }

    fn do_apply_getting_started(&mut self) {
        if !self.can_mutate() {
            self.set_error("Brave is running; close it or tick 'Force' first");
            return;
        }
        let Some(prof) = self.current_profile().cloned() else {
            self.set_error("no profile selected");
            return;
        };
        let path = prof.prefs_path();
        let mut root = match prefs::read_prefs(&path) {
            Ok(r) => r,
            Err(e) => {
                self.set_error(format!("read Preferences: {}", e));
                return;
            }
        };
        let name_dirty = self.gs_profile_name_working != self.gs_profile_name_original;
        let startup_dirty = self.gs_startup_working != self.gs_startup_original;
        if name_dirty
            && let Err(e) = prefs::set_path(
                &mut root,
                "profile.name",
                serde_json::Value::String(self.gs_profile_name_working.clone()),
            )
        {
            self.set_error(format!("set profile.name: {}", e));
            return;
        }
        if startup_dirty
            && let Some(v) = self.gs_startup_working
            && let Err(e) = prefs::set_path(
                &mut root,
                "session.restore_on_startup",
                serde_json::Value::from(v),
            )
        {
            self.set_error(format!("set startup: {}", e));
            return;
        }
        let backup = match prefs::write_prefs(&path, &root) {
            Ok(b) => b,
            Err(e) => {
                self.set_error(format!("write: {}", e));
                return;
            }
        };
        let disk = prefs::read_prefs(&path).ok();
        let name_ok = !name_dirty
            || disk
                .as_ref()
                .and_then(|r| prefs::get_path(r, "profile.name").and_then(|v| v.as_str()))
                .map(|s| s == self.gs_profile_name_working.as_str())
                .unwrap_or(false);
        let startup_ok = !startup_dirty
            || disk.as_ref().and_then(|r| {
                prefs::get_path(r, "session.restore_on_startup").and_then(|v| v.as_i64())
            }) == self.gs_startup_working;
        if name_ok && startup_ok {
            self.gs_profile_name_original = self.gs_profile_name_working.clone();
            self.gs_startup_original = self.gs_startup_working;
            let mut parts: Vec<&str> = Vec::new();
            if name_dirty {
                parts.push("profile name");
            }
            if startup_dirty {
                parts.push("startup mode");
            }
            self.set_status(format!(
                "applied: {}; backup: {}",
                parts.join(", "),
                backup.display()
            ));
        } else {
            self.set_error(format!(
                "write succeeded but values did NOT persist. Close every Brave {} \
                 window (including tray) and retry. Backup at {}",
                self.selected_channel.map(|c| c.name()).unwrap_or(""),
                backup.display()
            ));
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Low-frequency poll for "is Brave running" so the banner flips when
        // the user closes Brave. 3s keeps us out of the tight repaint loop
        // without making the state feel laggy; the user can also click the
        // Recheck button for an instant update.
        ctx.request_repaint_after(std::time::Duration::from_secs(3));
        let now = std::time::Instant::now();
        let stale = self
            .last_process_poll
            .map(|t| now.duration_since(t).as_secs() >= 3)
            .unwrap_or(true);
        if stale {
            self.refresh_brave_running();
            self.last_process_poll = Some(now);
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Brave Offline Config Editor");
                ui.separator();
                if ui.button("Refresh").clicked() {
                    self.refresh_all();
                }
                ui.separator();

                let current_label = match self.selected_channel {
                    Some(ch) => match self.installed_version(ch) {
                        Some(v) => format!("{} v{}", ch.name(), v),
                        None => ch.name().to_string(),
                    },
                    None => "(none)".to_string(),
                };
                let prev = self.selected_channel;
                egui::ComboBox::from_label("Channel")
                    .selected_text(current_label)
                    .show_ui(ui, |ui| {
                        for ins in &self.installs {
                            let label = match &ins.version {
                                Some(v) => format!("{} v{}", ins.channel.name(), v),
                                None => ins.channel.name().to_string(),
                            };
                            ui.selectable_value(
                                &mut self.selected_channel,
                                Some(ins.channel),
                                label,
                            );
                        }
                    });
                if self.selected_channel != prev {
                    self.reload_channel();
                    self.refresh_brave_running();
                }

                ui.separator();
                let prev_prof = self.selected_profile.clone();
                let prof_label = self
                    .selected_profile
                    .clone()
                    .unwrap_or_else(|| "(none)".into());
                egui::ComboBox::from_label("Profile")
                    .selected_text(prof_label)
                    .show_ui(ui, |ui| {
                        for prof in &self.profiles {
                            let label = match &prof.display_name {
                                Some(name) => format!("{} — {}", prof.dir_name, name),
                                None => prof.dir_name.clone(),
                            };
                            ui.selectable_value(
                                &mut self.selected_profile,
                                Some(prof.dir_name.clone()),
                                label,
                            );
                        }
                    });
                if self.selected_profile != prev_prof {
                    self.reload_profile_views();
                    if let Some(name) = &self.selected_profile {
                        self.data_profile_checked.insert(name.clone(), true);
                    }
                }
            });

            ui.horizontal(|ui| {
                let ch_label = self.selected_channel.map(|c| c.name()).unwrap_or("(none)");
                if self.brave_running {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 140, 30),
                        format!(
                            "⚠ Brave {} is running — close it before applying changes",
                            ch_label
                        ),
                    );
                    ui.checkbox(&mut self.force, "Force");
                    if ui.button("Recheck").clicked() {
                        self.refresh_brave_running();
                    }
                } else {
                    ui.colored_label(
                        egui::Color32::from_rgb(60, 160, 80),
                        format!("Brave {} is not running", ch_label),
                    );
                    if ui.button("Recheck").clicked() {
                        self.refresh_brave_running();
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::GettingStarted, "🚀 Get started");
                ui.selectable_value(&mut self.tab, Tab::Shields, "🛡 Shields");
                ui.selectable_value(&mut self.tab, Tab::Filters, "🧹 Filters");
                ui.selectable_value(&mut self.tab, Tab::Privacy, "🔒 Privacy");
                ui.selectable_value(&mut self.tab, Tab::DeleteData, "🗑 Delete Data");
                ui.selectable_value(&mut self.tab, Tab::System, "⚙ System");
                ui.separator();
                ui.selectable_value(&mut self.tab, Tab::Flags, "🚩 Flags");
                ui.selectable_value(&mut self.tab, Tab::Preferences, "📝 Preferences");
                ui.selectable_value(&mut self.tab, Tab::Backups, "📁 Backups");
            });
        });

        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            if let Some(err) = &self.error {
                ui.colored_label(
                    egui::Color32::from_rgb(200, 80, 80),
                    format!("Error: {}", err),
                );
            } else if !self.status.is_empty() {
                ui.label(&self.status);
            } else {
                ui.label(" ");
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::GettingStarted => self.ui_getting_started(ui),
            Tab::Shields => self.ui_shield_pane(ui, shields::Pane::Shields),
            Tab::Filters => self.ui_filters(ui),
            Tab::Privacy => self.ui_shield_pane(ui, shields::Pane::Privacy),
            Tab::DeleteData => self.ui_delete_data(ui),
            Tab::System => self.ui_shield_pane(ui, shields::Pane::System),
            Tab::Flags => self.ui_flags(ui),
            Tab::Preferences => self.ui_preferences(ui),
            Tab::Backups => self.ui_backups(ui),
        });

        self.ui_delete_confirm(ctx);
    }
}

impl App {
    fn ui_getting_started(&mut self, ui: &mut egui::Ui) {
        if self.current_profile().is_none() {
            ui.label("(no profile selected)");
            return;
        }
        ui.heading("Get started");
        ui.label(
            egui::RichText::new(
                "Quick-access alternative to brave://settings/getStarted. Avoids the \
                 settings page entirely — useful if that page is unresponsive.",
            )
            .small()
            .weak(),
        );
        ui.separator();

        ui.label(egui::RichText::new("Profile name").strong());
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.gs_profile_name_working);
            if self.gs_profile_name_working != self.gs_profile_name_original {
                ui.colored_label(egui::Color32::from_rgb(220, 140, 30), "modified");
            }
        });
        ui.label(
            egui::RichText::new("Display name shown in the profile picker and window title.")
                .small()
                .weak(),
        );

        ui.add_space(10.0);
        ui.separator();
        ui.label(egui::RichText::new("On startup").strong());
        let current_startup = self.gs_startup_working;
        let mut sel = current_startup.unwrap_or(5);
        ui.radio_value(&mut sel, 5, "Open the New Tab page");
        ui.radio_value(&mut sel, 1, "Continue where you left off");
        ui.radio_value(
            &mut sel,
            4,
            "Open a specific page or set of pages (edit URLs in Brave)",
        );
        if Some(sel) != current_startup {
            self.gs_startup_working = Some(sel);
        }
        ui.label(
            egui::RichText::new(
                "Writes session.restore_on_startup (5 = NTP, 1 = resume, 4 = URLs).",
            )
            .small()
            .weak(),
        );

        ui.add_space(10.0);
        ui.separator();
        ui.label(egui::RichText::new("Channel info").strong());
        if let Some(ins) = self.current_install() {
            let ver = ins.version.as_deref().unwrap_or("unknown");
            ui.monospace(format!("Brave {} v{}", ins.channel.name(), ver));
            ui.monospace(format!("exe: {}", ins.exe.display()));
        }
        if let Some(p) = self.current_profile() {
            ui.monospace(format!("profile dir: {}", p.path.display()));
        }

        ui.add_space(10.0);
        ui.separator();
        let name_dirty = self.gs_profile_name_working != self.gs_profile_name_original;
        let startup_dirty = self.gs_startup_working != self.gs_startup_original;
        let dirty = name_dirty || startup_dirty;
        ui.horizontal(|ui| {
            let apply = egui::Button::new("Apply changes");
            let resp = ui.add_enabled(self.can_mutate() && dirty, apply);
            if !self.can_mutate() {
                resp.clone().on_disabled_hover_text(self.disabled_reason());
            }
            if resp.clicked() {
                self.do_apply_getting_started();
            }
            if ui
                .add_enabled(dirty, egui::Button::new("Discard edits"))
                .clicked()
            {
                self.gs_profile_name_working = self.gs_profile_name_original.clone();
                self.gs_startup_working = self.gs_startup_original;
            }
            if dirty {
                ui.colored_label(egui::Color32::from_rgb(220, 140, 30), "unsaved changes");
            }
        });
    }

    fn ui_shield_pane(&mut self, ui: &mut egui::Ui, pane: shields::Pane) {
        if self.current_profile().is_none() {
            ui.label("(no profile selected)");
            return;
        }
        ui.label("Current values are read from Preferences. Pick a new value and click Apply.");
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut current_section = "";
            for k in shields::KEYS.iter().filter(|k| k.pane == pane) {
                if k.section != current_section {
                    if !current_section.is_empty() {
                        ui.add_space(6.0);
                    }
                    ui.heading(k.section);
                    ui.separator();
                    current_section = k.section;
                }
                egui::Grid::new(format!("grid_{}", k.name))
                    .num_columns(3)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(k.name).on_hover_text(k.desc);
                        let current_sym = self.shield_current.get(k.name).cloned().unwrap_or(None);
                        let current_str = current_sym.unwrap_or("<default>").to_string();
                        ui.label(current_str);
                        let pending = self
                            .shield_pending
                            .entry(k.name.to_string())
                            .or_insert_with(|| "(keep)".to_string());
                        egui::ComboBox::from_id_salt(format!("sh_{}", k.name))
                            .selected_text(pending.clone())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(pending, "(keep)".to_string(), "(keep)");
                                for v in k.values {
                                    ui.selectable_value(pending, v.symbol.to_string(), v.symbol);
                                }
                            });
                        ui.end_row();
                    });
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            let apply = egui::Button::new("Apply changes");
            let resp = ui.add_enabled(self.can_mutate(), apply);
            if !self.can_mutate() {
                resp.clone().on_disabled_hover_text(self.disabled_reason());
            }
            if resp.clicked() {
                self.do_apply_shields();
            }
            if ui.button("Reset selections").clicked() {
                self.shield_pending.clear();
            }
            if ui.button("Reread").clicked() {
                self.reload_profile_views();
            }
        });
        ui.label("Each Apply writes a timestamped .bak-<stamp> next to Preferences first.");
    }

    fn ui_filters(&mut self, ui: &mut egui::Ui) {
        if self.paths.is_none() {
            ui.label("(no channel selected)");
            return;
        }
        ui.label(
            "Brave adblock filter lists (stored in Local State — shared across all profiles of this channel).",
        );
        ui.label(
            egui::RichText::new(
                "Restart Brave for changes to take effect. Missing catalog file \
                 just means the \"gkboaolp…\" component hasn't downloaded yet \
                 — flip any filter in brave://settings/shields/filters once, close \
                 Brave, then reopen this tool.",
            )
            .small()
            .weak(),
        );

        let regional_dirty = self.filter_regional_pending.iter().any(|(uuid, v)| {
            let default = self
                .filter_catalog
                .iter()
                .find(|c| &c.uuid == uuid)
                .map(|c| c.default_enabled)
                .unwrap_or(false);
            let current = *self.filter_regional_current.get(uuid).unwrap_or(&default);
            *v != current
        });
        let subs_dirty = !self.filter_subscription_toggles.is_empty()
            || !self.filter_subscription_removes.is_empty();
        let custom_dirty = self.filter_custom_working != self.filter_custom_original;
        let any_dirty = regional_dirty || subs_dirty || custom_dirty;

        ui.separator();
        ui.horizontal(|ui| {
            let apply = egui::Button::new("Apply changes");
            let resp = ui.add_enabled(self.can_mutate() && any_dirty, apply);
            if !self.can_mutate() {
                resp.clone().on_disabled_hover_text(self.disabled_reason());
            }
            if resp.clicked() {
                self.do_apply_filters();
            }
            if ui
                .add_enabled(any_dirty, egui::Button::new("Discard edits"))
                .clicked()
            {
                self.filter_regional_pending.clear();
                self.filter_subscription_toggles.clear();
                self.filter_subscription_removes.clear();
                self.filter_custom_working = self.filter_custom_original.clone();
            }
            if ui.button("Reload from disk").clicked() {
                self.reload_profile_views();
            }
            if any_dirty {
                ui.colored_label(egui::Color32::from_rgb(220, 140, 30), "unsaved changes");
            }
        });

        egui::ScrollArea::vertical()
            .id_salt("filters_scroll")
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.heading("Filter lists (catalog)");
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.text_edit_singleline(&mut self.filter_search);
                });

                if self.filter_catalog.is_empty() {
                    if self.filter_regional_current.is_empty() {
                        ui.weak("(catalog unavailable and no regional overrides stored)");
                    } else {
                        ui.label(
                            egui::RichText::new("Catalog file missing; showing raw UUIDs only.")
                                .small()
                                .weak(),
                        );
                        let uuids: Vec<String> =
                            self.filter_regional_current.keys().cloned().collect();
                        for uuid in uuids {
                            self.ui_filter_row(ui, &uuid, &uuid, false);
                        }
                    }
                } else {
                    // Catalog is pre-sorted at load time. We only clone the
                    // (uuid, label, default) triples we'll actually render —
                    // cheaper than cloning every CatalogEntry per frame just
                    // to satisfy the &self / &mut self borrow split.
                    let needle = self.filter_search.to_lowercase();
                    let rows: Vec<(String, String, bool)> = self
                        .filter_catalog
                        .iter()
                        .filter(|e| !e.hidden)
                        .filter(|e| {
                            if needle.is_empty() {
                                true
                            } else {
                                let hay = format!(
                                    "{} {} {}",
                                    e.title.to_lowercase(),
                                    e.uuid.to_lowercase(),
                                    e.langs.join(",").to_lowercase()
                                );
                                hay.contains(&needle)
                            }
                        })
                        .map(|e| {
                            let label = if e.langs.is_empty() {
                                e.title.clone()
                            } else {
                                format!("{} [{}]", e.title, e.langs.join(","))
                            };
                            (e.uuid.clone(), label, e.default_enabled)
                        })
                        .collect();
                    for (uuid, label, default_enabled) in &rows {
                        self.ui_filter_row(ui, uuid, label, *default_enabled);
                    }
                }

                ui.add_space(10.0);
                ui.separator();
                ui.heading("Custom filter subscriptions");
                if self.filter_subscriptions_current.is_empty() {
                    ui.weak("(no custom subscription URLs)");
                }
                let subs = self.filter_subscriptions_current.clone();
                for sub in &subs {
                    let removed = self.filter_subscription_removes.contains(&sub.url);
                    ui.horizontal(|ui| {
                        if removed {
                            ui.label(
                                egui::RichText::new("× pending removal")
                                    .color(egui::Color32::from_rgb(200, 80, 80)),
                            );
                            if ui.small_button("undo").clicked() {
                                self.filter_subscription_removes.retain(|u| u != &sub.url);
                            }
                        } else {
                            let mut enabled = *self
                                .filter_subscription_toggles
                                .get(&sub.url)
                                .unwrap_or(&sub.enabled);
                            if ui.checkbox(&mut enabled, "").changed() {
                                self.filter_subscription_toggles
                                    .insert(sub.url.clone(), enabled);
                            }
                            if ui.small_button("×").on_hover_text("Remove").clicked() {
                                self.filter_subscription_removes.push(sub.url.clone());
                            }
                        }
                        ui.vertical(|ui| {
                            if let Some(title) = &sub.title
                                && !title.is_empty()
                            {
                                ui.label(title);
                            }
                            ui.monospace(&sub.url);
                        });
                    });
                }

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Add subscription URL:");
                    ui.text_edit_singleline(&mut self.filter_sub_add_buffer);
                    if ui.button("Add").clicked() {
                        let u = self.filter_sub_add_buffer.trim().to_string();
                        if !u.is_empty()
                            && !self.filter_subscriptions_current.iter().any(|s| s.url == u)
                        {
                            self.filter_subscriptions_current
                                .push(filters::Subscription {
                                    url: u.clone(),
                                    enabled: true,
                                    title: None,
                                    last_update: None,
                                });
                            self.filter_subscription_toggles.insert(u, true);
                        }
                        self.filter_sub_add_buffer.clear();
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.heading("Custom filter rules");
                ui.label(
                    egui::RichText::new(
                        "One rule per line (same syntax as the box on brave://adblock).",
                    )
                    .small()
                    .weak(),
                );
                ui.add(
                    egui::TextEdit::multiline(&mut self.filter_custom_working)
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY),
                );
            });
    }

    fn ui_filter_row(&mut self, ui: &mut egui::Ui, uuid: &str, label: &str, default_enabled: bool) {
        let stored = self.filter_regional_current.get(uuid).copied();
        let effective = stored.unwrap_or(default_enabled);
        let mut pending = *self.filter_regional_pending.get(uuid).unwrap_or(&effective);
        ui.horizontal(|ui| {
            if ui.checkbox(&mut pending, "").changed() {
                self.filter_regional_pending
                    .insert(uuid.to_string(), pending);
            }
            let pending_marker = if pending != effective { " *" } else { "" };
            let default_marker = match stored {
                Some(_) => "",
                None => " (catalog default)",
            };
            ui.label(format!("{}{}{}", label, default_marker, pending_marker))
                .on_hover_text(uuid);
        });
    }

    fn ui_delete_data(&mut self, ui: &mut egui::Ui) {
        if self.profiles.is_empty() {
            ui.label("(no profiles found for this channel)");
            return;
        }
        ui.horizontal(|ui| {
            ui.label("Profiles:");
            if ui.small_button("Select all").clicked() {
                for p in &self.profiles {
                    self.data_profile_checked.insert(p.dir_name.clone(), true);
                }
            }
            if ui.small_button("Clear all").clicked() {
                for p in &self.profiles {
                    self.data_profile_checked.insert(p.dir_name.clone(), false);
                }
            }
            let ticked = self
                .profiles
                .iter()
                .filter(|p| *self.data_profile_checked.get(&p.dir_name).unwrap_or(&false))
                .count();
            ui.weak(format!("({} of {} ticked)", ticked, self.profiles.len()));
        });
        egui::ScrollArea::vertical()
            .id_salt("data_profiles_scroll")
            .max_height(140.0)
            .show(ui, |ui| {
                for prof in &self.profiles {
                    let checked = self
                        .data_profile_checked
                        .entry(prof.dir_name.clone())
                        .or_insert(false);
                    let label = match &prof.display_name {
                        Some(name) => format!("{} — {}", prof.dir_name, name),
                        None => prof.dir_name.clone(),
                    };
                    ui.checkbox(checked, label);
                }
            });

        ui.separator();
        ui.label("Data to delete (hover a category to see its description; expand for paths):");
        for c in data::CATEGORIES {
            let checked = self
                .data_category_checked
                .entry(c.id.to_string())
                .or_insert(c.default_checked);
            ui.horizontal(|ui| {
                ui.checkbox(checked, c.label).on_hover_text(c.desc);
                ui.weak(format!("({} path(s))", c.targets.len()));
            });
            egui::CollapsingHeader::new(egui::RichText::new("show paths").small().weak())
                .id_salt(format!("cat_paths_{}", c.id))
                .default_open(false)
                .show(ui, |ui| {
                    for t in c.targets {
                        let rendered = if t.ends_with('/') {
                            format!("{}  (contents cleared; directory kept)", t)
                        } else {
                            t.to_string()
                        };
                        ui.monospace(rendered);
                    }
                });
        }

        ui.separator();
        ui.colored_label(
            egui::Color32::from_rgb(220, 140, 30),
            "This is destructive. Bookmarks, extensions, and settings are preserved. \
             Everything else in the checked categories is permanently removed.",
        );
        ui.horizontal(|ui| {
            if ui.button("Estimate size").clicked() {
                self.do_estimate_data_size();
            }
            let btn = egui::Button::new("Delete Data…");
            let resp = ui.add_enabled(self.can_mutate(), btn);
            if !self.can_mutate() {
                resp.clone().on_disabled_hover_text(self.disabled_reason());
            }
            if resp.clicked() {
                self.prepare_delete_confirmation();
            }
        });
        if !self.last_data_report.is_empty() {
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("data_report_scroll")
                .show(ui, |ui| {
                    for line in &self.last_data_report {
                        ui.monospace(line);
                    }
                });
        }
    }

    fn ui_delete_confirm(&mut self, ctx: &egui::Context) {
        if !self.data_dialog_open {
            return;
        }
        let size_str = data::format_bytes(self.data_dialog_size_bytes);
        let profiles = self.data_dialog_profiles.join(", ");
        let categories: Vec<&str> = self
            .data_dialog_categories
            .iter()
            .map(|id| data::lookup(id).map(|c| c.label).unwrap_or("?"))
            .collect();
        let dialog_categories: Vec<&'static data::DataCategory> = self
            .data_dialog_categories
            .iter()
            .filter_map(|id| data::lookup(id))
            .collect();
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new("Delete browser data — are you sure?")
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("This will delete {} of data.", size_str));
                ui.separator();
                ui.label(format!("Profiles  : {}", profiles));
                ui.label(format!("Categories: {}", categories.join(", ")));
                ui.separator();
                ui.label(egui::RichText::new("Exactly what gets removed (per profile):").strong());
                egui::ScrollArea::vertical()
                    .id_salt("confirm_paths_scroll")
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for c in &dialog_categories {
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(c.label).strong());
                            for t in c.targets {
                                let line = if t.ends_with('/') {
                                    format!("  {}  (contents cleared; directory kept)", t)
                                } else {
                                    format!("  {}", t)
                                };
                                ui.monospace(line);
                            }
                        }
                    });
                ui.separator();
                ui.colored_label(
                    egui::Color32::from_rgb(220, 140, 30),
                    "Bookmarks, Preferences, Secure Preferences, and Extensions are \
                     preserved. Nothing outside the paths above is touched. \
                     This cannot be undone.",
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    let delete_btn = egui::Button::new(
                        egui::RichText::new("Delete").color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(160, 40, 40));
                    if ui.add(delete_btn).clicked() {
                        confirm = true;
                    }
                });
            });
        if cancel {
            self.data_dialog_open = false;
        }
        if confirm {
            self.data_dialog_open = false;
            self.do_delete_data();
        }
    }

    fn ui_flags(&mut self, ui: &mut egui::Ui) {
        if self.paths.is_none() {
            ui.label("(no channel selected)");
            return;
        }
        ui.label(
            "Custom brave://flags (stored in Local State — shared across all profiles of this channel).",
        );
        ui.label(
            "Each entry is \"<flag-name>@<index>\". Index 1 = Enabled for simple toggles, \
             2 = Disabled. Multi-choice flags use higher indexes.",
        );
        ui.label(
            egui::RichText::new(
                "Removing an entry reverts that flag to its default. Unknown / mistyped \
                 flag names are silently ignored by Brave — no feedback here.",
            )
            .small()
            .weak(),
        );
        ui.separator();

        let dirty = self.flags_working != self.flags_original;
        ui.horizontal(|ui| {
            ui.label(format!("{} flag(s) set", self.flags_working.len()));
            if dirty {
                ui.colored_label(egui::Color32::from_rgb(220, 140, 30), "unsaved changes");
            }
            if ui.button("Reload from disk").clicked() {
                self.reload_profile_views();
            }
            if ui.button("Reset all (clear list)").clicked() {
                self.flags_working.clear();
            }
        });

        egui::ScrollArea::vertical()
            .id_salt("flags_list_scroll")
            .max_height(360.0)
            .show(ui, |ui| {
                if self.flags_working.is_empty() {
                    ui.weak("(no custom flags — everything is at Brave's defaults)");
                }
                let mut to_remove: Option<usize> = None;
                for (idx, entry) in self.flags_working.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui
                            .small_button("×")
                            .on_hover_text("Remove this flag (revert to default)")
                            .clicked()
                        {
                            to_remove = Some(idx);
                        }
                        ui.monospace(entry);
                    });
                }
                if let Some(i) = to_remove {
                    self.flags_working.remove(i);
                }
            });

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Add flag:");
            let resp = ui.text_edit_singleline(&mut self.flags_add_buffer);
            let pressed_enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Add").clicked() || pressed_enter {
                self.do_add_flag();
            }
        });
        ui.label(
            egui::RichText::new(
                "Format: \"<flag-name>@<N>\" (e.g. \"enable-experimental-web-platform-features@1\"). \
                 A plain name without @N defaults to @1.",
            )
            .small()
            .weak(),
        );

        ui.separator();
        ui.horizontal(|ui| {
            let apply_btn = egui::Button::new("Apply changes");
            let resp = ui.add_enabled(self.can_mutate() && dirty, apply_btn);
            if !self.can_mutate() {
                resp.clone().on_disabled_hover_text(self.disabled_reason());
            }
            if resp.clicked() {
                self.do_apply_flags();
            }
            if ui
                .add_enabled(dirty, egui::Button::new("Discard edits"))
                .clicked()
            {
                self.flags_working = self.flags_original.clone();
            }
        });
        ui.label(
            egui::RichText::new(
                "Writes browser.enabled_labs_experiments in Local State with a \
                 timestamped .bak- backup. Restart Brave for changes to take effect.",
            )
            .small()
            .weak(),
        );
    }

    fn ui_preferences(&mut self, ui: &mut egui::Ui) {
        if self.current_profile().is_none() {
            ui.label("(no profile selected)");
            return;
        }

        // Detect whether what's currently loaded is a backup (.bak-*) rather
        // than the active Preferences — so we can colour the File: label and
        // the reload button differently.
        let loaded_is_backup = self
            .prefs_loaded_from
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().contains(".bak-"))
            .unwrap_or(false);
        let live_path = self.current_profile().map(|p| p.prefs_path());

        ui.horizontal(|ui| {
            // Button label depends on context:
            //   - nothing loaded           -> "Load active Preferences"
            //   - backup loaded            -> "← Back to active Preferences"
            //   - live file loaded         -> "Reload active Preferences"
            let reload_label = match (&self.prefs_loaded_from, loaded_is_backup) {
                (None, _) => "Load active Preferences",
                (Some(_), true) => "← Back to active Preferences",
                (Some(_), false) => "Reload active Preferences",
            };
            if ui
                .button(reload_label)
                .on_hover_text(
                    live_path
                        .as_ref()
                        .map(|p| format!("Loads {}", p.display()))
                        .unwrap_or_else(|| "Loads the selected profile's Preferences".to_string()),
                )
                .clicked()
            {
                self.do_load_preferences();
            }
            if ui
                .button("Pretty")
                .on_hover_text("Reformat JSON with indentation")
                .clicked()
            {
                self.do_prettify_preferences();
            }
            if ui
                .button("Minify")
                .on_hover_text("Collapse to one line (Brave's own format)")
                .clicked()
            {
                self.do_minify_preferences();
            }
            let save = egui::Button::new("Save (validate + backup)");
            let enabled = self.can_mutate() && self.prefs_loaded_from.is_some();
            if ui.add_enabled(enabled, save).clicked() {
                self.do_save_preferences();
            }
            if self.prefs_dirty {
                ui.colored_label(egui::Color32::from_rgb(220, 140, 30), "unsaved changes");
            }
        });

        // File: <path> with a coloured "editing backup" badge when relevant.
        if let Some(path) = self.prefs_loaded_from.clone() {
            ui.horizontal(|ui| {
                if loaded_is_backup {
                    ui.colored_label(egui::Color32::from_rgb(220, 140, 30), "Editing BACKUP →");
                } else {
                    ui.label("File:");
                }
                ui.monospace(path.display().to_string());
            });
            if loaded_is_backup {
                ui.label(
                    egui::RichText::new(
                        "Save will write into this backup file, not the active \
                         Preferences. To apply an edited backup to Brave, use \
                         Restore on the Backups tab after saving.",
                    )
                    .small()
                    .weak(),
                );
            }
        } else {
            ui.label("No file loaded. Click 'Load active Preferences' to read the selected profile's Preferences file.");
            return;
        }
        ui.horizontal(|ui| {
            ui.label("Find:");
            ui.text_edit_singleline(&mut self.prefs_search);
            if !self.prefs_search.is_empty() {
                let needle = self.prefs_search.to_lowercase();
                let count = self.prefs_buffer.to_lowercase().matches(&needle).count();
                ui.label(format!("{} match(es)", count));
            }
        });
        if let Some(err) = &self.prefs_parse_error {
            ui.colored_label(
                egui::Color32::from_rgb(200, 80, 80),
                format!("JSON error: {}", err),
            );
        }
        egui::ScrollArea::both().show(ui, |ui| {
            let resp = ui.add(
                egui::TextEdit::multiline(&mut self.prefs_buffer)
                    .font(egui::TextStyle::Monospace)
                    .code_editor()
                    .desired_rows(28)
                    .desired_width(f32::INFINITY),
            );
            if resp.changed() {
                self.prefs_dirty = true;
            }
        });
    }

    fn ui_backups(&mut self, ui: &mut egui::Ui) {
        if let Some(prof) = self.current_profile() {
            ui.label(format!(
                "Backups for profile: {} ({})",
                prof.dir_name,
                prof.path.display()
            ));
        }
        ui.horizontal(|ui| {
            if ui.button("Create backup now").clicked() {
                self.do_backup();
            }
        });
        ui.separator();
        if self.backup_files.is_empty() {
            ui.label("(no backup files found)");
            return;
        }
        let backups = self.backup_files.clone();
        egui::ScrollArea::vertical()
            .max_height(180.0)
            .show(ui, |ui| {
                for bkp in backups {
                    ui.horizontal(|ui| {
                        ui.monospace(
                            bkp.file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                        );
                        if ui
                            .button("Open in editor")
                            .on_hover_text(
                                "Load this backup into the Preferences tab. \
                                 Save writes edits back into the backup file; \
                                 use Restore to copy the (edited) backup over \
                                 the active Preferences.",
                            )
                            .clicked()
                        {
                            self.load_file_into_editor(bkp.clone());
                        }
                        let restore = egui::Button::new("Restore");
                        if ui.add_enabled(self.can_mutate(), restore).clicked() {
                            let _ = self.do_restore(bkp.clone());
                        }
                        if ui.button("Diff vs current").clicked() {
                            self.do_diff_backup(bkp.clone());
                        }
                    });
                }
            });
        if !self.diff_output.is_empty() {
            ui.separator();
            if let Some(p) = &self.diff_from {
                ui.label(format!(
                    "Diff: {} vs current Preferences",
                    p.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                ));
            }
            egui::ScrollArea::vertical()
                .id_salt("diff_scroll")
                .show(ui, |ui| {
                    for line in &self.diff_output {
                        ui.monospace(line);
                    }
                });
        }
    }
}

pub fn run() -> Result<()> {
    // Translate CTRL_C_EVENT / CTRL_CLOSE_EVENT into a process exit. Because
    // this binary is compiled as a console-subsystem exe (see main.rs header),
    // cmd/PowerShell waits for it and forwards console control events.
    let _ = ctrlc::set_handler(|| {
        std::process::exit(0);
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 620.0])
            .with_min_inner_size([700.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Brave Offline Config Editor",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("GUI error: {}", e))
}
