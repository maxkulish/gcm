# Spec Review Synthesis: clo-941

**Synthesized**: 2026-11-17
**Pipeline**: lok spec-review

---

**Reviewer status:** Only the Claude fallback produced a usable review, so this synthesis comes from that one source.

- **Gemini failed** (`success=False`). Its output was the help text of `opencode run`, not a review. The call was probably built with the wrong arguments.
- **Ollama failed.** The model `glm-5` was retired on 2026-07-15, and the service suggests `glm-5.3-flash:cloud` instead.

Because there is one source, no finding can be marked as agreed or disputed. The Agreement and Disagreement sections are empty, and every finding is listed as single-reviewer.

## Agreement (High Confidence)
| # | Finding | Severity |
|---|---------|----------|
| - | None: only one reviewer returned a review | - |

## Disagreement (Needs Human Decision)
| # | Topic | Gemini Position | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|-----------------|
| - | None: only one reviewer returned a review | Failed | Failed | - |

## Novel Insights (Single Reviewer)
| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | The reported failure did not come from the 2xx body-read path. ureq prints a truncated body as `io: Peer disconnected` (`ureq-3.3.0/src/error.rs:236`, `:208`). The triage transcript shows `could not reach the ... API: unexpected end of file` 44 times, with no `io: ` prefix. A bare `io::Error` like that comes only from the `Io(e)` arm of `map_ureq_error` (`src/provider/http.rs:454`), which runs before any response arrives. The spec's claims "The request did reach the API" and "That text sent the triage toward network causes" do not match this evidence. `.session/notes.md` F2 makes the same mistake. | Claude | Blocking |
| 2 | Even with this fix, the same outage will still print "could not reach ... unexpected end of file". AC-4 keeps `map_ureq_error` unchanged, and the spec must say this plainly. Renaming the pre-response EOF (`run.rs:584`) is a scope change that needs Max's decision. | Claude | Blocking |
| 3 | AC-1 must say that `{msg}` is the inner `io::Error` text, so the line reads `...was complete: Peer disconnected`. If the implementer passes the outer `ureq::Error` text, the line gets the `io: ` prefix. | Claude | Major |
| 4 | Every `Transport` arm must keep the outer `e.to_string()`. Otherwise `is_timeout` can no longer find "timeout" in `ureq::Error::Timeout` (AC-5). | Claude | Major |
| 5 | Step 4 must name how many `Act::TruncateBody` entries each test needs. If there are too few, the stub's 30 s accept loop ends and the next request fails as `Transport` (connection refused), which gives a false result. `timeout_mid_body` already queues two entries for one `--dry-run`. | Claude | Major |
| 6 | The step 4 stub must close the socket explicitly (`drop` or `shutdown`) after writing 10 bytes, not sleep. | Claude | Minor |
| 7 | The spec lists three edge cases: zero body bytes, chunked `0\r\n` followed by FIN, and a non-2xx `Take` read. They are correct, but only the first has a planned test. Add a stub test for zero bytes, or drop it from the list. | Claude | Minor |
| 8 | During a body read, ureq turns `ConnectionReset` and `ConnectionAborted` into `UnexpectedEof "Peer disconnected"` (`run.rs:676-686`, `:732`). Matching those two kinds is harmless, but only unit tests will exercise them. | Claude | Info |
| 9 | The matches in `vertex.rs:125`, `gemini.rs:361` and `ollama.rs:307` need a check for `_` arms. The compiler sweep in step 3 will catch them. | Claude | Info |

The reviewer confirmed these as correct: the latent defect at `http.rs:197-202` and `:257-262`, AC-1 to AC-6 being testable, the Ollama pass-through, the unchanged retry set and `is_timeout`, `scripts/acceptance.sh:1215`, and the step order 1 → 2/3 → 4 at size S.

## Consolidated Verdict
**NEEDS_REVISION.** The code change is correct and small. The problem statement's causal claim is what fails: the evidence says the reported string came from the pre-response path, not from the body read.

## Priority Actions
1. **Rewrite section 1.** Say that the observed `unexpected end of file` came through `map_ureq_error` (no `io: ` prefix), not through the 2xx body read. Correct `.session/notes.md` F2 to match. Present the change as a fix for a real latent defect, not as the fix for what Max saw.
2. **Ask Max whether the pre-response EOF text is in scope.** That is Open Question 2, and it would change `map_ureq_error`. Optionally, confirm the path first with a `GCM_DEBUG` probe.
3. **Fix the message text in AC-1.** `{msg}` must be the inner `io::Error` text, and the `Transport` arms must keep the outer `ureq::Error` text so that `is_timeout` still works.
4. **Fix step 4.** Name the number of `Act::TruncateBody` entries for each test, and close the socket explicitly instead of sleeping.
5. **Settle the zero-bytes edge case.** Add a stub test for it, or remove it from the list.
6. **Repair the review pipeline.** Fix the Gemini call so it does not print `opencode run` help, and switch the Ollama model from the retired `glm-5` to `glm-5.3-flash:cloud`.
