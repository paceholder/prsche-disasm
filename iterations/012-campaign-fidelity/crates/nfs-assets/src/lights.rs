//! Car light groups: port of `R3DCar_UpdateLights` 0x439360.
//!
//! The original keeps two arrays per drawn car: "group lit" (`R3DCar+0x2d4`) and
//! "group level" (`R3DCar+0x2d8`). Glare sprites (`ef` records of the car CRP)
//! name a group; a glare is drawn only while its group is lit, at the group's
//! level. Group meanings are in `docs/re/lighting.md`.

/// Number of light groups the original uses (0..=13).
pub const LIGHT_GROUP_COUNT: usize = 14;

/// Headlight fade time after the switch changes, ms (`pDVar10[1] = 1000`).
pub const HEADLIGHT_FADE_MS: u32 = 1000;
/// Brake-group level for tail lights (headlights on, not braking): `0x3f19999a`.
pub const TAIL_LIGHT_LEVEL: f32 = 0.6;
/// Headlight level in the showroom: `0x3f4ccccd`.
pub const SHOWROOM_HEADLIGHT_LEVEL: f32 = 0.8;
/// Zone damage above which the zone's lights are broken.
pub const BROKEN_LIGHT_DAMAGE: f32 = 30.0;

/// What `R3DCar_UpdateLights` reads from the vehicle.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LightInputs {
    /// Effective brake pedal byte: `car+0xd80`, or `car+0xd7f` when `car+0xd98 != 0`.
    pub brake: u8,
    /// Gear `car+0xd82 == 0` (reverse).
    pub reverse_gear: bool,
    /// Light switch, `car+0xd87` bit 0.
    pub light_switch: bool,
    /// Indicator switches: `[0]` = `car+0xd87` bit 3 (groups 4, 6, 10),
    /// `[1]` = bit 4 (groups 5, 7, 11).
    pub indicators: [bool; 2],
    /// `car+0xb50 == 3`: all indicators blink.
    pub hazard: bool,
    /// Zone damage `car+0x4e4[0..9]`.
    pub damage: [f32; 9],
    /// Frontend/showroom scene (`0x65b928 != 0`).
    pub showroom: bool,
}

/// Linear fade like the original animator at `R3DCar+0x64` (`+0x110` start,
/// `+0x114` duration, `+0x118` from, `+0x11c` to).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Fade {
    start_ms: u32,
    duration_ms: u32,
    from: f32,
    to: f32,
}

impl Fade {
    /// Current value; a finished fade clears its duration (as the original does).
    fn value(&mut self, now_ms: u32) -> f32 {
        if self.duration_ms == 0 {
            return self.to;
        }
        let t = now_ms.wrapping_sub(self.start_ms) as f32 / self.duration_ms as f32;
        if t > 1.0 {
            self.duration_ms = 0;
            return self.to;
        }
        (self.to - self.from) * t + self.from
    }

    fn busy(&self, now_ms: u32) -> bool {
        now_ms.wrapping_sub(self.start_ms) < self.duration_ms
    }
}

/// Light state of one car.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightGroups {
    pub lit: [bool; LIGHT_GROUP_COUNT],
    pub level: [f32; LIGHT_GROUP_COUNT],
    /// Previous light switch (`R3DCar+0x567`).
    switch_state: bool,
    fade: Fade,
}

impl Default for LightGroups {
    fn default() -> Self {
        Self {
            lit: [false; LIGHT_GROUP_COUNT],
            // Hypothesis: levels start at 1; the race branch never writes the
            // indicator/reverse/glint levels, yet those glares are drawn.
            level: [1.0; LIGHT_GROUP_COUNT],
            switch_state: false,
            fade: Fade::default(),
        }
    }
}

/// Indicator blink phase at `seconds`: `(ticks / (0x5deb48 >> 1)) & 1`.
/// Hypothesis: `0x5deb48` is ticks per second, so the phase flips twice a second.
pub fn blink_phase(seconds: f64) -> bool {
    ((seconds.max(0.0) * 2.0) as u64) & 1 == 1
}

impl LightGroups {
    pub fn is_lit(&self, group: u8) -> bool {
        self.lit.get(group as usize).copied().unwrap_or(false)
    }

    pub fn level(&self, group: u8) -> f32 {
        self.level.get(group as usize).copied().unwrap_or(0.0)
    }

