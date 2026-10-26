import init, { create_viewer, EngineAudio, TyreAudio, MusicTrack, MusicDirector, RaceAnnouncer, engine_bank_for_sim, shell_profile_new, shell_profile_buy_356, shell_apply_event_result, shell_get_first_evolution_event, shell_get_first_factory_event, shell_get_dealership_catalog, parse_car_catalogue_json, shell_get_used_cars, shell_get_parts_catalog, shell_get_tournament_cups, shell_buy_car, shell_sell_car, shell_buy_part, shell_repair_car, shell_get_factory_missions, shell_complete_factory_mission, fe_parse_layout, fe_decode_fsh, fe_parse_font, fe_parse_strings, fe_track_catalog_json, fe_event_infos_json, fe_quick_race_cars_json, fe_pick_opponents_json } from "./package/viewer_impl.js";
import { carClickZone, carPartMs, ZONE_CHANNEL } from "./fe-car-click.js";
import { PauseMenu, drawPauseMenu, CODE as PAUSE_CODE } from "./pause-menu.js";

// A lap finish or elapsed time alone cannot award a Factory mission.
function factoryGoalSatisfied(driver, elapsed, timeLimit) {
  return Number.isFinite(elapsed) && elapsed > 0
    && Number.isFinite(timeLimit) && timeLimit > 0 && elapsed <= timeLimit
    && typeof driver?.get_mission_goal_supported === "function"
    && driver.get_mission_goal_supported() === true
    && typeof driver.get_mission_goal_reached === "function"
    && driver.get_mission_goal_reached() === true;
}

// The authoritative evaluation (including penalties) must precede pass audio/UI.
function evaluateFactoryResult(driver, event, profile, elapsed, position, evaluate) {
  if (!factoryGoalSatisfied(driver, elapsed, event.time_limit) || !profile
      || !Number.isInteger(event.mission_index) || event.mission_index < 1
      || typeof evaluate !== "function") return null;
  const result = JSON.parse(evaluate(JSON.stringify(profile), event.mission_index, elapsed,
    driver.get_stunt_180?.() ?? false, driver.get_stunt_360?.() ?? false,
    driver.get_stunt_jturn?.() ?? false, driver.get_cone_hits?.() ?? 0,
    0.0, position === 1));
  if (typeof result.passed !== "boolean" || !result.profile) {
    throw new Error("Invalid Factory result");
  }
  return result;
}

const KNOWN_TRACKS = [
  { id: "skidpad", name: "Skidpad (Полигон)" },
  { id: "alps", name: "Alps (Альпы)" },
  { id: "autobahn", name: "Autobahn (Автобан)" },
  { id: "canyon", name: "Corsica (Корсика)" },
  { id: "castle", name: "Auvergne (Овернь)" },
  { id: "coastal", name: "Côte d'Azur (Лазурный берег)" },
  { id: "farmland", name: "Normandie (Нормандия)" },
  { id: "foothills", name: "Pyrenees (Пиренеи)" },
  { id: "forest", name: "Schwarzwald (Шварцвальд)" },
  { id: "industrial", name: "Zone Industrielle (Промзона)" },
  { id: "monaco1", name: "Monte Carlo 1 (Монте-Карло 1)" },
  { id: "monaco2", name: "Monte Carlo 2 (Монте-Карло 2)" },
  { id: "monaco3", name: "Monte Carlo 3 (Монте-Карло 3)" },
  { id: "monaco4", name: "Monte Carlo 4 (Монте-Карло 4)" },
  { id: "monaco5", name: "Monte Carlo 5 (Монте-Карло 5)" },
];

const KNOWN_CARS = [
  { id: "356_1", name: "356 \"No. 1\" Roadster (1948)" },
  { id: "356a", name: "356 A 1600 Coupe (1956)" },
  { id: "356b", name: "356 B 2000 GS Carrera 2 (1960)" },
  { id: "550", name: "550 A Spyder (1956)" },
  { id: "901", name: "911 Coupe (901) (1964)" },
  { id: "914", name: "914/4 (1970)" },
  { id: "928", name: "928 (1978)" },
  { id: "930", name: "911 Turbo 3.3 (930) (1978)" },
  { id: "935", name: "935/78 \"Moby Dick\" (1978)" },
  { id: "944", name: "944 (1982)" },
  { id: "959", name: "959 (1987)" },
  { id: "964", name: "911 Carrera 2 (964) (1989)" },
  { id: "993", name: "911 Carrera (993) (1995)" },
  { id: "996", name: "911 Carrera (996) (1998)" },
  { id: "boxster", name: "Boxster (986) (1997)" },
  { id: "gt1", name: "911 GT1 Straßenversion (1998)" },
  { id: "gt2", name: "911 GT2 (993) (1996)" },
  { id: "gt3", name: "911 GT3 (996) (1999)" },
  { id: "cop356", name: "Полиция: 356" },
  { id: "cop356_german", name: "Polizei: 356" },
  { id: "cop930", name: "Полиция: 930 Turbo" },
  { id: "cop930_german", name: "Polizei: 930" },
  { id: "cop993", name: "Полиция: 993" },
  { id: "cop993_german", name: "Polizei: 993" },
];

const canvas = document.querySelector("#viewport");
const folderInput = document.querySelector("#folderInput");
const fileInput = document.querySelector("#fileInput");
const sourceBadge = document.querySelector("#sourceBadge");
const resetSourceButton = document.querySelector("#resetSourceButton");
const modeSelect = document.querySelector("#modeSelect");
const targetLabel = document.querySelector("#targetLabel");
const targetSelect = document.querySelector("#targetSelect");
const topologyField = document.querySelector("#topologyField");
const topologyCheckbox = document.querySelector("#topologyCheckbox");
const tourButton = document.querySelector("#tourButton");
const cameraButton = document.querySelector("#cameraButton");

function cycleCarCamera() {
  const name = viewer.cycle_car_view();
  setStatus(`Камера: ${name}`);
}
const paintButton = document.querySelector("#paintButton");
const hudCanvas = document.querySelector("#hudCanvas");
const pauseCanvas = document.querySelector("#pauseCanvas");
const loadButton = document.querySelector("#loadButton");
const resetButton = document.querySelector("#resetButton");
const statusLine = document.querySelector("#status");
const summary = document.querySelector("#summary");
const menuButton = document.querySelector("#menuButton");
const hudButton = document.querySelector("#hudButton");
const detailsButton = document.querySelector("#detailsButton");
const facStatusBadge = document.querySelector("#facStatusBadge");
const facStatusLabel = document.querySelector("#facStatusLabel");
const facStatusVal = document.querySelector("#facStatusVal");
const facStatusSub = document.querySelector("#facStatusSub");
const countdownBanner = document.querySelector("#countdownBanner");
const countdownText = document.querySelector("#countdownText");
const wrongWayBanner = document.querySelector("#wrongWayBanner");
const raceResultsModal = document.querySelector("#raceResultsModal");
const resultsSubtitle = document.querySelector("#resultsSubtitle");
const resultsLapTime = document.querySelector("#resultsLapTime");
const resultsBestTime = document.querySelector("#resultsBestTime");
const raceRestartBtn = document.querySelector("#raceRestartBtn");

let menuVisible = true;
let hudVisible = true;
// Debug panel (#debugPanel): live state of the debug keys; Backquote toggles, the choice is remembered.
let debugPanelVisible = true;
try { debugPanelVisible = localStorage.getItem("nfs5.debugPanel") !== "0"; } catch {}
let raceHud = null; // original 2D HUD (race-hud.js), created on first use
let raceHudLoading = false;
let raceHudCarIndex = null;
let lightsOn = false;
let lastPhase = 0;

function formatRaceTime(secs) {
  if (!Number.isFinite(secs) || secs <= 0) return "--:--.-";
  const mins = Math.floor(secs / 60);
  const remainingSecs = (secs % 60).toFixed(1);
  return `${String(mins).padStart(2, "0")}:${remainingSecs.padStart(4, "0")}`;
}

if (raceRestartBtn) {
  raceRestartBtn.addEventListener("click", () => {
    if (viewer) {
      viewer.restart_race();
      if (raceResultsModal) raceResultsModal.hidden = true;
      eventFinishedHandled = false;
      setStatus("Гонка перезапущена / старт на решётке.");
      queueFrame();
    }
  });
}

// --- WebAudio Synthesizer (009 Sound) ---
// Original engine sound: the wasm `EngineAudio` runs the recovered mixer at 100 Hz and returns
// voice commands; this class plays them with Web Audio (docs/re/audio.md, docs/formats/bnk.md).
// Approximations (docs/approximations.md): gain = volume / 127, pan = pan / 0x8000.
const ENGINE_CMD_STRIDE = 7;
const ENGINE_SMOOTH_S = 0.005;

class EngineVoices {
  constructor() {
    this.files = null; // { names, bytes, simName, bank } of the loaded race car
    this.audio = null; // wasm EngineAudio
    this.slots = new Map(); // slot id -> { rate, detune, loopStart, loopEnd, channels: Float32Array[] }
    this.buffers = new Map(); // slot id -> AudioBuffer (per AudioContext)
    this.voices = new Map(); // voice id -> { src, gain, panner }
    this.active = false;
    this.bus = null;
    this.busCtx = null;
  }

  free() {
    if (this.audio) { try { this.audio.free(); } catch {} }
    this.audio = null;
    this.slots.clear();
    this.buffers.clear();
  }

  // Parses the bank and .spl now; throws with a readable reason on failure.
  load(names, bytes, simName, bank) {
    this.deactivate();
    this.free();
    this.files = null;
    const audio = EngineAudio.load(names, bytes, simName, bank);
    this.adopt(audio);
    this.files = { names, bytes, simName, bank };
  }

  // Takes the decoded bank slots of a wasm audio object (EngineAudio, TyreAudio).
  adopt(audio) {
    for (let i = 0; i < audio.slot_count(); i++) {
      const channels = [];
      for (let c = 0; c < audio.slot_channels(i); c++) channels.push(audio.slot_pcm(i, c));
      const ls = audio.slot_loop_start(i);
      const le = audio.slot_loop_end(i);
      this.slots.set(audio.slot_id(i), {
        rate: audio.slot_sample_rate(i),
        detune: audio.slot_detune_cents(i),
        loopStart: ls >= 0 && le > ls ? ls : -1,
        loopEnd: le,
        channels,
      });
    }
    this.audio = audio;
  }

  unload() {
    this.deactivate();
    this.free();
    this.files = null;
  }

  get loaded() {
    return this.audio !== null;
  }

  buffer(slot, ctx) {
    let buf = this.buffers.get(slot);
    if (buf && buf.__ctx === ctx) return buf;
    const s = this.slots.get(slot);
    if (!s || !s.channels.length || !s.channels[0].length) return null;
    buf = ctx.createBuffer(s.channels.length, s.channels[0].length, s.rate);
    s.channels.forEach((pcm, c) => buf.copyToChannel(pcm, c));
    buf.__ctx = ctx;
    this.buffers.set(slot, buf);
    return buf;
  }

  // Fresh mixer state: replays the start sample, like the original after EngineAudio_Load.
  activate() {
    if (!this.audio) return;
    this.deactivate();
    this.audio.set_running(false);
    this.active = true;
  }

  deactivate() {
    for (const v of this.voices.values()) this.disposeVoice(v);
    this.voices.clear();
    this.active = false;
  }

  disposeVoice(v) {
    try { v.src.onended = null; v.src.stop(); } catch {}
    try { v.src.disconnect(); v.gain.disconnect(); v.panner.disconnect(); } catch {}
  }

  update(dt, rpm, redline, throttle, view, gear) {
    const ctx = soundManager.ctx;
    if (!this.audio || !ctx || !soundManager.enabled) return;
    try {
      if (!this.active) this.activate();
      if (ctx.state === "suspended") ctx.resume().catch(() => {});
      const out = this.audio.update(dt, rpm, redline, throttle, view, gear);
      for (let i = 0; i + ENGINE_CMD_STRIDE <= out.length; i += ENGINE_CMD_STRIDE) {
        this.execute(ctx, out[i], out[i + 1], out[i + 2], out[i + 3], out[i + 4], out[i + 5], out[i + 6]);
      }
    } catch (error) {
      // An audio error must not stop the frame loop: drop engine sound and report it.
      console.error("Engine sound stopped:", error);
      this.unload();
      if (typeof setStatus === "function") setStatus(`Звук двигателя отключён: ${error && error.message ? error.message : error}`);
    }
  }

  execute(ctx, kind, voice, slot, volume, pitch, pan, looped) {
    const now = ctx.currentTime;
    const v = this.voices.get(voice);
    switch (kind) {
      case 0: { // Start
        const s = this.slots.get(slot);
        const buffer = this.buffer(slot, ctx);
        if (!s || !buffer) return;
        if (this.busCtx !== ctx) {
          this.bus = ctx.createGain();
          this.bus.connect(ctx.destination);
          this.busCtx = ctx;
        }
        const src = ctx.createBufferSource();
        src.buffer = buffer;
        if (looped) {
          src.loop = true;
          if (s.loopStart >= 0) {
            src.loopStart = s.loopStart / s.rate;
            src.loopEnd = s.loopEnd / s.rate;
          }
        }
        src.playbackRate.value = (2 ** (s.detune / 1200)) * (pitch / 4096);
        const gain = ctx.createGain();
        gain.gain.value = volume / 127;
        const panner = ctx.createStereoPanner();
        panner.pan.value = Math.max(-1, Math.min(1, pan / 0x8000));
        src.connect(gain);
        gain.connect(panner);
        panner.connect(this.bus);
        const entry = { src, gain, panner, detune: s.detune };
        src.onended = () => {
          if (this.voices.get(voice) === entry) this.voices.delete(voice);
          try { src.disconnect(); gain.disconnect(); panner.disconnect(); } catch {}
        };
        const old = this.voices.get(voice);
        if (old) this.disposeVoice(old);
        this.voices.set(voice, entry);
        src.start();
        return;
      }
      case 1: // SetVolume
        if (v) v.gain.gain.setTargetAtTime(volume / 127, now, ENGINE_SMOOTH_S);
        return;
      case 2: // SetPan
        if (v) v.panner.pan.setTargetAtTime(Math.max(-1, Math.min(1, pan / 0x8000)), now, ENGINE_SMOOTH_S);
        return;
      case 3: // SetPitch
        if (v) v.src.playbackRate.setTargetAtTime((2 ** (v.detune / 1200)) * (pitch / 4096), now, ENGINE_SMOOTH_S);
        return;
      case 4: // Stop
        if (v) {
          this.disposeVoice(v);
          this.voices.delete(voice);
        }
        return;
    }
  }
}

const engineSound = new EngineVoices();

// Original tyre squeal and surface rumble (docs/re/audio.md, "Tyre and surface sounds"): the wasm
// `TyreAudio` runs the recovered logic at 32 Hz on the car state of the frame; the voices are the
// looped SND_coll.bnk slots. A Stop carries the original's fade in ticks, played at 100 Hz
// (approximation A110).
const TYRE_FADE_TICK_S = 0.01;
const TYRE_LOG_LIMIT = 40;
const TYRE_CHANNELS = ["front", "rear", "rumble"];

class TyreVoices extends EngineVoices {
  constructor() {
    super();
    this.lastInputs = [];
    this.log = []; // newest last: { t, kind, slot, volume }
    this.starts = 0;
  }

  load(names, bytes) {
    this.deactivate();
    this.free();
    this.files = null;
    this.adopt(TyreAudio.load(names, bytes));
    this.files = { names, bytes };
  }

  deactivate() {
    if (this.audio && this.active) { try { this.audio.silence(); } catch {} }
    super.deactivate();
  }

  update(dt, inputs, view) {
    const ctx = soundManager.ctx;
    if (!this.audio || !ctx || !soundManager.enabled) return;
    try {
      if (!this.active) { this.deactivate(); this.active = true; }
      if (ctx.state === "suspended") ctx.resume().catch(() => {});
      this.lastInputs = Array.from(inputs);
      const out = this.audio.update(dt, inputs, view);
      for (let i = 0; i + ENGINE_CMD_STRIDE <= out.length; i += ENGINE_CMD_STRIDE) {
        const [kind, voice, slot, volume, pitch, fade, looped] = out.slice(i, i + ENGINE_CMD_STRIDE);
        if (kind === 0 || kind === 4) {
          this.log.push({ t: Math.round(performance.now()), kind: kind === 0 ? "start" : "stop", slot, volume });
          if (kind === 0) this.starts++;
          if (this.log.length > TYRE_LOG_LIMIT) this.log.shift();
        }
        this.execute(ctx, kind, voice, slot, volume, pitch, fade, looped);
      }
    } catch (error) {
      console.error("Tyre sound stopped:", error);
      this.unload();
    }
  }

  // Stop fades out over `fade` ticks; the other commands are the engine's (pan unused).
  execute(ctx, kind, voice, slot, volume, pitch, fade, looped) {
    if (kind !== 4) return super.execute(ctx, kind, voice, slot, volume, pitch, 0, looped);
    const v = this.voices.get(voice);
    if (!v) return;
    this.voices.delete(voice);
    const now = ctx.currentTime;
    const end = now + Math.max(fade, 1) * TYRE_FADE_TICK_S;
    try {
      v.gain.gain.cancelScheduledValues(now);
      v.gain.gain.setValueAtTime(v.gain.gain.value, now);
      v.gain.gain.linearRampToValueAtTime(0, end);
      v.src.onended = () => { try { v.src.disconnect(); v.gain.disconnect(); v.panner.disconnect(); } catch {} };
      v.src.stop(end + 0.02);
    } catch { this.disposeVoice(v); }
  }

  // Readout for `__nfs.state().audio.tyres`.
  readout() {
    if (!this.audio) return { loaded: false };
    const s = Array.from(this.audio.state());
    const voices = [];
    for (let i = 0; i + 5 <= s.length - 2; i += 5) {
      voices.push({ channel: TYRE_CHANNELS[s[i]], slot: s[i + 1], volume: s[i + 2], control: s[i + 3], pitch: s[i + 4] });
    }
    const i = this.lastInputs;
    return {
      loaded: true, active: this.active, ticks: s[s.length - 2], starts: s[s.length - 1], voices,
      skidFront: [i[0], i[1]], skidRear: [i[2], i[3]], surface: i[7], speed: i[11],
      log: this.log.slice(-20),
    };
  }
}

const tyreSound = new TyreVoices();

class SoundManager {
  constructor() {
    this.ctx = null;
    this.enabled = true; // on by default; the AudioContext starts on the first user gesture
  }

  toggle() {
    this.enabled = !this.enabled;
    gameAudio.setEnabled(this.enabled);
    if (this.enabled) {
      this.init();
    } else {
      this.stop();
    }
    return this.enabled;
  }

  init() {
    if (!this.ctx && typeof window !== "undefined" && (window.AudioContext || window.webkitAudioContext)) {
      try {
        const AudioCtx = window.AudioContext || window.webkitAudioContext;
        this.ctx = new AudioCtx();
      } catch {}
    }
    if (this.ctx && this.ctx.state === "suspended") {
      this.ctx.resume().catch(() => {});
    }
  }

  stop() {
    engineSound.deactivate();
    tyreSound.deactivate();
  }

  playCue(freq, durationMs) {
    if (!this.enabled || !this.ctx) return;
    try {
      if (this.ctx.state === "suspended") this.ctx.resume().catch(() => {});
      const osc = this.ctx.createOscillator();
      const gain = this.ctx.createGain();
      osc.type = "sine";
      osc.frequency.setValueAtTime(freq, this.ctx.currentTime);
      gain.gain.setValueAtTime(0.15, this.ctx.currentTime);
      gain.gain.exponentialRampToValueAtTime(0.001, this.ctx.currentTime + durationMs / 1000);
      osc.connect(gain);
      gain.connect(this.ctx.destination);
      osc.start();
      osc.stop(this.ctx.currentTime + durationMs / 1000);
    } catch {}
  }

  playPass() {
    this.playCue(523.25, 120);
    setTimeout(() => this.playCue(659.25, 120), 100);
    setTimeout(() => this.playCue(783.99, 250), 200);
  }

  playFail() {
    this.playCue(240, 150);
    setTimeout(() => this.playCue(180, 300), 120);
  }
}

const soundManager = new SoundManager();

