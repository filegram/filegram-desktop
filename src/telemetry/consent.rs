//! Where the build came from and whether it may report anything.
//!
//! Flathub requires reporting to be off until the user says otherwise, and the
//! Snap ships the same binary as the GitHub release, so the channel is detected
//! at runtime instead of being compiled in.

/// How this build was installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Flatpak,
    Snap,
    /// GitHub release and everything repackaging it: winget, Chocolatey,
    /// Homebrew, Scoop, deb/rpm, RuStore.
    Direct,
}

/// What to do on this launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Collect nothing until the user answers the first-run dialog.
    Ask,
    /// Collect; `notice` asks for the one-time banner shown to users who were
    /// never given a dialog.
    Collect {
        notice: bool,
    },
    Off,
}

impl Channel {
    pub fn detect() -> Self {
        Self::detect_from(
            std::path::Path::new("/.flatpak-info").exists(),
            std::env::var_os("SNAP").is_some(),
        )
    }

    fn detect_from(flatpak_info: bool, snap_var: bool) -> Self {
        match (flatpak_info, snap_var) {
            (true, _) => Channel::Flatpak,
            (false, true) => Channel::Snap,
            (false, false) => Channel::Direct,
        }
    }

    /// Reported as-is, so these names outlive any refactoring here.
    pub fn name(self) -> &'static str {
        match self {
            Channel::Flatpak => "flatpak",
            Channel::Snap => "snap",
            Channel::Direct => "direct",
        }
    }

    /// Store rules that demand an explicit opt-in.
    fn must_ask(self) -> bool {
        matches!(self, Channel::Flatpak | Channel::Snap)
    }
}

/// `saved` is the persisted choice; `None` means never asked.
pub fn decide(channel: Channel, saved: Option<bool>) -> Decision {
    match saved {
        Some(true) => Decision::Collect { notice: false },
        Some(false) => Decision::Off,
        None if channel.must_ask() => Decision::Ask,
        None => Decision::Collect { notice: true },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatpak_marker_wins_over_a_stray_snap_variable() {
        assert_eq!(Channel::detect_from(true, true), Channel::Flatpak);
    }

    #[test]
    fn snap_variable_alone_reads_as_snap() {
        assert_eq!(Channel::detect_from(false, true), Channel::Snap);
    }

    #[test]
    fn no_marker_reads_as_a_direct_install() {
        assert_eq!(Channel::detect_from(false, false), Channel::Direct);
    }

    #[test]
    fn store_builds_ask_before_collecting_anything() {
        for channel in [Channel::Flatpak, Channel::Snap] {
            assert_eq!(decide(channel, None), Decision::Ask);
        }
    }

    #[test]
    fn direct_installs_collect_and_say_so_once() {
        assert_eq!(
            decide(Channel::Direct, None),
            Decision::Collect { notice: true }
        );
    }

    #[test]
    fn a_saved_choice_is_honored_on_every_channel() {
        for channel in [Channel::Flatpak, Channel::Snap, Channel::Direct] {
            assert_eq!(
                decide(channel, Some(true)),
                Decision::Collect { notice: false }
            );
            assert_eq!(decide(channel, Some(false)), Decision::Off);
        }
    }

    #[test]
    fn channel_names_are_stable_report_keys() {
        assert_eq!(Channel::Flatpak.name(), "flatpak");
        assert_eq!(Channel::Snap.name(), "snap");
        assert_eq!(Channel::Direct.name(), "direct");
    }
}
