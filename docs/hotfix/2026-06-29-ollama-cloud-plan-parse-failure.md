# Hotfix: Ollama cloud models fail commit-plan parsing

- Date: 2026-06-29
- Affected component: `src/provider/ollama.rs`, `src/provider/mod.rs` (grouping prompt), `src/plan.rs` (defensive parser)
- Severity: Medium (degraded behavior, not data loss). Multi-commit grouping silently falls back to single-commit mode.
- Reported by: manual testing with `nemotron-3-nano:30b-cloud`

## Symptom

Running `gcm` against an Ollama cloud model produced this on stderr, then a (correct) single commit:

```
gcm: could not parse the Ollama response: plan parse error: could not extract a commit plan from the response. Falling back to single-commit mode.
```

The single-commit message was good, so the basic message path worked. Only the multi-commit grouping path failed.

## Environment

- Provider: Ollama, model `nemotron-3-nano:30b-cloud`
- Ollama version: 0.30.11
- The model is a **cloud passthrough** model (the `:cloud` / `-cloud` tag), proxied by Ollama to a remote backend rather than run locally as a GGUF.

## Root cause

gcm's grouping path requests structured output by sending Ollama's `format` field with a JSON schema (`build_plan_payload` in `src/provider/ollama.rs:188`, schema from `src/plan.rs:229`). Ollama enforces that schema through grammar-constrained decoding **only for local GGUF models**. For cloud passthrough models the `format` field is effectively a no-op: the remote model never receives the schema and is not constrained by it.

The grouping system prompt (`GROUPING_SYSTEM_PROMPT`, `src/provider/mod.rs:341`) names the fields (`groups[0]`, `summary`, `commit_message`) but never states the actual JSON shape or gives an example. The complete schema lived only in the `format` field. So when the cloud model did not see that field, it had to guess the wrapper shape and guessed wrong.

The result was syntactically valid JSON in a model-invented shape:

```json
{ "commits": [ { "message": "...", "files": ["..."] } ] }
```

instead of the required shape:

```json
{ "groups": [ { "files": ["..."], "summary": "...", "commit_message": "..." } ] }
```

`parse_defensive` / `recover_groups` (`src/plan.rs:80`) only recognize a top-level `groups` key, so no candidate yielded a `Plan`, producing `PlanError::Parse("could not extract a commit plan from the response")` and the announced single-commit fallback (`src/main.rs:770`).

The single-commit (message) path worked because `build_message_payload` sends no `format` field and needs only free-form text.

## Evidence

### 1. Reproduction with the current payload

Sending the plan request (with `format` schema) to the model returned the wrong shape, and the model's own `thinking` confirmed it never saw a schema:

```
message.content -> { "commits": [ { "message": ..., "files": [...] } ] }
message.thinking -> "We need to infer the schema. Not given explicitly... we need to guess."
```

### 2. Fix verification

Re-sending the same request with the explicit shape and a short example added to the system prompt produced the correct shape, which `parse_defensive` accepts:

```json
{
  "groups": [
    { "files": ["src/auth.rs", "src/auth_test.rs"], "summary": "Implement login functionality", "commit_message": "feat(auth): add login functionality and tests" },
    { "files": ["README.md"], "summary": "docs: update README", "commit_message": null }
  ]
}
```

## Fix

### Primary (verified)

Embed the explicit JSON shape and a small example in `GROUPING_SYSTEM_PROMPT` (`src/provider/mod.rs:341`), so the prompt itself fully specifies the output even when the provider does not enforce `format`. This is belt-and-suspenders with the existing `format` field and benefits every provider/model that ignores or weakly honors structured output, not just Ollama cloud models.

The prompt must state:
- The top-level key MUST be `groups` (an array). Do not use `commits`.
- Each group has `files` (array of exact paths), `summary` (string), and `commit_message` (full conventional commit on `groups[0]` only, `null` for the rest).
- Include a short literal example object.

### Defense-in-depth (optional)

Make `recover_groups` in `src/plan.rs` tolerant of the common alias shape so a stray-key response still parses:
- top-level `commits` treated as `groups`
- per-group `message` treated as `commit_message`

This keeps the strict schema as the primary contract while recovering from models that emit a near-miss shape.

## Acceptance criteria

- [ ] `gcm` against `nemotron-3-nano:30b-cloud` produces a multi-commit plan instead of falling back to single-commit.
- [ ] The grouping prompt fully specifies the `groups` shape with an example.
- [ ] Existing local-model behavior is unchanged (the `format` field is still sent).
- [ ] Unit test: a `commits`/`message` shaped response is handled (recovered or covered by the prompt contract) without a Parse error, if the defense-in-depth change is included.

## Notes

- Consider a one-line note in docs that Ollama cloud (`:cloud` / `-cloud`) models do not enforce `format`, so structured-output features rely on prompt-level schema.
