// Usage: node verify-browser.cjs <playwright-core directory> <URL> <output directory>
const { chromium } = require(process.argv[2]);
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { spawn, spawnSync } = require('node:child_process');

(async () => {
  const output = path.resolve(process.argv[4]);
  fs.mkdirSync(output, { recursive: true });
  let server;
  let ready;
  let url = process.argv[3];
  if (url === 'auto') {
    const root = path.resolve(__dirname, '../../..');
    const python = spawnSync('py', ['-3', '-c', 'import sys; print(sys.executable)'], { encoding: 'utf8', windowsHide: true });
    if (python.status !== 0) throw new Error(python.stderr || 'Python discovery failed');
    ready = path.join(output, `server-ready-${Date.now()}.json`);
    server = spawn(python.stdout.trim(), [path.join(root, 'scripts/serve-web.py'), '--iteration', '012-campaign-fidelity', '--port', '0', '--ready-file', ready], { cwd: root, windowsHide: true, stdio: 'ignore' });
    try {
      for (let attempt = 0; !fs.existsSync(ready); attempt++) {
        if (server.exitCode !== null || attempt > 100) throw new Error('Owned test server failed');
        await new Promise(resolve => setTimeout(resolve, 100));
      }
      const info = JSON.parse(fs.readFileSync(ready, 'utf8'));
      assert.equal(info.pid, server.pid);
      url = `http://127.0.0.1:${info.port}/`;
    } catch (error) { server.kill(); throw error; }
  }
  let browser;
  try {
  // Use full Chromium's headless mode, not headless-shell, to avoid OS focus theft during driving.
  browser = await chromium.launch({ channel: 'chromium', headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  // Harness-only access to read-only telemetry; no test global in shipped main.js.
  await page.route('**/main.js', async route => {
    const response = await route.fetch();
    await route.fulfill({ response, body: (await response.text()) + '\n globalThis.readDriveTestState = () => ({ pose: Array.from(viewer.get_car_pose()), props: Array.from(viewer.get_scenario_prop_positions()), sim: viewer.is_sim_mode(), mass: viewer.get_car_sim_mass(), simName: viewer.get_car_sim_name(), participants: viewer.get_total_participants(), laps: viewer.get_total_laps(), career: activeCareerEvent, profile: JSON.stringify(currentProfile), phase: viewer.get_race_phase(), speed: viewer.get_car_speed(), gear: viewer.get_car_gear(), gates: viewer.get_mission_passed_gates(), goalSupported: viewer.get_mission_goal_supported(), goalReached: viewer.get_mission_goal_reached(), timedOut: viewer.get_mission_timed_out() });' });
  });
  const errors = [];
  const checks = [];
  const knownFailures = [];
  page.on('pageerror', error => errors.push(String(error)));
  page.on('console', message => {
    if (message.type() === 'error') errors.push(message.text());
    if (message.type() === 'warning') console.log('browser warning:', message.text());
  });
  const screenshot = name => page.screenshot({ path: path.join(output, name), fullPage: true });
  const sourceMass = name => fs.readFileSync(path.resolve(__dirname,
    '../../../local/game/GameData/Simulation/CarData', `${name}.sim`)).readFloatLE(0x40);
  const sourceName = name => fs.readFileSync(path.resolve(__dirname,
    '../../../local/game/GameData/Simulation/CarData', `${name}.sim`)).subarray(0, 64).toString('utf8').split('\0')[0].trim();
  const loaded = name => page.waitForFunction(name => {
    const status = document.querySelector('#status').textContent;
    return status.includes(`"${name}" загружен`) && !document.querySelector('#targetSelect').disabled;
  }, name, { timeout: 60000 });
  try {
    await page.goto(url);
    console.log('page loaded; waiting for skidpad');
    await loaded('skidpad');
    checks.push('skidpad boot');
    assert.equal((await page.evaluate(() => readDriveTestState())).mass, sourceMass('356coupe11'));
    checks.push('garage 356 runtime mass matches original 356coupe11.sim');
    console.log('skidpad loaded');
    assert.equal(await page.locator('#debugPanel').isVisible(), false);
    assert.equal(await page.locator('#summary').isVisible(), false);
    await screenshot('web-menu.png');
    await page.locator('#tourButton').click();
    assert.equal(await page.locator('#menu').isVisible(), false);
    assert.equal(await page.locator('#debugPanel').isVisible(), true);
    const waitForStart = () => page.waitForFunction(() => [0, 2].includes(readDriveTestState().phase), null, { timeout: 15000 });
    for (const mode of ['sim', 'arcade']) {
      if (mode === 'arcade') await page.keyboard.press('m');
      assert.equal(await page.evaluate(() => readDriveTestState().sim), mode === 'sim');
      for (const [key, sign, label] of [['a', -1, 'left'], ['ArrowRight', 1, 'right']]) {
        await page.keyboard.press('r');
        await waitForStart();
        await page.keyboard.down('w');
        await page.waitForTimeout(1200);
        const before = await page.evaluate(() => readDriveTestState().pose);
        await page.keyboard.down(key);
        await page.waitForTimeout(400);
        await page.keyboard.up(key);
        await page.keyboard.up('w');
        const after = await page.evaluate(() => readDriveTestState().pose);
        const headingRight = after[3] * -before[5] + after[5] * before[3];
        assert.ok(headingRight * sign > 0.001, `${mode} ${label}: headingRight=${headingRight}`);
        checks.push(`${mode} ${label} headingRight=${headingRight.toFixed(4)}`);
        await screenshot(`web-${mode}-${label}.png`);
      }
      if (mode === 'sim') {
        await page.keyboard.press('r');
        await waitForStart();
        await page.keyboard.down('w');
        await page.waitForFunction(() => readDriveTestState().speed > 15);
        await page.keyboard.up('w');
        await page.keyboard.down('s');
        await page.waitForFunction(() => {
          const state = readDriveTestState();
          return state.gear < 0 && state.speed < -3;
        }, null, { timeout: 12000 });
        await page.keyboard.up('s');
        await screenshot('web-sim-reverse.png');
        await page.keyboard.down('w');
        await page.waitForFunction(() => {
          const state = readDriveTestState();
          return state.gear > 0 && state.speed > 3;
        }, null, { timeout: 12000 });
        await page.keyboard.up('w');
        checks.push('sim held S brakes then reverses; held W brakes reverse then drives forward');
      }
    }
    await page.keyboard.press('r');
    await waitForStart();
    await page.keyboard.down('w');
    await page.waitForTimeout(1800);
    await page.keyboard.down('d');
    await page.waitForTimeout(350);
    await page.keyboard.up('d');
    await page.keyboard.up('w');
    const speed = Number(await page.locator('#speedValue').textContent());
    assert.ok(speed > 5, `driving speed ${speed}`);
    checks.push(`drive + simultaneous throttle/steer; speed=${speed}`);
    await screenshot('web-drive.png');
    await page.keyboard.press('h');
    assert.equal(await page.locator('#hudCanvas').isVisible(), false);
    await screenshot('web-hidden-hud.png');
    await page.keyboard.press('Escape');
    assert.equal(await page.locator('#menu').isVisible(), true);
    checks.push('Esc restores menu; H hides HUD');
    await page.locator('#targetSelect').selectOption('alps');
    await loaded('alps');
    await page.locator('#detailsButton').click();
    assert.equal(await page.locator('#summary').isVisible(), true);
    await screenshot('web-alps.png');
    await page.locator('#detailsButton').click();
    await page.locator('#modeSelect').selectOption('car');
    await page.waitForFunction(() => !document.querySelector('#targetSelect').disabled);
    await page.locator('#targetSelect').selectOption('356a');
    await loaded('356a');
    assert.equal(await page.locator('#debugPanel').isVisible(), false);
    await page.keyboard.press('Escape');
    await screenshot('web-car.png');
    await page.keyboard.press('Escape');
    checks.push('alps/car catalog switch; diagnostic panel');
    await page.locator('#modeSelect').selectOption('track');
    await page.waitForFunction(() => !document.querySelector('#targetSelect').disabled);
    await page.locator('#targetSelect').selectOption('skidpad');
    await loaded('skidpad');
    await page.locator('#tourButton').click();
    // Losing focus must release keys; return to the canvas without a new keydown.
    await page.keyboard.down('w');
    await page.waitForTimeout(400);
    const other = await browser.newPage();
    await other.bringToFront();
    await other.waitForTimeout(200);
    await page.bringToFront();
    await page.keyboard.up('w');
    await page.keyboard.press('h');
    const before = Number(await page.locator('#speedValue').textContent());
    await page.waitForTimeout(500);
    const after = Number(await page.locator('#speedValue').textContent());
    assert.ok(after <= before + 1, `focus release coast ${before} -> ${after}`);
    checks.push(`focus release ${before} -> ${after}`);
    await other.close();

    // Verify Dealership (Point 2: car purchase no error)
    await page.keyboard.press('Escape');
    await page.locator('#dealershipBtn').click();
    assert.equal(await page.locator('#dealershipModal').isVisible(), true);
    await page.waitForTimeout(400);
    await page.locator('#dealershipUsedTab').click();
    await page.waitForTimeout(400);
    await screenshot('web-dealership.png');
    await page.locator('#dealershipNewTab').click();
    await page.waitForTimeout(300);
    await page.locator('#dealershipCloseBtn').click();
    checks.push('dealership catalog and used market verified without errors');

    // Verify Canyon track loading (Point 4: Canyon track not broken)
    await page.locator('#targetSelect').selectOption('canyon');
    await loaded('canyon');
    await page.locator('#tourButton').click();
    await waitForStart();
    await page.waitForTimeout(1000);
    const canyonPose = await page.evaluate(() => readDriveTestState().pose);
    assert.ok(Number.isFinite(canyonPose[1]), 'canyon altitude ' + canyonPose[1]);
    await screenshot('web-canyon.png');
    await page.keyboard.press('Escape');
    checks.push('canyon loaded and car grounded at y=' + canyonPose[1].toFixed(1));

    // Career identity: Evolution enters tournament selection, without Factory imagery.
    await page.locator('#careerEvolutionBtn').click();
    assert.equal(await page.locator('#evolutionTournamentsModal').isVisible(), true);
    assert.equal(await page.locator('#eventBriefingModal').isVisible(), false);
    await screenshot('web-evolution-cups.png');
    const firstCupBtn = page.locator('#evolutionCupsGrid .cup-card:first-child button');
    await firstCupBtn.click();
    assert.equal(await page.locator('#briefingCategoryBadge').textContent(), 'PORSCHE EVOLUTION');
    assert.equal(await page.locator('#eventBriefingModal').getAttribute('data-career-mode'), 'evolution');
    assert.equal(await page.locator('.briefing-visual-col').isVisible(), false);
    assert.equal(await page.locator('#briefingTrackFacility').textContent(), 'EVOLUTION TOURNAMENT');
    await screenshot('web-evolution-briefing.png');
    checks.push('Evolution cup selection and briefing contain no Factory portrait or mission map');

    // A damaged required SIM must keep the briefing open, never start on model-table physics.
    await page.route('**/356coupe11.sim', route => route.fulfill({ status: 200, body: Buffer.from([0, 1, 2]) }));
    await page.locator('#briefingStartBtn').click();
    await page.waitForFunction(() => document.querySelector('#status').textContent.includes('Заезд не начат:'));
    assert.equal(await page.locator('#eventBriefingModal').isVisible(), true);
    await page.unroute('**/356coupe11.sim');
    await page.locator('#briefingStartBtn').click();
    await page.waitForFunction(() => document.querySelector('#status').textContent.includes('Карьерный заезд начат:'));
    assert.equal((await page.evaluate(() => readDriveTestState())).mass, sourceMass('356coupe11'));
    assert.equal((await page.evaluate(() => readDriveTestState())).sim, true);
    await page.keyboard.press('Escape');
    checks.push('Evolution rejects damaged SIM; retry applies original 356coupe11.sim');

    // Verify Factory Driver campaign (Points 1 & 3: authentic briefing & genuine car model)
    await page.locator('#careerFactoryBtn').click();
    assert.equal(await page.locator('#factoryMissionsModal').isVisible(), true);
    await page.waitForTimeout(400);
    await screenshot('web-factory-ladder.png');
    const firstMissionBtn = page.locator('#factoryMissionsGrid .cup-card:first-child button');
    await firstMissionBtn.click();
    assert.equal(await page.locator('#eventBriefingModal').isVisible(), true);
    assert.equal(await page.locator('#briefingCategoryBadge').textContent(), 'PORSCHE FACTORY DRIVER');
    assert.equal(await page.locator('#eventBriefingModal').getAttribute('data-career-mode'), 'factory');
    assert.equal(await page.locator('#briefingInstructorImg').isVisible(), true);
    assert.equal(await page.locator('#briefingMapImg').isVisible(), false);
    const briefingCanvas = await page.locator('#eventBriefingModal .briefing-card').boundingBox();
    const portraitBox = await page.locator('.instructor-photo-frame').boundingBox();
    assert.ok(Math.abs(briefingCanvas.width / briefingCanvas.height - 4 / 3) < 0.01);
    assert.ok(Math.abs((portraitBox.x - briefingCanvas.x) / briefingCanvas.width - 147 / 640) < 0.01);
    assert.ok(Math.abs((portraitBox.y - briefingCanvas.y) / briefingCanvas.height - 235 / 480) < 0.01);
    assert.ok((await page.locator('#briefingDesc').textContent()).length > 0);
    await screenshot('web-factory-briefing.png');
    await page.locator('#briefingMapBtn').click();
    assert.equal(await page.locator('#briefingMapImg').isVisible(), true);
    await screenshot('web-factory-mission-map.png');
    await page.locator('#briefingMapBtn').click();
    await page.locator('#briefingStartBtn').click();
    await page.waitForFunction(() => readDriveTestState().goalSupported, null, { timeout: 30000 });
    await page.waitForTimeout(1000);
    const missionState = await page.evaluate(() => readDriveTestState());
    assert.equal(missionState.mass, sourceMass('boxster25'));
    assert.equal(missionState.simName, sourceName('boxster25'));
    assert.equal(missionState.sim, true);
    checks.push('Factory 0M01 runtime mass matches original boxster25.sim');
    assert.equal(missionState.phase, 2);
    assert.equal(missionState.goalReached, false);
    assert.equal(missionState.timedOut, false);
    assert.ok(Math.abs(missionState.pose[0] + 6.844955) < 0.5, 'SCN Start X');
    assert.ok(Math.abs(missionState.pose[2] + 147.240662) < 0.5, 'SCN Start Z');
    assert.ok(missionState.pose[1] > 0.1 && missionState.pose[1] < 1.0, 'car rests on the mission pad');
    await screenshot('web-factory-mission-drive.png');
    const activeArrows = props => {
      assert.equal(props.length, 15, 'all five source triggerable arrows loaded');
      return [0, 1, 2, 3, 4].filter(i => props[i * 3 + 1] < 8000);
    };
    assert.deepEqual(activeArrows(missionState.props), [0], 'Start link selects Arrow 1; L A hidden');
    assert.equal(missionState.gates, 0, 'nearby End cannot finish before waypoints');
    await page.keyboard.down('w');
    try {
      await page.waitForFunction(() => {
        const props = readDriveTestState().props;
        return props.length === 15 && props[4] < 8000 && props[1] > 8000;
      }, null, { timeout: 12000 });
    } finally { await page.keyboard.up('w'); }
    assert.deepEqual(activeArrows((await page.evaluate(() => readDriveTestState())).props), [1]);
    const firstGate = await page.evaluate(() => readDriveTestState());
    assert.equal(firstGate.gates, 1);
    assert.ok(firstGate.pose[2] < -115.951881, 'body diagonal triggers before center crosses the source segment');
    await screenshot('web-factory-arrow-waypoint.png');
    await page.keyboard.press('r');
    await page.waitForTimeout(200);
    assert.deepEqual(activeArrows((await page.evaluate(() => readDriveTestState())).props), [0], 'restart restores Start arrow');
    assert.equal((await page.evaluate(() => readDriveTestState())).gates, 0, 'restart clears trigger progression');
    checks.push('0M01 source triggerable metadata: Start Arrow 1, driven waypoint Arrow 2, restart Arrow 1; L A hidden');
    // Separate diagnostic for the known 0M01 surface/start regression. Quick Race
    // below must still be exercised even when this mission-specific probe fails.
    try {
      await page.keyboard.down('w');
      await page.waitForFunction(() => readDriveTestState().speed > 15, null, { timeout: 12000 });
      await page.keyboard.up('w');
      await page.keyboard.down('s');
      await page.waitForFunction(() => readDriveTestState().gear < 0 && readDriveTestState().speed < -3, null, { timeout: 12000 });
      await page.keyboard.up('s');
      await page.keyboard.down('w');
      await page.waitForFunction(() => readDriveTestState().gear > 0 && readDriveTestState().speed > 3, null, { timeout: 12000 });
      checks.push('0M01 Boxster held S/W direction changes');
    } catch (error) {
      const diagnostic = { error: String(error), state: await page.evaluate(() => readDriveTestState()) };
      fs.writeFileSync(path.join(output, 'factory-controls.json'), JSON.stringify(diagnostic, null, 2));
      await screenshot('web-factory-controls.png');
      knownFailures.push('0M01 held S/W direction change: see factory-controls.json');
    } finally {
      await page.keyboard.up('w');
      await page.keyboard.up('s');
    }
    await page.keyboard.press('Escape');
    checks.push('0M01 source briefing placement and SCN start/support; no instant goal (original offset/mission rules not verified)');

    // Quick Race must remove the previous Factory state and leave the profile untouched.
    const profileBeforeQuickRace = (await page.evaluate(() => readDriveTestState())).profile;
    await page.locator('#quickRaceBtn').click();
    await page.locator('#quickRaceTrack').selectOption('skidpad');
    await page.locator('#quickRaceCar').selectOption('356a_coupe');
    await page.locator('#quickRaceLaps').selectOption('5');
    await page.locator('#quickRaceOpponents').selectOption('0');
    await screenshot('web-quick-race-setup.png');
    await page.locator('#quickRaceStartBtn').click();
    await page.waitForFunction(() => document.querySelector('#quickRaceModal').hidden && document.querySelector('#status').textContent.startsWith('Quick Race:'));
    await waitForStart();
    const soloRace = await page.evaluate(() => readDriveTestState());
    assert.equal(soloRace.career, null);
    assert.equal(soloRace.goalSupported, false);
    assert.equal(soloRace.participants, 1);
    assert.equal(soloRace.laps, 5);
    assert.equal(soloRace.mass, sourceMass('356Acoupe16'));
    assert.equal(soloRace.simName, sourceName('356Acoupe16'));
    await page.keyboard.down('w');
    await page.waitForFunction(() => readDriveTestState().speed > 10, null, { timeout: 12000 });
    await page.keyboard.up('w');
    await screenshot('web-quick-race-solo.png');
    await page.keyboard.press('r');
    await waitForStart();
    const restarted = await page.evaluate(() => readDriveTestState());
    assert.equal(restarted.participants, 1);
    assert.equal(restarted.laps, 5);
    assert.equal(restarted.mass, sourceMass('356Acoupe16'));
    await page.keyboard.press('Escape');
    await page.locator('#quickRaceBtn').click();
    await page.locator('#quickRaceCar').selectOption('boxster_986');
    await page.locator('#quickRaceLaps').selectOption('1');
    await page.locator('#quickRaceOpponents').selectOption('3');
    await page.locator('#quickRaceStartBtn').click();
    await page.waitForFunction(() => document.querySelector('#quickRaceModal').hidden && document.querySelector('#status').textContent.startsWith('Quick Race:'));
    await waitForStart();
    const gridRace = await page.evaluate(() => readDriveTestState());
    assert.equal(gridRace.participants, 4);
    assert.equal(gridRace.laps, 1);
    assert.equal(gridRace.simName, sourceName('boxster25'));
    assert.equal(gridRace.profile, profileBeforeQuickRace);
    await screenshot('web-quick-race-grid.png');
    checks.push('Quick Race after Factory: original 356 A/Boxster SIM, 5/1 laps, solo/3 AI, restart and unchanged profile');

    await page.keyboard.down('w');
    await page.waitForFunction(() => readDriveTestState().speed > 15, null, { timeout: 12000 });
    await page.keyboard.up('w');
    await page.keyboard.down('s');
    await page.waitForFunction(() => readDriveTestState().gear < 0 && readDriveTestState().speed < -3, null, { timeout: 12000 });
    await page.keyboard.up('s');
    await page.keyboard.down('w');
    await page.waitForFunction(() => readDriveTestState().gear > 0 && readDriveTestState().speed > 3, null, { timeout: 12000 });
    await page.keyboard.up('w');
    checks.push('original Boxster SIM held S to reverse and W back to forward in Quick Race');

    assert.deepEqual(errors, []);
    fs.writeFileSync(path.join(output, 'browser-check.json'), JSON.stringify({ browser: browser.version(), iteration: '012-campaign-fidelity', checks, errors, knownFailures }, null, 2));
    console.log(JSON.stringify({ checks, errors, knownFailures }));
    if (knownFailures.length) process.exitCode = 1;
  } catch (error) {
    await screenshot('web-failure.png').catch(() => {});
    fs.writeFileSync(path.join(output, 'browser-failure.json'), JSON.stringify({ checks, errors, error: String(error), state: await page.evaluate(() => readDriveTestState()).catch(() => null), status: await page.locator('#status').textContent().catch(() => '') }, null, 2));
    throw error;
  } finally {
    await browser.close();
  }
  } finally {
    if (server) server.kill();
    if (ready && fs.existsSync(ready)) {
      try { fs.unlinkSync(ready); } catch {}
    }
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
