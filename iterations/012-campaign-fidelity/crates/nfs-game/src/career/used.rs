//! The used-car market (`docs/re/career.md` "Used cars"): car values, the market cars,
//! offers for the player's listed cars, selling and buying.
//!
//! The market `[0x691b88]` holds pointers to car objects: market cars it owns and the
//! player's garage cars listed for sale. Here a slot is either a market car or the handle of
//! a garage car. The original random generators are global; they are passed in explicitly.

use super::catalogue::{CarCatalogue, CarRecord, PartRecord, PartsCatalogue};
use super::garage::{BuyRefusal, OwnedCar, GARAGE_LIMIT};
use super::parts::part_value;
use super::{Career, CareerEvent};

/// The game generator `0x538040` (six words `[0x5de910..0x5de924]`, add with carry).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameRandom {
    pub s: [u32; 6],
}

impl Default for GameRandom {
    /// The .data state of `[0x5de910..]` before any seeding.
    fn default() -> Self {
        GameRandom { s: [0xf22d_0e56, 0x8831_26e9, 0xc624_dd2f, 0x0702_c49c, 0x9e35_3f7d, 0x6fdf_3b64] }
    }
}

impl GameRandom {
    /// `0x538110`: seed (the used market passes `time(0)`).
    pub fn seeded(v: i32) -> Self {
        let v = v as u32;
        GameRandom {
            s: [
                v.wrapping_sub(0x0dd2_f1aa),
                v.wrapping_sub(0x77ce_d917),
                v.wrapping_sub(0x39db_22d1),
                v.wrapping_add(0x0702_c49c),
                v.wrapping_sub(0x61ca_c083),
                v.wrapping_add(0x6fdf_3b64),
            ],
        }
    }

    /// `0x538040`: `s5 += 1`, each word adds the next one plus carry; returns `s0`.
    pub fn next(&mut self) -> u32 {
        let [s0, s1, s2, s3, s4, s5] = self.s;
        let a = s4.wrapping_add(s5);
        let c = u32::from(a < s5 || a < s4);
        let b = a.wrapping_add(c).wrapping_add(s3);
        let c2 = u32::from(b < s3);
        let d = b.wrapping_add(c2).wrapping_add(s2);
        let c3 = u32::from(d < s2);
        let e = d.wrapping_add(c3).wrapping_add(s1);
        let c4 = u32::from(e < s1);
        let mut n = [s0.wrapping_add(e).wrapping_add(c4), e, d, b, a, s5.wrapping_add(1)];
        if n[5] == 0 {
            n[4] = n[4].wrapping_add(1);
            if n[4] == 0 {
                n[3] = n[3].wrapping_add(1);
                if n[3] == 0 {
                    n[2] = n[2].wrapping_add(1);
                    if n[2] == 0 {
                        n[1] = n[1].wrapping_add(1);
                        if n[1] == 0 {
                            n[0] = n[0].wrapping_add(1);
                        }
                    }
                }
            }
        }
        self.s = n;
        n[0]
    }
}

/// The C runtime `rand` (`0x5a17c9`, MSVC: `state × 214013 + 2531011`, bits 16..30).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrtRandom {
    pub state: u32,
}

impl Default for CrtRandom {
    fn default() -> Self {
        CrtRandom { state: 1 }
    }
}

impl CrtRandom {
    pub fn next(&mut self) -> i32 {
        self.state = self.state.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.state >> 16) & 0x7fff) as i32
    }
}

/// Generators and the decal count `[0x6115f8]` the market uses.
pub struct MarketRandom<'a> {
    pub game: &'a mut GameRandom,
    pub crt: &'a mut CrtRandom,
    pub decal_count: u32,
}

/// One entry of the market list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarketSlot {
    /// A car offered by the market (+0x6a0 = 0).
    Market(OwnedCar),
    /// A garage car the player listed for sale (by [`OwnedCar::handle`]).
    Listed(u64),
}

/// `[0x691b88]` and the market year `[0x691b58]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsedMarket {
    pub year: i32,
    pub slots: Vec<MarketSlot>,
}

