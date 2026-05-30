# IMPLEMENTATION_PLAN_AGENTOOL.md
## Phase 3 — Agentool: Universal API → MCP Translator

> **Mission.** Wrap any Application Programming Interface or website and expose it as a structured, agent-callable tool over the Model Context Protocol — automatically. One command (`agentool wrap <URL>`) generates a typed schema, mounts a Hyper + Tokio MCP server, and (optionally) caches schemas in AgentMem and injects credentials from AgentID.

> **Status of the ecosystem.**
> - Phase 1 — AgentID — **shipped** (https://github.com/samvardhan03/agentid)
> - Phase 2 — AgentMem — **shipped** (https://github.com/Muskangujar/AgentMem)
> - Phase 3 — Agentool — **this document**
>
> Agentool is the third standalone product in the AgentBase phased blueprint (see `/Users/shekhawat/Desktop/blueprints/agentbase_phased_blueprint.md`). It must work 100% alone. AgentID and AgentMem are **optional, runtime-detected dependencies** — never hard imports.

> **Audience.** Two engineers ("Dev A" and "Dev B") shipping in strict parallel for ~12 weeks (Weeks 19–30 of the master schedule). This document is the merge contract between their branches. It is also the brief a junior coding agent should be able to execute one ticket at a time without ambiguity.

> **Author conventions.** Patterned after `agentmem/EXECUTION_PLAN_DEV_A.md` — same tone, same gate structure, same "no drift" discipline.

---

## Table of Contents

0. [Strategic Frame & Non-Goals](#0-strategic-frame--non-goals)
1. [Two-Track Split — The Merge Contract](#1-two-track-split--the-merge-contract)
2. [Repository Skeleton — Empty Files to Create First](#2-repository-skeleton--empty-files-to-create-first)
3. [The `ToolSchema` Data Contract (shared boundary)](#3-the-toolschema-data-contract)
4. [TRACK A — Dev A: Rust Core + C++ Schema Inference](#4-track-a--dev-a-rust-core--c-schema-inference)
5. [TRACK B — Dev B: Python SDK + TypeScript CLI/Dashboard](#5-track-b--dev-b-python-sdk--typescript-clidashboard)
6. [Optional Integrations (AgentID, AgentMem) — runtime detection only](#6-optional-integrations)
7. [Build, Test, CI](#7-build-test-ci)
8. [The 12-Week Schedule (gated)](#8-the-12-week-schedule)
9. [Definition of Done — per track and overall](#9-definition-of-done)
10. [Risk Register & Mitigations](#10-risk-register)

---

## 0. Strategic Frame & Non-Goals

### 0.1 What Agentool actually is

Agentool is a three-path schema generator + an MCP server runtime:

```
Input: URL
  ├──→ Path 1: OpenAPI 3.x spec found?       → Rust streaming parser  → ToolSchema
  ├──→ Path 2: HTML docs page (Mintlify…)?   → C++ libxml2 inferrer   → ToolSchema
  └──→ Path 3: No spec, no docs              → Playwright recorder    → ToolSchema
                                                        ↓
                                              ToolSchema (JSON)
                                                        ↓
                                          Hyper + Tokio MCP server
                                                        ↓
                                    Claude Desktop / Cursor / LangGraph
```

A `ToolSchema` is the single internal currency. Every path produces one; the MCP server consumes one.

### 0.2 v1 non-goals (cut scope ruthlessly)

The blueprint says **ship at 80% quality**. The following are explicitly **out of scope for v1**:

- ❌ OpenAPI 2.0 (Swagger) — only 3.x in v1
- ❌ AsyncAPI, GraphQL introspection, gRPC reflection
- ❌ OAuth 2.0 *interactive* flows — v1 supports Bearer / API key / Basic only (OAuth client_credentials is borderline; defer)
- ❌ Distributed multi-node MCP server — v1 is single-process
- ❌ Web UI for browser-recording playback — record.py outputs JSON, Schema Explorer reads it
- ❌ Schema diffing across versions — v1 ships only "fetch / parse / serve"
- ❌ Anything that requires JS execution in the docs parser (only static HTML)

### 0.3 The phased-blueprint design rule (re-stated, do not violate)

> Each tool works 100% alone. AgentID and AgentMem are detected at runtime via `importlib.util.find_spec`. **No hard dependency. No import error if they're missing.** This is enforced in CI by a smoke test that installs `agentool` into a virtualenv with `--no-deps` and runs the README example.

---

## 1. Two-Track Split — The Merge Contract

### 1.1 The split, in one diagram

```
┌──────────────────────── REPO ROOT ────────────────────────┐
│                                                            │
│   ┌─────────── TRACK A — Dev A ────────────┐               │
│   │  core/                                 │  ← Rust + C++ │
│   │    ├── src/        (Rust)              │               │
│   │    ├── cpp/        (C++ via cxx)       │               │
│   │    ├── proto/      (MCP/gRPC schemas)  │               │
│   │    ├── tests/                          │               │
│   │    ├── examples/                       │               │
│   │    ├── build.rs                        │               │
│   │    └── Cargo.toml                      │               │
│   └────────────────────────────────────────┘               │
│                                                            │
│   ┌─────────── TRACK B — Dev B ────────────┐               │
│   │  sdk/python/                           │  ← Python     │
│   │    ├── agentool/                       │               │
│   │    ├── src/         (PyO3 maturin)     │               │
│   │    └── pyproject.toml                  │               │
│   │  sdk/typescript/                       │  ← TS + Bun   │
│   │    ├── src/         (React dashboard)  │               │
│   │    ├── cli/         (agentool CLI)     │               │
│   │    ├── dashboard/                      │               │
│   │    └── package.json                    │               │
│   │  registry/          (schema configs)   │               │
│   └────────────────────────────────────────┘               │
│                                                            │
│   ┌─────── SHARED (both edit, with care) ──┐               │
│   │  README.md                             │               │
│   │  .gitignore                            │               │
│   │  IMPLEMENTATION_PLAN_AGENTOOL.md  ←this │               │
│   │  core/proto/schema.proto    (Dev A owns) │             │
│   │  schemas/tool_schema.schema.json (Dev A owns) │        │
│   └────────────────────────────────────────┘               │
└────────────────────────────────────────────────────────────┘
```

### 1.2 Hard ownership rules (zero merge conflicts)

| Path | Owner | The other dev may… |
|---|---|---|
| `core/` (entire subtree) | **Dev A** | Read only. Open issues. Never edit. |
| `sdk/python/` | **Dev B** | Read only. |
| `sdk/typescript/` | **Dev B** | Read only. |
| `registry/` | **Dev B** | Read only. |
| `core/proto/`, `schemas/` | **Dev A** | Dev B reads to derive types. Changes require a PR review by Dev B. |
| `README.md` | **Either** | Edit only the section explicitly delegated to your track. |
| `IMPLEMENTATION_PLAN_AGENTOOL.md` | **Neither** | Frozen after kickoff. Amendments via PR with both reviewers. |
| `.github/workflows/*` | **Dev A** owns `core-*.yml`, **Dev B** owns `sdk-*.yml`. | Never cross-edit. |

### 1.3 The seam — how the two tracks talk

There are exactly **three** points where Track A and Track B meet. Lock these contracts in Week 1, do not renegotiate without both signoffs:

| Seam # | What | Owner of the contract | Format |
|---|---|---|---|
| **S1** | `ToolSchema` — the JSON object that every parser emits and the MCP server consumes | Dev A | `schemas/tool_schema.schema.json` (JSON Schema) |
| **S2** | PyO3 surface — the Rust functions exposed to Python | Dev A | `core/src/py.rs` exports listed in §3.3 |
| **S3** | MCP wire protocol — the JSON-RPC 2.0 messages the dashboard / CLI / Claude Desktop send | Dev A | `core/proto/mcp.md` (markdown of the JSON-RPC envelope) |

Dev B builds against these three contracts. If Dev A changes them, Dev A bumps the Cargo + pyproject minor version and posts in the PR description.

---

## 2. Repository Skeleton — Empty Files to Create First

Before either dev writes code, **commit this skeleton in a single bootstrap PR**. Both devs review. Once merged, neither dev creates new top-level dirs without a sign-off from the other.

### 2.1 Full tree (empty files; touch + commit)

```
agentool/
├── .gitignore
├── .github/
│   └── workflows/
│       ├── core-build.yml          # Dev A
│       ├── core-test.yml           # Dev A
│       ├── sdk-python.yml          # Dev B
│       └── sdk-typescript.yml      # Dev B
├── README.md
├── LICENSE                          # Apache-2.0
├── IMPLEMENTATION_PLAN_AGENTOOL.md  # this file
├── schemas/
│   └── tool_schema.schema.json     # S1 contract — Dev A owns
├── registry/                        # Dev B owns
│   ├── README.md
│   ├── github.schema.json          # placeholder
│   ├── stripe.schema.json          # placeholder
│   └── pubmed.schema.json          # placeholder
├── core/                            # ── TRACK A ────────────────
│   ├── Cargo.toml
│   ├── build.rs
│   ├── proto/
│   │   ├── mcp.md                  # S3 contract — human-readable
│   │   └── tool_schema.proto       # optional gRPC mirror of S1
│   ├── src/
│   │   ├── lib.rs                  # crate root, re-exports
│   │   ├── error.rs                # thiserror AgentoolError
│   │   ├── schema.rs               # ToolSchema, Method, Param structs
│   │   ├── openapi.rs              # streaming OpenAPI 3.x parser
│   │   ├── mcp_server.rs           # hyper + tokio JSON-RPC handler
│   │   ├── http_client.rs          # reqwest wrapper with retries
│   │   ├── auth_inject.rs          # Bearer / API key / Basic injection
│   │   ├── rate_limit.rs           # token bucket per-tool
│   │   ├── retry.rs                # exponential backoff + circuit breaker
│   │   ├── infer_bridge.rs         # cxx bridge to C++ inferrer
│   │   ├── py.rs                   # PyO3 surface (S2) — only file Dev B reads
│   │   └── bin/
│   │       └── agentool_server.rs  # `agentool-server` standalone binary
│   ├── cpp/
│   │   ├── schema_infer.cpp        # libxml2 HTML → ToolSchema fragments
│   │   ├── schema_infer.h
│   │   ├── patterns/
│   │   │   ├── mintlify.cpp        # Mintlify-specific selectors
│   │   │   ├── readme.cpp          # ReadMe.com
│   │   │   ├── swagger_ui.cpp      # Swagger UI rendered HTML
│   │   │   └── gitbook.cpp         # GitBook
│   │   └── README.md               # how patterns are registered
│   ├── tests/
│   │   ├── openapi_roundtrip.rs
│   │   ├── mcp_protocol.rs
│   │   └── fixtures/
│   │       ├── github.openapi.json
│   │       ├── stripe.openapi.json
│   │       └── pubmed.docs.html
│   └── examples/
│       └── smoke.rs                # `cargo run --example smoke`
└── sdk/                             # ── TRACK B ────────────────
    ├── python/
    │   ├── pyproject.toml
    │   ├── README.md
    │   ├── src/
    │   │   └── lib.rs              # maturin PyO3 shim — re-exports core::py
    │   ├── Cargo.toml              # maturin crate (separate from core)
    │   ├── agentool/
    │   │   ├── __init__.py         # `from .tool import Tool`
    │   │   ├── tool.py             # the Tool class (Python-side façade)
    │   │   ├── record.py           # Playwright browser recorder
    │   │   ├── _native.pyi         # type stubs for the PyO3 module
    │   │   ├── integrations/
    │   │   │   ├── __init__.py
    │   │   │   ├── langgraph.py
    │   │   │   ├── crewai.py
    │   │   │   ├── agentid.py      # optional credential injection
    │   │   │   └── agentmem.py     # optional schema caching
    │   │   └── cli_entry.py        # `python -m agentool …` fallback
    │   └── tests/
    │       ├── test_tool.py
    │       ├── test_serve_mcp.py
    │       └── test_optional_integrations.py
    └── typescript/
        ├── package.json
        ├── bunfig.toml
        ├── tsconfig.json
        ├── vite.config.ts
        ├── index.html
        ├── src/                    # React dashboard
        │   ├── App.tsx
        │   ├── index.tsx
        │   ├── api/
        │   │   ├── mcp.ts          # JSON-RPC client (S3 consumer)
        │   │   └── schema.ts       # ToolSchema TS types (S1 consumer)
        │   └── components/
        │       ├── SchemaExplorer.tsx        # the NEW tab
        │       ├── EndpointTree.tsx
        │       ├── EndpointDetail.tsx
        │       ├── CallTester.tsx
        │       └── RegistryBrowser.tsx
        ├── cli/
        │   ├── index.ts            # `agentool` entrypoint
        │   └── commands/
        │       ├── wrap.ts         # `agentool wrap <URL>`
        │       ├── serve.ts        # `agentool serve <schema>`
        │       ├── record.ts       # shells out to record.py
        │       └── registry.ts     # `agentool registry add|list|publish`
        └── dashboard/              # built static assets land here
            └── .gitkeep
```

### 2.2 Empty-file bootstrap script (run once, both devs review the diff)

This is the only "code" both devs touch together. It commits empty placeholders so subsequent edits are pure diffs against existing files — no `git mv` surprises.

```bash
# Run from repo root after `git init`
set -e
mkdir -p core/{src/bin,cpp/patterns,proto,tests/fixtures,examples}
mkdir -p sdk/python/{agentool/integrations,src,tests}
mkdir -p sdk/typescript/{src/{api,components},cli/commands,dashboard}
mkdir -p schemas registry .github/workflows

# Track-A files
touch core/Cargo.toml core/build.rs
touch core/src/{lib,error,schema,openapi,mcp_server,http_client,auth_inject,rate_limit,retry,infer_bridge,py}.rs
touch core/src/bin/agentool_server.rs
touch core/cpp/schema_infer.{cpp,h}
touch core/cpp/patterns/{mintlify,readme,swagger_ui,gitbook}.cpp
touch core/cpp/README.md
touch core/proto/{mcp.md,tool_schema.proto}
touch core/tests/{openapi_roundtrip,mcp_protocol}.rs
touch core/examples/smoke.rs

# Track-B files
touch sdk/python/{pyproject.toml,README.md,Cargo.toml}
touch sdk/python/src/lib.rs
touch sdk/python/agentool/{__init__,tool,record,cli_entry,_native.pyi}.py 2>/dev/null || true
# (the .pyi must be created without the .py double-suffix; this line is illustrative)
touch sdk/python/agentool/__init__.py sdk/python/agentool/tool.py
touch sdk/python/agentool/record.py sdk/python/agentool/cli_entry.py
touch sdk/python/agentool/_native.pyi
touch sdk/python/agentool/integrations/{__init__,langgraph,crewai,agentid,agentmem}.py
touch sdk/python/tests/{test_tool,test_serve_mcp,test_optional_integrations}.py

touch sdk/typescript/{package.json,bunfig.toml,tsconfig.json,vite.config.ts,index.html}
touch sdk/typescript/src/{App.tsx,index.tsx}
touch sdk/typescript/src/api/{mcp.ts,schema.ts}
touch sdk/typescript/src/components/{SchemaExplorer,EndpointTree,EndpointDetail,CallTester,RegistryBrowser}.tsx
touch sdk/typescript/cli/index.ts
touch sdk/typescript/cli/commands/{wrap,serve,record,registry}.ts
touch sdk/typescript/dashboard/.gitkeep

# Shared
touch schemas/tool_schema.schema.json
touch registry/{README.md,github.schema.json,stripe.schema.json,pubmed.schema.json}
touch .github/workflows/{core-build,core-test,sdk-python,sdk-typescript}.yml
touch README.md LICENSE .gitignore

git add -A
git commit -m "chore: bootstrap agentool monorepo skeleton (Phase 3)"
```

After this commit, **no new top-level directories without a joint PR**.

---

## 3. The `ToolSchema` Data Contract

### 3.1 Why this is gated first

Both devs depend on this shape. Dev A's OpenAPI parser emits it; Dev A's C++ inferrer emits fragments of it; Dev A's MCP server consumes it; Dev B's Python `Tool` class wraps it; Dev B's React Schema Explorer renders it. If the shape changes mid-sprint, every component churns.

**Action: Dev A produces `schemas/tool_schema.schema.json` and `core/src/schema.rs` in Week 1 Day 1–2. Both are reviewed by Dev B before any other work starts.**

### 3.2 The shape (informative — Dev A finalizes the JSON Schema)

```jsonc
{
  "tool_id":     "github",
  "version":     "1.0.0",
  "base_url":    "https://api.github.com",
  "auth": {
    "type": "bearer" | "api_key" | "basic" | "none",
    "in":   "header" | "query",
    "name": "Authorization",
    "scheme": "Bearer"        // optional, only for bearer
  },
  "rate_limit": {
    "rps": 10,
    "burst": 30
  },
  "methods": [
    {
      "name":        "search_repositories",
      "description": "Search repos by query.",
      "http": {
        "method": "GET",
        "path":   "/search/repositories"
      },
      "params": [
        { "name": "q",     "in": "query", "type": "string",  "required": true  },
        { "name": "sort",  "in": "query", "type": "string",  "required": false,
          "enum": ["stars","forks","updated"] }
      ],
      "returns": {
        "kind":   "object",
        "fields": [
          { "name": "total_count", "type": "integer" },
          { "name": "items",       "type": "array",
            "items": { "$ref": "#/components/Repository" } }
        ]
      }
    }
  ],
  "components": { "Repository": { /* ... */ } },
  "provenance": {
    "source":    "openapi" | "html_infer" | "browser_record",
    "fetched_at": "2026-05-30T00:00:00Z",
    "source_url": "https://api.github.com/openapi.json"
  }
}
```

### 3.3 The PyO3 surface (S2) — exact function list

`core/src/py.rs` exposes these and only these to Python. Dev B can plan against this list on Week 1:

```rust
// Pseudocode for the contract — exact attrs in Phase 1.

#[pyfunction] fn parse_openapi_url(url: &str) -> PyResult<ToolSchemaPy>;
#[pyfunction] fn parse_openapi_str(spec: &str) -> PyResult<ToolSchemaPy>;
#[pyfunction] fn infer_from_html(url: &str, html: &str) -> PyResult<ToolSchemaPy>;
#[pyfunction] fn schema_from_json(json: &str) -> PyResult<ToolSchemaPy>;
#[pyfunction] fn schema_to_json(schema: &ToolSchemaPy) -> PyResult<String>;

#[pyclass]    struct ToolSchemaPy { /* fields readable from Python */ }
#[pymethods] impl ToolSchemaPy {
    #[getter] fn methods(&self) -> Vec<MethodPy>;
    #[getter] fn tool_id(&self) -> &str;
    fn call_blocking(&self, method: &str, params: &PyDict,
                     auth: Option<&PyDict>) -> PyResult<PyObject>;
}

#[pyclass]    struct McpServerHandle { /* opaque */ }
#[pyfunction] fn start_mcp_server(schema: &ToolSchemaPy, port: u16)
                                  -> PyResult<McpServerHandle>;
#[pymethods] impl McpServerHandle {
    fn stop(&self) -> PyResult<()>;
    fn port(&self) -> u16;
}
```

Dev B mocks these for unit tests in Week 1–2 (`sdk/python/tests/conftest.py` provides a fake `_native` module).

### 3.4 The MCP wire protocol (S3) — pinned subset

v1 implements **only** these JSON-RPC 2.0 methods (Model Context Protocol spec, 2024-11-05 revision):

| Method | Direction | Purpose |
|---|---|---|
| `initialize` | client → server | handshake |
| `tools/list` | client → server | enumerate methods from the `ToolSchema` |
| `tools/call` | client → server | invoke a method; server proxies to upstream |
| `ping` | both | keepalive |
| `notifications/cancelled` | client → server | abort an in-flight call |

Stretch (only if Week 11 is clean): `resources/list`, `prompts/list`. Otherwise defer to v1.1.

---

## 4. TRACK A — Dev A: Rust Core + C++ Schema Inference

### 4.1 Out of scope for Dev A (do not touch)

- `sdk/python/agentool/**/*.py` (the Python façade is Dev B's)
- `sdk/typescript/**` (entire tree)
- `registry/**` (Dev B's namespace)
- Any pip / bun / npm / playwright command

Dev A's deliverable is `cargo build` green + tests green + a documented PyO3 surface that Dev B can call from Python. **Dev A never opens a `.py` or `.ts` file.**

### 4.2 `core/Cargo.toml` — exact dependency list

Pin versions. Do not "upgrade to latest." These were chosen for ABI stability and known-good interop.

```toml
[package]
name         = "agentool-core"
version      = "0.1.0"
edition      = "2021"
rust-version = "1.75"

[lib]
name       = "agentool_core"
crate-type = ["rlib", "cdylib"]   # cdylib so the Python maturin crate links it

[[bin]]
name = "agentool-server"
path = "src/bin/agentool_server.rs"

[dependencies]
# ── async runtime + HTTP server ──────────────────────────────
tokio         = { version = "1.36", features = ["full"] }
hyper         = { version = "1.2",  features = ["http1", "http2", "server"] }
hyper-util    = { version = "0.1",  features = ["tokio"] }
http-body-util = "0.1"
tower         = { version = "0.4",  features = ["limit", "timeout", "util"] }

# ── outbound HTTP (upstream API calls) ───────────────────────
reqwest       = { version = "0.12", default-features = false,
                  features = ["rustls-tls", "json", "stream", "gzip", "brotli"] }
url           = "2.5"

# ── serialization ────────────────────────────────────────────
serde         = { version = "1.0", features = ["derive"] }
serde_json    = { version = "1.0", features = ["preserve_order"] }
# streaming JSON for the OpenAPI parser:
serde-json-core = { version = "0.5", default-features = false }
struson       = "0.5"          # streaming JSON reader; faster than serde_json on 50MB specs

# ── error handling ───────────────────────────────────────────
thiserror     = "1.0"
anyhow        = "1.0"          # only inside `bin/` — never in library code

# ── concurrency primitives ───────────────────────────────────
parking_lot   = "0.12"
dashmap       = "5.5"          # per-tool rate-limit state

# ── rate limit + retry ───────────────────────────────────────
governor      = "0.6"          # token-bucket; provides DefaultDirectRateLimiter
backoff       = { version = "0.4", features = ["tokio"] }

# ── observability ────────────────────────────────────────────
tracing       = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# ── C++ bridge ───────────────────────────────────────────────
cxx           = "1.0"

# ── PyO3 surface (S2) ────────────────────────────────────────
pyo3          = { version = "0.21", features = ["extension-module", "abi3-py38"] }

# ── utility ──────────────────────────────────────────────────
bytes         = "1.5"
once_cell     = "1.19"
uuid          = { version = "1.7", features = ["v4"] }
chrono        = { version = "0.4", features = ["serde"] }

[build-dependencies]
cxx-build = "1.0"
pkg-config = "0.3"   # to locate libxml2

[dev-dependencies]
tokio        = { version = "1.36", features = ["full", "test-util"] }
wiremock     = "0.6"
tempfile     = "3"
insta        = { version = "1.36", features = ["json"] }   # snapshot tests
pretty_assertions = "1"
```

**Compiler flags / linking (in `build.rs`):**

```rust
// build.rs — pseudocode for Dev A to flesh out

// 1. Find libxml2 via pkg-config; fail loudly if missing.
let lib = pkg_config::Config::new()
    .atleast_version("2.9")
    .probe("libxml-2.0")
    .expect("libxml2 not found — `brew install libxml2` or `apt install libxml2-dev`");

// 2. Build the C++ side with cxx-build.
cxx_build::bridges([
    "src/infer_bridge.rs",
])
.file("cpp/schema_infer.cpp")
.file("cpp/patterns/mintlify.cpp")
.file("cpp/patterns/readme.cpp")
.file("cpp/patterns/swagger_ui.cpp")
.file("cpp/patterns/gitbook.cpp")
.includes(&lib.include_paths)
.flag_if_supported("-std=c++17")
.compile("agentool_infer");

// 3. Link libxml2.
for p in &lib.link_paths { println!("cargo:rustc-link-search=native={}", p.display()); }
for l in &lib.libs       { println!("cargo:rustc-link-lib={}",            l); }
```

### 4.3 Phase plan (Track A only)

Each phase ends with a **PAUSE gate** — Dev A stops, posts the green CI run + a short writeup, waits for Dev B to ack before moving on.

#### Phase A1 — Schema + OpenAPI parser (Weeks 1–3)

**Files touched:**
`core/Cargo.toml`, `core/build.rs`, `core/src/{lib,error,schema,openapi}.rs`,
`schemas/tool_schema.schema.json`, `core/proto/{mcp.md,tool_schema.proto}`,
`core/tests/{openapi_roundtrip}.rs`, `core/examples/smoke.rs`,
fixtures `core/tests/fixtures/{github,stripe}.openapi.json`.

**Deliverables:**
1. `schemas/tool_schema.schema.json` finalized (S1 contract closed).
2. `core/src/schema.rs` — Rust mirror of S1: `ToolSchema`, `Method`, `Param`, `Auth`, `Provenance`. `#[derive(Serialize, Deserialize)]`. Round-trips against the JSON Schema in CI.
3. `core/src/openapi.rs` — **streaming** parser using `struson`. Why streaming: GitHub's OpenAPI spec is 30 MB; `serde_json::from_reader` allocates a `Value` for the whole thing. We can't afford that on a 512 MB Cloud worker.
4. CLI binary `agentool-server` accepts `--openapi-url <url>` and prints the parsed `ToolSchema` as JSON to stdout. No HTTP server yet.
5. Snapshot tests with `insta` against fixtures.

**Pause gate A1:** `cargo test -p agentool-core` green, `cargo run --bin agentool-server -- --openapi-url file://core/tests/fixtures/github.openapi.json` prints a non-empty `ToolSchema`. Post the JSON output in the PR.

#### Phase A2 — C++ schema inference via cxx (Weeks 3–5)

**Files touched:**
`core/cpp/schema_infer.{cpp,h}`, `core/cpp/patterns/{mintlify,readme,swagger_ui,gitbook}.cpp`,
`core/src/infer_bridge.rs`, `core/tests/fixtures/pubmed.docs.html`.

**Deliverables:**
1. `schema_infer.h` exposes one C++ class:
   ```cpp
   namespace agentool {
     struct InferredFragment {
       std::string method;      // GET / POST / …
       std::string path;
       std::vector<ParamHint> params;
       std::string auth_hint;   // "bearer" / "api_key" / ""
     };
     std::vector<InferredFragment> infer_endpoints(
         rust::Str url, rust::Str html);
   }
   ```
2. `core/src/infer_bridge.rs` — `#[cxx::bridge]` declaration matching the above.
3. Pattern files under `core/cpp/patterns/` register themselves with a static registry; `schema_infer.cpp` dispatches by detecting which docs platform generated the HTML (look for `<meta name="generator">`, Mintlify's `__NEXT_DATA__`, ReadMe's `data-rdme`, Swagger UI's `swagger-ui-container` class, GitBook's `__gitbook`).
4. The bridge is **never called from `mcp_server.rs`** directly — only from a future "infer-then-serve" path which we wire in Phase A4. In A2, the entry point is the binary flag `agentool-server --infer-from-html <file>`.

**Pause gate A2:** `cargo test --test infer_bridge` green; running against `pubmed.docs.html` produces at least 5 inferred endpoints with sensible HTTP verbs. Post the JSON in the PR.

#### Phase A3 — HTTP client + rate limit + retry (Weeks 5–7)

**Files touched:**
`core/src/{http_client,rate_limit,retry,auth_inject}.rs`, new fixtures in `core/tests/fixtures/`,
`core/tests/mcp_protocol.rs` (placeholder only — filled in A4).

**Deliverables:**

1. `http_client.rs` — a `UpstreamClient` struct that owns one `reqwest::Client` (HTTP/2, connection-pooled). Public surface:
   ```rust
   pub async fn invoke(
       &self,
       schema:  &ToolSchema,
       method:  &str,
       params:  serde_json::Value,
       auth:    Option<&AuthCredentials>,
   ) -> Result<serde_json::Value, AgentoolError>;
   ```
   Internally: resolves the method, builds the URL, injects auth (via `auth_inject.rs`), applies the rate limiter (via `rate_limit.rs`), wraps the actual call in `retry::with_backoff(...)`.

2. `rate_limit.rs` — wraps `governor::DefaultDirectRateLimiter` keyed by `tool_id`. A `DashMap<String, Arc<RateLimiter<…>>>`. The limit comes from `ToolSchema.rate_limit` if present, else a per-host default of 10 rps. Provides:
   ```rust
   pub async fn acquire(&self, tool_id: &str, rps: u32, burst: u32);
   ```

3. `retry.rs` — exponential backoff with jitter (use `backoff::ExponentialBackoff`). Retries only on 429, 502, 503, 504, and `reqwest::Error::is_timeout()`. Plus a **circuit breaker** per `tool_id`:
   - Closed → Open after 5 consecutive failures.
   - Open for 30s, then Half-Open.
   - Half-Open allows 1 probe; success → Closed, failure → Open again with 60s.
   Implementation: `parking_lot::Mutex<CircuitState>` in a `DashMap` keyed by `tool_id`.

4. `auth_inject.rs` — single function:
   ```rust
   pub fn inject(req: &mut reqwest::RequestBuilder,
                 auth_spec: &AuthSpec,
                 creds:     &AuthCredentials);
   ```
   Bearer/API key/Basic. **Never logs the credential.** Tracing fields are explicitly marked `skip(creds)`.

5. Integration tests with `wiremock`:
   - 429 → backoff → eventual success.
   - 5×500 → circuit opens, next call fails fast.
   - Bearer header is present and exactly `Bearer <token>`.

**Pause gate A3:** all wiremock tests green. Post a `cargo test -- --nocapture` log excerpt showing backoff timing.

#### Phase A4 — MCP server (Weeks 7–9)

**Files touched:**
`core/src/mcp_server.rs`, `core/src/bin/agentool_server.rs`, `core/proto/mcp.md`,
`core/tests/mcp_protocol.rs`.

**Deliverables:**

1. `mcp_server.rs` — a Hyper service that accepts JSON-RPC 2.0 over HTTP (POST `/mcp`). Server signature:
   ```rust
   pub async fn serve(schema: Arc<ToolSchema>,
                      addr:   std::net::SocketAddr,
                      shutdown: tokio::sync::oneshot::Receiver<()>)
       -> Result<(), AgentoolError>;
   ```
   Routes:
   - `initialize` → returns server capabilities + protocol version.
   - `tools/list` → maps `schema.methods` to MCP `Tool` objects (name + JSON schema for params derived from `Method.params`).
   - `tools/call` → looks up the method, calls `UpstreamClient::invoke`, returns the JSON result wrapped in MCP `CallToolResult`.
   - `ping` → `{}`.

2. Binary `agentool-server` gains flags:
   ```
   agentool-server serve --schema <path> --port 3000
   agentool-server serve --openapi-url <url> --port 3000   # parse-then-serve
   agentool-server serve --infer-from-url <url> --port 3000 # fetch HTML → C++ infer → serve
   ```

3. `core/proto/mcp.md` — human-readable spec of every supported method with example payloads. **This is the S3 contract Dev B reads.**

4. `core/tests/mcp_protocol.rs` — spins up the server on an ephemeral port, sends `initialize`, `tools/list`, `tools/call` against a wiremock upstream, asserts the JSON-RPC envelopes.

**Pause gate A4:** Dev B can run `cargo run --bin agentool-server -- serve --schema schemas/example.json --port 3000` and POST a `tools/list` request from `curl`. Post the curl transcript.

#### Phase A5 — PyO3 surface + cleanup (Weeks 9–10)

**Files touched:**
`core/src/py.rs`, `core/src/lib.rs` (re-exports), `core/Cargo.toml` (already has pyo3).

**Deliverables:**

1. `py.rs` implements the exact surface from §3.3. No new functions. No clever extras.
2. `lib.rs` exports a `#[pymodule] fn _native(py: Python, m: &PyModule)` that registers every `#[pyfunction]` and `#[pyclass]`. **This is the symbol `sdk/python/src/lib.rs` will re-export to maturin.**
3. `start_mcp_server` spawns a Tokio runtime in a background thread, returns a handle; `McpServerHandle::stop` sends on the oneshot channel and joins the thread.
4. Examples in `core/examples/smoke.rs` exercise the public Rust API end-to-end (parse OpenAPI → serve MCP on a random port → call a mocked upstream → assert).

**Pause gate A5:** `cargo build --release` green; `nm target/release/libagentool_core.dylib | grep _native` (or `.so` on Linux) shows the symbol. Hand off to Dev B.

#### Phase A6 — Hardening + integration support (Weeks 10–12)

Bug triage from Dev B's testing. **No new features in this phase.** Only: fix what Dev B's tests expose, document edge cases in `core/cpp/README.md` and `core/proto/mcp.md`, ship the launch binary.

### 4.4 Dev A's "do not" list (taped to the monitor)

- ❌ Do not import `openapiv3` — write the streaming parser by hand with `struson`.
- ❌ Do not depend on `axum`. We use raw Hyper for control over the JSON-RPC envelope and lower binary size.
- ❌ Do not introduce `anyhow` into library code — only inside `bin/`. Library = `thiserror`.
- ❌ Do not log auth credentials. Ever. Add a `#[deny(clippy::print_stdout)]` lint on `auth_inject.rs`.
- ❌ Do not block the Tokio runtime. The C++ infer call is CPU-bound — wrap calls in `tokio::task::spawn_blocking`.
- ❌ Do not touch anything under `sdk/`.

---

## 5. TRACK B — Dev B: Python SDK + TypeScript CLI/Dashboard

### 5.1 Out of scope for Dev B (do not touch)

- `core/**` (entire subtree)
- `schemas/tool_schema.schema.json` (read-only)
- `core/proto/*.md` (read-only — these are the contracts)
- `.github/workflows/core-*.yml`

### 5.2 Python SDK layout

#### 5.2.1 `sdk/python/pyproject.toml` — exact contents

```toml
[build-system]
requires      = ["maturin>=1.5,<2.0"]
build-backend = "maturin"

[project]
name            = "agentool"
version         = "0.1.0"
description     = "Universal API → MCP translator. Wrap any API as an agent-callable tool."
readme          = "README.md"
license         = { text = "Apache-2.0" }
requires-python = ">=3.9"
authors         = [{ name = "Samvardhan Singh" }]
keywords        = ["mcp", "agents", "openapi", "ai", "llm"]
classifiers     = [
  "Development Status :: 3 - Alpha",
  "License :: OSI Approved :: Apache Software License",
  "Programming Language :: Python :: 3",
  "Programming Language :: Rust",
  "Topic :: Software Development :: Libraries",
]

dependencies = [
  # NO hard runtime deps beyond the native module.
  # AgentID and AgentMem are intentionally NOT listed here.
  "typing-extensions>=4.0; python_version<'3.11'",
]

[project.optional-dependencies]
record       = ["playwright>=1.42"]          # browser recording
integrations = ["agentid>=0.1", "agentmem>=0.1"]   # opt-in convenience
dev          = [
  "pytest>=8", "pytest-asyncio>=0.23",
  "pytest-mock>=3", "ruff>=0.4", "mypy>=1.9",
  "respx>=0.20",   # HTTPX mocking for tests
]

[project.scripts]
# falls back when the Bun CLI isn't installed
agentool = "agentool.cli_entry:main"

[project.urls]
Homepage = "https://github.com/samvardhan/agentool"
Issues   = "https://github.com/samvardhan/agentool/issues"

[tool.maturin]
manifest-path        = "Cargo.toml"
module-name          = "agentool._native"
features             = ["pyo3/extension-module"]
python-source        = "."        # the `agentool/` package sits next to pyproject
include              = ["agentool/_native.pyi"]
```

The maturin `Cargo.toml` at `sdk/python/Cargo.toml` is **separate** from `core/Cargo.toml`. It's a thin crate that depends on `agentool-core` via a path dep and re-exports the `#[pymodule]`:

```toml
# sdk/python/Cargo.toml
[package]
name    = "agentool-py"
version = "0.1.0"
edition = "2021"

[lib]
name       = "_native"
crate-type = ["cdylib"]
path       = "src/lib.rs"

[dependencies]
pyo3           = { version = "0.21", features = ["extension-module", "abi3-py38"] }
agentool-core  = { path = "../../core" }
```

#### 5.2.2 Phase plan (Track B — Python half)

**Phase B1-py — façade scaffold (Weeks 1–3)**

While Dev A is in A1/A2, Dev B builds the Python façade against a **fake `_native` module**:

```python
# sdk/python/tests/conftest.py — pseudocode
@pytest.fixture(autouse=True)
def fake_native(monkeypatch):
    fake = SimpleNamespace(
        parse_openapi_url = lambda u: FakeSchema(...),
        parse_openapi_str = lambda s: FakeSchema(...),
        infer_from_html   = lambda u, h: FakeSchema(...),
        schema_from_json  = lambda j: FakeSchema(...),
        start_mcp_server  = lambda s, p: FakeHandle(p),
    )
    monkeypatch.setattr("agentool._native", fake, raising=False)
```

This unblocks Dev B for the first 5 weeks while Dev A's Rust isn't ready.

Files filled in B1-py:
- `agentool/__init__.py` → `from .tool import Tool; __all__ = ["Tool"]`
- `agentool/tool.py` → the `Tool` class:
  ```python
  class Tool:
      def __init__(self, url: str | None = None, *,
                   schema: dict | None = None,
                   identity = None,         # optional AgentID
                   memory   = None):        # optional AgentMem
          ...
      @property
      def methods(self) -> list[Method]: ...
      def call(self, method_name: str, **kwargs) -> Any: ...
      def serve_mcp(self, port: int = 3000) -> ServerHandle: ...
      @classmethod
      def from_registry(cls, name: str) -> "Tool": ...
  ```
- `agentool/_native.pyi` → type stubs matching §3.3 exactly.

**Phase B2-py — `record.py` Playwright recorder (Weeks 3–5)**

`record.py` is a standalone script invocable as `python -m agentool.record <URL>`:

1. Launches `playwright.async_api` Chromium with `--auto-open-devtools-for-tabs`.
2. Subscribes to `page.on("request")` and `page.on("response")`.
3. Records every XHR/fetch with method, URL, headers (auth-redacted), request body, response body sample.
4. On `Ctrl-C` or window close, runs a heuristic inferrer in Python (since Dev A's C++ is HTML-only) that:
   - Groups requests by URL template (`/users/{id}` ← `/users/123`, `/users/456`).
   - Names the method after `${verb}_${last_path_segment}` with disambiguation.
   - Writes `tool.schema.json` conforming to `schemas/tool_schema.schema.json` with `provenance.source = "browser_record"`.
5. Validates the output against the JSON Schema using `jsonschema` (added to `record` extra).

**Phase B3-py — integrations (Weeks 5–7)**

`agentool/integrations/agentid.py`:

```python
# Runtime detection only. Never `import agentid` at module top-level.
import importlib.util

def is_available() -> bool:
    return importlib.util.find_spec("agentid") is not None

def inject_credentials(tool: "Tool", identity) -> None:
    """Look up stored credentials for tool.tool_id via the AgentID vault.
       Returns silently if AgentID is not installed."""
    if not is_available():
        return
    from agentid import AgentIdentity   # safe: guarded
    ...
```

`agentool/integrations/agentmem.py`:

```python
def is_available() -> bool:
    return importlib.util.find_spec("agentmem") is not None

def cache_schema(tool_id: str, schema_json: str, memory) -> None:
    if not is_available(): return
    memory.set(f"agentool:schema:{tool_id}", schema_json)

def load_cached(tool_id: str, memory) -> str | None:
    if not is_available(): return None
    return memory.get(f"agentool:schema:{tool_id}")

def log_call(tool_id: str, method: str, latency_ms: int, ok: bool, memory) -> None:
    if not is_available(): return
    memory.log_episode(
        action=f"called {tool_id}.{method}",
        result_summary=("ok" if ok else "fail") + f" in {latency_ms}ms",
        tags=["tool_call", tool_id],
    )
```

The `Tool.__init__` calls `integrations.agentid.inject_credentials(self, identity)` and `integrations.agentmem.cache_schema(...)` defensively. **Both no-op when their respective packages are missing.**

`agentool/integrations/langgraph.py` + `crewai.py` — thin shims that wrap a `Tool` instance as a callable conforming to each framework's tool interface. Same pattern: guarded imports.

**Phase B4-py — real native module + integration tests (Weeks 7–10)**

Once Dev A clears Pause gate A5, Dev B drops the fake `_native` and tests against the real one. Adds:

- `tests/test_serve_mcp.py` — spins up `Tool.serve_mcp(0)`, sends JSON-RPC over httpx, asserts results.
- `tests/test_optional_integrations.py` — three runs in CI:
  1. `pip install agentool` (no extras) — README example runs.
  2. `pip install agentool[integrations]` — AgentID + AgentMem present, integration paths exercised.
  3. `pip install agentool` then `pip install agentid` later — runtime detection kicks in.

### 5.3 TypeScript SDK layout

#### 5.3.1 `sdk/typescript/package.json` — exact contents

```json
{
  "name": "@agentbase/agentool",
  "version": "0.1.0",
  "type": "module",
  "private": false,
  "description": "Agentool CLI + Dashboard (Bun + React).",
  "license": "Apache-2.0",
  "bin": {
    "agentool": "./cli/index.ts"
  },
  "scripts": {
    "dev":          "vite",
    "build":        "tsc -b && vite build --outDir dashboard",
    "preview":      "vite preview",
    "cli":          "bun run cli/index.ts",
    "typecheck":    "tsc --noEmit",
    "lint":         "biome check .",
    "test":         "bun test"
  },
  "dependencies": {
    "react":            "^18.3.0",
    "react-dom":        "^18.3.0",
    "@tanstack/react-query": "^5.30.0",
    "zustand":          "^4.5.0",
    "tailwindcss":      "^3.4.0",
    "lucide-react":     "^0.378.0",
    "clsx":             "^2.1.0",
    "commander":        "^12.0.0",
    "execa":            "^9.1.0",
    "ajv":              "^8.13.0"
  },
  "devDependencies": {
    "@types/react":     "^18.3.0",
    "@types/react-dom": "^18.3.0",
    "@types/bun":       "latest",
    "@vitejs/plugin-react": "^4.3.0",
    "vite":             "^5.2.0",
    "typescript":       "^5.4.0",
    "@biomejs/biome":   "^1.7.0"
  }
}
```

#### 5.3.2 Phase plan (Track B — TypeScript half)

**Phase B1-ts — CLI scaffold (Weeks 2–4)**

`cli/index.ts` uses `commander`:

```
agentool wrap <URL>            # fetch → parse → emit tool.schema.json
agentool serve <schema.json>   # shells out to `agentool-server` binary
agentool record <URL>          # shells out to `python -m agentool.record`
agentool registry list
agentool registry add <name> <path>
agentool registry publish <name>
agentool dashboard --port 7100
```

Implementation rule: **the TS CLI never re-implements logic** that lives in the Rust binary or Python recorder. It is a UX layer that `execa`s `agentool-server` (for `wrap`, `serve`) and `python -m agentool.record` (for `record`). This keeps the merge surface tiny.

`agentool wrap <URL>` flow:
1. Probe URL for `/openapi.json`, `/swagger.json`, `/api-docs`. Fall back to HEAD on the URL itself.
2. If OpenAPI found → call `agentool-server parse --openapi-url <url> --out tool.schema.json`.
3. Else fetch the URL as HTML, call `agentool-server infer --html <file> --url <url> --out tool.schema.json`.
4. Validate the emitted JSON against `schemas/tool_schema.schema.json` using `ajv`. Print summary.

**Phase B2-ts — Schema Explorer dashboard tab (Weeks 4–8)**

This is the most visible UI deliverable. It is added as a new **top-level tab** in the existing Bun + React dashboard layout (AgentMem already establishes the pattern at `agentmem/sdk/typescript/src/App.tsx`).

Component breakdown:

| File | Responsibility |
|---|---|
| `components/SchemaExplorer.tsx` | The tab. Sidebar = `EndpointTree`, main = `EndpointDetail` + `CallTester`. |
| `components/EndpointTree.tsx` | Tree view of `ToolSchema.methods` grouped by HTTP verb / path prefix. Search box. |
| `components/EndpointDetail.tsx` | Renders one `Method`: signature, params table, return shape, auth requirements, provenance pill (openapi / html_infer / browser_record). |
| `components/CallTester.tsx` | Form generated from `Method.params`. "Send" button issues a `tools/call` JSON-RPC over `api/mcp.ts`. Displays response + latency + status. |
| `components/RegistryBrowser.tsx` | Lists `registry/*.schema.json`. "Install" copies into the active workspace. |
| `api/mcp.ts` | Typed JSON-RPC client. One function per S3 method. |
| `api/schema.ts` | TypeScript types **generated** from `schemas/tool_schema.schema.json` via `json-schema-to-typescript` at build time. Never hand-written. |

The generation step is part of the `build` script:

```jsonc
// add to "scripts"
"prebuild": "json2ts ../../schemas/tool_schema.schema.json -o src/api/schema.ts"
```

**Phase B3-ts — Registry folder + community contribution flow (Weeks 8–10)**

`registry/README.md` documents the contribution flow:

1. Run `agentool wrap <URL>` to generate `tool.schema.json`.
2. Validate locally: `agentool validate tool.schema.json`.
3. Copy into `registry/<name>.schema.json`, fill in human-curated `description` fields.
4. Open a PR. CI runs the JSON Schema check + a smoke `tools/list` against the schema using `agentool-server`.

The dashboard's `RegistryBrowser.tsx` fetches `registry/` over GitHub Raw (no backend required for v1).

**Phase B4-ts — Polish + launch assets (Weeks 10–12)**

- Vite production build → `dashboard/` static assets, served by `agentool-server` on `/`.
- README hero animation: a GIF of `agentool wrap https://api.github.com` then opening the dashboard.

### 5.4 Dev B's "do not" list (taped to the monitor)

- ❌ Never `import agentid` or `import agentmem` at module top-level. Always behind `importlib.util.find_spec`.
- ❌ Never edit anything under `core/`. If the PyO3 surface needs a new function, open an issue tagged `track-a`, propose it, wait for Dev A to ship it.
- ❌ Never hand-write `src/api/schema.ts` — it's generated.
- ❌ The TS CLI never re-implements OpenAPI parsing or HTTP retry. Always `execa`.
- ❌ Don't add new top-level dirs. New components go under `sdk/typescript/src/components/`.

---

## 6. Optional Integrations

These are the *only* places where Agentool touches AgentID or AgentMem. All four of them live in Track B (`sdk/python/agentool/integrations/`). The Rust core has **zero knowledge** of either sister product.

| File | What it does | Behavior when sister package missing |
|---|---|---|
| `integrations/agentid.py` | Pull credentials from AgentID vault keyed by `tool_id`; inject into `Tool.call` via PyO3 surface. | No-op silently. `Tool` still works with manually-set credentials. |
| `integrations/agentmem.py` | (a) Cache parsed schemas in AgentMem structured memory under `agentool:schema:<tool_id>`. (b) Log each `tool.call` as an episode. | No-op silently. |
| `integrations/langgraph.py` | Adapter exposing `Tool` as a LangGraph tool. | Requires `langgraph` installed; raises ImportError on import (not on `Tool` use). |
| `integrations/crewai.py` | Same, for CrewAI. | Same. |

**Detection rule:**

```python
import importlib.util
def _has(pkg: str) -> bool:
    return importlib.util.find_spec(pkg) is not None
```

Cache the result at module load. Never retry per-call.

**CI gate (Track B owns):**

```yaml
# .github/workflows/sdk-python.yml — pseudocode
- name: install without integrations
  run: pip install -e .
- name: smoke test (no agentid/agentmem)
  run: pytest -k "not integration"
- name: install with integrations
  run: pip install -e .[integrations]
- name: smoke test (with sister packages)
  run: pytest tests/test_optional_integrations.py
```

---

## 7. Build, Test, CI

### 7.1 Per-track CI files (no cross-edits)

| Workflow file | Owner | Triggers on |
|---|---|---|
| `.github/workflows/core-build.yml` | Dev A | changes under `core/**` or `schemas/**` |
| `.github/workflows/core-test.yml`  | Dev A | same |
| `.github/workflows/sdk-python.yml` | Dev B | changes under `sdk/python/**` or `registry/**` |
| `.github/workflows/sdk-typescript.yml` | Dev B | changes under `sdk/typescript/**` or `registry/**` |

A separate `release.yml` (joint ownership) only runs on tag push and orchestrates: `cargo publish --dry-run` → maturin build wheels → bun build → upload artifacts.

### 7.2 Test pyramid

| Layer | Owner | Tooling | What it covers |
|---|---|---|---|
| Rust unit | Dev A | `cargo test` | schema round-trip, openapi parsing, rate-limit math, retry backoff, circuit breaker state machine |
| Rust integration | Dev A | `cargo test` + `wiremock` | http_client against fake upstreams, MCP JSON-RPC envelope correctness |
| C++ unit | Dev A | `cargo test --test infer_bridge` (drives via cxx) | each pattern file extracts ≥ N endpoints from its fixture |
| Python unit | Dev B | `pytest` + fake `_native` | `Tool` façade behavior, integration guards |
| Python integration | Dev B | `pytest` + real native | end-to-end `Tool(url).call(...)` against a wiremock-like upstream |
| TS unit | Dev B | `bun test` | JSON-RPC client encoding, schema TS-type generation determinism |
| Smoke (cross-track) | Joint | shell | `agentool wrap https://api.github.com` → schema is non-empty → `agentool serve` → `tools/list` returns ≥ 10 methods |

The cross-track smoke runs in a separate workflow that's only required on PRs to `main`. Daily/branch builds run only the owning track's pipeline.

### 7.3 Versioning

Both crates and both packages share one version number, bumped together on each release tag. Pre-1.0 means breaking changes are allowed at every 0.x bump; that buys Track A latitude to evolve the PyO3 surface.

---

## 8. The 12-Week Schedule

Aligned to the blueprint's Weeks 19–30 (Months 5–8). Bracketed entries show the matching Pause gate.

```
WEEK 1  ─── Bootstrap PR (skeleton). Lock S1/S2/S3 contracts.
            DEV A: schema.rs + JSON Schema           [Phase A1 starts]
            DEV B: pyproject.toml + Tool stub        [Phase B1-py starts]

WEEK 2  ─── DEV A: openapi.rs streaming parser
            DEV B: agentool/__init__.py + tool.py façade against fake _native
                   sdk/typescript/package.json + Vite scaffold [B1-ts starts]

WEEK 3  ─── DEV A: openapi tests + fixtures        [Pause gate A1 — ack required]
            DEV B: cli/index.ts wrap/serve/record commands

WEEK 4  ─── DEV A: schema_infer.h + cxx::bridge
            DEV B: dashboard tab routing, App.tsx layout

WEEK 5  ─── DEV A: pattern files (mintlify, readme, swagger_ui, gitbook)
            DEV B: record.py Playwright recorder    [B2-py]

WEEK 6  ─── DEV A: infer_bridge tests              [Pause gate A2]
            DEV B: SchemaExplorer.tsx + EndpointTree.tsx

WEEK 7  ─── DEV A: http_client.rs + rate_limit.rs
            DEV B: integrations/{agentid,agentmem}.py guarded glue [B3-py]

WEEK 8  ─── DEV A: retry.rs + circuit breaker
            DEV B: EndpointDetail.tsx + CallTester.tsx

WEEK 9  ─── DEV A: wiremock integration tests      [Pause gate A3]
            DEV B: real-native swap-in; remove fake _native
                   RegistryBrowser.tsx + registry/ folder seed [B3-ts]

WEEK 10 ─── DEV A: mcp_server.rs JSON-RPC routes
            DEV B: integration tests against real native

WEEK 11 ─── DEV A: agentool-server binary flags    [Pause gate A4]
            DEV B: dashboard polish, launch GIF

WEEK 12 ─── DEV A: py.rs + lib.rs pymodule        [Pause gate A5 → handoff]
            DEV B: README, launch assets, integration PR review
            JOINT: cross-track smoke green; cut v0.1.0 tag
```

### 8.1 Sync rituals

- **Mon 30-min standup** — both devs, asynchronously written.
- **Friday demo** — whoever just cleared a Pause gate posts a 60-sec terminal recording.
- **PR review SLA** — 24 h business; never block the other dev > 1 working day.

---

## 9. Definition of Done

### 9.1 Track A DoD (Dev A signs)

- [ ] `cargo build --release` green on Linux x86_64 + macOS arm64.
- [ ] `cargo test` green; ≥ 80 % line coverage in `openapi.rs`, `http_client.rs`, `retry.rs`.
- [ ] `agentool-server serve --openapi-url https://api.github.com/openapi.json --port 3000` starts in < 2 s and serves `tools/list` in < 50 ms p99.
- [ ] C++ inferrer extracts ≥ 5 endpoints from each of: Mintlify, ReadMe, Swagger UI, GitBook fixture.
- [ ] `schemas/tool_schema.schema.json` finalized; `core::schema::ToolSchema` round-trips against it.
- [ ] `core/src/py.rs` exports exactly the surface in §3.3; `cdylib` symbol present.
- [ ] `core/proto/mcp.md` documents every supported JSON-RPC method with example payloads.
- [ ] No `unsafe` blocks outside the cxx bridge.
- [ ] No credential string ever flows through `tracing::info!` / `println!` (enforced by clippy lint + grep CI step).

### 9.2 Track B DoD (Dev B signs)

- [ ] `pip install agentool` (no extras) — README example runs.
- [ ] `pip install agentool[integrations]` — AgentID credential injection + AgentMem schema caching + episode logging all exercised in tests.
- [ ] `python -m agentool.record https://example.com` opens a browser, records 3 requests, emits a valid `tool.schema.json`.
- [ ] `bun run cli/index.ts wrap https://api.github.com` produces a non-empty schema in < 5 s.
- [ ] Schema Explorer tab renders any valid `tool.schema.json` with sidebar, detail pane, and a working CallTester.
- [ ] `src/api/schema.ts` is regenerated from `schemas/tool_schema.schema.json` in `prebuild`; no drift.
- [ ] `registry/` contains ≥ 3 working schemas (github, stripe, pubmed) that pass `agentool validate`.
- [ ] Zero hard imports of `agentid` or `agentmem` at module load time (grep CI step).

### 9.3 Joint DoD (both sign before tagging v0.1.0)

- [ ] Cross-track smoke workflow green on `main`.
- [ ] README hero animation merged.
- [ ] PyPI dry-run + npm dry-run + crates.io dry-run all pass.
- [ ] `agentool-server` standalone binary is < 25 MB stripped.
- [ ] One end-to-end demo recorded: `agentool wrap https://api.github.com` → open dashboard → call `search_repositories` → see result.

---

## 10. Risk Register

| # | Risk | Likelihood | Impact | Owner | Mitigation |
|---|---|---|---|---|---|
| R1 | OpenAPI spec sizes vary wildly (some specs are 100 MB) — `serde_json` OOMs | High | High | Dev A | Streaming parser via `struson`. Memory-budget test fixture in `core/tests/fixtures/large.openapi.json` (synthetic 80 MB). |
| R2 | libxml2 unavailable on dev's machine | Medium | Medium | Dev A | `build.rs` fails loudly with the install command. Document in `core/cpp/README.md`. |
| R3 | PyO3 surface drift breaks Dev B repeatedly | Medium | High | Both | Lock §3.3 in Week 1; changes require minor-version bump + PR with both reviewers. |
| R4 | Playwright in `record.py` flaky on CI | Medium | Low | Dev B | Mark recorder tests `@pytest.mark.skip_in_ci`; smoke recorder only locally. |
| R5 | MCP spec evolves before v0.1.0 ships | Low | Medium | Dev A | Pin to 2024-11-05 revision; document version in `core/proto/mcp.md`. |
| R6 | Optional-integration runtime detection logs noise in user terminals | Medium | Low | Dev B | All detection results cached at module import; never logged at INFO level. |
| R7 | Schema Explorer becomes a vector for prompt injection (LLM reads attacker-controlled descriptions) | Low | High | Dev B | Strip HTML tags from `description` fields at render time; document the threat in the dashboard footer. |
| R8 | Dev A and Dev B both edit `README.md` and conflict | High | Low | Both | README is split into commented `<!-- DEV A SECTION -->` / `<!-- DEV B SECTION -->` blocks; never cross. |
| R9 | Circuit breaker swallows transient failures permanently | Medium | High | Dev A | Half-Open probe timer is unit-tested with controllable clock; `governor`'s clock injection. |
| R10 | A competitor ships unified product first | Low | Medium | Strategic | Per blueprint §13: speed of individual launches. Agentool ships standalone first; integration is a feature, not the moat. |

---

## Appendix A — Cargo dependency cheat-sheet (Dev A reference)

| Crate | Version | Where used | Why this and not alternative |
|---|---|---|---|
| `tokio` | 1.36 | everywhere async | de-facto runtime; we already know it from AgentMem |
| `hyper` | 1.2 | `mcp_server.rs` | low-level control over JSON-RPC envelope; smaller than axum |
| `reqwest` | 0.12 + rustls | `http_client.rs` | rustls avoids system OpenSSL hassles in CI |
| `struson` | 0.5 | `openapi.rs` | streaming JSON; 30 MB OpenAPI specs are real |
| `governor` | 0.6 | `rate_limit.rs` | clock-injectable token bucket, well-tested |
| `backoff` | 0.4 | `retry.rs` | exponential + jitter built in; works with `tokio` feature |
| `cxx` | 1.0 | `infer_bridge.rs` | same bridge crate AgentMem uses; consistent monorepo style |
| `pyo3` | 0.21 + abi3 | `py.rs` | abi3 means one wheel covers Python 3.8+ |
| `tracing` | 0.1 | everywhere | structured logging compatible with OpenTelemetry exporters later |
| `dashmap` | 5.5 | rate-limit/circuit state | lock-free concurrent map; per-tool keying |
| `wiremock` | 0.6 (dev) | tests | upstream API simulator |

## Appendix B — Glossary

- **MCP** — Model Context Protocol, the JSON-RPC 2.0 protocol Anthropic published (2024-11-05) for agent ↔ tool communication.
- **ToolSchema** — Agentool's internal data model. One per wrapped API. See §3.
- **OpenAPI 3.x** — REST API description format. Agentool's "fast path" input.
- **cxx** — Rust crate that generates the C++ ↔ Rust bridge code at build time.
- **Pause gate** — A hard stop in Dev A's schedule where they post evidence (CI run + transcript) and wait for Dev B's ack before continuing. Prevents Dev A from getting weeks ahead and breaking the seam contracts unilaterally.
- **Provenance** — `ToolSchema.provenance.source` field: `openapi` / `html_infer` / `browser_record`. Lets the dashboard show *how* a schema was generated.

---

*Three tools. Four launches. One platform. Phase 3 starts here.*

— Authored against `agentbase_phased_blueprint.md`, Section 5.
— Repo target: `github.com/samvardhan/agentool`.
— License: Apache 2.0.
