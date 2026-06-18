# pi-linear-mcp

Pi CLI bridge to Linear's hosted MCP server. Connects via SSE (default)
or Streamable HTTP, lists Linear's tools, and re-registers each one in
pi under the `mcp__linear__` prefix so the orchestrator phase scripts
can call them with the same names Claude uses.

## Why this exists

Pi extensions do NOT inherit Claude's MCP server configuration. Each
extension that needs MCP must establish its own client connection. The
gcm orchestrator phase scripts call `mcp__linear__get_issue`,
`mcp__linear__save_issue`, `mcp__linear__save_comment`, etc. - this
bridge is what makes those names resolvable inside pi.

The bridge mirrors the pattern used by `~/Code/mentis/.pi/extensions/plane/`.

## Configuration

Required env var:

```bash
export LINEAR_API_KEY=lin_api_...
```

Optional:

```bash
# default: http (Streamable HTTP). SSE kept as fallback because the
# MCP SDK has deprecated SSE in favour of Streamable HTTP.
export LINEAR_MCP_TRANSPORT=http   # or 'sse'

# default: filtered to the approved 7-tool subset
# (see docs/guides/linear-mcp-adapter.md §2). Set to "1" to register
# every tool Linear's MCP exposes — escape hatch for one-off tasks
# that need an excluded tool. Prefer escalating to the user instead.
export LINEAR_MCP_FULL_SURFACE=0   # or '1'
```

If `LINEAR_API_KEY` is missing, the extension prints a follow-up
message and exits cleanly - the orchestrator will then fail fast on
the first `mcp__linear__*` call rather than silently no-op.

## Installation

```bash
cd .pi/extensions/linear
npm install

# temporary
pi -e .pi/extensions/linear/index.ts

# permanent (symlink)
ln -s $(pwd)/.pi/extensions/linear ~/.pi/agent/extensions/gcm-linear
```

## Tool surface

By default the bridge registers only the 7-tool approved subset defined
in `docs/guides/linear-mcp-adapter.md` §2, plus `get_team` as a
conditional. Linear's hosted MCP exposes ~30 tools; filtering keeps
the agent prompt small and prevents accidental selection of the wrong
tool.

Approved (registered):

- `mcp__linear__list_issues`
- `mcp__linear__get_issue`
- `mcp__linear__save_issue`
- `mcp__linear__list_comments`
- `mcp__linear__save_comment`
- `mcp__linear__list_issue_statuses`
- `mcp__linear__list_projects`
- `mcp__linear__get_team` (conditional)

Excluded by default: everything else (cycles, documents, milestones,
labels, attachments, users, project mutators, …). If a task genuinely
needs an excluded tool, escalate to the user. The
`LINEAR_MCP_FULL_SURFACE=1` env var is an escape hatch that registers
every tool Linear exposes — use sparingly, since it grows the agent
prompt and weakens the contract.

When Linear ships new tools, they only appear if their name is added
to `APPROVED_TOOLS` in `index.ts` (or `LINEAR_MCP_FULL_SURFACE=1` is
set). Update the adapter doc when adding a tool.

## Auth notes

Linear's hosted MCP at `https://mcp.linear.app/sse` accepts a Linear
personal API key as a Bearer token. If your workspace requires OAuth
instead, replace the `Authorization` header construction in `index.ts`
with the OAuth token flow.

## See also

- `../orchestrate/README.md` - extension-level docs for the orchestrator
- `../../IMPLEMENTATION_SUMMARY.md` - high-level pi flow overview
- `../../../docs/guides/linear-mcp.md` - tool reference and team context
- `../../../docs/guides/linear-mcp-adapter.md` - approved-subset contract,
  per-tool API, cache rules, phase-action matrix
