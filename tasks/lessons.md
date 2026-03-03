# Lessons Learned

## 2026-03-03
- Rule: Treat AI review comments that point to filesystem path handling as high-priority until disproven; add explicit ID validation at store boundaries.
- Rule: When embedding dynamic values in query strings, URL-encode first, then HTML-escape for attributes.
- Rule: For deduped file writes under possible concurrency, avoid `exists()+write`; prefer `create_new` or temp+rename patterns.
- Rule: Before claiming feature availability, verify the live LAN service binary version/API surface (`/api` endpoint list) matches the current branch.
