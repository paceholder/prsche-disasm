# PR #25 integration review

Reviewed author head: `d31792977c65880f3df0be820d6b2729e53ee8bb`,
[PR #25](https://github.com/xfynx/prsche-disasm/pull/25), by
[paceholder](https://github.com/paceholder). Integrated against recovery checkpoint
`e92fbbb`. This updates the experimental Rust iteration 012; it does not promote
Rust algorithms into accepted C++ recovery or close the full-game milestone.

## Corrections during intake

- The missing-graph/AIS fallback now converts its existing 1250 kg AI estimate to
  the original player's `kg / 128` collision units, including inverse mass and
  inertia. The normal original-AI path already agreed. A synthetic mixed-mode
  regression checks equal separation and central-impulse sharing.
- FFN line spacing is the signed sum of header bytes `+0x12/+0x13`, as loaded at
  `0053c70a..0053c71a` and consumed at `0053db6b..0053db77`. Reading only the second
  byte returned zero for our version-100 fonts. Synthetic signed-metric coverage
  replaces assertions tied to another version's artwork; installed fonts parse.
- HUD tests now locate the repository's `local/game` through a portable file URL;
  both asset checks execute instead of using another checkout layout.
- Author attribution and contribution scope are recorded in [LICENSING.md](../../LICENSING.md).

## Evidence and validation

The supplied recorder script generated all 33 local fixture files. After LF
normalization, 32 match the author's manifest exactly. The remaining
`vehicle-sim.tsv` difference is the previously identified `928coupeGTS54.sim`
input-asset revision. Regenerated fixtures use our indexed original binary and
our assets; no game-derived recordings are added to Git.

The real-road suite passes 18,520 owner queries and 2,368 closed-loop steps across
Autobahn, Alps, Monaco 1 and Canyon. Synthetic car-pair fixtures and recorder are
unchanged from the earlier independent 424-contact/420-impulse reproduction.
These are controlled Unicorn comparisons, not complete native-game acceptance.

Native viewer and WASM workspace checks pass. Rust package suites pass after
focused corrections/reruns; three existing GPU/resource diagnostics remain
ignored. All 21 web checks pass, including both installed-HUD checks. Snapshot
isolation/overwrite protection passes. Unchanged passed comparisons were reused.

Speech is incomplete for our `zzzxeng.viv` revision (SHA256
`f89ab2839d14515a170e3dc2dfff37cff2b3df1be3dfcec6f50b8a3fd52cd64a`).
The strict parser rejects 130 distinct range-overrun layouts and 25 signed negative
counts; five distinct referenced banks decode. The test checks these exact
failures only under that archive's byte fingerprint, and requires every phrase
in other revisions to decode. This is negative coverage, not restored speech.
Original tag signedness and bank-relative offsets were confirmed; preparation
remains unproved. No padding, offset subtraction or guessed decoder change was
introduced. The [validation backlog](../recovery-validation-backlog.md) owns the
next trace and the remaining graceful-reporting runtime check.

The browser WASM build ran Alps Quick Race with three original-AI opponents.
Player telemetry reached 118 km/h; AI advanced, chase camera and HUD rendered.
This smoke run does not prove full visual fidelity or career completion.

Commands, logs, hashes and screenshots are retained in the ignored
`local/reports/pr25-ready-review-20261025/` directory. Asset-derived fixtures stay
under `local/experiments/pr25-ready/local/fixtures/`.

Repeatable checks from `iterations/012-campaign-fidelity`, with generated fixtures
in repository `local/fixtures` or selected by `NFS_LOCAL_FIXTURES`:

```sh
cargo test --locked --offline --workspace -- --nocapture
cargo check --locked --offline --workspace --bin porsche-viewer
cargo check --locked --offline --workspace --target wasm32-unknown-unknown
node --test web/*.test.mjs
```

Final local acceptance uses the recorded workspace run plus focused package,
road-support and speech reruns; earlier failed logs remain labelled as failures.

## Use in C++ recovery

Use the [evidence map](../../research/binary-index/pr25-evidence.json) to locate
recorders, original VAs, field mappings and intermediate-state comparisons.
Verify the exact original body and its consumers before each C++ intake. Keep
the existing native harnesses and canonical owners; do not translate the Rust
implementation wholesale or treat approximate adapters as recovered algorithms.

Rollover/roof contact, damage, parts, complete careers, full front-end coverage
and native visual acceptance remain open. The documented kinematic fallback,
rendering approximations and controlled recorder stubs remain explicit limits.
