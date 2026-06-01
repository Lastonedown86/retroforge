use crate::error::RfError;

const MAGIC: &[u8; 8] = b"ANDROID!";

fn rd_u32(img: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([img[off], img[off + 1], img[off + 2], img[off + 3]])
}

fn pages(n: usize, page: usize) -> usize {
    n.div_ceil(page)
}

struct Layout {
    page: usize,
    kernel_off: usize,
    kernel_size: usize,
    ramdisk_off: usize,
    ramdisk_size: usize,
}

fn parse(img: &[u8]) -> Result<Layout, RfError> {
    if img.len() < 40 || &img[0..8] != MAGIC {
        return Err(RfError::ExecFailed("not an Android boot image".into()));
    }
    let page = rd_u32(img, 36) as usize;
    if page == 0 {
        return Err(RfError::ExecFailed("zero page size".into()));
    }
    let kernel_size = rd_u32(img, 8) as usize;
    let ramdisk_size = rd_u32(img, 16) as usize;
    let kernel_off = page;
    let ramdisk_off = kernel_off + pages(kernel_size, page) * page;
    if ramdisk_off + ramdisk_size > img.len() {
        return Err(RfError::ExecFailed("truncated boot image".into()));
    }
    Ok(Layout {
        page,
        kernel_off,
        kernel_size,
        ramdisk_off,
        ramdisk_size,
    })
}

/// The compressed ramdisk bytes from an Android boot image.
pub fn extract_ramdisk(img: &[u8]) -> Result<Vec<u8>, RfError> {
    let l = parse(img)?;
    Ok(img[l.ramdisk_off..l.ramdisk_off + l.ramdisk_size].to_vec())
}

/// Return a new boot image with the ramdisk replaced, `ramdisk_size` patched,
/// and all regions page-padded. Kernel, header (except size), second area, and
/// any trailing bytes are preserved. The `id` SHA is intentionally left stale —
/// hakchi `boota` U-Boot command does not verify it.
pub fn replace_ramdisk(img: &[u8], new_ramdisk: &[u8]) -> Result<Vec<u8>, RfError> {
    let l = parse(img)?;
    let page = l.page;
    let second_off = l.ramdisk_off + pages(l.ramdisk_size, page) * page;

    let mut out = Vec::with_capacity(img.len() + new_ramdisk.len());
    out.extend_from_slice(&img[0..page]);
    out[16..20].copy_from_slice(&(new_ramdisk.len() as u32).to_le_bytes());
    out.extend_from_slice(&img[l.kernel_off..l.kernel_off + l.kernel_size]);
    pad_to_page(&mut out, page);
    out.extend_from_slice(new_ramdisk);
    pad_to_page(&mut out, page);
    if second_off < img.len() {
        out.extend_from_slice(&img[second_off..]);
    }
    Ok(out)
}

fn pad_to_page(buf: &mut Vec<u8>, page: usize) {
    let rem = buf.len() % page;
    if rem != 0 {
        buf.resize(buf.len() + (page - rem), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a synthetic Android image: page=2048, kernel=`k`, ramdisk=`r`.
    fn make_img(page: usize, kernel: &[u8], ramdisk: &[u8]) -> Vec<u8> {
        let mut img = vec![0u8; page]; // header page
        img[..8].copy_from_slice(MAGIC);
        img[8..12].copy_from_slice(&(kernel.len() as u32).to_le_bytes());
        img[16..20].copy_from_slice(&(ramdisk.len() as u32).to_le_bytes());
        img[36..40].copy_from_slice(&(page as u32).to_le_bytes());
        let mut k = kernel.to_vec();
        k.resize(pages(kernel.len(), page) * page, 0);
        let mut r = ramdisk.to_vec();
        r.resize(pages(ramdisk.len(), page) * page, 0);
        img.extend_from_slice(&k);
        img.extend_from_slice(&r);
        img
    }

    #[test]
    fn extract_ramdisk_returns_exact_bytes() {
        let img = make_img(2048, b"KERNELDATA", b"RAMDISKBYTES");
        assert_eq!(extract_ramdisk(&img).unwrap(), b"RAMDISKBYTES");
    }

    #[test]
    fn replace_ramdisk_updates_size_and_bytes_keeps_kernel() {
        let img = make_img(2048, b"KERNELDATA", b"OLDRAMDISK");
        let out = replace_ramdisk(&img, b"A_MUCH_LONGER_NEW_RAMDISK_PAYLOAD").unwrap();
        assert_eq!(
            rd_u32(&out, 16) as usize,
            b"A_MUCH_LONGER_NEW_RAMDISK_PAYLOAD".len()
        );
        assert_eq!(
            extract_ramdisk(&out).unwrap(),
            b"A_MUCH_LONGER_NEW_RAMDISK_PAYLOAD"
        );
        let page = 2048usize;
        assert_eq!(&out[page..page + b"KERNELDATA".len()], b"KERNELDATA");
    }

    #[test]
    fn rejects_non_android() {
        let mut img = make_img(2048, b"K", b"R");
        img[0] = b'X';
        assert!(extract_ramdisk(&img).is_err());
    }
}
