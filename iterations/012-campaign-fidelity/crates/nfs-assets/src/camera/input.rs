//! Camera keys of the race loop: the interface bit word of `Input_PollControllers`
//! `0x412e70` and the reads of `Race_MainLoop` `0x4152e0` through `0x4138d0`.
//!
//! Every 32 Hz poll stores one word in `0x5e954c[poll & 0x1f]`; bit `i` is set while the
//! binding of op `0x1ee + i` reports more than 0x40 (a keyboard key reports 0xff while held
//! and no Shift/Ctrl/Alt is down that the binding does not ask for, `Input_KeyGetter`
//! `0x411c20`). For every new poll, outside the pause menu, the main loop reads the look
//! keys as levels (`0x4138d0(bit, 0)`: held in this poll) and the camera cycle as an edge
//! (`0x4138d0(0x17, 1)`: set now, clear in the previous poll). Default keys `fe.txt`:
//! `I_LOOKBEHIND = key,SC_B`, `I_CYCLECAMERA = key,SC_C`; the other look keys are unbound.
//! Verified against the original run in Unicorn (`scripts/research/replay-look-input.py`,
//! test `tests/original_look_input.rs`).

use super::original::LookKeys;

/// Op of interface bit 0 (`MAINEXIT`; the interface slots start at `0x5e98e8`).
pub const INTERFACE_OP0: u16 = 0x1ee;
/// `I_LOOKBEHIND` (record at `0x5d35e0`), read into `g_LookBehind` `0x5e99b0`.
pub const OP_LOOK_BEHIND: u16 = 0x201;
/// `I_LOOKLEFT`, `g_LookLeft` `0x5e99b4`.
pub const OP_LOOK_LEFT: u16 = 0x202;
/// `I_LOOKRIGHT`, `g_LookRight` `0x5e99ac`.
pub const OP_LOOK_RIGHT: u16 = 0x203;
/// `I_LOOKFORWARD`, `g_LookForward` `0x5e99a8`.
pub const OP_LOOK_FORWARD: u16 = 0x204;
/// `I_CYCLECAMERA`, edge -> `Camera_Cycle(0)` `0x465a60`.
pub const OP_CYCLE_CAMERA: u16 = 0x205;

/// Interface bit of `op` in the poll word.
pub const fn interface_bit(op: u16) -> u32 {
    1 << (op - INTERFACE_OP0)
}

/// Held camera keys of one poll (after the modifier test of the Key getter).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CameraKeys {
    pub look_behind: bool,
    pub look_left: bool,
    pub look_right: bool,
    pub look_forward: bool,
    pub cycle_camera: bool,
}

impl CameraKeys {
    /// Keys as the original Key getter reads them: a held Shift, Ctrl or Alt makes every
    /// binding without that modifier (all the defaults) read as released.
    pub fn with_modifier(self, modifier_held: bool) -> Self {
        if modifier_held { Self::default() } else { self }
    }

    /// The camera bits of the poll word.
    pub fn interface_word(&self) -> u32 {
        [
            (self.look_behind, OP_LOOK_BEHIND),
            (self.look_left, OP_LOOK_LEFT),
            (self.look_right, OP_LOOK_RIGHT),
            (self.look_forward, OP_LOOK_FORWARD),
            (self.cycle_camera, OP_CYCLE_CAMERA),
        ]
        .into_iter()
        .filter(|(held, _)| *held)
        .fold(0, |w, (_, op)| w | interface_bit(op))
    }
}

/// `0x4138d0(bit, 0)` for the four look keys of `Race_MainLoop`.
pub fn look_keys(word: u32) -> LookKeys {
    let held = |op| word & interface_bit(op) != 0;
    LookKeys {
        forward_5e99a8: held(OP_LOOK_FORWARD),
        right_5e99ac: held(OP_LOOK_RIGHT),
        behind_5e99b0: held(OP_LOOK_BEHIND),
        left_5e99b4: held(OP_LOOK_LEFT),
    }
}

/// `0x4138d0(bit, 1)`: set in `word`, clear in `previous`.
pub fn pressed(previous: u32, word: u32, op: u16) -> bool {
    interface_bit(op) & !previous & word != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn look_behind_is_bit_0x13_and_cycle_0x17() {
        assert_eq!(interface_bit(OP_LOOK_BEHIND), 1 << 0x13);
        assert_eq!(interface_bit(OP_CYCLE_CAMERA), 1 << 0x17);
        let keys = CameraKeys { look_behind: true, ..Default::default() };
        assert!(look_keys(keys.interface_word()).behind_5e99b0);
        assert_eq!(look_keys(keys.with_modifier(true).interface_word()), LookKeys::default());
    }

    #[test]
    fn cycle_is_an_edge() {
        let w = CameraKeys { cycle_camera: true, ..Default::default() }.interface_word();
        assert!(pressed(0, w, OP_CYCLE_CAMERA));
        assert!(!pressed(w, w, OP_CYCLE_CAMERA));
    }
}
