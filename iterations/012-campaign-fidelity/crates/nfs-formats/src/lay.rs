//! Front-end layout (`FEData/Layouts/*.lay`): an INI file of `[section.N]` blocks.
//!
//! Section 0 describes the screen (`screen=`, `room=`, `callbacklist=`, `musicfile=`,
//! `canplaygame=`); every other section is one control (`id=NFS5.<TYPE>` plus attributes).
//! Sections are kept in file order, which is the order the original creates (and draws) the
//! controls in. Keys keep their spelling; lookups ignore ASCII case (`BLoffy` / `BLoffX`).
//! Format notes: `docs/formats/lay.md` in the workspace root.
use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaySection {
    /// `N` of `[section.N]`.
    pub index: u32,
    /// `key=value` pairs in file order; values are trimmed, may be empty.
    pub attrs: Vec<(String, String)>,
}

impl LaySection {
    /// Value of `key` (ASCII case-insensitive); the last occurrence wins.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .rev()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
    /// Non-empty value of `key`.
    pub fn value(&self, key: &str) -> Option<&str> {
        self.get(key).filter(|v| !v.is_empty())
    }
    /// Control type: `id=NFS5.PICTURE` -> `PICTURE`, `REAL.BLACK_640x480` -> `BLACK_640x480`.
    pub fn control_type(&self) -> Option<&str> {
        let id = self.value("id")?;
        Some(id.split_once('.').map_or(id, |(_, t)| t))
    }
    pub fn int(&self, key: &str) -> Option<i64> {
        parse_int(self.value(key)?)
    }
    /// `pos=x,y` / `size=w,h`.
    pub fn pair(&self, key: &str) -> Option<(i32, i32)> {
        let (a, b) = self.value(key)?.split_once(',')?;
        Some((parse_int(a)? as i32, parse_int(b)? as i32))
    }
    /// `0xAARRGGBB` colour.
    pub fn color(&self, key: &str) -> Option<u32> {
        self.int(key).map(|v| v as u32)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub sections: Vec<LaySection>,
}

impl Layout {
    /// Section 0 (the screen description), if present.
    pub fn screen(&self) -> Option<&LaySection> {
        self.sections.iter().find(|s| s.index == 0)
    }
    /// All sections except section 0, in file order.
    pub fn controls(&self) -> impl Iterator<Item = &LaySection> {
        self.sections.iter().filter(|s| s.index != 0)
    }
}

/// Decimal or `0x` hexadecimal integer, optionally signed.
pub fn parse_int(text: &str) -> Option<i64> {
    let t = text.trim();
    let (neg, t) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t),
    };
    let v = if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()?
    } else {
        t.parse::<i64>().ok()?
    };
    Some(if neg { -v } else { v })
}

pub fn parse_lay(input: &[u8]) -> Result<Layout> {
    // The shipped files are ASCII with CRLF; accept Latin-1 bytes rather than fail.
    let text: String = input.iter().map(|&b| b as char).collect();
    let mut sections: Vec<LaySection> = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with("//") {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let name = rest
                .strip_suffix(']')
                .ok_or_else(|| format!("lay line {}: unterminated section", n + 1))?;
            let index = name
                .trim()
                .to_ascii_lowercase()
                .strip_prefix("section.")
                .and_then(|i| i.parse::<u32>().ok())
                .ok_or_else(|| format!("lay line {}: expected [section.N]", n + 1))?;
            sections.push(LaySection {
                index,
                attrs: Vec::new(),
            });
        } else if let Some((k, v)) = line.split_once('=') {
            let section = sections
                .last_mut()
                .ok_or_else(|| format!("lay line {}: key before first section", n + 1))?;
            section
                .attrs
                .push((k.trim().to_string(), v.trim().to_string()));
        } else {
            return Err(format!("lay line {}: expected section or key=value", n + 1));
        }
    }
    Ok(Layout { sections })
}

fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                out.push_str(&format!("\\u{:04x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `[{"index":N,"attrs":{"key":"value",...}}, ...]`; keys lower-cased, last occurrence wins.
pub fn layout_json(layout: &Layout) -> String {
    let mut out = String::from("[");
    for (i, s) in layout.sections.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"index\":{},\"attrs\":{{", s.index));
        let mut seen: Vec<String> = Vec::new();
        let mut first = true;
        for (k, _) in s.attrs.iter() {
            let key = k.to_ascii_lowercase();
            if seen.contains(&key) {
                continue;
            }
            if !first {
                out.push(',');
            }
            first = false;
            json_str(&mut out, &key);
            out.push(':');
            json_str(&mut out, s.get(&key).unwrap_or(""));
            seen.push(key);
        }
        out.push_str("}}");
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = b"[section.0]\r\nid=REAL.BLACK_640x480\r\nscreen=ID_MOVIEOVAL\r\nroom=ID_GARAGE\r\n\r\n[section.2]\r\nid=NFS5.PICTURE\r\npos=448,274\r\nflip=\r\nBLoffy=3\r\n\r\n[section.10]\r\nid=NFS5.GOTOBUTTON\r\nmaincolor=0xFFBC0000\r\nstemleftheight=-2\r\n";

    #[test]
    fn keeps_file_order_and_types() {
        let lay = parse_lay(SAMPLE).unwrap();
        let idx: Vec<u32> = lay.sections.iter().map(|s| s.index).collect();
        assert_eq!(idx, [0, 2, 10]);
        assert_eq!(lay.screen().unwrap().get("room"), Some("ID_GARAGE"));
        let pic = &lay.sections[1];
        assert_eq!(pic.control_type(), Some("PICTURE"));
        assert_eq!(pic.pair("pos"), Some((448, 274)));
        assert_eq!(pic.get("flip"), Some(""));
        assert_eq!(pic.value("flip"), None);
        assert_eq!(pic.int("bloffY"), Some(3));
        let button = &lay.sections[2];
        assert_eq!(button.color("maincolor"), Some(0xffbc0000));
        assert_eq!(button.int("stemleftheight"), Some(-2));
        assert_eq!(lay.controls().count(), 2);
    }

    #[test]
    fn json_round_trip_shape() {
        let json = layout_json(&parse_lay(SAMPLE).unwrap());
        assert!(json.starts_with("[{\"index\":0,\"attrs\":{\"id\":\"REAL.BLACK_640x480\""));
        assert!(json.contains("\"bloffy\":\"3\""));
        assert!(json.contains("\"maincolor\":\"0xFFBC0000\""));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_lay(b"pos=1,2\r\n").is_err());
        assert!(parse_lay(b"[section.x]\r\n").is_err());
        assert!(parse_lay(b"[section.0]\r\nnonsense\r\n").is_err());
    }

    #[test]
    fn installed_layouts_parse() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/FEData/Layouts");
        let Ok(rd) = std::fs::read_dir(&dir) else {
            eprintln!("skip: FEData/Layouts not installed");
            return;
        };
        let mut count = 0;
        for f in rd.flatten() {
            let name = f.file_name().to_string_lossy().to_ascii_lowercase();
            if !name.ends_with(".lay") {
                continue;
            }
            let lay = parse_lay(&std::fs::read(f.path()).unwrap())
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(lay.screen().is_some(), "{name}: no section 0");
            count += 1;
        }
        assert_eq!(count, 98);
        let main = parse_lay(&std::fs::read(dir.join("GameSetup.lay")).unwrap()).unwrap();
        let screen = main.screen().unwrap();
        assert_eq!(screen.get("screen"), Some("ID_MOVIEOVAL"));
        assert_eq!(screen.get("room"), Some("ID_GARAGE"));
        assert_eq!(screen.get("callbacklist"), Some("ID_MAINSCREEN"));
        let buttons: Vec<_> = main
            .controls()
            .filter(|s| s.control_type() == Some("GOTOBUTTON"))
            .collect();
        assert_eq!(buttons.len(), 8);
        assert_eq!(buttons[0].pair("pos"), Some((377, 71)));
        assert_eq!(buttons[0].get("destination"), Some("selectname"));
    }
}
