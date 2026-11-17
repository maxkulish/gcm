## Verdict: PASS_WITH_NOTES

The implementation satisfies AC-1 through AC-6. `--plan-only --all` generates one message and uses the same human preview block as `--dry-run --all`. Without `--all`, a merge still makes zero provider requests and a grouping fallback makes only the grouping request. The preview returns before cache clearing and index staging. The existing `--reset` acceptance case passed. I found no production correctness or security defect. [src/main.rs:917](/Users/mk/Code/gcm--fix-clo-940-plan-only-all/src/main.rs:917) · [src/main.rs:979](/Users/mk/Code/gcm--fix-clo-940-plan-only-all/src/main.rs:979)

## Findings

- **MEDIUM — The provider-failure test can pass without reaching the provider.** CLO-940-7 checks exit 1, `mode: "single"`, an absent top-level `message`, and an unchanged index. A local diff or budget error can satisfy every assertion. Clear `$CAPTURE`, set `GCM_RETRY_MAX=0` for this invocation, and assert one request plus `error.code == "Provider"`. This closes the AC-7 request-count gap for the failure case. [scripts/acceptance.sh:1335](/Users/mk/Code/gcm--fix-clo-940-plan-only-all/scripts/acceptance.sh:1335)
- **LOW — The human preview test accepts output written only to stderr.** CLO-940-1 merges stderr into stdout and searches for the message anywhere. It would pass if stdout were empty or the preview block disappeared. Capture stdout separately and compare it with `--dry-run --all` on the same fixture. I verified independently that the current release binary produces byte-identical stdout for both commands. [scripts/acceptance.sh:1256](/Users/mk/Code/gcm--fix-clo-940-plan-only-all/scripts/acceptance.sh:1256)
- **LOW — The cutover guide now gives incorrect credential advice.** It says the `--all` preview needs no API key. The preview now calls the selected provider, so a hosted provider such as Groq requires its configured credential. [docs/guides/cutover-from-bash.md:87](/Users/mk/Code/gcm--fix-clo-940-plan-only-all/docs/guides/cutover-from-bash.md:87)

## Missing Items

AC-7 has cases for every requested scenario. The two assertions above are the remaining coverage gaps. All CLO-940 cases, AC-493-10, and AC-493-13 passed. Formatting, Clippy, and all 618 Rust tests passed. Runtime checks used `cc34f07`. I also reviewed `cff44c7`, whose subsequent changes were documentation and a Rust comment.

The full acceptance run recorded **291 passed, 2 failed, 1 skipped**. Both failures were in unchanged cases. AC-488-1 matched a retry warning after successful recovery. AC-493-7 expected `Provider` after removing the key, but with no provider configured it received `OnboardingRequired`. [scripts/acceptance.sh:795](/Users/mk/Code/gcm--fix-clo-940-plan-only-all/scripts/acceptance.sh:795) · [scripts/acceptance.sh:1203](/Users/mk/Code/gcm--fix-clo-940-plan-only-all/scripts/acceptance.sh:1203)

## Recommendations

Tighten CLO-940-7 and CLO-940-1 and correct the cutover guide. The runtime implementation can stand as written.