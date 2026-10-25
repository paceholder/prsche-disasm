import init, { create_viewer, EngineAudio, engine_bank_for_sim, shell_profile_new, shell_profile_buy_356, shell_apply_event_result, shell_get_first_evolution_event, shell_get_first_factory_event, shell_get_dealership_catalog, parse_car_catalogue_json, shell_get_used_cars, shell_get_parts_catalog, shell_get_tournament_cups, shell_buy_car, shell_sell_car, shell_buy_part, shell_repair_car, shell_get_factory_missions, shell_complete_factory_mission, fe_parse_layout, fe_decode_fsh, fe_parse_font, fe_parse_strings } from "./package/viewer_impl.js";

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
  { id: "canyon", name: "Canyon (Каньон)" },
  { id: "castle", name: "Schwarzwald (Замок)" },
  { id: "coastal", name: "Côte d'Azur (Лазурный берег)" },
  { id: "farmland", name: "Normandie (Нормандия)" },
  { id: "foothills", name: "Pyrenees (Пиренеи)" },
  { id: "forest", name: "Black Forest (Шварцвальд)" },
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
const cameraBadge = document.querySelector("#cameraBadge");

// HUD camera name (original camera names in 6 DOF driving: TailCam0, HeliCam2, ...).
function syncCameraBadge() {
  if (!viewer || !cameraBadge || typeof viewer.get_camera_name !== "function") return;
  const text = `C: ${viewer.get_camera_name()}`;
  if (cameraBadge.textContent !== text) cameraBadge.textContent = text;
}

function cycleCarCamera() {
  const name = viewer.cycle_car_view();
  setStatus(`Камера: ${name}`);
  syncCameraBadge();
}
const paintButton = document.querySelector("#paintButton");
const hud = document.querySelector("#hud");
const speedValue = document.querySelector("#speedValue");
const gearValue = document.querySelector("#gearValue");
const rpmBar = document.querySelector("#rpmBar");
const loadButton = document.querySelector("#loadButton");
const resetButton = document.querySelector("#resetButton");
const statusLine = document.querySelector("#status");
const summary = document.querySelector("#summary");
const menuButton = document.querySelector("#menuButton");
const hudButton = document.querySelector("#hudButton");
const detailsButton = document.querySelector("#detailsButton");
const raceHudTop = document.querySelector("#raceHudTop");
const racePos = document.querySelector("#racePos");
const raceTotalPos = document.querySelector("#raceTotalPos");
const raceLap = document.querySelector("#raceLap");
const raceTotalLaps = document.querySelector("#raceTotalLaps");
const raceLapTime = document.querySelector("#raceLapTime");
const raceBestLap = document.querySelector("#raceBestLap");
const racePosBadge = document.querySelector("#racePosBadge");
const raceLapBadge = document.querySelector("#raceLapBadge");
const raceTimeLabel = document.querySelector("#raceTimeLabel");
const facStatusBadge = document.querySelector("#facStatusBadge");
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
    this.files = { names, bytes, simName, bank };
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

class SoundManager {
  constructor() {
    this.ctx = null;
    this.enabled = true; // on by default; the AudioContext starts on the first user gesture
    this.skidNoise = null;
    this.skidFilter = null;
    this.skidGain = null;
  }

  toggle() {
    this.enabled = !this.enabled;
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
    if (this.ctx && !this.skidNoise) {
      try {
        const bufferSize = this.ctx.sampleRate * 2;
        const noiseBuffer = this.ctx.createBuffer(1, bufferSize, this.ctx.sampleRate);
        const output = noiseBuffer.getChannelData(0);
        for (let i = 0; i < bufferSize; i++) {
          output[i] = Math.random() * 2 - 1;
        }
        this.skidNoise = this.ctx.createBufferSource();
        this.skidNoise.buffer = noiseBuffer;
        this.skidNoise.loop = true;

        this.skidFilter = this.ctx.createBiquadFilter();
        this.skidFilter.type = "bandpass";
        this.skidFilter.frequency.setValueAtTime(900, this.ctx.currentTime);
        this.skidFilter.Q.setValueAtTime(1.5, this.ctx.currentTime);

        this.skidGain = this.ctx.createGain();
        this.skidGain.gain.setValueAtTime(0.0, this.ctx.currentTime);

        this.skidNoise.connect(this.skidFilter);
        this.skidFilter.connect(this.skidGain);
        this.skidGain.connect(this.ctx.destination);
        this.skidNoise.start();
      } catch {}
    }
  }