/// Appearance chance by age (`0x5b7a10`): a car of age a is offered when `[a] ≤ rand % 60`.
const AGE_CHANCE: [u32; 61] = [
    5, 10, 15, 20, 25, 35, 50, 70, 80, 80, 70, 50, 40, 30, 20, 10, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 4, 4, 3, 3, 3, 2, 2,
];
/// (lo, hi, a, b) on `rand % 100`: value `a + rand % (b − a)`.
const SHARE_ROWS: [[i32; 4]; 4] = [[0, 5, 0, 0], [6, 45, 1, 10], [46, 75, 11, 20], [76, 100, 21, 50]]; // 0x5b7b08
const MILEAGE_ROWS: [[i32; 4]; 4] = [[0, 5, 0, 0], [6, 20, 2000, 15000], [21, 80, 15000, 25000], [81, 100, 25000, 100000]]; // 0x5b7b48
const WEAR_ROWS: [[i32; 4]; 4] = [[0, 5, 0, 0], [6, 20, 1, 20], [21, 80, 20, 40], [81, 100, 40, 100]]; // 0x5b7b88
const RACE_ROWS: [[i32; 4]; 4] = [[0, 5, 0, 0], [6, 20, 1, 2], [21, 80, 3, 10], [81, 100, 11, 50]]; // 0x5b7bc8
/// (lo, hi, pct) value adjustments: mileage (0x5b7c08, applied as pct·base / −100) and
/// condition (0x5b7c48, pct·base / 100).
const VALUE_MILEAGE: [[i32; 3]; 5] =
    [[0, 5000, 0], [5001, 10000, -10], [10001, 15000, -15], [15001, 20000, -25], [20001, 1_000_000, -30]];
const VALUE_CONDITION: [[i32; 3]; 5] = [[0, 5, -1], [6, 10, -6], [11, 15, -11], [16, 25, -16], [26, 100, -26]];
/// Buyer interest by mileage for ages ≤ 10, ≤ 20, ≤ 30, older (0x5b7c88, 0x5b7cb8, 0x5b7ce8,
/// 0x5b7d18).
const BUYER_MILEAGE: [[[i32; 3]; 4]; 4] = [
    [[0, 500, 3], [501, 2000, 2], [2001, 5000, 1], [5001, 1_000_000, -1]],
    [[0, 500, 4], [501, 2000, 3], [2001, 5000, 2], [10000, 1_000_000, -1]],
    [[0, 500, 8], [501, 2000, 6], [2001, 5000, 4], [15000, 1_000_000, -1]],
    [[0, 500, 15], [501, 2000, 10], [2001, 5000, 5], [20000, 1_000_000, -1]],
];
/// (lo, hi, val) on the first year (0x5b7d48; the ranges are ages, so no year matches).
const BUYER_YEAR: [[i32; 3]; 5] = [[0, 9, -1], [10, 19, 3], [20, 29, 5], [30, 39, 10], [40, 1000, 15]];
/// (lo, hi, chance %, no-sale flag) on the buyer score (0x5b7d88).
const BUYER_SCORE: [[i32; 4]; 7] = [
    [0, 83, 20, 1],
    [84, 89, 40, 1],
    [90, 95, 60, 1],
    [95, 99, 80, 1],
    [100, 104, 80, 0],
    [105, 109, 90, 0],
    [110, 1_000_000, 100, 0],
];

/// `__ftol`: truncation, low dword of the 64-bit result (0 for NaN and out-of-range values,
/// the x87 integer indefinite).
fn ftol(x: f64) -> i32 {
    if x.is_nan() || x >= 9.223_372_036_854_775_807e18 || x < -9.223_372_036_854_775_808e18 {
        return 0;
    }
    x as i64 as i32
}

/// MSVC signed division by a constant through a magic multiply (`imul`, `sar 5`, sign fix).
fn div_magic(x: i32, magic: i32) -> i32 {
    let hi = ((x as i64 * magic as i64) >> 32) as i32;
    let q = hi >> 5;
    q.wrapping_add((q as u32 >> 31) as i32)
}
fn div100(x: i32) -> i32 {
    div_magic(x, 0x51eb_851f)
}
fn div_neg100(x: i32) -> i32 {
    div_magic(x, 0xae14_7ae1u32 as i32)
}

/// A table row on `r` with an optional random value `a + rand % (b − a)` (the last matching
/// row wins; the generator runs only for matching rows with b ≠ a).
fn table_value(rows: &[[i32; 4]], r: i32, rng: &mut GameRandom) -> i32 {
    let mut v = 0;
    for row in rows {
        if row[0] <= r && r <= row[1] {
            let span = row[3].wrapping_sub(row[2]);
            let x = if span == 0 { 0 } else { rng.next() % span as u32 };
            v = (x as i32).wrapping_add(row[2]);
        }
    }
    v
}

fn bswap_at(rec: &CarRecord, o: usize) -> i32 {
    let b = rec.bytes();
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as i32
}

