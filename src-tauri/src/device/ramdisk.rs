use crate::error::RfError;

const NEWC_MAGIC: &[u8; 6] = b"070701";
const HDR_LEN: usize = 110;

fn hex8(b: &[u8], off: usize) -> usize {
    let s = std::str::from_utf8(&b[off..off + 8]).unwrap_or("0");
    usize::from_str_radix(s, 16).unwrap_or(0)
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

/// A parsed newc entry view into the archive.
struct Entry<'a> {
    name: String,
    data: &'a [u8],
    start: usize,
    end: usize, // byte after this entry (next entry start)
}

/// Iterates newc entries up to (and including) TRAILER!!!.
struct CpioIter<'a> {
    buf: &'a [u8],
    pos: usize,
    done: bool,
}

impl<'a> CpioIter<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0, done: false }
    }
}

impl<'a> Iterator for CpioIter<'a> {
    type Item = Entry<'a>;
    fn next(&mut self) -> Option<Entry<'a>> {
        if self.done || self.pos + HDR_LEN > self.buf.len() {
            return None;
        }
        let h = &self.buf[self.pos..];
        if &h[0..6] != NEWC_MAGIC {
            return None;
        }
        let filesize = hex8(h, 54);
        let namesize = hex8(h, 94);
        let name_off = self.pos + HDR_LEN;
        let name_end = name_off + namesize;
        let name = String::from_utf8_lossy(&self.buf[name_off..name_end - 1]).to_string();
        let data_off = align4(name_end);
        let data_end = data_off + filesize;
        let next = align4(data_end);
        let ent = Entry {
            name: name.clone(),
            data: &self.buf[data_off..data_end],
            start: self.pos,
            end: next,
        };
        if name == "TRAILER!!!" {
            self.done = true;
        }
        self.pos = next;
        Some(ent)
    }
}

/// Replace the data of the file named `path`, re-emitting all other entries
/// byte-for-byte. Errors if `path` is absent.
pub fn cpio_replace(cpio: &[u8], path: &str, new_data: &[u8]) -> Result<Vec<u8>, RfError> {
    let mut out = Vec::with_capacity(cpio.len() + new_data.len());
    let mut found = false;
    for ent in CpioIter::new(cpio) {
        if ent.name == path {
            found = true;
            let name_end = ent.start + HDR_LEN + hex8(&cpio[ent.start..], 94);
            let header_and_name_end = align4(name_end);
            let mut head = cpio[ent.start..header_and_name_end].to_vec();
            let fs = format!("{:08x}", new_data.len());
            head[54..62].copy_from_slice(fs.as_bytes());
            out.extend_from_slice(&head);
            out.extend_from_slice(new_data);
            while out.len() % 4 != 0 {
                out.push(0);
            }
        } else {
            out.extend_from_slice(&cpio[ent.start..ent.end]);
        }
    }
    if !found {
        return Err(RfError::RamdiskError(format!("entry not found: {path}")));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal newc entry writer for test fixtures.
    fn entry(name: &str, data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        let f = |n: u32| format!("{n:08x}");
        v.extend_from_slice(b"070701");
        let fields = [
            1u32,            // ino
            0o100644,        // mode
            0,               // uid
            0,               // gid
            1,               // nlink
            0,               // mtime
            data.len() as u32, // filesize  (offset 54)
            0, 0, 0, 0,      // devmajor, devminor, rdevmajor, rdevminor
            (name.len() + 1) as u32, // namesize (offset 94)
            0,               // check
        ];
        for x in fields {
            v.extend_from_slice(f(x).as_bytes());
        }
        v.extend_from_slice(name.as_bytes());
        v.push(0);
        while v.len() % 4 != 0 {
            v.push(0);
        }
        v.extend_from_slice(data);
        while v.len() % 4 != 0 {
            v.push(0);
        }
        v
    }

    fn trailer() -> Vec<u8> {
        entry("TRAILER!!!", &[])
    }

    fn archive(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut v = Vec::new();
        for e in entries {
            v.extend_from_slice(e);
        }
        v.extend_from_slice(&trailer());
        v
    }

    #[test]
    fn cpio_replace_swaps_target_keeps_siblings() {
        let a = entry("etc/a.txt", b"AAAA");
        let target = entry("etc/boot.png", b"OLD");
        let c = entry("etc/c.txt", b"CCCC");
        let arc = archive(&[a.clone(), target, c.clone()]);
        let out = cpio_replace(&arc, "etc/boot.png", b"NEWLONGERPNGDATA").unwrap();
        assert_eq!(cpio_read(&out, "etc/boot.png").unwrap(), b"NEWLONGERPNGDATA");
        assert_eq!(cpio_read(&out, "etc/a.txt").unwrap(), b"AAAA");
        assert_eq!(cpio_read(&out, "etc/c.txt").unwrap(), b"CCCC");
    }

    #[test]
    fn cpio_replace_errs_when_missing() {
        let arc = archive(&[entry("x", b"1")]);
        assert!(matches!(
            cpio_replace(&arc, "nope", b"z"),
            Err(RfError::RamdiskError(_))
        ));
    }

    // Test-only reader used to verify replace output.
    fn cpio_read(cpio: &[u8], path: &str) -> Option<Vec<u8>> {
        for ent in CpioIter::new(cpio) {
            if ent.name == path {
                return Some(ent.data.to_vec());
            }
        }
        None
    }
}
