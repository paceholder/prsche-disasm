// DOM-state regression tests; these do not replace a browser/GPU visual run.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

class Element {
  constructor() {
    this.listeners = new Map();
    this.attributes = new Map();
    this.classes = new Set();
    this.classList = {
      toggle: (name, enabled) => enabled ? this.classes.add(name) : this.classes.delete(name),
      add: (name) => this.classes.add(name),
      remove: (name) => this.classes.delete(name),
    };
    this.style = {};
    this.dataset = {};
    this.hidden = true;
    this.value = "track";
    this.children = [];
  }
  addEventListener(type, callback) { this.listeners.set(type, callback); }
  setAttribute(name, value) { this.attributes.set(name, value); }
  focus() { focused = this; }
  dispatch(type, event = {}) { this.listeners.get(type)?.(event); }
  replaceChildren(...children) { this.children = children; }
  appendChild(child) { this.children.push(child); }
}
class Input extends Element {}
class Textarea extends Element {}
class Select extends Element {}
const elements = new Map();
let focused;
const element = (id) => {
  if (!elements.has(id)) elements.set(id, new Element());
  return elements.get(id);
};
const document = new Element();
document.body = new Element();
document.querySelector = element;
document.getElementById = (id) => element(`#${id}`);
document.querySelectorAll = () => [];
document.createElement = () => new Element();
const window = new Element();
const context = vm.createContext({
  document, window, console, setTimeout,
  HTMLInputElement: Input, HTMLTextAreaElement: Textarea, HTMLSelectElement: Select,
});
const source = readFileSync(new URL("main.js", import.meta.url), "utf8")
  .replace(/^import .*;\r?\n/, "")
  .replace(/\bboot\(\);\s*$/, "");
