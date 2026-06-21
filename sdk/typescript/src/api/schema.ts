/* ════════════════════════════════════════════════════════════════════════════
   Agentool — Schema types & API layer
   ════════════════════════════════════════════════════════════════════════════ */

// ── Types matching the ToolSchema JSON format ───────────────────────────────

export interface SchemaParam {
  name: string;
  type: string;
  required: boolean;
  location?: string;   // "query" | "path" | "body" | "header"
  description?: string;
}

export interface SchemaMethod {
  name: string;
  description: string;
  http: {
    method: string;   // GET | POST | PUT | PATCH | DELETE
    path: string;
  };
  params: SchemaParam[];
  returns?: { type: string; description?: string } | string;
}

export interface SchemaAuth {
  auth_type: string;   // "bearer" | "api_key" | "none"
  location?: string | null;
  name?: string | null;
  scheme?: string | null;
}

export interface SchemaProvenance {
  source: string;
  fetched_at: string;
  source_url?: string;
}

export interface ToolSchema {
  tool_id: string;
  version: string;
  base_url: string;
  auth?: SchemaAuth;
  rate_limit?: { requests_per_second: number } | null;
  methods: SchemaMethod[];
  components?: Record<string, unknown>;
  provenance?: SchemaProvenance;
}

// ── API calls ───────────────────────────────────────────────────────────────

/** Fetch all registry schemas from the Vite dev server plugin */
export async function fetchRegistrySchemas(): Promise<ToolSchema[]> {
  const res = await fetch('/api/registry_schemas');
  if (!res.ok) throw new Error(`Registry fetch failed: ${res.status}`);
  return res.json();
}

/**
 * Call an API method through a simple proxy.
 * For now this does a direct fetch (works for CORS-enabled APIs).
 */
export async function callMethod(
  baseUrl: string,
  method: SchemaMethod,
  params: Record<string, string>,
  authToken?: string,
): Promise<{ status: number; body: string; duration: number }> {
  const start = performance.now();

  // Build the URL: substitute path params, append query params
  let path = method.http.path;
  const queryParams: Record<string, string> = {};

  for (const p of method.params) {
    const val = params[p.name];
    if (!val) continue;

    if (p.location === 'path') {
      path = path.replace(`{${p.name}}`, encodeURIComponent(val));
    } else if (p.location === 'query' || method.http.method === 'GET') {
      queryParams[p.name] = val;
    }
  }

  const qs = new URLSearchParams(queryParams).toString();
  const url = `${baseUrl}${path}${qs ? '?' + qs : ''}`;

  const headers: Record<string, string> = {
    'Accept': 'application/json',
  };

  if (authToken) {
    headers['Authorization'] = `Bearer ${authToken}`;
  }

  // Build body for non-GET methods
  let body: string | undefined;
  if (method.http.method !== 'GET') {
    const bodyParams: Record<string, string> = {};
    for (const p of method.params) {
      if (p.location === 'body' && params[p.name]) {
        bodyParams[p.name] = params[p.name];
      }
    }
    if (Object.keys(bodyParams).length > 0) {
      headers['Content-Type'] = 'application/json';
      body = JSON.stringify(bodyParams);
    }
  }

  try {
    const res = await fetch(url, {
      method: method.http.method,
      headers,
      body,
    });
    const text = await res.text();
    const duration = performance.now() - start;

    // Try to pretty-print JSON
    let formatted: string;
    try {
      formatted = JSON.stringify(JSON.parse(text), null, 2);
    } catch {
      formatted = text;
    }

    return { status: res.status, body: formatted, duration };
  } catch (err) {
    const duration = performance.now() - start;
    return {
      status: 0,
      body: `Network error: ${err instanceof Error ? err.message : String(err)}`,
      duration,
    };
  }
}
