# Lessons: CLO-493 Add automation surface: JSON, non-interactive flags, structured logging

## L1 - Keep automation JSON mode stdout strictly machine-readable

**Source incident:** `docs/status/clo-493-workflow.yaml:211-213`, history action `codex_validation_complete` in `CLO-493`.

**Rule:** For CLI paths with `--json` / non-interactive behavior, interactive prompts and human-facing diagnostics must stay on stderr; only the JSON envelope should be emitted on stdout, including early-exit/error cases.

**How to apply:**
- When adding automation-oriented output modes, add acceptance checks that reject non-JSON stdout noise in both success and failure flows.
- Route prompt text (`--yes` fallback), reset/cache status, and mode warnings explicitly to stderr unless a command explicitly documents mixed-mode output.