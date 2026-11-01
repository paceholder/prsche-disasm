//! Car texture tint: the colour words of a car (body, two more, interior) baked into its
//! texture pages by alpha code (`docs/re/cockpit.md` "Interior colour").
//!
//! `R3DCar` page fill (vtable +0x28, 0x443f60) copies every FSH image of a `.tpg` file into its
//! page; when the image's `tint` key is set (`fileN.details` then `fileN.xxxx`, default 1,
//! vtable +0x30 0x439ee0) it goes through 0x444130, which recolours a texel by its alpha:
//! `k = (a + 1) * 20 >> 8`; `k` 0, 10 and 20 keep the texel, any other `k` selects colour slot
//! `18 - k` (`k < 10`) or `19 - k`, and the texel becomes `slot.rgb * R >> 8` with alpha
//! `a != 0`. The 18 slots are the car's four colour words `res+0x218, +0x21c, +0x220, +0x224`
//! indexed by the decal package's bands (0x443db0; `packages.ini` read by 0x443760, bands 14
//! and 15 default to 3, the others to 0). `res+0x224` is the interior word (`COLORCONFIG`
//! element 3 = owned car `+0x678`, 0x4eeda0), so the interior tiles, whose alpha codes are
//! `k` 3 and 4, take the interior colour, and the `k` 16 tiles (dash) the body colour.
//!
//! Evidence: read from the decompile (0x443f60, 0x444130, 0x443db0, 0x443760), not recorded.

/// Colour slot of a texel alpha (0x444130), `None` for an untinted texel (`k` 0, 10, 20).
pub fn tint_slot(alpha: u8) -> Option<usize> {
    let k = ((u32::from(alpha) + 1) * 20) >> 8;
    match k {
        0 | 10 | 20 => None,
        k if k < 10 => Some(18 - k as usize),
        k => Some(19 - k as usize),
    }
}

/// Bands of decal package 1 (`packages.ini` `[package1]` is empty), the decal of every catalogue
/// car (`GenericCar` accessor slot 0x84 returns 0, 0x508e70): defaults of 0x443760, band 14 and
/// 15 = 3 (interior word), all others 0 (body word).
pub const PACKAGE1_BANDS: [u8; 18] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 3, 0, 0];

/// One texel of 0x444130's 32-bit path (`rgba` bytes; `colours` = the four colour words as
/// R, G, B bytes in `res+0x218` order: body, `+0x21c`, `+0x220`, interior).
pub fn tint_texel(rgba: [u8; 4], colours: &[[u8; 3]; 4], bands: &[u8; 18]) -> [u8; 4] {
    let Some(slot) = tint_slot(rgba[3]) else {
        return rgba;
    };
    let colour = colours[usize::from(bands[slot] & 3)];
    let r = u32::from(rgba[0]);
    let mul = |c: u8| ((u32::from(c) * r) >> 8) as u8;
    [mul(colour[0]), mul(colour[1]), mul(colour[2]), if rgba[3] != 0 { 255 } else { 0 }]
}

/// A page with the tint applied to the texels of tinted tiles (`mask`, one flag per texel).
pub fn tint_page(rgba: &[u8], mask: &[bool], colours: &[[u8; 3]; 4], bands: &[u8; 18]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    for (texel, _) in out.chunks_exact_mut(4).zip(mask).filter(|(_, &m)| m) {
        let t = tint_texel([texel[0], texel[1], texel[2], texel[3]], colours, bands);
        texel.copy_from_slice(&t);
    }
    out
}

/// Whether 0x444130 tints a 32-bit image on a page of this TPG `bpp` (`tpageN.details`, default
/// 1555 = 0x613): pages 32 (0x20) and 1555 do, 4444 (0x115c) and others take the plain blit.
pub fn page_tints(tpg_bpp: u32) -> bool {
    matches!(tpg_bpp, 0 | 32 | 1555)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_follow_the_alpha_code() {
        // k = (a + 1) * 20 >> 8
        assert_eq!(tint_slot(0), None); // k 0
        assert_eq!(tint_slot(11), None); // k 0
        assert_eq!(tint_slot(12), Some(17)); // k 1
        assert_eq!(tint_slot(38), Some(15)); // k 3 (interior tiles)
        assert_eq!(tint_slot(51), Some(14)); // k 4
        assert_eq!(tint_slot(127), None); // k 10
        assert_eq!(tint_slot(140), Some(8)); // k 11
        assert_eq!(tint_slot(204), Some(3)); // k 16 (body paint, dash)
        assert_eq!(tint_slot(243), Some(0)); // k 19
        assert_eq!(tint_slot(255), None); // k 20
        // every k except 0, 10, 20 has its own slot 0..17
        let mut seen: Vec<usize> = (0..=255u8).filter_map(tint_slot).collect();
        seen.dedup();
        assert_eq!(seen, (0..18).rev().collect::<Vec<_>>());
    }

    #[test]
    fn interior_texels_take_the_interior_word() {
        let colours = [[200, 10, 20], [1, 2, 3], [4, 5, 6], [0x58, 0x67, 0x82]];
        // k 3 -> slot 15 -> band 3 (interior); value = c * R >> 8, alpha 255
        assert_eq!(tint_texel([255, 0, 0, 38], &colours, &PACKAGE1_BANDS), [0x57, 0x66, 0x81, 255]);
        assert_eq!(tint_texel([128, 9, 9, 51], &colours, &PACKAGE1_BANDS), [44, 51, 65, 255]);
        // k 16 -> slot 3 -> band 0 (body)
        assert_eq!(tint_texel([255, 7, 7, 204], &colours, &PACKAGE1_BANDS), [199, 9, 19, 255]);
        // artwork (k 20), cut-out (k 0) and k 10 keep the texel
        for a in [255, 0, 127] {
            assert_eq!(tint_texel([1, 2, 3, a], &colours, &PACKAGE1_BANDS), [1, 2, 3, a]);
        }
    }

    #[test]
    fn untinted_tiles_keep_their_texels() {
        let colours = [[255, 0, 0]; 4];
        let page = [255, 255, 255, 38, 255, 255, 255, 38];
        assert_eq!(
            tint_page(&page, &[true, false], &colours, &PACKAGE1_BANDS),
            [254, 0, 0, 255, 255, 255, 255, 38]
        );
    }

    #[test]
    fn page_formats() {
        assert!(page_tints(0) && page_tints(32) && page_tints(1555));
        assert!(!page_tints(4444));
    }
}
