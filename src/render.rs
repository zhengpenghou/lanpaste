use pulldown_cmark::{Options, Parser, html};

use crate::types::RecentItem;

const PAGE_CSS: &str = r#"
:root {
  --bg: #f2f4f7;
  --panel: #ffffff;
  --panel-muted: #f7f9fc;
  --text: #122033;
  --text-dim: #4a5a70;
  --border: #d7e0ea;
  --link: #1f5fae;
  --link-hover: #15457e;
  --code-bg: #0f1726;
  --code-fg: #f8fbff;
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  background: radial-gradient(circle at 100% 0%, #e8eef8 0%, var(--bg) 45%);
  color: var(--text);
  font-family: "IBM Plex Sans", "Segoe UI", "Helvetica Neue", Arial, sans-serif;
  font-size: clamp(1rem, 0.94rem + 0.25vw, 1.125rem);
  line-height: 1.5;
}

.shell {
  max-width: 900px;
  margin: 0 auto;
  padding: 0.75rem 0.9rem 1.5rem;
}

@media (min-width: 768px) {
  .shell {
    padding: 1rem;
  }
}

.card {
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 0.9rem;
  box-shadow: 0 8px 18px rgba(0, 0, 0, 0.05);
  padding: 1rem;
}

.paste-header {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.9rem;
}

.paste-meta {
  color: var(--text-dim);
  font-size: 0.95em;
}

.paste-meta code {
  background: #edf2f8;
  padding: 0.15rem 0.35rem;
  border-radius: 0.3rem;
}

.toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}

button,
.button-link {
  min-height: 2.75rem;
  border-radius: 0.55rem;
  border: 1px solid var(--border);
  background: var(--panel);
  color: var(--text);
  font: inherit;
  padding: 0.45rem 0.8rem;
  cursor: pointer;
  text-decoration: none;
}

button:hover,
.button-link:hover {
  border-color: #b8c8da;
  background: var(--panel-muted);
}

.content {
  overflow-wrap: break-word;
}

.content h1,
.content h2,
.content h3,
.content h4,
.content h5,
.content h6 {
  line-height: 1.25;
  margin-top: 1.1em;
  margin-bottom: 0.5em;
}

.content p,
.content li {
  max-width: 75ch;
}

.content a {
  color: var(--link);
  text-decoration-color: color-mix(in srgb, var(--link), transparent 45%);
  text-underline-offset: 0.12em;
}

.content a:hover {
  color: var(--link-hover);
}

pre {
  background: var(--code-bg);
  color: var(--code-fg);
  padding: 0.75rem;
  border-radius: 0.65rem;
  overflow-x: auto;
  white-space: pre;
  tab-size: 4;
  position: relative;
}

pre code {
  background: transparent;
  color: inherit;
  white-space: inherit;
  font-family: "JetBrains Mono", "SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace;
  tab-size: 4;
}

p code,
li code,
td code,
th code {
  background: #edf2f8;
  color: #1f2c3f;
  border-radius: 0.3rem;
  padding: 0.1em 0.32em;
  font-family: "JetBrains Mono", "SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace;
}

.code-copy {
  position: absolute;
  top: 0.4rem;
  right: 0.4rem;
  min-height: 2.1rem;
  font-size: 0.85em;
  border-color: #4d6078;
  background: rgba(18, 34, 51, 0.85);
  color: #ffffff;
}

.code-copy:hover {
  background: rgba(18, 34, 51, 1);
}

.table-wrap {
  overflow-x: auto;
  margin: 0.85rem 0;
}

table {
  border-collapse: collapse;
  min-width: 100%;
}

th,
td {
  border: 1px solid var(--border);
  padding: 0.5rem;
  text-align: left;
  vertical-align: top;
}

thead th {
  background: var(--panel-muted);
}

blockquote {
  margin: 1em 0;
  padding: 0.2em 0.9em;
  border-left: 4px solid #a9bfd7;
  color: var(--text-dim);
  background: #f8fbff;
}

details {
  border: 1px solid var(--border);
  border-radius: 0.6rem;
  padding: 0.45rem 0.7rem;
  margin: 0.8rem 0;
  background: var(--panel-muted);
}

summary {
  cursor: pointer;
  font-weight: 600;
}

