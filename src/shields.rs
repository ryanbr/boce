//! Curated allowlist of Brave shield settings we know how to read and write.
//!
//! Each shield has a set of symbolic values; each value is a recipe of JSON
//! writes (one per storage location). This lets us model settings like
//! "trackers-and-ads" whose Standard and Aggressive modes differ on a
//! *distinguishing* key (cosmeticFiltering) while sharing values on others
//! (shieldsAds, trackers).
//!
//! Exception-shaped writes go to:
//!   profile.content_settings.exceptions.<feature>."*,*"
//!     = { "last_modified": "<FILETIME-us>", "setting": <int> }
//!
//! Direct-shaped writes go to a dotted JSON path, as a bool or int.

use anyhow::{Result, anyhow};
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub enum ValueEnc {
    Int(i64),
    Bool(bool),
    Str(&'static str),
    /// A single-field dict `{<inner_key>: <int>}`. Used by Brave content
    /// settings like `cosmeticFilteringV2` whose `setting` field is itself
    /// a JSON object rather than a bare int.
    Dict(&'static str, i64),
}

impl ValueEnc {
    pub fn to_json(self) -> Value {
        match self {
            ValueEnc::Int(n) => Value::from(n),
            ValueEnc::Bool(b) => Value::from(b),
            ValueEnc::Str(s) => Value::from(s),
            ValueEnc::Dict(k, n) => {
                let mut m = serde_json::Map::new();
                m.insert(k.to_string(), Value::from(n));
                Value::Object(m)
            }
        }
    }

    pub fn matches(self, v: &Value) -> bool {
        match self {
            ValueEnc::Int(n) => v.as_i64() == Some(n),
            ValueEnc::Bool(b) => v.as_bool() == Some(b),
            ValueEnc::Str(s) => v.as_str() == Some(s),
            ValueEnc::Dict(k, n) => v
                .as_object()
                .and_then(|m| m.get(k))
                .and_then(|x| x.as_i64())
                == Some(n),
        }
    }
}

/// Individual write target + the value to write there.
#[derive(Debug, Clone, Copy)]
pub enum WriteTarget {
    /// Shield-style exception map: write to `<feature>."*,*".setting`.
    Exception(&'static str, ValueEnc),
    /// Direct dotted path.
    Direct(&'static str, ValueEnc),
}

impl WriteTarget {
    pub fn path_for_display(&self) -> String {
        match self {
            WriteTarget::Exception(feature, _) => format!("{}.\"*,*\".setting", feature),
            WriteTarget::Direct(path, _) => (*path).to_string(),
        }
    }
}

/// One symbolic value in a shield's allowed set.
#[derive(Debug, Clone, Copy)]
pub struct ShieldValue {
    pub symbol: &'static str,
    pub writes: &'static [WriteTarget],
}

/// Where to read when determining which symbol is current. Must be one of the
/// write targets used by *every* symbol (with distinct values, so we can tell
/// them apart).
#[derive(Debug, Clone, Copy)]
pub enum ReadTarget {
    Exception(&'static str),
    Direct(&'static str),
}

/// Which GUI tab a given shield belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Shields,
    Privacy,
    System,
}

/// Which on-disk file holds this shield's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefFile {
    /// Per-profile `<profile>/Preferences`.
    Preferences,
    /// Browser-wide `<user_data>/Local State`.
    LocalState,
}

#[derive(Debug, Clone, Copy)]
pub struct ShieldKey {
    pub name: &'static str,
    pub pane: Pane,
    pub section: &'static str,
    pub file: PrefFile,
    pub desc: &'static str,
    pub read_target: ReadTarget,
    pub values: &'static [ShieldValue],
}

pub const SECTION_SHIELDS: &str = "Block trackers and ads";
pub const SECTION_PRIVACY: &str = "Privacy and security";
pub const SECTION_TOR: &str = "Tor windows";
pub const SECTION_SYSTEM: &str = "System";

// Feature paths under profile.content_settings.exceptions.
const EX_SHIELDS_ADS: &str = "profile.content_settings.exceptions.shieldsAds";
const EX_TRACKERS: &str = "profile.content_settings.exceptions.trackers";
const EX_COSMETIC_V2: &str = "profile.content_settings.exceptions.cosmeticFilteringV2";
const COSMETIC_V2_INNER: &str = "cosmeticFilteringV2";
const EX_HTTPS: &str = "profile.content_settings.exceptions.httpsUpgrades";
const EX_JS: &str = "profile.content_settings.exceptions.javascript";
const EX_FP: &str = "profile.content_settings.exceptions.fingerprintingV2";
const EX_COOKIES: &str = "profile.content_settings.exceptions.shieldsCookiesV3";
const EX_FORGET: &str = "profile.content_settings.exceptions.brave_remember_1p_storage";

pub const KEYS: &[ShieldKey] = &[
    // Trackers & ads — Brave's combined dropdown. Distinguisher is
    // cosmeticFilteringV2, whose `setting` is a dict {cosmeticFilteringV2: int}.
    // ControlType enum: ALLOW=0, BLOCK=1 (Aggressive), BLOCK_THIRD_PARTY=2
    // (Standard), DEFAULT=3. shieldsAds/trackers use the Chromium
    // ContentSetting enum (1=ALLOW, 2=BLOCK).
    ShieldKey {
        name: "trackers-and-ads",
        pane: Pane::Shields,
        section: SECTION_SHIELDS,
        file: PrefFile::Preferences,
        desc: "Trackers & ads blocking (Brave's combined Shields dropdown)",
        read_target: ReadTarget::Exception(EX_COSMETIC_V2),
        values: &[
            ShieldValue {
                symbol: "disabled",
                writes: &[
                    WriteTarget::Exception(EX_SHIELDS_ADS, ValueEnc::Int(1)),
                    WriteTarget::Exception(EX_TRACKERS, ValueEnc::Int(1)),
                    WriteTarget::Exception(
                        EX_COSMETIC_V2,
                        ValueEnc::Dict(COSMETIC_V2_INNER, 0),
                    ),
                ],
            },
            ShieldValue {
                symbol: "standard",
                writes: &[
                    WriteTarget::Exception(EX_SHIELDS_ADS, ValueEnc::Int(2)),
                    WriteTarget::Exception(EX_TRACKERS, ValueEnc::Int(2)),
                    WriteTarget::Exception(
                        EX_COSMETIC_V2,
                        ValueEnc::Dict(COSMETIC_V2_INNER, 2),
                    ),
                ],
            },
            ShieldValue {
                symbol: "aggressive",
                writes: &[
                    WriteTarget::Exception(EX_SHIELDS_ADS, ValueEnc::Int(2)),
                    WriteTarget::Exception(EX_TRACKERS, ValueEnc::Int(2)),
                    WriteTarget::Exception(
                        EX_COSMETIC_V2,
                        ValueEnc::Dict(COSMETIC_V2_INNER, 1),
                    ),
                ],
            },
        ],
    },
    ShieldKey {
        name: "https-upgrade",
        pane: Pane::Shields,
        section: SECTION_SHIELDS,
        file: PrefFile::Preferences,
        desc: "Upgrade connections to HTTPS",
        read_target: ReadTarget::Exception(EX_HTTPS),
        values: &[
            ShieldValue {
                symbol: "disabled",
                writes: &[WriteTarget::Exception(EX_HTTPS, ValueEnc::Int(1))],
            },
            ShieldValue {
                symbol: "standard",
                writes: &[WriteTarget::Exception(EX_HTTPS, ValueEnc::Int(3))],
            },
            ShieldValue {
                symbol: "strict",
                writes: &[WriteTarget::Exception(EX_HTTPS, ValueEnc::Int(2))],
            },
        ],
    },
    ShieldKey {
        name: "block-scripts",
        pane: Pane::Shields,
        section: SECTION_SHIELDS,
        file: PrefFile::Preferences,
        desc: "Block scripts on every site by default",
        read_target: ReadTarget::Exception(EX_JS),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Exception(EX_JS, ValueEnc::Int(1))],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Exception(EX_JS, ValueEnc::Int(2))],
            },
        ],
    },
    ShieldKey {
        name: "block-fingerprinting",
        pane: Pane::Shields,
        section: SECTION_SHIELDS,
        file: PrefFile::Preferences,
        desc: "Block fingerprinting (on = Standard; strict blocks aggressively)",
        read_target: ReadTarget::Exception(EX_FP),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Exception(EX_FP, ValueEnc::Int(1))],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Exception(EX_FP, ValueEnc::Int(3))],
            },
            ShieldValue {
                symbol: "strict",
                writes: &[WriteTarget::Exception(EX_FP, ValueEnc::Int(2))],
            },
        ],
    },
    ShieldKey {
        name: "block-cookies",
        pane: Pane::Shields,
        section: SECTION_SHIELDS,
        file: PrefFile::Preferences,
        desc: "Cookie policy under shields",
        read_target: ReadTarget::Exception(EX_COOKIES),
        values: &[
            ShieldValue {
                symbol: "allow",
                writes: &[WriteTarget::Exception(EX_COOKIES, ValueEnc::Int(1))],
            },
            ShieldValue {
                symbol: "block",
                writes: &[WriteTarget::Exception(EX_COOKIES, ValueEnc::Int(2))],
            },
            ShieldValue {
                symbol: "block-third-party",
                writes: &[WriteTarget::Exception(EX_COOKIES, ValueEnc::Int(3))],
            },
        ],
    },
    ShieldKey {
        name: "forget-me",
        pane: Pane::Shields,
        section: SECTION_SHIELDS,
        file: PrefFile::Preferences,
        desc: "Forget me when I close this site (block = forget)",
        read_target: ReadTarget::Exception(EX_FORGET),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Exception(EX_FORGET, ValueEnc::Int(1))],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Exception(EX_FORGET, ValueEnc::Int(2))],
            },
        ],
    },
    ShieldKey {
        name: "de-amp",
        pane: Pane::Privacy,
        section: SECTION_PRIVACY,
        file: PrefFile::Preferences,
        desc: "Auto-redirect AMP pages (prefer publisher URLs)",
        read_target: ReadTarget::Direct("brave.de_amp.enabled"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct("brave.de_amp.enabled", ValueEnc::Bool(false))],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct("brave.de_amp.enabled", ValueEnc::Bool(true))],
            },
        ],
    },
    ShieldKey {
        name: "debounce",
        pane: Pane::Privacy,
        section: SECTION_PRIVACY,
        file: PrefFile::Preferences,
        desc: "Auto-redirect tracking URLs (debounce)",
        read_target: ReadTarget::Direct("brave.debounce.enabled"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct("brave.debounce.enabled", ValueEnc::Bool(false))],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct("brave.debounce.enabled", ValueEnc::Bool(true))],
            },
        ],
    },
    ShieldKey {
        name: "reduce-language",
        pane: Pane::Privacy,
        section: SECTION_PRIVACY,
        file: PrefFile::Preferences,
        desc: "Prevent sites from fingerprinting me based on my language preferences",
        read_target: ReadTarget::Direct("brave.reduce_language"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct("brave.reduce_language", ValueEnc::Bool(false))],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct("brave.reduce_language", ValueEnc::Bool(true))],
            },
        ],
    },
    // Standard Chromium pref: Send "Do Not Track" header.
    ShieldKey {
        name: "send-dnt",
        pane: Pane::Privacy,
        section: SECTION_PRIVACY,
        file: PrefFile::Preferences,
        desc: "Send a \"Do Not Track\" request with your browsing traffic",
        read_target: ReadTarget::Direct("enable_do_not_track"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct("enable_do_not_track", ValueEnc::Bool(false))],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct("enable_do_not_track", ValueEnc::Bool(true))],
            },
        ],
    },
    // Standard Chromium pref: WebRTC IP handling policy. String-valued.
    // Values from chrome/browser/media/webrtc/webrtc_event_log_manager.cc and
    // content/public/common/webrtc_ip_handling_policy.h.
    ShieldKey {
        name: "webrtc-policy",
        pane: Pane::Privacy,
        section: SECTION_PRIVACY,
        file: PrefFile::Preferences,
        desc: "WebRTC IP handling policy (controls what IPs WebRTC may expose)",
        read_target: ReadTarget::Direct("webrtc.ip_handling_policy"),
        values: &[
            ShieldValue {
                symbol: "default",
                writes: &[WriteTarget::Direct(
                    "webrtc.ip_handling_policy",
                    ValueEnc::Str("default"),
                )],
            },
            ShieldValue {
                symbol: "default-public-and-private",
                writes: &[WriteTarget::Direct(
                    "webrtc.ip_handling_policy",
                    ValueEnc::Str("default_public_and_private_interfaces"),
                )],
            },
            ShieldValue {
                symbol: "default-public-only",
                writes: &[WriteTarget::Direct(
                    "webrtc.ip_handling_policy",
                    ValueEnc::Str("default_public_interface_only"),
                )],
            },
            ShieldValue {
                symbol: "disable-non-proxied-udp",
                writes: &[WriteTarget::Direct(
                    "webrtc.ip_handling_policy",
                    ValueEnc::Str("disable_non_proxied_udp"),
                )],
            },
        ],
    },
    // Go Off-The-Record when visiting sensitive sites (dropdown).
    ShieldKey {
        name: "otr-sensitive-sites",
        pane: Pane::Privacy,
        section: SECTION_PRIVACY,
        file: PrefFile::Preferences,
        desc: "Go Off-The-Record when visiting sensitive sites (Ask / Always / Never)",
        read_target: ReadTarget::Direct("brave.request_otr.request_otr_action_option"),
        values: &[
            ShieldValue {
                symbol: "ask",
                writes: &[WriteTarget::Direct(
                    "brave.request_otr.request_otr_action_option",
                    ValueEnc::Int(0),
                )],
            },
            ShieldValue {
                symbol: "always",
                writes: &[WriteTarget::Direct(
                    "brave.request_otr.request_otr_action_option",
                    ValueEnc::Int(1),
                )],
            },
            ShieldValue {
                symbol: "never",
                writes: &[WriteTarget::Direct(
                    "brave.request_otr.request_otr_action_option",
                    ValueEnc::Int(2),
                )],
            },
        ],
    },
    // Use Google services for push messaging.
    ShieldKey {
        name: "google-push",
        pane: Pane::Privacy,
        section: SECTION_PRIVACY,
        file: PrefFile::Preferences,
        desc: "Use Google services for push messaging",
        read_target: ReadTarget::Direct("brave.gcm.channel_status"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct(
                    "brave.gcm.channel_status",
                    ValueEnc::Bool(false),
                )],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct(
                    "brave.gcm.channel_status",
                    ValueEnc::Bool(true),
                )],
            },
        ],
    },
    // Block Microsoft Recall screenshots of Brave tabs. Lives in Local State.
    ShieldKey {
        name: "block-ms-recall",
        pane: Pane::Privacy,
        section: SECTION_PRIVACY,
        file: PrefFile::LocalState,
        desc: "Block Microsoft Recall from capturing Brave tabs (Windows only; default on)",
        read_target: ReadTarget::Direct("brave.windows_recall_disabled"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct(
                    "brave.windows_recall_disabled",
                    ValueEnc::Bool(false),
                )],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct(
                    "brave.windows_recall_disabled",
                    ValueEnc::Bool(true),
                )],
            },
        ],
    },
    // Tor in Private Window. INVERTED (pref is tor.tor_disabled).
    ShieldKey {
        name: "tor-private-window",
        pane: Pane::Privacy,
        section: SECTION_TOR,
        file: PrefFile::LocalState,
        desc: "Private window with Tor",
        read_target: ReadTarget::Direct("tor.tor_disabled"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct("tor.tor_disabled", ValueEnc::Bool(true))],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct("tor.tor_disabled", ValueEnc::Bool(false))],
            },
        ],
    },
    ShieldKey {
        name: "onion-only-in-tor",
        pane: Pane::Privacy,
        section: SECTION_TOR,
        file: PrefFile::LocalState,
        desc: "Only resolve .onion addresses in Tor windows",
        read_target: ReadTarget::Direct("tor.onion_only_in_tor_windows"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct(
                    "tor.onion_only_in_tor_windows",
                    ValueEnc::Bool(false),
                )],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct(
                    "tor.onion_only_in_tor_windows",
                    ValueEnc::Bool(true),
                )],
            },
        ],
    },
    // --- System tab ---
    // Standard Chromium: keep Brave alive in the system tray after all windows close.
    ShieldKey {
        name: "background-mode",
        pane: Pane::System,
        section: SECTION_SYSTEM,
        file: PrefFile::Preferences,
        desc: "Continue running background apps when Brave is closed",
        read_target: ReadTarget::Direct("background_mode.enabled"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct("background_mode.enabled", ValueEnc::Bool(false))],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct("background_mode.enabled", ValueEnc::Bool(true))],
            },
        ],
    },
    // Standard Chromium: GPU / hardware acceleration. Takes effect after restart.
    ShieldKey {
        name: "hardware-acceleration",
        pane: Pane::System,
        section: SECTION_SYSTEM,
        file: PrefFile::Preferences,
        desc: "Use graphics acceleration when available (requires restart)",
        read_target: ReadTarget::Direct("hardware_acceleration_mode.enabled"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct(
                    "hardware_acceleration_mode.enabled",
                    ValueEnc::Bool(false),
                )],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct(
                    "hardware_acceleration_mode.enabled",
                    ValueEnc::Bool(true),
                )],
            },
        ],
    },
    // Standard Chromium: warn confirmation when closing a window with multiple tabs.
    ShieldKey {
        name: "warn-before-quit",
        pane: Pane::System,
        section: SECTION_SYSTEM,
        file: PrefFile::Preferences,
        desc: "Warn me before closing window with multiple tabs",
        read_target: ReadTarget::Direct("browser.warn_before_closing_multiple_tabs"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct(
                    "browser.warn_before_closing_multiple_tabs",
                    ValueEnc::Bool(false),
                )],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct(
                    "browser.warn_before_closing_multiple_tabs",
                    ValueEnc::Bool(true),
                )],
            },
        ],
    },
    // Brave-specific: whether closing the last tab also closes the window.
    ShieldKey {
        name: "close-last-tab",
        pane: Pane::System,
        section: SECTION_SYSTEM,
        file: PrefFile::Preferences,
        desc: "Close window when closing last tab",
        read_target: ReadTarget::Direct("brave.enable_closing_last_tab"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct(
                    "brave.enable_closing_last_tab",
                    ValueEnc::Bool(false),
                )],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct(
                    "brave.enable_closing_last_tab",
                    ValueEnc::Bool(true),
                )],
            },
        ],
    },
    // Brave-specific: the "press Esc to exit fullscreen" bubble on entering fullscreen.
    ShieldKey {
        name: "fullscreen-reminder",
        pane: Pane::System,
        section: SECTION_SYSTEM,
        file: PrefFile::Preferences,
        desc: "Show full screen reminder to press Esc on exit",
        read_target: ReadTarget::Direct("brave.show_fullscreen_reminder"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct(
                    "brave.show_fullscreen_reminder",
                    ValueEnc::Bool(false),
                )],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct(
                    "brave.show_fullscreen_reminder",
                    ValueEnc::Bool(true),
                )],
            },
        ],
    },
    // Brave VPN: protocol preference. Requires Brave VPN subscription/install
    // to have visible effect.
    ShieldKey {
        name: "vpn-wireguard",
        pane: Pane::System,
        section: SECTION_SYSTEM,
        file: PrefFile::Preferences,
        desc: "Use WireGuard protocol in Brave VPN (else IKEv2 fallback)",
        read_target: ReadTarget::Direct("brave.brave_vpn.wireguard_enabled"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct(
                    "brave.brave_vpn.wireguard_enabled",
                    ValueEnc::Bool(false),
                )],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct(
                    "brave.brave_vpn.wireguard_enabled",
                    ValueEnc::Bool(true),
                )],
            },
        ],
    },
    // Memory Saver — Chromium's high-efficiency mode. The older .enabled bool
    // is deprecated; .state is the int enum (0=Disabled, 1=Deprecated, 2=Enabled).
    ShieldKey {
        name: "memory-saver",
        pane: Pane::System,
        section: SECTION_SYSTEM,
        file: PrefFile::Preferences,
        desc: "Memory Saver — frees memory from inactive tabs",
        read_target: ReadTarget::Direct("performance_tuning.high_efficiency_mode.state"),
        values: &[
            ShieldValue {
                symbol: "off",
                writes: &[WriteTarget::Direct(
                    "performance_tuning.high_efficiency_mode.state",
                    ValueEnc::Int(0),
                )],
            },
            ShieldValue {
                symbol: "on",
                writes: &[WriteTarget::Direct(
                    "performance_tuning.high_efficiency_mode.state",
                    ValueEnc::Int(2),
                )],
            },
        ],
    },
];

