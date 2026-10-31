//! Raw records of the car catalogue `nfs5.car` (0x670 bytes) and the parts table `nfs5.prt`
//! (0x168 bytes), as the career code reads them. The original copies these records into the
//! owned-car objects and their part lists, so they are kept as bytes with typed accessors.
//! Field evidence: workspace `docs/formats/car-catalogue.md`, `docs/re/career.md`.

pub const CAR_RECORD_SIZE: usize = 0x670;
pub const PART_RECORD_SIZE: usize = 0x168;

fn i16_at(b: &[u8], o: usize) -> i16 {
    b.get(o..o + 2).map_or(0, |x| i16::from_le_bytes([x[0], x[1]]))
}
fn i32_at(b: &[u8], o: usize) -> i32 {
    b.get(o..o + 4).map_or(0, |x| i32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}
fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_bits(i32_at(b, o) as u32)
}
fn cstr_at(b: &[u8], o: usize, n: usize) -> &[u8] {
    let s = &b[o.min(b.len())..(o + n).min(b.len())];
    &s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())]
}

/// One `nfs5.car` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarRecord(pub Vec<u8>);

impl CarRecord {
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
    /// +0x00 car id.
    pub fn id(&self) -> i16 {
        i16_at(&self.0, 0x000)
    }
    /// +0x7f model (`.crp` base name), checked for presence by `Career_RebuildCarLists`.
    pub fn model(&self) -> &[u8] {
        cstr_at(&self.0, 0x07f, 0x32)
    }
    /// +0x1ac group / class string id (501.. "Coupé" ...), second sort key of 0x4f1d40.
    pub fn group(&self) -> i32 {
        i32_at(&self.0, 0x1ac)
    }
    /// +0x1b0 era string id (521..523), first sort key of 0x4f1d40.
    pub fn era(&self) -> i32 {
        i32_at(&self.0, 0x1b0)
    }
    /// +0x1f4: 0 = sold / listed; 1 and 2 go to the lists 0x6919fc / 0x6919f8 (traffic, cops).
    pub fn list_flag(&self) -> i16 {
        i16_at(&self.0, 0x1f4)
    }
    /// +0x1f8 first dealer (model) year.
    pub fn first_year(&self) -> i16 {
        i16_at(&self.0, 0x1f8)
    }
    /// +0x202 last dealer year.
    pub fn last_year(&self) -> i16 {
        i16_at(&self.0, 0x202)
    }
    /// +0x204 kind (class): 2 and 3 are never sold by the dealer.
    pub fn kind(&self) -> i16 {
        i16_at(&self.0, 0x204)
    }
    /// +0x208 base price.
    pub fn base_price(&self) -> i32 {
        i32_at(&self.0, 0x208)
    }
    /// +0x20c + 4·age: value percent by age (f32).
    pub fn value_percent(&self, age: i32) -> f32 {
        f32_at(&self.0, (0x20c + 4 * age as i64).max(0) as usize)
    }
    /// +0x32e count, +0x330 ids: parts the market offers for the car.
    pub fn market_parts(&self) -> Vec<i16> {
        let n = i16_at(&self.0, 0x32e).max(0) as usize;
        (0..n).map(|k| i16_at(&self.0, 0x330 + 2 * k)).collect()
    }
    /// +0x3bc count, +0x3be ids: the parts the car comes with.
    pub fn stock_parts(&self) -> Vec<i16> {
        let n = i16_at(&self.0, 0x3bc).max(0) as usize;
        (0..n).map(|k| i16_at(&self.0, 0x3be + 2 * k)).collect()
    }
}

/// One `nfs5.prt` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartRecord(pub Vec<u8>);

impl PartRecord {
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
    /// +0x00 part id.
    pub fn id(&self) -> i16 {
        i16_at(&self.0, 0x00)
    }
    /// +0x08 model text (car family).
    pub fn model(&self) -> &[u8] {
        cstr_at(&self.0, 0x08, 0x1e)
    }
    /// +0x26 category (1 engine … 10 body/rims).
    pub fn category(&self) -> i16 {
        i16_at(&self.0, 0x26)
    }
    /// +0x28 part name ("Engine (Standard)", "Brakes", ...).
    pub fn name(&self) -> &[u8] {
        cstr_at(&self.0, 0x28, 0x1e)
    }
    /// +0x46 year the part appears.
    pub fn year(&self) -> i16 {
        i16_at(&self.0, 0x46)
    }
    /// +0x5c base price.
    pub fn price(&self) -> i32 {
        i32_at(&self.0, 0x5c)
    }
    /// +0x164 wear / condition (f32), per installed copy.
    pub fn condition(&self) -> f32 {
        f32_at(&self.0, 0x164)
    }
    pub fn set_condition(&mut self, v: f32) {
        self.0[0x164..0x168].copy_from_slice(&v.to_le_bytes());
    }
}

/// `nfs5.car` as loaded (`[0x6919e8]`, all records in file order).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CarCatalogue {
    pub records: Vec<CarRecord>,
}

impl CarCatalogue {
    pub fn from_bytes(b: &[u8]) -> Self {
        CarCatalogue { records: b.chunks_exact(CAR_RECORD_SIZE).map(|c| CarRecord(c.to_vec())).collect() }
    }
    /// `0x4f4360`: index of the record with this id, else record 0.
    pub fn find(&self, id: i32) -> usize {
        self.records.iter().position(|r| r.id() as i32 == id).unwrap_or(0)
    }
}

/// `nfs5.prt` as loaded (`[0x628f84]`, count `[0x628f80]`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PartsCatalogue {
    pub records: Vec<PartRecord>,
}

impl PartsCatalogue {
    pub fn from_bytes(b: &[u8]) -> Self {
        PartsCatalogue { records: b.chunks_exact(PART_RECORD_SIZE).map(|c| PartRecord(c.to_vec())).collect() }
    }
    /// `0x49c470`: the record with this id (the original logs "Part %d not found." and
    /// returns NULL otherwise).
    pub fn find(&self, id: i32) -> Option<&PartRecord> {
        self.records.iter().find(|r| r.id() as i32 == id)
    }
}