fn i16_rec(rec: &CarRecord, o: usize) -> i16 {
    i16::from_le_bytes([rec.bytes()[o], rec.bytes()[o + 1]])
}

const ENGINE: &[u8] = b"Engine";
const ENGINE_STD: &[u8] = b"Engine (Standard)";

/// `OwnedCar_PartCondition` 0x4ef580: mean condition of the installed parts of a category
/// (x87 sum, divided only for two or more parts).
pub fn category_condition(car: &OwnedCar, cat: i16) -> f64 {
    let mut sum = 0.0f64;
    let mut n = 0;
    for p in car.installed.iter().filter(|p| p.category() == cat) {
        sum += p.condition() as f64;
        n += 1;
    }
    if n > 1 { sum / n as f64 } else { sum }
}

/// The f32-stored category means of `0x4ef960`.
fn category_condition_f32(car: &OwnedCar, cat: i16) -> f32 {
    category_condition(car, cat) as f32
}

/// `OwnedCar_Condition` 0x4ef960: ((m7 + m6) · 0.5 + m1 + m10 + m9 + m8 + m5 + m4 + m3 + m2)
/// · 0.11111111, the category means stored as f32 except m1.
pub fn car_condition(car: &OwnedCar) -> f64 {
    let m2 = category_condition_f32(car, 2) as f64;
    let m3 = category_condition_f32(car, 3) as f64;
    let m4 = category_condition_f32(car, 4) as f64;
    let m5 = category_condition_f32(car, 5) as f64;
    let m6 = category_condition_f32(car, 6) as f64;
    let m7 = category_condition_f32(car, 7) as f64;
    let m8 = category_condition_f32(car, 8) as f64;
    let m9 = category_condition_f32(car, 9) as f64;
    let m10 = category_condition_f32(car, 10) as f64;
    let m1 = category_condition(car, 1);
    ((m7 + m6) * 0.5f32 as f64 + m1 + m10 + m9 + m8 + m5 + m4 + m3 + m2) * 0.111_111_11f32 as f64
}

/// `Car_MarketValue` 0x4f8e50 in market year `year`: base = base price × value percent at
/// `[car + 4·year − 0x1c60]` (the record's value table indexed by year − 1948, not by age)
/// × 0.01, then + mileage / condition / race-counter adjustments + installed − stock part
/// values; at least 5 % of the base.
pub fn market_value(car: &OwnedCar, year: i32) -> i32 {
    let base_price = car.i32(0x20c);
    let off = 4i64 * year as i64 - 0x1c60;
    let pct = if off >= 0 && (off as usize) + 4 <= car.image.len() { car.f32(off as usize) } else { 0.0 };
    let base = ftol(base_price as f64 * pct as f64 * 0.01);
    let mileage = car.mileage();
    let mut mil = 0;
    for row in &VALUE_MILEAGE {
        if row[0] <= mileage && mileage <= row[1] {
            mil = div_neg100(row[2].wrapping_mul(base));
        }
    }
    let mut cond = 0;
    for row in &VALUE_CONDITION {
        let c = ftol(car_condition(car));
        if row[0] <= c {
            let c = ftol(car_condition(car));
            if c <= row[1] {
                cond = div100(row[2].wrapping_mul(base));
            }
        }
    }
    let wins = div100(car.wins().wrapping_mul(base));
    let inst = car.installed.iter().fold(0i32, |a, p| a.wrapping_add(part_value(p, year)));
    let stock = car.stock.iter().fold(0i32, |a, p| a.wrapping_add(part_value(p, year)));
    let total = inst.wrapping_sub(stock).wrapping_add(wins).wrapping_add(cond).wrapping_add(mil).wrapping_add(base);
    let floor = base as f64 * 0.05f32 as f64;
    if (total as f64) < floor {
        ftol(floor)
    } else {
        total
    }
}

/// Engine-family and same-part conflicts of `UsedCar_RandomParts` for market picks.
fn pick_conflicts(c: &PartRecord, q: &PartRecord) -> bool {
    (c.name() == q.name() && c.category() == q.category())
        || (c.name() == ENGINE_STD && q.name() == ENGINE)
        || (c.name() == ENGINE && q.name() == ENGINE_STD)
        || (c.name() == ENGINE && q.name() == ENGINE)
        || (c.name() == ENGINE_STD && q.name() == ENGINE_STD)
}

