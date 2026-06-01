# MCP Protocol — S3 Contract

## Overview

The agentool MCP server speaks **JSON-RPC 2.0** transported over plain HTTP. All
requests are `POST /mcp` with a `Content-Type: application/json` body. The server
always responds with HTTP 200 and a JSON-RPC 2.0 envelope regardless of the
logical outcome; protocol-level errors are signalled via the `"error"` key rather
than via HTTP status codes. Any other HTTP method returns 405; any other path
returns 404. This is the **S3 Contract**: Track B (Python SDK) must implement
clients that send requests and parse responses in the exact format shown below.

---

## Methods

### `initialize`

Sent by the client before any other call to negotiate the protocol version and
discover server capabilities.

**Request**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {}
}
```

**Response**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "tools": { "listChanged": false }
    },
    "serverInfo": {
      "name": "agentool",
      "version": "0.1.0"
    }
  }
}
```

---

### `tools/list`

Returns all tools exposed by this server instance. Each tool maps to one HTTP
endpoint in the underlying `ToolSchema`.

**Request**
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/list",
  "params": {}
}
```

**Response**
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "tools": [
      {
        "name": "search",
        "description": "Search things.",
        "inputSchema": {
          "type": "object",
          "properties": {
            "q": { "type": "string" }
          },
          "required": ["q"]
        }
      }
    ]
  }
}
```

`inputSchema` always has `"type": "object"`. The `"properties"` map contains one
entry per `Param` in the method definition. The `"required"` array lists all
parameter names where `required == true`. Optional `"description"` and `"enum"`
keys are included on each property when the underlying `Param` carries them.

---

### `tools/call`

Invokes a named tool by proxying the call to the upstream API. Path parameters
are substituted into the URL template; query and body parameters are forwarded
in the request.

**Request**
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "search",
    "arguments": {
      "q": "rust async"
    }
  }
}
```

**Response (success)**
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"results\": [{\"title\": \"Tokio\", \"url\": \"https://tokio.rs\"}]}"
      }
    ],
    "isError": false
  }
}
```

**Response (upstream failure)**
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "request failed after retries: upstream returned 503: Service Unavailable"
      }
    ],
    "isError": true
  }
}
```

Note that an upstream failure is represented as a successful JSON-RPC result
with `"isError": true`, not as a JSON-RPC error object. This matches the MCP
specification: tool-level errors are content, not protocol errors.

---

### `ping`

A no-op health check. The result is always an empty object.

**Request**
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "ping",
  "params": {}
}
```

**Response**
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "result": {}
}
```

---

## Error Codes

When the server itself cannot process a request, it returns an `"error"` object
instead of a `"result"`:

```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "error": {
    "code": -32601,
    "message": "Method not found"
  }
}
```

| Code    | Name             | Trigger                                                    |
|---------|------------------|------------------------------------------------------------|
| -32700  | Parse error      | Request body is not valid JSON                             |
| -32601  | Method not found | `method` is not one of the four supported JSON-RPC methods |
| -32602  | Invalid params   | `params` is missing required fields or has wrong types     |
| -32603  | Internal error   | Unexpected server-side failure (includes `"data"` detail)  |

---

## Track B Compatibility Note

This document is the **S3 Contract**. The Python SDK (Track B) must send
requests with exactly the JSON shapes shown above and must handle responses in
the same format. In particular:

- The `"jsonrpc": "2.0"` field is required on every request.
- The `"id"` field may be `null`, a number, or a string; the response will echo
  back the same value.
- `tools/call` results always use the `content` / `isError` envelope — never a
  bare value.
- HTTP status codes other than 200 indicate a transport-level problem (wrong
  path or wrong HTTP method), not a JSON-RPC error.
