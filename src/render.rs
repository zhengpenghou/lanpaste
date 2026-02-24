use pulldown_cmark::{Options, Parser, html};

use crate::types::RecentItem;

pub fn render_markdown(md: &str) -> String {
    let parser = Parser::new_ext(md, Options::all());
    let mut html_out = String::new();
    html::push_html(&mut html_out, parser);
    let mut builder = ammonia::Builder::default();
    builder
        .add_tag_attributes("pre", &["class"])
        .add_tag_attributes("code", &["class"]);
    let sanitized = builder.clean(&html_out).to_string();
    promote_mermaid_blocks(&sanitized)
}

pub fn looks_like_markdown(text: &str) -> bool {
    let s = text.trim();
    if s.is_empty() {
        return false;
    }
    s.lines().any(|line| {
        let l = line.trim_start();
        l.starts_with("#")
            || l.starts_with("```")
            || l.starts_with("~~~")
            || l.starts_with("- ")
            || l.starts_with("* ")
            || l.starts_with("> ")
            || l.starts_with("|")
            || l.starts_with("1. ")
    })
}

pub fn render_page(title: &str, body_html: &str) -> String {
    format!(
        "<!doctype html><html><head>\
<meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{}</title>\
<link rel=\"stylesheet\" href=\"https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.css\">\
<style>\
body{{font-family:ui-sans-serif,system-ui,-apple-system,Segoe UI,Roboto,Ubuntu,Helvetica,Arial,sans-serif;max-width:980px;margin:24px auto;padding:0 16px;line-height:1.6;color:#111;}}\
h1,h2,h3,h4,h5,h6{{line-height:1.25;margin-top:1.25em;}}\
pre{{background:#f6f8fa;padding:12px;border-radius:8px;overflow-x:auto;tab-size:4;}}\
code{{font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,Liberation Mono,monospace;tab-size:4;}}\
p code,li code{{background:#f2f2f2;padding:0.1em 0.35em;border-radius:4px;}}\
table{{border-collapse:collapse;width:100%;display:block;overflow-x:auto;}}\
th,td{{border:1px solid #ddd;padding:8px;text-align:left;vertical-align:top;}}\
thead th{{background:#f6f8fa;}}\
blockquote{{margin:1em 0;padding:0.1em 1em;border-left:4px solid #ddd;color:#444;background:#fafafa;}}\
.mermaid{{background:#fff;border:1px solid #eee;border-radius:8px;padding:8px;}}\
</style>\
</head><body>{}\
<script defer src=\"https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.js\"></script>\
<script defer src=\"https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/contrib/auto-render.min.js\"></script>\
<script type=\"module\">import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs';mermaid.initialize({{startOnLoad:true,securityLevel:'strict'}});</script>\
<script>\
window.addEventListener('DOMContentLoaded',function(){{\
if(window.renderMathInElement){{\
window.renderMathInElement(document.body,{{\
delimiters:[{{left:'$$',right:'$$',display:true}},{{left:'$',right:'$',display:false}},{{left:'\\\\(',right:'\\\\)',display:false}},{{left:'\\\\[',right:'\\\\]',display:true}}],\
throwOnError:false\
}});\
}}\
}});\
</script>\
</body></html>",
        html_escape(title),
        body_html
    )
}

