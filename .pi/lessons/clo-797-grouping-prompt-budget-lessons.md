# CLO-797 lessons - grouping prompt token budget

## L1 - A pessimistic token estimate may shape a prompt but must not refuse one

**Source incident**: CLO-797 F2/D1. The 2-bytes-per-token estimator refused a 281 KB `--all` message that released gcm 0.8.3 sent to Groq gpt-oss-120b successfully. The measured ratio was 2.79 bytes per token (docs/investigations/clo-797-assets/measurements.txt).

**Rule**: An over-count is safe only where the cost is a smaller prompt, such as the grouping ladder. A pre-flight that refuses a request without sending it needs an estimate that errs low, with the provider context-window error as the backstop.

**How to apply**: When a design adds a token pre-flight, name the decision that each estimate drives: shrink or refuse. Use `estimate_tokens` for shrinking and `estimate_tokens_generous` (src/budget.rs:55) for refusing. Never let one estimator serve both decisions.

## L2 - The context-window table must cover the models in the owner's config, and Groq's /models endpoint is the source

**Source incident**: CLO-797 F4. The table listed one exact Groq model. The owner's qwen/qwen3.8-27b fell to the 32,768 default and was refused (fixed in 53139a6).

**Rule**: A window entry keyed on one exact model id misses the sibling ids that providers ship every few weeks. Groq's `GET /openai/v1/models` returns `context_window` per model, and it is the authoritative source for Groq entries.

**How to apply**: Key Groq entries on a family prefix (src/budget.rs:103), and check the owner's configured models against the table before a live run. Cite a provider-published source for every entry. The Gemini and OpenAI entries still cite aggregators.

## L3 - Commit and push orchestrate artifacts before pausing at a checkpoint

**Source incident**: CLO-797 F1. The first worktree was removed while the design commit was unpushed and the plan file was uncommitted. PROJECT.md on main still reported the plan as approved. The design survived only as an unreachable object (07119ff), and the plan survived only in the transcript.

**Rule**: A worktree removal discards unpushed work without warning. An aggregation file must not report an approval that the workflow YAML does not record.

**How to apply**: At every checkpoint where the session stops for the owner, commit and push the design, plan and workflow YAML first. Before resuming a task, compare PROJECT.md with the workflow YAML and with `origin/<branch>`.

## L4 - A cache-regeneration test must seed an advanced cache, not a fresh plan

**Source incident**: CLO-797 F3. A fresh plan with a null group-1 message is rejected with "group 1 has no commit message. Falling back", so the regeneration path in `commit_first_group` never runs.

**Rule**: Regeneration runs only for a cache that has advanced past group 1.

**How to apply**: To reach that path in an integration test, commit group 1 of a two-group plan whose group-2 message is null (this needs `git commit -S`), or write the cache file with a matching fingerprint. See `cached_group_message_over_budget_sends_nothing` (tests/grouping_budget.rs:358).
