# Lessons: CLO-947 report publish

Durable rules from the review gates that published a report into a directory and reported success.

---

## L1 - `mv` onto an existing directory is not a failed publish

**Source incident:** CLO-947 (`docs/specs/2026-11-20-clo-947-report-publish.md`), deferred from the PR #90 review. The review gates published with `mv -f <file> <target>`. When `<target>` was a directory, `mv` moved the file into it and exited 0, so the gate passed while the next step read a path with no report.

**Rule:** Every report and diff publish in the review gates calls `review_publish` from `.lok/lib/review-guard.sh`. It refuses a directory at the target, runs `mv -f --`, and checks that the target is a regular file afterwards. A failure gives `REVIEW_PUBLISH_FAILED: <target>: <reason>` and a non-zero exit.

**How to apply:** When you add a publish step to a gate, call `review_publish` and handle its exit status. `scripts/test-review-guard.sh` fails on a bare `mv` in the gate files.

## L2 - lok rewrites the stderr of a failed step, so the reason goes to a run file

**Source incident:** CLO-947, measured on lok 20261107.0.2. lok drops the stdout of a failed step and rewrites its stderr: a line with `429`, `quota` or `not found` becomes category text, and other lines are cut to 80 characters. A target path under the slug `clo-429` showed as "rate limited".

**Rule:** A lok step that must explain a failure writes the full line to a file in the run directory and prints a short fixed line on stderr that lok passes through unchanged. The review gates use `.lok/local/review-runs/<slug>/publish-failed` and `REVIEW_PUBLISH_FAILED: see the run publish-failed file`.

**How to apply:** Before you rely on lok output to name a failure, run the step under `lok run` with a slug that contains `429` and check the line that lok prints.