// --- Original music and race speech (docs/re/music.md, docs/re/audio.md "race speech") ---
// The wasm MusicDirector runs the original player (Music_Play 0x4ae4d0, Music_Tick 0x4ad9c0);
// MusicStream executes its stream commands with Web Audio. RaceAnnouncer picks the original
// phrases (countdown 0x414ff0, laps 0x4277d0, result 0x41d050); playSpeech plays them.
// Approximations (docs/approximations.md): A50 gain law, A51 tick rate, A52 stream buffering,
// A53 phrase loading, A54 speech inputs, A56 front-end mapping.
const MUSIC_CHUNK_FRAMES = 64000; // 2 s at 32 kHz per decode call
const MUSIC_AHEAD_S = 4;

class MusicStream {
  constructor(owner) {
    this.owner = owner;
    this.gen = 0;
    this.file = -1;
    this.loading = false;
    this.track = null;
    this.sources = [];
    this.nextTime = 0;
    this.ramp = null; // VolumeRamp (game-audio.js)
    this.gain = null;
    this.gainCtx = null;
  }
  get playing() {
    return this.loading || this.track !== null || this.sources.length > 0;
  }
  // One clock for the volume ramp: the AudioContext only exists after the first gesture, and
  // switching from page time to its clock would read the ramp before its start (silence).
  now() { return performance.now() / 1000; }
  volume() { return this.ramp ? this.ramp.value(this.now()) : 0; }
  node(ctx) {
    if (this.gainCtx !== ctx) {
      this.gain = ctx.createGain();
      this.gain.gain.value = this.volume() / 127;
      this.gain.connect(this.owner.musicBus(ctx));
      this.gainCtx = ctx;
    }
    return this.gain;
  }
  setVolume(v) {
    this.ramp.set(v, this.now());
    const ctx = soundManager.ctx;
    if (ctx && this.gainCtx === ctx) {
      this.gain.gain.cancelScheduledValues(ctx.currentTime);
      this.gain.gain.setValueAtTime(v / 127, ctx.currentTime);
    }
  }
  fade(ms, v) {
    const t = this.now();
    const from = this.volume();
    this.ramp.fade(ms, v, t);
    const ctx = soundManager.ctx;
    if (ctx && this.gainCtx === ctx) {
      const g = this.gain.gain;
      g.cancelScheduledValues(ctx.currentTime);
      g.setValueAtTime(from / 127, ctx.currentTime);
      g.linearRampToValueAtTime(v / 127, ctx.currentTime + Math.max(ms, 1) / 1000);
    }
  }
  stop() {
    this.gen++;
    this.loading = false;
    this.track?.free?.();
    this.track = null;
    for (const s of this.sources) { try { s.onended = null; s.stop(); s.disconnect(); } catch {} }
    this.sources = [];
    this.file = -1;
  }
  async start(file) {
    this.stop();
    const gen = this.gen;
    this.file = file;
    this.loading = true;
    try {
      const bytes = await this.owner.readMusicFile(file);
      if (gen !== this.gen) return;
      this.track = new MusicTrack(bytes);
      this.nextTime = 0;
    } catch (error) {
      console.warn("Music track unavailable:", error);
    } finally {
      if (gen === this.gen) this.loading = false;
    }
    this.pump();
  }
  // Keeps MUSIC_AHEAD_S of decoded audio queued on the context clock (A52).
  pump() {
    const ctx = soundManager.ctx;
    if (!this.track || !ctx || ctx.state !== "running") return;
    const out = this.node(ctx);
    if (this.nextTime < ctx.currentTime) this.nextTime = ctx.currentTime + 0.05;
    while (this.track && this.nextTime - ctx.currentTime < MUSIC_AHEAD_S) {
      const ch = this.track.channels();
      const pcm = this.track.decode(MUSIC_CHUNK_FRAMES);
      const frames = pcm.length / ch;
      if (frames > 0) {
        const buf = ctx.createBuffer(ch, frames, this.track.sample_rate());
        for (let c = 0; c < ch; c++) buf.copyToChannel(pcm.subarray(c * frames, (c + 1) * frames), c);
        const src = ctx.createBufferSource();
        src.buffer = buf;
        src.connect(out);
        src.start(this.nextTime);
        this.nextTime += frames / this.track.sample_rate();
        this.sources.push(src);
        src.onended = () => {
          this.sources = this.sources.filter(s => s !== src);
          try { src.disconnect(); } catch {}
        };
      }
      if (this.track.done()) { this.track.free?.(); this.track = null; }
    }
  }
}

class GameAudio {
  constructor() {
    this.ready = false;
    this.director = null;
    this.stream = new MusicStream(this);
    this.musicToc = null;
    this.musicPath = null;
    this.musicWhole = null; // archive bytes when the server ignores ranges
    this.fe = null;
    this.level = 127;
    this.screen = null; // ScreenMusic
    this.inRace = false;
    this.events = null; // RaceEvents
    this.helpers = null;
    this.announcer = null;
    this.speechPath = null;
    this.speechLoading = null;
    this.speechSource = null;
    this.speechEnd = 0;
    this.speechBuffers = new Map();
    this.bus = null;
    this.busCtx = null;
    this.enabled = true;
    this.log = [];
  }

  record(kind, detail) {
    this.log.push({ t: Math.round(performance.now()), kind, ...detail });
    if (this.log.length > 200) this.log.shift();
  }

  musicBus(ctx) {
    if (this.busCtx !== ctx) {
      this.bus = ctx.createGain();
      this.bus.gain.value = this.enabled ? 1 : 0;
      this.bus.connect(ctx.destination);
      this.busCtx = ctx;
    }
    return this.bus;
  }

  setEnabled(on) {
    this.enabled = on;
    if (this.bus && this.busCtx) this.bus.gain.setValueAtTime(on ? 1 : 0, this.busCtx.currentTime);
    if (!on && this.speechSource) { try { this.speechSource.stop(); } catch {} }
  }

  async init(files) {
    try {
      this.helpers = await import("./game-audio.js");
      const find = re => files.find(p => re.test(p.replace(/\\/g, "/")));
      this.musicPath = find(/(^|\/)gamedata\/music\/zzzymus\.viv$/i);
      this.speechPath = find(/(^|\/)gamedata\/speech\/zzzxeng\.viv$/i);
      const fePath = find(/^fe\.txt$/i);
      const feText = fePath ? await (await fetch("/game/" + fePath)).text() : "";
      this.fe = this.helpers.parseFeAudio(feText);
      let stored = NaN;
      try { stored = parseInt(localStorage.getItem("nfs5.musicLevel") ?? "", 10); } catch {}
      this.level = Number.isFinite(stored) ? Math.max(0, Math.min(127, stored)) : this.fe.musicLevel;
      this.syncSlider();
      this.screen = new this.helpers.ScreenMusic();
      this.events = new this.helpers.RaceEvents();
      this.stream.ramp = new this.helpers.VolumeRamp();
      if (!this.musicPath) throw new Error("GameData/Music/zzzymus.viv not found");
      this.musicToc = this.helpers.parseVivToc(await this.readRange(this.musicPath, 0, 65535));
      this.director = new MusicDirector(this.musicToc.map(e => e.name), this.level, (Date.now() >>> 0) || 1);
      this.director.set_random_playlist(this.fe.randomPlaylist);
      this.ready = true;
      setInterval(() => this.tick(), this.helpers.MUSIC_TICK_MS);
    } catch (error) {
      console.warn("Music unavailable:", error);
    }
  }

  async readRange(path, start, end) {
    const resp = await fetch("/game/" + path.replace(/\\/g, "/"), { headers: { Range: `bytes=${start}-${end}` } });
    if (!resp.ok) throw new Error(`${path}: HTTP ${resp.status}`);
    const data = new Uint8Array(await resp.arrayBuffer());
    if (resp.status === 206) return data;
    this.musicWhole = data; // no range support: keep the whole archive
    return data.subarray(start, end + 1);
  }

  async readMusicFile(file) {
    const e = this.musicToc[file];
    if (!e) throw new Error(`music file ${file}`);
    if (this.musicWhole) return this.musicWhole.slice(e.offset, e.offset + e.size);
    return this.readRange(this.musicPath, e.offset, e.offset + e.size - 1);
  }

  execute() {
    const c = this.director.take_commands();
    for (let i = 0; i + 3 <= c.length; i += 3) {
      const [kind, a, b] = [c[i], c[i + 1], c[i + 2]];
      if (kind === 0) {
        this.record("music-start", { file: a, name: this.musicToc[a]?.name });
        this.stream.start(a);
      } else if (kind === 1) this.stream.setVolume(a);
      else if (kind === 2) this.stream.fade(a, b);
      else if (kind === 3) { this.record("music-stop", {}); this.stream.stop(); }
    }
  }

  // Which music the original would ask for now (A56: the web shell counts as `menu*`).
  context() {
    const phase = viewer?.get_race_phase ? viewer.get_race_phase() : 0;
    const race = Boolean(viewer && viewer.is_drive_mode?.() && phase > 0 && !frontEnd?.visible);
    return { race, phase };
  }

  tick() {
    if (!this.ready) return;
    const d = this.director;
    const { race } = this.context();
    d.set_status(this.stream.playing, this.stream.volume());
    if (race && !this.inRace) {
      // Audio_RaceStart 0x4a88a0: fade the menu music, start the race playlist.
      d.set_front_end(false);
      d.stop(this.helpers.SCREEN_FADE_MS);
      d.set_playlist(Int32Array.from(this.fe.playlist));
      d.play("", 0);
      this.screen.reset();
    } else if (!race && this.inRace) {
      d.stop(0); // Audio_StopRace 0x4ab270
    }
    this.inRace = race;
    if (!race) {
      d.set_front_end(true);
      const attrs = frontEnd?.visible ? frontEnd.screen?.screenAttrs : null;
      if (!frontEnd?.visible || attrs) this.screen.update(this.helpers.screenMusicPattern(attrs), d);
    }
    d.tick(this.stream.playing, this.stream.volume());
    this.execute();
    this.stream.pump();
  }

  setLevel(level) {
    this.level = Math.max(0, Math.min(127, level | 0));
    try { localStorage.setItem("nfs5.musicLevel", String(this.level)); } catch {}
    if (!this.director) return;
    this.director.set_status(this.stream.playing, this.stream.volume());
    this.director.set_level(this.level); // Music_SetVolume(80 × level >> 7) 0x4ae8c0
    this.execute();
  }

  syncSlider() {
    const s = document.querySelector("#musicLevel");
    if (s) s.value = String(this.level);
  }

  // --- race speech ---
  loadSpeech() {
    if (this.announcer || this.speechLoading || !this.speechPath) return this.speechLoading;
    this.speechLoading = (async () => {
      try {
        const resp = await fetch("/game/" + this.speechPath.replace(/\\/g, "/"));
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        const bytes = new Uint8Array(await resp.arrayBuffer());
        this.announcer = RaceAnnouncer.load(bytes, this.fe?.speechLevel ?? 127, (Date.now() >>> 0) || 1);
      } catch (error) {
        console.warn("Race speech unavailable:", error);
      }
    })();
    return this.speechLoading;
  }

  get speechReady() { return this.announcer !== null; }

  speechBuffer(ctx, i) {
    let b = this.speechBuffers.get(i);
    if (b && b.__ctx === ctx) return b;
    const pcm = this.announcer.phrase_pcm(i);
    if (!pcm.length) return null;
    b = ctx.createBuffer(1, pcm.length, this.announcer.phrase_rate(i));
    b.copyToChannel(pcm, 0);
    b.__ctx = ctx;
    this.speechBuffers.set(i, b);
    return b;
  }

  // Per frame from the race HUD state.
  frame(phase, remaining) {
    if (!this.helpers || !viewer) return;
    if (!this.speechLoading) this.loadSpeech();
    const a = this.announcer;
    const prevPhase = this.events.phase;
    const ev = this.events.update({ phase, lap: viewer.get_current_lap(), best: viewer.get_best_lap_time() });
    if (ev.start) a?.reset();
    const step = this.helpers.announcerStep(phase, prevPhase, remaining);
    if (a && step !== null) {
      const p = a.countdown(step);
      if (p >= 0) this.record("speech-request", { phrase: p, step });
    }
    const cars = viewer.get_total_participants();
    const place = viewer.get_player_position();
    if (a && ev.lap) a.lap_completed(ev.lap.lapsDone, viewer.get_total_laps(), cars, place, ev.lap.best);
    if (ev.finished) {
      const pattern = a ? a.race_finished(place, cars) : (cars > 1 ? (place === 1 ? "win*" : "lose*") : "");
      if (pattern && this.director) { this.director.play(pattern, 1); this.execute(); }
    }
    if (ev.pause && this.director) { this.director.stop(0); this.execute(); } // Audio_Pause 0x4ab150
    if (ev.resume && this.director) { this.director.set_level(this.level); this.director.play("", 0); this.execute(); }
    this.playSpeech();
  }

  playSpeech() {
    const a = this.announcer, ctx = soundManager.ctx;
    if (!a) return;
    const now = ctx ? ctx.currentTime : 0;
    const remaining = this.speechSource ? Math.round((this.speechEnd - now) * 100) : -1;
    const next = a.update(remaining < 0 ? -1 : remaining);
    if (next < 0) return;
    this.record("speech-play", { phrase: next, name: a.phrase_name(next) });
    if (!ctx || !this.enabled || !soundManager.enabled) { this.speechSource = null; return; }
    const buf = this.speechBuffer(ctx, next);
    if (!buf) { this.speechSource = null; return; }
    const src = ctx.createBufferSource();
    src.buffer = buf;
    const gain = ctx.createGain();
    // Two identical voices at speech × 127 × 0x55 >> 14 (0x4b04e0, 0x4a8300); A50.
    gain.gain.value = 2 * a.volume() / 127;
    src.connect(gain);
    gain.connect(ctx.destination);
    src.start();
    this.speechSource = src;
    this.speechEnd = now + buf.duration;
    src.onended = () => {
      if (this.speechSource === src) this.speechSource = null;
      try { src.disconnect(); gain.disconnect(); } catch {}
    };
  }
}

const gameAudio = new GameAudio();

// --- Profile Persistence and State ---
const storage = typeof localStorage !== "undefined" ? localStorage : {
  getItem: () => null,
  setItem: () => {},
  removeItem: () => {},
};

const PROFILE_KEY = "porsche_profile_011_v2";
let currentProfile = null;
const RANK_NAMES = ["Applicant", "Junior Test Driver", "Test Driver", "Senior Test Driver", "Chief Test Driver"];

function loadStoredProfile() {
  try {
    const raw = storage.getItem(PROFILE_KEY);
    if (raw) {
      currentProfile = JSON.parse(raw);
    }
  } catch {}
  if (currentProfile && typeof currentProfile.version !== "number") {
    currentProfile.version = 1;
  }
  // Older JS starter profiles used a nonexistent SIM name for the same starter model.
  for (const car of currentProfile?.garage || []) {
    if (car.model_name === "356_1" && car.sim_name === "356road11") {
      car.sim_name = "356coupe11";
    }
    if (String(car.model_name || '').toLowerCase() === "boxster" && car.sim_name === "boxster") {
      car.sim_name = "boxster25";
    }
  }
  if (!currentProfile) {
    if (typeof shell_profile_new === "function") {
      try {
        const json = shell_profile_new("Driver");
        currentProfile = JSON.parse(json);
      } catch {}
    }
    if (!currentProfile) {
      currentProfile = {
        version: 1,
        name: "Driver",
        credits: 11000,
        factory_rank: 0,
        garage: [
          {
            model_name: "356_1",
            display_name: "356 'No. 1' Roadster (1948)",
            sim_name: "356coupe11",
            era: "Classic",
            year: 1948,
            color_index: 0,
            price_paid: 11000,
            condition: 1.0,
            mileage_km: 0.0,
            installed_parts: [],
          },
        ],
        selected_car_index: 0,
        evolution_career: {
          current_era: "Classic",
          completed_tournaments: [],
          unlocked_eras: ["Classic"],
          trophies: [],
          total_winnings: 0,
        },
        factory_driver: {
          driver_rank: "Applicant",
          current_tier: 1,
          current_mission_code: "0M01",
          completed_missions: [],
          best_times: {},
          passed_count: 0,
        },
        race_history: [],
      };
    }
    saveProfile();
  }
  updateProfileUI();
}

function saveProfile() {
  if (currentProfile) {
    try {
      storage.setItem(PROFILE_KEY, JSON.stringify(currentProfile));
    } catch {}
  }
  updateProfileUI();
}

function updateProfileUI() {
  if (!currentProfile) return;
  const nameEl = document.querySelector("#profileNameDisplay");
  const credEl = document.querySelector("#profileCreditsDisplay");
  const rankEl = document.querySelector("#profileRankDisplay");
  const mName = document.querySelector("#profileNameInput");
  const mCred = document.querySelector("#modalCreditsDisplay");
  const mRank = document.querySelector("#modalRankDisplay");
  const gList = document.querySelector("#profileGarageList");
  const buyBtn = document.querySelector("#profileBuyCarBtn");

  const rankStr = RANK_NAMES[currentProfile.factory_rank] || `Rank ${currentProfile.factory_rank}`;
  const credStr = `${currentProfile.credits.toLocaleString()} CR`;

  if (nameEl) nameEl.textContent = currentProfile.name;
  if (credEl) credEl.textContent = currentProfile.credits.toLocaleString();
  if (rankEl) rankEl.textContent = rankStr;

  if (mName && typeof document.activeElement !== "undefined" && document.activeElement !== mName) {
    mName.value = currentProfile.name;
  }
  if (mCred) mCred.textContent = credStr;
  if (mRank) mRank.textContent = rankStr;

  const has356 = currentProfile.garage && currentProfile.garage.some(c => c.model_name && c.model_name.startsWith("356"));
  if (buyBtn) {
    if (has356) {
      buyBtn.disabled = true;
      buyBtn.textContent = "Куплено: '50 356 Ferdinand";
    } else {
      buyBtn.disabled = currentProfile.credits < 11000;
      buyBtn.textContent = "Купить '50 356 Coupé Ferdinand (11 000 CR)";
    }
  }

  if (gList) {
    gList.replaceChildren();
    if (!currentProfile.garage || currentProfile.garage.length === 0) {
      const emptyDiv = document.createElement("div");
      emptyDiv.className = "garage-empty";
      emptyDiv.textContent = "Гараж пуст. Купите авто в автосалоне.";
      gList.appendChild(emptyDiv);
    } else {
      currentProfile.garage.forEach((c, i) => {
        const item = document.createElement("div");
        item.className = "garage-item" + (i === currentProfile.selected_car_index ? " selected" : "");
        const info = document.createElement("span");
        const condPct = Math.round((c.condition !== undefined ? c.condition : 1.0) * 100);
        const mileageKm = Math.round(c.mileage_km || 0);
        info.innerHTML = `<b>${c.display_name || c.model_name}</b> &bull; ${mileageKm} км &bull; ${condPct}%`;

        const btnWrap = document.createElement("div");
        btnWrap.style.display = "flex";
        btnWrap.style.gap = "6px";

        if (i !== currentProfile.selected_car_index) {
          const selectBtn = document.createElement("button");
          selectBtn.type = "button";
          selectBtn.className = "btn-subtle";
          selectBtn.style.padding = "2px 6px";
          selectBtn.textContent = "Выбрать";
          selectBtn.onclick = () => {
            currentProfile.selected_car_index = i;
            saveProfile();
            updateProfileUI();
          };
          btnWrap.appendChild(selectBtn);
        }

        const sellBtn = document.createElement("button");
        sellBtn.type = "button";
        sellBtn.className = "btn-subtle";
        sellBtn.style.padding = "2px 6px";
        sellBtn.style.color = "#f87171";
        sellBtn.textContent = "Продать";
        sellBtn.disabled = currentProfile.garage.length <= 1;
        sellBtn.onclick = () => {
          if (typeof shell_sell_car === "function") {
            try {
              const res = shell_sell_car(JSON.stringify(currentProfile), i);
              currentProfile = JSON.parse(res);
              saveProfile();
              updateProfileUI();
              soundManager.playPass();
              setStatus("Автомобиль продан, средства зачислены на счёт.");
            } catch (err) {
              alert(`Ошибка продажи: ${err}`);
            }
          }
        };
        btnWrap.appendChild(sellBtn);

        item.appendChild(info);
        item.appendChild(btnWrap);
        gList.appendChild(item);
      });
    }
  }
}

// --- 010 Dealership & Tuning Shop State ---
let currentDealershipTab = "new";
let currentDealershipEra = "all";

function openDealership() {
  const modal = document.querySelector("#dealershipModal");
  if (!modal) return;
  modal.hidden = false;
  renderDealershipGrid();
}

