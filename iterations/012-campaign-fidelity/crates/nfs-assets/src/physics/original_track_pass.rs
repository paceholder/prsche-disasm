//! `Car_CollideTrackObjects` `0x498de0` over a track's objects: the 5 m
//! query, the near test, the contact class and the dispatch to the cylinder
//! and box responses (`docs/re/track-objects.md` section 3).
//!
//! Approximation A171: objects come from a grid over their XZ bounds, one visit
//! each, instead of the retained tree's leaf records (`0x473870`).

use std::collections::HashMap;

use nfs_formats::animdefs::{AnimDefinition, AnimDefs};

use super::original_pair::{PairCar, PairGlobals};
use super::original_track_object_build::{ObjectShape, TrackObject};
use super::original_track_objects::{
    BoxObject, CylinderObject, ObjectCar, box_response, cylinder_response, near_test,
};
use super::original_track_smash::{ContactClass, SmashState, contact_class};

/// Query radius of `0x473870` [VA 0x498de0: `0x40a00000`].
pub const QUERY_RADIUS: f32 = 5.0;
/// Recorded contact strength of the record-only classes [VA 0x498de0: `0x41700000`].
pub const RECORD_STRENGTH: f32 = 15.0;
/// Contact code of a drive-through record [VA 0x498de0].
pub const DRIVE_THROUGH_CODE: u32 = 0x6000f;
const CELL: f32 = 16.0;

/// A smash that started in this pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmashEvent {
    pub object_index: usize,
    /// Length of the car velocity at the contact (picks the speed tier).
    pub speed: f32,
}

/// A solid contact of the player with an object (for sparks and sound).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectHit {
    /// Impact strength `+0x430`.
    pub impact: f32,
    /// Contact position `+0x440`.
    pub point: [f32; 3],
    /// Car velocity after the response.
    pub velocity: [f32; 3],
}