/// The same check for the stock parts merged afterwards (no Engine / Engine (Standard)
/// cross check).
fn stock_conflicts(c: &PartRecord, q: &PartRecord) -> bool {
    (c.name() == q.name() && c.category() == q.category())
        || (c.name() == ENGINE && q.name() == ENGINE)
        || (c.name() == ENGINE_STD && q.name() == ENGINE_STD)
}

impl Career {
    /// `UsedCar_RandomParts` 0x4f8590: random installed parts for a market car: a share of
    /// the car's parts market (picked by index with a shrinking range; the same part can be
    /// picked twice), conflicts skipped, then the stock parts not replaced; ten zone words
    /// +0x6a8 and a condition per part from the wear table, the "Body" part getting the zone
    /// mean.
    pub fn used_car_random_parts(&self, car: &mut OwnedCar, parts: &PartsCatalogue, rng: &mut GameRandom) {
        car.installed.clear();
        let market = self.parts_market(car, parts, self.used_market.year);
        let n = market.len() as i32;
        let r = (rng.next() % 100) as i32;
        let share = table_value(&SHARE_ROWS, r, rng);
        let count = ftol(share.wrapping_mul(n) as f64 * 0.01);
        if n > 0 {
            let mut avail = n;
            for _ in 0..count.max(0) {
                let u = if avail == 0 { 0 } else { rng.next() % avail as u32 } as usize;
                let cand = &parts.records[market[u]];
                if !car.installed.iter().any(|q| pick_conflicts(cand, q)) {
                    car.installed.push(cand.clone());
                }
                avail -= 1;
            }
        }
        let stock = car.stock.clone();
        for s in &stock {
            if !car.installed.iter().any(|q| stock_conflicts(s, q)) {
                car.installed.push(s.clone());
            }
        }
        let mut sum = 0.0f32;
        for z in 0..10 {
            let r = (rng.next() % 100) as i32;
            let w = table_value(&WEAR_ROWS, r, rng) as f32;
            car.set_f32(0x6a8 + 4 * z, w);
            sum = (w as f64 + sum as f64) as f32;
        }
        let mean = (sum as f64 * 0.1f32 as f64) as f32;
        for p in car.installed.iter_mut() {
            if p.category() == 10 && p.name() == b"Body" {
                p.set_condition(mean);
            } else {
                let r = (rng.next() % 100) as i32;
                p.set_condition(table_value(&WEAR_ROWS, r, rng) as f32);
            }
        }
    }

    /// `UsedCar_Create` 0x4f82c0: a market car of catalogue record `rec`.
    pub fn create_used_car(&mut self, rec: &CarRecord, parts: &PartsCatalogue, r: &mut MarketRandom) -> OwnedCar {
        let (mut body, mut second, mut third) = (0xffff_0000u32 as i32, 0xffff_0000u32 as i32, 0xffff_0000u32 as i32);
        let mut interior = -1i32;
        let ncol = i16_rec(rec, 0x3e6);
        if ncol > 0 {
            let u = (r.game.next() % ncol as i32 as u32) as usize;
            body = bswap_at(rec, 0x3ec + 16 * u);
            second = bswap_at(rec, 0x3f0 + 16 * u);
            third = bswap_at(rec, 0x3f4 + 16 * u);
        }
        let nint = i16_rec(rec, 0x528);
        if nint > 0 {
            let u = (r.game.next() % nint as i32 as u32) as usize;
            interior = bswap_at(rec, 0x530 + 16 * u);
        }
        let mut decal = 0i32;
        if r.crt.next() % 100 > 50 {
            decal = (r.crt.next() as u32 % r.decal_count) as i32;
        }
        let mut decal_colour = -1i32;
        if r.crt.next() % 100 > 80 {
            decal_colour = r.crt.next() % 100 - 1;
        }
        let mut car = OwnedCar::new(rec, [body, interior, second, third, decal, decal_colour], parts, self.cars_alive());
        car.handle = self.next_handle();
        let x = (r.game.next() % 100) as i32;
        let mileage = table_value(&MILEAGE_ROWS, x, r.game);
        car.set_mileage(mileage);
        self.used_car_random_parts(&mut car, parts, r.game);
        let x = (r.game.next() % 100) as i32;
        let races = table_value(&RACE_ROWS, x, r.game);
        car.set_wins(races);
        car.set_owned_flag(0);
        let v = market_value(&car, self.used_market.year);
        car.set_asking_price(v);
        car
    }

