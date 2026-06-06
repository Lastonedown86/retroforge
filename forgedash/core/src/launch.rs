use std::io;
use std::path::Path;

/// One handoff line consumed by forge-loop.sh: `<core>\t<rom_path>\n`.
pub fn handoff_line(rom_path: &str, core: &str) -> String {
    format!("{}\t{}\n", core, rom_path)
}

/// Write the handoff line to `<dir>/handoff`.
pub fn write_handoff(dir: &Path, rom_path: &str, core: &str) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("handoff"), handoff_line(rom_path, core))
}

/// RetroArch joypad button indices for a controller (from RA's udev autoconfig).
/// Defaults are the Nintendo Clovercon (NES Classic), verified on dp-nes.
pub struct PadMap {
    pub enable_hotkey: u8, // Select
    pub exit: u8,          // Start
    pub save: u8,          // A
    pub load: u8,          // B
    pub slot_inc: u8,      // D-pad Right
    pub slot_dec: u8,      // D-pad Left
}

impl PadMap {
    pub fn clovercon() -> PadMap {
        PadMap { enable_hotkey: 8, exit: 9, save: 0, load: 1, slot_inc: 12, slot_dec: 11 }
    }
}

/// Build the RetroArch override config that makes RA headless (no RGUI) and binds
/// in-game hotkeys (Select held = enable_hotkey). Appended via `--appendconfig`.
pub fn ra_override_cfg(pad: &PadMap) -> String {
    format!(
        // menu_driver "null" segfaults RetroArch 1.7.0 when starting content (HW-confirmed
        // on dp-nes); "rgui" runs, and the NES pad can't reach the menu-toggle (autoconf
        // maps it to "Home", absent on the pad) so RGUI is effectively invisible/unreachable.
        "input_joypad_driver = \"udev\"\n\
         input_driver = \"udev\"\n\
         menu_driver = \"rgui\"\n\
         fps_show = \"false\"\n\
         menu_enable_widgets = \"false\"\n\
         menu_show_load_content_animation = \"false\"\n\
         input_enable_hotkey_btn = \"{hk}\"\n\
         input_exit_emulator_btn = \"{exit}\"\n\
         input_save_state_btn = \"{save}\"\n\
         input_load_state_btn = \"{load}\"\n\
         input_state_slot_increase_btn = \"{inc}\"\n\
         input_state_slot_decrease_btn = \"{dec}\"\n",
        hk = pad.enable_hotkey,
        exit = pad.exit,
        save = pad.save,
        load = pad.load,
        inc = pad.slot_inc,
        dec = pad.slot_dec,
    )
}

/// Write `<dir>/ra-override.cfg` for forge-loop.sh to `--appendconfig`.
pub fn write_ra_override(dir: &Path, pad: &PadMap) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("ra-override.cfg"), ra_override_cfg(pad))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_line_is_tab_separated() {
        assert_eq!(handoff_line("roms/smb3.nes", "fceumm"), "fceumm\troms/smb3.nes\n");
    }

    #[test]
    fn override_has_headless_and_clovercon_hotkeys() {
        let c = ra_override_cfg(&PadMap::clovercon());
        assert!(c.contains("menu_driver = \"rgui\""));
        assert!(c.contains("input_joypad_driver = \"udev\""));
        assert!(c.contains("input_driver = \"udev\""));
        assert!(c.contains("fps_show = \"false\""));
        assert!(c.contains("input_enable_hotkey_btn = \"8\""));   // Select
        assert!(c.contains("input_exit_emulator_btn = \"9\""));   // Start
        assert!(c.contains("input_save_state_btn = \"0\""));      // A
        assert!(c.contains("input_load_state_btn = \"1\""));      // B
        assert!(c.contains("input_state_slot_increase_btn = \"12\"")); // D-pad Right
        assert!(c.contains("input_state_slot_decrease_btn = \"11\"")); // D-pad Left
    }
}