vm.runInContext(source + `\n globalThis.testApi = {
  setViewer: value => viewer = value,
  activeKeys, syncTourButton, updateNavigation, showBriefing, factoryGoalSatisfied, evaluateFactoryResult, findSimResource, buildQuickRaceCars,
  primeMotion: () => { velForward = 3; velRight = 2; velZoom = 1; },
  motion: () => [velForward, velRight, velZoom],
};`, context);
let driving = false;
let lastControls;
// Original C-key cycle from TailCam0 (Camera_Cycle, list & 3); V is the debug free camera.
const cameraCycle = ["HeliCam2", "BumperCam2", "BumperCam", "TailCam0"];
let cameraIndex = 3;
let freeCamera = false;
let originalModel = false;
let lights = false;
let sideView = 0;
let aiLimit = -1;
context.testApi.setViewer({
  toggle_vehicle_model: () => originalModel = !originalModel,
  has_original_vehicle: () => true,
  cycle_car_view: () => { freeCamera = false; cameraIndex = (cameraIndex + 1) % 4; return cameraCycle[cameraIndex]; },
  get_camera_name: () => freeCamera ? "Free" : cameraCycle[cameraIndex],
  toggle_free_camera: () => freeCamera = !freeCamera,
  is_free_camera: () => freeCamera,
  is_original_vehicle: () => originalModel,
  toggle_lights: () => lights = !lights,
  cycle_side_view: () => sideView = sideView === 0 ? 25 : sideView > 0 ? -25 : 0,
  get_side_view: () => sideView,
  cycle_ai_speed_limit: () => aiLimit = aiLimit < 0 ? 30 : aiLimit > 0 ? 0 : -1,
  get_ai_speed_limit: () => aiLimit,
  is_drive_mode: () => driving,
  has_car: () => true,
  toggle_camera_mode: () => driving = !driving,
  update_car: (...args) => lastControls = args,
  get_car_speed: () => 0,
  get_car_gear: () => 0,
  get_car_rpm: () => 0,
  get_car_engine_rpm: () => 0,
  get_car_redline_rpm: () => 6000,
  get_car_view: () => 0,
  get_tyre_sound_inputs: () => [],
});
context.testApi.syncTourButton();
assert.equal(element("#hudCanvas").hidden, true, "HUD hidden before driving");
element("#tourButton").dispatch("click");
assert.equal(driving, true);
assert.equal(document.body.classes.has("menu-hidden"), true);
assert.equal(element("#hudCanvas").hidden, false);
assert.equal(element("#debugPanel").hidden, false, "debug panel is on by default in a race");
assert.equal(focused, element("#viewport"));
// Exercise actual keyboard listeners and navigation, including both key layouts.
const key = (type, code, extra = {}) => window.dispatch(type, {
  code, target: new Element(), repeat: false, preventDefault() {}, ...extra,
});
for (const [gas, left, right] of [["KeyW", "KeyA", "KeyD"], ["ArrowUp", "ArrowLeft", "ArrowRight"]]) {
  for (const [turn, sign] of [[left, -1], [right, 1]]) {
    key("keydown", gas);
    key("keydown", turn);
    context.testApi.updateNavigation(1 / 60);
    assert.deepEqual(Array.from(lastControls), [1 / 60, 1, sign, false]);
    key("keyup", gas);
    key("keyup", turn);
    context.testApi.updateNavigation(1 / 60);
    assert.deepEqual(Array.from(lastControls), [1 / 60, 0, 0, false]);
  }
}
for (const reverse of ["KeyS", "ArrowDown"]) {
  key("keydown", reverse);
  key("keydown", "KeyA");
  key("keydown", "KeyD");
  key("keydown", "Space");
  context.testApi.updateNavigation(1 / 60);
  assert.deepEqual(Array.from(lastControls), [1 / 60, -1, 0, true]);
  window.dispatch("blur");
  context.testApi.updateNavigation(1 / 60);
  assert.deepEqual(Array.from(lastControls), [1 / 60, 0, 0, false]);
}
context.testApi.updateNavigation(1 / 60);
const panel = () => element("#debugPanel").textContent.replace(/\u00a0/g, " ");
context.testApi.updateNavigation(1 / 60);
context.testApi.syncTourButton();
for (const part of ["O car prototype", "K collisions original", "M 6 DOF", "L lights off", "C TailCam0",
  "V free cam off", "Shift+V side off", "B boxes off", "H HUD on", "R reset", "Shift+R restart", "Esc menu"]) {
  assert.ok(panel().includes(part), `panel shows "${part}": ${panel()}`);
}
assert.ok(panel().includes("J AI full speed"));
key("keydown", "KeyJ");
assert.ok(panel().includes("J AI 30 km/h"));
key("keydown", "KeyJ");
assert.ok(panel().includes("J AI parked"));
key("keydown", "KeyJ");
const shownCameras = [];
for (let i = 0; i < 4; i++) {
  key("keydown", "KeyC");
  key("keyup", "KeyC");
  shownCameras.push(panel().match(/C (\w+)/)[1]);
}
assert.deepEqual(shownCameras, ["HeliCam2", "BumperCam2", "BumperCam", "TailCam0"]);
key("keydown", "KeyO");
assert.equal(originalModel, true, "O selects the original vehicle model");
assert.ok(panel().includes("O car original"));
key("keydown", "KeyO");
assert.ok(panel().includes("O car prototype"));
key("keydown", "KeyV");
assert.ok(panel().includes("C Free") && panel().includes("V free cam on"), "V toggles the free camera");
key("keydown", "KeyV");
assert.ok(panel().includes("C TailCam0") && panel().includes("V free cam off"));
key("keydown", "KeyL");
assert.ok(panel().includes("L lights on"), "L shows in the panel");
key("keydown", "KeyV", { shiftKey: true });
assert.ok(panel().includes("Shift+V side R 25\u00b0"), panel());
key("keydown", "KeyV", { shiftKey: true });
assert.ok(panel().includes("Shift+V side L 25\u00b0"));
key("keydown", "KeyV", { shiftKey: true });
assert.ok(panel().includes("Shift+V side off"));
window.dispatch("keydown", { code: "KeyH", target: new Element(), repeat: false });
assert.equal(element("#hudCanvas").hidden, true);
window.dispatch("keydown", { code: "Backquote", target: new Element(), repeat: false });
assert.equal(element("#debugPanel").hidden, true, "Backquote hides the debug panel");
window.dispatch("keydown", { code: "Backquote", target: new Element(), repeat: false });
assert.equal(element("#debugPanel").hidden, false, "Backquote shows it again");
window.dispatch("keydown", { code: "Escape", target: new Element(), repeat: false, preventDefault() {} });
assert.equal(document.body.classes.has("menu-hidden"), false);
assert.equal(element("#menuButton").attributes.get("aria-expanded"), "true");
window.dispatch("keydown", { code: "KeyW", target: new Select(), repeat: false });
assert.equal(context.testApi.activeKeys.size, 0, "selectors must not drive");
window.dispatch("keydown", { code: "KeyW", target: new Element(), repeat: false });
assert.equal(context.testApi.activeKeys.has("KeyW"), true);
context.testApi.primeMotion();
window.dispatch("blur");
assert.equal(context.testApi.activeKeys.size, 0);
assert.deepEqual(Array.from(context.testApi.motion()), [0, 0, 0]);
element("#detailsButton").dispatch("click");
assert.equal(element("#summary").hidden, false);
element("#detailsButton").dispatch("click");
assert.equal(element("#summary").hidden, true);
element("#tourButton").dispatch("click");
assert.equal(driving, false);
assert.equal(document.body.classes.has("menu-hidden"), false);
assert.equal(element("#hudCanvas").hidden, true);

