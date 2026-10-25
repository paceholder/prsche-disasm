//! Which objects a car smashes, and which pieces start (`docs/re/track-objects.md`
//! section 5): the car-contact class of vtable slot +0x20 [VA 0x487290] with
//! the manager decision `0x4767d0` and the piece selection of `0x476380`.

use nfs_formats::animdefs::{AnimDefinition, PieceDef};

/// Result of `obj.slot(+0x20)(car velocity)` [VA 0x487290, 0x4767d0].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactClass {
    /// Already smashed (manager entry `+0x1c` set): no contact.
    Gone = 0,
    /// The smash starts now: the pass only records the contact.
    Smash = 1,
    /// Solid: cylinder or box response.
    Solid = 2,
    /// Drive-through: records the contact when `+0x35c > 2.0`.
    DriveThrough = 3,
}

/// Per-object smash state: whether the manager entry already holds an animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmashState {
    /// Manager entry present, `entry+0x1c == 0`.
    Intact,
    /// Smash started (`entry+0x1c != 0`); the object stays gone until the
    /// manager reset `0x476170` (race restart).
    Smashed,
}

/// The pieces `0x476380` starts for a car speed `speed` (length of the car
/// velocity): `n` = number of leading thresholds below `speed`; the tier is
/// `n - 1`; with `n == 0` a single piece (the first) starts. `None` when the
/// definition has no pieces (`def+0x18 == 0`).
pub fn smash_pieces(def: &AnimDefinition, speed: f32) -> Option<&[PieceDef]> {
    if def.tiers.is_empty() || def.tiers.iter().all(|t| t.pieces.is_empty()) {
        return None;
    }
    let mut n = 0;
    for tier in &def.tiers {
        if speed <= tier.speed {
            break;
        }
        n += 1;
    }
    match n {
        0 => Some(&def.tiers[0].pieces[..def.tiers[0].pieces.len().min(1)]),
        n => Some(&def.tiers[n - 1].pieces),
    }
}

/// Slot +0x20: class of an object for a contact at `speed` [VA 0x487290].
///
/// Evidence correction (playtest and video of the original, 2026-10-25): bushes
/// are drive-through. The decompile reads `drivethru` (`obj+0x24`) only on the
/// manager-less branch of 0x487290 (`2 + (obj+0x24 != 0)`) and the scene
/// manager is created unconditionally, so by the code alone bushes would be
/// solid; the code that makes them pass was not found (the manager init
/// `0x4772f0` special-cases `kSmackable`/`SIGN` entries and may mark others).
/// The animdefs flag therefore decides first (approximation A178): an object
/// with `drivethru` is class 3, which only records the contact (and not even
/// that below `+0x35c = 2.0`).
/// An intact object whose definition accepts the smash returns `Smash` and the
/// caller must switch it to `Smashed`.
pub fn contact_class(
    manager_present: bool,
    drive_through: bool,
    state: SmashState,
    def: Option<&AnimDefinition>,
    speed: f32,
) -> ContactClass {
    if drive_through {
        return ContactClass::DriveThrough;
    }
    if !manager_present {
        return ContactClass::Solid;
    }
    match state {
        SmashState::Smashed => ContactClass::Gone,
        SmashState::Intact => match def.and_then(|d| smash_pieces(d, speed)) {
            Some(_) => ContactClass::Smash,
            None => ContactClass::Solid,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfs_formats::animdefs::{AnimDefs, parse_animdefs};

    fn defs() -> AnimDefs {
        parse_animdefs(
            "SECTION:ANIM { tag=BBOX; collideType=kBox; speed=0; \
             SECTION:PIECE { playbackrate=5; QuatGroup=BX01; } \
             SECTION:PIECE { playbackrate=5; QuatGroup=BX02; } }\n\
             SECTION:ANIM { tag=TEMP; collideType=kCylinder; speed=0; \
             SECTION:PIECE { playbackrate=3; QuatGroup=SIGN; } speed=5; \
             SECTION:PIECE { playbackrate=8; QuatGroup=SIGN; } \
             SECTION:PIECE { playbackrate=9; QuatGroup=SIGN; } }\n\
             SECTION:ANIM { tag=STRE; collideType=kCylinder; }",
        )
        .unwrap()
    }

    #[test]
    fn crate_pieces_start_at_any_speed_above_zero() {
        let d = defs();
        let bbox = &d.definitions[0];
        assert_eq!(smash_pieces(bbox, 3.0).unwrap().len(), 2);
        assert_eq!(smash_pieces(bbox, 0.0).unwrap().len(), 1);
    }

    #[test]
    fn speed_tiers_select_the_last_threshold_below_the_speed() {
        let d = defs();
        let temp = &d.definitions[1];
        assert_eq!(smash_pieces(temp, 3.0).unwrap().len(), 1);
        assert_eq!(smash_pieces(temp, 5.0).unwrap().len(), 1);
        assert_eq!(smash_pieces(temp, 5.1).unwrap().len(), 2);
    }

    #[test]
    fn classes() {
        let d = defs();
        let bbox = &d.definitions[0];
        let tree = &d.definitions[2];
        let c = |mgr, thru, st, def, v| contact_class(mgr, thru, st, def, v);
        assert_eq!(
            c(true, false, SmashState::Intact, Some(bbox), 4.0),
            ContactClass::Smash
        );
        assert_eq!(
            c(true, false, SmashState::Smashed, Some(bbox), 4.0),
            ContactClass::Gone
        );
        assert_eq!(
            c(true, false, SmashState::Intact, Some(tree), 4.0),
            ContactClass::Solid
        );
        // The drive-through flag (bushes) decides first (A178).
        assert_eq!(
            c(true, true, SmashState::Intact, Some(tree), 4.0),
            ContactClass::DriveThrough
        );
        assert_eq!(
            c(false, true, SmashState::Intact, None, 4.0),
            ContactClass::DriveThrough
        );
    }
}
