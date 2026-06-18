# Task Completion Summary Template

**Purpose**: Single canonical format for the final "task done" message shown to the user. Used by `task/phases/complete.md` (Step 7) and `pr/finalize.md` (Step 10). Output must be deterministic — same workflow YAML produces the same summary regardless of which LLM renders it.

---

## Source of Data

Every field below is read from `docs/status/clo-XX-workflow.yaml`. **Do not invent values.** If a field is missing from the YAML, print the literal placeholder shown in the "Missing data" column. Never narrate ("all assumptions held"); render counts ("8 held / 0 violated / 0 untested").

| Block | YAML key | Missing data |
|-------|----------|--------------|
| Task title | `task_title` | `(no title recorded)` |
| Task type | `task_type` | `unknown` |
| Linear URL | `linear_url` or `task_url` | omit row |
| Linear status | `linear.status_at_complete` | `(unchanged)` |
| Branch | `linear.branch_actual` | `(none)` |
| PR url / number | `phases.pr.pr_url` / `pr_number` | `(no PR)` — skip PR block entirely if both empty |
| Merged timestamp | `phases.complete.merged_at` (fallback `phases.pr.merged_at`) | `(not merged)` |
| Merge commit | `phases.complete.merge_commit` (short to 7) | `(none)` |
| CI status | `phases.pr.ci_passed` → ✅ passed / ❌ failed / ⚠️ pending | ⚠️ pending |
| Discovery approach | `phases.discovery.approach_chosen` | `(none)` |
| Baseline score | `phases.discovery.baseline_score` + `/10` | `(none)` |
| Alternatives considered | `phases.discovery.approaches_identified` | `(none)` |
| PRD file | `phases.discovery.prd_file` | `(none)` |
| Discovery report | `phases.discovery.discovery_report` | `(none)` |
| Design doc | `phases.design.design_doc` | `(none)` |
| Assumption totals | `len(phases.design.assumptions)` plus tally of `phases.implement.assumption_outcomes[*].outcome` (`held` / `violated` / `untested`) | `0 surfaced` |
| Design review verdict | `phases.design.review_verdict` | `(none)` |
| Applied suggestions | `len(phases.design.applied_suggestions)` | `0` |
| Flagged suggestions | `len(phases.design.flagged_suggestions)` | `0` |
| Plan file | `phases.plan.plan_file` | `(none)` |
| Commits | `len(phases.implement.commits)` + first 7 chars of each SHA, comma-joined | `0` |
| Validation verdicts | `phases.implement.codex_verdict` · `gemini_validation_report` presence · `validation_synthesis_verdict` | `(not run)` |
| Fix passes | `phases.implement.validation_fix_iteration_count` | `0` |
| Review threads resolved | `phases.pr.review_comment_threads_resolved` | `0` |
| Reviews addressed flag | `phases.pr.reviews_addressed` → ✅ / ❌ | ❌ |
| Bot review wait | `phases.pr.bot_review_wait_completed` → ✅ completed / ⚠️ skipped | ⚠️ skipped |
| Pre-merge re-fetch | `phases.pr.pre_merge_refetch_passed` → ✅ passed / ❌ failed / ⚠️ skipped | ⚠️ skipped |
| Lessons file | `phases.complete.lessons_file` | omit Lessons block if empty |
| Lessons list | `phases.complete.lessons_learned[]` (one bullet per item) | omit Lessons block if empty |
| Aggregation files updated | `phases.complete.aggregation_files_updated` → ✅ / ❌ on each of the three files | ❌ on all three |
| Footer phase / workflow / status | `workflow.current_phase` · `workflow.status` · derived (`✅ DONE` when both are `complete`, else `⚠️ IN PROGRESS`) | as observed |

---

## Phase-skip Rules

Operational tasks (`task_type: operational`) often skip Design, Plan, and full Implementation. For each section below:

- **Discovery**: if `phases.discovery.status` is `skipped` or `pending`, print the section header followed by `   (skipped)` on the next line. Do not omit the header.
- **Design**: same rule using `phases.design.status`.
- **Implementation**: same rule using `phases.implement.status`. If status is `complete` but `commits` is empty (docs-only operational change), show `Commits: 0 (docs only)`.
- **Pull Request** and **PR Review**: if no `pr_url` exists, omit both blocks entirely (no header, no `(skipped)` line) — operational tasks without code changes don't show PR sections at all.
- **Lessons**: omit the whole block if `lessons_learned` is empty.

The four mandatory blocks that always render regardless of task type: **Task**, **Discovery** (or skip line), **Aggregation Files**, **Footer**.

---

## Canonical Template

Render exactly this layout. Preserve the box-drawing characters, indentation (3-space indent inside each block), emoji choices, and separator widths. Do not add commentary, headings, or trailing text outside the bottom separator.

```
═══════════════════════════════════════════════════════════
  ✅ TASK COMPLETE — {{task_id}}
═══════════════════════════════════════════════════════════

📋 Task
   Title:   {{task_title}}
   Type:    {{task_type}}
   Linear:  {{linear_url}}  →  {{linear_status_at_complete}}
   Branch:  {{branch_actual}}

🔗 Pull Request
   URL:       {{pr_url}}  (#{{pr_number}})
   Merged:    {{merged_at}}
   Commit:    {{merge_commit_short}}
   CI:        {{ci_status_icon}} {{ci_status_word}}

🔍 Discovery
   Approach:        {{approach_chosen}}
   Baseline score:  {{baseline_score}}/10
   Alternatives:    {{approaches_identified}} considered
   PRD:             {{prd_file}}
   Report:          {{discovery_report}}

📐 Design
   Document:     {{design_doc}}
   Assumptions:  {{n_surfaced}} surfaced → {{n_held}} held · {{n_violated}} violated · {{n_untested}} untested
   Review:       Gemini → {{review_verdict}}
   Suggestions:  {{n_applied}} applied · {{n_flagged}} flagged (deferred)

⚙️  Implementation
   Plan:         {{plan_file}}
   Commits:      {{n_commits}}  ({{commit_shorts}})
   Validation:   Codex {{codex_verdict}} · Gemini {{gemini_verdict}} · Synthesis {{synthesis_verdict}}
   Fix passes:   {{validation_fix_iteration_count}}

🛠️  Pull Request Review
   Review threads resolved:  {{review_threads_resolved}}
   Bot review wait:          {{bot_review_wait_icon}} {{bot_review_wait_word}}
   Pre-merge re-fetch:       {{pre_merge_refetch_icon}} {{pre_merge_refetch_word}}

📚 Lessons Learned
   File: {{lessons_file}}
   • {{lesson_1}}
   • {{lesson_2}}

📂 Aggregation Files
   {{project_md_icon}} docs/PROJECT.md
   {{roadmap_md_icon}} docs/ROADMAP.md
   {{dependencies_md_icon}} docs/DEPENDENCIES.md

───────────────────────────────────────────────────────────
 Phase: {{current_phase}} · Workflow: {{workflow_status}} · Status: {{final_status_icon}} {{final_status_word}}
 → Next: check docs/DEPENDENCIES.md for unblocked tasks.
═══════════════════════════════════════════════════════════
```

---

## Done / Not-Done Signal

The footer line is the single authoritative "is this finished?" signal:

- `✅ DONE` — `workflow.current_phase == complete` AND `workflow.status == complete`
- `⚠️ IN PROGRESS` — any other combination

A reader (or downstream tooling) can grep `Status: ✅ DONE` to determine completion without parsing YAML.

---

## What This Template Replaces

- `task/phases/complete.md` Step 7 — Display Completion Summary
- `pr/finalize.md` Step 10 — both Worktree Mode and Regular Branch Mode blocks (one template handles both; `gd` worktree-cleanup instructions stay in their own "Next Steps" message after the summary)
