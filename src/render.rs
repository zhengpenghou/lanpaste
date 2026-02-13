use pulldown_cmark::{Options, Parser, html};

use crate::types::RecentItem;

pub fn render_markdown(md: &str) -> String {
    let parser = Parser::new_ext(md, Options::all());
    let mut html_out = String::new();
    html::push_html(&mut html_out, parser);
    ammonia::clean(&html_out)
}

pub fn render_page(title: &str, body_html: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title></head><body>{}</body></html>",
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

pub fn render_dashboard(recent: &[RecentItem]) -> String {
    let mut rows = String::new();
    if recent.is_empty() {
        rows.push_str(
            "<tr><td colspan=\"6\">No pastes yet. POST to <code>/api/v1/paste</code> to create one.</td></tr>",
        );
    } else {
        for item in recent {
            let id = html_escape(&item.id);
            let path = html_escape(&item.path);
            let tag = html_escape(item.tag.as_deref().unwrap_or("-"));
            let created = html_escape(&item.created_at.to_string());
            let ctype = html_escape(&item.content_type);
            rows.push_str(&format!(
                "<tr>\
                    <td><a href=\"/p/{id}\">{id}</a></td>\
                    <td>{created}</td>\
                    <td>{tag}</td>\
                    <td>{ctype}</td>\
                    <td>{}</td>\
                    <td><a href=\"/api/v1/p/{id}\">meta</a> · <a href=\"/api/v1/p/{id}/raw\">raw</a> · <code>{path}</code></td>\
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
    fn page_wraps_body() {
        let out = render_page("x", "<p>ok</p>");
        assert!(out.contains("<title>x</title>"));
        assert!(out.contains("<p>ok</p>"));
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
            path: "pastes/2026/02/13/01TEST__note.md".to_string(),
            commit: "abc123".to_string(),
            tag: Some("demo".to_string()),
            size: 12,
            content_type: "text/markdown".to_string(),
        }]);
        assert!(out.contains("LAN Paste Dashboard"));
        assert!(out.contains("/api/v1/paste"));
        assert!(out.contains("/api/v1/p/01TEST/raw"));
    }
}