function renderDealershipGrid() {
  const grid = document.querySelector("#dealershipGrid");
  if (!grid) return;
  grid.replaceChildren();

  let cars = [];
  if (currentDealershipTab === "new") {
    if (typeof shell_get_dealership_catalog === "function") {
      try { cars = JSON.parse(shell_get_dealership_catalog()); } catch {}
    }
  } else {
    if (typeof shell_get_used_cars === "function") {
      try { cars = JSON.parse(shell_get_used_cars()); } catch {}
    }
  }

  if (currentDealershipEra !== "all") {
    cars = cars.filter(c => c.era === currentDealershipEra);
  }

  if (cars.length === 0) {
    const empty = document.createElement("div");
    empty.style.color = "#888";
    empty.style.padding = "20px";
    empty.textContent = "Нет доступных автомобилей для выбранной категории.";
    grid.appendChild(empty);
    return;
  }

  cars.forEach(car => {
    const item = document.createElement("div");
    item.className = "market-item";

    const title = document.createElement("div");
    title.className = "market-item-title";
    title.textContent = car.display_name || car.model_name;

    const sub = document.createElement("div");
    sub.className = "market-item-sub";
    const mileageStr = car.mileage_km ? `${car.mileage_km.toLocaleString()} км` : "Новый (0 км)";
    const condPct = Math.round((car.condition || 1.0) * 100);
    sub.innerHTML = `Год: <b>${car.year}</b> &bull; Пробег: <b>${mileageStr}</b> &bull; Состояние: <b>${condPct}%</b>`;

    const gauge = document.createElement("div");
    gauge.className = "condition-gauge";
    const fill = document.createElement("div");
    fill.className = "condition-fill" + (condPct < 50 ? " danger" : condPct < 80 ? " warn" : "");
    fill.style.width = `${condPct}%`;
    gauge.appendChild(fill);

    const priceRow = document.createElement("div");
    priceRow.className = "market-item-price";
    const priceVal = document.createElement("span");
    priceVal.textContent = `${car.price.toLocaleString()} CR`;

    const buyBtn = document.createElement("button");
    buyBtn.type = "button";
    buyBtn.className = "market-buy-btn";
    buyBtn.textContent = "Купить";
    const canAfford = currentProfile && currentProfile.credits >= car.price;
    buyBtn.disabled = !canAfford;

    buyBtn.addEventListener("click", () => {
      if (typeof shell_buy_car === "function") {
        try {
          const res = shell_buy_car(JSON.stringify(currentProfile), car.id, currentDealershipTab === "used");
          currentProfile = JSON.parse(res);
          saveProfile();
          renderDealershipGrid();
          soundManager.playPass();
          setStatus(`Куплен ${car.display_name} за ${car.price.toLocaleString()} CR`);
        } catch (e) {
          alert(`Ошибка покупки: ${e}`);
        }
      }
    });

    priceRow.appendChild(priceVal);
    priceRow.appendChild(buyBtn);

    item.appendChild(title);
    item.appendChild(sub);
    item.appendChild(gauge);
    item.appendChild(priceRow);
    grid.appendChild(item);
  });
}

let currentTuningCategory = "all";

function openPartsShop() {
  const modal = document.querySelector("#partsShopModal");
  if (!modal) return;
  modal.hidden = false;
  renderPartsShop();
}

function renderPartsShop() {
  const grid = document.querySelector("#tuningPartsGrid");
  const carName = document.querySelector("#tuningCarName");
  const carMileage = document.querySelector("#tuningCarMileage");
  const carCondition = document.querySelector("#tuningCarCondition");
  const repairAllBtn = document.querySelector("#tuningRepairAllBtn");

  if (!currentProfile || !currentProfile.garage || currentProfile.garage.length === 0) {
    if (grid) {
      grid.replaceChildren();
      const empty = document.createElement("div");
      empty.textContent = "В гараже нет машин. Сначала купите авто в автосалоне.";
      grid.appendChild(empty);
    }
    return;
  }

  const selectedIdx = currentProfile.selected_car_index || 0;
  const activeCar = currentProfile.garage[selectedIdx] || currentProfile.garage[0];
  const carModel = activeCar.model_name || "356_1";
  const condPct = Math.round((activeCar.condition !== undefined ? activeCar.condition : 1.0) * 100);

  if (carName) carName.textContent = activeCar.display_name || activeCar.model_name;
  if (carMileage) carMileage.textContent = `${Math.round(activeCar.mileage_km || 0).toLocaleString()} км`;
  if (carCondition) carCondition.textContent = `${condPct}%`;

  if (repairAllBtn) {
    const isWorn = condPct < 99;
    const estimatedCost = Math.round((activeCar.base_price || 11000) * (1.0 - (activeCar.condition || 1.0)) * 0.4);
    repairAllBtn.textContent = isWorn ? `Ремонт узлов (${estimatedCost.toLocaleString()} CR)` : "Авто в идеале (0 CR)";
    repairAllBtn.disabled = !isWorn || currentProfile.credits < estimatedCost;
    repairAllBtn.onclick = () => {
      if (typeof shell_repair_car === "function") {
        try {
          const res = shell_repair_car(JSON.stringify(currentProfile), selectedIdx);
          currentProfile = JSON.parse(res);
          saveProfile();
          renderPartsShop();
          soundManager.playPass();
          setStatus("Автомобиль и все установленные узлы полностью отремонтированы!");
        } catch (e) {
          alert(`Ошибка ремонта: ${e}`);
        }
      }
    };
  }

  if (!grid) return;
  grid.replaceChildren();

  let parts = [];
  if (typeof shell_get_parts_catalog === "function") {
    try { parts = JSON.parse(shell_get_parts_catalog(carModel)); } catch {}
  }

  if (currentTuningCategory !== "all") {
    parts = parts.filter(p => p.category === currentTuningCategory);
  }

  parts.forEach(part => {
    const item = document.createElement("div");
    item.className = "market-item";

    const title = document.createElement("div");
    title.className = "market-item-title";
    title.textContent = part.name;

    const sub = document.createElement("div");
    sub.className = "market-item-sub";
    sub.innerHTML = `Категория: <b>${part.category_name || part.category}</b>`;

    const priceRow = document.createElement("div");
    priceRow.className = "market-item-price";
    const priceVal = document.createElement("span");
    priceVal.textContent = `${part.price.toLocaleString()} CR`;

    const installBtn = document.createElement("button");
    installBtn.type = "button";
    installBtn.className = "market-buy-btn";
    installBtn.textContent = "Установить";
    const canAfford = currentProfile.credits >= part.price;
    installBtn.disabled = !canAfford;

    installBtn.addEventListener("click", () => {
      if (typeof shell_buy_part === "function") {
        try {
          const res = shell_buy_part(JSON.stringify(currentProfile), selectedIdx, part.part_id);
          currentProfile = JSON.parse(res);
          saveProfile();
          renderPartsShop();
          soundManager.playPass();
          setStatus(`Установлена деталь: ${part.name}`);
        } catch (e) {
          alert(`Ошибка установки детали: ${e}`);
        }
      }
    });

    priceRow.appendChild(priceVal);
    priceRow.appendChild(installBtn);

    item.appendChild(title);
    item.appendChild(sub);
    item.appendChild(priceRow);
    grid.appendChild(item);
  });
}

let currentEvoEra = "classic";

function openEvolutionTournaments() {
  const modal = document.querySelector("#evolutionTournamentsModal");
  if (!modal) return;
  modal.hidden = false;
  renderEvolutionCups();
}

function renderEvolutionCups() {
  const grid = document.querySelector("#evolutionCupsGrid");
  if (!grid) return;
  grid.replaceChildren();

  let cups = [];
  if (typeof shell_get_tournament_cups === "function") {
    try { cups = JSON.parse(shell_get_tournament_cups(currentEvoEra)); } catch {}
  }

  if (cups.length === 0) {
    const empty = document.createElement("div");
    empty.style.color = "#888";
    empty.style.padding = "20px";
    empty.textContent = "Кубки для выбранной эпохи загружаются...";
    grid.appendChild(empty);
    return;
  }

  cups.forEach(cup => {
    const card = document.createElement("div");
    card.className = "cup-card";

    const title = document.createElement("div");
    title.className = "cup-title";
    title.textContent = cup.title;

    const desc = document.createElement("div");
    desc.className = "cup-desc";
    desc.textContent = cup.description;

    const legsList = document.createElement("div");
    legsList.className = "cup-legs";
    legsList.innerHTML = cup.legs.map((l, i) => `Этап ${i + 1}: <b>${l.track_display_name}</b> (${l.laps} круга)`).join("<br/>");

    const stats = document.createElement("div");
    stats.className = "cup-stats";
    stats.innerHTML = `<span>Взнос: <b>${cup.entry_fee.toLocaleString()} CR</b></span><span>Призовой фонд: <b style="color:#d3bd83">${cup.total_prize_purse.toLocaleString()} CR</b></span>`;

    const enterBtn = document.createElement("button");
    enterBtn.type = "button";
    enterBtn.className = "btn-primary";
    enterBtn.style.marginTop = "8px";
    enterBtn.textContent = "Участвовать в кубке";

    enterBtn.addEventListener("click", () => {
      const modal = document.querySelector("#evolutionTournamentsModal");
      if (modal) modal.hidden = true;
      const leg0 = cup.legs[0];
      const eventData = {
        id: `cup_${cup.id}`,
        title: cup.title,
        description: cup.description,
        track_name: leg0.track_id,
        track_display: leg0.track_display_name,
        laps: leg0.laps,
        opponents: 3,
        entry_fee: cup.entry_fee,
        first_prize: Math.round(cup.total_prize_purse * 0.5),
        goal_text: `1-е место (${leg0.laps} круга)`,
        trackId: leg0.track_id,
      };
      showBriefing(eventData);
    });

    card.appendChild(title);
    card.appendChild(desc);
    card.appendChild(legsList);
    card.appendChild(stats);
    card.appendChild(enterBtn);
    grid.appendChild(card);
  });
}

// --- Factory Driver Campaign Ladder (011) ---
let currentFactoryTier = 1;

function openFactoryLadder() {
  const modal = document.querySelector("#factoryMissionsModal");
  if (!modal) return;
  modal.hidden = false;
  const rankBadge = document.querySelector("#facCurrentRankBadge");
  if (rankBadge) {
    rankBadge.textContent = currentProfile?.factory_driver?.driver_rank || "Applicant";
  }
  renderFactoryMissions();
}

function renderFactoryMissions() {
  const grid = document.querySelector("#factoryMissionsGrid");
  if (!grid) return;
  grid.replaceChildren();

  let missions = [];
  if (typeof shell_get_factory_missions === "function") {
    try {
      missions = JSON.parse(shell_get_factory_missions());
    } catch {}
  }

  if (missions.length === 0) {
    const empty = document.createElement("div");
    empty.style.color = "#888";
    empty.style.padding = "20px";
    empty.textContent = "Миссии Factory Driver загружаются...";
    grid.appendChild(empty);
    return;
  }

  const tierMissions = missions.filter(m => m.tier === currentFactoryTier);
  const completedCodes = new Set(currentProfile?.factory_driver?.completed_missions || []);
  const currentCode = currentProfile?.factory_driver?.current_mission_code || "0M01";
  const bestTimes = currentProfile?.factory_driver?.best_times || {};

  tierMissions.forEach(mission => {
    const card = document.createElement("div");
    card.className = "cup-card";

    const isCompleted = completedCodes.has(mission.code);
    const isUnlocked = isCompleted || mission.code === currentCode;

    const header = document.createElement("div");
    header.style.display = "flex";
    header.style.justifyContent = "space-between";
    header.style.alignItems = "center";

    const title = document.createElement("div");
    title.className = "cup-title";
    title.textContent = `${mission.index}. ${mission.title} (${mission.code})`;

    const badge = document.createElement("span");
    badge.className = "rank-badge";
    if (isCompleted) {
      badge.style.background = "#15803d";
      badge.textContent = `ПРОЙДЕНО (${bestTimes[mission.code]?.toFixed(1) || "--"}с)`;
    } else if (isUnlocked) {
      badge.style.background = "#0284c7";
      badge.textContent = "ДОСТУПНО";
    } else {
      badge.style.background = "#555";
      badge.textContent = "ЗАБЛОКИРОВАНО";
    }
    header.appendChild(title);
    header.appendChild(badge);

    const desc = document.createElement("div");
    desc.className = "cup-desc";
    desc.textContent = mission.briefing;

    const details = document.createElement("div");
    details.className = "cup-stats";
    const carName = mission.car_model.toUpperCase();
    const trackName = mission.track_name || mission.track_id;
    const timeLimitStr = mission.time_limit > 0 ? `Лимит: <b>${mission.time_limit.toFixed(1)} с</b>` : "Очная дуэль (1-е место)";
    details.innerHTML = `<span>Авто: <b>Porsche ${carName}</b></span><span>Трасса: <b>${trackName}</b></span><span>${timeLimitStr}</span>`;

    const enterBtn = document.createElement("button");
    enterBtn.type = "button";
    enterBtn.className = isUnlocked ? "btn-primary" : "btn-subtle";
    enterBtn.disabled = !isUnlocked;
    enterBtn.style.marginTop = "8px";
    enterBtn.textContent = isCompleted ? "Пройти снова" : "Начать испытание";

    enterBtn.addEventListener("click", () => {
      const facModal = document.querySelector("#factoryMissionsModal");
      if (facModal) facModal.hidden = true;
      const eventData = {
        id: mission.code,
        mission_index: mission.index,
        title: `Задание ${mission.index}: ${mission.title}`,
        description: mission.briefing,
        track_name: mission.track_id,
        track: mission.track_name,
        car: `Porsche ${mission.car_model.toUpperCase()}`,
        car_sim: mission.car_sim,
        goal: mission.time_limit > 0 ? `Лимит: ${mission.time_limit.toFixed(1)} с` : "Победа в очной дуэли",
        reward: mission.tier === 1 && mission.index === 13 ? "Повышение до Test Driver + '78 911 Turbo" :
                mission.tier === 2 && mission.index === 23 ? "Повышение до Chief Test Driver + '73 Carrera RS" :
                mission.tier === 3 && mission.index === 34 ? "Титул Master Ace + '99 911 GT3" : "Карьерный прогресс",
        is_factory: true,
        time_limit: mission.time_limit,
        trackId: mission.track_id,
        pass_msg: mission.pass_message,
        fail_msg: mission.fail_message,
        mission_obj: mission,
      };
      showBriefing(eventData);
    });

    card.appendChild(header);
    card.appendChild(desc);
    card.appendChild(details);
    card.appendChild(enterBtn);
    grid.appendChild(card);
  });
}

// --- Career Events & Briefing State ---
let activeCareerEvent = null;
let eventFinishedHandled = false;

const CAR_SPECS = {
  "356_1": { name: "Porsche 356 'No. 1' Roadster (1948)", power: "40 л.с. (30 кВт)", weight: "585 кг", drive: "RR (Заднемоторный)" },
  "356": { name: "Porsche 356 Coupé Ferdinand (1950)", power: "44 л.с. (32 кВт)", weight: "820 кг", drive: "RR (Заднемоторный)" },
  "356a": { name: "Porsche 356 A 1600 Coupé (1956)", power: "60 л.с. (44 кВт)", weight: "850 кг", drive: "RR (Заднемоторный)" },
  "356b": { name: "Porsche 356 B 2000 GS Carrera 2 (1960)", power: "130 л.с. (96 кВт)", weight: "1 010 кг", drive: "RR (Заднемоторный)" },
  "550": { name: "Porsche 550 A Spyder (1956)", power: "135 л.с. (99 кВт)", weight: "550 кг", drive: "MR (Среднемоторный)" },
  "901": { name: "Porsche 911 Coupé (901) (1964)", power: "130 л.с. (96 кВт)", weight: "1 080 кг", drive: "RR (Заднемоторный)" },
  "914": { name: "Porsche 914/4 (1970)", power: "80 л.с. (59 кВт)", weight: "940 кг", drive: "MR (Среднемоторный)" },
  "911_rs": { name: "Porsche 911 Carrera RS 2.7 (1973)", power: "210 л.с. (154 кВт)", weight: "960 кг", drive: "RR (Заднемоторный)" },
  "911rs": { name: "Porsche 911 Carrera RS 2.7 (1973)", power: "210 л.с. (154 кВт)", weight: "960 кг", drive: "RR (Заднемоторный)" },
  "928": { name: "Porsche 928 (1978)", power: "240 л.с. (177 кВт)", weight: "1 450 кг", drive: "FR (Передний двигатель)" },
  "930": { name: "Porsche 911 Turbo 3.3 (930) (1978)", power: "300 л.с. (221 кВт)", weight: "1 300 кг", drive: "RR (Заднемоторный турбо)" },
  "935": { name: "Porsche 935/78 'Moby Dick' (1978)", power: "845 л.с. (621 кВт)", weight: "1 025 кг", drive: "RR (Заднемоторный гоночный)" },
  "944": { name: "Porsche 944 (1982)", power: "163 л.с. (120 кВт)", weight: "1 180 кг", drive: "FR (Трансэксл)" },
  "959": { name: "Porsche 959 (1987)", power: "450 л.с. (331 кВт)", weight: "1 450 кг", drive: "AWD (Полный привод)" },
  "964": { name: "Porsche 911 Carrera 2 (964) (1989)", power: "250 л.с. (184 кВт)", weight: "1 350 кг", drive: "RR (Заднемоторный)" },
  "993": { name: "Porsche 911 Carrera (993) (1995)", power: "272 л.с. (200 кВт)", weight: "1 370 кг", drive: "RR (Заднемоторный)" },
  "996": { name: "Porsche 911 Carrera (996) (1998)", power: "300 л.с. (221 кВт)", weight: "1 320 кг", drive: "RR (Заднемоторный)" },
  "boxster": { name: "Porsche Boxster (986) (1997)", power: "204 л.с. (150 кВт)", weight: "1 252 кг", drive: "MR (Среднемоторный)" },
  "gt1": { name: "Porsche 911 GT1 Straßenversion (1998)", power: "544 л.с. (400 кВт)", weight: "1 150 кг", drive: "MR (Среднемоторный битурбо)" },
  "gt2": { name: "Porsche 911 GT2 (993) (1996)", power: "430 л.с. (316 кВт)", weight: "1 290 кг", drive: "RR (Заднемоторный турбо)" },
  "gt3": { name: "Porsche 911 GT3 (996) (1999)", power: "360 л.с. (265 кВт)", weight: "1 350 кг", drive: "RR (Заднемоторный атмосферный)" },
};

