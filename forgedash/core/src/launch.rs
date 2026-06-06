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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_line_is_tab_separated() {
        assert_eq!(handoff_line("roms/smb3.nes", "fceumm"), "fceumm\troms/smb3.nes\n");
    }
}
