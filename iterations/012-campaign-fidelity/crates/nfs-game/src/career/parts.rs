//! Parts: prices by year, the parts market of a car, the player's spare parts, installing,
//! repair (`docs/re/career.md` "Parts"). Part condition +0x164 is wear in percent / 2:
//! `ftol(2 × condition)` is the share of the price a part has lost.

use super::catalogue::{PartRecord, PartsCatalogue};
use super::garage::{part_less, OwnedCar};
use super::{strings, Career};

/// `g_PartInflationPct` 0x5ba4a4: yearly price increase in percent (62 entries).
pub const INFLATION_PCT: [i32; 62] = [
    1, 2, 2, 2, 2, 3, 4, 5, 4, 5, 6, 7, 8, 12, 4, 5, 4, 2, 3, 2, 3, 4, 3, 5, 6, 7, 2, 4, 5, 8, 9, 2, 5, 4, 6, 3, 7, 4,
    7, 6, 6, 4, 7, 3, 6, 8, 3, 4, 5, 2, 4, 3, 4, 2, 5, 2, 1, 3, 7, 5, 4, 3,
];

/// Repair groups of the repair screen (`0x50f110`, checkboxes `+0x284..+0x2a8`): part
/// categories per group; group 8 is the body (zones and the "Body" part), 9 everything.
pub const REPAIR_GROUPS: usize = 10;

/// `__ftol` (x87 truncation, low 32 bits of the 64-bit result).
fn ftol(x: f64) -> i32 {
    x as i64 as i32
}

/// Σ of the first `year − part year` inflation entries (at most 62).
fn inflation_sum(p: &PartRecord, year: i32) -> i32 {
    let age = year - p.year() as i32;
    if age <= 0 {
        return 0;
    }
    INFLATION_PCT.iter().take(age as usize).fold(0i32, |a, &v| a.wrapping_add(v))
}

/// `ftol(2.0 × condition)`.
fn wear_pct(p: &PartRecord) -> i32 {
    ftol(2.0f32 as f64 * p.condition() as f64)
}

/// `0x5181c0`: price × (1 + Σ inflation / 100), integer.
pub fn inflated_price(p: &PartRecord, year: i32) -> i32 {
    let price = p.price();
    price.wrapping_mul(inflation_sum(p, year)) / 100 + price
}

/// `Part_Value` 0x518220: the inflated price × (100 − loss) / 100 with loss =
/// `ftol(2 × condition + 20)` (`[0x5b2470]` = 20: a used part loses 20 % at once) clamped to
/// 0..100.
pub fn part_value(p: &PartRecord, year: i32) -> i32 {
    let w = ftol(2.0f32 as f64 * p.condition() as f64 + 20.0f32 as f64).clamp(0, 100);
    (100 - w).wrapping_mul(inflated_price(p, year)) / 100
}

/// `Part_RepairCost` 0x518880: the inflated price × wear / 100 (wear not clamped).
pub fn repair_cost(p: &PartRecord, year: i32) -> i32 {
    inflated_price(p, year).wrapping_mul(wear_pct(p)) / 100
}

/// `0x518900`: inflated price − `Part_Value`, at least 0 (the cost `0x518a80` pays).
pub fn replace_cost(p: &PartRecord, year: i32) -> i32 {
    let price = p.price();
    let sum = inflation_sum(p, year);
    let r = (price.wrapping_mul(sum) / 100).wrapping_sub(part_value(p, year)).wrapping_add(price);
    if r < 1 { 0 } else { r }
}

/// Category groups of the repair screen, in checkbox order (`0x4efbc0..0x4eff80`).
fn group_categories(group: usize) -> &'static [i16] {
    match group {
        0 => &[1],
        1 => &[2],
        2 => &[3],
        3 => &[4],
        4 => &[5],
        5 => &[6, 7],
        6 => &[8],
        7 => &[9],
        8 => &[10],
        _ => &[2, 3, 4, 5, 6, 7, 8, 9, 10, 1],
    }
}

fn is_body(p: &PartRecord) -> bool {
    p.category() == 10 && p.name() == b"Body"
}

impl Career {
    /// `Parts_BuildMarket` 0x517d10: for each id of the car's market list, every parts-table
    /// record with that id and year ≤ `year` (the category-10 "Body" excluded), sorted by
    /// `Part_Less`. Returns indices into the parts table.
    pub fn parts_market(&self, car: &OwnedCar, parts: &PartsCatalogue, year: i32) -> Vec<usize> {
        let mut out = Vec::new();
        for id in car.record().market_parts() {
            for (k, p) in parts.records.iter().enumerate() {
                if p.id() == id && p.year() as i32 <= year && !is_body(p) {
                    out.push(k);
                }
            }
        }
        let mut recs: Vec<(usize, &PartRecord)> = out.iter().map(|&k| (k, &parts.records[k])).collect();
        for i in 0..recs.len() {
            for j in i + 1..recs.len() {
                if part_less(recs[j].1, recs[i].1) {
                    recs.swap(i, j);
                }
            }
        }
        recs.into_iter().map(|(k, _)| k).collect()
    }

    /// `0x517f70`: spare parts that fit the car (id in its market list, once per matching
    /// list entry; the "Body" part excluded). Indices into [`Career::spare_parts`].
    pub fn fitting_spares(&self, car: &OwnedCar) -> Vec<usize> {
        let market = car.record().market_parts();
        let mut out = Vec::new();
        for (k, p) in self.spare_parts.iter().enumerate() {
            for &id in &market {
                if id == p.id() && !is_body(p) {
                    out.push(k);
                }
            }
        }
        out
    }

