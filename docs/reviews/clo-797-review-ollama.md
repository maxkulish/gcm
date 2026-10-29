# Design Review: clo-797

**Reviewer**: Codex via Ollama (glm-5.3:cloud)
**Reviewed**: 2026-09-17
**Design Document**: docs/designs/clo-797-grouping-prompt-budget.md
**Pipeline**: reviewer invoked directly; the lok design-review workflow aborts at its synthesis step (see docs/status/clo-797-workflow.yaml)

---

# Design review: CLO-797 grouping prompt budget

## Verdict

**NEEDS_REVISION**

The diagnosis is excellent and the core direction is right: deduplicate the path inventory, remove pointless untracked stubs, budget the assembled prompt instead of `body`, and derive the budget from the model window. D1 and D2 alone appear sufficient for the measured 515-file reproduction.

However, the document currently contains internal contradictions between its acceptance criteria and the proposed degradation algorithm, and two “open questions” are actually implementation-critical parameters. I would not start implementation until those are resolved.

---

## Strengths

- The root-cause analysis is unusually strong: it identifies both the redundant path copies and the fact that `cap_total` only bounds `body`.
- The correction that all providers share `grouping_user_content` is important and materially reduces the implementation scope.
- Keeping the `Provider` trait unchanged is the right call.
- D1’s merged `{path, xy}` inventory is a clean replacement for `file_list` + `status`.
- D2 correctly recognizes that past-the-cap untracked stubs are redundant when every path is already present in the inventory.
- D3’s move to budget the assembled prompt is the central architectural fix.
- D5’s typed pre-flight error is much better than relying on a provider HTTP 400.
- The test plan is mostly concrete and includes the critical assembled-prompt regression test.
- The public-surface analysis in Q1 is a good example of checking migration cost before designing around it.

---

## Blocking findings

### 1. AC2 is not implementable as written

AC2 says:

> no path appears more than once in the grouping prompt.

Test 1 then says:

> every path appears exactly once across all sections of the assembled prompt.

That cannot be true if `body` contains normal diff content. Git diff bodies contain path headers such as:

```text
diff --git a/src/diff.rs b/src/diff.rs
```

Even the D3 header-only degradation explicitly preserves per-file headers. Therefore, even after D1 and D2, changed paths will still naturally appear in both the merged `files` inventory and the diff headers in `body`.

This is not a minor wording issue: the proposed regression test would fail on any fixture with real diff content.

**Required change**

Pick one of two designs:

1. **Recommended:** redefine AC2 as “there is exactly one path inventory; no redundant path-list or stub copy duplicates it.” Allow `body` to contain paths as part of actual diff headers. The test should count occurrences in the inventory and generated omission stubs, not literal occurrences across the entire rendered prompt.
2. If AC2 really means “each path occurs exactly once in the entire prompt,” then `body` must reference files by index rather than include path headers. That is a substantially larger design change and should be stated explicitly.

As written, AC2 and test 1 do not match the proposed prompt structure.

---

### 2. AC4 contradicts D3’s degradation ladder

AC4 says:

> a working tree whose diff alone exceeds the window produces an actionable gcm error, not a provider HTTP 400.

But D3 says:

1. shrink `body`;
2. drop `body` entirely, keeping `files` + `stat`;
3. only fail if `files` + `stat` exceed the budget.

That means a tree whose original diff body exceeds the window will not necessarily produce an error. If `files` and `stat` fit, D3 drops the diff body entirely and still sends a grouping request based only on filenames and statuses.

That directly conflicts with AC4 and with the problem statement’s statement that a repo whose diff body exceeds the window should hard-error.

**Required change**

Choose the intended behavior explicitly:

- If grouping without diff content is unacceptable, make “body would be fully dropped” a pre-flight failure. D3 should then stop at header-only degradation, and full body removal should trigger D5.
- If filename/status-only grouping is considered useful, revise AC4 and clearly define it as an explicitly announced degraded mode. But then the design must say what quality guarantees, if any, remain.

