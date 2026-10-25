// Evolution event info (tournament.lay): the Tournament.* accessors over nfs5.trn records.
// Pure module, tested in frontend.test.mjs. Evidence: Tourn_DataAccessor [VA 0x506600],
// docs/re/career.md "Tournament record".

export const DIFFICULTY_BASE = 450;      // 450 + track record +0x78 [VA 0x506600]
export const ERA_CLASSIC = 0x209;

// Number text of FUN_004dbf80 + format 0x5ccb84: thousands separators ("12,000").
export function money(n) {
  return String(Math.trunc(n)).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

export class EventInfoSetup {
  // events: fe_event_infos_json(); tracks: fe_track_catalog_json() (all records); strings: id -> text.
  constructor({ events = [], tracks = [], strings = {}, era = ERA_CLASSIC } = {}) {
    this.strings = strings;
    this.tracks = tracks;
    // The list of the selected era: tournaments only (type 2 = end-of-era race is excluded).
    this.events = events.filter(e => e.era === era && e.record_type !== 2);
    this.index = 0;
  }

  str(id) { return this.strings[String(id)] ?? ""; }
  current() { return this.events[this.index] ?? null; }

  trackById(id) { return this.tracks.find(t => t.id === id) ?? null; }

  // The races' tracks in order; the first one stands for "remaining tracks" of a new event.
  locations() {
    const e = this.current();
    return (e?.races ?? []).map(r => this.trackById(r.track_id)).filter(Boolean);
  }

  // Direction index for the map: the first race's direction (forward 0, backward 1).
  direction() { return this.current()?.races?.[0]?.reverse ? 1 : 0; }

  // The panel lists the capital-letter names: string (name_string + 0x14) [VA 0x4fcfb5].
  locationNames() {
    return this.locations().map(t => this.str(t.name_string + 0x14));
  }

  // One entry per race for the spline control: small map pack, direction (race reverse flag,
  // nfs5.trn race +4 [VA 0x4fcdfc]) and name.
  raceViews() {
    const e = this.current();
    return (e?.races ?? []).map(r => {
      const t = this.trackById(r.track_id);
      return t ? { sm: t.small_pack, direction: r.reverse ? 1 : 0, name: this.str(t.name_string + 0x14) } : null;
    }).filter(Boolean);
  }

  accessor(name) {
    const m = /^Tournament\.Tournament(?:\(([A-Z]+)\))?$/.exec(name || "");
    if (!m) return null;
    const e = this.current();
    const sel = m[1];
    if (sel === "SHORT") {
      return {
        items: () => this.events.map(x => this.str(x.short_string)),
        index: () => this.index,
        text: () => this.str(e?.short_string),
        select: i => { this.index = Math.max(0, Math.min(this.events.length - 1, i)); },
        step: d => { this.index = (this.index + d + this.events.length) % this.events.length; },
      };
    }
    const text = () => {
      if (!e) return "";
      switch (sel) {
        case "DESCRIPTION": return this.str(e.desc_string);
        case "ENTRYFEE": return money(e.entry_fee);
        case "CARRESTRICTION": return this.str(e.restriction_string);
        case "PRIZEMONEY": return money(e.prize_money);
        case "TIMESCOMPLETED": return String(e.times_completed);
        case "DIFFICULTY": {
          const t = this.locations()[0];
          return t ? this.str(DIFFICULTY_BASE + t.difficulty) : "";
        }
        // No selector: the title (A221: the original's plain accessor is not in 0x506600).
        default: return this.str(e.title_string);
      }
    };
    return { text };
  }
}
