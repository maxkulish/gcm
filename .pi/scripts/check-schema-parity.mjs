#!/usr/bin/env node
// Verify PHASE_CONFIG in .pi/extensions/orchestrate/index.ts and the
// "Required exit state" / "History events required" declarations in
// .pi/orchestrator/phases/*.md agree.
//
// Why: PHASE_CONFIG is the gate the runtime enforces. The phase
// markdown is what humans read. Drift between the two means either
// the runtime rejects work that the docs say is complete, or accepts
// work the docs claim is gated. Both are silent failures.
//
// Usage:
//   node .pi/scripts/check-schema-parity.mjs            # exit 0/1
//   node .pi/scripts/check-schema-parity.mjs --verbose  # always print

import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const PI_ROOT = join(SCRIPT_DIR, "..");
const INDEX_TS = join(PI_ROOT, "extensions/orchestrate/index.ts");
const PHASES_DIR = join(PI_ROOT, "orchestrator/phases");

const VERBOSE = process.argv.includes("--verbose");

function parsePhaseConfig(src) {
  const m = src.match(/const PHASE_CONFIG[\s\S]*?=\s*\{([\s\S]*?)\n\};/);
  if (!m) throw new Error("PHASE_CONFIG block not found in index.ts");
  const body = m[1];
  const phases = {};
  const phaseRe = /^\s{2}(\w+):\s*\{\s*\n\s*requiredFields:\s*\[([^\]]*)\],\s*\n\s*historyEvents:\s*\[([^\]]*)\]/gm;
  let match;
  while ((match = phaseRe.exec(body))) {
    const name = match[1];
    const fields = (match[2].match(/"([^"]+)"/g) || []).map((s) => s.slice(1, -1));
    const events = (match[3].match(/"([^"]+)"/g) || []).map((s) => s.slice(1, -1));
    phases[name] = { fields, events };
  }
  return phases;
}

function parsePhaseMd(name, src) {
  const result = { fields: [], events: [], optionalFields: [] };

  const reqIdx = src.indexOf("Required exit state");
  if (reqIdx >= 0) {
    const after = src.slice(reqIdx);
    const yamlMatch = after.match(/```yaml\s*\n([\s\S]*?)```/);
    if (yamlMatch) {
      const yaml = yamlMatch[1];
      const phaseRe = new RegExp(`(?:^|\\n) {2}${name}:\\s*\\n([\\s\\S]*?)(?=\\n\\S|$)`);
      const pm = yaml.match(phaseRe);
      if (pm) {
        for (const line of pm[1].split("\n")) {
          const fm = line.match(/^ {4}([a-zA-Z_][a-zA-Z0-9_]*):/);
          if (!fm) continue;
          const isOptional = /#\s*optional/i.test(line);
          if (isOptional) result.optionalFields.push(fm[1]);
          else result.fields.push(fm[1]);
        }
      }
    }
  }

  const hist = src.match(/History events required:\s*([\s\S]*?)(?:\.\s|\n\n|Optional:|$)/);
  if (hist) {
    const events = hist[1].match(/`([^`]+)`/g);
    if (events) result.events = events.map((e) => e.slice(1, -1));
  }

  return result;
}

function diff(a, b) {
  return a.filter((x) => !b.includes(x));
}

function main() {
  const indexSrc = readFileSync(INDEX_TS, "utf8");
  const codeConfig = parsePhaseConfig(indexSrc);

  const drifts = [];
  const summaries = [];

  for (const phase of Object.keys(codeConfig)) {
    const mdPath = join(PHASES_DIR, `${phase}.md`);
    let mdSrc;
    try {
      mdSrc = readFileSync(mdPath, "utf8");
    } catch {
      drifts.push({ phase, kind: "missing-md", detail: mdPath });
      continue;
    }
    const md = parsePhaseMd(phase, mdSrc);
    const code = codeConfig[phase];

    const fieldsOnlyInCode = diff(code.fields, md.fields);
    const fieldsOnlyInMd = diff(md.fields, code.fields);
    const eventsOnlyInCode = diff(code.events, md.events);
    const eventsOnlyInMd = diff(md.events, code.events);

    summaries.push({
      phase,
      codeFields: code.fields.length,
      mdFields: md.fields.length,
      codeEvents: code.events.length,
      mdEvents: md.events.length,
    });

    if (fieldsOnlyInCode.length || fieldsOnlyInMd.length) {
      drifts.push({ phase, kind: "fields", onlyInCode: fieldsOnlyInCode, onlyInMd: fieldsOnlyInMd });
    }
    if (eventsOnlyInCode.length || eventsOnlyInMd.length) {
      drifts.push({ phase, kind: "events", onlyInCode: eventsOnlyInCode, onlyInMd: eventsOnlyInMd });
    }
  }

  if (VERBOSE) {
    console.log("Phase coverage:");
    for (const s of summaries) {
      console.log(`  ${s.phase.padEnd(12)} fields code=${s.codeFields} md=${s.mdFields}  events code=${s.codeEvents} md=${s.mdEvents}`);
    }
    console.log("");
  }

  if (drifts.length === 0) {
    console.log("OK: PHASE_CONFIG agrees with phase markdown on required fields and history events.");
    process.exit(0);
  }

  console.error("DRIFT detected between PHASE_CONFIG (index.ts) and .pi/orchestrator/phases/*.md:");
  for (const d of drifts) {
    if (d.kind === "missing-md") {
      console.error(`  [${d.phase}] phase markdown missing at ${d.detail}`);
      continue;
    }
    console.error(`  [${d.phase}] ${d.kind}:`);
    if (d.onlyInCode.length) console.error(`    only in code (index.ts):  ${d.onlyInCode.join(", ")}`);
    if (d.onlyInMd.length)    console.error(`    only in markdown:         ${d.onlyInMd.join(", ")}`);
  }
  console.error("");
  console.error("Resolve by either marking docs-only fields as '# optional' inline,");
  console.error("or adding the missing fields to the corresponding side. See .pi/AGENTS.md.");
  process.exit(1);
}

main();
