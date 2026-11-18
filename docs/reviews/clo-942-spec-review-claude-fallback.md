# Spec Review: clo-942

**Reviewer**: Claude (fallback)
**Reviewed**: 2026-11-18
**Pipeline**: lok spec-review
**Note**: Both external reviewers failed; this is the fallback review

---

I've finished checking the spec against the repository and am writing up the review now.

# Spec Review: CLO-942, the orchestrate pi extension on pi 1.5.x

I checked the spec against the code. I read `.pi/extensions/orchestrate/index.ts`, both `package.json` files, both lockfiles and `.github/`, and I ran `pi --version`.

## 1. Problem Statement

The problem statement is accurate, and the code confirms each claim:
- `StringEnum` is imported at `index.ts:3` and used at lines 226, 241 and 242.
- The `Static<>` aliases are at lines 246-247.
- `WorkflowPhase` is derived from `WORKFLOW_PHASES` at line 41.
- The orchestrate lockfile is still on pi 1.1.0, and the overrides from PR #81 are present.

One small inaccuracy: the reference versions do not match the linear manifest. Linear's `package.json` still declares `^1.1.0`. Only its lockfile resolves 1.5.2. AC3 (`^1.5.2`) is therefore stricter than "same as linear". That choice is reasonable, but the spec does not mention it.

## 2. Acceptance Criteria

AC1 to AC7 are testable and match the Linear ticket. AC8 has a real defect.

- **AC8 cannot be checked at PR time.** Dependabot alerts only track the default branch. A push to `chore/clo-942-pi-upgrade` changes nothing in the alerts API, so AC8 can only be checked after the merge. The spec also has no baseline: it does not record which alerts are open now. Without that list, "no *new* alert" cannot be measured. The `gh api .../dependabot/alerts` call also returns only the first page (30 alerts) unless it uses `--paginate` or `per_page=100`.
- **AC5 does not pin the TypeScript version.** Neither lockfile contains `typescript`, so `npx tsc` uses whatever version npx resolves. The AC5 text also leaves out the `--module nodenext --moduleResolution nodenext --target es2022` flags that Evaluation #1 uses. The two must state the same command.
- **Evaluation #2 only tests the negative case.** The line `const p: ...["phase"] = "bogus"` also fails when the type is `never`. A second line that assigns a valid phase and compiles proves that the type is the `WorkflowPhase` union.

## 3. Constraints & Assumptions

The constraints are good. The plain `{type, enum}` shape, the ban on `Type.Union(Literal)`, the ban on `as any` and `@ts-*`, and "do not touch linear" all fit the task. The escalation triggers are relevant.

One assumption is not stated: the runtime validator accepts the `Type.Unsafe` schema. `pi-coding-agent` 1.5.2 brings its own `typebox@1.3.27` (`node_modules/@earendil-works/pi-coding-agent/node_modules/typebox`). The extension imports `typebox@1.3.36` from the root. The types will most likely line up, because the same split exists today on 1.1.0. However, `tsc` passing does not prove that pi rejects an invalid phase at runtime.

## 4. Decomposition / Phases

The order is correct: 1 and 2 land together, and 3 follows. Two notes:

- **Sub-task 3 (brace-expansion) is outside the ticket's scope.** The ticket names only undici and minimatch. The evidence favours removing the override: in the linear lockfile, `minimatch@11.0.3` already declares `brace-expansion: ^5.0.12`, so the `^5.0.10` override is redundant. This makes the sub-task low-risk. Keep it, but describe it in the PR as an extra.
- **minimatch will move down.** The current orchestrate lockfile, with overrides, has `minimatch` 11.0.9 and `undici` 8.12.4. Without overrides, both can resolve lower (linear has 11.0.3 and 8.12.3). Both still meet AC7. The spec must call this expected, so the reviewer does not read it as a regression.

## 5. Risks & Open Questions

- **The load check is optional, but pi 1.5.2 is installed locally** (`/Users/mk/bin/pi`, `pi --version` prints 1.5.2). The spec's "if pi is not installed" branch therefore does not apply. Make the live check mandatory: the extension loads, the orchestrate tools register, and one call with an invalid phase is rejected.
- **npm audit on the 1.5.2 tree can report advisories outside undici and minimatch.** Linear needed an `@hono/node-server` override, but that package comes in through `@modelcontextprotocol/sdk`, and orchestrate does not depend on it. The existing escalation trigger covers this case. I did not check the full dependency tree of `pi-mcp` 1.5.2.
- No CI workflow type-checks `.pi/extensions/`, because `ci.yml` does not reference them. AC5 is therefore checked only locally. This is a follow-up, not a blocker.

## Verdict

**APPROVE_WITH_SUGGESTIONS**

## Priority Actions

1. Rewrite AC8:
   - Record the open alerts for `.pi/extensions/orchestrate/package-lock.json` now, as a baseline.
   - Check again after the PR is merged to `main`, not after the push.
   - Add `--paginate` to the `gh api` call.
2. Make the Pi load check mandatory, because pi 1.5.2 is installed. Add one tool call with an invalid `phase` to prove that runtime validation still enforces the enum.
3. Pin the TypeScript version in AC5 and Evaluation #1, for example `npx -p typescript@<x> tsc ...`, and use the same flags in both places.
4. Add a positive assignment to Evaluation #2, so that a `never` type cannot pass.
5. Mark sub-task 3 as an extra beyond the ticket in the PR body, and cite `minimatch` declaring `brace-expansion ^5.0.12` as the evidence.
6. In section 1, say that `^1.5.2` is stricter than linear's manifest (`^1.1.0`), and that the lower resolved versions of minimatch and undici are expected.