    /// One frame of `R3DCar_UpdateLights` 0x439360. `now_ms` is the tick clock
    /// (`GetTickCount`), `blink` the indicator phase ([`blink_phase`]).
    pub fn update(&mut self, inputs: &LightInputs, now_ms: u32, blink: bool) {
        if inputs.showroom {
            self.update_showroom();
            return;
        }
        let set = |s: &mut Self, group: usize, on: bool| s.lit[group] = on;
        let braking = inputs.brake != 0;
        set(self, 8, inputs.reverse_gear);
        set(self, 2, braking);
        set(self, 3, braking);
        let [mut ind0, mut ind1] = inputs.indicators;
        let front0 = if inputs.hazard {
            ind0 = true;
            ind1 = true;
            blink
        } else {
            ind0 && blink
        };
        set(self, 4, front0);
        set(self, 6, ind0 && !blink);
        set(self, 5, ind1 && blink);
        set(self, 7, ind1 && !blink);
        set(self, 10, if ind0 { blink } else { braking });
        set(self, 11, if ind1 { blink } else { braking });
        let switch = inputs.light_switch;
        set(self, 0, switch);
        set(self, 1, switch);
        set(self, 9, false);
        set(self, 12, true);
        set(self, 13, true);

        // Headlight fade (R3DCar+0x567, animator at R3DCar+0x64).
        const MAX_LEVEL: f32 = 1.0; // R3DCar+0x70; Hypothesis: 1.0.
        if self.switch_state != switch {
            self.switch_state = switch;
            if !self.fade.busy(now_ms) {
                let current = self.fade.value(now_ms);
                self.fade.from = current;
                self.fade.to = if switch { MAX_LEVEL } else { 0.0 };
                self.fade.start_ms = now_ms;
                self.fade.duration_ms = HEADLIGHT_FADE_MS;
            }
        }
        let headlight = self.fade.value(now_ms) / MAX_LEVEL;
        if !switch && headlight > 0.0 {
            set(self, 0, true);
            set(self, 1, true);
        }
        self.level[0] = headlight;
        self.level[1] = headlight;
        if !switch || braking {
            for group in [2, 3, 10, 11] {
                self.level[group] = 1.0;
            }
        } else {
            self.level[2] = TAIL_LIGHT_LEVEL;
            self.level[3] = TAIL_LIGHT_LEVEL;
            self.level[10] = if ind0 { 1.0 } else { TAIL_LIGHT_LEVEL };
            self.level[11] = if ind1 { 1.0 } else { TAIL_LIGHT_LEVEL };
            set(self, 2, true);
            set(self, 3, true);
            if !ind0 {
                set(self, 10, true);
            }
            if !ind1 {
                set(self, 11, true);
            }
        }
        self.apply_damage(&inputs.damage);
    }

    fn update_showroom(&mut self) {
        for (group, level) in [
            (0, SHOWROOM_HEADLIGHT_LEVEL),
            (1, SHOWROOM_HEADLIGHT_LEVEL),
            (2, TAIL_LIGHT_LEVEL),
            (3, TAIL_LIGHT_LEVEL),
            (10, TAIL_LIGHT_LEVEL),
            (11, TAIL_LIGHT_LEVEL),
            (8, 0.0),
        ] {
            self.lit[group] = true;
            self.level[group] = level;
        }
        self.lit[9] = true;
    }

