# Run431: reviewed experimental x86 lowering (PR24)

[PR24](https://github.com/xfynx/prsche-disasm/pull/24), reviewed head
`747c49534cf128bb6c105254910ec4bc69e21fd2`, adds a separate backend inheriting the
unchanged SHA-checked FPU/277/257 chain. It improves integer flags, branches,
bit-string addressing, DIV8, string direction and LOCK32. This is research
infrastructure for generating and checking C++ candidates, outside the game graph.
Original Porsche.exe SHA256: `ddd748fdbe6d2030e31f9257a4e01852749460b6b58560a6b4a8559d3799ff39`.

## Maintainer corrections and acceptance (2026-10-24)

The submitted verifier could report unexecuted cases as PASS and return success
after native failure. Corrected verification requires the exact ordered form list,
actual completed totals, normal native exit and no untranslated forms. Faults,
unexpected stops, empty input and exhausted retries fail. Attempts are fresh;
committed receipts are never overwritten. Both runners retain commands and hashes.

New unsupported widths are rejected: BT/BTS16, LOCK8/16 and SHLD/SHRD16. DIV8
checks zero/overflow before register writes and stops with `UnsupportedInput`;
this does not emulate guest #DE. Valid DIV8 and the rejected inputs are compared
with native execution. Six unsupported-width probes must reject.

On AMD Ryzen7 5700X/MSVC x86, final results are integer961/288300 cases,
SSE172/51600, branches16/4800, strings14/4200: **348900 comparisons PASS**,
plus **six DIV8 edges**. All forms completed300 cases. Division sampling explicitly
uses nonfaulting inputs and records rejected samples; it does not hide a count gap.
LOCK flags/memory200000 cases PASS; two threads produce exactly4000000 increments,
while the non-atomic control produces2470152. Four complete mutation runs detect
571 PF,224 AF,1 rotate-OF and7 DF failing forms. Mutation verification exits0 only
when an actual mismatch is detected in a complete run; crashes cannot count.
Injected lowered crash/stop checks both reject with0 completed cases and exit1.
Zero-case/existing-output checks reject; six malformed LOCK outputs reject.

[maintainer-validation.json](maintainer-validation.json) pins final tested sources,
receipts, generated artifacts, commands and counts. Full attempts are under
`local/reports/pr-review-20261024/final-*-001/`. The unchanged `*-oracle.json` files
in this run are the contributor's historical Intel i3-3220 receipts, not new
acceptance of their totals or of universal undefined-flag behavior.

## Commands

```powershell
py -3 scripts/research/verify-v2-x86-exact.py --set int --cases 300
py -3 scripts/research/verify-v2-x86-exact.py --set sse --cases 300
py -3 scripts/research/verify-v2-x86-exact.py --set branch --cases 300
py -3 scripts/research/verify-v2-x86-exact.py --set string --cases 300
py -3 scripts/research/verify-v2-x86-exact.py --set int --mutate no-parity
py -3 scripts/research/verify-v2-x86-exact.py --set int --mutate no-af
py -3 scripts/research/verify-v2-x86-exact.py --set int --mutate rot-of
py -3 scripts/research/verify-v2-x86-exact.py --set string --mutate no-df
py -3 scripts/research/verify-v2-x86-exact-lock.py
```

Automatic attempts are unique; `--output-dir` must be fresh. `--vc-root` can select
an MSVC version directory; otherwise local BuildTools/installed VS are discovered.
The default flag policy compares defined/preserved bits. `--flags cpu-profile`
also compares undefined bits and may fail on another CPU; Intel behavior is not
an invariant of the original game's era. These tests require MSVC x86, Capstone,
the local original PE and indexed catalog.

x87 remains the limited inherited binary64 model; existing80-bit/FSQRT rejections
remain. General traps, precision, MMX/x87 aliasing, alignment faults and non-temporal
ordering remain outside acceptance. LOCK32 uses MSVC-compatible intrinsics and
identity-mapped host memory, not a general guest-memory runtime. Source comments
saying portable/exact must be read within these bounds. Generated candidates still
need original ABI, ownership, consumers and native proof before game intake.
Game remains Run388:211TUs,194 unresolved names/201refs,noEXE.
