//! The original process-global random generator of `nfs5.exe`.
//!
//! Every call site inlines the same step (e.g. `0x494977`, `0x499730`,
//! `0x49daf3`, `0x49e461`): `product = [0x5d102c] * [0x5d1028]` (32-bit
//! `imul`), `[0x655a0c] = product`, `[0x5d1028] = product & 0xffff`, and the
//! drawn word is `(product >> 8) & 0xffff`.
//!
//! The shipped `.data` has state `0xf874af01` and multiplier 1, so after the
//! first draw the word is constant. The runtime seeding is unknown
//! (`docs/re/vehicle.md` open questions).

/// State `[0x5d1028]` of the shipped image.
pub const SHIPPED_STATE: u32 = 0xf874_af01;
/// Multiplier `[0x5d102c]` of the shipped image.
pub const SHIPPED_MULTIPLIER: u32 = 1;

/// One step on the raw globals: updates `state` and returns the full product
/// (the value written to `[0x655a0c]`).
pub fn step(state: &mut u32, multiplier: u32) -> u32 {
    let product = multiplier.wrapping_mul(*state);
    *state = product & 0xffff;
    product
}

/// The drawn 16-bit word of a product.
pub fn word(product: u32) -> u32 {
    (product >> 8) & 0xffff
}

/// The three globals touched by a draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OriginalLcg {
    /// `[0x5d1028]`.
    pub state_5d1028: u32,
    /// `[0x5d102c]`.
    pub multiplier_5d102c: u32,
    /// `[0x655a0c]`: the last full product.
    pub product_655a0c: u32,
}

impl Default for OriginalLcg {
    /// The shipped `.data` values (`[0x655a0c]` is `.bss`).
    fn default() -> Self {
        Self {
            state_5d1028: SHIPPED_STATE,
            multiplier_5d102c: SHIPPED_MULTIPLIER,
            product_655a0c: 0,
        }
    }
}

impl OriginalLcg {
    /// Draw a word in `0..0x10000`.
    pub fn next_word(&mut self) -> u32 {
        self.product_655a0c = step(&mut self.state_5d1028, self.multiplier_5d102c);
        word(self.product_655a0c)
    }
}
