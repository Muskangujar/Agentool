#include "schema_infer.h"

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

// ── Platform detection ────────────────────────────────────────────────────────
// ReadMe.com signals:
//  - <meta name="readme">
//  - Attributes: data-rdme, data-readme
//  - Class "rdme-" prefix
//  - Link to hub.readme.com

bool detect_readme(xmlDocPtr doc) {
    if (_xpath_exists(doc, "//meta[@name='readme' or @name='ReadMe']")) return true;
    if (_xpath_exists(doc, "//*[@data-rdme or @data-readme]"))          return true;
    if (_xpath_exists(doc, "//*[contains(@class,'rdme-')]"))             return true;
    return false;
}

// ── Endpoint extraction ───────────────────────────────────────────────────────
// ReadMe puts endpoint info in .api-reference sections.
// Method+path are in adjacent spans or a header like "GET /path".

void extract_readme(xmlDocPtr doc, std::vector<InferredFragment>& out) {
    xmlXPathContextPtr ctx = xmlXPathNewContext(doc);
    if (!ctx) { extract_generic(doc, out); return; }

    // ReadMe often marks methods with data-method + data-path on a wrapper div
    xmlXPathObjectPtr obj = xmlXPathEvalExpression(
        reinterpret_cast<const xmlChar*>("//*[@data-method]"), ctx);

    if (obj && obj->type == XPATH_NODESET && obj->nodesetval) {
        for (int i = 0; i < obj->nodesetval->nodeNr; ++i) {
            xmlNodePtr node = obj->nodesetval->nodeTab[i];
            xmlChar* m = xmlGetProp(node,
                reinterpret_cast<const xmlChar*>("data-method"));
            xmlChar* p = xmlGetProp(node,
                reinterpret_cast<const xmlChar*>("data-path"));
            if (m && p) {
                std::string method(reinterpret_cast<const char*>(m));
                std::string path(reinterpret_cast<const char*>(p));
                if (!path.empty() && path[0] == '/') {
                    out.push_back({method, path, ""});
                }
            }
            if (m) xmlFree(m);
            if (p) xmlFree(p);
        }
    }
    if (obj) xmlXPathFreeObject(obj);
    xmlXPathFreeContext(ctx);

    if (out.empty()) {
        extract_generic(doc, out);
    }
}

} // namespace agentool
