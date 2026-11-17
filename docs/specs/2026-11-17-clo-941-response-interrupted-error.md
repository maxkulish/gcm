# Spec: Name a connection that closes before a complete response

**Created**: 2026-11-17
**Linear**: CLO-941 (rescoped on 2026-11-17, widened at the spec checkpoint)
**Estimated scope**: S (5 files, 4 sub-tasks)

## 1. Problem Statement

CLO-941 reported that grouping one changed file failed with
`could not reach the <Provider> API: unexpected end of file` and fell back to
single-commit mode. The failure did not reproduce on gcm 0.8.5. The triage
binary passed 6 of 6 runs on Vertex and Groq, and Max's own one-file
`gcm --plan-only --json` runs on Vertex and Groq both returned a plan
(`.session/notes.md` F1, F2). Max rescoped the issue to the error text.

The reported text came from the `Io(e)` arm of `map_ureq_error`
(`src/provider/http.rs:454`), which handles errors before a response arrives:

- ureq prints its own `Io` errors as `io: <inner>` (`ureq-3.3.0/src/error.rs:236`).
  The 2xx body-read paths stringify the outer `ureq::Error`, so their text has that
  prefix. The reported text has no prefix.
- ureq reports its own disconnect as `Io(UnexpectedEof, "Peer disconnected")`
  (`error.rs:206-209`). The reported text, `unexpected end of file`, is the bare
  text of `io::ErrorKind::UnexpectedEof`. rustls returns that error when the peer
  closes the socket during the TLS handshake
  (rustls 0.23 `conn.rs`, around line 725).
- A localhost probe on gcm 0.8.5+605b077 that closed the socket during the TLS
  handshake printed exactly `could not reach the Groq API: unexpected end of file`.

The missing prefix identifies the code path. It does not prove that the API
received the request, and it does not identify the cause of the original outage.
The text "could not reach" is still wrong for this case: gcm reached the endpoint,
and the connection closed before a complete response arrived.

The same wrong text appears on a second path. When the server sends a 2xx status
line and headers and then closes the connection before the body is complete,
`send_once` (`src/provider/http.rs:257-262`) and `get_once`
(`src/provider/http.rs:197-202`) map the body-read error to
`ErrorKind::Transport(e.to_string())`. `ErrorKind::Transport` displays as
`could not reach the {p} API: {msg}` (`src/provider/identity.rs:57`).

How the errors arrive from ureq 3.3.0:

- A close before the response headers, on plain HTTP: `Io(UnexpectedEof,
  "Peer disconnected")` (`run.rs:584`).
- A close during the TLS handshake: `Io(UnexpectedEof)` from rustls, text
  `unexpected end of file`.
- A 2xx body that ends early: `Io(UnexpectedEof, "Peer disconnected")`
  (`run.rs:713-733`). During a body read, ureq turns `ConnectionReset` and
  `ConnectionAborted` into `UnexpectedEof` (`run.rs:676-686`).
- A reset or abort before the response: `Io` with kind `ConnectionReset` or
  `ConnectionAborted`.
- A refused connection: `Io(ConnectionRefused)`. A DNS failure: `HostNotFound`.
  A certificate or TLS protocol error: an `Io` of another kind, or another
  `ureq::Error` variant. These are real "could not reach" cases.
- A body that stalls until the timeout: a timeout error. gcm keeps it as
  `Transport("timeout ...")`, and `is_timeout` (`src/provider/diagnostics.rs:122-128`)
  turns it into the `GCM_HTTP_TIMEOUT_SECS` advice. This path must not change.

Who is affected: every gcm user on every HTTP provider, when a provider, a proxy or
the network closes a connection after it was opened. The fallback itself is correct.
Only the text and the JSON `fallback.raw_code` are wrong.

## 2. Acceptance Criteria

- [ ] AC-1: An `io::Error` of kind `UnexpectedEof`, `ConnectionReset` or
      `ConnectionAborted` produces a new `ErrorKind::Interrupted(String)`. This
      applies before the response (in `map_ureq_error`, including the TLS
      handshake) and during a 2xx body read (in `send_once` and `get_once`).
- [ ] AC-2: `Interrupted` displays as
      `the connection to the {p} API closed before a complete response was received: {msg}`.
      `{msg}` is the inner `io::Error` text (`Peer disconnected` or
      `unexpected end of file`), never the outer `ureq::Error` text with its `io: `
      prefix. The text does not contain "could not reach".
