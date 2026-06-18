import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { SSEClientTransport } from "@modelcontextprotocol/sdk/client/sse.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import {
  DEFAULT_MAX_BYTES,
  DEFAULT_MAX_LINES,
  formatSize,
  truncateHead,
  type ExtensionAPI,
} from "@mariozechner/pi-coding-agent";
import { Type } from "typebox";
import { Text } from "@mariozechner/pi-tui";

const LINEAR_MCP_SSE_URL = "https://mcp.linear.app/sse";
const LINEAR_MCP_HTTP_URL = "https://mcp.linear.app/mcp";
const TOOL_PREFIX = "mcp__linear__";

// Approved tool subset — see docs/guides/linear-mcp-adapter.md §2.
const APPROVED_TOOLS = new Set<string>([
  "list_issues",
  "get_issue",
  "save_issue",
  "list_comments",
  "save_comment",
  "list_issue_statuses",
  "list_projects",
]);

// Conditional tools registered alongside the core 7. See adapter §2.2.
const CONDITIONAL_TOOLS = new Set<string>(["get_team"]);

const registeredLinearTools = new Map<string, string>();
let activeLinearClient: Client | null = null;

async function closeActiveLinearClient(): Promise<void> {
  if (!activeLinearClient) return;
  try {
    await activeLinearClient.close();
  } catch {
    // best-effort: swallow shutdown errors so refresh can proceed
  }
  activeLinearClient = null;
}

function toolFingerprint(tool: MCPTool): string {
  return JSON.stringify({ d: tool.description ?? "", s: tool.inputSchema ?? null });
}

type MCPTool = {
  name: string;
  description?: string;
  inputSchema?: unknown;
};

export default function (pi: ExtensionAPI) {
  const apiKey = process.env.LINEAR_API_KEY;
  if (!apiKey) {
    pi.sendUserMessage(
      "LINEAR_API_KEY not set. Linear tools unavailable. Set it: export LINEAR_API_KEY=lin_api_...",
      { deliverAs: "followUp" },
    );
    return;
  }

  // pi fires session_start on first start and on reload — single entry point avoids
  // racing two concurrent registrations against the same activeLinearClient.
  pi.on("session_start", async () => {
    const connected = await registerLinearTools(pi, apiKey).catch((err: Error) => {
      pi.sendUserMessage(`Linear MCP connection failed: ${err.message}`, { deliverAs: "followUp" });
      return false;
    });

    if (!connected) {
      pi.sendUserMessage("Linear MCP unavailable. Keeping previously registered tools.", { deliverAs: "followUp" });
    }
  });

  pi.on("session_shutdown", async () => {
    await closeActiveLinearClient();
  });
}

function buildLinearTransport(apiKey: string) {
  const transportMode = (process.env.LINEAR_MCP_TRANSPORT || "http").toLowerCase();
  const headers = { Authorization: `Bearer ${apiKey}` };
  return transportMode === "sse"
    ? new SSEClientTransport(new URL(LINEAR_MCP_SSE_URL), {
        requestInit: { headers },
        eventSourceInit: {
          fetch: (url: string | URL, init?: RequestInit) =>
            fetch(url, { ...(init || {}), headers: { ...(init?.headers || {}), ...headers } }),
        },
      })
    : new StreamableHTTPClientTransport(new URL(LINEAR_MCP_HTTP_URL), {
        requestInit: { headers },
      });
}

async function ensureLinearClient(apiKey: string): Promise<Client> {
  if (activeLinearClient) return activeLinearClient;
  const client = new Client({ name: "pi-linear", version: "1.0.0" });
  await Promise.race([
    client.connect(buildLinearTransport(apiKey)),
    new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error("Linear MCP connection timed out after 10s")), 10_000),
    ),
  ]);
  activeLinearClient = client;
  return client;
}


function schemaToTypeBox(schema: unknown): unknown {
  if (!schema || typeof schema !== "object") {
    return Type.Object({});
  }

  const input = schema as {
    type?: string;
    properties?: Record<string, unknown>;
    required?: string[];
    items?: unknown;
    enum?: unknown[];
    additionalProperties?: unknown;
    description?: string;
  };

  switch (input.type) {
    case "string":
      return Type.String({ description: input.description });
    case "number":
      return Type.Number({ description: input.description });
    case "integer":
      return Type.Integer({ description: input.description });
    case "boolean":
      return Type.Boolean({ description: input.description });
    case "array":
      return Type.Array(schemaToTypeBox(input.items), { description: input.description });
    case "object": {
      const required = new Set(input.required || []);
      const entries = input.properties || {};
      const props: Record<string, unknown> = {};

      for (const [key, value] of Object.entries(entries)) {
        const child = schemaToTypeBox(value);
        props[key] = required.has(key) ? child : Type.Optional(child as any);
      }

      return Object.keys(props).length > 0
        ? Type.Object(props, {
            additionalProperties: false,
            description: input.description,
          })
        : Type.Object({}, { additionalProperties: false, description: input.description });
    }
    default:
      if (Array.isArray(input.enum)) {
        const variants = input.enum
          .filter((item) => typeof item === "string")
          .map((item) => Type.Literal(item));
        if (variants.length > 0) {
          return Type.Union(variants);
        }
      }
      return Type.Record(Type.String(), Type.Any());
  }
}

