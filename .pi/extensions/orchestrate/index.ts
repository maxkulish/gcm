import type { ExtensionAPI, ExtensionContext, MessageRenderer } from "@mariozechner/pi-coding-agent";
import type { AutocompleteItem } from "@mariozechner/pi-tui";
import { StringEnum } from "@mariozechner/pi-ai";
import { Text } from "@mariozechner/pi-tui";
import { Type, type Static } from "typebox";
import {
  DEFAULT_MAX_BYTES,
  DEFAULT_MAX_LINES,
  formatSize,
  truncateHead,
  withFileMutationQueue,
} from "@mariozechner/pi-coding-agent";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import * as yaml from "js-yaml";

const TASK_ID_REGEX = /^CLO-\d+$/i;

const ORCHESTRATOR_ENTRY_TYPE = "orchestrate-workflow-state";
const ORCHESTRATOR_UI_ID = "orchestrate";

const WORKFLOW_PHASES = [
  "init",
  "discovery",
  "spec",
  "operational",
  "design",
  "plan",
  "implement",
  "pr",
  "execute",
  "document",
  "complete",
  "status",
  "blocked",
] as const;

const TASK_TYPES = ["development", "specification", "operational"] as const;

type WorkflowPhase = (typeof WORKFLOW_PHASES)[number];
type TaskType = (typeof TASK_TYPES)[number];

type WorkflowUpdateMap = Record<string, unknown>;

interface OrchestratorRuntimeSummary {
  task_id?: string;
  phase?: string;
  task_type?: TaskType;
  workflow_status?: string;
  last_seen?: string;
}

interface WorkflowHistoryEvent {
  timestamp: string;
  action: string;
  phase: string;
  details: string;
}

interface TokenUsageEntry {
  recorded_at: string;
  provider: string;
  model?: string;
  prompt_tokens: number;
  completion_tokens: number;
  task_label?: string;
}

interface LinearBlock {
  team?: string;
  project?: string;
  status_at_start?: string;
  priority?: string | number;
  branch_suggested?: string;
  branch_actual?: string;
  blocks?: string[];
  blocked_by?: string[];
}

interface WorkflowState {
  task_id?: string;
  task_title?: string;
  task_url?: string;
  task_type?: TaskType;
  classification_reason?: string;
  task_profile?: {
    has_backend?: boolean;
    has_frontend?: boolean;
    has_data_model?: boolean;
    has_external_deps?: boolean;
    skip_probe?: boolean;
  };
  pending_human_action?: {
    type?: string;
    message?: string;
    since?: string;
    context?: Record<string, any>;
  } | null;
  linear?: LinearBlock;
  workflow?: {
    current_phase?: WorkflowPhase;
    status?: "active" | "blocked" | "paused" | "complete" | "in_progress" | "checkpoint";
    created_at?: string;
    updated_at?: string;
  };
  phases?: {
    discovery?: {
      status?: string;
      approved?: boolean;
      problem_statement?: string;
      selected_approach?: string;
      prior_art_searched?: boolean;
      approach_reasoning?: string;
      skipped_probe?: boolean;
      reason?: string;
      skip_reason?: string;
      token_usage?: TokenUsageEntry[];
    };
    spec?: {
      status?: string;
      spec_file?: string;
      approved?: boolean;
      auto_approved?: boolean;
      auto_approval_reason?: string;
      review_completed?: boolean;
      review_skip_reason?: string;
      review_gemini?: string | null;
      review_ollama?: string | null;
      review_synthesis?: string | null;
      review_verdict?: string | null;
      review_applied?: boolean;
      applied_suggestions?: string[];
      flagged_suggestions?: string[];
      skip_reason?: string;
      token_usage?: TokenUsageEntry[];
    };
    design?: {
      status?: string;
      reason?: string;
      skip_reason?: string;
      design_doc?: string;
      draft_ready?: boolean;
      finalized?: boolean;
      review_completed?: boolean;
      probe_completed?: string[];
      probe_decision?: string | null;
      review_gemini?: string | null;
      review_ollama?: string | null;
      review_verdict?: string | null;
      review_applied?: boolean;
      applied_suggestions?: string[];
      flagged_suggestions?: string[];
      token_usage?: TokenUsageEntry[];
    };
    plan?: {
      status?: string;
      reason?: string;
      skip_reason?: string;
      plan_file?: string;
      approved?: boolean;
      token_usage?: TokenUsageEntry[];
    };
    implement?: {
      status?: string;
      last_phase_completed?: string;
      commits?: string[];
      codex_validated?: boolean;
      codex_verdict?: string;
      codex_report?: string;
      gemini_validation_report?: string;
      validation_synthesis_report?: string;
      validation_synthesis_verdict?: string;
      validation_fix_iteration_count?: number;
      token_usage?: TokenUsageEntry[];
    };
    pr?: {
      status?: string;
      pr_url?: string;
      pr_number?: number | string;
      ci_passed?: boolean;
      bot_review_wait_completed?: boolean;
      bot_review_wait_completed_at?: string;
      reviews_addressed?: boolean;
      reviews_addressed_skip_reason?: string;
      pre_merge_refetch_passed?: boolean;
      pre_merge_refetch_at?: string;
      approved?: boolean;
      merged_at?: string | null;
      merge_commit?: string | null;
    };
    complete?: {
      status?: string;
      aggregation_files_updated?: boolean;
      aggregation_files_skip_reason?: string;
      merged_at?: string | null;
      completed_at?: string | null;
      lessons_learned?: string[];
      lessons_file?: string;
      lessons_skip_reason?: string;
    };
    execute?: {
      status?: string;
      findings?: string;
      steps_completed?: string[];
    };
    document?: {
      status?: string;
      doc_file?: string;
      lessons_learned?: string[];
    };
  };
  history?: WorkflowHistoryEvent[];
}

