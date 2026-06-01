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
// GitBook signals:
//  - JavaScript global __gitbook
//  - Class "gitbook-" prefix
//  - <meta name="generator" content="GitBook ...">

bool detect_gitbook(xmlDocPtr doc) {
    if (_xpath_exists(doc,
            "//meta[contains(translate(@content,'GITBOOK','gitbook'),'gitbook')]"))
        return true;
    if (_xpath_exists(doc, "//*[contains(@class,'gitbook-')]")) return true;
    if (_xpath_exists(doc, "//*[@id='gitbook-plugin-page-toc']")) return true;
    return false;
}

// ── Endpoint extraction ───────────────────────────────────────────────────────
// GitBook API docs typically use HTTP method + path in a header or code block.
// We look for the pattern inside h3/h4 headings (common in manually-authored
// GitBook API docs) before falling back to the generic code-block scanner.

void extract_gitbook(xmlDocPtr doc, std::vector<InferredFragment>& out) {
    // Headings in GitBook often contain "METHOD /path" descriptions
    const char* heading_xpath = "//h2 | //h3 | //h4";
    xmlXPathContextPtr ctx = xmlXPathNewContext(doc);
    if (!ctx) { extract_generic(doc, out); return; }

    static const char* const verbs[] = {
        "GET", "POST", "PUT", "DELETE", "PATCH", nullptr
    };

    xmlXPathObjectPtr obj = xmlXPathEvalExpression(
        reinterpret_cast<const xmlChar*>(heading_xpath), ctx);

    if (obj && obj->type == XPATH_NODESET && obj->nodesetval) {
        for (int i = 0; i < obj->nodesetval->nodeNr; ++i) {
            xmlNodePtr node = obj->nodesetval->nodeTab[i];
            xmlChar* text = xmlNodeGetContent(node);
            if (!text) continue;
            std::string content(reinterpret_cast<const char*>(text));
            xmlFree(text);

            // Check if heading starts with an HTTP verb
            for (const char* const* v = verbs; *v; ++v) {
                const std::string candidate(*v);
                if (content.size() > candidate.size() + 1
                    && content.compare(0, candidate.size(), candidate) == 0
                    && content[candidate.size()] == ' ') {
                    std::string path = content.substr(candidate.size() + 1);
                    // Strip trailing whitespace
                    while (!path.empty() && (path.back() == ' ' || path.back() == '\n'))
                        path.pop_back();
                    if (!path.empty() && path[0] == '/') {
                        out.push_back({candidate, path, ""});
                    }
                    break;
                }
            }
        }
    }
    if (obj) xmlXPathFreeObject(obj);
    xmlXPathFreeContext(ctx);

    if (out.empty()) {
        extract_generic(doc, out);
    }
}

} // namespace agentool
