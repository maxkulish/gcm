# Lessons: CLO-534 — Gemini structured-output schema compatibility

Source: `docs/status/clo-534-workflow.yaml`
Date: 2026-07-07

---

## L1 - Every new structured-output path needs a provider-specific schema variant

**Source incident**: `gcm resolve` on Google/Gemini failed with HTTP 400 because the resolve path reused the generic `resolve_schema()`, which emits `additionalProperties: false`. Gemini's `generationConfig.responseSchema` only accepts the OpenAPI-3.0 subset and rejects `additionalProperties`.

**Rule**: When adding a new structured-output call, assume the provider-specific schema may differ from the OpenAI/Groq strict JSON Schema. Create the generic schema for OpenAI-compatible providers **and** a Gemini/OpenAPI-3.0 variant unless you have proof the same schema works for both.

Differences to enforce for Gemini:
- Types are upper-case: `OBJECT`, `ARRAY`, `STRING`, `INTEGER`, `NUMBER`, `BOOLEAN`.
- Nullability is `nullable: true`, never a `["string", "null"]` type union.
- **No `additionalProperties`**. It is silently ignored at best and rejected at worst.
- Use `propertyOrdering` for field order hints.

**How to apply**: Name the pair `*_schema()` (OpenAI/strict) and `gemini_*_schema()` (OpenAPI-3.0 subset), mirroring the existing `plan::schema()` / `plan::gemini_schema()` pattern. Add a unit test asserting the Gemini variant has no `additionalProperties` and uses uppercase types.

## L2 - A single shared schema is a regression risk across providers

**Source incident**: The fix for resolve was obvious once the grouping path's `gemini_schema()` was pointed out, but the resolve path had not been mirrored. Each new feature that sends a schema must explicitly decide per-provider wire format.

**Rule**: Treat `additionalProperties: false` as OpenAI-only. If you see it in a schema destined for Gemini, that's a bug.

**How to apply**: In code review, flag any new `responseSchema` / `response_format` / `format` payload that uses a shared schema without a Gemini variant. The test pattern is: `gemini_*_schema_is_openapi_subset` asserting `schema.get("additionalProperties").is_none()`.
