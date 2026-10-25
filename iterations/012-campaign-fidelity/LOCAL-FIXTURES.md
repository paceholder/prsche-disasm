# Local test recordings

Some tests compare the port with the original `nfs5.exe`. They replay recordings
made by running the original functions in [Unicorn](https://www.unicorn-engine.org/)
on data from the game. Because the recordings are derived from the game, they are not
in the repository. This page explains how to make them from your own copy.

Without the recordings these tests print a `SKIPPED` line and pass; everything else
runs normally.

## What you need

- An installed copy of the game: a directory with `nfs5.exe` (`Porsche.exe` v3.5;
  the virtual addresses are identical) and its `GameData/`.
- Python 3 with `unicorn==2.1.3`. The recorders use only the standard library otherwise.
- A Rust toolchain (`cargo`) for the road-support step.

```sh
python3 -m venv .venv
.venv/bin/pip install unicorn==2.1.3
```

## Generate

From the repository root:

```sh
GAME_DIR=/path/to/game PYTHON=.venv/bin/python \
  scripts/research/record-local-fixtures.sh --check
```

This writes 33 TSV files (about 35 MB) to the gitignored `local/fixtures/` and takes
several minutes. With `--check` the script then compares every file with
`scripts/research/local-fixtures.sha256`, the hashes of the data the tests were verified
with. A mismatch means a different game build, modified game files or a changed
recorder. Every recorder is deterministic, so the same inputs give the same bytes.

To point the tests at recordings stored elsewhere, set `NFS_LOCAL_FIXTURES=<dir>`.

## What is recorded

All recorders are in `scripts/research/`. The tests are in `crates/nfs-assets/tests/`.

| Files | Recorder | Original code | Game input | Test |
|---|---|---|---|---|
| `vehicle-sim.tsv` | `replay-vehicle-sim.py` | `Sim_Load`, `Sim_DeriveFields` 0x49cb50 | every `GameData/Simulation/CarData/*.sim` | `original_vehicle_sim.rs` |
| `vehicle-tyre-*.tsv` | `replay-vehicle-tyre.py` | tyre grip and force (0x4a1900 and callees) | `.sim` files | `original_vehicle_tyre.rs` |
| `vehicle-engine-*.tsv` | `replay-vehicle-engine.py` | engine and gearbox (`Car_EngineDriveForce` 0x49e440 and callees) | `.sim` files | `original_vehicle_engine.rs` |
| `vehicle-controls-*.tsv` | `replay-vehicle-controls.py` | driver controls | `.sim` files | `original_vehicle_controls.rs` |
| `vehicle-step-*.tsv` | `replay-vehicle-step.py` | planar step in `Sched_Step` 0x414ff0 order | `.sim` files | `vehicle_step.rs`, `original_vehicle_driver.rs` |
| `vehicle-ground-*.tsv` | `replay-vehicle-ground.py` | `Car_GroundContactTick` 0x49bd40, `Car_WheelGroundAndAttitude` 0x499a70 | `.sim` files | `vehicle_ground.rs` |
| `camera-*.tsv` | `replay-camera.py` | chase, heli, bumper and cockpit cameras (0x461a30–0x464bf0) | `GameData/CarModel/*.tpg` | `original_camera.rs` |
| `engine-audio-*.tsv` | `replay-engine-audio.py` | `EngineAudio_PushState` 0x4ac780, `EngineAudio_Mix` 0x4ac290 | engine banks in `GameData/Sounds` | `original_engine_audio.rs` |
| `ai-track-graph.tsv`, `ai-track-query.tsv` | `replay-ai-track.py`, `replay-ai-query.py` | track node graph (`TrackNodes_Build` 0x480140, `.jnc` loader 0x481080) and queries | `GameData/Track/*.crp`, `*.jnc` | `original_ai_track.rs` |
| `ai-ais.tsv` | `replay-ai-ais.py` | `Ais_Load` 0x49ca30 | `GameData/Simulation/AICarData/*.ais` | `original_ai_ais.rs` |
| `ai-planner.tsv` | `replay-ai-planner.py` | AI planners in `Car_ReadInputs` 0x4991e0 | tracks and AI car data | `original_ai_planner.rs` |
| `ai-loop.tsv` | `replay-ai-loop.py` | closed-loop AI runs | tracks and AI car data | `original_ai_loop.rs` |
| `support-*.tsv` | `replay-road-support.py` | road support and `Car_TrackLocate` 0x49af70 | type-1 support polygons of four tracks, dumped first by the `support_dump` example | `original_road_support.rs` |

The recordings that stay in the repository (`tests/fixtures/car-pair-*.tsv`,
`tests/fixtures/tyre-audio-steps.tsv`) use synthetic inputs only.

## Adding a recording

A recorder that reads game files writes to `local/fixtures/`. Add it to
`record-local-fixtures.sh`, put its hash in `local-fixtures.sha256` and its name in
`.gitignore`. Load it in tests with `tests/support/local_fixtures.rs` and
`skip_without_local_fixtures!`.