// 009 Shell UI & Modals regression checks
// Sound is on by default; the button toggles it off and on again.
assert.equal(element("#soundBtn").classes.has("active"), true, "Sound starts on");
element("#soundBtn").dispatch("click");
assert.equal(element("#soundBtn").classes.has("active"), false, "Sound button toggles off");
element("#soundBtn").dispatch("click");
assert.equal(element("#soundBtn").classes.has("active"), true, "Sound button toggles on");

element("#careerEvolutionBtn").dispatch("click");
assert.equal(element("#evolutionTournamentsModal").hidden, false, "Evolution opens its tournament selection");
assert.equal(element("#eventBriefingModal").hidden, true, "Evolution does not skip straight to the first race");
element("#evolutionCloseBtn").dispatch("click");
assert.equal(element("#evolutionTournamentsModal").hidden, true);

element("#careerFactoryBtn").dispatch("click");
assert.equal(element("#factoryMissionsModal").hidden, false, "Factory missions ladder opens");
element("#factoryModalCloseBtn").dispatch("click");
assert.equal(element("#factoryMissionsModal").hidden, true, "Factory missions ladder closes");

context.testApi.showBriefing({ title: "Factory mission", description: "Test", is_factory: true,
  car: "Boxster", track: "Skid Pad", reward: "Rank", mission_obj: { code: "0M01", car_model: "boxster" } });
assert.equal(element("#eventBriefingModal").dataset.careerMode, "factory");
assert.equal(element("#briefingMapImg").src, "./assets/map_0m01.png");
assert.equal(element("#briefingInstructorImg").src, "./assets/people_rolp.png");
assert.equal(element("#briefingMapBtn").hidden, false);
element("#briefingMapBtn").dispatch("click");
assert.equal(element("#eventBriefingModal").dataset.factoryView, "map");
assert.equal(element("#briefingMapBtn").textContent, "БРИФИНГ");
element("#briefingMapBtn").dispatch("click");
assert.equal(element("#eventBriefingModal").dataset.factoryView, "briefing");
element("#briefingCancelBtn").dispatch("click");
context.testApi.showBriefing({ title: "Evolution cup", description: "Race", is_factory: false,
  car: "356", track: "Canyon", reward: "Credits" });
assert.equal(element("#eventBriefingModal").dataset.careerMode, "evolution");
assert.equal(element("#briefingTrackFacility").textContent, "EVOLUTION TOURNAMENT");
assert.equal(element("#briefingMapImg").src, "./assets/map_0m01.png", "Evolution must not select a factory map");
assert.equal(element("#briefingMapBtn").hidden, true);
element("#briefingCancelBtn").dispatch("click");

element("#profileBtn").dispatch("click");
assert.equal(element("#profileModal").hidden, false, "Profile modal opens");
element("#profileModalCloseBtn").dispatch("click");
assert.equal(element("#profileModal").hidden, true, "Profile modal closes");