img {
  max-width: 100%;
  height: auto;
  border-radius: 0.45rem;
}

#lightbox {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.86);
  display: none;
  align-items: center;
  justify-content: center;
  padding: 1rem;
  z-index: 9999;
}

#lightbox.open {
  display: flex;
}

#lightbox img {
  max-width: min(96vw, 1400px);
  max-height: 92vh;
  border-radius: 0.5rem;
  background: #ffffff;
}

.helper-text {
  color: var(--text-dim);
  font-size: 0.92em;
}

.tag-list {
  display: flex;
  flex-wrap: wrap;
  gap: 0.45rem;
  margin: 0.45rem 0 1rem;
}

.tag-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.32rem;
  border: 1px solid var(--border);
  border-radius: 999px;
  padding: 0.28rem 0.65rem;
  color: var(--text);
  text-decoration: none;
  background: var(--panel);
}

.tag-chip.active {
  border-color: #87a8cf;
  background: #edf5ff;
}

.dashboard-table {
  width: 100%;
}

.dashboard-table td,
.dashboard-table th {
  white-space: nowrap;
}

.dashboard-table td.links {
  white-space: normal;
}
"#;

const PAGE_SCRIPTS: &str = r#"
<script defer src="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.js"></script>
<script defer src="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/contrib/auto-render.min.js"></script>
<script type="module">
import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs';
mermaid.initialize({ startOnLoad: true, securityLevel: 'strict' });
</script>
<script>
window.addEventListener('DOMContentLoaded', function () {
  if (window.renderMathInElement) {
    window.renderMathInElement(document.body, {
      delimiters: [
        { left: '$$', right: '$$', display: true },
        { left: '$', right: '$', display: false },
        { left: '\\(', right: '\\)', display: false },
        { left: '\\[', right: '\\]', display: true }
      ],
      throwOnError: false
    });
  }

  wrapTables();
  installCodeCopyButtons();
  installToolbar();
  installImageLightbox();
});

function copyText(value) {
  if (!value) {
    return Promise.resolve(false);
  }
  if (navigator.clipboard && navigator.clipboard.writeText) {
    return navigator.clipboard.writeText(value).then(function () { return true; }).catch(function () { return fallbackCopy(value); });
  }
  return Promise.resolve(fallbackCopy(value));
}

function fallbackCopy(value) {
  var ta = document.createElement('textarea');
  ta.value = value;
  ta.style.position = 'fixed';
  ta.style.opacity = '0';
  document.body.appendChild(ta);
  ta.focus();
  ta.select();
  var ok = false;
  try {
    ok = document.execCommand('copy');
  } catch (_err) {
    ok = false;
  }
  ta.remove();
  return ok;
}

function setButtonTextTemporarily(button, message) {
  if (!button) {
    return;
  }
  var original = button.textContent;
  button.textContent = message;
  window.setTimeout(function () {
    button.textContent = original;
  }, 1200);
}

function installToolbar() {
  var rawButton = document.getElementById('copy-raw');
  var renderedButton = document.getElementById('copy-rendered');
  var linkButton = document.getElementById('copy-link');
  var source = document.getElementById('raw-markdown');
  var rendered = document.getElementById('paste-content');
  var canonical = document.body.getAttribute('data-canonical-url');

  if (rawButton) {
    rawButton.addEventListener('click', function () {
      copyText(source ? source.value : '').then(function (ok) {
        setButtonTextTemporarily(rawButton, ok ? 'Copied' : 'Failed');
      });
    });
  }

  if (renderedButton) {
    renderedButton.addEventListener('click', function () {
      copyText(rendered ? rendered.innerText : '').then(function (ok) {
        setButtonTextTemporarily(renderedButton, ok ? 'Copied' : 'Failed');
      });
    });
  }

  if (linkButton) {
    linkButton.addEventListener('click', function () {
      var path = canonical || window.location.pathname;
      copyText(window.location.origin + path).then(function (ok) {
        setButtonTextTemporarily(linkButton, ok ? 'Copied' : 'Failed');
      });
    });
  }
}

