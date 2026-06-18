# Documentation

All project documentation lives here. The layout follows a lifecycle: an idea
moves from discovery, to a PRD, to a design, to a plan, to implementation, with
reviews and a status record along the way. Each stage has its own folder so the
artifacts stay separated by intent rather than by topic.

Three top-level files give an always-current view of the work; the subfolders
hold the per-task artifacts that back them.

## Top-level dashboards

| File | Purpose |
|------|---------|
| [PROJECT.md](PROJECT.md) | Live dashboard: active work (WIP-limited), prioritized backlog, recently completed, blocked. The single source of "what's happening now". |
| [ROADMAP.md](ROADMAP.md) | Phased plan: each phase groups related tasks with status and dependencies. The "where we're going". |
| [DEPENDENCIES.md](DEPENDENCIES.md) | Cross-task blockers and what is unblocked and ready to start. |

Keep the `Last Updated` line at the top of each current when you touch it.

## Stage folders

Per-task artifact folders (commands write here):

| Folder | Holds | Produced during |
|--------|-------|-----------------|
| `discovery/` | Discovery reports: problem framing, prior art, candidate approaches, discovery-debt score. One file per task. | Discovery, before committing to a solution. |
| `prds/` | Product requirements documents that define *what* to build and why. | Requirements. |
| `design-docs/` | Design docs: the chosen approach, the *how*, trade-offs, data/control flow. One file per task. (See the note below on `designs/`.) | Design. |
| `specs/` | Specifications for specification-type tasks (`docs/specs/YYYY-MM-DD-<task>-<slug>.md`). | Spec. |
| `plans/` | Implementation plans: ordered, checkable steps an engineer (or agent) executes. | Planning, before writing code. |
| `reviews/` | Review outputs. Per-reviewer files plus a synthesis (e.g. `<task>-review-synthesis.md`). | Design and code review. |
| `status/` | Per-task workflow status, one `<task>-workflow.yaml` per task tracking which phases are done. | Throughout, updated as a task moves. |
| `operations/`, `migrations/`, `audits/` | Outputs of operational tasks (ops reports, migration notes, security/compliance audits). | Operational tasks. |
| `guides/` | User-facing guides: setup, usage, how-to. Long-lived, not tied to a single task. | Any time. |
| `investigations/` | Exploratory write-ups: gap analyses, spikes, "how does X work" notes not tied to one deliverable. | Any time. |

Architecture/context folders (commands read these for grounding; you populate them):

| Folder | Holds |
|--------|-------|
| `adrs/` | Architecture Decision Records (`adr-NNN-<slug>.md`). Read during design/spec for invariants. |
| `arch/` | Architecture documents. Read in full during design review. |
| `context/` | Active patterns and invariants (e.g. `system-patterns.md`). Read during design. |

> **Known path inconsistency (inherited from lok, worth standardizing):** the
> migrated commands disagree on the design-doc folder. The Claude-side flow
> (`.claude/commands/design-doc`, `plan`, `pr`) writes to **`design-docs/`**,
> while the pi/lok runtime (`.pi/orchestrator/phases/design.md`,
> `.lok/prompts/design-draft-prompt.md`, `.lok/workflows/*.toml`) writes to
> **`designs/`**. Both folders exist here so neither runtime breaks, but a task
> that moves between the two runtimes will split its design doc across two
> folders. Pick one convention and update the other side's references to match.
> (A similar minor split exists for `prds/` vs `prd/`.)

## File naming

- Lowercase, dash-separated slugs: `health-check-probe.md`.
- Prefix with a date when the artifact is point-in-time (status snapshots,
  discovery reports, incident plans): `2026-06-18-validation-resilience.md`.
- If you adopt an issue tracker, prefix with the task id instead:
  `gcm-42-health-check-probe.md`. Reuse the same id across the matching files in
  `discovery/`, `designs/`, `plans/`, `status/`, and `reviews/` so a task's
  artifacts are easy to find.

## Lifecycle at a glance

```
discovery/  ->  prds/  ->  designs/  ->  reviews/  ->  plans/  ->  implement
                                                                      |
                                                          status/ (updated throughout)
```

Not every task needs every stage. A small fix may go straight to a plan; a large
feature walks the whole path. `status/<task>-workflow.yaml` records which stages
ran and which were skipped, and why.
