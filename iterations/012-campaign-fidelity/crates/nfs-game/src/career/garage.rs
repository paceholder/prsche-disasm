//! Dealer, car lists and the player's garage (`docs/re/career.md` "Garage, dealer").
//!
//! Owned cars are the original 0x760-byte objects (`0x4ee3b0`): a serial, a copy of the
//! catalogue record, six colour words, the market fields, tuning defaults, and two part lists
//! (`+0x758` installed copies, `+0x75c` stock copies). They are kept as the byte image plus
//! the two part lists, so the `PlayerInfo` save section can store them as the original does.

use super::catalogue::{CarCatalogue, CarRecord, PartRecord, PartsCatalogue, CAR_RECORD_SIZE};
use super::{strings, Career, CareerEvent};

/// Size of the owned-car object.
pub const OWNED_CAR_SIZE: usize = 0x760;
/// Garage capacity (`Garage_BuyDealerCar` 0x501cf0: refused at 80 cars, string 61).
pub const GARAGE_LIMIT: usize = 80;

/// Game type for the car lists (`Game.GameType` accessor; 3 when it is missing).
pub const GAME_TYPE_EVOLUTION: i32 = 4;

/// One owned (or used-market) car.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedCar {
    /// The 0x760-byte object; the vector pointers +0x758 / +0x75c are kept zero.
    pub image: Vec<u8>,
    /// `+0x758`: installed part copies (sorted by `0x49c350`).
    pub installed: Vec<PartRecord>,
    /// `+0x75c`: stock part copies (catalogue order).
    pub stock: Vec<PartRecord>,
    /// Port-side identity (the original uses the object address): links a garage car to its
    /// used-market listing.
    pub handle: u64,
    /// In the car registry `[0x69188c]` (built by the constructor `0x4eeba0`; cars loaded
    /// from a profile are not).
    pub registered: bool,
}

macro_rules! field {
    ($get:ident, $set:ident, $ty:ty, $off:expr, $doc:literal) => {
        #[doc = $doc]
        pub fn $get(&self) -> $ty {
            <$ty>::from_le_bytes(self.image[$off..$off + std::mem::size_of::<$ty>()].try_into().unwrap())
        }
        pub fn $set(&mut self, v: $ty) {
            self.image[$off..$off + std::mem::size_of::<$ty>()].copy_from_slice(&v.to_le_bytes());
        }
    };
}

impl OwnedCar {
    /// `0x4ee3b0` → `0x4eeba0` → `0x4ee9a0`: a car object for a catalogue record.
    ///
    /// `colours` are the six words of the constructor (+0x674..+0x688; for dealer cars the
    /// `DealerCar` accessor slots 0x74..0x88, for used cars body, interior, two more colour
    /// words, decal, decal colour). `others_alive` is whether the car-object registry
    /// `[0x69188c]` is non-empty: the original's serial loop compares the new object's own,
    /// uninitialised +0 word with a counter, which gives 1 for a zeroed block when another
    /// car exists, else 0 (**approximation A300**: the original heap contents are unknown).
    pub fn new(record: &CarRecord, colours: [i32; 6], parts: &PartsCatalogue, others_alive: bool) -> Self {
        let mut image = vec![0u8; OWNED_CAR_SIZE];
        image[4..4 + CAR_RECORD_SIZE].copy_from_slice(&record.bytes()[..CAR_RECORD_SIZE]);
        let mut c = OwnedCar { image, installed: Vec::new(), stock: Vec::new(), handle: 0, registered: true };
        c.set_serial(others_alive as i32);
        for (k, v) in colours.iter().enumerate() {
            c.set_i32(0x674 + 4 * k, *v);
        }
        c.set_owned_flag(1);
        // Tuning defaults (0x4eeba0): 50, 50, 50, 0, 0, 0, 1.0, 30, 30, then 8 zeros; the 17
        // words are copied to +0x714.
        for (k, v) in [50.0f32, 50.0, 50.0, 0.0, 0.0, 0.0, 1.0, 30.0, 30.0].iter().enumerate() {
            c.set_f32(0x6d0 + 4 * k, *v);
        }
        for k in 0..17 {
            let v = c.i32(0x6d0 + 4 * k);
            c.set_i32(0x714 + 4 * k, v);
        }
        c.reset_parts(parts);
        // 0x4ee3b0: ten condition words +0x6a8 cleared after the constructor.
        for k in 0..10 {
            c.set_i32(0x6a8 + 4 * k, 0);
        }
        c
    }