pub fn lookup(name: &str) -> Result<&'static ShieldKey> {
    KEYS.iter()
        .find(|k| k.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow!("unknown shield key '{}' (see `shields list-keys`)", name))
}

pub fn lookup_symbol(key: &ShieldKey, sym: &str) -> Result<&'static ShieldValue> {
    key.values
        .iter()
        .find(|v| v.symbol.eq_ignore_ascii_case(sym))
        .ok_or_else(|| {
            let allowed: Vec<&str> = key.values.iter().map(|v| v.symbol).collect();
            anyhow!(
                "invalid value '{}' for {} (allowed: {})",
                sym,
                key.name,
                allowed.join(", ")
            )
        })
}

/// Return the symbolic name of the currently-stored value, if we can decode it.
pub fn current_symbol(root: &Value, key: &ShieldKey) -> Option<&'static str> {
    let cur = read_raw(root, &key.read_target)?;
    for v in key.values {
        // Find the write in this symbol whose target matches read_target.
        for w in v.writes {
            if write_matches_read(w, &key.read_target) {
                let enc = match w {
                    WriteTarget::Exception(_, e) => *e,
                    WriteTarget::Direct(_, e) => *e,
                };
                if enc.matches(&cur) {
                    return Some(v.symbol);
                }
            }
        }
    }
    None
}

