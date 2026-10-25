//! The racing line `[0x5e671c]` (`docs/re/ai.md` section 3): the file
//! `<track><REVERSE>.lsp` in `GameData/Track/racer/work/` (`RacingLine_New`
//! 0x4031f0), a u32 header and one `(x, z)` f32 pair per track node;
//! `RacingLine_Point` 0x403450 reads pair `node` (`+0x58` = data + 4,
//! `0x4033d0`).

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RacingLine {
    pub points: Vec<[f32; 2]>,
}

impl RacingLine {
    pub fn from_bytes(data: &[u8]) -> Self {
        let points = data
            .get(4..)
            .unwrap_or(&[])
            .as_chunks::<8>()
            .0
            .iter()
            .map(|c| {
                [
                    f32::from_le_bytes(c[0..4].try_into().unwrap()),
                    f32::from_le_bytes(c[4..8].try_into().unwrap()),
                ]
            })
            .collect();
        Self { points }
    }

    /// File name for a track and direction (`%s%s%d.lsp`, data 0x5cbda8).
    pub fn file_name(track: &str, reverse: bool) -> String {
        format!("{track}{}.lsp", reverse as u8)
    }

    /// `0x403450`: `(x, z)` of `node` (0 outside the file).
    pub fn point(&self, node: i32) -> [f32; 2] {
        self.points.get(node as usize).copied().unwrap_or([0.0; 2])
    }
}
