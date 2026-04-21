//! Brave adblock filter list state (stored in Local State).
//!
//! Prefs:
//!   brave.ad_block.regional_filters   dict UUID -> { "enabled": bool }
//!   brave.ad_block.list_subscriptions dict URL  -> { "enabled": bool, "title": str, ... }
//!   brave.ad_block.custom_filters     string (multi-line filter rules)
//!
//! The "regional_filters" dict only stores entries the user has deviated from
//! the catalog default on. The catalog itself lives on disk as a
//! component-updater-delivered file:
//!   <UserDataDir>/gkboaolpopklhgplhaaiboijnklogmbc/<version>/list_catalog.json
//! Each catalog entry carries uuid, title, langs, default_enabled, hidden, etc.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

pub const CATALOG_COMPONENT_ID: &str = "gkboaolpopklhgplhaaiboijnklogmbc";
pub const CATALOG_FILE_NAME: &str = "list_catalog.json";

pub const PREF_REGIONAL: &str = "brave.ad_block.regional_filters";
pub const PREF_SUBSCRIPTIONS: &str = "brave.ad_block.list_subscriptions";
pub const PREF_CUSTOM_FILTERS: &str = "brave.ad_block.custom_filters";

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // url + desc are metadata we may surface later
pub struct CatalogEntry {
    pub uuid: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub langs: Vec<String>,
    #[serde(default)]
    pub default_enabled: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub desc: String,
}

/// Locate the latest version directory for the filter-list catalog component
/// under `<user_data>/<component_id>/` and return the `list_catalog.json` path.
pub fn catalog_path(user_data: &Path) -> Option<PathBuf> {
    let comp_dir = user_data.join(CATALOG_COMPONENT_ID);
    if !comp_dir.is_dir() {
        return None;
    }
    let mut versions: Vec<(Vec<u64>, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(&comp_dir).ok()? {
        let entry = entry.ok()?;
        if !entry.file_type().ok()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let parts: Vec<u64> = name.split('.').filter_map(|p| p.parse().ok()).collect();
        if parts.is_empty() {
            continue;
        }
        versions.push((parts, entry.path()));
    }
    versions.sort_by(|a, b| a.0.cmp(&b.0));
    let latest = versions.pop()?.1;
    let catalog = latest.join(CATALOG_FILE_NAME);
    if catalog.is_file() {
        Some(catalog)
    } else {
        None
    }
}

/// Load + parse the filter-list catalog. Returns an empty list (not an error)
/// when the catalog isn't available — users can still edit raw UUIDs via the
/// Preferences tab.
pub fn load_catalog(user_data: &Path) -> Result<Vec<CatalogEntry>> {
    let Some(path) = catalog_path(user_data) else {
        return Ok(Vec::new());
    };
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut entries: Vec<CatalogEntry> =
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
    // Pre-sort once at load time so the GUI doesn't re-sort a ~300-entry
    // catalog on every frame.
    entries.sort_by_key(|e| e.title.to_lowercase());
    Ok(entries)
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // last_update may surface in UI later
pub struct Subscription {
    pub url: String,
    pub enabled: bool,
    pub title: Option<String>,
    pub last_update: Option<String>,
}

/// Parse the `list_subscriptions` dict from a Local State root into a sorted Vec.
pub fn read_subscriptions(root: &serde_json::Value) -> Vec<Subscription> {
    let mut out = Vec::new();
    let Some(dict) = crate::prefs::get_path(root, PREF_SUBSCRIPTIONS) else {
        return out;
    };
    let Some(map) = dict.as_object() else {
        return out;
    };
    for (url, v) in map {
        let enabled = v.get("enabled").and_then(|b| b.as_bool()).unwrap_or(false);
        let title = v.get("title").and_then(|t| t.as_str()).map(str::to_string);
        let last_update = v
            .get("last_successful_update_attempt")
            .or_else(|| v.get("last_update_attempt"))
            .and_then(|t| t.as_str())
            .map(str::to_string);
        out.push(Subscription {
            url: url.clone(),
            enabled,
            title,
            last_update,
        });
    }
    out.sort_by(|a, b| a.url.cmp(&b.url));
    out
}

/// Read the enabled state for a given regional-filter UUID from Local State.
/// Returns None when no explicit value is stored (catalog default applies).
pub fn read_regional_enabled(root: &serde_json::Value, uuid: &str) -> Option<bool> {
    crate::prefs::get_path(root, PREF_REGIONAL)?
        .as_object()?
        .get(uuid)?
        .get("enabled")?
        .as_bool()
}

/// Write the enabled state for a regional-filter UUID. Creates the
/// `brave.ad_block.regional_filters` dict if absent.
pub fn write_regional_enabled(
    root: &mut serde_json::Value,
    uuid: &str,
    enabled: bool,
) -> Result<()> {
    crate::prefs::ensure_object(root, PREF_REGIONAL)?;
    let dict = crate::prefs::get_path_mut(root, PREF_REGIONAL)
        .ok_or_else(|| anyhow::anyhow!("failed to resolve {}", PREF_REGIONAL))?;
    let map = dict
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("{} is not an object", PREF_REGIONAL))?;
    let entry = map
        .entry(uuid.to_string())
        .or_insert_with(|| serde_json::Value::Object(Default::default()));
    let obj = entry
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("regional entry for {} is not an object", uuid))?;
    obj.insert("enabled".to_string(), serde_json::Value::Bool(enabled));
    Ok(())
}

/// Write a subscription entry (enabled state). Creates minimal fields.
pub fn write_subscription_enabled(
    root: &mut serde_json::Value,
    url: &str,
    enabled: bool,
) -> Result<()> {
    crate::prefs::ensure_object(root, PREF_SUBSCRIPTIONS)?;
    let dict = crate::prefs::get_path_mut(root, PREF_SUBSCRIPTIONS)
        .ok_or_else(|| anyhow::anyhow!("failed to resolve {}", PREF_SUBSCRIPTIONS))?;
    let map = dict
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("{} is not an object", PREF_SUBSCRIPTIONS))?;
    let entry = map
        .entry(url.to_string())
        .or_insert_with(|| serde_json::Value::Object(Default::default()));
    let obj = entry
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("subscription entry is not an object"))?;
    obj.insert("enabled".to_string(), serde_json::Value::Bool(enabled));
    Ok(())
}

/// Remove a subscription entirely.
pub fn remove_subscription(root: &mut serde_json::Value, url: &str) -> Result<()> {
    let Some(dict) = crate::prefs::get_path_mut(root, PREF_SUBSCRIPTIONS) else {
        return Ok(());
    };
    if let Some(map) = dict.as_object_mut() {
        map.remove(url);
    }
    Ok(())
}

pub fn read_custom_filters(root: &serde_json::Value) -> String {
    crate::prefs::get_path(root, PREF_CUSTOM_FILTERS)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

pub fn write_custom_filters(root: &mut serde_json::Value, text: &str) -> Result<()> {
    crate::prefs::set_path(
        root,
        PREF_CUSTOM_FILTERS,
        serde_json::Value::String(text.to_string()),
    )
}
