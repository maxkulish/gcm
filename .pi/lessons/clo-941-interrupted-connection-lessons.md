# Lessons: CLO-941 interrupted-connection error text

Durable rules from relabelling an early connection close in the HTTP provider layer.

---

## L1 - Read the shape of a transport error text to find the code path that built it

**Source incident:** CLO-941 (`docs/specs/2026-11-17-clo-941-response-interrupted-error.md`). The first spec draft assumed the reported `could not reach the <p> API: unexpected end of file` came from a 2xx body read; the spec review showed it could not, and the scope had to be widened at the checkpoint.

**Rule:** In gcm on ureq 3, the text locates the mapping arm. An `io: ` prefix means the outer `ureq::Error` was stringified (the 2xx body-read arms of `send_once` and `get_once`). No prefix means the `Io(e)` arm of `map_ureq_error`, before a response. `Peer disconnected` is ureq's own disconnect; the bare `unexpected end of file` is the io-kind text that rustls returns for a close during the TLS handshake. The text identifies the path, not the cause of the outage.

**How to apply:** Before writing a fix for a reported transport error, match the exact reported text to its arm in `src/provider/http.rs` (`map_ureq_error`, `map_body_read_error`) and to `ureq-3.3.0/src/error.rs:206-236`. Write the spec against that arm, not against the arm that first looks plausible.

---

## L2 - Preview modes never carry a fallback envelope

**Source incident:** CLO-941 spec review. Evaluation row 6 expected `--plan-only --json` to return `status: fallback` with `fallback.raw_code`; it cannot.

**Rule:** `--plan-only` makes no fallback provider call (`src/main.rs:917`), and `--dry-run` returns a `plan` envelope with `mode: single` (`src/main.rs:977`). Only a real commit is re-wrapped as a `fallback` envelope (`src/main.rs:1138`).

**How to apply:** Assert `fallback.raw_code` only in a `--yes --json` test that reaches a commit and guards the envelope with `signing_available()`, as `transition_announced_under_json` and `truncated_body_fallback_reports_interrupted_raw_code` in `tests/observability.rs` do. In preview-mode tests, assert the `plan` envelope and the stderr text.

---

## L3 - A plain TCP stub reproduces a TLS-handshake close

**Source incident:** CLO-941 regression test `tls_handshake_close_is_named_interrupted` (`tests/observability.rs`).

**Rule:** No TLS server is needed to test a handshake EOF. Point the provider base URL at the plain `TcpListener` stub with an `https://` scheme; the stub reads the ClientHello and drops the socket, and rustls reports `UnexpectedEof` with the text `unexpected end of file`.

**How to apply:** Use `stub(vec![Act::CloseBeforeResponse]).replace("http://", "https://")`. Read the request before the drop, so the close is a FIN and not a reset with unread data.