pub struct TrackObjectWorld {
    /// Solid contacts since the last drain.
    pub hits: Vec<ObjectHit>,
    /// The object of the last solid contact (debug report).
    pub last_hit: Option<usize>,
    objects: Vec<TrackObject>,
    definitions: Vec<Option<AnimDefinition>>,
    states: Vec<SmashState>,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

fn cell(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

impl TrackObjectWorld {
    pub fn new(objects: Vec<TrackObject>, anim: &AnimDefs) -> Self {
        let definitions = objects
            .iter()
            .map(|o| {
                anim.find_tag_or_first(o.tag)
                    .filter(|d| d.tiers.iter().any(|t| !t.pieces.is_empty()))
                    .cloned()
            })
            .collect();
        Self::with_definitions(objects, definitions)
    }

    /// `definitions[i]` is the definition with pieces of object `i`, if any.
    pub fn with_definitions(
        objects: Vec<TrackObject>,
        definitions: Vec<Option<AnimDefinition>>,
    ) -> Self {
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (i, o) in objects.iter().enumerate() {
            for cx in cell(o.bounds_min[0])..=cell(o.bounds_max[0]) {
                for cz in cell(o.bounds_min[2])..=cell(o.bounds_max[2]) {
                    grid.entry((cx, cz)).or_default().push(i as u32);
                }
            }
        }
        let states = vec![SmashState::Intact; objects.len()];
        Self {
            hits: Vec::new(),
            last_hit: None,
            objects,
            definitions,
            states,
            grid,
        }
    }

    pub fn objects(&self) -> &[TrackObject] {
        &self.objects
    }

    pub fn state(&self, index: usize) -> SmashState {
        self.states[index]
    }

    /// Race restart: every object is whole again [VA 0x476170].
    pub fn reset(&mut self) {
        self.states.fill(SmashState::Intact);
    }

    /// Indices of the objects whose XZ bounds grown by 5 contain `position`,
    /// in index order [VA 0x473870, 0x486510].
    fn near(&self, position: [f32; 3]) -> Vec<usize> {
        let mut found: Vec<usize> = Vec::new();
        for cx in cell(position[0] - QUERY_RADIUS)..=cell(position[0] + QUERY_RADIUS) {
            for cz in cell(position[2] - QUERY_RADIUS)..=cell(position[2] + QUERY_RADIUS) {
                if let Some(list) = self.grid.get(&(cx, cz)) {
                    found.extend(list.iter().map(|&i| i as usize));
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        found.retain(|&i| self.objects[i].near_point(position, QUERY_RADIUS));
        found
    }

    /// One pass for one car. `events` receives the smashes that start.
    /// The object cooldown `+0x43c` is 0 in a race and not modelled.
    pub fn pass(
        &mut self,
        car: &mut ObjectCar,
        globals: &mut PairGlobals,
        events: &mut Vec<SmashEvent>,
    ) {
        for index in self.near(car.pair.position_330) {
            let object = &self.objects[index];
            if !near_test(car, object.centre(), object.radius()) {
                continue;
            }
            let v = car.pair.velocity_33c;
            let speed = ((v[0] as f64).powi(2) + (v[1] as f64).powi(2) + (v[2] as f64).powi(2))
                .sqrt() as f32;
            let before = (
                car.pair.code_438,
                car.pair.contact_pos_440,
                car.pair.impact_430,
            );
            let class = contact_class(
                true,
                object.drive_through,
                self.states[index],
                self.definitions[index].as_ref(),
                speed,
            );
            match class {
                ContactClass::Gone => {}
                ContactClass::Solid => match &object.shape {
                    ObjectShape::Cylinder => cylinder_response(
                        car,
                        &CylinderObject {
                            bounds_min: object.bounds_min,
                            bounds_max: object.bounds_max,
                            radius: object.radius(),
                            id: object.id,
                        },
                        globals,
                    ),
                    ObjectShape::Box { corners, .. } => box_response(
                        car,
                        &BoxObject {
                            bounds_min: object.bounds_min,
                            bounds_max: object.bounds_max,
                            corners: *corners,
                            id: object.id,
                        },
                        globals,
                    ),
                },
                ContactClass::DriveThrough => {
                    // (not reached with the manager present)
                    let p = &mut car.pair;
                    if p.speed_35c > 2.0 {
                        p.impact_430 = RECORD_STRENGTH;
                        p.code_438 = DRIVE_THROUGH_CODE;
                        p.contact_pos_440 = p.position_330;
                    }
                }
                ContactClass::Smash => {
                    self.states[index] = SmashState::Smashed;
                    events.push(SmashEvent {
                        object_index: index,
                        speed,
                    });
                    let p = &mut car.pair;
                    p.code_438 = p.type_44c | 0x50000;
                    p.contact_pos_440 = p.position_330;
                    p.impact_430 = RECORD_STRENGTH;
                }
            }
            let p = &car.pair;
            if class == ContactClass::Solid
                && (p.code_438, p.contact_pos_440, p.impact_430) != before
            {
                self.last_hit = Some(index);
                self.hits.push(ObjectHit {
                    impact: p.impact_430,
                    point: p.contact_pos_440,
                    velocity: p.velocity_33c,
                });
            }
        }
    }
}

/// Track-object view of a car that has only the collision view (the 6 DOF
/// body's adapter and AI cars): body points and wheel records are `local_points`
/// (relative to the box centre in the car axes R, U, F) moved into the world;
/// the ground frame is level (`+0x490`, `+0x4a8` = R, F, as in `barrier_state`).
pub fn object_car_from_pair(pair: &PairCar, local_points: &[[f32; 3]; 4]) -> ObjectCar {
    let world = |p: &[f32; 3]| -> [f32; 3] {
        std::array::from_fn(|i| {
            pair.position_330[i]
                + p[0] * pair.axis_r_364[i]
                + p[1] * pair.axis_u_370[i]
                + p[2] * pair.axis_f_37c[i]
        })
    };
    let points = local_points.map(|p| world(&p));
    let v = pair.velocity_33c;
    let dot = |a: [f32; 3]| a[0] * v[0] + a[1] * v[1] + a[2] * v[2];
    ObjectCar {
        pair: pair.clone(),
        wheel_records_7f8: points,
        body_points_1dc: points.to_vec(),
        local_velocity_d58: [
            dot(pair.axis_r_364),
            dot(pair.axis_u_370),
            dot(pair.axis_f_37c),
        ],
        field_3a0: 1.0,
        field_428: 0.0,
        code_464: 0,
        ground_right_490: pair.axis_r_364,
        ground_forward_4a8: pair.axis_f_37c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfs_formats::animdefs::parse_animdefs;

    fn object(tag: &[u8; 4], shape: ObjectShape, min: [f32; 3], max: [f32; 3]) -> TrackObject {
        TrackObject {
            article_index: 0,
            name: String::new(),
            tag: u32::from_le_bytes(*tag),
            id: 0,
            drive_through: false,
            bounds_min: min,
            bounds_max: max,
            shape,
        }
    }

    fn car_at(x: f32, z: f32, vz: f32) -> ObjectCar {
        let mut c = ObjectCar::default();
        c.pair.position_330 = [x, 0.65, z];
        c.pair.velocity_33c = [0.0, 0.0, vz];
        c.pair.speed_35c = vz.abs();
        c.pair.axis_r_364 = [1.0, 0.0, 0.0];
        c.pair.axis_u_370 = [0.0, 1.0, 0.0];
        c.pair.axis_f_37c = [0.0, 0.0, 1.0];
        c.pair.extents_3a8 = [0.9, 0.65, 2.1];
        c.pair.mass_354 = 1.25;
        c.pair.inv_mass_358 = 0.8;
        c.pair.inertia_394 = 7.5;
        c.pair.inv_inertia_398 = 1.0 / 7.5;
        c.ground_right_490 = c.pair.axis_r_364;
        c.ground_forward_4a8 = c.pair.axis_f_37c;
        c
    }

    fn defs() -> AnimDefs {
        parse_animdefs(
            "SECTION:ANIM { tag=BBOX; collideType=kBox; speed=0; \
             SECTION:PIECE { playbackrate=5; QuatGroup=BX01; } }\n\
             SECTION:ANIM { tag=STRE; collideType=kCylinder; }",
        )
        .unwrap()
    }

    #[test]
    fn far_object_does_nothing() {
        let tree = object(
            b"STRE",
            ObjectShape::Cylinder,
            [-0.5, 0.0, -0.5],
            [0.5, 5.0, 0.5],
        );
        let mut world = TrackObjectWorld::new(vec![tree], &defs());
        let mut car = car_at(8.0, 0.0, -20.0);
        let before = car.clone();
        world.pass(&mut car, &mut PairGlobals::default(), &mut Vec::new());
        assert_eq!(car, before);
    }

    #[test]
    fn bush_is_drive_through_and_tree_is_solid() {
        let mut bush = object(
            b"BUSH",
            ObjectShape::Cylinder,
            [-0.5, 0.0, -0.5],
            [0.5, 1.5, 0.5],
        );
        bush.drive_through = true;
        let tree = object(
            b"STRE",
            ObjectShape::Cylinder,
            [-0.5, 0.0, -0.5],
            [0.5, 5.0, 0.5],
        );
        let mut car = car_at(0.0, 2.2, -25.0);
        let mut world = TrackObjectWorld::new(vec![bush], &defs());
        world.pass(&mut car, &mut PairGlobals::default(), &mut Vec::new());
        assert_eq!(car.pair.velocity_33c, [0.0, 0.0, -25.0], "no push");
        assert_eq!(car.pair.position_330, [0.0, 0.65, 2.2]);
        assert!(world.hits.is_empty());
        let mut car = car_at(0.0, 2.2, -25.0);
        let mut world = TrackObjectWorld::new(vec![tree], &defs());
        world.pass(&mut car, &mut PairGlobals::default(), &mut Vec::new());
        assert_ne!(car.pair.velocity_33c, [0.0, 0.0, -25.0]);
    }

    #[test]
    fn pole_hit_head_on_pushes_the_car_out() {
        let tree = object(
            b"STRE",
            ObjectShape::Cylinder,
            [-0.5, 0.0, -0.5],
            [0.5, 5.0, 0.5],
        );
        let mut world = TrackObjectWorld::new(vec![tree], &defs());
        let mut car = car_at(0.0, 2.2, -25.0);
        world.pass(&mut car, &mut PairGlobals::default(), &mut Vec::new());
        assert!(
            car.pair.position_330[2] > 2.2,
            "{:?}",
            car.pair.position_330
        );
        assert!(car.pair.impact_430 > 10.0);
        assert_eq!(car.pair.code_438 & 0xf0000, 0x50000);
    }

    #[test]
    fn crate_smashes_once_and_stops_colliding() {
        let corners = [
            [-0.75, 0.0, -0.75],
            [-0.75, 0.0, 0.75],
            [0.75, 0.0, 0.75],
            [0.75, 0.0, -0.75],
        ];
        let b = object(
            b"BBOX",
            ObjectShape::Box {
                corners,
                radius: 1.1,
            },
            [-0.75, 0.0, -0.75],
            [0.75, 1.5, 0.75],
        );
        let mut world = TrackObjectWorld::new(vec![b], &defs());
        let mut events = Vec::new();
        let mut car = car_at(0.0, 2.6, -20.0);
        world.pass(&mut car, &mut PairGlobals::default(), &mut events);
        assert_eq!(events.len(), 1);
        assert_eq!(car.pair.velocity_33c, [0.0, 0.0, -20.0], "no push");
        assert_eq!(car.pair.impact_430, 15.0);
        assert_eq!(car.pair.code_438, 0x50000);
        assert_eq!(world.state(0), SmashState::Smashed);
        // Second hit: nothing.
        let mut again = car.clone();
        world.pass(&mut again, &mut PairGlobals::default(), &mut events);
        assert_eq!(events.len(), 1);
        assert_eq!(again, car);
        world.reset();
        assert_eq!(world.state(0), SmashState::Intact);
    }
}
