# Lessons: CLO-942 orchestrate pi extension on pi 1.5.x

Durable rules from moving `.pi/extensions/orchestrate` to pi 1.5.2.

---

## L1 - On pi 1.5.x, define string enums with `Type.Unsafe`, not the pi-ai `StringEnum`

**Source incident:** CLO-942 (`docs/specs/2026-11-18-clo-942-orchestrate-pi-upgrade.md`). After the bump, `Static<>` of the tool parameter objects inferred `unknown` for every enum field, and `tsc` failed where the code read those fields.

**Rule:** In pi 1.5.x, `StringEnum` from `@earendil-works/pi-ai` returns a pi-schema `Schema`, not a typebox schema. Inside a typebox `Type.Object`, it breaks `Static<>` inference. A local helper `Type.Unsafe<T[number]>({ type: "string", enum: [...values] })` emits the same plain-enum JSON schema, keeps the narrowed union type, and removes the pi-ai dependency.

**How to apply:** When an extension in `.pi/extensions/` moves to pi 1.5.x, replace the pi-ai `StringEnum` import with the helper in `.pi/extensions/orchestrate/index.ts:42`. Then check with a type-equality assertion that each field infers the exact union, not `unknown` or `string`.

---

## L2 - Check a pi tool's parameter validation headless

**Source incident:** CLO-942 AC9, which required proof that Pi 1.5.2 rejects an invalid `phase` at runtime.

**Rule:** `pi -p --mode json --no-session -ne -e <extension>/index.ts --tools <tool>` loads only that extension and prints `tool_execution_end` events. The events carry Pi's own validation message (`phase: expected one of [...]`) with `isError: true`. No interactive session is needed.

**How to apply:** For a schema change in a pi extension, prompt the model in this mode to call the tool with one invalid value and otherwise valid arguments, then read the `tool_execution_end` event. Point any state-changing call at a disposable ID (CLO-942 used `CLO-99942`) and delete the file it creates.

---

## L3 - After removing an npm override, read `npm ls`, not only `npm audit`

**Source incident:** CLO-942 sub-task 3, removal of the `brace-expansion` override in `.pi/extensions/orchestrate/package.json`.

**Rule:** `npm install` does not dedupe after an override is removed. It kept the existing `brace-expansion` 5.0.13 under `minimatch` and added 5.0.12 under `pi-coding-agent`. A clean audit says only that no resolved version is vulnerable; it does not show which versions were resolved.

**How to apply:** After you drop an override, run `npm ls <package>` and record the lowest resolved version next to the audit result. Compare that version with the patched floor from the advisory that the override existed for.
