use std::fs;
use std::path::Path;

use crate::device::hmod;
use crate::error::RfError;

/// The Allwinner DRAM-init blob, bundled (small, redistributable boot tooling).
#[allow(dead_code)] // consumed by memboot command in a later task
const FES1: &[u8] = include_bytes!("../../resources/fes1.bin");

/// The bundled fes1 DRAM-init blob.
#[allow(dead_code)] // consumed by memboot command in a later task
pub fn fes1() -> &'static [u8] {
    FES1
}

/// Ensure the runtime blobs (inside hakchi-latest.hmod) are downloaded/cached.
#[allow(dead_code)] // consumed by memboot command in the next task
pub fn ensure_blobs(cache_dir: &Path) -> Result<(), RfError> {
    hmod::ensure_hmod(cache_dir)?;
    Ok(())
}

#[allow(dead_code)] // consumed by memboot command in the next task
fn read_hmod(cache_dir: &Path) -> Result<Vec<u8>, RfError> {
    let path = hmod::cache_path(cache_dir);
    fs::read(&path).map_err(|_| {
        RfError::BlobMissing("hmod not cached; run ensure_blobs first".into())
    })
}

/// The U-Boot binary extracted from the cached hmod.
#[allow(dead_code)] // consumed by memboot command in the next task
pub fn uboot(cache_dir: &Path) -> Result<Vec<u8>, RfError> {
    hmod::extract_entry(&read_hmod(cache_dir)?, "boot/uboot.bin")
}

/// The boot image extracted from the cached hmod.
#[allow(dead_code)] // consumed by memboot command in the next task
pub fn boot_img(cache_dir: &Path) -> Result<Vec<u8>, RfError> {
    hmod::extract_entry(&read_hmod(cache_dir)?, "boot/boot.img")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fes1_is_present_and_nontrivial() {
        assert!(fes1().len() > 1024, "fes1 blob looks too small");
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn uboot_errors_when_hmod_absent() {
        let dir = std::env::temp_dir().join("retroforge-blobs-test-absent");
        let _ = std::fs::remove_dir_all(&dir);
        let err = uboot(&dir).unwrap_err();
        assert!(matches!(err, RfError::BlobMissing(_)));
    }
}
