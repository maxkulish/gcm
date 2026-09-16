# Lessons: lok workflow templates

## L1 - Guard `{{ steps.X.* }}` with `{% if steps.X is defined %}` when the step can be skipped

**Source incident**: CLO-799 design phase. `.lok/workflows/design-review.toml`'s
synthesis prompt referenced `{{ steps.claude_fallback.success }}` and
`{{ steps.claude_fallback.output }}`. The `claude_fallback` step has a `when`
condition that was false (both external reviewers were not needed), so the step
was skipped and the workflow aborted with
`step 'synthesis' has unknown variable '{{ steps.claude_fallback.success }}'`.
The Gemini review had already succeeded; the failure lost the whole run's
review-file writes and forced a manual reviewer invocation.

**Rule**: In a lok workflow, a step referenced by a *downstream prompt template*
must be guarded with `{% if steps.X is defined %}` (as
`pre-pr-validation.toml` does) whenever that step has a `when` condition or a
`depends_on` that can leave it unrun. A bare `{{ steps.X.success }}` or
`{{ steps.X.output }}` is a hard template error when the step is skipped, not an
empty string. Bare `.success` is only safe in a `when` *expression* (the engine
evaluates those), never inside a `{{ }}` interpolation.

**How to apply**: When adding a conditional step to a `.lok/workflows/*.toml`
workflow, grep every later prompt for `steps.<name>.` and wrap each reference in
`{% if steps.<name> is defined %}...{% else %}SKIPPED{% endif %}`. The
`write_reports` shell step needs the same guard for `'{{ steps.X.output }}'`
assignments. After editing, validate by forcing the skip path (make `when`
false) and confirming the workflow still reaches its final write step.
