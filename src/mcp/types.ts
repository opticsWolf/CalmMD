/** Shared shapes for the local MCP bridge. Mirrored by `crates/calmmd-core/src/mcp/`. */

export interface McpConnectionInfo {
  enabled: boolean;
  /** Full Streamable HTTP endpoint, e.g. http://127.0.0.1:52431/mcp */
  url: string | null;
  /** Per-launch bearer token. Shown only to the local user. */
  token: string | null;
  windowLabel: string;
}

export interface McpRequest {
  id: string;
  windowLabel: string;
  method: string;
  params: Record<string, unknown>;
}

export type McpResponse =
  | { id: string; ok: true; result: unknown }
  | { id: string; ok: false; error: string };