// 010 Dealership & Parts Tuning Shop checks
element("#dealershipBtn").dispatch("click");
assert.equal(element("#dealershipModal").hidden, false, "Dealership modal opens");
element("#dealershipCloseBtn").dispatch("click");
assert.equal(element("#dealershipModal").hidden, true, "Dealership modal closes");

element("#partsShopBtn").dispatch("click");
assert.equal(element("#partsShopModal").hidden, false, "Parts shop modal opens");
element("#partsShopCloseBtn").dispatch("click");
assert.equal(element("#partsShopModal").hidden, true, "Parts shop modal closes");

// Input field typing isolation
window.dispatch("keydown", { code: "KeyW", target: new Input(), repeat: false });
assert.equal(context.testApi.activeKeys.size, 0, "Inputs must not capture drive keys");

const goal = context.testApi.factoryGoalSatisfied;
const missionDriver = (supported, reached) => ({
  get_mission_goal_supported: () => supported,
  get_mission_goal_reached: () => reached,
});
assert.equal(goal({}, 12, 32), false, "old viewer/lap-only result must not award a mission");
assert.equal(goal(missionDriver(false, true), 12, 32), false, "unsupported goals cannot award progress");
assert.equal(goal(missionDriver(true, false), 12, 32), false, "time alone cannot award progress");
assert.equal(goal(missionDriver(true, true), 32.01, 32), false, "late goal fails");
assert.equal(goal(missionDriver(true, true), 0, 32), false, "reset is not a finish");
assert.equal(goal(missionDriver(true, true), NaN, 32), false);
assert.equal(goal(missionDriver(true, true), 12, 32), true);
const evaluateFactory = context.testApi.evaluateFactoryResult;
const factoryEvent = { time_limit: 32, mission_index: 1 };
let evaluations = 0;
const penalizedDriver = { ...missionDriver(true, true), get_cone_hits: () => 2 };
const penaltyEvaluator = (...args) => {
  evaluations++;
  assert.equal(args[6], 2, "actual cone hits must reach authoritative evaluation");
  return JSON.stringify({ passed: false, total_time: 34, penalty: 4, profile: { marker: "unchanged" } });
};
const penalizedResult = evaluateFactory(penalizedDriver, factoryEvent, { marker: "unchanged" }, 30, 1, penaltyEvaluator);
assert.equal(penalizedResult.passed, false, "raw finish before 32 seconds must not override penalty failure");
assert.equal(penalizedResult.total_time, 34);
assert.equal(evaluations, 1);
assert.equal(evaluateFactory(missionDriver(true, false), factoryEvent, {}, 12, 1, penaltyEvaluator), null);
assert.equal(evaluateFactory(missionDriver(false, true), factoryEvent, {}, 12, 1, penaltyEvaluator), null);
assert.equal(evaluations, 1, "unreached/unsupported goal must never mutate profile");
assert.equal(evaluateFactory(penalizedDriver, factoryEvent, {}, 12, 1, null), null);
const passedResult = evaluateFactory(penalizedDriver, factoryEvent, {}, 12, 1,
  () => JSON.stringify({ passed: true, total_time: 16, penalty: 4, profile: { progressed: true } }));
