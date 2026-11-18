# Spec: Move the orchestrate pi extension to pi 1.5.x and remove the undici and minimatch overrides

**Created**: 2026-11-18
**Linear**: [CLO-942](https://linear.app/cloud-ai/issue/CLO-942)
**Estimated scope**: S (3 files, 3 sub-tasks)

## 1. Problem Statement

The repository holds two pi extensions under `.pi/extensions/`. Each has its own `package.json` and `package-lock.json`, and Dependabot scans both manifests.

PR #81 (commit `ad4a5d3`) closed 23 Dependabot alerts. In `.pi/extensions/linear`, an in-range `npm update` moved the `@earendil-works/pi-*` dev dependencies from 1.1.0 to 1.5.2, and the patched transitive versions came with it. In `.pi/extensions/orchestrate`, the same update produced 13 TypeScript errors. That extension therefore stayed on pi 1.1.0, and PR #81 added two `overrides` entries to `.pi/extensions/orchestrate/package.json` instead: `undici: ^8.12.2` and `minimatch: ^11.0.1`. Overrides pin transitive versions behind the backs of the packages that declare them, so they must be removed once the direct dependencies pull patched versions on their own.

The type errors have one cause. In pi 1.5.x, `StringEnum` from `@earendil-works/pi-ai` returns a pi-schema `Schema<T>`, not a TypeBox schema. `.pi/extensions/orchestrate/index.ts` imports `StringEnum` at line 3 and nests it in `Type.Object` from `typebox` at lines 226 (`phase`), 241 (`from_phase`) and 242 (`to_phase`), all with `WORKFLOW_PHASES`. With a non-TypeBox member, `Static<typeof UpdateWorkflowStateParams>` and `Static<typeof TransitionPhaseParamsSchema>` (lines 246-247) infer `unknown`, and every access to the parameters fails to type-check. Runtime is not affected, because Pi maps the pi-ai import to a compat entry. The extension is loaded by Pi as TypeScript source (`"main": "index.ts"`); there is no build step.

Line 3 is the only import from `@earendil-works/pi-ai` in `index.ts`. After the fix the extension does not use pi-ai, and `.pi/extensions/linear` does not list pi-ai either.

Reference versions from `.pi/extensions/linear/package-lock.json`: pi-coding-agent 1.5.2, pi-tui 1.5.2, undici 8.12.3, minimatch 11.0.3, brace-expansion 5.0.13. That manifest keeps a `brace-expansion: ^5.0.10` override. Its `package.json` still declares the pi packages as `^1.1.0`; only its lockfile resolves 1.5.2. This spec declares `^1.5.2`, which is stricter, so a fresh install cannot fall back to 1.1.0 and its unpatched transitive versions.

The current orchestrate lockfile resolves undici 8.12.4 and minimatch 11.0.9 through the overrides. Without the overrides, npm can resolve lower versions (for example 8.12.3 and 11.0.3, as in linear). That is expected, as long as AC7 holds.

Pi 1.5.2 is installed on the development machine (`pi --version`). `pi-coding-agent` bundles its own typebox (1.3.27), while the extension uses typebox 1.3.36, so `tsc` alone does not prove that Pi validates the parameters at runtime. Neither lockfile contains `typescript`; the global `tsc` is 7.0.2.

A second, pre-existing error blocks a clean type-check on both pi versions. Line 1013 casts to `WorkflowState["workflow"]["status"]`, but `workflow` is optional (line 101), so indexing it raises TS2339. A review run with TypeScript 7.0.2 and the AC5 flags measured: pi 1.1.0 gives 1 error, pi 1.5.2 gives 14, the local helper reduces that to 1, and `NonNullable<WorkflowState["workflow"]>["status"]` brings it to 0.

## 2. Acceptance Criteria

- [ ] AC1: `index.ts` defines a local helper built on `Type.Unsafe<T>({ type: "string", enum: [...] })` from `typebox`, and the three `WORKFLOW_PHASES` fields use it. The helper emits the plain `{ type: "string", enum: [...] }` JSON schema, with no `anyOf` and no `const`.
- [ ] AC1b: The `status` cast at line 1013 uses `NonNullable<WorkflowState["workflow"]>["status"]`. The change is type-only and the emitted JavaScript stays the same.
- [ ] AC2: `index.ts` has no import from `@earendil-works/pi-ai`, and `package.json` has no `@earendil-works/pi-ai` entry in `devDependencies`, `peerDependencies` or `peerDependenciesMeta`.
- [ ] AC3: `package.json` declares `@earendil-works/pi-coding-agent` and `@earendil-works/pi-tui` dev dependencies as `^1.5.2`, and the lockfile resolves both at 1.5.x.
- [ ] AC4: `package.json` `overrides` has no `undici` and no `minimatch` entry. `brace-expansion` stays unless sub-task 3 proves it is not needed.
- [ ] AC5: `npx -p typescript@7.0.2 tsc --noEmit --skipLibCheck --module nodenext --moduleResolution nodenext --target es2022 index.ts` reports 0 errors.
- [ ] AC6: `npm audit` in `.pi/extensions/orchestrate` reports 0 vulnerabilities.
- [ ] AC7: The lockfile resolves `undici` at or above 8.12.2 and `minimatch` at or above 11.0.1 without overrides.
- [ ] AC8: After the PR merges to `main`, GitHub shows no open Dependabot alert for `.pi/extensions/orchestrate/package-lock.json` beyond the baseline recorded before implementation.
- [ ] AC9: Pi 1.5.2 loads the extension and registers the orchestrate tools. An `update_workflow_state` call that is valid in every argument except `phase: "bogus"` is rejected by Pi's parameter validation, and the diagnostic names `phase` as the field that failed the enum check.

**Verification method**: commands in the Evaluation table, run in `.pi/extensions/orchestrate`. AC8: record the baseline before implementation and check again after the merge to `main` (Dependabot tracks the default branch only), with `gh api --paginate repos/maxkulish/gcm/dependabot/alerts` filtered on the manifest path and `state == "open"`. AC9 is a manual check in a Pi session in this repository, against a disposable workflow file (`docs/status/clo-99942-workflow.yaml` for the dummy task ID `CLO-99942`, deleted after the check; the tool accepts numeric IDs only), so that a missing argument or missing file cannot produce the error instead of the enum.

## 3. Constraints

**Must**:
- Change only `.pi/extensions/orchestrate/index.ts`, `.pi/extensions/orchestrate/package.json` and `.pi/extensions/orchestrate/package-lock.json` (plus the workflow and project docs).
- Keep the CLO-942 entries that `main` already records in `docs/PROJECT.md`, `docs/ROADMAP.md` and `docs/DEPENDENCIES.md` (commit `f19c750`); update them, do not replace them.
- Keep the enum JSON schema in the plain `{ type: "string", enum }` shape, because some providers reject `anyOf`/`const`.
- Keep the `Static<>` types of the two parameter schemas narrowed to the `WorkflowPhase` union, not `string` or `unknown`.
- Regenerate the lockfile with npm (`npm install`), not by hand.

**Must-not**:
- Do not use `Type.Union(Type.Literal(...))` for the enum (emits `anyOf`/`const`).
- Do not silence type errors with `as any`, `// @ts-ignore` or `@ts-expect-error`.
- Do not touch `.pi/extensions/linear`.
- Do not change tool behavior, tool names, descriptions or other parameters.

**Prefer**:
- A small generic helper, for example `function StringEnum<const T extends readonly string[]>(values: T, options?)` returning `Type.Unsafe<T[number]>({ type: "string", enum: [...values], ...options })`, so the call sites stay unchanged apart from the import.
- Follow the pi `docs/extensions.md#tool-parameter-schemas-and-typebox` guidance where it applies.

**Escalate when**:
- `tsc` reports errors that are not caused by the `StringEnum` typing or the line 1013 cast (another 1.5.x API change).
- `npm audit` still reports a vulnerability that only an override can fix.
- A pi 1.5.x peer requirement forces a different `typebox` major.

## 4. Decomposition

1. **Replace StringEnum and fix the status cast**: add the local `Type.Unsafe` helper in `index.ts`, remove the pi-ai import at line 3, keep the call sites at lines 226, 241, 242. Change the cast at line 1013 to `NonNullable<WorkflowState["workflow"]>["status"]`. - files: `.pi/extensions/orchestrate/index.ts`
2. **Bump pi and drop overrides**: remove pi-ai from all three dependency blocks, set pi-coding-agent and pi-tui dev dependencies to `^1.5.2`, delete the `undici` and `minimatch` overrides, regenerate `package-lock.json` with `npm install`, then run `tsc` and `npm audit`. - files: `.pi/extensions/orchestrate/package.json`, `.pi/extensions/orchestrate/package-lock.json`
3. **Check brace-expansion** (extra, not in the ticket; `minimatch@11.0.3` already declares `brace-expansion ^5.0.12`, so removal is low-risk): temporarily remove the `brace-expansion` override, run `npm install` and `npm audit`. If audit stays at 0 and the lockfile resolves brace-expansion at or above 5.0.10, keep it removed; otherwise restore it. Record the result in the PR body and label it as an extra. - files: `.pi/extensions/orchestrate/package.json`, `.pi/extensions/orchestrate/package-lock.json`

**Dependency order**: 1 and 2 land together (2 cannot pass `tsc` without 1); 3 follows 2.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 0 | Dependabot baseline | list of open alerts for the manifest, saved in the workflow YAML | `gh api --paginate repos/maxkulish/gcm/dependabot/alerts --jq '.[] \| select(.state=="open" and .dependency.manifest_path==".pi/extensions/orchestrate/package-lock.json") \| .number'` before sub-task 1 |
| 1 | Type-check | 0 errors | `npx -p typescript@7.0.2 tsc --noEmit --skipLibCheck --module nodenext --moduleResolution nodenext --target es2022 index.ts` |
| 2 | Parameter types exactly `WorkflowPhase` | all three assertions compile; changing any field to a narrower or wider type fails `tsc` | temporary scratch lines with a mutual-assignability check: `type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;` then `const a: Equal<UpdateWorkflowStateParams["phase"], WorkflowPhase> = true;`, the same for `TransitionPhaseParams["from_phase"]` and `TransitionPhaseParams["to_phase"]`; remove the lines after |
| 3 | Emitted JSON schema shape | `{ type: "string", enum: [...WORKFLOW_PHASES] }`, no `anyOf`/`const` | scratch script that imports the helper output and prints `JSON.stringify`, or inspect the helper body |
| 4 | Audit | `found 0 vulnerabilities` | `npm audit` |
| 5 | Transitive versions | undici >= 8.12.2, minimatch >= 11.0.1, no overrides for them | `npm ls undici minimatch` and read `package.json` |
| 6 | pi versions | pi-coding-agent and pi-tui 1.5.x; no pi-ai in the tree as a direct dependency | `npm ls @earendil-works/pi-coding-agent @earendil-works/pi-tui @earendil-works/pi-ai` |
| 7 | No pi-ai import | no match, `rg` exit code 1 (exit 2 means a path error, not a pass) | `rg 'pi-ai' index.ts package.json; echo $?` |
| 8 | Dependabot | no open alert for the manifest beyond the baseline from #0 | same command as #0, after the merge to `main` |
| 9 | Pi load and runtime validation | the orchestrate tools register; the call is rejected and the diagnostic names `phase` and its allowed enum values | create the disposable `docs/status/clo-99942-workflow.yaml`, start `pi` in the repository, list tools, call `update_workflow_state` with `task_id: "CLO-99942"`, a valid `action` and `details` and `phase: "bogus"`; delete the file after |

**Edge cases to verify**:
- pi-coding-agent 1.5.x may still pull pi-ai transitively; that is fine, AC2 is about direct dependencies only.
- `npm ls` may print `invalid` or `extraneous` for peer dependencies; the run must show no `ERR`.
- No CI workflow type-checks `.pi/extensions/`, so AC5 runs only locally. A CI check is a follow-up outside CLO-942.