- [ ] AC-3: `fallback.raw_code` for this failure is `Interrupted` in `--json` output.
- [ ] AC-4: The failure leads to the existing fallback. It is not retried:
      `is_retryable` stays `RateLimit | Server` only. The output contract of each
      mode stays as it is today: `--plan-only` and `--dry-run` return a `plan`
      envelope with `mode: single`, and only a real commit returns a `fallback`
      envelope.
- [ ] AC-5: These failures keep `could not reach the {p} API: ...` and
      `raw_code: "Transport"`: a refused connection, a DNS failure, a certificate or
      TLS protocol error, and every other `Io` kind.
- [ ] AC-6: A 2xx body that stalls until `GCM_HTTP_TIMEOUT_SECS` keeps today's
      behavior: `Transport("timeout ...")` and the "within Ns ... GCM_HTTP_TIMEOUT_SECS"
      diagnostic. The existing test `timeout_mid_body` passes unchanged. A
      pre-response timeout stays `ErrorKind::Timeout`.
- [ ] AC-7: Regression tests in `tests/observability.rs` use the local stub server
      for both paths: a close before the response and a 2xx body that ends early.

**Verification method**: `cargo test` (unit tests for the mapping, Display text and
raw_code; integration tests from AC-7), `cargo clippy --all-targets -- -D warnings`,
`cargo fmt --check`, and `scripts/acceptance.sh` (its fallback check at line 1215
asserts only that `raw_code` is non-empty, so it needs no edit).

## 3. Constraints

**Must**:
- Classify by the `io::ErrorKind`, not by message text. One predicate decides
  whether an `io::Error` is an interruption (`UnexpectedEof | ConnectionReset |
  ConnectionAborted`). `map_ureq_error` and the 2xx body-read helper both call it.
- In `map_ureq_error`, only the `Io(e)` arm changes: an interruption becomes
  `Interrupted(e.to_string())`, and every other `Io` stays `Transport(e.to_string())`.
  `StatusCode`, `Timeout`, `HostNotFound` and the other arms do not change.
- In the 2xx body-read helper, `Io(e)` with an interruption kind becomes
  `Interrupted(e.to_string())` with the inner text. Every other body-read error keeps
  `Transport(err.to_string())` with the outer `ureq::Error` text, as today.
  `is_timeout` finds "timeout" only in that outer text (AC-6).
- Use one helper for the 2xx body-read mapping and call it from both `send_once`
  and `get_once`.
- Add the variant to every exhaustive `match` on `ErrorKind`: Display in
  `identity.rs`, `provider_error_code` in `src/output.rs:259-272`, and any match the
  compiler reports.
- Document the variant like its neighbours: "The connection closed before a
  complete response arrived: during the TLS handshake, before the headers, or
  during a 2xx body (not retried)."
- Keep the stub changes inside the existing `Act` enum of `tests/observability.rs`.

**Must-not**:
- Do not add `Interrupted` to `is_retryable`, and do not change retry counts or
  timeouts (out of scope with CLO-940 and retry tuning).
- Do not change the Ollama `Transport` wrapper (`src/provider/ollama.rs:69-80`).
  `Interrupted` passes through unwrapped, because "start `ollama serve`" is wrong
  for a server that accepted the connection. A refused connection to Ollama stays
  `Transport` and keeps that advice.
- Do not change `is_timeout`.
- Do not change the `--plan-only` or `--dry-run` envelopes, and do not add a
  provider call to `--plan-only`.
- Do not change the one-file grouping prompt or the grouping request builder.

**Prefer**:
- Unit tests next to the code they test, in the style of the existing tests in
  `identity.rs`, `output.rs` and `http.rs`.
- Integration tests sized like `timeout_mid_body` and
  `transition_announced_under_json`.

**Escalate when**:
- A stub test shows that ureq reports either case as anything other than `Io` with
  one of the three kinds above.
- A consumer outside this repository is found that matches on
  `raw_code == "Transport"` for this case.

## 4. Decomposition

1. **Variant and text**: add `ErrorKind::Interrupted(String)`, its Display line and
   its `raw_code`, with unit tests for both. Files: `src/provider/identity.rs`,
   `src/output.rs`.
2. **Mapping**: add the interruption predicate, use it in the `Io` arm of
   `map_ureq_error`, and add the 2xx body-read helper that `send_once` and
   `get_once` call. Unit-test `map_ureq_error` and the helper. File:
   `src/provider/http.rs`.