    /// `0x4ee9a0`: stock copies from the record's stock list (+0x3c0 of the object), the
    /// installed list as copies of them, sorted by `0x49c350` (selection swaps). Ids missing
    /// from the parts table are skipped (the original copies from NULL).
    pub fn reset_parts(&mut self, parts: &PartsCatalogue) {
        let rec = CarRecord(self.image[4..4 + CAR_RECORD_SIZE].to_vec());
        self.stock = rec.stock_parts().iter().filter_map(|&id| parts.find(id as i32).cloned()).collect();
        self.installed = self.stock.clone();
        sort_parts(&mut self.installed);
    }

    pub fn i16_at(&self, o: usize) -> i16 {
        i16::from_le_bytes([self.image[o], self.image[o + 1]])
    }
    pub fn i32(&self, o: usize) -> i32 {
        i32::from_le_bytes(self.image[o..o + 4].try_into().unwrap())
    }
    pub fn set_i32(&mut self, o: usize, v: i32) {
        self.image[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&self, o: usize) -> f32 {
        f32::from_bits(self.i32(o) as u32)
    }

    /// The ten damage zone words `+0x6a8` (0..100 per zone, `car+0x4e4` order).
    pub fn damage_zones(&self) -> [f32; 10] {
        std::array::from_fn(|z| self.f32(0x6a8 + 4 * z))
    }

    /// Zones of the front-end 3D car (`0x4c065c..0x4c068b`): the record's words when the
    /// layout's `data.damage` attribute is set (garage, paint shop, used-car damage screen),
    /// else 0; written into the preview car's `car+0x4e4` through `0x4d1a00`, so the model
    /// morph (`R3DCar_UpdateDamage` 0x438230) shows them.
    pub fn preview_damage_zones(&self, layout_damage: bool) -> [f32; 10] {
        if layout_damage { self.damage_zones() } else { [0.0; 10] }
    }

    /// `.DAMAGEZONES` 0x14d of `FE_WriteCarConfig` 0x4eeda0: the zone words, zeros for game
    /// types 0..3 and 9; `Car_Reset` 0x410730 loads them into the race car's `car+0x4e4`, so a
    /// career car starts the race with its saved damage.
    pub fn race_damage_zones(&self, game_type: i32) -> [f32; 10] {
        if matches!(game_type, 0..=3 | 9) { [0.0; 10] } else { self.damage_zones() }
    }

    /// `Record_LoadZoneDamage` 0x4edd40 (after a career race): the player's config block
    /// (`0x5e75a0 + 600·player`: zones `+0x1c4`, part ids `+0x124`, part wear `+0x174`, the
    /// `.DAMAGEZONES` / `.PARTINDEX` / `.PARTDAMAGE` keys) goes back into the car: the zone
    /// words are copied, every installed part whose id equals one of the first `n` ids
    /// (`n` = installed count) takes that wear (0 when not above 0), and the first "Body" part
    /// gets `sum(zones) · 0.1f` (x87 at 53-bit precision; the original writes through NULL
    /// when there is none, here nothing happens).
    pub fn apply_race_damage(&mut self, zones: &[f32; 10], ids: &[i32; 20], wear: &[f32; 20]) {
        let mut sum = 0.0f64;
        for (z, &v) in zones.iter().enumerate() {
            self.set_f32(0x6a8 + 4 * z, v);
            sum += f64::from(v);
        }
        let body = (sum * f64::from(0.1f32)) as f32;
        let n = self.installed.len().min(20);
        for p in self.installed.iter_mut() {
            for k in 0..n {
                if i32::from(p.id()) == ids[k] {
                    p.set_condition(if wear[k] > 0.0 { wear[k] } else { 0.0 });
                }
            }
        }
        if let Some(p) = self.installed.iter_mut().find(|p| p.category() == 10 && p.name() == b"Body") {
            p.set_condition(body);
        }
    }
    pub fn set_f32(&mut self, o: usize, v: f32) {
        self.set_i32(o, v.to_bits() as i32);
    }

    /// The catalogue record copy (+0x004).
    pub fn record(&self) -> CarRecord {
        CarRecord(self.image[4..4 + CAR_RECORD_SIZE].to_vec())
    }
    pub fn car_id(&self) -> i16 {
        i16::from_le_bytes([self.image[4], self.image[5]])
    }

    field!(serial, set_serial, i32, 0x000, "+0x000 serial.");
    field!(asking_price, set_asking_price, i32, 0x68c, "+0x68c asking price (used market).");
    field!(sold_flag, set_sold_flag, u8, 0x690, "+0x690 market flag.");
    field!(listed_flag, set_listed_flag, u8, 0x691, "+0x691 listed for sale.");
    field!(offer_flag, set_offer_flag, u8, 0x692, "+0x692 a buyer made an offer.");
    field!(offer, set_offer, i32, 0x694, "+0x694 offer (paid by `Garage_SellCar`).");
    field!(mileage, set_mileage, i32, 0x698, "+0x698 odometer (+3000 per Evolution race).");
    field!(wins, set_wins, i32, 0x69c, "+0x69c race counter (see `ApplyOutcome::wins`).");
    field!(owned_flag, set_owned_flag, u8, 0x6a0, "+0x6a0 1 = the player's car, 0 = market car.");
    field!(price_paid, set_price_paid, i32, 0x6a4, "+0x6a4 price paid (0 for prize cars).");
}

/// `0x49c350` part order: category +0x26, then price +0x5c, then id, ascending.
pub(crate) fn part_less(a: &PartRecord, b: &PartRecord) -> bool {
    if a.category() != b.category() {
        return a.category() < b.category();
    }
    if a.price() == b.price() {
        return a.id() < b.id();
    }
    a.price() < b.price()
}

/// The selection-swap sort the original uses for its lists: `for i, for j > i:
/// if less(list[j], list[i]) swap` (not stable).
fn selection_swap<T>(list: &mut [T], less: impl Fn(&T, &T) -> bool) {
    for i in 0..list.len() {
        for j in i + 1..list.len() {
            if less(&list[j], &list[i]) {
                list.swap(i, j);
            }
        }
    }
}

pub fn sort_parts(list: &mut [PartRecord]) {
    selection_swap(list, part_less);
}

/// `0x4f1d40` key: era +0x1b0, group +0x1ac, first year +0x1f8.
fn car_less(a: &CarRecord, b: &CarRecord) -> bool {
    (a.era(), a.group(), a.first_year()) < (b.era(), b.group(), b.first_year())
}

/// The catalogue lists `Career_RebuildCarLists` 0x4f3e40 builds (indices into the catalogue),
/// each sorted by `0x4f0f50`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CarLists {
    /// `[0x6919ec]` cars of the catalogue (Evolution: kind ≠ 2).
    pub all: Vec<usize>,
    /// `[0x6919f0]` cars whose model file is present (source of the used market).
    pub model: Vec<usize>,
    /// `[0x6919f4]` Evolution: first year ≤ year; other modes: model present.
    pub year: Vec<usize>,
    /// `[0x6919fc]` records with +0x1f4 == 1.
    pub flag1: Vec<usize>,
    /// `[0x6919f8]` records with +0x1f4 == 2.
    pub flag2: Vec<usize>,
    /// `[0x691a00]` the dealer.
    pub dealer: Vec<usize>,
}

