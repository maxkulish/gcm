# Lessons: PR review failures

Durable rules extracted from PR-review incidents on this repo. Phase
scripts cite this file by name rather than restating the rationale.

These rules are non-negotiable inside the `pr` phase. Loosening them
requires landing a new incident report here first.

---

## L1 - "No CI configured" is not equivalent to "no bot reviewers"

**Source incident:** PR #4 (CLO-332) merged with 7 unaddressed inline
comments after the orchestrator concluded "No CI or bot reviewers
configured" within seconds of PR creation. Gemini-code-assist had in
fact posted 7 comments roughly 90 seconds later.

**Rule:** CI presence and bot-reviewer presence are independent facts
and must be verified independently. Inferring one from the other - in
either direction - is a fatal class of error.

**How to apply:**
- Bot-reviewer presence is verified by inspecting the last 5 closed PRs
  in this repo for reviews authored by `gemini-code-assist` or
  `copilot-pull-request-reviewer`. The check lives in `pr.md` §3.5.1.5.
- "No CI configured" by itself is never a valid rationale for skipping
  the review-fetch step. If CI is absent but bots are installed, bots
  still need to be waited for.

---

## L2 - 180 seconds minimum wait before any "no comments" conclusion

**Source incident:** Same PR #4 above. The decision to skip review
processing was logged before bots had a chance to post.

**Rule:** No "no inline comments" rationale may be recorded sooner than
180 seconds after PR creation. Gemini and Copilot routinely post in the
60-180s window; an empty fetch before then is meaningless.

**How to apply:**
- `pr.md` §3.5.1 computes `MIN_WAIT_UNTIL = PR.createdAt + 180s` from
  the API timestamp - not the orchestrator's wall clock - and blocks
  the next step until that moment passes.
- This is a floor, not a target. If a bot has posted within the window,
  proceed. If it has not and bots are confirmed installed, keep waiting
  past 180s until comments arrive or a reasonable upper bound elapses
  (5-10 minutes is normal for slow Copilot runs).

---

## L3 - Every author reply ends with `/gemini review` on its own line

**Source incident:** Pre-CLO-332, replies to Copilot and human
reviewers were not re-validated by Gemini, leaving threads in
ambiguous resolved-but-unvalidated state. Several merged with
unaddressed concerns the author had implicitly declined.

**Rule:** Every author reply to ANY review comment - Gemini, Copilot,
human, anyone - ends with the `/gemini review` trailer on its own line.
Gemini is treated as the universal validator: it re-evaluates the
rationale, the fix description, or the declined suggestion regardless
of who originally posted the comment.

**How to apply:**
- `pr.md` §3.5.6 mandates the trailer on every reply template.
- `pr.md` §3.5.6.5 runs a regex gate against `gh api` reply output that
  rejects the phase if any reply since the latest push lacks the
  trailer on its own line. The regex is `(^|\n)/gemini review\s*$`.
- The trailer is not optional for "trivial" replies; the gate makes no
  exceptions.

---

## L4 - Re-fetch comments immediately before merge

**Source incident:** Late bot comments posted between the last
addressing pass and the merge button. Without a final re-fetch they
were missed.

**Rule:** Before merging, re-run the comment fetch one last time. If
new unresolved threads appeared since the last addressing pass, return
to the addressing loop. The merge button is the final gate.

**How to apply:**
- `pr.md` §5.0 (mandatory pre-merge re-fetch) hits the same
  `gh api repos/.../pulls/<n>/comments --paginate` call used in §3.5.2,
  and gates the merge command on a clean result.

---

## L5 - Stale comments are not auto-skipped

**Rule:** When the code under a comment has changed since the comment
was posted (line within 5 of `original_line` modified in
`<original_commit_id>..HEAD`), flag the comment as `[STALE?]` and
confirm with the user before acting. Do not silently drop it - the user
may want the stale feedback addressed against the new code shape.

**How to apply:** `pr.md` §3.5.4 contains the `git diff` check and
gating language.

---

## L6 - Bot-review completion is an observed review, not a timer

**Source incident:** PR #24 (CLO-382) was merged at 07:07:48Z, 261s
after PR creation. Gemini posted its review at 07:08:16Z and the
blocking inline comment at 07:08:17Z. A 180s minimum wait would not
have caught this; merging before observing the bot review missed a
high-priority inline comment.

**Rule:** For repos with installed bot reviewers, the PR phase must
wait for a bot review on the current head commit (`pull_request.head.sha`)
before concluding review fetching. A timeout is not proof that review is
clean. If installed bots do not produce a current-head review by the
10-minute deadline, block for user guidance rather than recording
`reviews_addressed: true`.

**How to apply:** `pr-review-cycle.md` step 1 polls
`repos/<owner>/<repo>/pulls/<n>/reviews` for `gemini-code-assist` or
`copilot-pull-request-reviewer` on the current head SHA, then fetches
inline comments and GraphQL review threads. Step 2 only permits a skip
when recent closed PRs confirm those bots are not installed.
