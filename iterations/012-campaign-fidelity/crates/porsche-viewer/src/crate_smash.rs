//! Smashed crates: the intact meshes stop being drawn and the pieces of the
//! definition play the baked `Anqt` frames of their article groups
//! (`docs/re/track-objects.md` sections 5 and 6).
//!
//! Approximation A172: playback is 30 frames per second from frame 0 for every
//! piece; `BaseGroup`, `LANDFLAT`/`DIRECTED` and the velocity offset of
//! `SmackAnim` are not applied.

use glam::{Mat4, Quat, Vec3};
use nfs_assets::{PropArticle, Smashable};

/// Assumed playback rate of the baked frames (approximation A172).
pub const FRAMES_PER_SECOND: f32 = 30.0;

/// One drawable: model matrix and the range of `Scene::meshes` to draw.
#[derive(Clone, Debug)]
pub struct CrateDraw {
    pub model: Mat4,
    pub mesh_range: std::ops::Range<usize>,
}

#[derive(Clone, Debug)]
struct Running {
    started: f32,
    /// Indices into the prop articles whose meshes and frames play.
    pieces: Vec<usize>,
}

/// Per-crate state: intact, or playing since a time.
#[derive(Clone, Debug, Default)]
pub struct CrateSmash {
    running: Vec<Option<Running>>,
}

impl CrateSmash {
    pub fn new(crates: usize) -> Self {
        Self {
            running: vec![None; crates],
        }
    }

    /// Race restart: every crate is whole again (`0x476170`).
    pub fn reset(&mut self) {
        self.running.iter_mut().for_each(|r| *r = None);
    }

    /// Starts the smash of crate `index` at time `now` with the pieces the
    /// definition picks for the contact speed.
    pub fn smash(
        &mut self,
        index: usize,
        now: f32,
        speed: f32,
        smashables: &[Smashable],
        props: &[PropArticle],
    ) {
        let Some(crate_def) = smashables.get(index) else {
            return;
        };
        let Some(pieces) =
            nfs_assets::physics::original_track_smash::smash_pieces(&crate_def.definition, speed)
        else {
            return;
        };
        let pieces = pieces
            .iter()
            .filter_map(|piece| {
                let group = piece.base_group.or(piece.quat_group)?;
                props.iter().position(|p| p.fourcc == group)
            })
            .collect();
        self.running[index] = Some(Running {
            started: now,
            pieces,
        });
    }

    /// Draw calls at time `now`: whole crates, and the pieces of running smashes.
    pub fn draws(
        &self,
        now: f32,
        smashables: &[Smashable],
        props: &[PropArticle],
    ) -> Vec<CrateDraw> {
        let mut out = Vec::new();
        for (i, crate_def) in smashables.iter().enumerate() {
            match self.running.get(i).and_then(Option::as_ref) {
                None => out.push(CrateDraw {
                    model: Mat4::IDENTITY,
                    mesh_range: crate_def.intact_meshes.clone(),
                }),
                Some(run) => {
                    let frame_pos = ((now - run.started) * FRAMES_PER_SECOND).max(0.0);
                    for &p in &run.pieces {
                        let prop = &props[p];
                        let Some(frame) = prop.frames.get(frame_pos as usize) else {
                            continue; // clip finished: the piece is gone
                        };
                        out.push(CrateDraw {
                            model: piece_model(&crate_def.placement, frame),
                            mesh_range: prop.mesh_range.clone(),
                        });
                    }
                }
            }
        }
        out
    }
}

/// Scene-space model matrix of a piece: local vertices (scene coordinates)
/// -> original coordinates -> the frame's rotation and position (crate-local)
/// -> the crate's `tr` placement -> scene coordinates.
pub fn piece_model(placement: &[f32; 16], frame: &nfs_assets::PropFrame) -> Mat4 {
    let flip = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
    // `tr` holds row vectors (translation in row 3); read as columns it already is
    // the column-vector matrix (its transpose).
    let tr = Mat4::from_cols_array(placement);
    let q = frame.rotation;
    let rot = Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize();
    let local = Mat4::from_rotation_translation(rot, Vec3::from(frame.position));
    flip * tr * local * flip
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfs_assets::PropFrame;

    #[test]
    fn identity_frame_and_placement_leave_a_point_alone() {
        let ident = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        let frame = PropFrame {
            rotation: [0., 0., 0., 1.],
            position: [0., 0., 0.],
        };
        let m = piece_model(&ident, &frame);
        let p = m.transform_point3(Vec3::new(1.0, 2.0, 3.0));
        assert!((p - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5);
    }

    #[test]
    fn placement_translation_moves_in_scene_z_mirrored() {
        let mut tr = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        tr[12] = 10.0;
        tr[14] = 20.0; // original z
        let frame = PropFrame {
            rotation: [0., 0., 0., 1.],
            position: [1., 0., 0.],
        };
        let p = piece_model(&tr, &frame).transform_point3(Vec3::ZERO);
        assert!((p - Vec3::new(11.0, 0.0, -20.0)).length() < 1e-4, "{p:?}");
    }
}
