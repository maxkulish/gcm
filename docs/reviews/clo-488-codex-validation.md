## Verdict: FAIL

## Findings
- HIGH — [src/plan.rs](/Users/mk/Code/gcm--feat-clo-488-typed-errors/src/plan.rs:70), [src/plan.rs](/Users/mk/Code/gcm--feat-clo-488-typed-errors/src/plan.rs:174): `parse_defensive` still cannot recover a response that is just the bare `groups` array, e.g. `[{"files":[...],"summary":"...","commit_message":"..."}]`. `recover_groups` only returns an array found behind a `groups` key, so the whole-array candidate falls through to `PlanError::Parse`. That misses one of the parser hardening cases called out in the review/spec (“re-wrap a bare groups array as {"groups": ...}”) and will incorrectly reject otherwise recoverable model output.
- MEDIUM — [src/groq.rs](/Users/mk/Code/gcm--feat-clo-488-typed-errors/src/groq.rs:332): the bounded non-2xx body read is implemented with `BodyWithConfig::limit(...).read_to_string().unwrap_or_default()`. In `ureq`, hitting the limit raises an error rather than returning the first N bytes, so any error body at or above the cap is dropped entirely here. The request stays bounded, but the 400 detail extraction no longer works in the exact case the cap is supposed to protect, so `BadRequest.detail` regresses to `None` instead of a capped best-effort message.
- LOW — [src/groq.rs](/Users/mk/Code/gcm--feat-clo-488-typed-errors/src/groq.rs:216): `GCM_RETRY_MAX` is parsed as `u64` and then cast with `as u32`. Values above `u32::MAX` wrap instead of being rejected/clamped/defaulted, so an out-of-range env var can silently change retry behavior (`4294967296` becomes `0` retries). That is not the “invalid -> default” behavior the spec describes for retry knobs.

## Missing Items
- [src/plan.rs](/Users/mk/Code/gcm--feat-clo-488-typed-errors/src/plan.rs:389): the test named `parse_defensive_recovers_and_rewraps_groups_array` does not cover the actual top-level bare-array case; it only covers `{"data":{"groups":[...]}}`.
- [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-488-typed-errors/scripts/acceptance.sh:529): AC-3 is only exercised for HTTP 401, not 403.
- [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-488-typed-errors/scripts/acceptance.sh:548): AC-8 only checks final `BadRequest` debug output. There is no test that retry attempts themselves are logged, or that `RateLimit` / `Server` variants are visible during transient retries.
- [src/groq.rs](/Users/mk/Code/gcm--feat-clo-488-typed-errors/src/groq.rs:332): there is no unit/acceptance coverage for the 4096-byte error-body cap path, even though the spec’s eval table calls for a >4096-byte case.
- I did not execute `cargo test` or `scripts/acceptance.sh` in this read-only environment, so this review is from code inspection only.

## Recommendations
- Teach `parse_defensive` to treat `Value::Array` as a recoverable top-level groups payload and re-wrap it as `{"groups": arr}` before deserializing `Plan`.
- Replace the `ureq` `limit(...).read_to_string().unwrap_or_default()` path with a true bounded reader that preserves the first 4096 bytes, then extract/truncate detail from that buffer.
- Parse `GCM_RETRY_MAX` directly as `u32` or clamp explicitly before storing it.
- Add tests for bare-array plan recovery, 403 auth failure, retry debug lines, and error bodies at `4096` and `>4096` bytes.