//! EA BIGF archive (`.viv`): big-endian header, then `offset, size, name\0` entries.
use crate::{bytes, Result};

pub const VIV_MAGIC: &[u8; 4] = b"BIGF";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VivEntry {
    pub name: String,
    pub offset: usize,
    pub size: usize,
}

fn u32be(d: &[u8], o: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(bytes(d, o, 4)?.try_into().unwrap()))
}

/// Lists the entries of a BIGF archive; every entry is bounds-checked against `d`.
pub fn parse_viv(d: &[u8]) -> Result<Vec<VivEntry>> {
    if bytes(d, 0, 4)? != VIV_MAGIC {
        return Err("not a BIGF archive".into());
    }
    let count = u32be(d, 8)? as usize;
    let mut p = 16;
    let mut entries = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let offset = u32be(d, p)? as usize;
        let size = u32be(d, p + 4)? as usize;
        p += 8;
        let tail = d.get(p..).ok_or("truncated name")?;
        let end = tail
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| format!("offset 0x{p:x}: unterminated name"))?;
        let name = tail[..end].iter().map(|&b| b as char).collect();
        p += end + 1;
        bytes(d, offset, size)?;
        entries.push(VivEntry { name, offset, size });
    }
    Ok(entries)
}

/// Returns the bytes of the first entry whose name equals `name` (ASCII case-insensitive).
pub fn viv_file<'a>(d: &'a [u8], entries: &[VivEntry], name: &str) -> Option<&'a [u8]> {
    entries
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case(name))
        .and_then(|e| d.get(e.offset..e.offset + e.size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let toc: usize = files.iter().map(|(n, _)| 8 + n.len() + 1).sum();
        let mut off = 16 + toc;
        let mut d = b"BIGF".to_vec();
        d.extend_from_slice(&0u32.to_be_bytes());
        d.extend_from_slice(&(files.len() as u32).to_be_bytes());
        d.extend_from_slice(&((16 + toc) as u32).to_be_bytes());
        for (name, data) in files {
            d.extend_from_slice(&(off as u32).to_be_bytes());
            d.extend_from_slice(&(data.len() as u32).to_be_bytes());
            d.extend_from_slice(name.as_bytes());
            d.push(0);
            off += data.len();
        }
        for (_, data) in files {
            d.extend_from_slice(data);
        }
        d
    }

    #[test]
    fn lists_and_reads_entries() {
        let d = archive(&[("a.bnk", b"xyz"), ("default.cfg", b"12")]);
        let e = parse_viv(&d).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(viv_file(&d, &e, "A.BNK"), Some(&b"xyz"[..]));
        assert_eq!(viv_file(&d, &e, "default.cfg"), Some(&b"12"[..]));
        assert_eq!(viv_file(&d, &e, "missing"), None);
    }

    #[test]
    fn rejects_bad_magic_and_out_of_range_entries() {
        assert!(parse_viv(b"BIGX\0\0\0\0\0\0\0\0\0\0\0\0").is_err());
        let mut d = archive(&[("a", b"xyz")]);
        let n = d.len();
        d.truncate(n - 1);
        assert!(parse_viv(&d).is_err());
    }
}
