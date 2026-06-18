# Design: `/repo-init` - one-command project bootstrap

**Date**: 2026-06-18
**Status**: Draft (awaiting review)
**Author**: Max Kulish (with Claude)

## Summary

`/repo-init` is a high-level orchestrator command that bootstraps a freshly-cloned
empty GitHub repo - or retrofits an existing one - into a working project with the
full agent-tooling stack. It replaces the manual sequence of "clone empty repo, then
run a handful of commands" with a single command that gathers configuration once,
fans the work out to parallel stage workers, verifies the result, and reports.

It composes two existing commands (`/cmd:create`, `/go:project-init`) into a coherent
whole and fills the gap neither covers: migrating `.pi`/`.lok` tooling, copying the
`docs/` structure, and coordinating everything from one entry point.

## Problem

The current workflow is manual and order-sensitive: create an empty repo on GitHub,
clone it, then run several commands to scaffold the language project, copy Claude
commands, copy `.pi`/`.lok` config, and set up `docs/`. Nothing ties these together,
nothing shares configuration between them (so the same questions get asked repeatedly),
and there is no single place that knows the full bootstrap recipe.

## Goals

- One command bootstraps a new repo end-to-end (scaffold + agent tooling + docs).
- The same command retrofits agent tooling + docs onto an existing project.
- Ask the user everything **once**, up front; no stage re-prompts.
- Reuse existing building blocks (`/cmd:create` mechanics) rather than reimplementing.
- Each stage is independently runnable and re-runnable.
- Reference repo is parameterized (default: `lok`), so the recipe is not hardcoded.

## Non-goals

- Creating the GitHub repo itself (the user already does `gh repo create` + clone).
- Branch protection, labels, first push (could be a later extension).
- Fleshed-out Rust+TS and Python scaffold templates (stubbed in v1 - retrofit covers
  the existing Rust project; fresh multi-stack repos come later).
- A general-purpose plugin system. Stages are a fixed, known set.

## Decisions (resolved during brainstorming)

| Question | Decision |
|----------|----------|
| Scope | Tooling **+** language scaffold |
| Scaffold source | Per-language template sets, GitHub-flavored, vendor-neutral |
| Stack choices | `Rust + TypeScript`, `Golang`, `Python` - always asked up front |
| Template completeness (v1) | **Golang authored** (minimal); Rust+TS and Python **stubbed** |
| Go template depth | **Minimal**: `go.mod`, `main.go`, `Makefile`, `.gitignore`, `CLAUDE.md` |
| Existing Rust project | Handled by **retrofit mode** (no scaffold templates needed) |
| Architecture | Orchestrator command + subcommands; parallel subagent fan-out |
| Interaction | All Q&A front-loaded into one preflight pass |
| CI flavor | GitHub Actions + optional GHCR (from `real-estate-nl`), not GitLab |
| Entry point | **`/repo-init:orchestrate`** - bare `/repo-init` + same-named subdir is an unsupported edge case (confirmed against Claude Code docs) |

## Reference repos (analyzed, grounding the templates)

| Repo | Role | Notes |
|------|------|-------|
| `~/Code/orchestrator/lok` | **Default tooling + docs reference** | Cleanest `.claude`/`.pi`/`.lok`; values to transform out: `lok`/`lokomotiv`, `CLO`, `cloud-ai`, `design-docs`↔`designs` |
| `~/Work/bot-reviewer` | Golang **structure** reference | Exemplary `cmd/`+`internal/` + tooling, but GitLab/Intel471-bound; only its structure informs the minimal Go template |
| `~/Code/real-estate-nl` | Multi-stack (Rust+TS+Python) model + **GitHub CI** reference | Per-subproject `CLAUDE.md`, root `Makefile` orchestration, GitHub Actions + GHCR `ghcr.io/maxkulish/...` |

## Architecture

### Two modes (auto-detected in preflight)

| Mode | Trigger | Stages that run |
|------|---------|-----------------|
| **fresh** | empty / near-empty repo | scaffold, claude, pi, docs, verify |
| **retrofit** | repo already has `Cargo.toml` / `go.mod` / `pyproject.toml` / `package.json` / `src/` | claude, pi, docs, verify (scaffold **skipped**; stack auto-detected from the manifest) |

The mode is auto-detected and confirmed with the user. Fresh-vs-retrofit collapses the
earlier "tooling only" vs "tooling + scaffold" scope decision into one self-selecting command.

### Component layout

Commands live in the global commands dir (`~/.claude/commands/`) so they are reusable
across every project:

```
~/.claude/commands/repo-init/
├── orchestrate.md            # /repo-init:orchestrate - orchestrator (entry point)
├── scaffold.md               # /repo-init:scaffold - render stack template set (fresh only)
├── claude.md                 # /repo-init:claude   - migrate .claude (non-interactive cmd:create)
├── pi.md                     # /repo-init:pi       - migrate .pi + .lok + lok.toml (+ npm install)
├── docs.md                   # /repo-init:docs     - mirror docs/ structure + AI-AGENTS.md from reference
├── verify.md                 # /repo-init:verify   - build/test/lint + leftover-value scan
└── templates/                # .tmpl only - never .md, so nothing here registers as a command
    ├── golang/               # authored (minimal, .tmpl files)
    ├── rust-ts/              # stub (STUB.tmpl)
    └── python/               # stub (STUB.tmpl)
```

