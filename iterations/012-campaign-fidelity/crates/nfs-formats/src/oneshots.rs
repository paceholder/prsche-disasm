//! Phrase table `oneshots.txt` of the speech archive (`GameData/Speech/zzzxeng.viv`).
//!
//! `Speech_Load` (`0x4affd0`) numbers every quoted string of the file in order: phrase `i` is
//! the `i`-th quoted name. `#Phrase_k...` lines only label the groups (they are not parsed by
//! the game). Notes: `docs/re/audio.md` "race speech" in the research workspace.

/// One phrase: its bank file name and the `#` group it is listed under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phrase {
    pub name: String,
    pub group: String,
}

/// Reads the phrase list. Like the original, quotes pair up across the whole file.
pub fn parse_oneshots(text: &[u8]) -> Vec<Phrase> {
    let mut out = Vec::new();
    let mut group = String::new();
    let mut open: Option<usize> = None;
    let mut line_start = 0;
    for (i, &b) in text.iter().enumerate() {
        if b == b'"' {
            match open.take() {
                Some(start) => out.push(Phrase {
                    name: String::from_utf8_lossy(&text[start..i]).into_owned(),
                    group: group.clone(),
                }),
                None => open = Some(i + 1),
            }
        } else if b == b'\n' || i + 1 == text.len() {
            if open.is_none() {
                let line = String::from_utf8_lossy(&text[line_start..=i]);
                let line = line.trim();
                if let Some(g) = line.strip_prefix('#') {
                    group = g.trim().to_string();
                }
            }
            line_start = i + 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_quoted_names_in_order() {
        let t = b"#Phrase_kCountdown\r\n    \"SOR-a-3.bnk\"\r\n    \"SOR-a-2.bnk\"\r\n\r\n#Phrase_kBestLap\t\r\n    \"Lap-best.bnk\"\r\n    \"Lap-best.bnk\"\r\n";
        let p = parse_oneshots(t);
        let names: Vec<_> = p.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            ["SOR-a-3.bnk", "SOR-a-2.bnk", "Lap-best.bnk", "Lap-best.bnk"]
        );
        assert_eq!(p[0].group, "Phrase_kCountdown");
        assert_eq!(p[3].group, "Phrase_kBestLap");
    }
}
