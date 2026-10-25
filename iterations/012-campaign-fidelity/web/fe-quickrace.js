// Quick Race settings behind QuickRace.lay and location.lay, with the original value lists
// (docs/re/frontend.md "Quick Race accessors"). Pure module: no DOM, tested in frontend.test.mjs.

// String ids [VA 0x4f9790 tables 0x5b9628 / 0x5b963c / 0x5b9650; VA 0x4f5720].
export const TRAFFIC_STRINGS = [403, 402, 401, 400];           // none, light, medium, heavy
export const DIRECTION_STRINGS = [410, 411, 412, 413];         // forward, backward, fwd/mirr, bwd/mirr
export const LAP_STRINGS = [420, 421, 422, 423, 424, 425, 426, 427]; // "1".."8"
export const OPPONENT_STRINGS = [640, 642, 643, 644, 641];     // same car, era, class, era/class, random
export const OPPONENT_SHORT_STRINGS = [640, 642, 643, 645, 641];
export const DIFFICULTY_BASE = 450;                            // beginner, advanced, expert
export const TRANSMISSION_STRINGS = [560, 561];                // manual, automatic [VA 0x5dab5c]
export const OFF_ON_STRINGS = [1500, 1501];                    // ABS, traction control [VA 0x5daa20, 0x5daa0c]
export const ROOF_STRINGS = [1509, 1510];                      // up (closed), down (open) [VA 0x5da9f0]
export const MAX_OPPONENTS = 7;                                // Game.NumOpponents 0..7 [VA 0x4e5520]

// Packed car colour (LE u32 of the bytes A, R, G, B in nfs5.car) -> CSS.
export function carColourCss(packed) {
  const v = packed >>> 0;
  return `rgb(${(v >>> 8) & 255},${(v >>> 16) & 255},${v >>> 24})`;
}

// Tracks offered in Quick Race [VA 0x4fbc10]: installed nfs5.trk records in file order, no
// Skidpad; records with unlock 3 need a finished career era (none outside the career).
// `all` lists every installed track, as the original does for GameType 3 (debugging aid).
export function quickRaceTracks(records, installed, all = false) {
  const have = new Set(installed.map(name => name.toLowerCase()));
  return records.filter(t => have.has(t.internal_name.toLowerCase())
    && (all || (t.internal_name.toLowerCase() !== "skidpad" && t.unlock !== 3)));
}

function wrap(i, n) { return n > 0 ? ((i % n) + n) % n : 0; }

export class QuickRaceSetup {
  // strings: id -> text; cars: quick-race car records (catalogue order already sorted);
  // tracks: quickRaceTracks(...) result.
  constructor({ strings = {}, cars = [], tracks = [] } = {}) {
    this.strings = strings;
    this.cars = cars;
    this.tracks = tracks;
    this.opponentMode = 0;
    this.opponents = 4;
    this.colour = 0;
    this.difficulty = 1; // "advanced" [VA 0x4e5520]
    // Car setup screen (settings.lay). The accessors live as long as the front end, so
    // SETTYPEQUICKRACE does not reset them. Selections at creation: transmission 1 (automatic)
    // [VA 0x4f1ea0], ABS/traction/roof 0, Game.Damage 1 [VA 0x4e5520].
    this.transmission = 1;
    this.abs = 0;
    this.traction = 0;
    this.roof = 0;
    this.damage = 1;
    this.reset();
  }

  str(id) { return this.strings[String(id)] ?? ""; }

  // SETTYPEQUICKRACE [VA 0x510c30, 0x4fc730]: car 0, track 0, traffic none, forward, laps index 1.
  reset() {
    this.car = 0;
    this.track = 0;
    this.traffic = 0;
    this.direction = 0;
    this.lapsIndex = 1;
  }

  currentCar() { return this.cars[this.car] ?? null; }
  currentTrack() { return this.tracks[this.track] ?? null; }