3. **Compiler sweep**: fix every other exhaustive match that the new variant breaks,
   if any (candidates: `vertex.rs`, `gemini.rs`, `ollama.rs`). Files: as the
   compiler reports.
4. **Regression tests**: add two `Act` variants to the stub.
   - `Act::CloseBeforeResponse`: read the request, then drop the socket without a
     reply.
   - `Act::TruncateBody(usize)`: write a 200 with `Content-Length: 4096`, write the
     given number of body bytes, then drop the socket. Never use a sleep.

   Tests, each on a two-file repo like `transition_announced_under_json`:
   - a. `--plan-only --json`, stub `[TruncateBody(10)]` (one entry: plan-only makes
     no fallback call). stdout: `status: plan`, `mode: single`, no `fallback` key.
     stderr: the AC-2 text, no "could not reach", `assert_plain`.
   - b. `--plan-only --json`, stub `[CloseBeforeResponse]`. Same asserts as a.
   - c. `--dry-run --yes`, stub `[TruncateBody(0), Reply(200, message_body())]`.
     stderr: the AC-2 text and "Falling back to single-commit mode.", no
     "could not reach". This covers zero body bytes.
   - d. `--yes --json`, stub `[TruncateBody(10), Reply(200, message_body())]`.
     stderr as in c. If `signing_available()`, the envelope has `status: fallback`
     and `fallback.raw_code == "Interrupted"`.
   - e. `--plan-only --json` with an `https://127.0.0.1:<port>` base URL, stub
     `[CloseBeforeResponse]`. The stub reads the TLS ClientHello and drops the
     socket, so rustls fails the handshake with `UnexpectedEof`. stderr: the AC-2
     text ending in `unexpected end of file`, no "could not reach". This pins the
     exact text from CLO-941.

   File: `tests/observability.rs`.

**Dependency order**: 1 before 2 and 3. 4 after 2.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | `map_ureq_error` on `Io(UnexpectedEof)`, `Io(ConnectionReset)`, `Io(ConnectionAborted)` | `Interrupted` with the inner text | `cargo test http::tests` |
| 2 | `map_ureq_error` on `Io(ConnectionRefused)`, `Io(InvalidData)`, `HostNotFound` | `Transport`, text unchanged | same |
| 3 | Body-read helper on the three interruption kinds | `Interrupted` with the inner text, no `io: ` prefix | same |
| 4 | Body-read helper on a timeout and `Io(PermissionDenied)` | `Transport` with the outer text, unchanged | same |
| 5 | Display of `Interrupted` for Groq | starts with "the connection to the Groq API closed before a complete response was received", no "could not reach" | `cargo test identity` |
| 6 | `provider_error_code` for `Interrupted` | `"Interrupted"` | `cargo test output` |
| 7 | Stub tests a and b (plan-only, both paths) | `plan` envelope, `mode: single`, AC-2 text on stderr | `cargo test --test observability` |
| 8 | Stub test c (dry-run, zero body bytes) | AC-2 text, fallback announced, no "could not reach" | same |
| 9 | Stub test d (real commit, `--json`) | `fallback.raw_code == "Interrupted"` when signing is available | same |
| 9b | Stub test e (`https://` URL, close during the TLS handshake) | `...closed before a complete response was received: unexpected end of file` on stderr | same |
| 10 | `timeout_mid_body`, `both_timeout_phases_name_the_budget`, `transition_announced_under_json`, Ollama connection-refused test | pass unchanged | `cargo test` |
| 11 | Full gate | all green | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` |

**Edge cases to verify**:
- The server closes the socket after the headers with zero body bytes (test c).
- During a body read, ureq turns `ConnectionReset` and `ConnectionAborted` into
  `UnexpectedEof` (`run.rs:676-686`), so only the unit tests exercise those kinds
  on that path.
- The TLS-handshake EOF: test e drives it through a plain TCP stub with an
  `https://` URL; unit test 1 covers the same `Io(UnexpectedEof)` input to
  `map_ureq_error`.
- A chunked response that ends with `0\r\n` and then a FIN: ureq treats it as
  complete (`run.rs:721-730`), so gcm must parse it normally, not report
  `Interrupted`.
- A non-2xx response whose error body breaks off: this path reads with `Take` and
  is unchanged.