/// `Car_DealerPrice` 0x4ee220: base price, or once the year is past the first year
/// `trunc(base × value[year − first]) / 100` (x87 product).
pub fn dealer_price(rec: &CarRecord, year: i32) -> i32 {
    let base = rec.base_price();
    let first = rec.first_year() as i32;
    if first < year {
        let v = rec.value_percent(year - first);
        let p = (base as f64 * v as f64) as i64 as i32;
        return p / 100;
    }
    base
}

/// The garage: owned cars `[0x691df8]` and the ones usable this year `[0x691dfc]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Garage {
    pub cars: Vec<OwnedCar>,
    /// Indices of `cars` shown for the current mode and year (`0x501ad0`).
    pub available: Vec<usize>,
}

/// Why a purchase failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuyRefusal {
    /// Cash below the price (string 802).
    NotEnoughCash,
    /// 80 cars owned (string 61).
    GarageFull,
}

impl BuyRefusal {
    pub fn string_id(self) -> i32 {
        match self {
            BuyRefusal::NotEnoughCash => strings::NOT_ENOUGH_CASH,
            BuyRefusal::GarageFull => strings::NO_MORE_THAN_EIGHTY,
        }
    }
}

impl Career {
    /// `Career_RebuildCarLists` 0x4f3e40 for `game_type` (`all_cars` = `[0x691a04]`);
    /// `model_present(model)` answers the original's `<dir><model>.crp` file check. Ends
    /// with the owned-car availability (`0x501ad0`). The front end's track list (`0x4fbc10`,
    /// called first) is requested as [`CareerEvent::RebuildTrackList`].
    pub fn rebuild_car_lists(
        &mut self,
        catalogue: &CarCatalogue,
        game_type: i32,
        all_cars: bool,
        model_present: impl Fn(&[u8]) -> bool,
    ) {
        self.events.push(CareerEvent::RebuildTrackList);
        let year = self.year;
        let mut l = CarLists::default();
        for (i, r) in catalogue.records.iter().enumerate() {
            match r.list_flag() {
                0 => {
                    let kind = r.kind();
                    if game_type == GAME_TYPE_EVOLUTION {
                        if kind != 2 {
                            l.all.push(i);
                        }
                        if model_present(r.model()) {
                            l.model.push(i);
                        }
                        if r.first_year() as i32 <= year {
                            l.year.push(i);
                        }
                    } else {
                        if (kind == 0 || kind == 2 || all_cars || game_type == 3) && kind != 3 {
                            l.all.push(i);
                        }
                        if model_present(r.model()) {
                            l.model.push(i);
                            l.year.push(i);
                        }
                    }
                }
                1 => l.flag1.push(i),
                2 => l.flag2.push(i),
                _ => {}
            }
            let in_years = r.first_year() as i32 <= year && year <= r.last_year() as i32;
            if (in_years || all_cars) && r.kind() != 2 && r.list_flag() == 0 && r.kind() != 3 {
                l.dealer.push(i);
            }
        }
        for list in [&mut l.all, &mut l.model, &mut l.year, &mut l.flag1, &mut l.flag2, &mut l.dealer] {
            selection_swap(list, |&a, &b| car_less(&catalogue.records[a], &catalogue.records[b]));
        }
        self.car_lists = l;
        self.update_available_cars(game_type);
    }