const TokenUsageInput = Type.Object({
  provider: Type.String({ description: "Provider id (gemini, codex, claude, ollama, ...)" }),
  model: Type.Optional(Type.String({ description: "Model identifier as reported by the provider" })),
  prompt_tokens: Type.Number({ description: "Tokens consumed by the prompt (input)" }),
  completion_tokens: Type.Number({ description: "Tokens consumed by the completion (output)" }),
  task_label: Type.Optional(Type.String({ description: "Short label distinguishing the call (e.g. 'design-review', 'codex-validation')" })),
});

const UpdateWorkflowStateParams = Type.Object({
  task_id: Type.String({ description: "Task ID (e.g. CLO-XX)" }),
  phase: StringEnum(WORKFLOW_PHASES),
  action: Type.String({ description: "History action type (e.g. pre_flight_checks_passed, pr_created)" }),
  details: Type.String({ description: "Details about the action" }),
  field_updates: Type.Optional(Type.Record(Type.String(), Type.Any())),
  phase_updates: Type.Optional(Type.Record(Type.String(), Type.Any())),
  workflow_updates: Type.Optional(Type.Record(Type.String(), Type.Any())),
  linear_updates: Type.Optional(Type.Record(Type.String(), Type.Any())),
  root_updates: Type.Optional(Type.Record(Type.String(), Type.Any())),
  token_usage: Type.Optional(Type.Array(TokenUsageInput, {
    description: "Append one or more token usage records to phases.<phase>.token_usage. Observational only; not validated against PHASE_CONFIG.",
  })),
});

const TransitionPhaseParamsSchema = Type.Object({
  task_id: Type.String({ description: "Task ID (e.g. CLO-XX)" }),
  from_phase: StringEnum(WORKFLOW_PHASES),
  to_phase: StringEnum(WORKFLOW_PHASES),
  validation_override: Type.Optional(Type.Boolean({ description: "Skip validation (use with caution)" })),
});

type UpdateWorkflowStateParams = Static<typeof UpdateWorkflowStateParams>;
type TransitionPhaseParams = Static<typeof TransitionPhaseParamsSchema>;

const ALLOWED_TRANSITIONS: Record<string, WorkflowPhase[]> = {
  init: ["discovery", "spec", "operational"],
  discovery: ["design"],
  spec: ["implement"],
  operational: ["execute", "document", "complete"],
  design: ["plan"],
  plan: ["implement"],
  implement: ["pr"],
  pr: ["complete"],
  execute: ["document", "complete"],
  document: ["complete", "pr"],
  complete: [],
};

const TYPE_ALLOWED_PHASES: Record<string, Set<WorkflowPhase>> = {
  development: new Set(["init", "discovery", "design", "plan", "implement", "pr", "complete"]),
  specification: new Set(["init", "spec", "implement", "pr", "complete"]),
  operational: new Set(["init", "operational", "execute", "document", "pr", "complete"]),
};

const PHASE_CONFIG: Record<string, {
  requiredFields: string[];
  historyEvents: string[];
  requiredTrueFields?: string[];
  auto_dispatch_after_transition?: boolean;
}> = {
  discovery: {
    requiredFields: ["status"],
    historyEvents: ["discovery_approved"],
    auto_dispatch_after_transition: false,
  },
  spec: {
    requiredFields: ["status", "spec_file", "approved", "review_completed"],
    historyEvents: ["spec_approved"],
  },
  design: {
    requiredFields: ["status", "design_doc", "draft_ready", "finalized", "review_completed", "human_review_completed", "assumptions_surfaced"],
    historyEvents: ["design_draft_ready", "design_assumptions_surfaced", "design_review_complete", "design_human_review_complete", "design_finalized"],
    auto_dispatch_after_transition: false,
  },
  plan: {
    requiredFields: ["status", "plan_file", "approved"],
    historyEvents: ["plan_created", "plan_approved"],
  },
  implement: {
    requiredFields: [
      "status",
      "assumptions_revalidated",
      "codex_validated",
      "codex_report",
      "gemini_validation_report",
      "validation_synthesis_report",
      "validation_synthesis_verdict",
    ],
    historyEvents: ["implementation_complete", "assumptions_revalidated", "codex_validation_complete"],
  },
  pr: {
    requiredFields: [
      "status",
      "pr_url",
      "pr_number",
      "ci_passed",
      "bot_review_wait_completed",
      "bot_review_wait_completed_at",
      "reviews_addressed",
      "pre_merge_refetch_passed",
      "pre_merge_refetch_at",
    ],
    historyEvents: [
      "pre_flight_checks_passed",
      "pr_created",
      "ci_passed",
      "bot_review_wait_completed",
      "review_addressed",
      "pre_merge_refetch_passed",
    ],
    requiredTrueFields: ["ci_passed", "bot_review_wait_completed", "reviews_addressed", "pre_merge_refetch_passed"],
  },
  operational: {
    requiredFields: ["status"],
    historyEvents: ["operational_started"],
  },
  execute: {
    requiredFields: ["status"],
    historyEvents: ["execution_complete"],
  },
  document: {
    requiredFields: ["status"],
    historyEvents: ["documentation_complete"],
  },
};

