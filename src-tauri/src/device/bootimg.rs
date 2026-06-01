use crate::error::RfError;

// Android boot image header: the kernel command line is a NUL-terminated string
// in a 512-byte field at offset 64.
const CMDLINE_OFFSET: usize = 64;
const CMDLINE_SIZE: usize = 512;
const MAGIC: &[u8; 8] = b"ANDROID!";

/// Append ` {token}` to the kernel command line of an Android boot image,
/// returning a modified copy. Errors if the image is malformed or the cmdline
/// field has no room.
pub fn inject_cmdline(boot_img: &[u8], token: &str) -> Result<Vec<u8>, RfError> {
    if boot_img.len() < CMDLINE_OFFSET + CMDLINE_SIZE {
        return Err(RfError::ExecFailed(
            "boot image too small for an Android header".into(),
        ));
    }
    if &boot_img[0..8] != MAGIC {
        return Err(RfError::ExecFailed("not an Android boot image".into()));
    }
    let mut out = boot_img.to_vec();
    let field = &mut out[CMDLINE_OFFSET..CMDLINE_OFFSET + CMDLINE_SIZE];
    let used = field.iter().position(|&b| b == 0).unwrap_or(CMDLINE_SIZE);
    let addition = format!(" {token}");
    let add = addition.as_bytes();
    // Need room for the addition plus a terminating NUL.
    if used + add.len() + 1 > CMDLINE_SIZE {
        return Err(RfError::CmdlineTooLong);
    }
    field[used..used + add.len()].copy_from_slice(add);
    field[used + add.len()] = 0;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boot_img_with_cmdline(cmdline: &str) -> Vec<u8> {
        let mut v = vec![0u8; CMDLINE_OFFSET + CMDLINE_SIZE + 16];
        v[0..8].copy_from_slice(MAGIC);
        v[CMDLINE_OFFSET..CMDLINE_OFFSET + cmdline.len()].copy_from_slice(cmdline.as_bytes());
        v
    }

    fn read_cmdline(img: &[u8]) -> String {
        let field = &img[CMDLINE_OFFSET..CMDLINE_OFFSET + CMDLINE_SIZE];
        let end = field.iter().position(|&b| b == 0).unwrap_or(CMDLINE_SIZE);
        String::from_utf8_lossy(&field[..end]).to_string()
    }

    #[test]
    fn appends_token_after_existing_cmdline() {
        let img = boot_img_with_cmdline("console=ttyS0");
        let out = inject_cmdline(&img, "hakchi-clovershell").unwrap();
        assert_eq!(read_cmdline(&out), "console=ttyS0 hakchi-clovershell");
    }

    #[test]
    fn appends_to_empty_cmdline() {
        let img = boot_img_with_cmdline("");
        let out = inject_cmdline(&img, "x").unwrap();
        assert_eq!(read_cmdline(&out), " x");
    }

    #[test]
    fn errors_when_field_full() {
        let img = boot_img_with_cmdline(&"a".repeat(CMDLINE_SIZE - 2));
        let err = inject_cmdline(&img, "toolong").unwrap_err();
        assert!(matches!(err, RfError::CmdlineTooLong));
    }

    #[test]
    fn rejects_non_android_image() {
        let mut img = boot_img_with_cmdline("x");
        img[0] = b'X';
        assert!(inject_cmdline(&img, "y").is_err());
    }

    #[test]
    fn rejects_too_small() {
        assert!(inject_cmdline(&[0u8; 8], "y").is_err());
    }

    // Hardware-adjacent check: run the REAL extract + inject against the cached
    // hakchi-latest.hmod boot/boot.img, proving injection is byte-correct on the
    // real Android image (not just a synthetic header). Kills hypothesis H3:
    // "inject_cmdline corrupts the real boot image". CI-safe — skips if the hmod
    // is not cached. Run with:
    //   set RF_HMOD_PATH=%LOCALAPPDATA%\com.retroforge.app\hakchi-latest.hmod
    //   cargo test --lib inject_real_boot_img -- --ignored --nocapture
    #[test]
    #[ignore = "needs the machine-local cached hmod; set RF_HMOD_PATH"]
    fn inject_real_boot_img_is_byte_correct() {
        let path = std::env::var("RF_HMOD_PATH").expect("set RF_HMOD_PATH to the cached hmod");
        let archive = std::fs::read(&path).expect("read cached hmod");
        let orig = crate::device::hmod::extract_entry(&archive, "boot/boot.img")
            .expect("extract boot/boot.img");

        let rd_u32 = |off: usize| {
            u32::from_le_bytes([
                orig[off],
                orig[off + 1],
                orig[off + 2],
                orig[off + 3],
            ])
        };
        // Standard Android boot_img_hdr v0 layout.
        eprintln!(
            "[H3] boot.img={} bytes  kernel={}  ramdisk={}  second={}  page={}",
            orig.len(),
            rd_u32(8),
            rd_u32(16),
            rd_u32(24),
            rd_u32(36),
        );
        let orig_end = orig[CMDLINE_OFFSET..CMDLINE_OFFSET + CMDLINE_SIZE]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(CMDLINE_SIZE);
        let orig_cmdline =
            String::from_utf8_lossy(&orig[CMDLINE_OFFSET..CMDLINE_OFFSET + orig_end]).to_string();
        eprintln!("[H3] orig cmdline ({orig_end}B): {orig_cmdline:?}");

        let out = inject_cmdline(&orig, "hakchi-clovershell").expect("inject must succeed");

        // 1. Same length — inject returns a same-size copy, never re-sizes.
        assert_eq!(out.len(), orig.len(), "image size changed");
        // 2. Header before the cmdline field is untouched (magic, all sizes/addrs).
        assert_eq!(
            &out[..CMDLINE_OFFSET],
            &orig[..CMDLINE_OFFSET],
            "header before cmdline changed"
        );
        // 3. Everything AFTER the 512B cmdline field is untouched — crucially the
        //    id[8] SHA at offset 576 and any extra_cmdline. Hakchi edits cmdline
        //    with no re-hash; if our write spilled past the field this would fail.
        assert_eq!(
            &out[CMDLINE_OFFSET + CMDLINE_SIZE..],
            &orig[CMDLINE_OFFSET + CMDLINE_SIZE..],
            "bytes past the cmdline field changed (id SHA / extra_cmdline)"
        );
        // 4. The booted kernel will read exactly this — orig + appended token.
        let new_end = out[CMDLINE_OFFSET..CMDLINE_OFFSET + CMDLINE_SIZE]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(CMDLINE_SIZE);
        let new_cmdline =
            String::from_utf8_lossy(&out[CMDLINE_OFFSET..CMDLINE_OFFSET + new_end]).to_string();
        eprintln!("[H3] new  cmdline ({new_end}B): {new_cmdline:?}");
        assert_eq!(new_cmdline, format!("{orig_cmdline} hakchi-clovershell"));
        // 5. The token the init script greps for is present.
        assert!(new_cmdline.contains("hakchi-clovershell"));
    }
}
