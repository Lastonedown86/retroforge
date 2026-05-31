use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::RfError;

pub const HMOD_URL: &str = "https://hakchi.net/hakchi/hmods/hakchi-latest.hmod";
pub const HMOD_FILENAME: &str = "hakchi-latest.hmod";

/// Path to the cached hmod inside `cache_dir`.
pub fn cache_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(HMOD_FILENAME)
}

/// Ensure the hmod is cached locally, downloading it once if absent. Returns the
/// cached file path.
pub fn ensure_hmod(cache_dir: &Path) -> Result<PathBuf, RfError> {
    let path = cache_path(cache_dir);
    if path.exists() {
        return Ok(path);
    }
    let bytes = download(HMOD_URL)?;
    fs::create_dir_all(cache_dir).map_err(|e| RfError::HmodFetchFailed(e.to_string()))?;
    fs::write(&path, &bytes).map_err(|e| RfError::HmodFetchFailed(e.to_string()))?;
    Ok(path)
}

fn download(url: &str) -> Result<Vec<u8>, RfError> {
    // Bounded timeouts so a stalled connection fails the worker (and surfaces a
    // Failed progress event) instead of hanging on "Fetching payload…" forever.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(120))
        .build();
    let resp = agent
        .get(url)
        .call()
        .map_err(|e| RfError::HmodFetchFailed(e.to_string()))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| RfError::HmodFetchFailed(e.to_string()))?;
    Ok(buf)
}

/// Extract a single entry (by path, e.g. "boot/uboot.bin") from an hmod archive.
/// Handles a gzip-compressed tar or a plain tar (sniffed by magic).
pub fn extract_entry(archive: &[u8], name: &str) -> Result<Vec<u8>, RfError> {
    let tar_bytes = if archive.starts_with(&[0x1f, 0x8b]) {
        let mut d = flate2::read::GzDecoder::new(archive);
        let mut v = Vec::new();
        d.read_to_end(&mut v)
            .map_err(|e| RfError::HmodExtractFailed(format!("gunzip: {e}")))?;
        v
    } else {
        archive.to_vec()
    };

    let mut tar = tar::Archive::new(&tar_bytes[..]);
    let entries = tar
        .entries()
        .map_err(|e| RfError::HmodExtractFailed(e.to_string()))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| RfError::HmodExtractFailed(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| RfError::HmodExtractFailed(e.to_string()))?
            .to_string_lossy()
            .trim_start_matches("./")
            .to_string();
        if path == name {
            let mut out = Vec::new();
            entry
                .read_to_end(&mut out)
                .map_err(|e| RfError::HmodExtractFailed(e.to_string()))?;
            return Ok(out);
        }
    }
    Err(RfError::HmodExtractFailed(format!("entry not found: {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tar() -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, data) in [("boot/uboot.bin", b"UBOOT" as &[u8]), ("boot/boot.img", b"IMG")] {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_cksum();
            builder.append_data(&mut header, name, data).unwrap();
        }
        builder.into_inner().unwrap()
    }

    #[test]
    fn extracts_named_entries_from_plain_tar() {
        let tar = make_tar();
        assert_eq!(extract_entry(&tar, "boot/uboot.bin").unwrap(), b"UBOOT");
        assert_eq!(extract_entry(&tar, "boot/boot.img").unwrap(), b"IMG");
    }

    #[test]
    fn extracts_from_gzipped_tar() {
        use flate2::write::GzEncoder;
        use std::io::Write;
        let tar = make_tar();
        let mut enc = GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&tar).unwrap();
        let gz = enc.finish().unwrap();
        assert_eq!(extract_entry(&gz, "boot/uboot.bin").unwrap(), b"UBOOT");
    }

    #[test]
    fn missing_entry_errors() {
        let tar = make_tar();
        let err = extract_entry(&tar, "boot/nope").unwrap_err();
        assert!(matches!(err, RfError::HmodExtractFailed(_)));
    }
}
