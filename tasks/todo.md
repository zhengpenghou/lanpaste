# Lanpaste Improvement Plan Execution (2026-03-03)

## Checklist
- [x] Phase 1: Responsive viewer UX and markdown polish
- [x] Phase 1: Copy/share utilities and prominent paste id in viewer
- [x] Phase 1: Recent listing page with tag filter (created_at desc)
- [x] Phase 2: Multipart image upload endpoint (`POST /api/v1/upload`)
- [x] Phase 2: Static image serving endpoint (`GET /files/{name}`) with cache headers
- [x] Phase 2: Viewer image responsiveness and lightbox/tap-to-zoom
- [x] Phase 3: Canonical ID route + slug alias redirect (`/p/{slug}` -> `/p/{id}`)
- [x] Phase 3: Slug collision handling (`-2`, `-3`, ...)
- [x] Contracts/docs: update OpenAPI + README
- [x] Verification: cargo test
- [x] Verification: cargo clippy --all-targets --all-features -- -D warnings
- [x] Git: split into small commits
- [x] Git: push branch and open PR

## Review
- Added mobile-first viewer polish (copy controls, code-copy buttons, table overflow wrapping, details/summary support, image lightbox).
- Added recents/tag dashboard route (`/recent`) and preserved existing API simplicity.
- Added image upload and serving (`POST /api/v1/upload`, `GET /files/{name}`) with sha256 dedupe and immutable cache headers.
- Added slug maps with collision suffixing and alias redirects (`/p/{slug}` -> `302 /p/{id}`), while keeping canonical ID route.
- Updated tests, OpenAPI contract, and README to reflect the new behavior.
