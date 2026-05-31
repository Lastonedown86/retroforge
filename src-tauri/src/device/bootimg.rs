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
}