Each stage `.md` is the **single source of truth** for that stage. It is both (a) a
standalone subcommand the user can run by hand (`/repo-init:pi`) and (b) the instruction
file the orchestrator hands to a subagent. Only `.md` files register as commands, so all
template/support files use a non-`.md` suffix (`.tmpl`) - same pattern as `/go:project-init`.
There is **no** bare top-level `repo-init.md`; the orchestrator is `repo-init/orchestrate.md`
(→ `/repo-init:orchestrate`) because a top-level file cannot safely coexist with a
same-named command directory.

### Config manifest

The orchestrator writes `.repo-init/config.yaml` after the Q&A. Every stage - whether run
as a subagent or standalone - reads it. Added to `.gitignore`; left in place after the run
so a single stage can be re-run idempotently. Schema:

```yaml
tool_name: gcm                 # replaces "lok"/"lokomotiv" in migrated tooling
module_path: github.com/maxkulish/gcm
owner: maxkulish
repo: gcm
stack: golang                  # golang | rust-ts | python
mode: fresh                    # fresh | retrofit
reference_repo: /Users/mk/Code/orchestrator/lok
vcs: github
linear:                        # or null
  workspace: cloud-ai          # target workspace (transform target)
  prefix: GCM                  # target issue prefix
groups: all                    # which command/extension groups to migrate
docs_convention: designs       # chosen design-doc folder: designs | design-docs
overwrite_policy: ask          # ask | overwrite | skip | backup
go_version: "1.26"
```

### Execution flow (orchestrator)

1. **Preflight** (main thread): assert git repo; detect mode (fresh vs retrofit) from
   existing manifests; detect VCS = GitHub via `git remote get-url origin`; derive
   `owner/repo` → `module_path`; auto-detect stack in retrofit mode.
2. **Q&A** (one AskUserQuestion pass): stack (if fresh), reference repo (default `lok`),
   confirm name/module, Linear workspace+prefix (or none), command/extension groups,
   `docs_convention`, overwrite policy. Validate the reference repo has `.claude/`
   (+ optional `.pi/`, `.lok/`, `docs/`).
3. **Write `.repo-init/config.yaml`.** The `.repo-init/` ignore line is owned by the
   `scaffold` stage in fresh mode (it writes `.gitignore`); in retrofit mode the
   orchestrator appends the line to the existing `.gitignore` after the barrier. This
   avoids two writers touching `.gitignore`.
4. **Fan out** the active stages as parallel subagents (single message, multiple Task
   calls). The orchestrator (the main agent following the command) dispatches the subagents
   via the Task tool - the same pattern `task:orchestrate` relies on, though not separately
   documented - handing each subagent the relevant `repo-init/<stage>.md` plus the manifest
   path. If parallel dispatch proves unreliable, the stages run sequentially (same files,
   same outcome, slower). Output paths are disjoint, so no worktree isolation is needed:

   | Stage | Owns (writes only here) | Source |
   |-------|-------------------------|--------|
   | scaffold (fresh only) | `go.mod`, `main.go`, `Makefile`, `.gitignore`, `CLAUDE.md` | golang template set |
   | claude | `.claude/` | reference repo (transformed) |
   | pi | `.pi/`, `.lok/`, `lok.toml` | reference repo (transformed) |
   | docs | `docs/` (structure + skeletons), `AI-AGENTS.md` | reference repo layout |

5. **Barrier**: wait for all active stages.
6. **Verify** (sequential, main thread): per-stack build/test/lint (Go: `go build ./...`
   `&& go test ./...` + `go vet`); `rg` scan for leftover reference-specific values
   (`lok`, `lokomotiv`, `CLO`, `cloud-ai`); print the resulting file tree.
7. **Report** + next steps; offer the initial commit.

### Stage details

**scaffold** (fresh only) - renders the minimal golang template:
- `go.mod` → `module {{MODULE_PATH}}` + `go {{GO_VERSION}}`
- `main.go` → minimal `package main` that builds and runs
- `Makefile` → `build`, `test`, `run`, `fmt`, `vet`, `tidy`
- `.gitignore` → Go ignores **plus** the `.repo-init/` line (merge if one already exists)
- `CLAUDE.md` → < 60 lines: project name, autonomy tier (default Tier 2), build commands,
  pointers to `docs/` and `AI-AGENTS.md`

**claude** - executes `/cmd:create`'s mechanics (its Phases 4-8: scan → transform → write
→ validate) driven by the manifest, **skipping** its interactive Phases 1-3. `/cmd:create`
remains the standalone interactive entry point; this stage is its config-driven sibling.

**pi** - copies `.pi/` (agents, orchestrator/phases, skills, lessons, scripts) and
`.lok/` (`lok.toml`, workflows, prompts) from the reference, applying the same value
transforms. For `.pi/extensions/*` (TypeScript): copy source + `package.json`, **never**
`node_modules`, then run `npm install` per extension (the reference repos expose a
`make pi-init` target for this).

