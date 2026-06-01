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

// ── Platform detection ────────────────────────────────────────────────────────
// Swagger UI signals:
//  - <div id="swagger-ui"> (the React mount point)
//  - Class "swagger-ui" on a container
//  - Class "opblock" on operation blocks

bool detect_swagger_ui(xmlDocPtr doc) {
    if (_xpath_exists(doc, "//*[@id='swagger-ui']"))           return true;
    if (_xpath_exists(doc, "//*[contains(@class,'swagger-ui')]")) return true;
    if (_xpath_exists(doc, "//*[contains(@class,'opblock')]")) return true;
    return false;
}

// ── Endpoint extraction ───────────────────────────────────────────────────────
// Swagger UI renders each endpoint as an .opblock div.
// The HTTP method is in a .opblock-summary-method span.
// The path is in a .opblock-summary-path span (or data-path attribute).

void extract_swagger_ui(xmlDocPtr doc, std::vector<InferredFragment>& out) {
    xmlXPathContextPtr ctx = xmlXPathNewContext(doc);
    if (!ctx) { extract_generic(doc, out); return; }

    // The most reliable selector: divs whose class contains "opblock" have
    // children with method and path spans.
    xmlXPathObjectPtr blocks = xmlXPathEvalExpression(
        reinterpret_cast<const xmlChar*>(
            "//*[contains(@class,'opblock-summary')]"), ctx);

    if (blocks && blocks->type == XPATH_NODESET && blocks->nodesetval) {
        for (int i = 0; i < blocks->nodesetval->nodeNr; ++i) {
            xmlNodePtr block = blocks->nodesetval->nodeTab[i];

            // Walk children to find method and path spans
            std::string method, path;
            for (xmlNodePtr child = block->children; child; child = child->next) {
                if (child->type != XML_ELEMENT_NODE) continue;
                xmlChar* cls = xmlGetProp(child,
                    reinterpret_cast<const xmlChar*>("class"));
                if (!cls) continue;
                std::string c(reinterpret_cast<const char*>(cls));
                xmlFree(cls);

                if (c.find("opblock-summary-method") != std::string::npos) {
                    xmlChar* t = xmlNodeGetContent(child);
                    if (t) { method = reinterpret_cast<const char*>(t); xmlFree(t); }
                } else if (c.find("opblock-summary-path") != std::string::npos) {
                    // Try data-path attribute first, then text content
                    xmlChar* dp = xmlGetProp(child,
                        reinterpret_cast<const xmlChar*>("data-path"));
                    if (dp) {
                        path = reinterpret_cast<const char*>(dp);
                        xmlFree(dp);
                    } else {
                        xmlChar* t = xmlNodeGetContent(child);
                        if (t) { path = reinterpret_cast<const char*>(t); xmlFree(t); }
                    }
                }
            }

            // Trim whitespace
            while (!method.empty() && method.back() == ' ') method.pop_back();
            while (!path.empty()   && path.back()   == ' ') path.pop_back();

            if (!method.empty() && !path.empty() && path[0] == '/') {
                out.push_back({method, path, ""});
            }
        }
    }
    if (blocks) xmlXPathFreeObject(blocks);
    xmlXPathFreeContext(ctx);

    if (out.empty()) {
        extract_generic(doc, out);
    }
}

} // namespace agentool