I would lean toward failing rather than sending a grouping prompt with no diff content, because semantic grouping with only paths is unlikely to meet user expectations and AC4 currently promises an error.

---

### 3. The single-commit non-goal conflicts with D5

The non-goals say:

> Changing the single-commit / message path's budget behaviour (CLO-486 O3 parity).

But D5 says:

> The single-commit prompt must get the same pre-flight check.

Adding a pre-flight error to the single-commit path is a behavior change, even if payload assembly and truncation remain unchanged.

This needs to be reworded so the scope is unambiguous. For example:

- Non-goal: changing how the single-commit prompt is gathered or truncated.
- In scope: adding a shared pre-flight context-window check to both grouping and single-commit calls.

Without that distinction, implementers and reviewers will reasonably disagree about whether the single-commit change belongs in this task.

---

### 4. Q2 is not an optional open question; `completion_reserve` is part of the safety invariant

The whole budget is:

```text
budget = window_tokens − completion_reserve − system_allowance
```

Leaving `completion_reserve` unresolved leaves the primary safety boundary unresolved. If it is too small, the provider can still return a context-window 400 or truncate the completion. If it is too large, gcm will unnecessarily degrade or reject prompts.

The document correctly notes that the plan currently echoes paths, but it does not define how much output space to reserve.

**Required change**

Specify a concrete reserve policy before implementation. At minimum it should include:

- fixed JSON/schema output overhead;
- reasoning-token allowance for reasoning models;
- path-echoing output estimate;
- provider maximum completion-token limits.

A reasonable shape is:

```text
completion_reserve =
    fixed_output_overhead
    + reasoning_allowance
    + min(provider_max_output_tokens, estimated_path_echo_output)
```

`estimated_path_echo_output` should be derived from the serialized path inventory, not merely file count, because path length varies enormously. A repo with 100 short paths and a repo with 100 very deep paths have very different output sizes.

Also check that the estimated completion fits the provider’s output-token cap. A prompt can fit the context window while the required completion still exceeds `max_tokens`; that is a separate failure mode and should produce a clear error or be explicitly deferred to CLO-801.

---

### 5. Q3 must be resolved because the single-commit path also needs the check

Q3 asks whether the pre-flight belongs in `gather_for_grouping` or `build_plan`. But D5 also requires the same check for the single-commit path, and that path is not owned by `build_plan`.

Putting policy only in `build_plan` solves grouping but leaves the message path with no natural owner. Putting it in `diff.rs` turns a gatherer into a policy layer.

**Recommended architecture**

Introduce a small shared budget-check service, for example:

```rust
struct PromptBudget {
    window_tokens: usize,
    completion_reserve: usize,
    system_allowance: usize,
}

fn ensure_prompt_fits(
    rendered_prompt: &str,
    budget: &PromptBudget,
) -> Result<(), GcmError>;
```

Call it from the orchestration layer before both provider calls:

- grouping plan call;
- single-commit message call;
- fallback message call.

Then:

- `diff.rs` remains a gatherer;
- `build_plan` decides whether to fall back;
- the same typed error and advice are reused on both paths;
- no provider call is made after a failed pre-flight.

This is a cleaner resolution than choosing only between `gather_for_grouping` and `build_plan`.

---

## High-priority findings

### 6. Rename and copy records are underspecified

The proposed inventory object is:

```json
{"path":"src/diff.rs","xy":" M"}
```

But git rename and copy records involve two paths. The CLO-487 path-agreement contract requires the model to see every path exactly as gcm will validate and stage it. A single `path` plus `xy` may not preserve both the old and new path for a rename.

**Required change**

Specify the representation explicitly, for example:

```json
{
  "path": "new/path.rs",
  "old_path": "old/path.rs",
  "xy": "R "
}
```

or define two entries, while ensuring each distinct path still appears exactly once in the inventory.