    /// Broken lights: zone damage above 30 switches the zone's groups off.
    fn apply_damage(&mut self, damage: &[f32; 9]) {
        const ZONES: [(usize, &[usize]); 6] = [
            (0, &[0, 4]),
            (2, &[1, 5]),
            (4, &[3, 7, 11]),
            (6, &[2, 6, 10]),
            (5, &[8]),
            (8, &[12, 13]),
        ];
        for (zone, groups) in ZONES {
            if damage[zone] > BROKEN_LIGHT_DAMAGE {
                for &group in groups {
                    self.lit[group] = false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(inputs: LightInputs, now_ms: u32, blink: bool) -> LightGroups {
        let mut groups = LightGroups::default();
        groups.update(&inputs, now_ms, blink);
        groups
    }

    #[test]
    fn braking_lights_the_brake_groups_at_full_level() {
        let g = run(LightInputs { brake: 200, ..Default::default() }, 0, false);
        for group in [2, 3, 10, 11] {
            assert!(g.is_lit(group), "group {group}");
            assert_eq!(g.level(group), 1.0);
        }
        let g = run(LightInputs::default(), 0, false);
        for group in [2, 3, 10, 11] {
            assert!(!g.is_lit(group), "group {group}");
        }
    }

    #[test]
    fn tail_lights_with_headlights_on() {
        let mut g = LightGroups::default();
        let on = LightInputs { light_switch: true, ..Default::default() };
        g.update(&on, 0, false);
        for group in [2, 3, 10, 11] {
            assert!(g.is_lit(group));
            assert_eq!(g.level(group), TAIL_LIGHT_LEVEL);
        }
        g.update(&LightInputs { brake: 1, ..on }, 10, false);
        assert_eq!(g.level(2), 1.0);
    }

    #[test]
    fn headlights_fade_in_over_one_second() {
        let mut g = LightGroups::default();
        let on = LightInputs { light_switch: true, ..Default::default() };
        g.update(&on, 5000, false);
        assert!(g.is_lit(0) && g.is_lit(1));
        assert_eq!(g.level(0), 0.0);
        g.update(&on, 5500, false);
        assert!((g.level(0) - 0.5).abs() < 1e-6);
        g.update(&on, 6001, false);
        assert_eq!(g.level(1), 1.0);
        // Switching off fades out and keeps the groups lit meanwhile.
        g.update(&LightInputs::default(), 7000, false);
        assert!(g.is_lit(0));
        assert_eq!(g.level(0), 1.0);
        g.update(&LightInputs::default(), 7250, false);
        assert!(g.is_lit(0) && (g.level(0) - 0.75).abs() < 1e-6);
        g.update(&LightInputs::default(), 8100, false);
        assert!(!g.is_lit(0));
        assert_eq!(g.level(0), 0.0);
    }

    #[test]
    fn toggling_during_a_fade_is_ignored() {
        let mut g = LightGroups::default();
        let on = LightInputs { light_switch: true, ..Default::default() };
        g.update(&on, 0, false);
        g.update(&LightInputs::default(), 500, false);
        // The fade towards 1 continues; the groups stay lit while level > 0.
        assert!((g.level(0) - 0.5).abs() < 1e-6);
        assert!(g.is_lit(0));
    }

    #[test]
    fn indicators_blink_and_hazard_uses_all_four() {
        let left = LightInputs { indicators: [true, false], ..Default::default() };
        let g = run(left, 0, true);
        assert!(g.is_lit(4) && !g.is_lit(6) && g.is_lit(10));
        assert!(!g.is_lit(5) && !g.is_lit(7) && !g.is_lit(11));
        let g = run(left, 0, false);
        assert!(!g.is_lit(4) && g.is_lit(6) && !g.is_lit(10));
        let hazard = LightInputs { hazard: true, ..Default::default() };
        let g = run(hazard, 0, true);
        assert!(g.is_lit(4) && g.is_lit(5) && g.is_lit(10) && g.is_lit(11));
        assert!(!g.is_lit(6) && !g.is_lit(7));
    }

    #[test]
    fn indicator_side_overrides_the_combined_brake_light() {
        let g = run(
            LightInputs { brake: 255, indicators: [true, false], ..Default::default() },
            0,
            false,
        );
        assert!(!g.is_lit(10), "blinking side follows the phase");
        assert!(g.is_lit(11), "other side shows the brake");
    }

    #[test]
    fn reverse_glints_and_constant_groups() {
        let g = run(LightInputs { reverse_gear: true, ..Default::default() }, 0, false);
        assert!(g.is_lit(8));
        assert!(!g.is_lit(9));
        assert!(g.is_lit(12) && g.is_lit(13));
    }

    #[test]
    fn damage_breaks_the_zone_lights() {
        let mut damage = [0.0; 9];
        damage[6] = 31.0;
        let g = run(LightInputs { brake: 255, damage, ..Default::default() }, 0, false);
        assert!(!g.is_lit(2) && !g.is_lit(6) && !g.is_lit(10));
        assert!(g.is_lit(3) && g.is_lit(11));
        damage[6] = 30.0;
        let g = run(LightInputs { brake: 255, damage, ..Default::default() }, 0, false);
        assert!(g.is_lit(2));
    }

    #[test]
    fn showroom_shows_everything() {
        let g = run(LightInputs { showroom: true, ..Default::default() }, 0, false);
        assert!(g.is_lit(0) && g.is_lit(9) && g.is_lit(8));
        assert_eq!(g.level(0), SHOWROOM_HEADLIGHT_LEVEL);
        assert_eq!(g.level(2), TAIL_LIGHT_LEVEL);
        assert_eq!(g.level(8), 0.0);
    }

    #[test]
    fn blink_phase_flips_twice_a_second() {
        assert!(!blink_phase(0.1));
        assert!(blink_phase(0.6));
        assert!(!blink_phase(1.1));
    }
}