/// The raw JSON value stored at `read_target`, or None if missing.
pub fn read_raw(root: &Value, target: &ReadTarget) -> Option<Value> {
    match target {
        ReadTarget::Direct(path) => crate::prefs::get_path(root, path).cloned(),
        ReadTarget::Exception(feature) => read_exception_star(root, feature),
    }
}

fn write_matches_read(w: &WriteTarget, r: &ReadTarget) -> bool {
    match (w, r) {
        (WriteTarget::Exception(wf, _), ReadTarget::Exception(rf)) => wf == rf,
        (WriteTarget::Direct(wp, _), ReadTarget::Direct(rp)) => wp == rp,
        _ => false,
    }
}

fn read_exception_star(root: &Value, feature: &str) -> Option<Value> {
    let dict = crate::prefs::get_path(root, feature)?;
    let entry = dict.get("*,*")?;
    entry.get("setting").cloned()
}

/// Apply a ShieldValue — writes each target.
pub fn apply_value(root: &mut Value, v: &ShieldValue) -> Result<()> {
    for w in v.writes {
        match w {
            WriteTarget::Direct(path, enc) => crate::prefs::set_path(root, path, enc.to_json())?,
            WriteTarget::Exception(feature, enc) => write_exception_star(root, feature, *enc)?,
        }
    }
    Ok(())
}