const runtime: {
  summary: OrchestratorRuntimeSummary;
  api: ExtensionAPI | null;
} = {
  summary: {},
  api: null,
};

function validateTaskId(taskId: string): boolean {
  return TASK_ID_REGEX.test(taskId);
}

function getWorkflowPath(workspaceRoot: string, taskId: string): string {
  const normalizedId = taskId.toLowerCase().replace(/^clo-/, "");
  if (!/^\d+$/.test(normalizedId)) {
    throw new Error(`Invalid task ID format: ${taskId}`);
  }
  return path.join(workspaceRoot, `docs/status/clo-${normalizedId}-workflow.yaml`);
}

function getPhasePath(workspaceRoot: string, phase: string): string {
  return path.join(workspaceRoot, ".pi", "orchestrator", "phases", `${phase}.md`);
}

function readWorkflowState(statePath: string): WorkflowState {
  if (!fs.existsSync(statePath)) {
    return {};
  }

  const fileContent = fs.readFileSync(statePath, "utf8");
  const loaded = yaml.load(fileContent);
  if (loaded === null || loaded === undefined) {
    return {};
  }
  if (typeof loaded !== "object") {
    throw new Error(`Invalid workflow file format: ${statePath}`);
  }
  return loaded as WorkflowState;
}

function writeWorkflowState(statePath: string, state: WorkflowState): void {
  const dir = path.dirname(statePath);
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  fs.writeFileSync(statePath, yaml.dump(state, { lineWidth: 120 }), "utf8");
}

async function mutateWorkflowState(
  statePath: string,
  mutator: (state: WorkflowState) => void | Promise<void>,
): Promise<WorkflowState> {
  return withFileMutationQueue(statePath, async () => {
    const state = readWorkflowState(statePath);
    try {
      await mutator(state);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      throw new Error(`Workflow mutator failed for ${statePath}: ${message}`);
    }
    writeWorkflowState(statePath, state);
    return state;
  });
}

function initializeWorkflow(taskId: string, flags: Set<string>): WorkflowState {
  let taskType: TaskType = "development";
  let initialPhase: WorkflowPhase = "init";

  if (flags.has("--ops")) {
    taskType = "operational";
    initialPhase = "operational";
  } else if (flags.has("--spec")) {
    taskType = "specification";
    initialPhase = "spec";
  }

  const now = new Date().toISOString();

  let phases: WorkflowState["phases"] = {};
  if (taskType === "development") {
    phases = {
      discovery: { status: "pending" },
      design: { status: "pending" },
      plan: { status: "pending" },
      implement: { status: "pending" },
      pr: { status: "pending" },
      complete: { status: "pending" },
    };
  } else if (taskType === "specification") {
    phases = {
      discovery: { status: "skipped", skip_reason: "Specification task", approved: true },
      spec: { status: "pending" },
      design: { status: "skipped", skip_reason: "Specification task - using /spec instead of full design doc" },
      plan: { status: "skipped", skip_reason: "Specification task - spec decomposition is the plan" },
      implement: { status: "pending" },
      pr: { status: "pending" },
      complete: { status: "pending" },
    };
  } else {
    phases = {
      execute: { status: "pending" },
      document: { status: "pending" },
      complete: { status: "pending" },
    };
  }

  return {
    task_id: taskId,
    task_type: taskType,
    classification_reason: "",
    task_profile: {
      has_backend: false,
      has_frontend: false,
      has_data_model: false,
      has_external_deps: false,
      skip_probe: false,
    },
    pending_human_action: null,
    linear: {
      team: "Cloud-ai",
      project: "Personal-health",
      status_at_start: "Backlog",
      blocks: [],
      blocked_by: [],
    },
    workflow: {
      current_phase: initialPhase,
      status: "active",
      created_at: now,
      updated_at: now,
    },
    phases,
    history: [{
      timestamp: now,
      action: "workflow_started",
      phase: "init",
      details: `Workflow initialized for ${taskId} as ${taskType}`,
    }],
  };
}

function validatePhase(state: WorkflowState): { valid: boolean; errors: string[] } {
  const currentPhase = state.workflow?.current_phase;
  if (!currentPhase) return { valid: false, errors: ["No current_phase set"] };
  if (currentPhase === "init" || currentPhase === "complete") return { valid: true, errors: [] };

  const config = PHASE_CONFIG[currentPhase];
  if (!config) return { valid: true, errors: [] };

  const errors: string[] = [];
  const phaseData = state.phases?.[currentPhase as keyof typeof state.phases];

  if ((phaseData as any)?.status !== "complete") {
    return { valid: true, errors: [] };
  }

  for (const field of config.requiredFields) {
    const value = (phaseData as Record<string, unknown> | undefined)?.[field];
    if (value === undefined || value === null || value === "") {
      errors.push(`Missing required field: ${currentPhase}.${field}`);
    }
  }

  for (const field of config.requiredTrueFields || []) {
    const value = (phaseData as Record<string, unknown> | undefined)?.[field];
    if (value !== true) {
      errors.push(`Required true field is not true: ${currentPhase}.${field}`);
    }
  }

  const historyActions = new Set(state.history?.map((entry) => entry.action) || []);
  for (const event of config.historyEvents) {
    if (!historyActions.has(event)) {
      errors.push(`Missing required history event: ${event}`);
    }
  }

  return { valid: errors.length === 0, errors };
}

