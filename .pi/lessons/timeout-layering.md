# Lessons: Timeout layering and effective deadlines

Durable rules from implementing layered step/backend/global timeouts.

---

## L1 - Backend client timeouts must not preempt `StepContext.timeout`

**Source incident:** CLO-384 pre-PR validation (`docs/reviews/clo-384-codex-validation.md`) found that `src/backend/ollama.rs` still built a reqwest client with a config/default timeout while the workflow wrapper enforced the resolved per-step `StepContext.timeout`. A long step timeout could therefore lose to a shorter backend-internal HTTP deadline before the authoritative outer timeout fired.

**Rule:** Once `StepContext.timeout` is the effective deadline for a backend call, backend transport/process timeouts may not enforce an independent shorter deadline. The wrapper around `backend.query(ctx)` is the source of truth unless the backend timeout is explicitly derived from `ctx.timeout`.

**How to apply:** When changing timeout resolution, grep backend constructors and query paths for `timeout`, `Client::builder`, `tokio::time::timeout`, `kill_on_drop`, and command lifecycle settings. Either remove lower-layer deadlines, derive them from `ctx.timeout`, or document a deliberate exception with a regression test proving the exception cannot preempt the step deadline.

---

## L2 - Timeout refactors need a whole-runner grep gate

**Source incident:** CLO-384 pre-PR validation (`docs/reviews/clo-384-codex-validation.md`) found duplicate timeout calculations in `src/workflow.rs` after the LLM query path had already been refactored. Shell steps, format, verification, synthesis, and workflow validation still used legacy `step.timeout` / `workflow.timeout` logic that contradicted the design.

**Rule:** A timeout helper is not single-source-of-truth until every step-scoped execution path either calls it or is explicitly classified as legacy/out-of-scope. Unit tests around the primary backend query path are not enough.

**How to apply:** Before validation, run a grep audit for `timeout`, `step.timeout`, `workflow.timeout`, `DEFAULT_TIMEOUT`, and `tokio::time::timeout` across `src/workflow.rs` and `src/backend/**`. Write down each remaining occurrence as one of: helper implementation, caller using `ctx.timeout`, intentional non-step timeout, test, or bug. Add regression coverage for any non-LLM step paths (shell/format/verify) that are in scope.