    fn slot_car(&mut self, k: usize) -> Option<&mut OwnedCar> {
        match &self.used_market.slots[k] {
            MarketSlot::Market(_) => match &mut self.used_market.slots[k] {
                MarketSlot::Market(c) => Some(c),
                _ => None,
            },
            MarketSlot::Listed(h) => {
                let h = *h;
                self.garage.cars.iter_mut().find(|c| c.handle == h)
            }
        }
    }

    /// `UsedMarket_Evaluate` 0x4f7e90: per slot a buyer score from the asking price against
    /// the value, the condition, age and mileage, the race counter; a sale (market cars: sold
    /// flag +0x690; listed cars: an offer of the asking price) or an offer of the value ±3 %
    /// (capped at the asking price).
    pub fn evaluate_used_market(&mut self, rng: &mut GameRandom) {
        let year = self.used_market.year;
        for k in 0..self.used_market.slots.len() {
            let Some(car) = self.slot_car(k) else { continue };
            let v = market_value(car, year);
            let markup = (v as f64 - car.asking_price() as f64) * 100.0 / v as f64;
            let c = ftol(car_condition(car));
            let mut score = ftol(markup).wrapping_sub(c).wrapping_add(100);
            let first = car.i16_at(0x1fc) as i32;
            let age = year - first;
            let table = if age <= 10 { 0 } else if age <= 20 { 1 } else if age <= 30 { 2 } else { 3 };
            let mileage = car.mileage();
            let mut mil = 0;
            for row in &BUYER_MILEAGE[table] {
                if row[0] <= mileage && mileage <= row[1] {
                    mil = row[2];
                }
            }
            score = score.wrapping_add((car.wins() / 5).wrapping_add(mil));
            let mut yb = 0;
            for row in &BUYER_YEAR {
                if row[0] <= first && first <= row[1] {
                    yb = row[2];
                }
            }
            score = score.wrapping_add(yb);
            let (mut chance, mut no_sale) = (0u32, true);
            for row in &BUYER_SCORE {
                if row[0] <= score && score <= row[1] {
                    chance = row[2] as u32;
                    no_sale = row[3] != 0;
                }
            }
            if !no_sale && rng.next() % 100 < chance {
                if car.owned_flag() != 0 {
                    car.set_offer_flag(1);
                    let a = car.asking_price();
                    car.set_offer(a);
                } else {
                    car.set_sold_flag(1);
                }
                continue;
            }
            car.set_offer_flag(1);
            let x = rng.next() % 601;
            let off = ftol((x as f64 - 300.0f32 as f64) * 1.0e-4f32 as f64 * v as f64);
            let mut offer = off.wrapping_add(v);
            if offer > car.asking_price() {
                offer = car.asking_price();
            }
            if offer < 0 {
                offer = 0;
            }
            car.set_offer(offer);
        }
    }

    /// `UsedMarket_Generate` 0x4f80c0: for each car of the model list older than market year
    /// − 1 (kind ≠ 2, 3), a new market car when the age chance ≤ rand % 60; beyond 25 slots
    /// the first market car leaves.
    pub fn generate_used_market(&mut self, catalogue: &CarCatalogue, parts: &PartsCatalogue, r: &mut MarketRandom) {
        let list = self.car_lists.model.clone();
        for idx in list {
            let rec = &catalogue.records[idx];
            let year = self.used_market.year;
            let first = rec.first_year() as i32;
            if first < year - 1 && rec.kind() != 3 && rec.kind() != 2 {
                let u = r.game.next();
                let age = (year - first) as usize;
                let chance = AGE_CHANCE.get(age).copied().unwrap_or(0x3d4c_cccd);
                if chance <= u % 60 {
                    let car = self.create_used_car(rec, parts, r);
                    if self.used_market.slots.len() > 0x18 {
                        if let Some(i) = self.used_market.slots.iter().position(|s| matches!(s, MarketSlot::Market(c) if c.owned_flag() == 0)) {
                            self.used_market.slots.remove(i);
                        }
                    }
                    self.used_market.slots.push(MarketSlot::Market(car));
                }
            }
        }
    }