function showBriefing(eventData) {
  activeCareerEvent = eventData;
  const modal = document.querySelector("#eventBriefingModal");
  if (!modal) return;

  const bCategory = document.querySelector("#briefingCategoryBadge");
  const bTitle = document.querySelector("#briefingTitle");
  const bInstructorName = document.querySelector("#briefingInstructorName");
  const bDesc = document.querySelector("#briefingDesc");
  const bTrackCar = document.querySelector("#briefingTrackCar");
  const bCarTitle = document.querySelector("#briefingCarTitle");
  const bCarPower = document.querySelector("#briefingCarPower");
  const bCarWeight = document.querySelector("#briefingCarWeight");
  const bCarDrive = document.querySelector("#briefingCarDrive");
  const bTrackName = document.querySelector("#briefingTrackName");
  const bGoal = document.querySelector("#briefingGoal");
  const bTimeLimit = document.querySelector("#briefingTimeLimit");
  const bRules = document.querySelector("#briefingRules");
  const bReward = document.querySelector("#briefingReward");

  const isFactory = Boolean(eventData.is_factory);
  modal.dataset.careerMode = isFactory ? "factory" : "evolution";
  modal.dataset.factoryView = "briefing";
  const mapButton = document.querySelector("#briefingMapBtn");
  if (mapButton) {
    mapButton.hidden = !isFactory;
    mapButton.textContent = "КАРТА";
  }
  const doneButton = document.querySelector("#briefingCancelBtn");
  if (doneButton) doneButton.textContent = isFactory ? "ГОТОВО" : "НАЗАД";
  const facility = document.querySelector("#briefingTrackFacility");
  if (facility) facility.textContent = isFactory ? "PORSCHE TEST TEAM" : "EVOLUTION TOURNAMENT";
  if (bCategory) {
    bCategory.textContent = isFactory ? "PORSCHE FACTORY DRIVER" : "PORSCHE EVOLUTION";
    bCategory.className = isFactory ? "briefing-category-badge factory" : "briefing-category-badge evolution";
  }

  if (bTitle) bTitle.textContent = eventData.title;
  if (bInstructorName) bInstructorName.textContent = "Инструктор Рольф";
  if (bDesc) bDesc.textContent = eventData.description;

  let carKey = "356_1";
  if (isFactory) {
    carKey = eventData.mission_obj?.car_model || eventData.car_sim || "boxster";
  } else {
    const selIdx = currentProfile?.selected_car_index || 0;
    carKey = currentProfile?.garage?.[selIdx]?.model_name || "356_1";
  }
  carKey = carKey.toLowerCase();
  const spec = CAR_SPECS[carKey] || {
    name: eventData.car || `Porsche ${carKey.toUpperCase()}`,
    power: "204 л.с. (150 кВт)",
    weight: "1 250 кг",
    drive: "Задний привод (RWD)",
  };

  if (bCarTitle) bCarTitle.textContent = spec.name;
  if (bCarPower) bCarPower.textContent = spec.power;
  if (bCarWeight) bCarWeight.textContent = spec.weight;
  if (bCarDrive) bCarDrive.textContent = spec.drive;

  const trackDisplay = eventData.track_display || eventData.track || eventData.track_name || eventData.trackId;
  if (bTrackName) bTrackName.textContent = trackDisplay;
  if (bTrackCar) bTrackCar.textContent = `${trackDisplay} / ${spec.name}`;
  if (bGoal) bGoal.textContent = eventData.goal || eventData.goal_text || "1-е место";

  if (bTimeLimit) {
    if (eventData.time_limit && eventData.time_limit > 0) {
      bTimeLimit.textContent = `${eventData.time_limit.toFixed(1)} сек`;
    } else if (eventData.laps) {
      bTimeLimit.textContent = `${eventData.laps} круга (без лимита)`;
    } else {
      bTimeLimit.textContent = "Без лимита времени";
    }
  }

  if (bRules) {
    if (isFactory) {
      bRules.textContent = "Точное соблюдение траектории; сбитые конусы +2.0 сек к времени.";
    } else {
      bRules.textContent = "Контактная борьба с соперниками; срезка углов штрафуется.";
    }
  }

  if (bReward) bReward.textContent = eventData.reward;

  const bMapImg = document.querySelector("#briefingMapImg");
  const bInstructorImg = document.querySelector("#briefingInstructorImg");

  if (isFactory && bInstructorImg) bInstructorImg.src = "./assets/people_rolp.png";

  if (isFactory && bMapImg) {
    let mapFile = "map_0m01.png";
    if (isFactory && eventData.mission_obj?.code) {
      const code = String(eventData.mission_obj.code).toLowerCase();
      mapFile = `map_${code}.png`;
    }
    bMapImg.src = `./assets/${mapFile}`;
    bMapImg.onerror = () => {
      bMapImg.src = "./assets/map_0m01.png";
    };
  }

  modal.hidden = false;
}

// Wire up Toolbar & Career Buttons
// Browsers only allow audio after a user gesture: start it on the first click or key.
const startAudioOnGesture = () => {
  if (soundManager.enabled) soundManager.init();
  window.removeEventListener("pointerdown", startAudioOnGesture, true);
  window.removeEventListener("keydown", startAudioOnGesture, true);
};
window.addEventListener("pointerdown", startAudioOnGesture, true);
window.addEventListener("keydown", startAudioOnGesture, true);

// Music level 0..127 (Audio.MusicLevel; playlist volume 80 × level >> 7, 0x4a82f0).
const musicLevelInput = document.querySelector("#musicLevel");
if (musicLevelInput) {
  musicLevelInput.addEventListener("input", () => gameAudio.setLevel(Number(musicLevelInput.value)));
}

const soundBtn = document.querySelector("#soundBtn");
if (soundBtn) {
  soundBtn.classList.toggle("active", soundManager.enabled);
  soundBtn.textContent = soundManager.enabled ? "🔊 Звук: ВКЛ" : "🔇 Звук: ВЫКЛ";
  soundBtn.addEventListener("click", () => {
    const enabled = soundManager.toggle();
    soundBtn.classList.toggle("active", enabled);
    soundBtn.textContent = enabled ? "🔊 Звук: ВКЛ" : "🔇 Звук: ВЫКЛ";
  });
}

const profileBtn = document.querySelector("#profileBtn");
const profileModal = document.querySelector("#profileModal");
const profileModalCloseBtn = document.querySelector("#profileModalCloseBtn");
const profileNameInput = document.querySelector("#profileNameInput");
const profileBuyCarBtn = document.querySelector("#profileBuyCarBtn");
const profileExportBtn = document.querySelector("#profileExportBtn");
const profileImportInput = document.querySelector("#profileImportInput");
const profileResetBtn = document.querySelector("#profileResetBtn");

