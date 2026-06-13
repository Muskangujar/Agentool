/* ════════════════════════════════════════════════════════════════════════════
   MCP-related types (placeholder for future MCP server management)
   ════════════════════════════════════════════════════════════════════════════ */

export interface McpServerInfo {
  tool_id: string;
  port: number;
  status: 'running' | 'stopped' | 'error';
  started_at?: string;
}

/** Future: fetch active MCP servers from the backend */
export async function fetchMcpServers(): Promise<McpServerInfo[]> {
  // Placeholder — will be wired to the Rust MCP server manager
  return [];
}