**docs** - mirrors the chosen reference's actual `docs/` folder set as empty skeletons
(adaptive, not a hardcoded list), seeds `PROJECT.md`/`ROADMAP.md`/`DEPENDENCIES.md`
skeletons, and standardizes the `design-docs`↔`designs` / `prds`↔`prd` split per
`docs_convention` (picking one, noting the other). Also adapts `AI-AGENTS.md` from the
reference.

**verify** - build/test/lint for the stack; leftover-value scan with auto-fix offer
(mirrors `/cmd:create` Phase 7); file-tree listing.

## Value transformation

Reuses `/cmd:create`'s transformation engine, extended to `.pi`/`.lok`. From the `lok`
reference, the concrete strings to rewrite:

| From | To | Where |
|------|----|-------|
| `lok`, `lokomotiv` | `{{tool_name}}` | crate/binary names, `.lok/`, command text |
| `CLO`, `clo-XX` | `{{prefix}}`, `{{prefix}}-XX` | commands, workflow files, agent docs |
| `linear.app/cloud-ai` | `linear.app/{{workspace}}` | command files |
| `docs/design-docs/` ↔ `docs/designs/` | per `docs_convention` | command + phase files |

## Error handling

- Invalid/missing reference → abort with expected-structure message.
- Non-empty target in fresh mode → per-path overwrite/skip/backup prompt (front-loaded).
- A subagent fails → report which stage; keep the others (disjoint outputs); offer to
  retry just that stage (re-dispatch, or run its standalone subcommand). Stages are
  idempotent: read manifest, write files.
- Verify build/test failure → surfaced, never silently "fixed".
- Leftover-value scan failures → auto-fix offer, then re-scan.

## Verification / acceptance criteria

Acceptance = run `/repo-init:orchestrate` against `gcm` (Go, fresh) end-to-end and confirm:
- `go build ./...` and `go test ./...` succeed.
- `.claude/`, `.pi/`, `.lok/`, `docs/` present and populated.
- Zero leftover `lok`/`lokomotiv`/`CLO`/`cloud-ai` occurrences.
- `CLAUDE.md` < 60 lines.
- Re-running a single stage (e.g. `/repo-init:docs`) is idempotent.

Secondary: run `/repo-init:orchestrate` against the existing Rust project → retrofit mode detected,
scaffold skipped, tooling + docs applied, project still builds.

A `--dry-run` flag prints the planned actions and file list without writing.

## User-level installation & verification

The `/repo-init:*` suite installs at the **user level** (`~/.claude/commands/repo-init/`), so
it is available from every project, not just `gcm`. This is a first-class requirement.

**Command-loading rules (confirmed against Claude Code docs):**
- Entry point is **`/repo-init:orchestrate`**. A bare top-level `repo-init.md` coexisting with
  a same-named `repo-init/` directory is an unsupported/undocumented edge case, so it is
  avoided; the orchestrator lives at `repo-init/orchestrate.md`, matching the proven
  `task/orchestrate.md` → `/task:orchestrate` pattern.
- Nested paths namespace with colons: `repo-init/scaffold.md` → `/repo-init:scaffold`.
- Only `.md` files register as commands; template/support files use `.tmpl` so nothing under
  `templates/**` registers - including stubs (`STUB.tmpl`, never `STUB.md`).
- User-level commands override project-level on a name clash and operate on the **target
  repo's CWD** (`$PWD`) - exactly what bootstrap needs.
- Files added under the existing `~/.claude/commands/` dir are picked up **within the
  session** via `/reload-skills` (a full restart is only needed for a brand-new top-level
  dir, which this is not).

**Verification method ("does it work at user level?")**: after authoring the files, run
`/reload-skills`; confirm `/repo-init:orchestrate` and every `/repo-init:<stage>` appear in
the command list; invoke `/repo-init:orchestrate` from an unrelated project dir and confirm it
writes to that project's CWD; confirm no file under `templates/**` appears as a command.

Acceptance criteria for user-level operation:
- `/repo-init:orchestrate` and every `/repo-init:<stage>` are listed and invocable from an
  unrelated project's working directory.
- No file under `~/.claude/commands/repo-init/templates/**` registers as a command.
- Running the orchestrator from inside a target repo writes to `$PWD`, not to `~/.claude`.

## Stubs (Rust+TS, Python)

A stub template set contains a `STUB.md` describing what a full set will include (per the
`real-estate-nl` model: Cargo workspace + `core/`, `frontend/` with Vite, `scraper/`/`poc/`
with `uv`, root Makefile, multi-stage Dockerfile, GitHub Actions matrix, per-subproject
`CLAUDE.md`). Selecting a stubbed stack in **fresh** mode warns the user and produces a
minimal placeholder; **retrofit** never needs them.

## Future work

- Flesh out Rust+TS and Python template sets when the first fresh repo needs them.
- Optional GitHub setup stage (branch protection, labels, first push).
- Optional CI/goreleaser/pre-commit add-on for Go projects that outgrow the minimal scaffold.
