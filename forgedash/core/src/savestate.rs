//! Pure RetroArch save-state slot model + path math. No I/O.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SlotKind {
    Auto,
    Manual(u8),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SaveSlot {
    pub kind: SlotKind,
    pub exists: bool,
    pub state_path: String,
    pub thumb_path: String,
}

/// Split a ROM path into (dir-with-trailing-slash, stem-without-extension).
fn split_dir_stem(rom_path: &str) -> (String, String) {
    let (dir, file) = match rom_path.rfind('/') {
        Some(i) => (&rom_path[..=i], &rom_path[i + 1..]),
        None => ("", rom_path),
    };
    let stem = match file.rfind('.') {
        Some(j) => &file[..j],
        None => file,
    };
    (dir.to_string(), stem.to_string())
}

/// Path of the auto-save for a ROM (`<dir><stem>.state.auto`).
pub fn auto_path(rom_path: &str) -> String {
    let (d, s) = split_dir_stem(rom_path);
    format!("{d}{s}.state.auto")
}

/// Path of a given slot's state file.
pub fn state_path(rom_path: &str, kind: SlotKind) -> String {
    let (d, s) = split_dir_stem(rom_path);
    match kind {
        SlotKind::Auto => format!("{d}{s}.state.auto"),
        SlotKind::Manual(0) => format!("{d}{s}.state"),
        SlotKind::Manual(n) => format!("{d}{s}.state{n}"),
    }
}

/// Thumbnail path for a state file (`<state>.png`).
pub fn thumb_path(state_path: &str) -> String {
    format!("{state_path}.png")
}

/// Build the ordered slot list for a ROM, given the file names in its directory.
/// Always includes Auto and Slot 0 (even if absent), then Manual(1..=max existing).
pub fn slots_for(rom_path: &str, dir_listing: &[String]) -> Vec<SaveSlot> {
    let (_d, stem) = split_dir_stem(rom_path);
    let has = |path: &str| {
        let fname = path.rsplit('/').next().unwrap_or(path);
        dir_listing.iter().any(|f| f == fname)
    };
    let mk = |kind: SlotKind| {
        let sp = state_path(rom_path, kind);
        SaveSlot {
            kind,
            exists: has(&sp),
            thumb_path: thumb_path(&sp),
            state_path: sp,
        }
    };

    let mut out = vec![mk(SlotKind::Auto), mk(SlotKind::Manual(0))];

    let prefix = format!("{stem}.state");
    let mut max_n = 0u8;
    for f in dir_listing {
        if let Some(rest) = f.strip_prefix(&prefix) {
            if let Ok(n) = rest.parse::<u8>() {
                if n > max_n {
                    max_n = n;
                }
            }
        }
    }
    for n in 1..=max_n {
        out.push(mk(SlotKind::Manual(n)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_helpers() {
        assert_eq!(auto_path("roms/g1.nes"), "roms/g1.state.auto");
        assert_eq!(state_path("roms/g1.nes", SlotKind::Manual(0)), "roms/g1.state");
        assert_eq!(state_path("roms/g1.nes", SlotKind::Manual(2)), "roms/g1.state2");
        assert_eq!(state_path("roms/g1.nes", SlotKind::Auto), "roms/g1.state.auto");
        assert_eq!(thumb_path("roms/g1.state"), "roms/g1.state.png");
        assert_eq!(state_path("g1.nes", SlotKind::Manual(0)), "g1.state");
    }

    #[test]
    fn slots_for_parses_listing_and_ignores_noise() {
        let listing: Vec<String> = [
            "g1.nes", "g1.state", "g1.state.auto", "g1.state1",
            "g1.srm", "g1.state1.png", "g2.state",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let slots = slots_for("roms/g1.nes", &listing);

        assert_eq!(slots.len(), 3);
        assert_eq!(slots[0].kind, SlotKind::Auto);
        assert!(slots[0].exists);
        assert_eq!(slots[0].state_path, "roms/g1.state.auto");
        assert_eq!(slots[1].kind, SlotKind::Manual(0));
        assert!(slots[1].exists);
        assert_eq!(slots[2].kind, SlotKind::Manual(1));
        assert!(slots[2].exists);
        assert_eq!(slots[2].thumb_path, "roms/g1.state1.png");
    }

    #[test]
    fn slots_for_empty_still_has_auto_and_slot0() {
        let listing = vec!["g1.nes".to_string()];
        let slots = slots_for("roms/g1.nes", &listing);
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].kind, SlotKind::Auto);
        assert!(!slots[0].exists);
        assert_eq!(slots[1].kind, SlotKind::Manual(0));
        assert!(!slots[1].exists);
    }
}