function validatePhaseTransition(state: WorkflowState, from: string, to: string): { valid: boolean; errors: string[] } {
  const errors: string[] = [];
  const currentPhase = state.workflow?.current_phase;

  if (from !== currentPhase) {
    errors.push(`Current phase mismatch: workflow is at "${currentPhase}", but attempting to transition from "${from}"`);
  }

  const allowedNext = ALLOWED_TRANSITIONS[from] || [];
  if (!allowedNext.includes(to as WorkflowPhase)) {
    errors.push(`Invalid transition: cannot move from "${from}" to "${to}". Allowed next phases: ${allowedNext.join(", ") || "none"}`);
  }

  const taskType = state.task_type || "development";
  const allowedPhases = TYPE_ALLOWED_PHASES[taskType];
  if (allowedPhases && !allowedPhases.has(to as WorkflowPhase)) {
    errors.push(`Phase "${to}" is not valid for task type "${taskType}"`);
  }

  const fromConfig = PHASE_CONFIG[from];
  if (fromConfig) {
    const phaseData = state.phases?.[from as keyof typeof state.phases];
    const phaseStatus = (phaseData as any)?.status;
    if (phaseStatus !== "complete" && phaseStatus !== "skipped") {
      errors.push(`Phase "${from}" not complete: status is "${phaseStatus || "undefined"}", expected "complete" or "skipped"`);
    }

    if (phaseStatus !== "skipped") {
      for (const field of fromConfig.requiredFields) {
        const value = (phaseData as Record<string, unknown> | undefined)?.[field];
        if (value === undefined || value === null || value === "") {
          errors.push(`Phase "${from}" missing required field: ${field}`);
        }
      }

      for (const field of fromConfig.requiredTrueFields || []) {
        const value = (phaseData as Record<string, unknown> | undefined)?.[field];
        if (value !== true) {
          errors.push(`Phase "${from}" required true field is not true: ${field}`);
        }
      }

      const historyActions = new Set(state.history?.map((entry) => entry.action) || []);
      for (const event of fromConfig.historyEvents) {
        if (!historyActions.has(event)) {
          errors.push(`Phase "${from}" missing required history event: ${event}`);
        }
      }
    }
  }

  return { valid: errors.length === 0, errors };
}

function addHistoryEvent(state: WorkflowState, action: string, phase: string, details: string) {
  if (!state.history) state.history = [];
  state.history.push({
    timestamp: new Date().toISOString(),
    action,
    phase,
    details,
  });
}

function deepMerge(target: Record<string, unknown>, source: Record<string, unknown>): void {
  for (const key of Object.keys(source)) {
    const sourceValue = source[key];
    const targetValue = target[key];

    if (Array.isArray(targetValue) && Array.isArray(sourceValue)) {
      target[key] = targetValue.concat(sourceValue);
    } else if (
      sourceValue &&
      typeof sourceValue === "object" &&
      !Array.isArray(sourceValue) &&
      targetValue &&
      typeof targetValue === "object" &&
      !Array.isArray(targetValue)
    ) {
      deepMerge(targetValue as Record<string, unknown>, sourceValue as Record<string, unknown>);
    } else {
      target[key] = sourceValue;
    }
  }
}

function notifyCtx(
  pi: ExtensionAPI,
  ctx: Pick<ExtensionContext, "ui" | "hasUI"> | undefined,
  message: string,
  level: "info" | "warning" | "error" = "info",
): void {
  if (ctx?.hasUI) {
    ctx.ui.notify(message, level);
    return;
  }
  pi.sendUserMessage(message, { deliverAs: "followUp" });
}

function buildSessionName(summary: OrchestratorRuntimeSummary): string | undefined {
  if (!summary.task_id) return undefined;
  const phase = summary.phase || "init";
  return `${summary.task_id} • ${phase}`;
}

function updateRuntimeUi(ctx: Pick<ExtensionContext, "ui" | "hasUI"> | undefined, summary: OrchestratorRuntimeSummary): void {
  if (!ctx?.hasUI) return;

  if (summary.task_id) {
    const phase = summary.phase || "unknown";
    const status = summary.workflow_status || "unknown";
    const taskType = summary.task_type || "unknown";

    ctx.ui.setStatus(
      ORCHESTRATOR_UI_ID,
      `${summary.task_id} • ${phase} • ${status}`,
    );
    ctx.ui.setWidget(ORCHESTRATOR_UI_ID, [
      `Orchestrator: ${summary.task_id}`,
      `Type: ${taskType}`,
      `Phase: ${phase}`,
      `Status: ${status}`,
    ]);
  } else {
    ctx.ui.setStatus(ORCHESTRATOR_UI_ID, undefined);
    ctx.ui.setWidget(ORCHESTRATOR_UI_ID, undefined);
  }
}

function restoreRuntimeFromSession(ctx: ExtensionContext): OrchestratorRuntimeSummary | undefined {
  const branch = ctx.sessionManager.getBranch();

  for (let i = branch.length - 1; i >= 0; i--) {
    const entry = branch[i];
    if (entry.type !== "custom" || entry.customType !== ORCHESTRATOR_ENTRY_TYPE) {
      continue;
    }

    const data = entry.data as OrchestratorRuntimeSummary | undefined;
    if (data?.task_id) {
      return {
        task_id: data.task_id,
        phase: data.phase,
        task_type: data.task_type,
        workflow_status: data.workflow_status,
        last_seen: data.last_seen,
      };
    }
  }

  return undefined;
}