  // Accessor by layout name, e.g. "Track1.Track(SHORT)"; null when unknown.
  accessor(name) {
    const m = /^([A-Za-z0-9]+)\.([A-Za-z0-9]+)(?:\(([A-Z0-9]+)\))?$/.exec(name || "");
    if (!m) return null;
    const [, base, field, sel] = m;
    const key = `${base}.${field}`;
    const list = (items, get, set) => ({
      items: () => items(),
      index: () => get(),
      text: () => items()[get()] ?? "",
      select: i => set(wrap(i, items().length)),
      step: d => set(wrap(get() + d, items().length)),
    });
    switch (key) {
      case "zMergedList.Car":
      case "GenericCar.Car":
        // The merged list's (SHORT) reaches slot 0x30, the display name at rec +0x02
        // (0x4bc4d0 -> 0x4f4a90 -> 0x4f2fe0), not the short name at +0x34 (0x4f3020).
        return list(() => this.cars.map(c => c.display_name || c.short_name),
          () => this.car, i => { this.car = i; this.colour = 0; });
      case "GenericCar.Colour": {
        const colours = () => this.currentCar()?.colours ?? [];
        return { ...list(() => colours().map(() => ""), () => this.colour, i => { this.colour = i; }),
          color: () => carColourCss(colours()[this.colour]?.[0] ?? 0),
          colorAt: i => carColourCss(colours()[i]?.[0] ?? 0) };
      }
      case "Track1.Track":
      case "LoopedTrack.Track": {
        const label = t => {
          if (sel === "SHORT") return this.str(t.name_string + 0x28);
          if (sel === "DIFFICULTY") return this.str(DIFFICULTY_BASE + t.difficulty);
          if (sel === "RECORDTIME") return "";   // no track records yet (A100)
          return this.str(t.name_string);
        };
        return list(() => this.tracks.map(label), () => this.track, i => { this.track = i; });
      }
      case "Track1.Direction":
        return list(() => DIRECTION_STRINGS.map(id => this.str(id)), () => this.direction, i => { this.direction = i; });
      case "Track1.Traffic":
        return list(() => TRAFFIC_STRINGS.map(id => this.str(id)), () => this.traffic, i => { this.traffic = i; });
      case "Track1.Laps":
        return list(() => LAP_STRINGS.map(id => this.str(id)), () => this.lapsIndex, i => { this.lapsIndex = i; });
      case "OpponentCar.Car":
        return list(() => (sel === "SHORT" ? OPPONENT_SHORT_STRINGS : OPPONENT_STRINGS).map(id => this.str(id)),
          () => this.opponentMode, i => { this.opponentMode = i; });
      case "Game.Difficutly": // sic, the original accessor name [VA 0x4e5520]
        return list(() => [0, 1, 2].map(i => this.str(DIFFICULTY_BASE + i)), () => this.difficulty, i => { this.difficulty = i; });
      case "Car.Transmission":
        return list(() => TRANSMISSION_STRINGS.map(id => this.str(id)), () => this.transmission, i => { this.transmission = i; });
      case "Car.ABS":
        return list(() => OFF_ON_STRINGS.map(id => this.str(id)), () => this.abs, i => { this.abs = i; });
      case "Car.Traction":
        return list(() => OFF_ON_STRINGS.map(id => this.str(id)), () => this.traction, i => { this.traction = i; });
      case "Car.Roof":
        return list(() => ROOF_STRINGS.map(id => this.str(id)), () => this.roof, i => { this.roof = i; });
      case "Game.Damage": // FEBoolAccessor; off/on text is a hypothesis (A130)
        return list(() => OFF_ON_STRINGS.map(id => this.str(id)), () => this.damage, i => { this.damage = i; });
      case "Game.NumOpponents":
        return list(() => Array.from({ length: MAX_OPPONENTS + 1 }, (_, i) => String(i)),
          () => this.opponents, i => { this.opponents = i; });
      default:
        return null;
    }
  }

  // TRACK1INC / TRACK1DEC [VA 0x50dd80, 0x50ddc0].
  stepTrack(d) { this.track = wrap(this.track + d, this.tracks.length); }

  // Race_GetSetup for GameType 0 [VA 0x4fc0b0]: laps only on circuits, direction -> flags.
  raceParams() {
    const track = this.currentTrack();
    return {
      car: this.currentCar(),
      colour: this.colour,
      track,
      laps: track?.circuit ? this.lapsIndex + 1 : 1,
      reverse: this.direction === 1 || this.direction === 3,
      mirror: this.direction >= 2,
      traffic: this.traffic,
      opponents: this.opponents,
      opponentMode: this.opponentMode,
      // FE_WriteCarConfig 0x4eeda0 writes ABS 0, traction 0 and roof 0 for game type 0, so only
      // the transmission and the damage option reach the race.
      manualBox: this.transmission === 0,
      damage: this.damage === 1,
    };
  }
}