async function registerLinearTools(pi: ExtensionAPI, apiKey: string): Promise<boolean> {
  await closeActiveLinearClient();
  const client = await ensureLinearClient(apiKey);
  const { tools } = await client.listTools();
  if (!tools || tools.length === 0) {
    pi.sendUserMessage("Linear MCP: connected but no tools discovered.", { deliverAs: "followUp" });
    return false;
  }

  const discoveredCount = Array.isArray(tools) ? tools.length : 0;
  const fullSurface = process.env.LINEAR_MCP_FULL_SURFACE === "1";
  const candidates = fullSurface
    ? (tools as MCPTool[])
    : (tools as MCPTool[]).filter((t) => APPROVED_TOOLS.has(t.name) || CONDITIONAL_TOOLS.has(t.name));

  let registeredCount = 0;
  let refreshedCount = 0;
  for (const tool of candidates) {
    const toolName = `${TOOL_PREFIX}${tool.name}`;
    const fingerprint = toolFingerprint(tool);
    const previous = registeredLinearTools.get(toolName);
    if (previous === fingerprint) {
      continue;
    }
    if (previous !== undefined) {
      refreshedCount += 1;
    } else {
      registeredCount += 1;
    }
    registeredLinearTools.set(toolName, fingerprint);

    const params = tool.inputSchema ? schemaToTypeBox(tool.inputSchema) : Type.Object({});
    const description = tool.description || `Linear: ${tool.name}`;

    pi.registerTool({
      name: toolName,
      label: tool.name,
      description,
      promptSnippet: `Use ${toolName} when interacting with Linear via MCP.`,
      promptGuidelines: [`Use ${toolName} to execute Linear MCP action ${tool.name}.`],
      parameters: params,

      async execute(_toolCallId: string, params: Record<string, unknown>) {
        let lastTransportError: unknown;
        for (let attempt = 1; attempt <= 2; attempt++) {
          let liveClient: Client;
          try {
            liveClient = await ensureLinearClient(apiKey);
          } catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            throw new Error(`Linear tool ${tool.name} unavailable: ${message}`);
          }

          let result: Awaited<ReturnType<Client["callTool"]>>;
          try {
            result = await liveClient.callTool({ name: tool.name, arguments: params });
          } catch (err) {
            // Transport-level failure (connection drop, timeout). Drop the client so
            // the next attempt re-establishes; tool-level errors come back via isError.
            lastTransportError = err;
            await closeActiveLinearClient();
            continue;
          }

          const resultText =
            (result.content as Array<{ type: string; text?: string }> | undefined)
              ?.map((chunk) => (chunk.type === "text" && chunk.text != null ? chunk.text : JSON.stringify(chunk)))
              .join("\n") || JSON.stringify(result);

          if (result.isError) {
            throw new Error(`Linear tool ${tool.name} failed: ${resultText}`);
          }

          const truncation = truncateHead(resultText, {
            maxLines: DEFAULT_MAX_LINES,
            maxBytes: DEFAULT_MAX_BYTES,
          });
          const finalText = truncation.truncated
            ? `${truncation.content}\n\n[Linear MCP output truncated: ${truncation.outputLines}/${truncation.totalLines} lines, ${formatSize(truncation.outputBytes)}/${formatSize(truncation.totalBytes)}]`
            : truncation.content;

          return {
            content: [{ type: "text", text: finalText }],
            details: {
              linear_tool: tool.name,
              result_count: Array.isArray(result.content) ? result.content.length : 0,
              truncated: truncation.truncated,
              attempts: attempt,
            },
          };
        }

        const message = lastTransportError instanceof Error ? lastTransportError.message : String(lastTransportError);
        throw new Error(`Linear tool ${tool.name} failed after reconnect: ${message}`);
      },

      renderCall(args: Record<string, unknown>, theme: any, _ctx: any) {
        const argCount = args ? Object.keys(args).length : 0;
        return new Text(
          `${theme.fg("toolTitle", theme.bold(tool.name))} ${theme.fg("muted", `${argCount} args`)}`,
          0,
          0,
        );
      },

      renderResult(result: any, _opts: any, theme: any, _ctx: any) {
        if (result.isError) {
          return new Text(theme.fg("error", `Linear tool failed`), 0, 0);
        }
        const details = result.content?.[0];
        const text = typeof details?.text === "string" ? details.text : "no output";
        return new Text(theme.fg("success", text.slice(0, 120)), 0, 0);
      },
    });
  }

  const surfaceMode = fullSurface ? "full" : "approved-subset";
  const refreshedSuffix = refreshedCount > 0 ? `, ${refreshedCount} refreshed` : "";
  pi.sendUserMessage(
    `Linear MCP: registered ${registeredCount}${refreshedSuffix} (${registeredLinearTools.size} tracked / ${discoveredCount} discovered, ${surfaceMode}).`,
    { deliverAs: "followUp" },
  );
  return true;
}
