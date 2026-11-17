# Spec Review: clo-941

**Reviewer**: Claude (fallback)
**Reviewed**: 2026-11-17
**Pipeline**: lok spec-review
**Note**: Both external reviewers failed; this is the fallback review

---

# Spec review: CLO-941, naming a 2xx response that breaks off

I checked the spec against the code in this worktree, the ureq 3.3.0 source and the transcript of triage session 8357d55b.

## 1. Problem Statement

**Correct:** The spec quotes the 2xx body-read paths accurately. `src/provider/http.rs:197-202` and `:257-262` map every read error to `Transport`, and `identity.rs:57` prints that as "could not reach". This defect is real.

**Blocking:** The failure in the report did not come from the code path this spec changes.

- The body-read code calls `e.to_string()` on a `ureq::Error`. ureq displays an `Io` error as `"io: {}"` (`ureq-3.3.0/src/error.rs:236`).
- When ureq finds a truncated body, it builds its own error with the text "Peer disconnected" (`error.rs:208`).
- A 2xx body that broke off would therefore print `could not reach the Vertex API: io: Peer disconnected`.
- The triage transcript shows a different string 44 times: `could not reach the {Vertex|Google|Groq} API: unexpected end of file`. It never has the `io: ` prefix and never says "Peer disconnected".
- A bare `io::Error` with no prefix is produced in only one place: the `Io(e)` arm of `map_ureq_error` (`http.rs:454`). That function handles failures **before any response arrives**.

So the CLO-941 failures were most likely a pre-response EOF, for example during TLS or the status line. The statements "The request did reach the API" and "That text sent the CLO-941 triage toward network and DNS causes" are not supported by this evidence. `.session/notes.md` F2, which names `http.rs:261`, has the same problem.

The rescoped Linear goal still holds as a fix to a real latent defect. The spec must stop presenting it as the fix for what Max saw. After this change, the same outage would still print "could not reach ... unexpected end of file".

## 2. Acceptance Criteria

- AC-1 to AC-6 are testable and match the Linear rescope.
- **AC-1 message text:** state that `{msg}` is the inner `io::Error` text, so the full line reads `...was complete: Peer disconnected`. Otherwise the implementer can pass the `ureq::Error` text and get `io: Peer disconnected`.
- **AC-4** keeps `map_ureq_error` unchanged. Given section 1, AC-4 also leaves the reported symptom unchanged, and the spec must say that in plain words.
- **AC-5:** correct. A body stall arrives as `ureq::Error::Timeout`, whose text "timeout: ..." still contains "timeout" for `is_timeout`. The helper must keep the outer `e.to_string()` for every `Transport` arm, or the `is_timeout` match breaks.

## 3. Constraints & Assumptions

- **Classifying by ureq error kind:** this is correct. Note one detail: during a body read, ureq itself turns `ConnectionReset` and `ConnectionAborted` into `remote_closed` and then reports `UnexpectedEof "Peer disconnected"` (`run.rs:676-686`, `:732`). Those two kinds reach gcm only from lower layers. Matching them is harmless, but the unit tests are the only place they are exercised.
- **Ollama pass-through, retry set unchanged, `is_timeout` unchanged:** all three agree with the code (`ollama.rs:74-80`, `identity.rs:97-104`, `diagnostics.rs:122-128`).
- **Exhaustive matches:** only `identity.rs` Display and `output.rs:259-272` are exhaustive. The matches in `vertex.rs:125`, `gemini.rs:361` and `ollama.rs:307` need a quick check for `_` arms. The compiler sweep in step 3 covers this.
- **External consumers:** no docs or schema list `raw_code` values. `scripts/acceptance.sh:1215` checks only that `raw_code` is non-empty, as the spec says.

## 4. Decomposition / Phases

- The order 1 → 2/3 → 4 is sound, and the S size fits.
- **Step 4 stub script:** state how many `Act` entries each test needs. `timeout_mid_body` queues `[StallBody, StallBody]` for one `--dry-run` run because more than one request is made. If `TruncateBody` has too few entries, the stub's 30 s accept loop ends and the second request fails as a connection refused (`Transport`). That gives a false result.
- **Step 4 socket close:** the stub must close the socket explicitly with `drop` or `shutdown` after writing 10 bytes. It must not sleep.

## 5. Risks & Open Questions

1. **Blocking:** which path produced "unexpected end of file"? It needs a probe with `GCM_DEBUG` logging, or a check of `map_ureq_error`'s `Io` arm under rustls EOF. Without that, the spec ships a label fix and the reported text stays the same.
2. Pre-response EOF ("recv_respone made no progress", `run.rs:584`) also shows "could not reach" even though a TCP connection existed. If Max wants that renamed too, it is a scope change that touches `map_ureq_error`. That needs his decision, not the implementer's.
3. Edge cases: zero body bytes, a chunked `0\r\n` followed by FIN, and a non-2xx `Take` read. These are correct against `run.rs:721-733`, but only the first one has a planned test. Add a stub case for zero bytes, or drop it from the list.

## Verdict

**NEEDS_REVISION.** The code change is correct and small. What needs revision is the problem statement's causal claim: the evidence contradicts it.

## Priority Actions

1. Rewrite section 1 to say that the observed `unexpected end of file` came through `map_ureq_error` (no `io: ` prefix), not through the 2xx body read. Fix `.session/notes.md` F2 to match. Ask Max whether the pre-response EOF text is in scope (Open Question 2).
2. In AC-1, state that `{msg}` is the inner `io::Error` text, and that the `Transport` arms keep the outer `ureq::Error` text so `is_timeout` still works.
3. In step 4, name the number of `Act::TruncateBody` entries per test and the explicit socket close.
4. Add a stub test for zero body bytes, or remove that edge case.
