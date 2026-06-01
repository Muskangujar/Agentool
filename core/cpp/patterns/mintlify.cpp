#include "schema_infer.h"
#include <cstring>

namespace agentool {

// ── XPath helper (private to this TU) ────────────────────────────────────────

static bool _xpath_exists(xmlDocPtr doc, const char* expr) {
    xmlXPathContextPtr ctx = xmlXPathNewContext(doc);
    if (!ctx) return false;
    xmlXPathObjectPtr obj = xmlXPathEvalExpression(
        reinterpret_cast<const xmlChar*>(expr), ctx);
    bool found = (obj && obj->type == XPATH_NODESET
                  && obj->nodesetval && obj->nodesetval->nodeNr > 0);
    if (obj) xmlXPathFreeObject(obj);
    xmlXPathFreeContext(ctx);
    return found;
}

static std::string _attr(xmlNodePtr node, const char* name) {
    xmlChar* v = xmlGetProp(node, reinterpret_cast<const xmlChar*>(name));
    if (!v) return "";
    std::string s(reinterpret_cast<const char*>(v));
    xmlFree(v);
    return s;
}

// ── Platform detection ────────────────────────────────────────────────────────
// Mintlify renders with Next.js. Signals:
//  - <script id="__NEXT_DATA__"> present
//  - <meta generator="Mintlify"> or content containing "mintlify"
//  - Elements with class names prefixed "mintlify-"

bool detect_mintlify(xmlDocPtr doc) {
    if (_xpath_exists(doc, "//script[@id='__NEXT_DATA__']")) return true;
    if (_xpath_exists(doc, "//*[contains(@class,'mintlify')]")) return true;
    if (_xpath_exists(doc,
            "//meta[contains(translate(@content,'MINTLFY','mintlfy'),'mintlif')]"))
        return true;
    return false;
}

// ── Endpoint extraction ───────────────────────────────────────────────────────
// Mintlify documents endpoints using a combination of:
//  - Method badges: <span class="... badge ...">GET</span> next to a path
//  - API reference divs: <div class="api-method"> or similar
//
// The extractor collects (method, path) pairs from these structures; if it
// finds nothing, it falls back to the generic <code>VERB /path</code> scanner.

void extract_mintlify(xmlDocPtr doc, std::vector<InferredFragment>& out) {
    xmlXPathContextPtr ctx = xmlXPathNewContext(doc);
    if (!ctx) { extract_generic(doc, out); return; }

    // Try: elements with a data-method attribute (Mintlify sometimes emits these)
    xmlXPathObjectPtr obj = xmlXPathEvalExpression(
        reinterpret_cast<const xmlChar*>("//*[@data-method and @data-path]"), ctx);

    if (obj && obj->type == XPATH_NODESET && obj->nodesetval) {
        for (int i = 0; i < obj->nodesetval->nodeNr; ++i) {
            xmlNodePtr node = obj->nodesetval->nodeTab[i];
            std::string method = _attr(node, "data-method");
            std::string path   = _attr(node, "data-path");
            if (!method.empty() && !path.empty() && path[0] == '/') {
                out.push_back({method, path, ""});
            }
        }
    }
    if (obj) xmlXPathFreeObject(obj);
    xmlXPathFreeContext(ctx);

    // Fallback to generic if platform-specific extraction yielded nothing
    if (out.empty()) {
        extract_generic(doc, out);
    }
}

} // namespace agentool