function persistRuntimeState(taskId: string, state: WorkflowState): void {
  if (!runtime.api) return;

  runtime.summary = {
    task_id: taskId,
    phase: state.workflow?.current_phase,
    task_type: state.task_type,
    workflow_status: state.workflow?.status,
    last_seen: new Date().toISOString(),
  };

  runtime.api.appendEntry(ORCHESTRATOR_ENTRY_TYPE, runtime.summary);

  const desiredName = buildSessionName(runtime.summary);
  if (desiredName && runtime.api.getSessionName() !== desiredName) {
    runtime.api.setSessionName(desiredName);
  }
}

function cachePersistedState(summary: OrchestratorRuntimeSummary): void {
  runtime.summary = summary;
}

function savePhaseOutputForDebug(fullOutput: string): string {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "pi-orchestrate-"));
  const tmpFile = path.join(tmpDir, "phase-instructions.md");
  fs.writeFileSync(tmpFile, fullOutput, "utf8");
  return tmpFile;
}

function formatPhasePrompt(
  taskId: string,
  phase: string,
  state: WorkflowState,
  statePath: string,
  phaseInstructions: string,
): string {
  const truncation = truncateHead(phaseInstructions, {
    maxLines: DEFAULT_MAX_LINES,
    maxBytes: DEFAULT_MAX_BYTES,
  });

  const phaseBody = truncation.truncated
    ? `${truncation.content}\n\n[Phase instructions truncated: ${truncation.outputLines}/${truncation.totalLines} lines`
      + ` (${formatSize(truncation.outputBytes)} / ${formatSize(truncation.totalBytes)}).`
      + ` Full output saved to: ${savePhaseOutputForDebug(phaseInstructions)}]`
    : truncation.content;

  return `
You are executing the gcm task orchestrator for ${taskId}.

Current State:
- Task ID: ${taskId}
- Current Phase: ${phase}
- Task Type: ${state.task_type || "unknown"}
- Workflow Status: ${state.workflow?.status || "unknown"}
- Linear Project: ${state.linear?.project || "gcm"} / Team: ${state.linear?.team || "Cloud-ai"}

State File: ${statePath}

Schema parity: This YAML must remain 100% compatible with the Claude flow at
.claude/commands/task/orchestrate.md so a task started in Claude can resume in pi
(and vice-versa). Do not invent new top-level keys; reuse existing ones.

Instructions for this phase:
---
${phaseBody}
---

Tools available:
- update_workflow_state, transition_phase (orchestrator)
- mcp__linear__* (Linear bridge) - for Linear issue/comment updates
- bash / shell, git, gh (host) - REQUIRED for steps that use \`gh api ...\`,
  \`gh pr ...\`, \`git push\`, \`cargo ...\`, \`make ...\`, etc. Step 3.5 of pr.md
  is shell-only; do not skip it just because no orchestrator tool covers it.
- Read / Edit / Write (host) - for editing files referenced by review comments
  or for any in-repo file changes the phase asks for.

Always use real newlines in Linear MCP body fields (no literal \\n).

Begin executing these instructions now. Update state after every significant action.
  `.trim();
}

function parseArgs(args: string): { taskId?: string; flags: Set<string> } {
  const argsList = args.split(/\s+/).filter(Boolean);
  const taskId = argsList.find((arg) => arg.toLowerCase().startsWith("clo-"));
  return { taskId, flags: new Set(argsList.filter((arg) => arg.startsWith("--"))) };
}

const FLAG_COMPLETIONS: AutocompleteItem[] = [
  { value: "--status", label: "--status", description: "Show current status without dispatching phase" },
  { value: "--ops", label: "--ops", description: "Treat task as operational workflow" },
  { value: "--spec", label: "--spec", description: "Treat task as specification workflow" },
  { value: "--skip-discovery", label: "--skip-discovery", description: "Skip discovery phase (jump to design)" },
];

function listWorkflowCompletions(workspaceRoot: string, prefix: string): AutocompleteItem[] {
  const dir = path.join(workspaceRoot, "docs/status");
  if (!fs.existsSync(dir)) return [];

  const lowered = prefix.toLowerCase();
  const items: AutocompleteItem[] = [];

  for (const file of fs.readdirSync(dir)) {
    const match = /^clo-(\d+)-workflow\.yaml$/i.exec(file);
    if (!match) continue;
    const taskId = `CLO-${match[1]}`;
    if (lowered && !taskId.toLowerCase().startsWith(lowered)) continue;

    let description: string | undefined;
    try {
      const state = readWorkflowState(path.join(dir, file));
      const phase = state.workflow?.current_phase || "?";
      const status = state.workflow?.status || "?";
      const title = state.task_title ? ` — ${state.task_title}` : "";
      description = `${phase} • ${status}${title}`;
    } catch {
      // skip malformed state file silently — completion is best-effort
    }

    items.push({ value: taskId, label: taskId, description });
  }

  items.sort((a, b) => {
    const an = parseInt(a.value.replace(/^CLO-/i, ""), 10);
    const bn = parseInt(b.value.replace(/^CLO-/i, ""), 10);
    return bn - an;
  });
  return items;
}

const orchestratorMessageRenderer: MessageRenderer<OrchestratorRuntimeSummary> = (message, _options, theme) => {
  const summary = (message.details ?? {}) as OrchestratorRuntimeSummary;
  if (!summary.task_id) return undefined;

  const phase = summary.phase || "unknown";
  const status = summary.workflow_status || "unknown";
  const taskType = summary.task_type || "unknown";

  let line = theme.fg("toolTitle", theme.bold(`Orchestrator ${summary.task_id}`));
  line += " ";
  line += theme.fg("accent", `${phase}`);
  line += theme.fg("muted", ` • ${status}`);
  line += theme.fg("dim", ` (${taskType})`);
  return new Text(line, 0, 0);
};

