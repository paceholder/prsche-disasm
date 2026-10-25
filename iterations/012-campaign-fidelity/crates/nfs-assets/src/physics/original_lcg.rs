//! The original process-global random generator of `nfs5.exe`.
//!
//! Every call site inlines the same step (e.g. `0x494977`, `0x499730`,
//! `0x49daf3`, `0x49e461`): `product = [0x5d102c] * [0x5d1028]` (32-bit
//! `imul`), `[0x655a0c] = product`, `[0x5d1028] = product & 0xffff`, and the
//! drawn word is `(product >> 8) & 0xffff`.
//!
//! The shipped `.data` has state `0xf874af01` and multiplier 1, so after the
//! first draw the word is constant. The race init `0x414eb0` seeds it with
//! `Rand_Seed` `0x4a4780` from the `RAND_SEED` option `[0x657a34]`
//! ([`OriginalLcg::seeded`]).

/// State `[0x5d1028]` of the shipped image.
pub const SHIPPED_STATE: u32 = 0xf874_af01;
/// Multiplier `[0x5d102c]` of the shipped image.
pub const SHIPPED_MULTIPLIER: u32 = 1;

/// `Rand_Seed` `0x4a4780` constants: multiplier and start state.
pub const SEED_MULTIPLIER: u32 = 0x075b_cd15;
pub const SEED_STATE: u32 = 0x3ade_68b1;
/// `rand_seed` of the installed `fe.txt` (`RAND_SEED` `[0x657a34]`); the
/// front end's writer of the option is not located (approximation A202).
pub const FE_RAND_SEED: u32 = 8165;

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
    /// `Rand_Seed` `0x4a4780`: multiplier 123456789, state `0x3ade68b1`, then
    /// `seed % 500` steps (the last product stays in `[0x655a0c]`). The race
    /// init `0x414eb0` calls it with `[0x657a34]`.
    pub fn seeded(seed: u32) -> Self {
        let mut lcg = Self {
            state_5d1028: SEED_STATE,
            multiplier_5d102c: SEED_MULTIPLIER,
            product_655a0c: 0,
        };
        for _ in 0..seed % 500 {
            lcg.product_655a0c = step(&mut lcg.state_5d1028, lcg.multiplier_5d102c);
        }
        lcg
    }

    /// The generator after the race init `0x414eb0` with the `fe.txt` seed.
    pub fn race_start() -> Self {
        Self::seeded(FE_RAND_SEED)
    }

    /// Draw a word in `0..0x10000`.
    pub fn next_word(&mut self) -> u32 {
        self.product_655a0c = step(&mut self.state_5d1028, self.multiplier_5d102c);
        word(self.product_655a0c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rand_seed_matches_0x4a4780() {
        // seed % 500 == 0: only the constants.
        let l = OriginalLcg::seeded(500);
        assert_eq!(
            (l.state_5d1028, l.multiplier_5d102c, l.product_655a0c),
            (SEED_STATE, SEED_MULTIPLIER, 0)
        );
        // One step: the full 32-bit start state times the multiplier.
        let l = OriginalLcg::seeded(1);
        let p = SEED_STATE.wrapping_mul(SEED_MULTIPLIER);
        assert_eq!((l.state_5d1028, l.product_655a0c), (p & 0xffff, p));
        // fe.txt seed 8165: 165 steps.
        let mut s = SEED_STATE;
        for _ in 0..165 {
            s = s.wrapping_mul(SEED_MULTIPLIER) & 0xffff;
        }
        assert_eq!(OriginalLcg::race_start().state_5d1028, s);
    }

    #[test]
    fn seeded_draws_vary() {
        // The shipped image repeats one word; the seeded generator does not.
        let mut shipped = OriginalLcg::default();
        shipped.next_word();
        assert_eq!(shipped.next_word(), shipped.next_word());
        let mut lcg = OriginalLcg::race_start();
        let words: std::collections::HashSet<u32> = (0..256).map(|_| lcg.next_word()).collect();
        assert!(words.len() > 200, "{} distinct words", words.len());
    }

    #[test]
    fn misfire_chance_follows_the_engine_damage() {
        // Car_EngineDriveForce 0x49e440 misfires when damage/300 > word/65536. With the
        // shipped image the word is 0xaf after one draw, so any damage above 0.8 misfired on
        // every step and the engine died after the first car hit (playtest 2026-10-25).
        let rate = |mut lcg: OriginalLcg, damage: f64| {
            let n = 4096;
            (0..n)
                .filter(|_| damage / 300.0 > lcg.next_word() as f64 / 65536.0)
                .count() as f64
                / n as f64
        };
        assert!(rate(OriginalLcg::default(), 1.0) > 0.99);
        for damage in [1.0, 30.0, 100.0] {
            let r = rate(OriginalLcg::race_start(), damage);
            assert!(
                (r - damage / 300.0).abs() < 0.03,
                "damage {damage}: rate {r}"
            );
        }
    }
}