function installCodeCopyButtons() {
  var codeBlocks = document.querySelectorAll('pre > code');
  codeBlocks.forEach(function (code) {
    var pre = code.parentElement;
    if (!pre || pre.querySelector(':scope > .code-copy')) {
      return;
    }

    var button = document.createElement('button');
    button.type = 'button';
    button.className = 'code-copy';
    button.textContent = 'Copy';
    button.addEventListener('click', function () {
      copyText(code.textContent || '').then(function (ok) {
        setButtonTextTemporarily(button, ok ? 'Copied' : 'Failed');
      });
    });
    pre.appendChild(button);
  });
}

function wrapTables() {
  var root = document.getElementById('paste-content') || document.body;
  var tables = root.querySelectorAll('table');
  tables.forEach(function (table) {
    if (table.parentElement && table.parentElement.classList.contains('table-wrap')) {
      return;
    }
    var wrapper = document.createElement('div');
    wrapper.className = 'table-wrap';
    table.parentNode.insertBefore(wrapper, table);
    wrapper.appendChild(table);
  });
}

function installImageLightbox() {
  var root = document.getElementById('paste-content');
  if (!root) {
    return;
  }

  var images = root.querySelectorAll('img');
  if (!images.length) {
    return;
  }

  var lightbox = document.createElement('div');
  lightbox.id = 'lightbox';
  lightbox.innerHTML = '<img alt="Expanded image">';
  var lightboxImg = lightbox.querySelector('img');
  lightbox.addEventListener('click', function () {
    lightbox.classList.remove('open');
  });
  document.body.appendChild(lightbox);

  images.forEach(function (img) {
    img.style.cursor = 'zoom-in';
    img.addEventListener('click', function () {
      lightboxImg.src = img.currentSrc || img.src;
      lightboxImg.alt = img.alt || 'Expanded image';
      lightbox.classList.add('open');
    });
  });
}
</script>
"#;

pub fn render_markdown(md: &str) -> String {
    let parser = Parser::new_ext(md, Options::all());
    let mut html_out = String::new();
    html::push_html(&mut html_out, parser);
    let mut builder = ammonia::Builder::default();
    builder
        .add_tag_attributes("pre", &["class"])
        .add_tag_attributes("code", &["class"])
        .add_tag_attributes("details", &["open"])
        .add_tags(["details", "summary"]);
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
        l.starts_with('#')
            || l.starts_with("```")
            || l.starts_with("~~~")
            || l.starts_with("- ")
            || l.starts_with("* ")
            || l.starts_with("> ")
            || l.starts_with('|')
            || l.starts_with("1. ")
    })
}

pub fn render_page(title: &str, body_html: &str, canonical_url: Option<&str>) -> String {
    let canonical_attr = canonical_url
        .map(|v| format!(" data-canonical-url=\"{}\"", html_escape(v)))
        .unwrap_or_default();
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{}</title><link rel=\"stylesheet\" href=\"https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.css\"><style>{}</style></head><body{}><main class=\"shell\">{}</main>{}</body></html>",
        html_escape(title),
        PAGE_CSS,
        canonical_attr,
        body_html,
        PAGE_SCRIPTS,
    )
}

pub fn render_view_shell(id: &str, content_html: &str, raw_markdown: &str) -> String {
    let id_escaped = html_escape(id);
    let raw_escaped = html_escape(raw_markdown);
    format!(
        "<section class=\"card\"><header class=\"paste-header\"><div><h1 style=\"margin:0\">Paste</h1><div class=\"paste-meta\">ID: <code>{id_escaped}</code></div></div><div class=\"toolbar\"><button id=\"copy-raw\" type=\"button\">Copy raw markdown</button><button id=\"copy-rendered\" type=\"button\">Copy rendered text</button><button id=\"copy-link\" type=\"button\">Copy link</button></div></header><article id=\"paste-content\" class=\"content\">{content_html}</article><textarea id=\"raw-markdown\" hidden>{raw_escaped}</textarea></section>",
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

fn url_encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push_str(&format!("{b:02X}"));
        }
    }
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

