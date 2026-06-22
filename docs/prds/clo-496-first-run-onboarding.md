# PRD: First-Run Onboarding Wizard (CLO-496)

**Linear Task**: [CLO-496](https://linear.app/cloud-ai/issue/CLO-496)
**Status**: Draft (discovery phase)
**Covers**: FR-40, FR-53, FR-54, FR-55

---

## Problem

`gcm` currently expects users to manually export provider API keys and set `GCM_PROVIDER` before the first run. There is no persistent config file and no guided setup, so a first-time user hits a fatal `ProviderError::MissingKey` or has to piece together env vars from `--help`.

## Goal

On first run with no config, guide the user through activating providers, capturing keys or a local endpoint, choosing a default, and persisting the configuration. Re-running the wizard (via `gcm config` or `--reconfigure`) must update choices idempotently without corrupting existing config.

## Requirements

1. Detect no-config / no-provider and launch an interactive wizard.
2. In a non-TTY context, print the needed configuration and exit non-zero.
3. Offer the five v1 providers: Groq, Gemini, Anthropic, OpenAI, and local Ollama.
4. Allow enabling one or more providers.
5. Capture each cloud key from env if present, else prompt securely (no echo).
6. Confirm the Ollama endpoint (default `http://localhost:11434`).
7. Let the user choose the default provider from the enabled set.
8. Persist config with `0600` permissions.
9. `gcm config` / `--reconfigure` re-runs the wizard idempotently.
10. Secrets must be `0600` or env-only; never world-readable; never committed.

## Acceptance Criteria

- A fresh user with no config runs `gcm`, completes the wizard, and lands a commit.
- Secrets are stored `0600` or referenced by env var; never world-readable, never committed.
- Re-running the wizard updates choices without corrupting existing config.

## Out of Scope

- Cloud key validation (pinging each provider) — not required for v1.
- Encrypted at-rest storage — permissions-only for now.
- Automatic alias installation (`git commit-ai`) — covered by CLO-497.

## Risks

- Prompting in a non-TTY CI context must degrade cleanly.
- Existing `--provider` / `--model` / env precedence must remain intact.
- Ollama endpoint confirmation must not block headless usage.