if (profileBtn && profileModal) {
  profileBtn.addEventListener("click", () => {
    updateProfileUI();
    profileModal.hidden = false;
  });
}
if (profileModalCloseBtn && profileModal) {
  profileModalCloseBtn.addEventListener("click", () => {
    profileModal.hidden = true;
  });
}
if (profileNameInput) {
  profileNameInput.addEventListener("input", (e) => {
    if (currentProfile) {
      currentProfile.name = e.target.value.trim() || "Driver";
      saveProfile();
    }
  });
}
if (profileBuyCarBtn) {
  profileBuyCarBtn.addEventListener("click", () => {
    if (!currentProfile) return;
    if (typeof shell_profile_buy_356 === "function") {
      try {
        const res = shell_profile_buy_356(JSON.stringify(currentProfile));
        currentProfile = JSON.parse(res);
        saveProfile();
        soundManager.playPass();
      } catch (err) {
        if (typeof alert === "function") alert(err);
      }
    } else {
      if (currentProfile.credits >= 11000) {
        currentProfile.credits -= 11000;
        currentProfile.garage.push({
          model_name: "356_1",
          sim_name: "356coupe11",
          color_index: 0,
          price_paid: 11000,
        });
        currentProfile.selected_car_index = currentProfile.garage.length - 1;
        saveProfile();
        soundManager.playPass();
      }
    }
    updateProfileUI();
  });
}
if (profileExportBtn) {
  profileExportBtn.addEventListener("click", () => {
    if (!currentProfile) return;
    if (typeof Blob !== "undefined" && typeof URL !== "undefined" && typeof URL.createObjectURL === "function") {
      const blob = new Blob([JSON.stringify(currentProfile, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `porsche_profile_${currentProfile.name.toLowerCase().replace(/[^a-z0-9]/g, "_")}.json`;
      a.click();
      URL.revokeObjectURL(url);
    }
  });
}
if (profileImportInput) {
  profileImportInput.addEventListener("change", async (e) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      const parsed = JSON.parse(text);
      if (parsed && typeof parsed.name === "string" && typeof parsed.credits === "number") {
        currentProfile = parsed;
        saveProfile();
        soundManager.playPass();
        setStatus(`Профиль "${currentProfile.name}" успешно импортирован.`);
      }
    } catch (err) {
      if (typeof alert === "function") alert("Ошибка при импорте: " + err.message);
    }
  });
}
if (profileResetBtn) {
  profileResetBtn.addEventListener("click", () => {
    storage.removeItem(PROFILE_KEY);
    currentProfile = null;
    loadStoredProfile();
    setStatus("Профиль сброшен к начальному состоянию.");
  });
}

const careerEvolutionBtn = document.querySelector("#careerEvolutionBtn");
if (careerEvolutionBtn) {
  careerEvolutionBtn.addEventListener("click", () => {
    openEvolutionTournaments();
  });
}

const dealershipBtn = document.querySelector("#dealershipBtn");
if (dealershipBtn) {
  dealershipBtn.addEventListener("click", () => {
    openDealership();
  });
}

const dealershipCloseBtn = document.querySelector("#dealershipCloseBtn");
if (dealershipCloseBtn) {
  dealershipCloseBtn.addEventListener("click", () => {
    const modal = document.querySelector("#dealershipModal");
    if (modal) modal.hidden = true;
  });
}

const dealershipNewTab = document.querySelector("#dealershipNewTab");
const dealershipUsedTab = document.querySelector("#dealershipUsedTab");
if (dealershipNewTab && dealershipUsedTab) {
  dealershipNewTab.addEventListener("click", () => {
    currentDealershipTab = "new";
    dealershipNewTab.classList.add("active");
    dealershipUsedTab.classList.remove("active");
    renderDealershipGrid();
  });
  dealershipUsedTab.addEventListener("click", () => {
    currentDealershipTab = "used";
    dealershipUsedTab.classList.add("active");
    dealershipNewTab.classList.remove("active");
    renderDealershipGrid();
  });
}

const queryAll = (sel) => (typeof document !== "undefined" && typeof document.querySelectorAll === "function") ? Array.from(document.querySelectorAll(sel)) : [];

queryAll(".era-chip").forEach(chip => {
  chip.addEventListener("click", () => {
    queryAll(".era-chip").forEach(c => c.classList.remove("active"));
    chip.classList.add("active");
    currentDealershipEra = chip.dataset.era || "all";
    renderDealershipGrid();
  });
});

const partsShopBtn = document.querySelector("#partsShopBtn");
if (partsShopBtn) {
  partsShopBtn.addEventListener("click", () => {
    openPartsShop();
  });
}

const partsShopCloseBtn = document.querySelector("#partsShopCloseBtn");
if (partsShopCloseBtn) {
  partsShopCloseBtn.addEventListener("click", () => {
    const modal = document.querySelector("#partsShopModal");
    if (modal) modal.hidden = true;
  });
}

queryAll(".cat-chip").forEach(chip => {
  chip.addEventListener("click", () => {
    queryAll(".cat-chip").forEach(c => c.classList.remove("active"));
    chip.classList.add("active");
    currentTuningCategory = chip.dataset.cat || "all";
    renderPartsShop();
  });
});

const evolutionCloseBtn = document.querySelector("#evolutionCloseBtn");
if (evolutionCloseBtn) {
  evolutionCloseBtn.addEventListener("click", () => {
    const modal = document.querySelector("#evolutionTournamentsModal");
    if (modal) modal.hidden = true;
  });
}

queryAll(".evo-era-chip").forEach(chip => {
  chip.addEventListener("click", () => {
    queryAll(".evo-era-chip").forEach(c => c.classList.remove("active"));
    chip.classList.add("active");
    currentEvoEra = chip.dataset.era || "classic";
    renderEvolutionCups();
  });
});

const careerFactoryBtn = document.querySelector("#careerFactoryBtn");
if (careerFactoryBtn) {
  careerFactoryBtn.addEventListener("click", () => {
    openFactoryLadder();
  });
}

const factoryModalCloseBtn = document.querySelector("#factoryModalCloseBtn");
if (factoryModalCloseBtn) {
  factoryModalCloseBtn.addEventListener("click", () => {
    const modal = document.querySelector("#factoryMissionsModal");
    if (modal) modal.hidden = true;
  });
}

queryAll(".fac-tier-chip").forEach(chip => {
  chip.addEventListener("click", () => {
    queryAll(".fac-tier-chip").forEach(c => c.classList.remove("active"));
    chip.classList.add("active");
    currentFactoryTier = parseInt(chip.dataset.tier || "1", 10);
    renderFactoryMissions();
  });
});

const briefingStartBtn = document.querySelector("#briefingStartBtn");
if (briefingStartBtn) {
  briefingStartBtn.addEventListener("click", async () => {
    const modal = document.querySelector("#eventBriefingModal");
    if (modal) modal.hidden = true;
    if (!activeCareerEvent) return;

    let specificScn = null;
    if (activeCareerEvent.is_factory && activeCareerEvent.mission_obj) {
      const code = String(activeCareerEvent.mission_obj.code || "");
      const tier = activeCareerEvent.mission_obj.tier || 1;
      const trackId = (activeCareerEvent.trackId || "").toLowerCase();
      if (code.toUpperCase() === "0M01") {
        specificScn = `${trackId}_st1.scn`;
      } else if (code.match(/^\d+m\d+$/i)) {
        const num = parseInt(code.slice(2), 10);
        specificScn = `${trackId}_st${tier}${num}.scn`;
      }
    }

    modeSelect.value = "track";
    updateOptions();
    targetSelect.value = activeCareerEvent.trackId;
    if (!await loadTarget(specificScn)) {
      if (modal) modal.hidden = false;
      return;
    }

    // Load authentic car model for this career event
    let targetCar = "boxster";
    if (activeCareerEvent.is_factory) {
      targetCar = activeCareerEvent.mission_obj?.car_model || activeCareerEvent.car_sim || "boxster";
    } else {
      const selectedIdx = currentProfile?.selected_car_index || 0;
      targetCar = currentProfile?.garage?.[selectedIdx]?.model_name || "356_1";
    }
    targetCar = targetCar.toLowerCase();

    try {
      const selectedIdx = currentProfile?.selected_car_index || 0;
      const simName = activeCareerEvent.is_factory
        ? activeCareerEvent.mission_obj?.car_sim || activeCareerEvent.car_sim
        : currentProfile?.garage?.[selectedIdx]?.sim_name;
      viewer.set_car_setup?.(false, true); // not a Quick Race: automatic box, damage on
      await loadRaceCar(targetCar, simName);
    } catch (error) {
      setStatus(`Заезд не начат: ${error instanceof Error ? error.message : String(error)}`);
      if (modal) modal.hidden = false;
      return;
    }

    if (viewer) {
      if (!viewer.is_sim_mode()) viewer.toggle_sim_mode();
      const isDuel = activeCareerEvent.mission_obj?.type === "duel" || (activeCareerEvent.mission_obj?.title || "").toLowerCase().includes("duel");
      if (typeof viewer.configure_mission === "function") {
        viewer.configure_mission(
          Boolean(activeCareerEvent.is_factory),
          activeCareerEvent.time_limit || 0.0,
          Boolean(isDuel)
        );
      }
      if (typeof viewer.configure_mission_code === "function") {
        viewer.configure_mission_code(activeCareerEvent.is_factory
          ? String(activeCareerEvent.mission_obj?.code || "") : "");
      }
      if (!viewer.is_drive_mode()) {
        viewer.toggle_camera_mode();
        setMenuVisible(false);
        syncTourButton();
      }
      viewer.restart_race();
      eventFinishedHandled = false;
      soundManager.playCue(880, 100);
      setStatus(`Карьерный заезд начат: ${activeCareerEvent.title}`);
    }
  });
}

const briefingCancelBtn = document.querySelector("#briefingCancelBtn");
if (briefingCancelBtn) {
  briefingCancelBtn.addEventListener("click", () => {
    const modal = document.querySelector("#eventBriefingModal");
    if (modal) modal.hidden = true;
    activeCareerEvent = null;
  });
}

const briefingMapBtn = document.querySelector("#briefingMapBtn");
if (briefingMapBtn) {
  briefingMapBtn.addEventListener("click", () => {
    const modal = document.querySelector("#eventBriefingModal");
    if (!modal || modal.dataset.careerMode !== "factory") return;
    const showMap = modal.dataset.factoryView !== "map";
    modal.dataset.factoryView = showMap ? "map" : "briefing";
    briefingMapBtn.textContent = showMap ? "БРИФИНГ" : "КАРТА";
  });
}

const raceContinueBtn = document.querySelector("#raceContinueBtn");
if (raceContinueBtn) {
  raceContinueBtn.addEventListener("click", () => {
    const wasFactory = Boolean(activeCareerEvent?.is_factory);
    if (raceResultsModal) raceResultsModal.hidden = true;
    activeCareerEvent = null;
    setMenuVisible(true);
    if (wasFactory) openFactoryLadder();
  });
}

// Original in-race pause (docs/re/pause.md, web/pause-menu.js): the race clock and the scheduler stop
// (viewer.set_paused), music stops through the phase 3 event (Audio_Pause 0x4ab150).
const pauseMenu = new PauseMenu();
let pauseText = null; // { drawText, textWidth } of frontend.js, loaded with the HUD

function pauseLive() {
  if (!viewer || frontEnd?.visible || !viewer.is_drive_mode?.() || !viewer.has_car?.()) return false;
  const phase = viewer.get_race_phase?.() ?? 0;
  return phase === 1 || phase === 2;
}

function pauseSync() {
  if (!pauseCanvas) return;
  const rect = canvas.getBoundingClientRect();
  const W = Math.max(1, Math.round(rect.width));
  const H = Math.max(1, Math.round(rect.height));
  const dpr = window.devicePixelRatio || 1;
  if (pauseCanvas.width !== Math.round(W * dpr) || pauseCanvas.height !== Math.round(H * dpr)) {
    pauseCanvas.width = Math.round(W * dpr);
    pauseCanvas.height = Math.round(H * dpr);
  }
  pauseCanvas.getContext("2d").setTransform(dpr, 0, 0, dpr, 0, 0);
  pauseMenu.resize(W, H);
}

function pauseDraw() {
  if (!pauseCanvas) return;
  pauseCanvas.hidden = !pauseMenu.isOpen;
  if (!pauseMenu.isOpen) return;
  pauseSync();
  pauseMenu.strings = raceHud?.strings ?? {};
  drawPauseMenu(pauseCanvas.getContext("2d"), pauseMenu, {
    fe: raceHud?.fe, fonts: raceHud?.fonts ?? {}, ...(pauseText ?? {}),
  });
}

function openPause(frozen = false) {
  if (!frozen && !pauseLive()) return false;
  viewer.set_paused(true);
  pauseMenu.open();
  clearNavigation();
  ensureRaceHud().then(() => import("./frontend.js")).then(fe => {
    pauseText = { drawText: fe.drawText, textWidth: fe.textWidth };
    pauseDraw();
  }).catch(() => {});
  pauseDraw();
  queueFrame();
  return true;
}

function closePause() {
  pauseMenu.close();
  pauseDraw();
}

// Menu codes of PauseMenu_Handle 0x42a6a0 / ExitMenu_Handle 0x421b70 and the loop reaction of 0x4152e0.
function pauseCode(code) {
  if (code === null || code === undefined) { pauseDraw(); return; }
  switch (code) {
    case PAUSE_CODE.CONTINUE: // Audio_Resume, unpause
      closePause();
      viewer.set_paused(false);
      break;
    case PAUSE_CODE.RACE_AGAIN: // restart from the grid (A153)
      closePause();
      viewer.restart_race();
      if (raceResultsModal) raceResultsModal.hidden = true;
      eventFinishedHandled = false;
      break;
    case PAUSE_CODE.EXIT_WINDOWS: // A153: acts as exit to game menu
    case PAUSE_CODE.EXIT_GAME_MENU: // leave the race loop to the front end; the world stays frozen
      closePause();
      activeCareerEvent = null;
      setMenuVisible(true);
      break;
    default: // quit... / cancel switch the menu, nothing else happens
      pauseDraw();
  }
  queueFrame();
}

if (pauseCanvas) {
  const local = e => { const r = pauseCanvas.getBoundingClientRect(); return [e.clientX - r.left, e.clientY - r.top]; };
  pauseCanvas.addEventListener("pointermove", e => { pauseMenu.pointerMove(...local(e)); pauseDraw(); });
  pauseCanvas.addEventListener("pointerdown", e => { e.preventDefault(); pauseCode(pauseMenu.click(...local(e))); });
}

function setMenuVisible(visible) {
  menuVisible = visible;
  // The original front end replaces the web shell's menu when it is active.
  if (frontEnd) {
    if (visible) showFrontEnd();
    else frontEnd.hide();
    syncFrontEndClasses();
  }
  document.body.classList.toggle("menu-hidden", !visible);
  if (!visible && viewer?.is_paused?.() && !pauseMenu.isOpen) openPause(true);
  menuButton.setAttribute("aria-expanded", String(visible));
  clearNavigation();
  if (visible) menuButton.focus();
  else canvas.focus();
  queueFrame();
}

function toggleHud() {
  hudVisible = !hudVisible;
  hudButton.setAttribute("aria-pressed", String(hudVisible));
  if (!hudVisible && raceHud) raceHud.clear();
  syncTourButton();
}

menuButton.addEventListener("click", () => setMenuVisible(!menuVisible));
document.querySelector("#hideMenuButton").addEventListener("click", () => setMenuVisible(false));
hudButton.addEventListener("click", toggleHud);
detailsButton.addEventListener("click", () => {
  summary.hidden = !summary.hidden;
  detailsButton.setAttribute("aria-expanded", String(!summary.hidden));
});

let viewer = null;
let selectedFiles = [];
let localGameAvailable = false;
let localGameFiles = [];
let useCustomFiles = false;

let dragging = false;
let lastPointer = null;
let needsFrame = true;

let availableCars = [];
let availableTracks = [];
let loadGeneration = 0;

function setStatus(text) {
  statusLine.textContent = text;
}

function queueFrame() {
  needsFrame = true;
}

function selectedName(file) {
  return file.webkitRelativePath || file.name;
}

function updateSourceUI() {
  if (!sourceBadge) return;
  if (useCustomFiles && selectedFiles.length > 0) {
    sourceBadge.textContent = "Пользовательские файлы";
    sourceBadge.classList.add("custom");
    if (resetSourceButton && localGameAvailable) {
      resetSourceButton.style.display = "inline-flex";
    }
  } else if (localGameAvailable) {
    sourceBadge.textContent = "local/game";
    sourceBadge.classList.remove("custom");
    if (resetSourceButton) {
      resetSourceButton.style.display = "none";
    }
  } else {
    sourceBadge.textContent = "Файлы не выбраны";
    sourceBadge.classList.remove("custom");
    if (resetSourceButton) {
      resetSourceButton.style.display = "none";
    }
  }
}

function scanLocalGameCatalog() {
  const basenames = new Set(
    localGameFiles.map((p) => p.split(/[\\/]/).pop().toLowerCase())
  );
  availableCars = [...basenames]
    .filter((name) => name.endsWith(".crp") && basenames.has(name.slice(0, -4) + ".tpg"))
    .map((name) => name.slice(0, -4))
    .sort();

  availableTracks = [...basenames]
    .filter((name) => name.endsWith(".crp") && basenames.has(name.slice(0, -4) + ".fsh"))
    .map((name) => name.slice(0, -4))
    .sort();
}

async function checkLocalGame() {
  try {
    const res = await fetch("/api/game-files");
    if (!res.ok) return false;
    const files = await res.json();
    if (Array.isArray(files) && files.length > 0) {
      localGameFiles = files;
      localGameAvailable = true;
      if (!gameAudio.initStarted) { gameAudio.initStarted = true; gameAudio.init(files); }
      if (!useCustomFiles) {
        scanLocalGameCatalog();
      }
      updateSourceUI();
      updateOptions();
      return true;
    }
  } catch {
    // Offline or server without /api/game-files
  }
  return false;
}

function updateOptions() {
  const isTrack = modeSelect.value === "track";
  targetLabel.textContent = isTrack ? "Трасса" : "Авто";
  if (topologyField) {
    topologyField.style.display = isTrack ? "inline-flex" : "none";
  }
  syncTourButton();

  const knownList = isTrack ? KNOWN_TRACKS : KNOWN_CARS;
  const availableSet = new Set(isTrack ? availableTracks : availableCars);
  const previousValue = targetSelect.value;

  targetSelect.replaceChildren();

  // If there are discovered items not in known list, collect them
  const knownIds = new Set(knownList.map((k) => k.id));
  const extraItems = [];
  for (const id of availableSet) {
    if (!knownIds.has(id)) {
      extraItems.push({ id, name: id });
    }
  }

  const combined = [...knownList, ...extraItems];
  const hasSource = (useCustomFiles && selectedFiles.length > 0) || localGameAvailable;

  for (const item of combined) {
    const opt = document.createElement("option");
    opt.value = item.id;
    const isAvail = availableSet.has(item.id);
    if (hasSource) {
      opt.textContent = isAvail ? item.name : `(нет файлов) ${item.name}`;
      opt.disabled = !isAvail;
    } else {
      opt.textContent = item.name;
    }
    targetSelect.appendChild(opt);
  }

  // Restore previous selection if still valid and enabled
  const options = Array.from(targetSelect.options);
  const matchingOption = options.find((o) => o.value === previousValue && !o.disabled);
  if (matchingOption) {
    targetSelect.value = matchingOption.value;
  } else {
    const firstEnabled = options.find((o) => !o.disabled);
    if (firstEnabled) {
      targetSelect.value = firstEnabled.value;
    } else if (options.length > 0) {
      targetSelect.selectedIndex = 0;
    }
  }
}

modeSelect.addEventListener("change", () => {
  updateOptions();
  setStatus(`Режим: ${modeSelect.value === "track" ? "Трасса" : "Автомобиль"}.`);
  const canLoad = (useCustomFiles && selectedFiles.length > 0) || localGameAvailable;
  if (canLoad && viewer) {
    loadTarget();
  }
});

targetSelect.addEventListener("change", () => {
  const canLoad = (useCustomFiles && selectedFiles.length > 0) || localGameAvailable;
  if (canLoad && viewer) {
    loadTarget();
  }
});

topologyCheckbox.addEventListener("change", () => {
  if (viewer) {
    viewer.set_show_topology(topologyCheckbox.checked);
    queueFrame();
    setStatus(`3D границы: ${topologyCheckbox.checked ? "включены" : "выключены"}.`);
  }
});

function rememberFiles(files) {
  selectedFiles = Array.from(files || []);
  if (selectedFiles.length > 0) {
    useCustomFiles = true;
    const basenames = new Set(
      selectedFiles.map((file) => selectedName(file).split(/[\\/]/).pop().toLowerCase())
    );
    availableCars = [...basenames]
      .filter((name) => name.endsWith(".crp") && basenames.has(name.slice(0, -4) + ".tpg"))
      .map((name) => name.slice(0, -4))
      .sort();

    availableTracks = [...basenames]
      .filter((name) => name.endsWith(".crp") && basenames.has(name.slice(0, -4) + ".fsh"))
      .map((name) => name.slice(0, -4))
      .sort();
  } else if (localGameAvailable) {
    useCustomFiles = false;
    scanLocalGameCatalog();
  }

  updateSourceUI();
  updateOptions();
  const count = selectedFiles.length;
  setStatus(
    count
      ? `Выбрано файлов вручную: ${count}. Трасс: ${availableTracks.length}, Авто: ${availableCars.length}.`
      : (localGameAvailable ? `Используются файлы из local/game (${availableTracks.length} трасс, ${availableCars.length} авто).` : "Файлы не выбраны.")
  );

  if (viewer && (availableTracks.length > 0 || availableCars.length > 0)) {
    loadTarget();
  }
}

folderInput.addEventListener("change", () => rememberFiles(folderInput.files));
fileInput.addEventListener("change", () => rememberFiles(fileInput.files));

if (resetSourceButton) {
  resetSourceButton.addEventListener("click", () => {
    useCustomFiles = false;
    selectedFiles = [];
    if (folderInput) folderInput.value = "";
    if (fileInput) fileInput.value = "";
    scanLocalGameCatalog();
    updateSourceUI();
    updateOptions();
    setStatus(`Возврат к local/game (${availableTracks.length} трасс, ${availableCars.length} авто).`);
    if (viewer) {
      loadTarget();
    }
  });
}

function findTrackFilesFromCatalog(target, specificScn = null, reverse = false) {
  const matching = [];
  const targetLower = target.toLowerCase();
  const scnTarget = specificScn ? specificScn.toLowerCase() : null;

  for (const relPath of localGameFiles) {
    const fullPath = relPath.replace(/\\/g, "/").toLowerCase();
    const basename = fullPath.split("/").pop();
    const stem = basename.replace(/\.[^.]+$/, "");
    const ext = basename.split(".").pop();

    if (fullPath.includes("/sky/") && (ext === "fsh" || ext === "bin" || ext === "txt") && stem === targetLower) {
      matching.push(relPath);
      continue;
    }
    if (fullPath.includes("/sky/") || fullPath.includes("/sky new/")) continue;
    if (fullPath.includes("/fedata/") || fullPath.includes("/trackart/") || fullPath.includes("/cardata/")) continue;

    if (ext === "scn") {
      // The start scene of the race direction (FUN_0046d600): the start line trigger.
      if (basename === `${targetLower}_${reverse ? "b" : "f"}start.scn`) {
        matching.push(relPath);
        continue;
      }
      if (basename.includes("audio") || basename.includes("camera") || basename.includes("start")) {
        continue;
      }
      if (scnTarget) {
        if (basename === scnTarget) {
          matching.push(relPath);
        }
      } else {
        if (stem.startsWith(targetLower) && !stem.match(/_st\d+$/i)) {
          matching.push(relPath);
        }
      }
      continue;
    }

    if (
      [
        `${targetLower}.crp`,
        `${targetLower}.fsh`,
        `${targetLower}.env`,
        `${targetLower}.edg`,
        `${targetLower}.map`,
        `${targetLower}.jnc`,
        "animdefs.txt",
        `${targetLower}0.lsp`,
        `${targetLower}1.lsp`,
        `${targetLower}.lsp`,
      ].includes(basename)
    ) {
      matching.push(relPath);
    }
  }
  return matching;
}

function findCarFilesFromCatalog(target) {
  const matching = [];
  const targetLower = target.toLowerCase();
  const commonFsh = new Set([
    "intglass.fsh",
    "shadow.fsh",
    "cabrio.fsh",
    "carcmn.fsh",
    "price.fsh",
    "head.fsh",
    "suit.fsh",
  ]);

  for (const relPath of localGameFiles) {
    const fullPath = relPath.replace(/\\/g, "/").toLowerCase();
    const basename = fullPath.split("/").pop();
    const stem = basename.replace(/\.[^.]+$/, "");
    const ext = basename.split(".").pop();

    // Glare and shadow textures (R3DCar_DrawGlare 0x43cdd0, R3DCar_DrawShadow 0x43b820).
    if (
      fullPath.endsWith("/render/common.fsh") ||
      fullPath.endsWith("/render/particle.fsh") ||
      fullPath.endsWith("/render/carpart.dat") ||
      fullPath.endsWith("/render/particle.dat")
    ) {
      matching.push(relPath);
      continue;
    }
    if (!fullPath.includes("carmodel")) continue;

    if (stem === targetLower && ["crp", "tpg", "clr"].includes(ext)) {
      matching.push(relPath);
      continue;
    }

    if (ext === "fsh") {
      if (
        stem.startsWith(targetLower) ||
        stem.startsWith("f" + targetLower) ||
        commonFsh.has(basename) ||
        stem.startsWith("eas") ||
        stem.startsWith("eden")
      ) {
        matching.push(relPath);
      }
    }
  }
  return matching;
}

async function fetchGameFiles(paths) {
  const fetchPromises = paths.map(async (relPath) => {
    const url = "/game/" + relPath.replace(/\\/g, "/");
    const resp = await fetch(url);
    if (!resp.ok) {
      throw new Error(`Не удалось загрузить ${relPath}: HTTP ${resp.status}`);
    }
    const buf = await resp.arrayBuffer();
    return {
      name: relPath.replace(/\\/g, "/"),
      data: new Uint8Array(buf),
    };
  });
  const results = await Promise.all(fetchPromises);
  const names = [];
  const bytes = [];
  for (const res of results) {
    names.push(res.name);
    bytes.push(res.data);
  }
  return { names, bytes };
}

// Model names and simulation names are different identifiers (e.g. boxster / boxster25).
// Resolve only the requested SIM; ambiguous or missing resources must not choose another car.
function findSimResource(resources, simName, nameOf = value => value) {
  if (typeof simName !== "string" || !simName.trim()) {
    throw new Error("Не указан файл физики автомобиля.");
  }
  const basename = simName.trim().split(/[\\/]/).pop().replace(/\.sim$/i, "").toLowerCase() + ".sim";
  let matches = resources.filter(value => nameOf(value).split(/[\\/]/).pop().toLowerCase() === basename);
  // 959coupe29 and 928coupeGTS54 also sit in GameData/Simulation; the original reads CarData.
  if (matches.length > 1) {
    const inCarData = matches.filter(value => /(^|[\\/])simulation[\\/]cardata[\\/]/i.test(nameOf(value)));
    if (inCarData.length === 1) matches = inCarData;
  }
  if (matches.length !== 1) {
    throw new Error(matches.length ? `Найдено несколько файлов ${basename}.` : `Не найден файл ${basename}.`);
  }
  return matches[0];
}

async function loadCarSim(simName) {
  let data;
  if (useCustomFiles && selectedFiles.length > 0) {
    const file = findSimResource(selectedFiles, simName, selectedName);
    data = new Uint8Array(await file.arrayBuffer());
  } else {
    const path = findSimResource(localGameFiles, simName);
    data = (await fetchGameFiles([path])).bytes[0];
  }
  viewer.set_car_sim(data, simName);
  await loadCarAis();
}

// The original AI needs the car's `.ais` (GameData/Simulation/AICarData/<model>.ais,
// default.ais as the game's fallback). Missing data keeps the kinematic AI.
async function loadCarAis() {
  if (typeof viewer.set_car_ais !== "function") return;
  try {
    const model = String(viewer.car_ais_model() || "default").toLowerCase();
    for (const want of [`${model}.ais`, "default.ais"]) {
      let data = null;
      if (useCustomFiles && selectedFiles.length > 0) {
        const file = selectedFiles.find(f => selectedName(f).split(/[\\/]/).pop().toLowerCase() === want);
        if (file) data = new Uint8Array(await file.arrayBuffer());
      } else {
        const path = localGameFiles.find(p => p.split(/[\\/]/).pop().toLowerCase() === want
          && /aicardata/i.test(p));
        if (path) data = (await fetchGameFiles([path])).bytes[0];
      }
      if (data) {
        viewer.set_car_ais(data);
        return;
      }
    }
  } catch (e) {
    console.warn("AI car data:", e);
  }
}

// `bank` is the catalogue record's engine bank; without it the first record with this SIM is used.
// `gauges` is the record's needle calibration (nfs5.car +0x2f0..+0x2fc, docs/re/cockpit.md);
// without it the first record with this model and SIM is used.
// `style` is the record's TPG style index (nfs5.car i16 +0x1c0: body variant, `[style{N+1}]`).
async function loadRaceCar(model, simName, bank = "", gauges = null, style = 0) {
  const target = model.toLowerCase();
  raceHudCarIndex = quickRaceCars.find(c => c.model_name.toLowerCase() === target)?.index ?? null;
  let files;
  if (useCustomFiles && selectedFiles.length > 0) {
    const selected = selectedFiles.filter(file => {
      const basename = selectedName(file).split(/[\\/]/).pop().toLowerCase();
      return [`${target}.crp`, `${target}.tpg`, `${target}.clr`, 'carpart.dat', 'particle.dat'].includes(basename) || basename.endsWith('.fsh');
    });
    files = { names: selected.map(selectedName), bytes: await Promise.all(selected.map(async file => new Uint8Array(await file.arrayBuffer()))) };
  } else {
    files = await fetchGameFiles(findCarFilesFromCatalog(target));
  }
  if (!files.names.some(name => name.split(/[\\/]/).pop().toLowerCase() === `${target}.crp`)) {
    throw new Error(`Не найдена модель ${target}.crp.`);
  }
  viewer.set_car_model(files.names, files.bytes, target, style);
  await setCarGauges(target, simName, gauges);
  await loadCarSim(simName);
  lastEngineSoundError = await loadEngineSound(simName, bank);
  const tyreError = await loadTyreSound();
  if (tyreError && !lastEngineSoundError) lastEngineSoundError = tyreError;
}

// Cockpit needle calibration of the race car; the needles rest at 0 if nfs5.car is missing.
async function setCarGauges(model, simName, gauges) {
  if (typeof viewer.set_car_gauges !== "function") return;
  try {
    let values = gauges;
    if (!values) {
      const records = await loadCarCatalogueRecords();
      const sim = String(simName || "").toLowerCase();
      const match = records.find(r => r.model.toLowerCase() === model && r.sim_name.toLowerCase() === sim)
        || records.find(r => r.model.toLowerCase() === model);
      values = match?.gauges;
    }
    if (Array.isArray(values) && values.length === 4) viewer.set_car_gauges(...values);
  } catch (error) {
    console.warn("Cockpit gauges unavailable:", error);
  }
}

// Bytes of one game file found by base name (preferring a path that contains `hint`).
async function readGameFileByName(basename, hint = "") {
  const want = basename.toLowerCase();
  const base = value => value.split(/[\\/]/).pop().toLowerCase();
  if (useCustomFiles && selectedFiles.length > 0) {
    const matches = selectedFiles.filter(file => base(selectedName(file)) === want);
    const file = matches.find(f => selectedName(f).toLowerCase().includes(hint)) || matches[0];
    if (!file) throw new Error(`нет файла ${basename}`);
    return { name: selectedName(file), data: new Uint8Array(await file.arrayBuffer()) };
  }
  const matches = localGameFiles.filter(path => base(path) === want);
  const path = matches.find(p => p.toLowerCase().includes(hint)) || matches[0];
  if (!path) throw new Error(`нет файла ${basename}`);
  const fetched = await fetchGameFiles([path]);
  return { name: fetched.names[0], data: fetched.bytes[0] };
}

// Loads the original engine sound of the race car (nfs5.car -> bank, <sim>.spl, <bank>.viv).
// Never throws: returns "" on success or the reason, and the race goes on without engine sound.
let lastEngineSoundError = "";
async function loadEngineSound(simName, recordBank = "") {
  engineSound.unload();
  try {
    const sim = String(simName || "").trim().replace(/\.sim$/i, "");
    if (!sim) throw new Error("не указан SIM автомобиля");
    const car = await readGameFileByName("nfs5.car", "fedata");
    const bank = recordBank || engine_bank_for_sim(car.data, sim);
    const spl = await readGameFileByName(`${sim}.spl`, "sounds");
    const viv = await readGameFileByName(`${bank}.viv`, "sounds");
    engineSound.load([spl.name, viv.name], [spl.data, viv.data], sim, bank);
    return "";
  } catch (error) {
    engineSound.unload();
    const reason = error instanceof Error ? error.message : String(error);
    console.warn("Engine sound unavailable:", reason);
    return `Звук двигателя недоступен: ${reason}`;
  }
}

// Loads the original tyre sounds (SND_coll.bnk). Never throws: returns "" or the reason.
async function loadTyreSound() {
  tyreSound.unload();
  try {
    const bank = await readGameFileByName("SND_coll.bnk", "sounds");
    tyreSound.load([bank.name], [bank.data]);
    return "";
  } catch (error) {
    tyreSound.unload();
    const reason = error instanceof Error ? error.message : String(error);
    console.warn("Tyre sound unavailable:", reason);
    return `Звук шин недоступен: ${reason}`;
  }
}

const quickRaceModal = document.querySelector('#quickRaceModal');
const quickRaceTrack = document.querySelector('#quickRaceTrack');
const quickRaceCar = document.querySelector('#quickRaceCar');
const quickRaceStartBtn = document.querySelector('#quickRaceStartBtn');
const quickRaceCancelBtn = document.querySelector('#quickRaceCancelBtn');
const quickRaceStatus = document.querySelector('#quickRaceStatus');
let quickRaceCars = [];

// Quick Race cars come from the original catalogue FEData/Data/nfs5.car: records with a SIM name
// whose model (.crp, matched case-insensitively) and SIM file are installed, in catalogue order.
// Keyed by record index because SIM names repeat with different engine banks.
function buildQuickRaceCars(records, installedModels, resources) {
  const models = new Set(installedModels.map(name => name.toLowerCase()));
  const cars = [];
  for (const record of records) {
    if (!record.sim_name || !models.has(record.model.toLowerCase())) continue;
    try { findSimResource(resources, record.sim_name); } catch { continue; }
    cars.push({
      id: String(record.index),
      index: record.index,
      display_name: record.display_name,
      model_name: record.model.toLowerCase(),
      sim_name: record.sim_name,
      bank: record.bank,
      gauges: record.gauges,
      colours: record.colours,
      ratings: record.ratings,
      group: record.group,
      year: record.year,
      style: record.style ?? 0,
    });
  }
  return cars;
}

async function loadCarCatalogueRecords() {
  let bytes;
  if (useCustomFiles && selectedFiles.length > 0) {
    const file = selectedFiles.find(f => /(^|[\\/])nfs5\.car$/i.test(selectedName(f)));
    if (!file) throw new Error('В выбранных файлах нет FEData/Data/nfs5.car.');
    bytes = new Uint8Array(await file.arrayBuffer());
  } else {
    const path = localGameFiles.find(p => /(^|[\\/])fedata[\\/]data[\\/]nfs5\.car$/i.test(p));
    if (!path) throw new Error('Не найден FEData/Data/nfs5.car.');
    bytes = (await fetchGameFiles([path])).bytes[0];
  }
  return JSON.parse(parse_car_catalogue_json(bytes));
}

async function openQuickRace() {
  quickRaceModal.hidden = false;
  quickRaceStatus.textContent = '';
  const populate = (select, options) => {
    const previous = select.value;
    select.replaceChildren();
    for (const [value, label] of options) {
      const option = document.createElement('option');
      option.value = value;
      option.textContent = label;
      select.appendChild(option);
    }
    if (options.some(([value]) => value === previous)) select.value = previous;
  };
  populate(quickRaceTrack, availableTracks.map(id => [id, KNOWN_TRACKS.find(track => track.id === id)?.name || id]));
  try {
    const resources = useCustomFiles ? selectedFiles.map(selectedName) : localGameFiles;
    quickRaceCars = buildQuickRaceCars(await loadCarCatalogueRecords(), availableCars, resources);
    populate(quickRaceCar, quickRaceCars.map(car => [car.id, car.display_name]));
    quickRaceStartBtn.disabled = !quickRaceCars.length || !availableTracks.length;
    if (quickRaceStartBtn.disabled) quickRaceStatus.textContent = 'Выберите папку игры с трассами, моделями и файлами физики автомобилей.';
  } catch (error) {
    quickRaceCars = [];
    populate(quickRaceCar, []);
    quickRaceStartBtn.disabled = true;
    quickRaceStatus.textContent = error instanceof Error ? error.message : String(error);
  }
  quickRaceTrack.focus();
}
document.querySelector('#quickRaceBtn').addEventListener('click', openQuickRace);
quickRaceCancelBtn.addEventListener('click', () => {
  if (!quickRaceCancelBtn.disabled) quickRaceModal.hidden = true;
});
quickRaceStartBtn.addEventListener('click', async () => {
  if (quickRaceStartBtn.disabled) return;
  const car = quickRaceCars.find(car => car.id === quickRaceCar.value);
  if (!car) return;
  const track = quickRaceTrack.value;
  const laps = Number(document.querySelector('#quickRaceLaps').value);
  const opponents = Number(document.querySelector('#quickRaceOpponents').value);
  await launchQuickRace({ car, track, laps, opponents, fromDialog: true });
});

// nfs5.trk record +0x6e == 1: a circuit (LINEAR_TRACK 0x1d is its negation).
async function trackIsCircuit(track) {
  const bytes = await readGamePath("FEData/Data/nfs5.trk");
  const records = bytes ? JSON.parse(fe_track_catalog_json(bytes)) : [];
  return Boolean(records.find(r => r.internal_name.toLowerCase() === track.toLowerCase())?.circuit);
}

// Race choices of the running race, for the debug state and the loading screen.
let lastRaceStart = null;

// Difficulty (SKILL), mirror and the opponents drawn from nfs5.per for the level (FUN_004e3160);
// each opponent car gets its driver's skill values. The page flips the picture for MIRROR.
async function setRaceOptions({ difficulty, mirror, opponents, car = null, colour = null }) {
  const { playerPaint, opponentPaints } = await import("./fe-quickrace.js");
  const per = await readGamePath("FEData/Data/nfs5.per");
  let skills = [];
  if (per && opponents > 0) {
    const seed = (Math.random() * 0x7fffffff) >>> 0;
    const drivers = JSON.parse(fe_pick_opponents_json(per, opponents, difficulty, seed));
    skills = drivers.flatMap(d => d.skill);
    lastRaceDrivers = drivers;
  }
  // Colours: the chosen one for the player (null keeps the default paint), a random list entry
  // per opponent (OpponentCar apply 0x4f4fa0).
  const own = colour === null ? null : playerPaint(car, colour);
  const paints = opponentPaints(car, opponents, () => Math.floor(Math.random() * 0x7fff)).flat();
  const rgb24 = c => (Math.round(c[0] * 255) << 16) | (Math.round(c[1] * 255) << 8) | Math.round(c[2] * 255);
  raceCarColours = [own ? rgb24(own) : 0xffffff, ...Array.from({ length: paints.length / 4 }, (_, i) => rgb24(paints.slice(i * 4, i * 4 + 4)))];
  viewer.set_race_options(difficulty, mirror, new Float32Array(skills), new Float32Array(own ?? []),
    new Float32Array(paints));
  canvas.classList.toggle("mirror", Boolean(mirror));
}
let lastRaceDrivers = [];
// 0xRRGGBB paint of each car, player first: the HUD list square and map marker use it.
let raceCarColours = null;

// Load and start a Quick Race (web dialog or the original front end's PLAYGAME).
// manualBox / damage: the car setup screen's transmission and Game.Damage [VA 0x4eeda0, 0x4e5520];
// defaults are the Quick Race ones (automatic box, damage on).
// reverse / mirror / difficulty / traffic: Track1.Direction, Game.Difficutly, Track1.Traffic
// (docs/re/frontend.md "Race start parameters"). Traffic cars are not raced yet (A100).
async function launchQuickRace({ car, track, laps, opponents, manualBox = false, damage = true, fromDialog = false,
  reverse = false, mirror = false, difficulty = 1, traffic = 0, colour = null, circuit = null }) {
  quickRaceStartBtn.disabled = true;
  quickRaceCancelBtn.disabled = true;
  quickRaceStatus.textContent = 'Загрузка заезда…';
  clearNavigation();
  // Clear career identity before loading or restarting any race; no rewards are possible.
  activeCareerEvent = null;
  if (raceResultsModal) raceResultsModal.hidden = true;
  try {
    if (frontEnd) {
      // Race loading screen: era0tr.fsh outside Evolution plus the track picture [VA 0x4a4a70].
      quickRaceModal.hidden = true;
      await frontEnd.showLoading({ pack: "era0tr.fsh", track, reverse });
      syncFrontEndClasses();
    }
    modeSelect.value = 'track';
    updateOptions();
    targetSelect.value = track;
    if (!await loadTarget(null, reverse)) throw new Error(statusLine.textContent);
    viewer.set_car_setup?.(manualBox, damage);
    await loadRaceCar(car.model_name, car.sim_name, car.bank || "", car.gauges || null, car.style || 0);
    if (!viewer.is_sim_mode()) viewer.toggle_sim_mode();
    await setRaceOptions({ difficulty, mirror, opponents, car, colour });
    viewer.set_linear_track?.(!(circuit ?? await trackIsCircuit(track)));
    viewer.configure_quick_race(laps, opponents);
    lastRaceStart = { reverse, mirror, difficulty, traffic, opponents };
    if (!viewer.is_drive_mode()) viewer.toggle_camera_mode();
    quickRaceModal.hidden = true;
    setMenuVisible(false);
    syncTourButton();
    setStatus(`Quick Race: ${car.display_name} · ${track}${lastEngineSoundError ? ` · ${lastEngineSoundError}` : ""}`);
  } catch (error) {
    quickRaceStatus.textContent = error instanceof Error ? error.message : String(error);
    setStatus(quickRaceStatus.textContent);
    if (frontEnd) {
      await showFrontEnd();
      if (fromDialog) quickRaceModal.hidden = false;
    }
  } finally {
    quickRaceStartBtn.disabled = false;
    quickRaceCancelBtn.disabled = false;
  }
}

async function loadTarget(specificScn = null, reverse = false) {
  if (!viewer) {
    setStatus("WebGPU еще не готов.");
    return false;
  }
  const isCustom = useCustomFiles && selectedFiles.length > 0;
  if (!isCustom && !localGameAvailable) {
    setStatus("Выберите папку игры или файлы.");
    return false;
  }

  const isTrack = modeSelect.value === "track";
  const generation = ++loadGeneration;
  const customFiles = [...selectedFiles];
  clearNavigation();
  const target = targetSelect.value.trim().toLowerCase() || (isTrack ? "skidpad" : "356a");

  loadButton.disabled = true;
  targetSelect.disabled = true;
  const sourceText = isCustom ? "пользовательских файлов" : "local/game";
  setStatus(`Чтение файлов для ${isTrack ? "трассы" : "авто"} "${target}" из ${sourceText}...`);

  try {
    let names = [];
    let bytes = [];

    if (isCustom) {
      if (isTrack) {
        for (const file of customFiles) {
          const fullPath = selectedName(file).replace(/\\/g, "/").toLowerCase();
          const basename = fullPath.split("/").pop();
          const stem = basename.replace(/\.[^.]+$/, "");
          const ext = basename.split(".").pop();

          if (fullPath.includes("/sky/") && (ext === "fsh" || ext === "bin" || ext === "txt") && stem === target) {
            names.push(selectedName(file));
            bytes.push(new Uint8Array(await file.arrayBuffer()));
            continue;
          }
          if (fullPath.includes("/sky/") || fullPath.includes("/sky new/")) continue;
          if (fullPath.includes("/fedata/") || fullPath.includes("/trackart/") || fullPath.includes("/cardata/")) continue;

          if (ext === "scn") {
            if (basename === `${target}_${reverse ? "b" : "f"}start.scn`) {
              names.push(selectedName(file));
              bytes.push(new Uint8Array(await file.arrayBuffer()));
              continue;
            }
            if (basename.includes("audio") || basename.includes("camera") || basename.includes("start")) {
              continue;
            }
            if (specificScn) {
              if (basename === specificScn.toLowerCase()) {
                names.push(selectedName(file));
                bytes.push(new Uint8Array(await file.arrayBuffer()));
              }
            } else {
              if (stem.startsWith(target) && !stem.match(/_st\d+$/i)) {
                names.push(selectedName(file));
                bytes.push(new Uint8Array(await file.arrayBuffer()));
              }
            }
            continue;
          }

          if (
            [
              `${target}.crp`,
              `${target}.fsh`,
              `${target}.env`,
              `${target}.edg`,
              `${target}.map`,
              `${target}.jnc`,
              "animdefs.txt",
              `${target}0.lsp`,
              `${target}1.lsp`,
              `${target}.lsp`,
            ].includes(basename)
          ) {
            names.push(selectedName(file));
            bytes.push(new Uint8Array(await file.arrayBuffer()));
          }
        }
      } else {
        for (const file of customFiles) {
          const basename = selectedName(file).split(/[\\/]/).pop().toLowerCase();
          if (
            ![`${target}.crp`, `${target}.tpg`, `${target}.clr`, "carpart.dat", "particle.dat"].includes(basename) &&
            !basename.endsWith(".fsh")
          ) {
            continue;
          }
          names.push(selectedName(file));
          bytes.push(new Uint8Array(await file.arrayBuffer()));
        }
      }
    } else {
      const paths = isTrack
        ? findTrackFilesFromCatalog(target, specificScn, reverse)
        : findCarFilesFromCatalog(target);

      if (paths.length === 0) {
        throw new Error(`Файлы для ${isTrack ? "трассы" : "авто"} "${target}" не найдены в local/game.`);
      }

      setStatus(`Загрузка ${paths.length} файлов для "${target}" по сети...`);
      const fetched = await fetchGameFiles(paths);
      names = fetched.names;
      bytes = fetched.bytes;
    }

    if (generation !== loadGeneration) return false;
    if (names.length === 0) {
      throw new Error(`Файлы для ${isTrack ? "трассы" : "авто"} "${target}" не найдены.`);
    }

    if (isTrack) {
      const text = viewer.load_track_direction(names, bytes, target, Boolean(reverse));
      canvas.classList.remove("mirror");
      viewer.set_show_topology(topologyCheckbox.checked);
      summary.textContent = text;
      const hasTopo = viewer.has_topology();
      setStatus(`Трасса "${target}" загружена из ${sourceText}.${hasTopo ? " F — начать заезд. T — границы." : " Границы не найдены."}`);

      if (typeof viewer.set_car_model === "function") {
        try {
          const selIdx = currentProfile?.selected_car_index || 0;
          const carModel = (currentProfile?.garage?.[selIdx]?.model_name || "356_1").toLowerCase();
          let carNames = [];
          let carBytes = [];
          if (isCustom) {
            for (const file of customFiles) {
              const basename = selectedName(file).split(/[\\/]/).pop().toLowerCase();
              if (![`${carModel}.crp`, `${carModel}.tpg`, `${carModel}.clr`, "carpart.dat", "particle.dat"].includes(basename) && !basename.endsWith(".fsh")) {
                continue;
              }
              carNames.push(selectedName(file));
              carBytes.push(new Uint8Array(await file.arrayBuffer()));
            }
          } else {
            const carPaths = findCarFilesFromCatalog(carModel);
            if (carPaths.length > 0) {
              const fetchedCar = await fetchGameFiles(carPaths);
              carNames = fetchedCar.names;
              carBytes = fetchedCar.bytes;
            }
          }
          if (carNames.length > 0) {
            viewer.set_car_model(carNames, carBytes, carModel);
            // Free driving uses the selected garage car's SIM as well.
            await loadCarSim(currentProfile?.garage?.[selIdx]?.sim_name || "356coupe11");
          }
        } catch (carErr) {
          console.warn("Auto-load car model onto track failed:", carErr);
        }
      }
    } else {
      const text = viewer.load(names, bytes, target);
      summary.textContent = text;
      setStatus(`Авто "${target}" загружено из ${sourceText}. Горячая клавиша C меняет цвет.`);
    }

    syncTourButton();
    queueFrame();
    return true;
  } catch (error) {
    if (generation !== loadGeneration) return false;
    setStatus(error instanceof Error ? error.message : String(error));
    return false;
  } finally {
    if (generation === loadGeneration) {
      loadButton.disabled = false;
      targetSelect.disabled = false;
    }
  }
}

loadButton.addEventListener("click", () => loadTarget());

function syncTourButton() {
  if (!tourButton) return;
  const isTrack = modeSelect.value === "track";
  const hasCar = viewer ? viewer.has_car() : false;
  tourButton.style.display = isTrack ? "inline-flex" : "none";
  if (cameraButton) cameraButton.style.display = isTrack && hasCar ? "inline-flex" : "none";
  if (paintButton) paintButton.style.display = isTrack && hasCar ? "inline-flex" : "none";

  const isDrive = viewer ? viewer.is_drive_mode() : false;
  tourButton.classList.toggle("active", isDrive);
  tourButton.textContent = isDrive ? "Орбита (F)" : (hasCar ? "Заезд (F)" : "Прогон (F)");

  const isDriveAndCar = isDrive && hasCar;
  if (hudCanvas) hudCanvas.hidden = !(isDriveAndCar && hudVisible);
  if (!isDriveAndCar && raceHud) raceHud.clear();
  updateDebugPanel();
  if (!isDriveAndCar) {
    if (countdownBanner) countdownBanner.hidden = true;
    if (wrongWayBanner) wrongWayBanner.hidden = true;
    if (raceResultsModal) raceResultsModal.hidden = true;
  }
}

tourButton.addEventListener("click", () => {
  if (!viewer) return;
  const isDrive = viewer.toggle_camera_mode();
  setMenuVisible(!isDrive);
  syncTourButton();
  setStatus(
    isDrive
      ? (viewer.has_car()
          ? "Режим заезда на авто включён (F). W: газ, S: тормоз/назад, A/D: руль, Space: ручник, C: камера, V: свободная камера, R: сброс."
          : "Режим прогона включён (F). WASD: полёт вдоль границ, Мышь: обзор, Shift: ускорение.")
      : "Орбитальная камера."
  );
  queueFrame();
});

if (cameraButton) {
  cameraButton.addEventListener("click", () => {
    if (!viewer) return;
    if (viewer.is_drive_mode() && viewer.has_car()) {
      cycleCarCamera();
      queueFrame();
    }
  });
}

if (paintButton) {
  paintButton.addEventListener("click", () => {
    if (!viewer) return;
    if (viewer.has_car()) {
      const idx = viewer.cycle_car_paint();
      const names = ["Guards Red", "Midnight Blue", "GT Silver", "Irish Green", "Speed Yellow", "Basalt Black"];
      setStatus(`Цвет авто: ${names[idx] || idx}`);
      queueFrame();
    }
  });
}

let panMode = false;

canvas.addEventListener("contextmenu", (event) => {
  event.preventDefault();
});

resetButton.addEventListener("click", () => {
  if (viewer) {
    if (viewer.is_drive_mode() && viewer.has_car()) {
      viewer.restart_race();
      if (raceResultsModal) raceResultsModal.hidden = true;
      setStatus("Гонка перезапущена / старт на решётке.");
    } else {
      viewer.reset_camera();
      setStatus("Камера сброшена.");
    }
    velForward = 0;
    velRight = 0;
    velZoom = 0;
    syncTourButton();
    queueFrame();
  }
});

canvas.addEventListener("pointerdown", (event) => {
  canvas.focus();
  dragging = true;
  panMode = event.button === 2 || event.button === 1 || event.shiftKey;
  lastPointer = { x: event.clientX, y: event.clientY };
  canvas.setPointerCapture(event.pointerId);
});

canvas.addEventListener("pointerup", (event) => {
  dragging = false;
  panMode = false;
  lastPointer = null;
  canvas.releasePointerCapture(event.pointerId);
});

canvas.addEventListener("pointercancel", () => {
  dragging = false;
  panMode = false;
  lastPointer = null;
});

canvas.addEventListener("pointermove", (event) => {
  if (!dragging || !viewer || !lastPointer) {
    return;
  }
  const dx = event.clientX - lastPointer.x;
  const dy = event.clientY - lastPointer.y;
  lastPointer = { x: event.clientX, y: event.clientY };
  if (panMode) {
    viewer.pan(dx, dy);
  } else {
    viewer.orbit(dx, dy);
  }
  queueFrame();
});

canvas.addEventListener(
  "wheel",
  (event) => {
    if (!viewer) {
      return;
    }
    event.preventDefault();
    viewer.zoom(event.deltaY);
    queueFrame();
  },
  { passive: false },
);

const activeKeys = new Set();
let lastTime = 0;
let velForward = 0;
let velRight = 0;
let velZoom = 0;

function isTyping(event) {
  return (
    event.target instanceof HTMLInputElement ||
    event.target instanceof HTMLTextAreaElement ||
    event.target instanceof HTMLSelectElement || event.target.isContentEditable
  );
}

function onKeyDown(event) {
  if (event.code === "Escape" && !event.repeat) {
    event.preventDefault();
    if (!quickRaceModal.hidden) {
      if (!quickRaceCancelBtn.disabled) quickRaceModal.hidden = true;
      return;
    }
    if (pauseMenu.isOpen) { pauseCode(pauseMenu.key("Escape")); return; }
    if (!menuVisible && openPause()) return;
    setMenuVisible(!menuVisible);
    return;
  }
  if (pauseMenu.isOpen) {
    if (["ArrowUp", "ArrowDown", "Enter", "NumpadEnter"].includes(event.code)) {
      event.preventDefault();
      pauseCode(pauseMenu.key(event.code) ?? undefined);
      pauseDraw();
    }
    return;
  }
  if (!quickRaceModal.hidden) return;
  if (isTyping(event)) return;
  if (event.ctrlKey || event.altKey || event.metaKey) return;
  if ((event.code === "KeyH" || event.code === "F5") && !event.repeat) {
    if (event.code === "F5") event.preventDefault?.();
    toggleHud();
    return;
  }
  if (event.code === "Backquote" && !event.repeat) {
    debugPanelVisible = !debugPanelVisible;
    try { localStorage.setItem("nfs5.debugPanel", debugPanelVisible ? "1" : "0"); } catch {}
    updateDebugPanel();
    return;
  }

  // Manual box (car setup screen): X shifts up, Z down (SHIFTUP/SHIFTDOWN events, A131).
  if ((event.code === "KeyX" || event.code === "KeyZ") && !event.repeat && viewer?.is_drive_mode?.()) {
    viewer.shift_gear?.(event.code === "KeyX" ? 1 : -1);
    return;
  }

  // Single-shot keys (ignore auto-repeat)
  // R: the original track reset (back on the road at the car's track node, the race goes
  // on). Shift+R: restart the race from the grid.
  if (event.code === "KeyR" && !event.repeat) {
    if (viewer) {
      if (viewer.is_drive_mode() && viewer.has_car() && !event.shiftKey
          && typeof viewer.request_car_reset === "function") {
        viewer.request_car_reset();
        setStatus("Сброс на трассу (R). Shift+R: перезапуск гонки.");
      } else if (viewer.is_drive_mode() && viewer.has_car()) {
        viewer.restart_race();
        if (raceResultsModal) raceResultsModal.hidden = true;
        setStatus("Гонка перезапущена / старт на решётке.");
      } else {
        viewer.reset_camera();
        setStatus("Камера сброшена.");
      }
      velForward = 0;
      velRight = 0;
      velZoom = 0;
      syncTourButton();
      queueFrame();
    }
    return;
  }
  if (event.code === "KeyT" && !event.repeat && modeSelect.value === "track") {
    topologyCheckbox.checked = !topologyCheckbox.checked;
    if (viewer) {
      viewer.set_show_topology(topologyCheckbox.checked);
      queueFrame();
    }
    setStatus(`3D границы: ${topologyCheckbox.checked ? "включены" : "выключены"}.`);
    return;
  }
  if (event.code === "KeyF" && !event.repeat && modeSelect.value === "track") {
    if (viewer) {
      const isDrive = viewer.toggle_camera_mode();
      setMenuVisible(!isDrive);
      syncTourButton();
      setStatus(
        isDrive
          ? (viewer.has_car()
              ? "Режим заезда на авто включён (F). W: газ, S: тормоз/назад, A/D: руль, Space: ручник, C: камера, V: свободная камера, R: сброс."
              : "Режим прогона включён (F). WASD: полёт вдоль границ, Мышь: обзор, Shift: ускорение.")
          : "Орбитальная камера."
      );
      queueFrame();
    }
    return;
  }
  if (event.code === "KeyB" && !event.repeat) {
    if (viewer) {
      const on = viewer.toggle_collision_boxes();
      setStatus(on ? "Collision boxes: shown (B)" : "Collision boxes: hidden (B)");
      if (on) console.log(viewer.collision_box_report());
      queueFrame();
    }
    return;
  }
  if (event.code === "KeyC" && !event.repeat) {
    if (viewer) {
      if (viewer.is_drive_mode() && viewer.has_car()) {
        cycleCarCamera();
      } else {
        viewer.cycle_paint_color();
      }
      queueFrame();
    }
    return;
  }
  if (event.code === "KeyL" && !event.repeat) {
    // Light switch (original Controller.Lights): headlights and tail lights.
    if (viewer && typeof viewer.toggle_lights === "function" && viewer.has_car()) {
      const on = viewer.toggle_lights();
      lightsOn = Boolean(on);
      setStatus(on ? "Фары включены (L)" : "Фары выключены (L)");
      queueFrame();
    }
    return;
  }
  if (event.code === "KeyV" && event.shiftKey && !event.repeat) {
    // Debug side chase view (not original): the drive camera swung 25° about the car.
    if (viewer && viewer.is_drive_mode() && viewer.has_car() && viewer.cycle_side_view) {
      const deg = viewer.cycle_side_view();
      setStatus(deg === 0 ? "Боковой вид выкл. (Shift+V)" : `Боковой вид ${deg > 0 ? "справа" : "слева"} ${Math.abs(deg)}° (Shift+V)`);
      queueFrame();
    }
    return;
  }
  if (event.code === "KeyV" && !event.repeat) {
    // Debug free camera, outside the original C-key cycle.
    if (viewer && viewer.is_drive_mode() && viewer.has_car()) {
      const free = viewer.toggle_free_camera();
      setStatus(free ? "Свободная камера (V)" : `Камера: ${viewer.get_camera_name()}`);
      queueFrame();
    }
    return;
  }
  if (event.code === "KeyP" && !event.repeat) {
    if (viewer) {
      if (viewer.has_car()) {
        const idx = viewer.cycle_car_paint();
        const names = ["Guards Red", "Midnight Blue", "GT Silver", "Irish Green", "Speed Yellow", "Basalt Black"];
        setStatus(`Цвет авто: ${names[idx] || idx}`);
      } else {
        viewer.cycle_paint_color();
      }
      queueFrame();
    }
    return;
  }

  // Prevent default page scroll on arrow keys or space
  if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Space"].includes(event.code)) {
    event.preventDefault();
  }

  activeKeys.add(event.code);
}
window.addEventListener("keydown", (event) => {
  onKeyDown(event);
  updateDebugPanel();
});