pub fn render_dashboard(
    recent: &[RecentItem],
    tag_counts: &[(String, usize)],
    selected_tag: Option<&str>,
) -> String {
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
                    <td><a href=\"/p/{id}\">{id}</a></td>\
                    <td>{created}</td>\
                    <td>{tag}</td>\
                    <td>{ctype}</td>\
                    <td>{}</td>\
                    <td class=\"links\"><a href=\"/api/v1/p/{id}\">meta</a> · <a href=\"/p/{id}/md\">md</a> · <a href=\"/p/{id}/{slug}\">legacy</a> · <code>{path}</code></td>\
                </tr>",
                item.size
            ));
        }
    }

    let selected = selected_tag.unwrap_or_default();
    let mut tags = String::new();
    let all_class = if selected.is_empty() {
        "tag-chip active"
    } else {
        "tag-chip"
    };
    tags.push_str(&format!(
        "<a class=\"{}\" href=\"/recent\">all <strong>{}</strong></a>",
        all_class,
        recent.len()
    ));
    for (tag, count) in tag_counts {
        let class = if selected == tag {
            "tag-chip active"
        } else {
            "tag-chip"
        };
        let encoded_tag = url_encode_component(tag);
        tags.push_str(&format!(
            "<a class=\"{}\" href=\"/recent?tag={}\">{} <strong>{}</strong></a>",
            class,
            html_escape(&encoded_tag),
            html_escape(tag),
            count
        ));
    }

    let body = format!(
        "<section class=\"card\"><h1 style=\"margin-top:0\">LAN Paste Dashboard</h1>\
         <p class=\"helper-text\">LAN-only recents feed with quick filters.</p>\
         <p>Quick API entry points:</p>\
         <ul>\
           <li><a href=\"/api\">/api</a> (index)</li>\
           <li><a href=\"/api/v1/recent?n=20\">/api/v1/recent?n=20</a></li>\
           <li><code>POST /api/v1/paste?name=note.md&amp;tag=demo</code></li>\
         </ul>\
         <h2>Recent Pastes</h2>\
         <div class=\"tag-list\">{tags}</div>\
         <div class=\"table-wrap\">\
         <table class=\"dashboard-table\">\
           <thead>\
             <tr><th>ID</th><th>Created</th><th>Tag</th><th>Content-Type</th><th>Bytes</th><th>Links</th></tr>\
           </thead>\
           <tbody>{rows}</tbody>\
         </table>\
         </div></section>"
    );
    render_page("LAN Paste Dashboard", &body, None)
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
    fn markdown_supports_details() {
        let out = render_markdown("<details><summary>more</summary>body</details>");
        assert!(out.contains("<details>"));
        assert!(out.contains("<summary>more</summary>"));
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
        let out = render_page("x", "<p>ok</p>", Some("/p/id"));
        assert!(out.contains("<title>x</title>"));
        assert!(out.contains("<p>ok</p>"));
        assert!(out.contains("katex"));
        assert!(out.contains("mermaid"));
        assert!(out.contains("data-canonical-url=\"/p/id\""));
    }

    #[test]
    fn view_shell_contains_copy_controls() {
        let out = render_view_shell("01TEST", "<h1>x</h1>", "# raw");
        assert!(out.contains("Copy raw markdown"));
        assert!(out.contains("id=\"raw-markdown\""));
    }

    #[test]
    fn escape_works() {
        assert_eq!(html_escape("<>&\"'"), "&lt;&gt;&amp;&quot;&#39;");
    }

    #[test]
    fn dashboard_contains_api_links() {
        let out = render_dashboard(
            &[RecentItem {
                id: "01TEST".to_string(),
                created_at: OffsetDateTime::now_utc(),
                path: "pastes/2026/02/13/01TEST__note.md.md".to_string(),
                commit: "abc123".to_string(),
                tag: Some("demo".to_string()),
                size: 12,
                content_type: "text/markdown".to_string(),
            }],
            &[("demo".to_string(), 1)],
            Some("demo"),
        );
        assert!(out.contains("LAN Paste Dashboard"));
        assert!(out.contains("/api/v1/paste"));
        assert!(out.contains("/p/01TEST/md"));
        assert!(out.contains("/recent?tag=demo"));
    }

    #[test]
    fn dashboard_url_encodes_tag_links() {
        let out = render_dashboard(
            &[],
            &[("a&b c+1".to_string(), 2)],
            Some("a&b c+1"),
        );
        assert!(out.contains("/recent?tag=a%26b%20c%2B1"));
    }

    #[test]
    fn slug_extract_works() {
        let slug = slug_from_rel_path("pastes/2026/02/13/01TEST__note.md.md").expect("slug");
        assert_eq!(slug, "note.md");
    }
}