Add a regression test for rename and copy paths. This is a direct CLO-487 contract risk and is more important than the newline-path test alone.

---

### 7. The 2.3 chars-per-token estimator is not universally conservative

The document says 2.3 is conservative because it is the low end of the measured data. But the measurements appear to be based on this repository’s mostly ASCII paths and diff content.

That does not establish a conservative bound for:

- Unicode paths;
- CJK content;
- emoji;
- long repeated non-ASCII strings;
- model-specific tokenizers;
- JSON-escaped control characters.

For some content, the true chars-per-token ratio can be well below 2.3, which would make the estimator undercount tokens and reintroduce the same provider 400.

**Required change**

Either:

- add a global safety margin;
- use a more conservative universal divisor;
- treat non-ASCII content separately;
- or explicitly document the residual risk.

Because real tokenization is a non-goal, the estimator still needs a stated worst-case policy. A “conservative for this fixture” estimator is not enough.

Also keep the existing provider-400 classification as a second line of defense. If the estimate is wrong, the provider error should still be translated into the same actionable context-window advice rather than exposed as a raw HTTP failure.

---

### 8. The “system allowance” and provider envelope are underspecified

The prose says the budget includes a fixed allowance for the system prompt and JSON envelope, but the formula only includes `system_allowance`:

```text
budget = window_tokens − completion_reserve − system_allowance
spent  = est(files) + est(stat) + est(body)
```

Provider payloads include more than the user content:

- system prompt;
- tool schema;
- response JSON schema;
- provider-specific request envelope;
- model parameter fields;
- serialization overhead.

Different providers may have different overhead.

**Required change**

Name the allowance explicitly, for example `envelope_allowance`, and define whether it is:

- a single worst-case constant across providers; or
- provider-specific.

Test 2 should validate the actual rendered `grouping_user_content`, not merely sum the `GroupingContext` fields. The integration capture test helps, but the unit test should also account for labels and the configured envelope allowance.

---

### 9. The static model-window table needs a precise matching and default policy

The recommendation of a static table is reasonable, but the design omits several important details:

- the initial table contents;
- the value of `DEFAULT_WINDOW`;
- whether matching is exact, prefix, or substring;
- precedence when one model name is a substring of another;
- whether lookup is model-only or `(provider, model)`;
- env override validation rules.

Model-only substring matching is fragile. For example, a generic family match can accidentally match a smaller-context variant. The same model name can also have different effective windows on different providers, especially Ollama.

**Required change**

Specify:

1. Exact model match first.
2. Longest family-prefix match second.
3. Provider-aware lookup where available.
4. Conservative `DEFAULT_WINDOW` for unknown models.
5. A warning when the default is used.
6. `GCM_CONTEXT_WINDOW_TOKENS` parsing rules: positive integer, bounded maximum, clear config error on invalid input.
7. Tests for substring collisions and env override precedence.

---

## Medium-priority findings

### 10. Prompt degradation is currently silent from the user’s perspective

D3 can shrink or remove diff content. D2 also silently removes per-file untracked stubs from the prompt body. The model sees a trailer, but the user does not necessarily know that grouping quality was degraded.

This should be operationally visible.

**Recommended change**

Emit a warning or diagnostic when:

- body is re-truncated;
- body is reduced to header-only sections;
- body is dropped entirely, if that mode is retained;
- the default window is used;
- the env override changes the budget.

Example:

```text
warning: grouping prompt reduced to fit the model context window
estimated prompt: 118,432 tokens
window: 131,072 tokens
diff content: 412 of 515 files reduced to headers
```

This can reuse the CLO-798 diagnostics vocabulary.

Without this, a poor grouping result caused by aggressive truncation will look like a model-quality problem rather than a budget decision.

---

### 11. Use structured JSON serialization, not manual string formatting

The design shows `files` as a JSON string. That is consistent with the existing `GroupingContext` shape, but the document should explicitly require that the JSON be produced with `serde_json` rather than `format!`.

This matters for:

- newlines;
- quotes;
- backslashes;
- control characters;
- Unicode escapes;
- rename paths.

The test plan covers newline paths; add quote, backslash, and Unicode path cases as well.

---

### 12. The `append_untracked` flag should not be a bare boolean

A flag such as `include_stubs: bool` creates an unclear call site:

```rust
append_untracked(..., true)
```

Use a self-documenting enum instead:

```rust
enum UntrackedRenderMode {
    PerFileStubs,
    SummaryTrailer,
}
```

This is a small code-quality improvement, but it prevents future call sites from misunderstanding the flag.

---

## Additional test-plan gaps

The existing test plan is good, but I would add:

1. **Rename/copy inventory test** — both old and new paths are represented and stageable.
2. **Single-commit fallback matrix**:
   - grouping prompt over budget, single-commit prompt fits → fallback succeeds;
   - grouping and single-commit prompts both over budget → one clear gcm error;
   - pre-flight failure makes zero HTTP requests.
3. **Invalid `GCM_CONTEXT_WINDOW_TOKENS`**:
   - empty;
   - non-numeric;
   - zero;
   - negative;
   - implausibly large.
4. **Model-name collision test** — exact match takes precedence over family substring.
5. **Unknown model default warning test**.
6. **Actual rendered prompt test** — not just summing context fields.
7. **Unicode and escaped-path estimator test**.
8. **Degradation warning test** — truncation is user-visible.
9. **Provider 400 fallback test** — if the estimator undercounts, the HTTP error is still translated into context-window advice.

---

## Criterion assessment

| Criterion | Assessment |
|---|---|
| Completeness | Strong problem analysis, but core budget parameters and path semantics are unresolved. |
| Architecture quality | D1–D3 are sound, but D3 conflicts with AC4 and the pre-flight owner is unclear. |
| Codebase alignment | Very good: shared facade, no provider trait change, public-surface awareness. |
| Code quality | Mostly clean; avoid boolean flags and require structured JSON generation. |
| Security posture | No secret-handling concerns, but env override validation and path encoding need explicit rules. |
| Operational readiness | Pre-flight error is good, but degradation and default-window use need user-visible diagnostics. |
| Concurrency safety | No new concerns; pre-flight is local and synchronous. |
| Blind spots | Rename paths, Unicode token estimation, provider output caps, provider envelope overhead, and silent body degradation are missing. |

---

## Prioritized action items

### P0 — must resolve before implementation

1. Redefine AC2 to match the actual prompt structure, or redesign `body` to avoid path headers.
2. Resolve the AC4 versus D3 contradiction: either fail when diff body must be fully dropped, or explicitly redefine degraded grouping and update AC4.
3. Clarify the single-commit non-goal versus the required single-commit pre-flight check.
4. Define `completion_reserve`, including path-echo output, reasoning tokens, and provider output caps.
5. Resolve Q3 with a shared pre-flight checker used by both grouping and single-commit paths.
6. Specify rename/copy representation in the merged `files` inventory.

### P1 — should resolve in the same design revision

7. Make the token estimator conservative for Unicode and non-ASCII content, or document a safety margin and provider-400 fallback.
8. Specify the envelope/system allowance and test the actual rendered prompt.
9. Define static model-table contents, matching precedence, `DEFAULT_WINDOW`, and env validation.
10. Add user-visible diagnostics for prompt degradation and unknown-window defaults.

### P2 — implementation polish

11. Replace the `append_untracked` boolean flag with an enum.
12. Require `serde_json` for inventory serialization.
13. Add rename, quote, backslash, Unicode, env-validation, fallback-matrix, and no-network-on-preflight tests.

---

## Overall recommendation

The design is close, and the core fix is well understood. The main problem is that the document’s strongest guarantees — “each path appears once” and “diff-too-large errors clearly” — are not currently implemented by the proposed algorithm. Resolve those contradictions and the two central budget parameters, then this should be ready for approval.
