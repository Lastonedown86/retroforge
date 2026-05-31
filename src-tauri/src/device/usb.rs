/// Allwinner FEL USB identity.
pub const FEL_VID: u16 = 0x1f3a;
pub const FEL_PID: u16 = 0xefe8;

/// True when a USB VID/PID pair is the Allwinner FEL device.
pub fn is_fel_device(vid: u16, pid: u16) -> bool {
    vid == FEL_VID && pid == FEL_PID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_fel_identity() {
        assert!(is_fel_device(0x1f3a, 0xefe8));
        assert!(!is_fel_device(0x1f3a, 0x0001));
        assert!(!is_fel_device(0x0000, 0xefe8));
    }
}