export default function (pi: ExtensionAPI) {
  runtime.api = pi;

  pi.registerMessageRenderer<OrchestratorRuntimeSummary>(ORCHESTRATOR_ENTRY_TYPE, orchestratorMessageRenderer);

  pi.on("session_start", async (_event, ctx: ExtensionContext) => {
    const restored = restoreRuntimeFromSession(ctx);
    cachePersistedState(restored || {});
    updateRuntimeUi(ctx, runtime.summary);
  });

  pi.on("resources_discover", async (_event, ctx: ExtensionContext) => {
    const phaseDir = path.join(ctx.cwd, ".pi", "orchestrator", "phases");
    if (fs.existsSync(phaseDir)) {
      return { promptPaths: [phaseDir] };
    }
    return {};
  });

  pi.on("before_agent_start", async (event, _ctx) => {
    if (!runtime.summary.task_id) {
      return;
    }

    const contextLine = `Orchestrator context: ${runtime.summary.task_id} • ${
      runtime.summary.phase || "unknown"
    } • ${runtime.summary.workflow_status || "unknown"}`;

    return {
      systemPrompt: `${event.systemPrompt}\n\n${contextLine}`,
      message: {
        customType: ORCHESTRATOR_ENTRY_TYPE,
        content: contextLine,
        display: false,
      },
    };
  });

  pi.registerCommand("task:orchestrate", {
    description: "Complete Task Lifecycle Management - Orchestrate CLO-XX workflows (gcm / Linear)",
    getArgumentCompletions: (argumentPrefix: string) => {
      const trimmed = argumentPrefix.trimStart();
      if (trimmed.startsWith("--")) {
        return FLAG_COMPLETIONS.filter((item) => item.value.startsWith(trimmed));
      }
      return listWorkflowCompletions(process.cwd(), trimmed);
    },
    handler: async (args: string, ctx: ExtensionContext) => {
      const { taskId, flags } = parseArgs(args);

      if (flags.has("--status")) {
        if (!taskId) {
          notifyCtx(pi, ctx, "Please provide a task ID for status check", "error");
          return;
        }
        if (!validateTaskId(taskId)) {
          notifyCtx(pi, ctx, `Invalid task ID format: ${taskId}. Must match CLO-XX pattern.`, "error");
          return;
        }
        await showStatus(pi, taskId, ctx);
        return;
      }

      if (!taskId) {
        notifyCtx(pi, ctx, "Usage: /task:orchestrate CLO-XX [--status] [--ops] [--spec] [--skip-discovery]", "error");
        return;
      }

      if (!validateTaskId(taskId)) {
        notifyCtx(pi, ctx, `Invalid task ID format: ${taskId}. Must match CLO-XX pattern.`, "error");
        return;
      }

      const workspaceRoot = process.cwd();
      const statePath = getWorkflowPath(workspaceRoot, taskId);
      let stateModified = false;

      const state = await mutateWorkflowState(statePath, (draft) => {
        if (!draft.workflow) {
          Object.assign(draft, initializeWorkflow(taskId, flags));
          stateModified = true;
        }

        if (flags.has("--ops")) {
          if (draft.task_type !== "operational" || draft.workflow?.current_phase !== "operational") {
            draft.task_type = "operational";
            draft.workflow!.current_phase = "operational";
            addHistoryEvent(draft, "workflow_modified", "operational", "Switched to operational workflow via --ops flag");
            stateModified = true;
          }
        } else if (flags.has("--spec")) {
          if (draft.task_type !== "specification" || draft.workflow?.current_phase !== "spec") {
            draft.task_type = "specification";
            draft.workflow!.current_phase = "spec";
            addHistoryEvent(draft, "workflow_modified", "spec", "Switched to spec workflow via --spec flag");
            stateModified = true;
          }
        }

        if (flags.has("--skip-discovery")) {
          if (draft.workflow?.current_phase === "init" || draft.workflow?.current_phase === "discovery") {
            if (!draft.phases) draft.phases = {};
            draft.phases.discovery = {
              status: "skipped",
              approved: true,
              skip_reason: "--skip-discovery flag",
            };
            draft.workflow!.current_phase = "design";
            addHistoryEvent(draft, "discovery_skipped", "discovery", "Skipped via --skip-discovery flag");
            addHistoryEvent(draft, "discovery_approved", "discovery", "Auto-approved skip");
            stateModified = true;
          }
        }

        if (stateModified) {
          if (!draft.workflow) {
            throw new Error(`Workflow initialization failed for ${taskId}`);
          }
          draft.workflow.updated_at = new Date().toISOString();
        }
      });

      const validation = validatePhase(state);
      if (!validation.valid) {
        notifyCtx(
          pi,
          ctx,
          `Cannot mark phase complete - missing exit-state requirements: ${validation.errors.join(", ")}. Set status back to "in_progress" and populate the missing fields/events first.`,
          "error",
        );
      }

      updateRuntimeUi(ctx, {
        task_id: taskId,
        phase: state.workflow?.current_phase,
        task_type: state.task_type,
        workflow_status: state.workflow?.status,
      });
      persistRuntimeState(taskId, state);
      await dispatchPhase(pi, taskId, state.workflow?.current_phase || "init", state, statePath, workspaceRoot);
    },
  });

  pi.registerTool({
    name: "update_workflow_state",
    label: "Update Workflow State",
    description: "Safely update the CLO workflow YAML with new state. Supports phase, workflow, linear, and root-level updates.",
    promptSnippet: "Update orchestrator workflow state for one task phase.",
    promptGuidelines: [
      "Use update_workflow_state to record phase progress and history events in workflow checkpoints.",
      "Prefer transition_phase for moving between phases instead of editing workflow.current_phase manually.",
    ],
    parameters: UpdateWorkflowStateParams,
    prepareArguments(args: any) {
      if (!args || typeof args !== "object") return args;
      const input = args as { task?: string; [key: string]: unknown };
      if (typeof input.task === "string" && input.task_id === undefined) {
        return { ...input, task_id: input.task };
      }
      return args;
    },
    async execute(_toolCallId: string, params: UpdateWorkflowStateParams, _signal, _onUpdate, ctx: ExtensionContext) {
      if (!validateTaskId(params.task_id)) {
        throw new Error(`Invalid task ID format: ${params.task_id}`);
      }

      const statePath = getWorkflowPath(process.cwd(), params.task_id);
      const state = await mutateWorkflowState(statePath, (draft) => {
        if (!draft.workflow) {
          throw new Error(`No workflow found for ${params.task_id}`);
        }

        addHistoryEvent(draft, params.action, params.phase, params.details);

        if (params.workflow_updates) {
          const updates = params.workflow_updates as WorkflowUpdateMap;
          const {
            current_phase,
            status,
            created_at,
            updated_at,
            ...rootFields
          } = updates as {
            current_phase?: WorkflowPhase;
            status?: string;
            created_at?: string;
            updated_at?: string;
          };

          if (current_phase !== undefined) draft.workflow!.current_phase = current_phase;
          if (status !== undefined) draft.workflow!.status = status as WorkflowState["workflow"]["status"];
          if (created_at !== undefined) draft.workflow!.created_at = created_at;
          draft.workflow!.updated_at = updated_at !== undefined ? updated_at : new Date().toISOString();
          Object.assign(draft, rootFields);
        } else {
          draft.workflow.updated_at = new Date().toISOString();
        }

        if (params.phase_updates) {
          if (!draft.phases) draft.phases = {};
          const phase = draft.phases as Record<string, WorkflowUpdateMap>;
          phase[params.phase] = {
            ...(phase[params.phase] || {}),
            ...params.phase_updates,
          };
        }

        if (params.field_updates) {
          if (!draft.phases) draft.phases = {};
          const phase = draft.phases as Record<string, WorkflowUpdateMap>;
          phase[params.phase] = {
            ...(phase[params.phase] || {}),
            ...params.field_updates,
          };
        }

        if (params.token_usage && params.token_usage.length > 0) {
          if (!draft.phases) draft.phases = {};
          const phases = draft.phases as Record<string, WorkflowUpdateMap>;
          const target = phases[params.phase] || (phases[params.phase] = {});
          const existing = (target.token_usage as TokenUsageEntry[] | undefined) ?? [];
          const recordedAt = new Date().toISOString();
          const appended = params.token_usage.map((entry) => ({
            recorded_at: recordedAt,
            provider: entry.provider,
            ...(entry.model !== undefined ? { model: entry.model } : {}),
            prompt_tokens: entry.prompt_tokens,
            completion_tokens: entry.completion_tokens,
            ...(entry.task_label !== undefined ? { task_label: entry.task_label } : {}),
          })) as TokenUsageEntry[];
          target.token_usage = [...existing, ...appended];
        }

        if (params.linear_updates) {
          if (!draft.linear) draft.linear = {};
          deepMerge(draft.linear as WorkflowUpdateMap, params.linear_updates as WorkflowUpdateMap);
        }

        if (params.root_updates) {
          deepMerge(draft as unknown as WorkflowUpdateMap, params.root_updates as WorkflowUpdateMap);
        }
      });

      updateRuntimeUi(ctx, {
        task_id: params.task_id,
        phase: state.workflow?.current_phase,
        task_type: state.task_type,
        workflow_status: state.workflow?.status,
      });
      persistRuntimeState(params.task_id, state);

      return {
        content: [{ type: "text", text: `Workflow state updated: ${params.action}` }],
        details: {
          task_id: params.task_id,
          phase: params.phase,
          action: params.action,
          new_phase: state.workflow?.current_phase,
          workflow_status: state.workflow?.status,
        },
      };
    },

    renderCall(args: UpdateWorkflowStateParams, theme: any, _context: any) {
      let text = theme.fg("toolTitle", theme.bold("update_workflow_state "));
      text += theme.fg("accent", `${args.task_id} ${args.phase}`);
      text += theme.fg("dim", ` → ${args.action}`);
      return new Text(text, 0, 0);
    },

    renderResult(result: any, options: any, theme: any, _context: any) {
      if (options?.isPartial) {
        return new Text(theme.fg("warning", "Updating workflow state..."), 0, 0);
      }
      return new Text(
        `${theme.fg("success", "✓")} ${theme.fg("muted", `${result.details?.task_id || "workflow"} ${result.details?.phase || ""}`)}`,
        0,
        0,
      );
    },
  });

  pi.registerTool({
    name: "transition_phase",
    label: "Transition Phase",
    description: "Transition workflow to the next phase with strict state machine validation (gcm rules)",
    promptSnippet: "Transition a CLO workflow to the next allowed phase.",
    promptGuidelines: [
      "Use transition_phase only when current phase requirements (status + required history/events) are complete.",
      "Set validation_override only for emergency/manual unblocks.",
    ],
    parameters: TransitionPhaseParamsSchema,
    async execute(_toolCallId: string, params: TransitionPhaseParams, _signal, _onUpdate, ctx: ExtensionContext) {
      if (!validateTaskId(params.task_id)) {
        throw new Error(`Invalid task ID format: ${params.task_id}`);
      }

      const workspaceRoot = process.cwd();
      const statePath = getWorkflowPath(workspaceRoot, params.task_id);

      const state = await mutateWorkflowState(statePath, (draft) => {
        if (!draft.workflow) {
          throw new Error(`No workflow found for ${params.task_id}`);
        }

        if (!params.validation_override) {
          const validation = validatePhaseTransition(draft, params.from_phase, params.to_phase);
          if (!validation.valid) {
            throw new Error(`Transition blocked: ${validation.errors.join("\n")}`);
          }
        }

        draft.workflow.current_phase = params.to_phase;
        draft.workflow.updated_at = new Date().toISOString();
        addHistoryEvent(draft, "phase_transition", params.from_phase, `Transitioned from ${params.from_phase} to ${params.to_phase}`);
      });

      updateRuntimeUi(ctx, {
        task_id: params.task_id,
        phase: state.workflow?.current_phase,
        task_type: state.task_type,
        workflow_status: state.workflow?.status,
      });
      persistRuntimeState(params.task_id, state);

      const fromConfig = PHASE_CONFIG[params.from_phase];
      const shouldAutoDispatch = fromConfig?.auto_dispatch_after_transition !== false;

      if (shouldAutoDispatch) {
        await dispatchPhase(pi, params.task_id, params.to_phase, state, statePath, workspaceRoot);
      } else {
        await emitPauseBanner(pi, params.task_id, params.from_phase, params.to_phase);
      }

      const replyText = shouldAutoDispatch
        ? `Transitioned to ${params.to_phase} phase and dispatched instructions`
        : `Transitioned to ${params.to_phase} phase. Paused at model-switch boundary; user must resume via /task:orchestrate ${params.task_id}.`;

      return {
        content: [{ type: "text", text: replyText }],
        details: {
          task_id: params.task_id,
          from_phase: params.from_phase,
          to_phase: params.to_phase,
          new_phase: params.to_phase,
        },
      };
    },

    renderCall(args: TransitionPhaseParams, theme: any, _context: any) {
      let text = theme.fg("toolTitle", theme.bold("transition_phase "));
      text += theme.fg("accent", `${args.from_phase} `);
      text += theme.fg("muted", `→ ${args.to_phase}`);
      if (args.validation_override) {
        text += theme.fg("warning", " (override)");
      }
      return new Text(text, 0, 0);
    },

    renderResult(result: any, options: any, theme: any, _context: any) {
      if (options?.isPartial) {
        return new Text(theme.fg("warning", "Transitioning phase..."), 0, 0);
      }
      return new Text(
        `${theme.fg("success", "✓")} ${theme.fg("muted", `Transitioned ${result.details?.from_phase} → ${result.details?.to_phase}`)}`,
        0,
        0,
      );
    },
  });
}