window.addEventListener("keyup", (event) => {
  activeKeys.delete(event.code);
});

// Original 2D race HUD (web/race-hud.js, docs/re/hud.md).
async function ensureRaceHud() {
  if (raceHud || raceHudLoading || !hudCanvas) return;
  raceHudLoading = true;
  try {
    const { RaceHud } = await import("./race-hud.js");
    const hudView = new RaceHud({
      canvas: hudCanvas, view: canvas, readFile: readGamePath,
      api: { fe_parse_layout, fe_decode_fsh, fe_parse_font, fe_parse_strings },
    });
    await hudView.init();
    const records = await readGamePath("FEData/Data/nfs5.car");
    hudView.carRecords = records;
    hudView.setCarRecord(records, raceHudCarIndex);
    raceHud = hudView;
  } catch (error) {
    console.error("race HUD failed", error);
  } finally {
    raceHudLoading = false;
  }
}

function drawRaceHud(dt) {
  if (!hudCanvas || typeof viewer?.get_hud_telemetry !== "function") return;
  if (!raceHud) { ensureRaceHud(); return; }
  if (!hudVisible) return;
  if (raceHud.boundIndex !== raceHudCarIndex) {
    raceHud.boundIndex = raceHudCarIndex;
    raceHud.setCarRecord(raceHud.carRecords, raceHudCarIndex);
  }
  try {
    raceHud.draw(viewer, { dt, lights: lightsOn, carColours: raceCarColours });
  } catch (error) {
    console.error("race HUD draw failed", error);
  }
}