/// Verify a ShieldValue actually persisted — every write target matches on disk.
pub fn verify_value(root: &Value, v: &ShieldValue) -> bool {
    for w in v.writes {
        let (cur, enc) = match w {
            WriteTarget::Direct(path, enc) => (crate::prefs::get_path(root, path).cloned(), *enc),
            WriteTarget::Exception(feature, enc) => (read_exception_star(root, feature), *enc),
        };
        match cur {
            Some(v) if enc.matches(&v) => {}
            _ => return false,
        }
    }
    true
}

fn write_exception_star(root: &mut Value, feature: &str, enc: ValueEnc) -> Result<()> {
    crate::prefs::ensure_object(root, feature)?;
    let dict = crate::prefs::get_path_mut(root, feature)
        .ok_or_else(|| anyhow!("failed to resolve {}", feature))?;
    let map = dict
        .as_object_mut()
        .ok_or_else(|| anyhow!("{} is not a JSON object", feature))?;

    let entry = map
        .entry("*,*".to_string())
        .or_insert_with(|| Value::Object(Default::default()));
    let entry_obj = entry
        .as_object_mut()
        .ok_or_else(|| anyhow!("'*,*' entry under {} is not an object", feature))?;
    entry_obj.insert("setting".to_string(), enc.to_json());
    entry_obj.insert(
        "last_modified".to_string(),
        Value::String(chrome_timestamp_now()),
    );
    Ok(())
}

fn chrome_timestamp_now() -> String {
    const EPOCH_OFFSET_US: i128 = 11_644_473_600 * 1_000_000;
    let unix_us = chrono::Utc::now().timestamp_micros() as i128;
    (unix_us + EPOCH_OFFSET_US).to_string()
}

/// Human path for CLI help output.
pub fn display_paths(k: &ShieldKey) -> String {
    let mut seen: Vec<String> = Vec::new();
    for v in k.values {
        for w in v.writes {
            let p = w.path_for_display();
            if !seen.contains(&p) {
                seen.push(p);
            }
        }
    }
    seen.join(", ")
}