    /// `UsedMarket_Refresh` 0x4f7db0 (after every tournament): seed the game generator with
    /// the clock (`clock` = `time(0)`), evaluate the market, drop sold market cars, reprice
    /// the market cars, generate new ones from `self.car_lists.model`.
    pub fn refresh_used_market(&mut self, catalogue: &CarCatalogue, parts: &PartsCatalogue, year: i32, clock: i32, r: &mut MarketRandom) {
        *r.game = GameRandom::seeded(clock);
        self.used_market.year = year;
        self.evaluate_used_market(r.game);
        while let Some(i) = self
            .used_market
            .slots
            .iter()
            .position(|s| matches!(s, MarketSlot::Market(c) if c.sold_flag() != 0 && c.owned_flag() == 0))
        {
            self.used_market.slots.remove(i);
        }
        let year = self.used_market.year;
        for k in 0..self.used_market.slots.len() {
            if let Some(c) = self.slot_car(k) {
                if c.owned_flag() == 0 {
                    let v = market_value(c, year);
                    c.set_asking_price(v);
                }
            }
        }
        self.generate_used_market(catalogue, parts, r);
    }

    /// `UsedMarket_ListCar` 0x4f91c0: garage car `g` offered at `asking`.
    pub fn list_car_for_sale(&mut self, g: usize, asking: i32) {
        let c = &mut self.garage.cars[g];
        c.set_asking_price(asking);
        c.set_listed_flag(1);
        c.set_sold_flag(0);
        c.set_offer_flag(0);
        c.set_offer(0);
        c.set_owned_flag(1);
        let h = c.handle;
        self.used_market.slots.push(MarketSlot::Listed(h));
    }

    /// `UsedMarket_Unlist` 0x4f9340 on the car with handle `h`: the slot leaves the market
    /// and the car's market fields (+0x68c, +0x690..+0x692, +0x694) are cleared. Returns the
    /// car when the slot held a market car.
    fn unlist_handle(&mut self, h: u64) -> Option<OwnedCar> {
        let i = self.used_market.slots.iter().position(|s| match s {
            MarketSlot::Listed(x) => *x == h,
            MarketSlot::Market(c) => c.handle == h,
        })?;
        let reset = |c: &mut OwnedCar| {
            c.set_asking_price(0);
            c.set_listed_flag(0);
            c.set_sold_flag(0);
            c.set_offer_flag(0);
            c.set_offer(0);
        };
        match self.used_market.slots.remove(i) {
            MarketSlot::Listed(h) => {
                if let Some(c) = self.garage.cars.iter_mut().find(|c| c.handle == h) {
                    reset(c);
                }
                None
            }
            MarketSlot::Market(mut c) => {
                reset(&mut c);
                Some(c)
            }
        }
    }

    /// `UsedMarket_Unlist` 0x4f9340: the listing of garage car `g` is withdrawn.
    pub fn unlist_car(&mut self, g: usize) {
        let h = self.garage.cars[g].handle;
        self.unlist_handle(h);
    }

    /// `UsedMarket_DeclineOffer` 0x4f9720.
    pub fn decline_offer(&mut self, g: usize) {
        let c = &mut self.garage.cars[g];
        c.set_offer_flag(0);
        c.set_offer(0);
    }

    /// `Garage_SellCar` 0x502190: garage car `g` is sold for its offer +0x694.
    pub fn sell_car(&mut self, g: usize, game_type: i32) {
        if g >= self.garage.cars.len() {
            return;
        }
        let car = self.garage.cars.remove(g);
        self.add_cash(car.offer());
        self.events.push(CareerEvent::Sound(10));
        // The car is still listed: drop the slot (the original resets the fields of the
        // object it then destroys).
        if let Some(i) = self.used_market.slots.iter().position(|s| matches!(s, MarketSlot::Listed(h) if *h == car.handle)) {
            self.used_market.slots.remove(i);
        }
        self.sort_garage();
        self.update_available_cars(game_type);
    }

    /// `Garage_BuyUsedCar` 0x501f40: market slot `k` (a market car) at its asking price:
    /// cash (802) and garage limit (61) checks, price paid +0x6a4, the market fields reset,
    /// the car joins the garage (sorted), sound 0xb.
    pub fn buy_used_car(&mut self, k: usize, game_type: i32) -> Result<(), BuyRefusal> {
        let Some(car) = self.slot_car(k) else { return Ok(()) };
        let price = car.asking_price();
        if self.cash < price {
            return Err(BuyRefusal::NotEnoughCash);
        }
        if self.garage.cars.len() >= GARAGE_LIMIT {
            return Err(BuyRefusal::GarageFull);
        }
        let car = self.slot_car(k).unwrap();
        car.set_price_paid(price);
        let h = car.handle;
        self.add_cash(price.wrapping_neg());
        if let Some(c) = self.unlist_handle(h) {
            self.garage.cars.push(c);
        }
        self.sort_garage();
        self.events.push(CareerEvent::Sound(0xb));
        self.update_available_cars(game_type);
        Ok(())
    }
}
