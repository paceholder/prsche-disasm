# Experimental x86 lowering tool

[Run431/PR24](../../iterations/v2/001-original-recovery/runs/431-x86-backend-exact/README.md)
adds a separately reviewed integer-flags/DF/LOCK32 backend and fail-closed
verifiers. Frozen PR20 sources and their proofs below remain unchanged.

Imported from PR #20, head 1bfa0145f7dcc78f4f525cf6046694bb48888cd1, with maintainer corrections from review. This is a research and differential-checking tool. It is not connected to the production game build and does not replace canonical original-function recovery.

The extension inherits the frozen Run277 backend with a checked SHA. It adds integer operations, a limited x87 model, SSE register/MXCSR handling and MMX operations. Unsupported instructions raise an error instead of becoming no-ops. Generated C++ still needs ABI, memory, dependency and original-behavior validation before any production use.

## Accepted corrections

- SSE/MMX effects replace the no-op lowering rejected in PR #2. SSE arithmetic and conversion helpers execute host instructions under the modeled MXCSR and retain sticky flags for the tested masked environments.
- Unordered FCOM sets C0/C2/C3 and invalid-operation IE for the tested quiet-NaN cases.
- FABS clears the binary64 sign bit, including negative zero and signed quiet NaNs.
- FLD/FST/FSTP memory operands must be 4 or 8 bytes. Ten-byte x87 values are explicitly rejected instead of silently truncated.
- FSQRT is explicitly rejected. The submitted implementation fixed the result for negative inputs and signed zero, but did not propagate guest exception status/control-word behavior. A correct value alone was insufficient.
- The native verifier reports supported and expected-unsupported fragments separately, derives the skipped index from the fragment list, fails on an unexpected case count, and preserves fresh attempt directories.

## Verified scope

The original PR fixture passed 108 fragments and 110592 cases. After the fail-closed FSQRT correction, the revised fixture passes **107 supported fragments / 109568 cases**, with FSQRT recorded as one expected rejection. This is bounded input coverage, not complete instruction or x87 correctness.

The edge regression executes generated FABS and FCOMPP/FNSTSW code against physical x86 results for 14 cases, including negative zero, signed quiet NaNs, unordered/equal/ordered comparisons and observable status. It also requires rejection of ten-byte FLD/FSTP and FSQRT. Earlier focused failure receipts are preserved in the local PR review experiment.

`powershell
py -3 scripts/research/verify-v2-x86-fpu.py --json
py -3 scripts/research/verify-v2-x86-fpu-edge.py --attempt 002
py -3 scripts/research/census-v2-x86-global.py --json
`

The edge command needs an unused attempt ID. The main native verifier automatically creates a new attempt directory, or accepts --output-dir with a fresh explicit path. The census measures only whether translation throws; its percentages do not mean the generated bodies compile, link or execute. It excludes thunks and multi-range functions and is not a complete-binary behavior audit. Run252 frontier census is another syntax check and may exit nonzero for intentionally rejected instructions.

## Explicit limitations

- x87 registers use binary64, not original 80-bit extended precision. General exception propagation, guest x87 rounding/control-word execution, stack faults, tags and unmasked traps are not fully modeled. The passing cases do not authorize arbitrary x87 recovery.
- MMX registers do not alias the x87 register array; the x87 tag word is absent. TOP behavior is checked for the tested MMX fragments. Mixed x87/MMX programs need separate validation.
- Alignment faults, reserved LDMXCSR faults, unmasked SIMD traps and the hardware memory-order/cache effects of non-temporal stores are not emulated.
- 3DNow!, reciprocal approximation and x87 environment save/restore remain rejected. An unknown instruction must not be accepted by silently dropping results.
- The six lowering mutations described in the upstream PR were not supplied as a repeatable negative-control harness. They are not counted as independently reproduced acceptance evidence.
- Hardware-oracle output does not prove all generated bodies, callbacks, resource data, game state, visual output or gameplay.

Use this tool to inspect candidates and construct differential experiments. Keep the original PE/index and native acceptance as the source of truth. See [the PR20 acceptance report](pr20-acceptance-2026-10-20.md) for pinned inputs and final results.
