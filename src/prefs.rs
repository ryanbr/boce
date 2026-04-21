use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde_json::Value;

/// Read `Preferences` as a JSON `Value`, preserving key order.
pub fn read_prefs(path: &Path) -> Result<Value> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let v: Value = serde_json::from_str(&raw)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(v)
}

/// Write `Preferences` back. Creates a timestamped backup of the existing file first.
/// Uses an atomic rename (write to .tmp then rename) so an interrupted write can't
/// truncate Preferences.
pub fn write_prefs(path: &Path, value: &Value) -> Result<PathBuf> {
    let backup = backup_file(path)?;

    let tmp = path.with_extension("brave-config.tmp");
    let pretty = serde_json::to_string(value).context("serializing Preferences")?;
    std::fs::write(&tmp, pretty)
        .with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("replacing {}", path.display()))?;

    Ok(backup)
}

/// Copy `path` to `path.bak-YYYYmmdd-HHMMSS` (same directory). Returns the backup path.
pub fn backup_file(path: &Path) -> Result<PathBuf> {
    if !path.is_file() {
        return Err(anyhow!("cannot back up: {} does not exist", path.display()));
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("invalid path: {}", path.display()))?
        .to_string_lossy()
        .into_owned();
    let backup_name = format!("{}.bak-{}", file_name, stamp);
    let backup = path.with_file_name(backup_name);
    std::fs::copy(path, &backup)
        .with_context(|| format!("copying {} -> {}", path.display(), backup.display()))?;
    Ok(backup)
}

/// Restore a backup file (`<name>.bak-*`) over its original.
pub fn restore_backup(backup: &Path) -> Result<PathBuf> {
    let name = backup
        .file_name()
        .ok_or_else(|| anyhow!("invalid backup path"))?
        .to_string_lossy()
        .into_owned();
    let (orig_name, _) = name
        .rsplit_once(".bak-")
        .ok_or_else(|| anyhow!("not a backup file (expected .bak-<stamp>): {}", name))?;
    let target = backup.with_file_name(orig_name);

    // Back up the current file (if present) before overwriting, so restore is reversible.
    if target.is_file() {
        let _ = backup_file(&target)?;
    }
    std::fs::copy(backup, &target)
        .with_context(|| format!("restoring {} -> {}", backup.display(), target.display()))?;
    Ok(target)
}

/// Recursively compare two JSON values. Returns a list of (path, left, right)
/// tuples describing every leaf where they differ. A leaf is a non-object,
/// non-array value, or an entire subtree missing from one side.
pub fn diff_values(a: &Value, b: &Value) -> Vec<DiffEntry> {
    let mut out = Vec::new();
    walk_diff("", a, b, &mut out);
    out
}

#[derive(Debug, Clone)]
pub struct DiffEntry {
    pub path: String,
    pub left: Option<Value>,
    pub right: Option<Value>,
}

fn walk_diff(path: &str, a: &Value, b: &Value, out: &mut Vec<DiffEntry>) {
    if a == b {
        return;
    }
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            let mut keys: Vec<&String> = ma.keys().chain(mb.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let child_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", path, k)
                };
                match (ma.get(k), mb.get(k)) {
                    (Some(av), Some(bv)) => walk_diff(&child_path, av, bv, out),
                    (Some(av), None) => out.push(DiffEntry {
                        path: child_path,
                        left: Some(av.clone()),
                        right: None,
                    }),
                    (None, Some(bv)) => out.push(DiffEntry {
                        path: child_path,
                        left: None,
                        right: Some(bv.clone()),
                    }),
                    (None, None) => {}
                }
            }
        }
        _ => out.push(DiffEntry {
            path: path.to_string(),
            left: Some(a.clone()),
            right: Some(b.clone()),
        }),
    }
}

/// Traverse `root` by dot-separated path, e.g. `brave.shields.ads_default`.
/// Returns a reference to the value or `None` if any segment is missing.
pub fn get_path<'a>(root: &'a Value, dotted: &str) -> Option<&'a Value> {
    let mut cur = root;
    for seg in dotted.split('.') {
        cur = cur.get(seg)?;
    }
    Some(cur)
}

/// Mutable counterpart of `get_path`.
pub fn get_path_mut<'a>(root: &'a mut Value, dotted: &str) -> Option<&'a mut Value> {
    let mut cur = root;
    for seg in dotted.split('.') {
        cur = cur.as_object_mut()?.get_mut(seg)?;
    }
    Some(cur)
}

/// Ensure an object exists at `dotted`, creating intermediate objects as needed.
/// If the leaf exists but is not an object, returns an error.
pub fn ensure_object(root: &mut Value, dotted: &str) -> Result<()> {
    let segs: Vec<&str> = dotted.split('.').collect();
    if segs.is_empty() {
        return Err(anyhow!("empty key"));
    }
    let mut cur = root;
    for seg in &segs {
        if !cur.is_object() {
            return Err(anyhow!("cannot descend into non-object"));
        }
        let map = cur.as_object_mut().unwrap();
        if !map.contains_key(*seg) {
            map.insert((*seg).to_string(), Value::Object(Default::default()));
        }
        let child = map.get_mut(*seg).unwrap();
        if !child.is_object() {
            return Err(anyhow!("'{}' exists but is not a JSON object", seg));
        }
        cur = child;
    }
    Ok(())
}

/// Set a value at `dotted`, creating intermediate objects as needed.
/// Errors if an intermediate segment exists and is not an object.
pub fn set_path(root: &mut Value, dotted: &str, new_value: Value) -> Result<()> {
    let segs: Vec<&str> = dotted.split('.').collect();
    if segs.is_empty() {
        return Err(anyhow!("empty key"));
    }
    let mut cur = root;
    for seg in &segs[..segs.len() - 1] {
        if !cur.is_object() {
            return Err(anyhow!("cannot descend into non-object at '{}'", seg));
        }
        let map = cur.as_object_mut().unwrap();
        if !map.contains_key(*seg) {
            map.insert((*seg).to_string(), Value::Object(Default::default()));
        }
        cur = map.get_mut(*seg).unwrap();
    }
    let last = segs.last().unwrap();
    let map = cur
        .as_object_mut()
        .ok_or_else(|| anyhow!("parent of '{}' is not an object", last))?;
    map.insert((*last).to_string(), new_value);
    Ok(())
}
