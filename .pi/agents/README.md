# Persona routing - gcm `.pi` agents

This directory holds reviewer / drafter personas dispatched by the
orchestrator phase scripts. Each persona is a Markdown file that:

- describes the persona's job, scope, and output format
- declares what is in-scope and (importantly) what is out-of-scope so
  reviewers don't fight over the same finding
- ends with a canonical verdict string parsed by the orchestrator
  (`approve | approve_with_changes | rework`) where applicable

## When personas are invoked

Personas are not called ad-hoc. They are dispatched by the phase
scripts under `.pi/orchestrator/phases/`. The dispatching script owns
the input contract (what context the persona receives) and the
expected output path. Phase scripts are the source of truth; this
table is a routing aid.

| Phase                   | Persona                 | Default model               | Role            | Output                                              |
|-------------------------|-------------------------|-----------------------------|-----------------|-----------------------------------------------------|
| `design`                | `claude-designer.md`    | Claude Opus 4.x             | Drafter         | `docs/designs/clo-XX-<slug>.md`                     |
| `design`                | `gemini-architect.md`   | `gemini-3.5-flash`    | Design reviewer | `docs/reviews/clo-XX-design-gemini.md`              |
| `spec`                  | `gemini-architect.md`   | `gemini-3.5-flash`    | Spec reviewer   | `docs/reviews/clo-XX-spec-gemini.md`                |
| `spec`                  | `ollama-rust-reviewer.md` | local Ollama (Qwen / similar) | Spec reviewer (footguns) | `docs/reviews/clo-XX-spec-ollama.md`        |
| `implement` (step 4)    | `codex-pre-pr.md`       | `gpt-5.5` (Codex)           | Validation gate | `docs/reviews/clo-XX-codex-validation.md`           |
| `implement` (step 4)    | `gemini-architect.md`   | `gemini-3.5-flash`    | Validation gate | `docs/reviews/clo-XX-gemini-validation.md`          |
| `implement` (step 4)    | `security-reviewer.md`  | Claude Opus 4.x             | Conditional - LLM backend / secret handling changes | inline in validation synthesis |
| `implement` (step 4)    | `ops-reviewer.md`       | Claude Opus 4.x             | Conditional - rarely (gcm is a CLI; only applies to deploy / packaging changes) | inline in validation synthesis |

The model column lists the *default*. Phase scripts and the
underlying tooling (`lok workflow run …`, `pi run …`) can override
via environment variables - see the relevant phase doc.

## Conditional dispatch by module

`security-reviewer.md` and `ops-reviewer.md` are NOT part of the
default gcm pipeline. gcm is a CLI orchestrator for LLM agents, not a
networked service, so the universal codex + gemini gate covers the
common case. Dispatch them only when the touched paths below fire:

| Touched path                         | Run security-reviewer | Run ops-reviewer  |
|--------------------------------------|-----------------------|-------------------|
| `src/backend/**` (LLM provider clients, API key handling) | yes | no |
| `src/config.rs` (credential / env-var surface) | yes | no |
| `src/conductor.rs`, `src/workflow.rs` | no  | no |
| `src/apply_verify/**`, `src/tasks/**`, `src/role/**` | no | no |
| Release packaging / install scripts  | no | yes |
| CI workflow files under `.github/`   | no | yes (only if release-relevant) |

Rule of thumb: if a change touches LLM API key handling, request
construction to external providers, or credential storage, dispatch
`security-reviewer`. If it touches how gcm is built, packaged, or
released, dispatch `ops-reviewer`. Most gcm changes need neither.

## Persona scope boundaries

Each persona explicitly disclaims topics handled by another persona.
The goal is non-overlapping finding domains:

| Topic                              | Owned by                |
|------------------------------------|-------------------------|
| Design fidelity                    | `gemini-architect.md`   |
| Rust idioms, lifetimes, generics   | `gemini-architect.md`   |
| Mechanical Rust footguns           | `ollama-rust-reviewer.md` |
| Test coverage of happy paths       | `codex-pre-pr.md`       |
| LLM API keys, provider secrets     | `security-reviewer.md`  |
| Prompt-injection risk in agent IO  | `security-reviewer.md`  |
| Release packaging, install paths   | `ops-reviewer.md`       |

If a finding could plausibly belong to two personas, prefer the one
listed in the persona's review-focus list. When in doubt, the
validation synthesis step (in `implement.md`) deduplicates.

## Verdict contract

Every reviewer persona ends with a verdict line of the form:

```
PASS | PASS_WITH_NOTES | FAIL
```

Legacy synonyms remain accepted for backward compatibility with older
workflow YAMLs and `PHASE_CONFIG` strings: `approve` (=PASS),
`approve_with_changes` (=PASS_WITH_NOTES), `rework` (=FAIL). Prefer
the uppercase form for new reviews to match
`.lok/workflows/pre-pr-validation.toml`.

The orchestrator parses this verbatim. Personas that deviate from
this format will silently fail the synthesis step. The drafter
persona (`claude-designer.md`) is an exception - it produces a draft
document, not a verdict.

## Adding a new persona

1. Copy an existing reviewer persona as a template (`codex-pre-pr.md`
   for pre-PR-style validators, `gemini-architect.md` for design
   reviewers).
2. Declare scope and out-of-scope explicitly. Out-of-scope must
   reference the persona that *does* own the topic.
3. Keep the verdict line format identical.
4. Add a row to the routing table above AND wire the dispatch into
   the relevant phase script under `.pi/orchestrator/phases/`. A
   persona file with no caller is dead code.
5. If the persona introduces a new output path (e.g.
   `docs/reviews/clo-XX-<persona>.md`), document it in the phase's
   "Required exit state" section, mirror it into `PHASE_CONFIG` if
   the orchestrator should gate on it, and re-run
   `node .pi/scripts/check-schema-parity.mjs`.

## Confidentiality

No persona is allowed to paste customer prompts, Linear ticket
bodies, or vault content into its review output. Reference file:line
and ticket IDs only. This rule lives in each persona's "Hard rules"
section.
