use pulldown_cmark::{Options, Parser, html};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
