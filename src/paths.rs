use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Beta,
    Nightly,
    Dev,
}

impl Channel {
    pub const ALL: [Channel; 4] = [
        Channel::Stable,
        Channel::Beta,
        Channel::Nightly,
        Channel::Dev,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Channel::Stable => "Stable",
            Channel::Beta => "Beta",
            Channel::Nightly => "Nightly",
            Channel::Dev => "Dev",
        }
    }

    /// Directory suffix used by Brave on each platform.
    /// Windows + macOS use "Brave-Browser" (+ "-Beta" etc.).
    /// Linux uses the same under ~/.config.
    fn dir_suffix(self) -> &'static str {
        match self {
            Channel::Stable => "Brave-Browser",
            Channel::Beta => "Brave-Browser-Beta",
            Channel::Nightly => "Brave-Browser-Nightly",
            Channel::Dev => "Brave-Browser-Dev",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChannelPaths {
    pub channel: Channel,
    /// Directory containing `Local State` and the profile subdirs.
    pub user_data: PathBuf,
    /// Directory holding on-disk cache (same as user_data on Windows/macOS, separate on Linux).
    pub cache_root: PathBuf,
}

impl ChannelPaths {
    pub fn exists(&self) -> bool {
        self.local_state_path().is_file()
    }

    pub fn local_state_path(&self) -> PathBuf {
        self.user_data.join("Local State")
    }
}

#[cfg(target_os = "windows")]
pub fn channel_paths(ch: Channel) -> Option<ChannelPaths> {
    let local = dirs::data_local_dir()?;
    let user_data = local
        .join("BraveSoftware")
        .join(ch.dir_suffix())
        .join("User Data");
    let cache_root = user_data.clone();
    Some(ChannelPaths {
        channel: ch,
        user_data,
        cache_root,
    })
}

#[cfg(target_os = "linux")]
pub fn channel_paths(ch: Channel) -> Option<ChannelPaths> {
    let config = dirs::config_dir()?;
    let cache = dirs::cache_dir()?;
    let user_data = config.join("BraveSoftware").join(ch.dir_suffix());
    let cache_root = cache.join("BraveSoftware").join(ch.dir_suffix());
    Some(ChannelPaths {
        channel: ch,
        user_data,
        cache_root,
    })
}

#[cfg(target_os = "macos")]
pub fn channel_paths(ch: Channel) -> Option<ChannelPaths> {
    let home = dirs::home_dir()?;
    let user_data = home
        .join("Library/Application Support/BraveSoftware")
        .join(ch.dir_suffix());
    let cache_root = home
        .join("Library/Caches/BraveSoftware")
        .join(ch.dir_suffix());
    Some(ChannelPaths {
        channel: ch,
        user_data,
        cache_root,
    })
}

pub fn installed_channels() -> Vec<ChannelPaths> {
    Channel::ALL
        .iter()
        .filter_map(|ch| channel_paths(*ch))
        .filter(|p| p.exists())
        .collect()
}