async function showStatus(pi: ExtensionAPI, taskId: string, ctx: ExtensionContext) {
  if (!validateTaskId(taskId)) {
    notifyCtx(pi, ctx, `Invalid task ID format: ${taskId}`, "error");
    return;
  }

  const workspaceRoot = process.cwd();
  const statePath = getWorkflowPath(workspaceRoot, taskId);
  if (!fs.existsSync(statePath)) {
    notifyCtx(pi, ctx, `No workflow file found for ${taskId}`, "warning");
    return;
  }

  const state = readWorkflowState(statePath);
  updateRuntimeUi(ctx, {
    task_id: taskId,
    phase: state.workflow?.current_phase,
    task_type: state.task_type,
    workflow_status: state.workflow?.status,
  });
  persistRuntimeState(taskId, state);
  await dispatchPhase(pi, taskId, "status", state, statePath, workspaceRoot);
}

async function emitPauseBanner(
  pi: ExtensionAPI,
  taskId: string,
  fromPhase: string,
  toPhase: string,
): Promise<void> {
  const banner = [
    "============================================================",
    `PAUSE: ${fromPhase} -> ${toPhase} boundary`,
    "============================================================",
    `Phase ${fromPhase} is complete. Workflow YAML is updated;`,
    `current_phase is now ${toPhase}.`,
    "",
    "This boundary is a model-switch point. Switch to your",
    `preferred model for ${toPhase} work, then resume:`,
    "",
    "  /compact                       (optional, frees context for the next phase)",
    `  /task:orchestrate ${taskId}`,
    "",
    "The phase markdown is self-contained - it reads the workflow",
    "YAML on dispatch - so a compacted history is safe.",
    "",
    "The next phase will not run until you do.",
    "============================================================",
  ].join("\n");
  pi.sendUserMessage(banner, { deliverAs: "followUp" });
}

async function dispatchPhase(
  pi: ExtensionAPI,
  taskId: string,
  phase: string,
  state: WorkflowState,
  statePath: string,
  workspaceRoot: string,
) {
  const phaseFilePath = getPhasePath(workspaceRoot, phase);
  if (!fs.existsSync(phaseFilePath)) {
    pi.sendUserMessage(`Phase file not found: ${phase}.md. Please create it and try again.`, {
      deliverAs: "followUp",
    });
    return;
  }

  const phaseInstructions = fs.readFileSync(phaseFilePath, "utf8");
  const prompt = formatPhasePrompt(taskId, phase, state, statePath, phaseInstructions);
  pi.sendUserMessage(prompt, { deliverAs: "followUp" });
}