    /// `0x501ad0`: owned cars shown; in Evolution only those whose first year ≤ year.
    pub fn update_available_cars(&mut self, game_type: i32) {
        let year = self.year;
        self.garage.available = (0..self.garage.cars.len())
            .filter(|&k| game_type != GAME_TYPE_EVOLUTION || self.garage.cars[k].record().first_year() as i32 <= year)
            .collect();
    }

    /// Whether any car object exists: the registry `[0x69188c]` (garage and market cars).
    pub(crate) fn cars_alive(&self) -> bool {
        self.garage.cars.iter().any(|c| c.registered)
            || self.used_market.slots.iter().any(|s| matches!(s, super::used::MarketSlot::Market(c) if c.registered))
    }

    pub(crate) fn next_handle(&mut self) -> u64 {
        self.handles += 1;
        self.handles
    }

    /// `Garage_Sort` 0x4f0ee0.
    pub(crate) fn sort_garage(&mut self) {
        selection_swap(&mut self.garage.cars, |a, b| car_less(&a.record(), &b.record()));
    }

    /// `Garage_BuyDealerCar` 0x501cf0 for catalogue record `index` with the colour words of
    /// the dealer screen: price `Car_DealerPrice`, cash ≥ price (802), fewer than 80 cars (61);
    /// the car (price paid +0x6a4) joins the garage, the garage is sorted (`0x4f0ee0`), cash
    /// pays the price, the availability is updated. Plays sound 0xb.
    pub fn buy_dealer_car(
        &mut self,
        catalogue: &CarCatalogue,
        parts: &PartsCatalogue,
        index: usize,
        colours: [i32; 6],
        game_type: i32,
    ) -> Result<(), BuyRefusal> {
        let rec = &catalogue.records[index];
        let price = dealer_price(rec, self.year);
        if self.cash < price {
            return Err(BuyRefusal::NotEnoughCash);
        }
        if self.garage.cars.len() >= GARAGE_LIMIT {
            return Err(BuyRefusal::GarageFull);
        }
        let mut car = OwnedCar::new(rec, colours, parts, self.cars_alive());
        car.handle = self.next_handle();
        car.set_price_paid(price);
        self.garage.cars.push(car);
        self.sort_garage();
        self.events.push(CareerEvent::Sound(0xb));
        self.add_cash(price.wrapping_neg());
        self.update_available_cars(game_type);
        Ok(())
    }