pub fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn html_unescape_minimal(input: &str) -> String {
    input
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn promote_mermaid_blocks(html_in: &str) -> String {
    let start_tag = "<pre><code class=\"language-mermaid\">";
    let end_tag = "</code></pre>";
    let mut out = String::with_capacity(html_in.len());
    let mut cursor = 0usize;
    while let Some(start_rel) = html_in[cursor..].find(start_tag) {
        let start = cursor + start_rel;
        out.push_str(&html_in[cursor..start]);
        let inner_start = start + start_tag.len();
        if let Some(end_rel) = html_in[inner_start..].find(end_tag) {
            let end = inner_start + end_rel;
            let code = &html_in[inner_start..end];
            out.push_str("<div class=\"mermaid\">");
            out.push_str(&html_unescape_minimal(code));
            out.push_str("</div>");
            cursor = end + end_tag.len();
        } else {
            out.push_str(&html_in[start..]);
            return out;
        }
    }
    out.push_str(&html_in[cursor..]);
    out
}

pub fn slug_from_rel_path(rel_path: &str) -> Option<String> {
    let file_name = std::path::Path::new(rel_path).file_name()?.to_str()?;
    let stem = file_name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(file_name);
    let (_, slug) = stem.split_once("__")?;
    if slug.is_empty() {
        None
    } else {
        Some(slug.to_string())
    }
}

pub fn render_dashboard(recent: &[RecentItem]) -> String {
    let mut rows = String::new();
    if recent.is_empty() {
        rows.push_str(
            "<tr><td colspan=\"6\">No pastes yet. POST to <code>/api/v1/paste</code> to create one.</td></tr>",
        );
    } else {
        for item in recent {
            let id = html_escape(&item.id);
            let slug = slug_from_rel_path(&item.path).unwrap_or_else(|| "paste".to_string());
            let slug = html_escape(&slug);
            let path = html_escape(&item.path);
            let tag = html_escape(item.tag.as_deref().unwrap_or("-"));
            let created = html_escape(&item.created_at.to_string());
            let ctype = html_escape(&item.content_type);
            rows.push_str(&format!(
                "<tr>\
                    <td><a href=\"/p/{id}/{slug}\">{id}</a></td>\
                    <td>{created}</td>\
                    <td>{tag}</td>\
                    <td>{ctype}</td>\
                    <td>{}</td>\
                    <td><a href=\"/api/v1/p/{id}\">meta</a> · <a href=\"/p/{id}/md\">md</a> · <code>{path}</code></td>\
                </tr>",
                item.size
            ));
        }
    }

    let body = format!(
        "<h1>LAN Paste Dashboard</h1>\
         <p>Quick API entry points:</p>\
         <ul>\
           <li><a href=\"/api\">/api</a> (index)</li>\
           <li><a href=\"/api/v1/recent?n=20\">/api/v1/recent?n=20</a></li>\
           <li><code>POST /api/v1/paste?name=note.md&amp;tag=demo</code></li>\
         </ul>\
         <h2>Recent Pastes</h2>\
         <table border=\"1\" cellpadding=\"6\" cellspacing=\"0\">\
           <thead>\
             <tr><th>ID</th><th>Created</th><th>Tag</th><th>Content-Type</th><th>Bytes</th><th>Links</th></tr>\
           </thead>\
           <tbody>{rows}</tbody>\
         </table>"
    );
    render_page("LAN Paste Dashboard", &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    #[test]
    fn markdown_is_sanitized() {
        let out = render_markdown("# hi\n\n<script>alert(1)</script>");
        assert!(out.contains("<h1>hi</h1>"));
        assert!(!out.contains("<script>"));
    }

    #[test]
    fn markdown_supports_table() {
        let out = render_markdown("| a | b |\n|---|---|\n| 1 | 2 |");
        assert!(out.contains("<table>"));
        assert!(out.contains("<td>1</td>"));
    }

    #[test]
    fn mermaid_fence_promoted() {
        let out = render_markdown("```mermaid\ngraph TD;\nA-->B;\n```");
        assert!(out.contains("<div class=\"mermaid\">"));
        assert!(out.contains("graph TD;"));
    }

    #[test]
    fn markdown_heuristic_works() {
        assert!(looks_like_markdown("# title\ntext"));
        assert!(looks_like_markdown("```rs\nfn main(){}\n```"));
        assert!(!looks_like_markdown("just plain text"));
    }

    #[test]
    fn page_wraps_body() {
        let out = render_page("x", "<p>ok</p>");
        assert!(out.contains("<title>x</title>"));
        assert!(out.contains("<p>ok</p>"));
        assert!(out.contains("katex"));
        assert!(out.contains("mermaid"));
    }

    #[test]
    fn escape_works() {
        assert_eq!(html_escape("<>&\"'"), "&lt;&gt;&amp;&quot;&#39;");
    }

    #[test]
    fn dashboard_contains_api_links() {
        let out = render_dashboard(&[RecentItem {
            id: "01TEST".to_string(),
            created_at: OffsetDateTime::now_utc(),
            path: "pastes/2026/02/13/01TEST__note.md.md".to_string(),
            commit: "abc123".to_string(),
            tag: Some("demo".to_string()),
            size: 12,
            content_type: "text/markdown".to_string(),
        }]);
        assert!(out.contains("LAN Paste Dashboard"));
        assert!(out.contains("/api/v1/paste"));
        assert!(out.contains("/p/01TEST/md"));
        assert!(out.contains("/p/01TEST/note.md"));
    }

    #[test]
    fn slug_extract_works() {
        let slug = slug_from_rel_path("pastes/2026/02/13/01TEST__note.md.md").expect("slug");
        assert_eq!(slug, "note.md");
    }
}
