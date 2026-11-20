# Lessons: CLO-943 README docs for the `--json` `message` field

Durable rules from documenting the single-commit preview envelope
(`docs/specs/2026-11-19-clo-943-json-message.md`).

---

## L1 - "Grouping falls back to a single commit" has four triggers, and `--plan-only` beats `--dry-run` without `--all`

**Source incident**: CLO-943 spec, owner review round 1. The draft spec tied the
fallback to one provider request and let `--dry-run` decide alone; both were wrong.

**Rule**: `run_fallback` runs on an invalid cached plan (`src/main.rs:494-511`), a
prompt-budget refusal (`src/main.rs:621`), a provider error classed as fallback
(`src/main.rs:689-695`), and a plan that fails validation (`src/main.rs:699`). The
first two run before any grouping request, so the request count depends on the
trigger. In `single_commit_path`, `--plan-only` without `--all` returns the `plan`
envelope before the `--dry-run` check (`src/main.rs:917`), so no `message` is generated.

**How to apply**: When docs or specs describe single-commit envelopes, name the case
"when grouping falls back to a single commit", state no request count, and qualify
`--dry-run` claims with "without `--plan-only`".

---

## L2 - Judge `scripts/acceptance.sh` by the failure set against a same-environment baseline

**Source incident**: CLO-943 implement phase. On this Mac the suite gave PASS=294
FAIL=1 (AC-488-1, fixed later by CLO-944) on base and revision. The Codex sandbox,
same code, failed AC-5, AC-488-1 and AC-493-7 (`OnboardingRequired` instead of
`Provider` without a configured provider, `scripts/acceptance.sh:1204`).

**Rule**: The absolute failure count of the suite depends on the environment. A change
is clean when the failure set on the revision equals the failure set on the base, with
the same binary and environment.

**How to apply**: Build the release binary once, record its `shasum`, run the suite on
the base and on the revision, and `diff` the `FAIL` lines.
