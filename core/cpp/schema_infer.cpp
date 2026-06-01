#include "schema_infer.h"
#include <algorithm>
#include <set>
#include <sstream>
#include <cstring>
#include <cstdio>

namespace agentool {

// ── Cast helpers ──────────────────────────────────────────────────────────────

static inline const xmlChar* xc(const char* s) noexcept {
    return reinterpret_cast<const xmlChar*>(s);
}
static inline const char* cc(const xmlChar* s) noexcept {
    return reinterpret_cast<const char*>(s);
}

// ── XPath helpers ─────────────────────────────────────────────────────────────

static bool xpath_exists(xmlDocPtr doc, const char* expr) {
    xmlXPathContextPtr ctx = xmlXPathNewContext(doc);
    if (!ctx) return false;
    xmlXPathObjectPtr obj = xmlXPathEvalExpression(xc(expr), ctx);
    bool found = (obj
                  && obj->type == XPATH_NODESET
                  && obj->nodesetval
                  && obj->nodesetval->nodeNr > 0);
    if (obj) xmlXPathFreeObject(obj);
    xmlXPathFreeContext(ctx);
    return found;
}

static std::vector<std::string> xpath_texts(xmlDocPtr doc, const char* expr) {  // NOLINT
    std::vector<std::string> out;
    xmlXPathContextPtr ctx = xmlXPathNewContext(doc);
    if (!ctx) return out;
    xmlXPathObjectPtr obj = xmlXPathEvalExpression(xc(expr), ctx);
    if (obj && obj->type == XPATH_NODESET && obj->nodesetval) {
        for (int i = 0; i < obj->nodesetval->nodeNr; ++i) {
            xmlNodePtr node = obj->nodesetval->nodeTab[i];
            xmlChar* text = xmlNodeGetContent(node);
            if (text) {
                out.emplace_back(cc(text));
                xmlFree(text);
            }
        }
    }
    if (obj) xmlXPathFreeObject(obj);
    xmlXPathFreeContext(ctx);
    return out;
}

static std::vector<std::string> xpath_attrs(
    xmlDocPtr doc, const char* node_expr, const char* attr_name)
{
    std::vector<std::string> out;
    xmlXPathContextPtr ctx = xmlXPathNewContext(doc);
    if (!ctx) return out;
    xmlXPathObjectPtr obj = xmlXPathEvalExpression(xc(node_expr), ctx);
    if (obj && obj->type == XPATH_NODESET && obj->nodesetval) {
        for (int i = 0; i < obj->nodesetval->nodeNr; ++i) {
            xmlNodePtr node = obj->nodesetval->nodeTab[i];
            xmlChar* val = xmlGetProp(node, xc(attr_name));
            if (val) {
                out.emplace_back(cc(val));
                xmlFree(val);
            }
        }
    }
    if (obj) xmlXPathFreeObject(obj);
    xmlXPathFreeContext(ctx);
    return out;
}

// ── Verb/path parser ──────────────────────────────────────────────────────────

static const char* const k_verbs[] = {
    "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", nullptr
};

static bool parse_verb_path(const std::string& raw,
                             std::string& verb_out,
                             std::string& path_out)
{
    // Strip leading whitespace
    size_t s = raw.find_first_not_of(" \t\n\r\f\v");
    if (s == std::string::npos) return false;
    const std::string text = raw.substr(s);

    for (const char* const* vp = k_verbs; *vp; ++vp) {
        const size_t vlen = std::strlen(*vp);
        if (text.size() <= vlen) continue;
        if (text.compare(0, vlen, *vp) == 0 && text[vlen] == ' ') {
            std::string path = text.substr(vlen + 1);
            // Strip trailing whitespace
            size_t e = path.find_last_not_of(" \t\n\r\f\v");
            if (e == std::string::npos) return false;
            path = path.substr(0, e + 1);
            // Must start with '/'
            if (path.empty() || path[0] != '/') return false;
            verb_out = *vp;
            path_out = path;
            return true;
        }
    }
    return false;
}

// ── JSON serialiser ───────────────────────────────────────────────────────────

static std::string json_esc(const std::string& s) {
    std::string out;
    out.reserve(s.size() + 4);
    for (unsigned char c : s) {
        switch (c) {
            case '"':  out += "\\\""; break;
            case '\\': out += "\\\\"; break;
            case '\n': out += "\\n";  break;
            case '\r': out += "\\r";  break;
            case '\t': out += "\\t";  break;
            default:
                if (c < 0x20) {
                    char buf[8];
                    std::snprintf(buf, sizeof(buf), "\\u%04x", c);
                    out += buf;
                } else {
                    out += static_cast<char>(c);
                }
        }
    }
    return out;
}

static std::string to_json(const std::vector<InferredFragment>& frags) {
    std::string out = "[";
    bool first = true;
    for (const auto& f : frags) {
        if (!first) out += ',';
        first = false;
        out += "{\"method\":\"" + json_esc(f.method)    + "\","
               "\"path\":\""   + json_esc(f.path)       + "\","
               "\"auth_hint\":\""+ json_esc(f.auth_hint) + "\"}";
    }
    out += ']';
    return out;
}

// ── Generic extractor ─────────────────────────────────────────────────────────

void extract_generic(xmlDocPtr doc, std::vector<InferredFragment>& out) {
    std::set<std::string> seen;

    // Primary: <code>VERB /path</code>
    for (const auto& text : xpath_texts(doc, "//code")) {
        std::string verb, path;
        if (parse_verb_path(text, verb, path) && seen.insert(verb + path).second) {
            out.push_back({verb, path, ""});
        }
    }

    // Secondary: <pre> blocks (may contain multi-line examples)
    if (out.size() < 3) {
        for (const auto& block : xpath_texts(doc, "//pre")) {
            std::istringstream ss(block);
            std::string line;
            while (std::getline(ss, line)) {
                std::string verb, path;
                if (parse_verb_path(line, verb, path) && seen.insert(verb + path).second) {
                    out.push_back({verb, path, ""});
                }
            }
        }
    }

    // Tertiary: data-method / data-path attributes (custom doc frameworks)
    {
        auto methods = xpath_attrs(doc, "//*[@data-method]", "data-method");
        auto paths   = xpath_attrs(doc, "//*[@data-path]",   "data-path");
        const size_t n = std::min(methods.size(), paths.size());
        for (size_t i = 0; i < n; ++i) {
            if (!methods[i].empty() && !paths[i].empty() && paths[i][0] == '/'
                && seen.insert(methods[i] + paths[i]).second) {
                out.push_back({methods[i], paths[i], ""});
            }
        }
    }
}

// ── Platform dispatch ─────────────────────────────────────────────────────────

struct Handler {
    bool (*detect)(xmlDocPtr);
    void (*extract)(xmlDocPtr, std::vector<InferredFragment>&);
};

static const Handler k_handlers[] = {
    {detect_mintlify,   extract_mintlify},
    {detect_readme,     extract_readme},
    {detect_swagger_ui, extract_swagger_ui},
    {detect_gitbook,    extract_gitbook},
};

std::vector<InferredFragment> infer_endpoints(
    const std::string& url,
    const std::string& html)
{
    std::vector<InferredFragment> result;
    if (html.empty()) return result;

    htmlDocPtr doc = htmlReadMemory(
        html.c_str(),
        static_cast<int>(html.size()),
        url.c_str(),
        "UTF-8",
        HTML_PARSE_NOWARNING | HTML_PARSE_NOERROR | HTML_PARSE_RECOVER
    );
    if (!doc) return result;

    bool matched = false;
    for (const auto& h : k_handlers) {
        if (h.detect(doc)) {
            h.extract(doc, result);
            matched = true;
            break;
        }
    }

    if (!matched || result.empty()) {
        extract_generic(doc, result);
    }

    xmlFreeDoc(doc);
    return result;
}

// ── CXX bridge ────────────────────────────────────────────────────────────────

rust::String infer_endpoints_json(rust::Str url, rust::Str html) {
    const std::string url_s(url.data(), url.size());
    const std::string html_s(html.data(), html.size());
    const std::string json = to_json(infer_endpoints(url_s, html_s));
    return rust::String(json.c_str(), json.size());
}

} // namespace agentool
