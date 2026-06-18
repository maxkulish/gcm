import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { SSEClientTransport } from "@modelcontextprotocol/sdk/client/sse.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";

const apiKey = process.env.LINEAR_API_KEY;
if (!apiKey) {
  console.error("LINEAR_API_KEY not set");
  process.exit(1);
}

const headers = { Authorization: `Bearer ${apiKey}` };

async function testTransport(name, transport) {
  const client = new Client({ name: "pi-linear-test", version: "1.0.0" });
  console.log(`\n--- Testing ${name} ---`);
  try {
    await Promise.race([
      client.connect(transport),
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error("Connection timed out after 10s")), 10_000)
      ),
    ]);
    const { tools } = await client.listTools();
    console.log(`${name}: SUCCESS - ${tools.length} tools discovered`);
    await client.close();
  } catch (err) {
    console.error(`${name}: FAILED - ${err.message}`);
  }
}

const sseTransport = new SSEClientTransport(new URL("https://mcp.linear.app/sse"), {
  requestInit: { headers },
  eventSourceInit: {
    fetch: (url, init) => fetch(url, { ...(init || {}), headers: { ...(init?.headers || {}), ...headers } }),
  },
});

const httpTransport = new StreamableHTTPClientTransport(new URL("https://mcp.linear.app/mcp"), {
  requestInit: { headers },
});

await testTransport("SSE", sseTransport);
await testTransport("HTTP", httpTransport);
