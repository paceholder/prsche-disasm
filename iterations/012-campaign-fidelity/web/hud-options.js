// Display options of the race HUD ("Anzeige", FEData/Layouts/display.lay): the screen layout
// (SCREENLAYOUT, race-config key 0x5b) and the 13 options of the selected config's hud2 record.
// The original keeps the options in the hud2 records and rewrites FEData/Data/nfs5-2.hud when the
// screen closes or the layout changes [VA 0x4cb820, 0x4cb860, 0x41ea50]; the page keeps the
// edits per config in localStorage instead (A85). Evidence: docs/re/hud.md "Layouts and options".
import { DISPLAY_OPTIONS, configOptions } from "./race-hud-core.js";

const STORAGE_KEY = "nfs5.hudDisplay";
const wrap = (i, n) => (n > 0 ? ((i % n) + n) % n : 0);

export class HudOptions {
  // storage: a localStorage-like object (or null); records: parseHudFiles() result (may be set later).
  constructor(storage = null, records = null) {
    this.storage = storage;
    this.records = records;
    this.layout = 0; // SCREENLAYOUT: fe.txt has no entry, so 0 = Config 1 [data fe.txt]
    this.configs = {}; // config index -> { option: value } edits
    this.version = 0;
    this.load();
  }

  load() {
    try {
      const raw = this.storage?.getItem(STORAGE_KEY);
      if (!raw) return;
      const v = JSON.parse(raw);
      if (Number.isInteger(v.layout) && v.layout >= 0 && v.layout < 4) this.layout = v.layout;
      if (v.configs && typeof v.configs === "object") this.configs = v.configs;
    } catch { /* keep defaults */ }
  }

  save() {
    this.version++;
    try { this.storage?.setItem(STORAGE_KEY, JSON.stringify({ layout: this.layout, configs: this.configs })); } catch { /* ignore */ }
  }

  // Options of config `index`: the shipped hud2 values with the saved edits on top.
  options(index = this.layout) {
    const base = this.records ? configOptions(this.records, index) : {};
    return { ...base, ...(this.configs[index] ?? {}) };
  }

  // Edits of config `index` (passed to selectConfig as overrides).
  overrides(index = this.layout) {
    return this.configs[index] ?? null;
  }

  value(entry) {
    if (!entry.option) return this.layout;
    return this.options()[entry.option] ?? 0;
  }

  set(entry, v) {
    if (!entry.option) this.layout = v;
    else this.configs[this.layout] = { ...(this.configs[this.layout] ?? {}), [entry.option]: v };
    this.save();
  }

  // Front-end accessor for `Display.<name>` (frontend.js host.accessor contract):
  // {items, index, text, select, step}; `str(id)` looks up festrings.
  accessor(name, str) {
    const entry = DISPLAY_OPTIONS.find(e => e.accessor.toLowerCase() === String(name || "").toLowerCase());
    if (!entry) return null;
    const items = () => entry.strings.map(id => str(id) ?? String(id));
    const n = entry.strings.length;
    return {
      items,
      index: () => Math.min(n - 1, Math.max(0, this.value(entry))),
      text: () => items()[Math.min(n - 1, Math.max(0, this.value(entry)))] ?? "",
      select: i => this.set(entry, wrap(i, n)),
      step: d => this.set(entry, wrap(this.value(entry) + d, n)),
    };
  }
}