    /// `0x518b80`: the spare parts list shown (the "Body" part excluded).
    pub fn listed_spares(&self) -> Vec<usize> {
        (0..self.spare_parts.len()).filter(|&k| !is_body(&self.spare_parts[k])).collect()
    }

    /// `0x5183c0`: a copy of the part joins the spare parts; cash pays the inflated price
    /// (no cash check here; the screen checks).
    pub fn buy_part(&mut self, p: &PartRecord, year: i32) {
        self.spare_parts.push(p.clone());
        let price = p.price();
        let cost = (inflation_sum(p, year).wrapping_mul(price) / 100).wrapping_neg().wrapping_sub(price);
        self.add_cash(cost);
    }

    /// `0x5182c0`: sell spare part `k` for its value.
    pub fn sell_spare_part(&mut self, k: usize, year: i32) {
        if k < self.spare_parts.len() {
            let v = part_value(&self.spare_parts[k], year);
            self.add_cash(v);
            self.spare_parts.remove(k);
        }
    }

    /// `Part_Install` 0x518510: the first installed part of the same category and name goes
    /// to the spare parts, then spare part `k` is appended to the installed list.
    pub fn install_part(&mut self, car: usize, k: usize) {
        let p = self.spare_parts[k].clone();
        let c = &mut self.garage.cars[car];
        if let Some(i) = c.installed.iter().position(|q| q.category() == p.category() && q.name() == p.name()) {
            let old = c.installed.remove(i);
            self.spare_parts.push(old);
        }
        let p = self.spare_parts.remove(k);
        self.garage.cars[car].installed.push(p);
    }

    /// `Part_Uninstall` 0x518740: installed part `i` goes to the spare parts.
    pub fn uninstall_part(&mut self, car: usize, i: usize) {
        let c = &mut self.garage.cars[car];
        if i < c.installed.len() {
            let p = c.installed.remove(i);
            self.spare_parts.push(p);
        }
    }

    /// `Part_Fix` 0x5189f0: pay the repair cost, condition 0.
    pub fn fix_installed_part(&mut self, car: usize, i: usize, year: i32) {
        let cost = repair_cost(&self.garage.cars[car].installed[i], year);
        self.add_cash(cost.wrapping_neg());
        self.garage.cars[car].installed[i].set_condition(0.0);
    }

    /// `Part_Replace` 0x518a80: pay the replace cost, condition 0.
    pub fn replace_installed_part(&mut self, car: usize, i: usize, year: i32) {
        let cost = replace_cost(&self.garage.cars[car].installed[i], year);
        self.add_cash(cost.wrapping_neg());
        self.garage.cars[car].installed[i].set_condition(0.0);
    }

    /// Repair cost of a group (`0x4efbc0`, `0x4efc20`, `0x4efc80`, `0x4efce0`, `0x4efd40`,
    /// `0x4efda0`, `0x4efe60`, `0x4efec0`, `0x4eff20`, all: `0x4eff80`): Σ `Part_RepairCost` of
    /// the installed parts of the group's categories, at the career year.
    pub fn repair_group_cost(&self, car: usize, group: usize) -> i32 {
        let c = &self.garage.cars[car];
        group_categories(group)
            .iter()
            .flat_map(|&cat| c.installed.iter().filter(move |p| p.category() == cat))
            .fold(0i32, |a, p| a.wrapping_add(repair_cost(p, self.year)))
    }

    /// Repair of a group (`0x4f0490..0x4f0700`, body `0x4f0750`, all `0x4f0830`): `Part_Fix`
    /// on the group's installed parts; the body clears the ten zone words +0x6a8 and fixes the
    /// first installed "Body" part (the original fixes it once per zone).
    pub fn repair_group(&mut self, car: usize, group: usize) {
        let year = self.year;
        let fix_body = |s: &mut Career| {
            for z in 0..10 {
                s.garage.cars[car].set_i32(0x6a8 + 4 * z, 0);
                if let Some(i) = s.garage.cars[car].installed.iter().position(is_body) {
                    s.fix_installed_part(car, i, year);
                }
            }
        };
        match group {
            8 => fix_body(self),
            9 => {
                for i in 0..self.garage.cars[car].installed.len() {
                    self.fix_installed_part(car, i, year);
                }
                fix_body(self);
            }
            g => {
                for &cat in group_categories(g) {
                    for i in 0..self.garage.cars[car].installed.len() {
                        if self.garage.cars[car].installed[i].category() == cat {
                            self.fix_installed_part(car, i, year);
                        }
                    }
                }
            }
        }
    }

    /// The repair button of the repair screen (`0x50f110`): the selected groups' costs must
    /// not exceed the cash (string 809), then the groups are repaired in checkbox order.
    pub fn repair_selected(&mut self, car: usize, selected: [bool; REPAIR_GROUPS]) -> Result<(), i32> {
        let total = (0..REPAIR_GROUPS)
            .filter(|&g| selected[g])
            .fold(0i32, |a, g| a.wrapping_add(self.repair_group_cost(car, g)));
        if self.cash < total {
            return Err(strings::NOT_ENOUGH_CASH_REPAIR);
        }
        for g in (0..REPAIR_GROUPS).filter(|&g| selected[g]) {
            self.repair_group(car, g);
        }
        Ok(())
    }
}