  stop() {
    engineSound.deactivate();
    if (this.skidGain && this.ctx) {
      try { this.skidGain.gain.setValueAtTime(0.0, this.ctx.currentTime); } catch {}
    }
  }

  update(rpm, speed, handbrake) {
    if (!this.enabled || !this.ctx || !this.skidNoise) return;
    try {
      if (this.skidGain) {
        const isSkidding = handbrake || (Math.abs(speed) > 20 && rpm > 0.85);
        this.skidGain.gain.setTargetAtTime(isSkidding ? 0.08 : 0.0, this.ctx.currentTime, 0.05);
      }
    } catch {}
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

function setMenuVisible(visible) {
  menuVisible = visible;
  // The original front end replaces the web shell's menu when it is active.
  if (frontEnd) {
    if (visible) showFrontEnd();
    else frontEnd.hide();
    syncFrontEndClasses();
  }
  document.body.classList.toggle("menu-hidden", !visible);
  menuButton.setAttribute("aria-expanded", String(visible));
  clearNavigation();
  if (visible) menuButton.focus();
  else canvas.focus();
  queueFrame();
}

function toggleHud() {
  hudVisible = !hudVisible;
  hudButton.setAttribute("aria-pressed", String(hudVisible));
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

function findTrackFilesFromCatalog(target, specificScn = null) {
  const matching = [];
  const targetLower = target.toLowerCase();
  const scnTarget = specificScn ? specificScn.toLowerCase() : null;

  for (const relPath of localGameFiles) {
    const fullPath = relPath.replace(/\\/g, "/").toLowerCase();
    const basename = fullPath.split("/").pop();
    const stem = basename.replace(/\.[^.]+$/, "");
    const ext = basename.split(".").pop();

    if (fullPath.includes("/sky/") && (ext === "fsh" || ext === "bin") && stem === targetLower) {
      matching.push(relPath);
      continue;
    }
    if (fullPath.includes("/sky/") || fullPath.includes("/sky new/")) continue;
    if (fullPath.includes("/fedata/") || fullPath.includes("/trackart/") || fullPath.includes("/cardata/")) continue;

    if (ext === "scn") {
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
    if (fullPath.endsWith("/render/common.fsh")) {
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
  const matches = resources.filter(value => nameOf(value).split(/[\\/]/).pop().toLowerCase() === basename);
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
}

// `bank` is the catalogue record's engine bank; without it the first record with this SIM is used.
async function loadRaceCar(model, simName, bank = "") {
  const target = model.toLowerCase();
  let files;
  if (useCustomFiles && selectedFiles.length > 0) {
    const selected = selectedFiles.filter(file => {
      const basename = selectedName(file).split(/[\\/]/).pop().toLowerCase();
      return [`${target}.crp`, `${target}.tpg`, `${target}.clr`].includes(basename) || basename.endsWith('.fsh');
    });
    files = { names: selected.map(selectedName), bytes: await Promise.all(selected.map(async file => new Uint8Array(await file.arrayBuffer()))) };
  } else {
    files = await fetchGameFiles(findCarFilesFromCatalog(target));
  }
  if (!files.names.some(name => name.split(/[\\/]/).pop().toLowerCase() === `${target}.crp`)) {
    throw new Error(`Не найдена модель ${target}.crp.`);
  }
  viewer.set_car_model(files.names, files.bytes, target);
  await loadCarSim(simName);
  lastEngineSoundError = await loadEngineSound(simName, bank);
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
      await frontEnd.showLoading({ pack: "era0tr.fsh", track });
      syncFrontEndClasses();
    }
    modeSelect.value = 'track';
    updateOptions();
    targetSelect.value = track;
    if (!await loadTarget()) throw new Error(statusLine.textContent);
    await loadRaceCar(car.model_name, car.sim_name, car.bank || "");
    if (!viewer.is_sim_mode()) viewer.toggle_sim_mode();
    viewer.configure_quick_race(laps, opponents);
    if (!viewer.is_drive_mode()) viewer.toggle_camera_mode();
    quickRaceModal.hidden = true;
    setMenuVisible(false);
    syncTourButton();
    setStatus(`Quick Race: ${car.display_name} · ${track}${lastEngineSoundError ? ` · ${lastEngineSoundError}` : ""}`);
  } catch (error) {
    quickRaceStatus.textContent = error instanceof Error ? error.message : String(error);
    if (frontEnd) {
      await showFrontEnd();
      quickRaceModal.hidden = false;
    }
  } finally {
    quickRaceStartBtn.disabled = false;
    quickRaceCancelBtn.disabled = false;
  }
});

async function loadTarget(specificScn = null) {
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

          if (fullPath.includes("/sky/") && (ext === "fsh" || ext === "bin") && stem === target) {
            names.push(selectedName(file));
            bytes.push(new Uint8Array(await file.arrayBuffer()));
            continue;
          }
          if (fullPath.includes("/sky/") || fullPath.includes("/sky new/")) continue;
          if (fullPath.includes("/fedata/") || fullPath.includes("/trackart/") || fullPath.includes("/cardata/")) continue;

          if (ext === "scn") {
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
            ![`${target}.crp`, `${target}.tpg`, `${target}.clr`].includes(basename) &&
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
        ? findTrackFilesFromCatalog(target, specificScn)
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
      const text = viewer.load_track(names, bytes, target);
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
              if (![`${carModel}.crp`, `${carModel}.tpg`, `${carModel}.clr`].includes(basename) && !basename.endsWith(".fsh")) {
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
  if (hud) {
    hud.hidden = !(isDriveAndCar && hudVisible);
  }
  const isRacingActive = isDriveAndCar && viewer && viewer.get_race_phase && viewer.get_race_phase() > 0;
  if (raceHudTop) {
    raceHudTop.hidden = !(isRacingActive && hudVisible);
  }
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

window.addEventListener("keydown", (event) => {
  if (event.code === "Escape" && !event.repeat) {
    event.preventDefault();
    if (!quickRaceModal.hidden) {
      if (!quickRaceCancelBtn.disabled) quickRaceModal.hidden = true;
      return;
    }
    setMenuVisible(!menuVisible);
    return;
  }
  if (!quickRaceModal.hidden) return;
  if (isTyping(event)) return;
  if (event.ctrlKey || event.altKey || event.metaKey) return;
  if (event.code === "KeyH" && !event.repeat) {
    toggleHud();
    return;
  }

  // Single-shot keys (ignore auto-repeat)
  if (event.code === "KeyR" && !event.repeat) {
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
  if (event.code === "KeyM" && !event.repeat) {
    if (viewer) {
      const isSim = viewer.toggle_sim_mode();
      const badge = document.getElementById("physicsModeBadge");
      if (badge) {
        badge.textContent = isSim ? "M: 6 DOF Физика" : "M: Аркада";
      }
      setStatus(
        isSim
          ? "Физика: 6 DOF Реалистичная симуляция (.sim, 4 независимых колеса, подвеска)"
          : "Физика: Прототип Аркадной модели"
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
  if (event.code === "KeyJ" && !event.repeat) {
    if (viewer) {
      const limit = viewer.cycle_ai_speed_limit();
      const text = limit < 0 ? "J: AI full speed" : limit === 0 ? "J: AI parked" : `J: AI ${Math.round(limit)} km/h`;
      const badge = document.getElementById("aiSpeedBadge");
      if (badge) badge.textContent = text;
      setStatus(text);
    }
    return;
  }
  if (event.code === "KeyK" && !event.repeat) {
    if (viewer) {
      const original = viewer.toggle_collision_model();
      const badge = document.getElementById("collisionModelBadge");
      if (badge) badge.textContent = original ? "K: original collisions" : "K: prototype collisions";
      setStatus(
        original
          ? "Collisions: original NFS5 response (K)"
          : "Collisions: prototype response (K)"
      );
      queueFrame();
    }
    return;
  }
  if (event.code === "KeyO" && !event.repeat) {
    if (viewer) {
      const original = viewer.toggle_vehicle_model();
      const badge = document.getElementById("vehicleModelBadge");
      if (badge) badge.textContent = original ? "O: original car" : "O: prototype car";
      setStatus(
        original
          ? "Vehicle model: original NFS5 (O)"
          : viewer.has_original_vehicle()
            ? "Vehicle model: prototype 6 DOF (O)"
            : "Vehicle model: original needs the car's .sim (O)"
      );
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
      setStatus(on ? "Фары включены (L)" : "Фары выключены (L)");
      queueFrame();
    }
    return;
  }
  if (event.code === "KeyV" && !event.repeat) {
    // Debug free camera, outside the original C-key cycle.
    if (viewer && viewer.is_drive_mode() && viewer.has_car()) {
      const free = viewer.toggle_free_camera();
      setStatus(free ? "Свободная камера (V)" : `Камера: ${viewer.get_camera_name()}`);
      syncCameraBadge();
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
});

window.addEventListener("keyup", (event) => {
  activeKeys.delete(event.code);
});

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
  if (isDriveCar) {
    let throttle = 0;
    if (activeKeys.has("KeyW") || activeKeys.has("ArrowUp")) throttle += 1.0;
    if (activeKeys.has("KeyS") || activeKeys.has("ArrowDown")) throttle -= 1.0;

    let steer = 0;
    if (activeKeys.has("KeyA") || activeKeys.has("ArrowLeft")) steer -= 1.0;
    if (activeKeys.has("KeyD") || activeKeys.has("ArrowRight")) steer += 1.0;

    const handbrake = activeKeys.has("Space");

    viewer.update_car(dt, throttle, steer, handbrake);
    syncCameraBadge();

    // Update HUD display
    const speed = Math.round(viewer.get_car_speed());
    const gear = viewer.get_car_gear();
    const rpm = Math.max(0, Math.min(1, viewer.get_car_rpm()));

    soundManager.update(rpm, speed, handbrake);
    engineSound.update(dt, viewer.get_car_engine_rpm(), viewer.get_car_redline_rpm(),
      Math.max(0, throttle), viewer.get_car_view(), gear);

    if (speedValue) speedValue.textContent = String(speed);
    if (gearValue) {
      gearValue.textContent = gear < 0 ? "R" : gear === 0 ? "N" : String(gear);
      gearValue.classList.toggle("reverse", gear < 0);
    }
    if (rpmBar) {
      rpmBar.style.width = `${Math.round(rpm * 100)}%`;
      if (rpm > 0.82) {
        rpmBar.style.background = "#ef4444";
      } else if (rpm > 0.65) {
        rpmBar.style.background = "#f59e0b";
      } else {
        rpmBar.style.background = "#d3bd83";
      }
    }
    const gaugeNeedle = document.querySelector("#gaugeNeedle");
    if (gaugeNeedle) {
      const angle = -125 + rpm * 250;
      gaugeNeedle.style.transform = `translate(-50%, -90%) rotate(${angle}deg)`;
    }

    // Update Race HUD
    const phase = viewer.get_race_phase ? viewer.get_race_phase() : 0;
    const isRacingActive = phase > 0;
    const isFinished = phase === 4 || phase === 5;
    // Dismissal does not end the simulation's terminal phase. Re-arm only when
    // a new attempt actually begins, including keyboard and toolbar restarts.
    if (!isFinished) eventFinishedHandled = false;

    if (raceHudTop) {
      raceHudTop.hidden = !(hudVisible && isRacingActive);
      if (isRacingActive) {
        const isFactory = Boolean(activeCareerEvent?.is_factory);
        const isDuel = activeCareerEvent?.mission_obj?.type === "duel" || (activeCareerEvent?.mission_obj?.title || "").toLowerCase().includes("duel");

        if (isFactory) {
          if (racePosBadge) racePosBadge.style.display = isDuel ? "flex" : "none";
          if (raceLapBadge) raceLapBadge.style.display = isDuel ? "flex" : "none";
          if (facStatusBadge) facStatusBadge.style.display = "flex";

          if (raceTimeLabel) raceTimeLabel.textContent = "ВРЕМЯ ЗАДАНИЯ";
          const elapsed = (typeof viewer.get_mission_elapsed_time === "function")
            ? viewer.get_mission_elapsed_time()
            : viewer.get_current_lap_time();
          if (raceLapTime) raceLapTime.textContent = formatRaceTime(elapsed);

          const timeLimit = activeCareerEvent.time_limit || 0;
          if (raceBestLap) {
            if (timeLimit > 0) {
              const remaining = Math.max(0, timeLimit - elapsed);
              raceBestLap.textContent = `ЛИМИТ: ${timeLimit.toFixed(1)}с (ОСТ: ${remaining.toFixed(1)}с)`;
            } else {
              raceBestLap.textContent = "ЦЕЛЬ: ПОБЕДА";
            }
          }

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
        } else {
          if (racePosBadge) racePosBadge.style.display = "flex";
          if (raceLapBadge) raceLapBadge.style.display = "flex";
          if (facStatusBadge) facStatusBadge.style.display = "none";
          if (raceTimeLabel) raceTimeLabel.textContent = "ВРЕМЯ КРУГА";

          if (racePos) racePos.textContent = String(viewer.get_player_position());
          if (raceTotalPos) raceTotalPos.textContent = `/${viewer.get_total_participants()}`;
          if (raceLap) raceLap.textContent = String(viewer.get_current_lap());
          if (raceTotalLaps) raceTotalLaps.textContent = `/${viewer.get_total_laps()}`;
          if (raceLapTime) raceLapTime.textContent = formatRaceTime(viewer.get_current_lap_time());
          if (raceBestLap) {
            const best = viewer.get_best_lap_time();
            raceBestLap.textContent = `ЛУЧШИЙ ${formatRaceTime(best)}`;
          }
        }
      }
    }

    // Countdown Banner
    if (countdownBanner) {
      if (phase === 1) {
        if (lastPhase !== 1) soundManager.playCue(440, 80);
        countdownBanner.hidden = false;
        const remaining = viewer.get_countdown_remaining();
        if (countdownText) {
          countdownText.classList.remove("go");
          if (remaining > 2.0) countdownText.textContent = "3";
          else if (remaining > 1.0) countdownText.textContent = "2";
          else countdownText.textContent = "1";
        }
      } else if (phase === 2 && lastPhase === 1) {
        soundManager.playCue(880, 150);
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
    if (hasGame && !useCustomFiles && new URLSearchParams(location.search).get("shell") !== "web") {
      await startFrontEnd();
    } else if (hasGame && !useCustomFiles) {
      await loadTarget();
    } else {
      setStatus("WebGPU готов. Выберите локальные файлы игры.");
    }
  } catch (error) {
    setStatus(error instanceof Error ? error.message : String(error));
    loadButton.disabled = true;
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
  if (!quickRaceCars.length) {
    quickRaceCars = buildQuickRaceCars(await loadCarCatalogueRecords(), availableCars, localGameFiles);
  }
  return quickRaceCars.find(car => car.id === quickRaceCar.value) ?? quickRaceCars[0] ?? null;
}

// room= of the shown layout; a 3DCAR control adds the selected car [VA 0x4bfdc0].
async function loadFrontEndRoom(room, carAccessor) {
  const id = (room || "").toUpperCase();
  const model = { ID_GARAGE: "garage", ID_FACTORY: "factory" }[id];
  try {
    // Rooms without a drawn model (ID_NONE, ID_TESTROOM, ID_TRACK, ...) show an empty scene.
    viewer.hide_room?.(!model);
    if (!model) return;
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
      if (car && frontEndCar !== car.model_name) {
        const files = await fetchGameFiles(findCarFilesFromCatalog(car.model_name.toLowerCase()));
        viewer.set_room_car(files.names, files.bytes, car.model_name.toLowerCase());
        frontEndCar = car.model_name;
      }
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
    case "PLAYGAME":
    case "COMBOBOX": openQuickRace(); return true;
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
  if (!frontEnd.stack.length) await frontEnd.start("gamesetup");
  else await frontEnd.show(frontEnd.stack[frontEnd.stack.length - 1]);
  syncFrontEndClasses();
}

async function startFrontEnd() {
  try {
    const { FrontEnd } = await import("./frontend.js");
    frontEnd = new FrontEnd(feCanvas, {
      api: { fe_parse_layout, fe_decode_fsh, fe_parse_font, fe_parse_strings },
      readFile: readGamePath,
      onRoom: loadFrontEndRoom,
      onAction: frontEndAction,
      accessor: name => {
        if ((name || "").startsWith("zMergedList.Car")) {
          const car = quickRaceCars.find(c => c.id === quickRaceCar.value) ?? quickRaceCars[0];
          return car?.display_name ?? "";
        }
        if ((name || "").startsWith("Track1.Track")) return quickRaceTrack.value || "";
        if (name === "Game.NumOpponents") return document.querySelector("#quickRaceOpponents")?.value ?? "";
        return "";
      },
      career: () => false,
      requestFrame: queueFrame,
    });
    await frontEnd.init();
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
    menuVisible = true;
    syncFrontEndClasses();
    setStatus("Original front end (?shell=web: web shell)");
  } catch (error) {
    frontEnd = null;
    syncFrontEndClasses();
    setStatus(`Original front end failed: ${error instanceof Error ? error.message : String(error)}`);
    await loadTarget();
  }
}

quickRaceCar.addEventListener("change", () => {
  if (frontEnd?.visible && frontEnd.screen?.controls.some(c => c.type === "3DCAR")) {
    loadFrontEndRoom(frontEnd.screen.screenAttrs.room, "zMergedList.Car");
  }
});

boot();