assert.equal(passedResult.passed, true);
const findSim = context.testApi.findSimResource;
const simFiles = ["GameData/Simulation/CarData/boxster25.sim", "GameData/Simulation/CarData/356Acoupe16.sim"];
assert.equal(findSim(simFiles, "BOXSTER25.SIM"), simFiles[0]);
assert.equal(findSim(simFiles, "356Acoupe16"), simFiles[1]);
assert.throws(() => findSim(simFiles, "boxster"), /boxster\.sim/);
assert.throws(() => findSim(simFiles, ""), /Не указан/);
assert.throws(() => findSim(["a/boxster25.sim", "other/boxster25.sim"], "boxster25"), /несколько/);
// 959coupe29 and 928coupeGTS54 exist twice; the original reads Simulation/CarData.
const twin = ["GameData/Simulation/928coupeGTS54.sim", "GameData/Simulation/CarData/928coupeGTS54.sim"];
assert.equal(findSim(twin, "928coupeGTS54"), twin[1]);
const customSim = { path: "Chosen\\GameData\\boxster25.sim" };
assert.equal(findSim([customSim], "boxster25", file => file.path), customSim);
// Exercise the actual frame loop and Continue handler with terminal race telemetry.
let phase = 2;
let resetRequests = 0;
context.testApi.setViewer({
  request_car_reset: () => { resetRequests++; },
  is_drive_mode: () => true, has_car: () => true, update_car() {},
  get_car_speed: () => 0, get_car_gear: () => 0, get_car_rpm: () => 0,
  get_car_engine_rpm: () => 0, get_car_redline_rpm: () => 6000, get_car_view: () => 0, get_tyre_sound_inputs: () => [],
  get_race_phase: () => phase, get_player_position: () => 1,
  get_total_participants: () => 1, get_current_lap: () => 1,
  get_total_laps: () => 1, get_current_lap_time: () => 12,
  get_best_lap_time: () => 12, is_wrong_way: () => false,
  get_countdown_remaining: () => 3, restart_race: () => { phase = 1; },
});
const tick = () => context.testApi.updateNavigation(1 / 60);
const resultModal = element("#raceResultsModal");
for (const terminalPhase of [4, 5]) {
  phase = 2;
  tick();
  phase = terminalPhase;
  tick();
  assert.equal(resultModal.hidden, false, `phase ${phase} presents results`);
  element("#raceContinueBtn").dispatch("click");
  for (let i = 0; i < 60; i++) tick();
  assert.equal(resultModal.hidden, true, "Continue must stay dismissed on subsequent frames");
  context.testApi.showBriefing({ title: "Next event", is_factory: true, mission_obj: { code: "1m01" } });
  tick();
  assert.equal(resultModal.hidden, true, "next briefing must not consume the previous finish");
  element("#briefingCancelBtn").dispatch("click");
  // R is the original track reset: the race goes on.
  const before = resetRequests;
  key("keydown", "KeyR");
  assert.equal(resetRequests, before + 1, "R requests a track reset");
  assert.equal(phase, terminalPhase, "R must not restart the race");
  for (const restart of [
    () => key("keydown", "KeyR", { shiftKey: true }),
    () => element("#resetButton").dispatch("click"),
    () => element("#raceRestartBtn").dispatch("click"),
  ]) {
    restart();
    assert.equal(phase, 1, "each restart starts a new countdown");
    tick();
    phase = terminalPhase;
    tick();
    assert.equal(resultModal.hidden, false, "restart arms results for a new attempt");
    element("#raceContinueBtn").dispatch("click");
  }
}
console.log("UI state: existing checks and result Continue/restart lifecycle passed");

// Quick Race list comes from the catalogue records, keyed by record index.
{
  const rec = (index, display_name, model, sim_name, bank = "sd1a", style = 0) => ({ index, display_name, model, sim_name, bank, style });
  const cars = context.testApi.buildQuickRaceCars([
    rec(0, "'50 356 1100 Cabriolet", "356_1", "356cabrio11"),
    rec(1, "Eden2", "Eden", ""),
    rec(2, "Gone", "nomodel", "356cabrio11"),
    rec(3, "NoSim", "356_1", "missing"),
    rec(4, "'00 911 Turbo (996)", "F996", "996Tcoupe36", "sd7a", 8),
    rec(5, "Twin", "F996", "996Tcoupe36", "sd7b"),
  ], ["356_1", "f996"], ["GameData/Simulation/CarData/356cabrio11.sim", "x/996tcoupe36.sim"]);
  assert.deepEqual(Array.from(cars, c => c.id), ["0", "4", "5"]);
  assert.equal(cars[1].model_name, "f996");
  assert.equal(cars[1].sim_name, "996Tcoupe36");
  assert.equal(cars[1].bank, "sd7a");
  assert.equal(cars[1].style, 8, "the TPG style index of the record reaches the car");
  assert.equal(cars[0].style, 0);
  assert.equal(cars[2].bank, "sd7b", "repeated SIM names stay distinct by record index");
}