function clearNavigation() {
  activeKeys.clear();
  velForward = 0;
  velRight = 0;
  velZoom = 0;
  lastTime = 0;
  dragging = false;
  lastPointer = null;
}
window.addEventListener("blur", clearNavigation);
document.addEventListener("visibilitychange", clearNavigation);
document.querySelector("#menu").addEventListener("focusin", clearNavigation);

function updateNavigation(dt) {
  if (!viewer) return;

  const isDriveCar = viewer.is_drive_mode() && viewer.has_car();
  if (isDriveCar && viewer.is_paused?.()) {
    // Paused: nothing steps; Audio_Pause sets the effect voices to volume 0 (re/audio.md) and the
    // phase 3 event stops the music director (re/music.md).
    engineSound.deactivate();
    tyreSound.deactivate();
    gameAudio.frame(viewer.get_race_phase(), 0);
    if (countdownBanner) countdownBanner.hidden = true;
    queueFrame();
    return;
  }
  if (isDriveCar) {
    let throttle = 0;
    if (activeKeys.has("KeyW") || activeKeys.has("ArrowUp")) throttle += 1.0;
    if (activeKeys.has("KeyS") || activeKeys.has("ArrowDown")) throttle -= 1.0;

    let steer = 0;
    if (activeKeys.has("KeyA") || activeKeys.has("ArrowLeft")) steer -= 1.0;
    if (activeKeys.has("KeyD") || activeKeys.has("ArrowRight")) steer += 1.0;

    const handbrake = activeKeys.has("Space");

    viewer.update_car(dt, throttle, steer, handbrake);

    // Update HUD display
    const speed = Math.round(viewer.get_car_speed());
    const gear = viewer.get_car_gear();
    const rpm = Math.max(0, Math.min(1, viewer.get_car_rpm()));

    engineSound.update(dt, viewer.get_car_engine_rpm(), viewer.get_car_redline_rpm(),
      Math.max(0, throttle), viewer.get_car_view(), gear);
    tyreSound.update(dt, viewer.get_tyre_sound_inputs(), viewer.get_car_view());

    // Update Race HUD
    const phase = viewer.get_race_phase ? viewer.get_race_phase() : 0;
    const isRacingActive = phase > 0;
    const isFinished = phase === 4 || phase === 5;
    // Dismissal does not end the simulation's terminal phase. Re-arm only when
    // a new attempt actually begins, including keyboard and toolbar restarts.
    if (!isFinished) eventFinishedHandled = false;

    // Factory missions keep a minimal status badge; races use the original HUD canvas.
    const isFactoryRun = Boolean(activeCareerEvent?.is_factory);
    if (facStatusBadge) {
      facStatusBadge.hidden = !(hudVisible && isRacingActive && isFactoryRun);
      if (isRacingActive && isFactoryRun) {
        const elapsed = (typeof viewer.get_mission_elapsed_time === "function")
          ? viewer.get_mission_elapsed_time()
          : viewer.get_current_lap_time();
        const timeLimit = activeCareerEvent.time_limit || 0;
        const limitText = timeLimit > 0 ? `${formatRaceTime(elapsed)} / ${timeLimit.toFixed(1)}с` : formatRaceTime(elapsed);
        const label = facStatusLabel;
        if (label) label.textContent = `ИСПЫТАНИЕ · ${limitText}`;
        if (facStatusVal && typeof viewer.get_cone_hits === "function") {
          const hits = viewer.get_cone_hits();
          facStatusVal.textContent = `КОНУСЫ: ${hits}`;
          facStatusVal.style.color = hits > 0 ? "#ef4444" : "#22c55e";
        }
        if (facStatusSub && typeof viewer.get_stunt_180 === "function") {
          const h180 = viewer.get_stunt_180();
          const h360 = viewer.get_stunt_360();
          const hj = viewer.get_stunt_jturn();
          let statusText = "ТЕСТ В ПРОЦЕССЕ";
          if (activeCareerEvent.mission_obj?.type === "spin360") {
            statusText = h360 ? "360°: ВЫПОЛНЕНО ✓" : "360°: ТРЕБУЕТСЯ";
          } else if (activeCareerEvent.mission_obj?.type === "slide180") {
            statusText = h180 ? "180°: ВЫПОЛНЕНО ✓" : "180°: ТРЕБУЕТСЯ";
          } else if (h360 || h180 || hj) {
            statusText = `СТАНТ: ${h360 ? "360° " : ""}${h180 ? "180° " : ""}${hj ? "J-TURN " : ""}✓`;
          }
          facStatusSub.textContent = statusText;
        }
      }
    }
    drawRaceHud(dt);

    // Countdown Banner (fallback only: the original H321Go is drawn by race-hud.js once the HUD is ready)
    if (countdownBanner && raceHud?.ready && raceHud.gauges) countdownBanner.hidden = true;
    else if (countdownBanner) {
      if (phase === 1) {
        // The original start lasts 7 s; "3, 2, 1" come in its last 3 s (Sched_Step 0x414ff0).
        const remaining = viewer.get_countdown_remaining();
        const digit = remaining > 3.0 ? null : remaining > 2.0 ? "3" : remaining > 1.0 ? "2" : "1";
        if (digit && countdownText?.textContent !== digit && !gameAudio.speechReady) soundManager.playCue(440, 80);
        countdownBanner.hidden = !digit;
        if (countdownText && digit) {
          countdownText.classList.remove("go");
          countdownText.textContent = digit;
        }
      } else if (phase === 2 && lastPhase === 1) {
        if (!gameAudio.speechReady) soundManager.playCue(880, 150);
        countdownBanner.hidden = false;
        if (countdownText) {
          countdownText.classList.add("go");
          countdownText.textContent = "СТАРТ!";
        }
        setTimeout(() => {
          if (countdownBanner) countdownBanner.hidden = true;
        }, 1200);
      } else if (phase !== 1 && (lastPhase !== 1 || phase > 2)) {
        countdownBanner.hidden = true;
      }
    }
    lastPhase = phase;
    gameAudio.frame(phase, phase === 1 ? viewer.get_countdown_remaining() : 0);

    // Wrong Way Alert
    if (wrongWayBanner) {
      wrongWayBanner.hidden = !(!activeCareerEvent?.is_factory && phase === 2 && viewer.is_wrong_way());
    }

    // Race Finished / Results Modal
    if (raceResultsModal) {
      if (isFinished && !eventFinishedHandled) {
        eventFinishedHandled = true;
        raceResultsModal.hidden = false;
        const pos = viewer.get_player_position();
        const total = viewer.get_total_participants();
        const lapTime = viewer.get_current_lap_time();
        const bestTime = viewer.get_best_lap_time();
        const elapsed = (typeof viewer.get_mission_elapsed_time === "function" && activeCareerEvent?.is_factory)
          ? viewer.get_mission_elapsed_time()
          : lapTime;

        if (resultsSubtitle) {
          resultsSubtitle.textContent = activeCareerEvent?.is_factory
            ? `Миссия: ${activeCareerEvent.title}`
            : `Позиция: P${pos} из ${total}`;
        }
        if (resultsLapTime) resultsLapTime.textContent = formatRaceTime(elapsed);
        if (resultsBestTime) resultsBestTime.textContent = formatRaceTime(bestTime);

        if (activeCareerEvent) {
          let passed = false;
          let factoryResult = null;
          if (activeCareerEvent.is_factory) {
            try {
              factoryResult = evaluateFactoryResult(viewer, activeCareerEvent, currentProfile,
                elapsed, pos, typeof shell_complete_factory_mission === "function"
                  ? shell_complete_factory_mission : null);
              passed = factoryResult?.passed === true;
              if (factoryResult) {
                currentProfile = factoryResult.profile;
                saveProfile();
                if (resultsLapTime) resultsLapTime.textContent = formatRaceTime(factoryResult.total_time);
                if (factoryResult.penalty > 0 && resultsSubtitle) {
                  resultsSubtitle.textContent += ` · Штраф: +${factoryResult.penalty.toFixed(1)} с`;
                }
              }
            } catch (e) {
              console.error("WASM factory mission completion error:", e);
              passed = false;
              setStatus("Не удалось сохранить результат испытания.");
            }
          } else {
            passed = pos === 1;
          }

          if (passed) {
            soundManager.playPass();
          } else {
            soundManager.playFail();
          }

          const speechEl = document.querySelector("#resultsSpeech");
          const rewardBadge = document.querySelector("#resultsRewardBadge");

          if (activeCareerEvent.is_factory) {
            if (speechEl) {
              speechEl.style.display = "block";
              speechEl.textContent = passed ? activeCareerEvent.pass_msg : activeCareerEvent.fail_msg;
            }
            if (rewardBadge) {
              rewardBadge.textContent = passed ? "✓ ТЕСТ ПРОЙДЕН" : "✗ ТЕСТ НЕ ПРОЙДЕН";
              rewardBadge.style.color = passed ? "#22c55e" : "#ef4444";
            }
          } else {
            if (speechEl) speechEl.style.display = "none";
            if (rewardBadge) {
              rewardBadge.textContent = passed ? "✓ 1-Е МЕСТО — НАГРАДА: +4 500 CR" : `ПОЗИЦИЯ P${pos} — ЗАЕЗД ЗАВЕРШЁН`;
              rewardBadge.style.color = passed ? "#22c55e" : "#d3bd83";
            }
          }

          if (activeCareerEvent.is_factory && passed && factoryResult) {
            const evalRes = factoryResult;
            if (evalRes.rank && evalRes.rank.length > 0) {
              setStatus(`Испытание пройдено! Присвоено новое звание: ${evalRes.rank}`);
              if (rewardBadge) {
                rewardBadge.textContent = `★ ПОВЫШЕНИЕ: ${evalRes.rank.toUpperCase()}`;
                rewardBadge.style.color = "#22c55e";
              }
            }
            if (evalRes.reward_car && evalRes.reward_car.length > 0) {
              setStatus(`ПОЛУЧЕН НАГРАДНОЙ АВТОМОБИЛЬ: ${evalRes.reward_car}! Добавлен в гараж.`);
              if (rewardBadge) {
                rewardBadge.textContent = `★ НАГРАДА: ${evalRes.reward_car.toUpperCase()} В ГАРАЖЕ!`;
                rewardBadge.style.color = "#d3bd83";
              }
            }
          } else if (!activeCareerEvent.is_factory && typeof shell_apply_event_result === "function" && currentProfile) {
            try {
              const resJson = shell_apply_event_result(
                JSON.stringify(currentProfile),
                activeCareerEvent.id,
                lapTime,
                pos
              );
              currentProfile = JSON.parse(resJson);
              saveProfile();
            } catch (e) {
              console.error("WASM event result apply error:", e);
            }
          } else if (!activeCareerEvent.is_factory && currentProfile) {
            if (passed) {
              currentProfile.credits += 4500;
            }
            saveProfile();
          }
        } else {
          if (pos === 1) soundManager.playPass();
          else soundManager.playFail();
        }
      }
    }

    if (isRacingActive || Math.abs(speed) > 0.1 || throttle !== 0 || steer !== 0 || handbrake) {
      queueFrame();
    }
    return;
  }

  engineSound.deactivate(); // not driving: silence the engine
  tyreSound.deactivate();
  let inForward = 0;
  let inRight = 0;
  let inZoom = 0;

  if (activeKeys.has("KeyW") || activeKeys.has("ArrowUp")) inForward += 1;
  if (activeKeys.has("KeyS") || activeKeys.has("ArrowDown")) inForward -= 1;
  if (activeKeys.has("KeyA") || activeKeys.has("ArrowLeft")) inRight -= 1;
  if (activeKeys.has("KeyD") || activeKeys.has("ArrowRight")) inRight += 1;
  if (activeKeys.has("KeyQ") || activeKeys.has("Minus") || activeKeys.has("NumpadSubtract")) inZoom += 1;
  if (activeKeys.has("KeyE") || activeKeys.has("Equal") || activeKeys.has("NumpadAdd")) inZoom -= 1;

  // Normalize diagonal movement
  const len = Math.hypot(inForward, inRight);
  if (len > 0.001) {
    inForward /= len;
    inRight /= len;
  }

  const speedMult = activeKeys.has("ShiftLeft") || activeKeys.has("ShiftRight") ? 2.5 : 1.0;
  inForward *= speedMult;
  inRight *= speedMult;
  inZoom *= speedMult;

  // Exponential smooth damping (decay = 12.0 gives responsive yet smooth inertia)
  const decay = 12.0;
  const factor = 1.0 - Math.exp(-decay * dt);

  velForward += (inForward - velForward) * factor;
  velRight += (inRight - velRight) * factor;
  velZoom += (inZoom - velZoom) * factor;

  // Snap to 0 when near rest and no input
  if (Math.abs(velForward) < 0.0005 && inForward === 0) velForward = 0;
  if (Math.abs(velRight) < 0.0005 && inRight === 0) velRight = 0;
  if (Math.abs(velZoom) < 0.0005 && inZoom === 0) velZoom = 0;

  if (velForward !== 0 || velRight !== 0) {
    viewer.move_ground(velForward * dt * 2.5, velRight * dt * 2.5);
    needsFrame = true;
  }
  if (velZoom !== 0) {
    viewer.zoom(velZoom * dt * 450.0);
    needsFrame = true;
  }
}

function resizeCanvas() {
  const rect = canvas.getBoundingClientRect();
  const scale = window.devicePixelRatio || 1;
  const width = Math.max(1, Math.round(rect.width * scale));
  const height = Math.max(1, Math.round(rect.height * scale));
  if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width;
    canvas.height = height;
    if (viewer) {
      viewer.resize(width, height);
    }
    queueFrame();
  }
  if (frontEnd) frontEnd.resize(width, height);
}

function animate(now = 0) {
  resizeCanvas();
  if (pauseMenu.isOpen) pauseDraw();
  applyPendingCamera();
  if (frontEnd?.visible) {
    frontEnd.render(now);
    // The room turns continuously (one turn per 30 s).
    if (viewer?.has_room?.()) needsFrame = true;
  }

  if (document.hasFocus() && !document.hidden && lastTime > 0 && now > 0) {
    const dt = Math.min((now - lastTime) / 1000, 0.1);
    updateNavigation(dt);
  }
  lastTime = now;
  updateDebugPanel();

  if (viewer && needsFrame) {
    try {
      viewer.render();
      needsFrame = false;
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error));
    }
  }
  requestAnimationFrame(animate);
}

async function boot() {
  loadStoredProfile();
  updateOptions();
  if (!navigator.gpu) {
    setStatus("WebGPU недоступен. Нужен браузер с поддержкой WebGPU.");
    loadButton.disabled = true;
    return;
  }
  try {
    await init();
    resizeCanvas();
    viewer = await create_viewer(canvas);
    setStatus("WebGPU готов.");
    animate();

    const hasGame = await checkLocalGame();
    const autostartParams = new URLSearchParams(location.search);
    if (hasGame && !useCustomFiles && autostartParams.get("race")) {
      await autostartRace(autostartParams);
    } else if (hasGame && !useCustomFiles && autostartParams.get("shell") !== "web") {
      await startFrontEnd();
    } else if (hasGame && !useCustomFiles) {
      await loadTarget();
    } else {
      setStatus("WebGPU готов. Выберите локальные файлы игры.");
    }
  } catch (error) {
    setStatus(error instanceof Error ? error.message : String(error));
    loadButton.disabled = true;
  } finally {
    document.getElementById("bootCover")?.remove();
  }
}

