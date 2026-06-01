#pragma once
#include <string>
#include <vector>
#include <libxml/HTMLparser.h>
#include <libxml/xpath.h>
#include "rust/cxx.h"

namespace agentool {

// ── Shared data types ─────────────────────────────────────────────────────────

struct InferredFragment {
    std::string method;    // "GET", "POST", etc.
    std::string path;      // "/absolute/path"
    std::string auth_hint; // "bearer", "api_key", "basic", or ""
};

// ── CXX bridge entry point ────────────────────────────────────────────────────
// Returns a JSON array: [{"method":"GET","path":"/foo","auth_hint":"bearer"},...]
rust::String infer_endpoints_json(rust::Str url, rust::Str html);

// ── C++ internal API ──────────────────────────────────────────────────────────
std::vector<InferredFragment> infer_endpoints(
    const std::string& url,
    const std::string& html
);

// Generic extractor — falls back to <code>VERB /path</code> pattern scanning.
// Exposed so pattern files can call it as their own fallback.
void extract_generic(xmlDocPtr doc, std::vector<InferredFragment>& out);

// ── Platform detect / extract pairs (implemented in patterns/*.cpp) ───────────
bool detect_mintlify(xmlDocPtr doc);
void extract_mintlify(xmlDocPtr doc, std::vector<InferredFragment>& out);

bool detect_readme(xmlDocPtr doc);
void extract_readme(xmlDocPtr doc, std::vector<InferredFragment>& out);

bool detect_swagger_ui(xmlDocPtr doc);
void extract_swagger_ui(xmlDocPtr doc, std::vector<InferredFragment>& out);

bool detect_gitbook(xmlDocPtr doc);
void extract_gitbook(xmlDocPtr doc, std::vector<InferredFragment>& out);

} // namespace agentool