    /// `Garage_AddPrizeCar` 0x502270: the catalogue car with this id (else record 0) joins the
    /// garage at the end, price paid 0, no sort, no garage limit.
    pub fn add_prize_car(
        &mut self,
        catalogue: &CarCatalogue,
        parts: &PartsCatalogue,
        car_id: i32,
        colours: [i32; 6],
        game_type: i32,
    ) {
        let rec = &catalogue.records[catalogue.find(car_id)];
        let mut car = OwnedCar::new(rec, colours, parts, self.cars_alive());
        car.handle = self.next_handle();
        car.set_price_paid(0);
        self.garage.cars.push(car);
        self.update_available_cars(game_type);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(first: i16, base: i32, values: &[f32]) -> CarRecord {
        let mut b = vec![0u8; CAR_RECORD_SIZE];
        b[0x1f8..0x1fa].copy_from_slice(&first.to_le_bytes());
        b[0x208..0x20c].copy_from_slice(&base.to_le_bytes());
        for (k, v) in values.iter().enumerate() {
            b[0x20c + 4 * k..0x210 + 4 * k].copy_from_slice(&v.to_le_bytes());
        }
        CarRecord(b)
    }

    #[test]
    fn dealer_price_depreciates_after_the_first_year() {
        let r = record(1950, 11000, &[100.0, 100.0, 100.0, 95.0, 93.0]);
        assert_eq!(dealer_price(&r, 1950), 11000);
        assert_eq!(dealer_price(&r, 1953), 10450);
        assert_eq!(dealer_price(&r, 1954), 10230);
    }

    #[test]
    fn new_car_layout() {
        let r = record(1950, 11000, &[]);
        let c = OwnedCar::new(&r, [1, 2, 3, 4, 5, 6], &PartsCatalogue::default(), false);
        assert_eq!((c.serial(), c.i32(0x674), c.i32(0x688), c.owned_flag()), (0, 1, 6, 1));
        assert_eq!((c.f32(0x6d0), c.f32(0x6e8), c.f32(0x6ec), c.f32(0x714 + 0x18)), (50.0, 1.0, 30.0, 1.0));
    }
}
