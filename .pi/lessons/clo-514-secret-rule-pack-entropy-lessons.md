# Lessons: CLO-514 — Secret rule-pack + entropy engine

## L1 — Regex-based prefix migration must test bare tokens, not just keyword-context tokens

**Source incident:** CLO-514 validation gate (Codex FAIL → PASS_WITH_NOTES). The AC7 no-regression test `ac7_legacy_prefix_shapes_still_detected` passed tokens through the keyword fast path (`token=ghp_…`), not the bare prefix rule. The actual prefix rules had narrower length requirements (gitleaks live-shapes) than the old inline detector (permissive `min_len`). A bare `ghp_abcdefghijklmnopqrstuvwxyz123456` (32 trailing chars) was caught by the old engine but not by the new rule (requires exactly 36 trailing). The test masked the gap.

**Rule:** When migrating from an inline prefix/keyword detector to a data-driven rule pack, regression tests MUST exercise bare prefix tokens (no keyword context) to verify the rule itself fires, not just the keyword fast path. A test that passes `keyword=prefix_token…` through the keyword fast path does not validate the prefix rule.

**How to apply:** For any prefix migration sub-task, add a test case that passes the bare token (e.g. `ghp_abcdef…`) without a keyword assignment context. Verify the precise rule catches it, not the generic or keyword-anywhere detector.

## L2 — Keyword-anywhere compatibility pass needed when migrating from `lower.find(key)` to regex-anchored assignment

**Source incident:** CLO-514 validation gate (Codex HIGH finding). The old engine's `assignment_value_ranges` did `lower.find(key)` — keyword anywhere on the line, then the next `=`/`:` value. The new regex `assignment_re()` anchored to line start after `[+\- \t]*`. This caused `const password = "…"`, `let token = "…"`, and `"api_key": "…"` to be missed — extremely common real-world forms.

**Rule:** When replacing a substring-based keyword detector with a regex-anchored one, preserve the old behavior as a compatibility pass unless the spec explicitly narrows the scope. Declaration-prefixed (`const`, `let`, `var`) and quoted-object-key (`"password":`) forms are common in real diffs and carry real credentials.

**How to apply:** Add a `keyword_anywhere_assignment()` function that scans for sensitive keywords anywhere on the line (matching the old `lower.find(key)` behavior) as a second pass after the line-start regex. Test with `const`, `let`, `export`, and quoted-key forms.