// ---- URL autostart and debug hook (tools/browser-shot.mjs) ----
// ?race=<track>&car=<id>&opponents=<n>&laps=<n>&lights=0|1&cam=<name>
// skips the front end and starts a Quick Race through the same calls as the Quick Race dialog.
let debugReady = false;
// `?cam=` camera still to select (after the race intro); `__nfs.ready` waits for it.
let debugPendingCamera = null;
let debugCameraTries = 0;
// One C press per frame: right after the intro the camera can still refuse the cycle (a
// pending re-init), so a press only counts once the name changes.
function applyPendingCamera() {
  if (!debugPendingCamera || !viewer || viewer.get_camera_name() === "AnimCam") return;
  if (viewer.get_camera_name() === debugPendingCamera || ++debugCameraTries > 600) {
    debugPendingCamera = null;
    return;
  }
  cycleCarCamera();
  queueFrame();
}
let debugError = null;
window.__nfs = {
  get raceStart() { return lastRaceStart; },
  get raceDrivers() { return lastRaceDrivers; },
  quickRace() { return frontEndSetup ? { cars: frontEndSetup.cars.length, tracks: frontEndSetup.tracks.map(t => t.internal_name) } : null; },
  get ready() { return debugReady && !debugPendingCamera; },
  get error() { return debugError; },
  get viewer() { return viewer; },
  get hud() { return raceHud; },
  get frontEnd() { return frontEnd; },
  state() {
    if (!viewer) return { ready: false, error: debugError };
    const pose = viewer.get_car_pose?.() ?? [];
    const drawnPose = viewer.get_drawn_pose?.() ?? [];
    const cameraEye = viewer.get_camera_eye?.() ?? [];
    return {
      ready: debugReady,
      speed: viewer.get_car_speed(),
      rpm: viewer.get_car_rpm(),
      gear: viewer.get_car_gear(),
      camera: viewer.get_camera_name(),
      model: viewer.is_original_vehicle?.() ? "original" : "prototype",
      phase: viewer.get_race_phase(),
      position: viewer.get_player_position(),
      lap: viewer.get_current_lap(),
      lapTime: viewer.get_current_lap_time(),
      resets: viewer.get_car_resets?.() ?? 0,
      audio: {
        music: gameAudio.director ? {
          state: gameAudio.director.state(), index: gameAudio.director.index(),
          file: gameAudio.director.current_file(), level: gameAudio.level,
          playing: gameAudio.stream.playing, volume: gameAudio.stream.volume(),
          gain: gameAudio.stream.gain?.gain.value ?? null,
        } : null,
        speech: gameAudio.speechReady,
        tyres: tyreSound.readout(),
        context: soundManager.ctx?.state ?? "none",
        log: gameAudio.log.slice(-40),
      },
      hud: { ...(raceHud?.last ?? {}), carIndex: raceHudCarIndex, lights: lightsOn, ready: Boolean(raceHud) },
      pose: Array.from(pose),
      drawnPose: Array.from(drawnPose),
      cameraEye: Array.from(cameraEye),
      status: statusLine.textContent,
      sparks: Array.from(viewer.get_spark_stats?.() ?? []),
      ai: JSON.parse(viewer.original_ai_state?.() ?? "null"),
    };
  },
};

async function autostartRace(params) {
  try {
    quickRaceCars = buildQuickRaceCars(await loadCarCatalogueRecords(), availableCars, localGameFiles);
    const wantCar = (params.get("car") || "").toLowerCase();
    const car = quickRaceCars.find(c => c.id.toLowerCase() === wantCar || c.model_name.toLowerCase() === wantCar) ?? quickRaceCars[0];
    if (!car) throw new Error("autostart: no cars");
    const track = params.get("race").toLowerCase();
    modeSelect.value = "track";
    updateOptions();
    targetSelect.value = track;
    clearNavigation();
    activeCareerEvent = null;
    // ?dir=0..3 (forward, backward, forward/mirrored, backward/mirrored), ?difficulty=0..2,
    // ?opponents=0..7, ?traffic=0..3 stand in for the location and quick race screens.
    const { raceStartFromUrl } = await import("./fe-quickrace.js");
    const start = raceStartFromUrl(params);
    if (!await loadTarget(null, start.reverse)) throw new Error(statusLine.textContent);
    // ?manual=1 / ?damage=0 stand in for the car setup screen.
    viewer.set_car_setup?.(params.get("manual") === "1", params.get("damage") !== "0");
    await loadRaceCar(car.model_name, car.sim_name, car.bank || "", car.gauges || null, car.style || 0);
    if (!viewer.is_sim_mode()) viewer.toggle_sim_mode();
    await setRaceOptions({ ...start, car, colour: params.has("colour") ? Number(params.get("colour")) : null });
    viewer.set_linear_track?.(!await trackIsCircuit(track));
    viewer.configure_quick_race(start.laps, start.opponents);
    lastRaceStart = start;
    if (!viewer.is_drive_mode()) viewer.toggle_camera_mode();
    setMenuVisible(false);
    syncTourButton();
    // Always the original vehicle model, collisions and AI; the old `model=` URL param is ignored.
    if (params.get("lights") === "1") lightsOn = Boolean(viewer.toggle_lights());
    const cam = params.get("cam");
    // The C key is ignored during the race intro (AnimCam, Camera_Cycle 0x465a60), and the
    // race only steps once the frame loop runs: animate() switches after the intro.
    debugPendingCamera = cam || null;
    debugCameraTries = 0;
    queueFrame();
    debugReady = true;
  } catch (error) {
    debugError = error instanceof Error ? error.message : String(error);
    setStatus(`Autostart failed: ${debugError}`);
  }
}

// Debug panel: the state of every debug key, top centre of the drive view (not part of the
// original HUD). Refreshed every frame and after each key press.
let debugPanelShown = null;
let debugPanelText = null;
function updateDebugPanel() {
  const panel = document.getElementById("debugPanel");
  if (!panel || !viewer) return;
  const show = Boolean(debugPanelVisible && viewer.is_drive_mode?.() && viewer.has_car?.() && !frontEnd?.visible);
  if (show !== debugPanelShown) {
    panel.hidden = !show;
    debugPanelShown = show;
  }
  if (!show) return;
  const onOff = (v) => (v ? "on" : "off");
  const side = viewer.get_side_view?.() ?? 0;
  const original = Boolean(viewer.is_original_vehicle?.());
  const items = [
    `L lights ${onOff(lightsOn)}`,
    `C ${viewer.get_camera_name?.() ?? "-"}`,
    `V free cam ${onOff(viewer.is_free_camera?.())}`,
    `Shift+V side ${side === 0 ? "off" : `${side > 0 ? "R" : "L"} ${Math.abs(Math.round(side))}\u00b0`}`,
    `B boxes ${onOff(viewer.is_collision_boxes?.())}`,
    `H HUD ${onOff(hudVisible)}`,
    `hit: ${viewer.object_hit_report?.() || "-"}`,
    "R reset", "Shift+R restart", "Esc menu", "` panel",
  ];
  const text = items.map((item) => item.replace(/ /g, "\u00a0")).join(" \u00b7 ");
  panel.classList.toggle("original", original);
  if (text !== debugPanelText) {
    panel.textContent = text;
    debugPanelText = text;
  }
}

// ---- original front end (web/frontend.js; docs/re/frontend.md) ----
let frontEnd = null;
let frontEndRoom = null;
let frontEndCar = null;
const feCanvas = document.querySelector("#feCanvas");

function syncFrontEndClasses() {
  document.body.classList.toggle("fe-active", Boolean(frontEnd?.visible));
}

// Game file by path relative to the game folder, matched case-insensitively.
async function readGamePath(path) {
  const want = path.replace(/\\/g, "/").toLowerCase();
  const rel = localGameFiles.find(p => p.replace(/\\/g, "/").toLowerCase() === want);
  if (!rel) return null;
  const resp = await fetch("/game/" + rel.replace(/\\/g, "/"));
  return resp.ok ? new Uint8Array(await resp.arrayBuffer()) : null;
}

async function frontEndQuickRaceCar() {
  return frontEndSetup?.currentCar() ?? null;
}

// Original Quick Race lists (web/fe-quickrace.js): the 15 nfs5.car records of class 0/2 in
// the original order [VA 0x4f3e40, 0x4f0f50] that are installed, and the quick-race tracks
// of nfs5.trk [VA 0x4fbc10] that are installed.
let frontEndSetup = null;
// Debug: every installed car and track in Quick Race, like the original CHEATALLCARS callback
// (0x50dd70 -> flag 0x691a04: all car classes) and GameType 3 (all tracks). `?cheat=allcars`
// turns it on and is remembered; `?cheat=off` turns it off.
function cheatAllCars() {
  const v = new URLSearchParams(location.search).get("cheat");
  try {
    if (v === "allcars") localStorage.setItem("nfs5.cheatAllCars", "1");
    if (v === "off") localStorage.removeItem("nfs5.cheatAllCars");
    return localStorage.getItem("nfs5.cheatAllCars") === "1";
  } catch {
    return v === "allcars";
  }
}
let raceStart = null;
let eventInfoSetup = null;
async function buildFrontEndSetup(strings) {
  const all = cheatAllCars();
  const { QuickRaceSetup, quickRaceTracks, raceStart: rs } = await import("./fe-quickrace.js");
  raceStart = rs;
  const records = await loadCarCatalogueRecords();
  quickRaceCars = buildQuickRaceCars(records, availableCars, localGameFiles);
  const carBytes = await readGamePath("FEData/Data/nfs5.car");
  const order = JSON.parse(fe_quick_race_cars_json(carBytes, all));
  const byIndex = new Map(records.map(r => [r.index, r]));
  const ids = order; // sorted by CarList_Sort (era, group, year), also for the cheat list
  const cars = ids
    .map(i => {
      const installed = quickRaceCars.find(c => c.index === i);
      const r = byIndex.get(i);
      return installed && r ? { ...installed, short_name: r.short_name, colours: r.colours, ratings: r.ratings, group: r.group, year: r.year,
        logo_pack: r.logo_pack, logo_shape: r.logo_shape } : null;
    })
    .filter(Boolean);
  const trackBytes = await readGamePath("FEData/Data/nfs5.trk");
  const allTracks = trackBytes ? JSON.parse(fe_track_catalog_json(trackBytes)) : [];
  const tracks = quickRaceTracks(allTracks, availableTracks, all);
  frontEndSetup = new QuickRaceSetup({ strings, cars, tracks });
  // Evolution event info (tournament.lay): nfs5.trn records over the whole track table.
  const trnBytes = await readGamePath("FEData/Data/nfs5.trn");
  const { EventInfoSetup } = await import("./fe-tournament.js");
  eventInfoSetup = trnBytes
    ? new EventInfoSetup({ events: JSON.parse(fe_event_infos_json(trnBytes)), tracks: allTracks, strings })
    : null;
}

async function frontEndPlayGame() {
  const p = frontEndSetup?.raceParams();
  if (!p?.car || !p.track) {
    setStatus("Quick Race: no installed car or track");
    return;
  }
  // Direction, mirror, difficulty and 0..7 opponents are raced; traffic and the opponent car
  // mode are chosen but not raced yet (A100).
  const start = raceStart(p);
  await launchQuickRace(start);
}

// Click on the garage car [VA 0x4c0eb0]: the zone (roof, doors, hood, trunk) follows the yaw of
// the room; the top, doors, hood and trunk lid move (A232). A second click on an open door sits
// the player in the car in the original (zoom, `0x4c7d70`), which is not ported: it closes the door.
// Our yaw is half a turn off the original's value (the car shows its rear at 0.5 here, the zone
// function expects the front at 0.5), so the zone follows what is on screen (A232).
function frontEndCarClick(x, y, screen) {
  const control = screen?.controls.find(c => c.type === "3DCAR" && Number(c.attrs.positiontype || 0) === 0);
  if (!control || !viewer?.has_room?.() || !viewer.room_car_has_part) return;
  const [px, py] = (control.attrs.pos || "0,0").split(",").map(Number);
  const [w, h] = (control.attrs.size || "0,0").split(",").map(Number);
  if (x < px || y < py || x >= px + w || y >= py + h) return;
  const hasTop = viewer.room_car_has_part(1);
  const zone = carClickZone(x, y, { left: px, top: py, right: px + w, bottom: py + h },
    (viewer.room_yaw_turns() + 0.5) % 1, hasTop);
  const channel = ZONE_CHANNEL[zone];
  if (!channel || !viewer.room_car_has_part(channel)) return;
  const open = viewer.room_car_anim_target(channel) > 0.5 ? 0 : 1;
  viewer.set_room_car_anim(channel, open, carPartMs(channel, open === 1));
  if (channel === 1 && frontEndSetup) frontEndSetup.roof = open;
  queueFrame();
}

// Top of the garage car from the Car.Roof setting (0 up, 1 down).
function applyRoofSetting(ms) {
  if (frontEndSetup && viewer?.room_car_has_part?.(1)) {
    viewer.set_room_car_anim(1, frontEndSetup.roof ? 1 : 0, ms);
  }
}

// Car compare (`carcompare.lay`): two 3DCAR controls, position types 1 and 2: the Quick Race
// car on the left, GenericCar2's car on the right, each in the first colour of its list for
// the right one and the chosen colour for the left one (A230).
async function loadFrontEndCompare() {
  if (!frontEndSetup || !viewer.has_room?.()) return;
  const cars3d = frontEnd?.screen?.controls.filter(c => c.type === "3DCAR") ?? [];
  if (cars3d.length < 2) return;
  const { playerPaint } = await import("./fe-quickrace.js");
  const load = async car => {
    const files = await fetchGameFiles(findCarFilesFromCatalog(car.model_name.toLowerCase()));
    viewer.set_car_model(files.names, files.bytes, car.model_name.toLowerCase(), car.style || 0);
  };
  const left = frontEndSetup.currentCar();
  const right = frontEndSetup.compareCar();
  if (!left || !right) return;
  await load(right);
  viewer.stash_compare_car(new Float32Array(playerPaint(right, 0) || []));
  await load(left);
  frontEndCar = `${left.model_name}:${left.style || 0}`;
  viewer.set_room_paint(new Float32Array(playerPaint(left, frontEndSetup.colour) || []));
  viewer.show_compare_cars(true);
  // Cars above the 2D backdrop (transparent canvas on top of the front-end canvas, A230).
  document.body.classList.add("fe-compare");
  queueFrame();
}

// room= of the shown layout; a 3DCAR control adds the selected car [VA 0x4bfdc0].
async function loadFrontEndRoom(room, carAccessor) {
  const id = (room || "").toUpperCase();
  const model = { ID_GARAGE: "garage", ID_FACTORY: "factory" }[id];
  try {
    // Rooms without a drawn model (ID_NONE, ID_TESTROOM, ID_TRACK, ...) show an empty scene.
    viewer.hide_room?.(!model);
    viewer.show_compare_cars?.(false);
    document.body.classList.remove("fe-compare");
    if (!model) {
      await loadFrontEndCompare();
      return;
    }
    if (frontEndRoom !== id || !viewer.has_room()) {
      const names = [`FEData/Models/${model}.crp`, `FEData/Models/${model}.tpg`];
      const bytes = await Promise.all(names.map(readGamePath));
      if (bytes.some(b => !b)) throw new Error(`missing ${model}.crp/.tpg`);
      viewer.load_room(names, bytes, id);
      frontEndRoom = id;
      frontEndCar = null;
    }
    if (carAccessor) {
      const car = await frontEndQuickRaceCar();
      // Model and TPG style identify the drawn body: records share a model (964 Carrera 2/4,
      // cabriolet/targa) and differ in `style`.
      const carKey = car ? `${car.model_name}:${car.style || 0}` : null;
      if (car && frontEndCar !== carKey) {
        const files = await fetchGameFiles(findCarFilesFromCatalog(car.model_name.toLowerCase()));
        viewer.set_room_car(files.names, files.bytes, car.model_name.toLowerCase(), car.style || 0);
        frontEndCar = carKey;
        applyRoofSetting(0);
      }
      // The colour button's list entry paints the body, as in the race (A127).
      const { playerPaint } = await import("./fe-quickrace.js");
      viewer.set_room_paint?.(new Float32Array((car && playerPaint(car, frontEndSetup?.colour ?? 0)) || []));
      viewer.show_room_car(Boolean(car));
    } else {
      viewer.show_room_car(false);
    }
  } catch (error) {
    setStatus(`Front end room: ${error instanceof Error ? error.message : String(error)}`);
  }
  queueFrame();
}

function frontEndAction(name) {
  switch (name) {
    case "SETTYPECAREERMODE": openEvolutionTournaments(); return true;
    case "SETTYPEFACTORYDRIVER": openFactoryLadder(); return true;
    // SETTYPEQUICKRACE resets the race settings and the car index [VA 0x510c30, 0x4fc730];
    // returning false lets the button go on to its destination.
    case "SETTYPEQUICKRACE": frontEndSetup?.reset(); return false;
    case "TRACK1INC": frontEndSetup?.stepTrack(1); queueFrame(); return true;
    case "TRACK1DEC": frontEndSetup?.stepTrack(-1); queueFrame(); return true;
    case "PLAYGAME": frontEndPlayGame(); return true;
    case "OPTIONS":
    case "QUIT":
      // Web shell (developer tools) until the options screens are restored.
      frontEnd.hide();
      syncFrontEndClasses();
      return true;
    default: return false;
  }
}

async function showFrontEnd() {
  if (!frontEnd) return;
  frontEndRoom = null;
  if (!frontEnd.levels.length) await frontEnd.start("gamesetup");
  else await frontEnd.refresh();
  syncFrontEndClasses();
}

async function startFrontEnd() {
  try {
    const { FrontEnd } = await import("./frontend.js");
    frontEnd = new FrontEnd(feCanvas, {
      topCanvas: document.querySelector("#feTopCanvas"),
      api: { fe_parse_layout, fe_decode_fsh, fe_parse_font, fe_parse_strings },
      readFile: readGamePath,
      onRoom: loadFrontEndRoom,
      onAction: frontEndAction,
      accessor: name => frontEndSetup?.accessor(name) ?? eventInfoSetup?.accessor(name) ?? null,
      // Track pictures of TrackArt: the location screen's wall tile and the event panel's map.
      trackPicture: screen => {
        if (screen === "location") {
          const t = frontEndSetup?.currentTrack();
          return t ? { sp: t.photo_pack, sm: t.small_pack, direction: frontEndSetup.direction } : null;
        }
        if (screen === "tournament" && eventInfoSetup?.current()) {
          return { key: eventInfoSetup.index, races: eventInfoSetup.raceViews() };
        }
        return null;
      },
      // FECallBackList update [VA 0x50cc30]: ID_MERGEDLISTCOLOR when 0x4f4df0 is true,
      // ID_MERGEDLISTCARSETUP otherwise. Hypothesis: 0x4f4df0 = "the car is not owned";
      // Quick Race offers only unowned (catalogue) cars here, so colour shows, car setup not.
      controlVisible: name => name !== "ID_MERGEDLISTCARSETUP",
      carLogo: (accessor, other) => {
        // Car compare: the right logo follows GenericCar2, the left one is `CarCompare1`.
        const car = /^GenericCar2\./i.test(accessor || "") ? frontEndSetup?.compareCar() : frontEndSetup?.currentCar();
        return car?.logo_pack ? { pack: car.logo_pack, shape: car.logo_shape } : null;
      },
      onChange: name => {
        // A new car (or colour) is shown in the garage at once.
        if (/^(zMergedList|GenericCar2?)\./.test(name || "") && frontEnd.screen?.controls.some(c => c.type === "3DCAR")) {
          loadFrontEndRoom(frontEnd.screen.screenAttrs.room, "zMergedList.Car");
        }
        // Car.Roof (settings pop-up) moves the top of a cabrio in the garage [VA 0x4c0990].
        if (name === "Car.Roof") applyRoofSetting(carPartMs(1, true));
        queueFrame();
      },
      career: () => false,
      onCarClick: frontEndCarClick,
      requestFrame: queueFrame,
    });
    await frontEnd.init();
    await buildFrontEndSetup(frontEnd.assets.strings);
    resizeCanvas();
    frontEnd.resize(canvas.width, canvas.height);
    // Start-up loading screen (loadscrn.fsh) while the menu and the garage load [VA 0x4dd600].
    await frontEnd.showLoading({ pack: "loadscrn.fsh" });
    syncFrontEndClasses();
    // Visual check: ?feloading=<pack>[,<track>] keeps a loading screen up.
    const preview = new URLSearchParams(location.search).get("feloading");
    if (preview) {
      const [pack, track] = preview.split(",");
      await frontEnd.showLoading({ pack, track: track || null });
      return;
    }
    await frontEnd.start("gamesetup");
    // Visual check: ?fescreen=location|tournament opens that layout over the main menu
    // (the Evolution flow does not lead to tournament.lay yet).
    const direct = new URLSearchParams(location.search).get("fescreen");
    if (direct) await frontEnd.goto(direct);
    menuVisible = true;
    syncFrontEndClasses();
    setStatus("Original front end (?shell=web: web shell)");
  } catch (error) {
    console.error("front end:", error);
    frontEnd = null;
    syncFrontEndClasses();
    setStatus(`Original front end failed: ${error instanceof Error ? error.message : String(error)}`);
    await loadTarget();
  }
}

boot();
