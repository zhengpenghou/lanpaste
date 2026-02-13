# lanpaste

`lanpaste` is a LAN-friendly, Git-backed paste service.

It stores each paste as a real file in a local Git repo, writes metadata next to it, and exposes a small HTTP API plus a built-in dashboard.

## Features

- Git-backed storage with one commit per paste
- Metadata per paste (`id`, `sha256`, `commit`, `content_type`, `tag`, `size`, `created_at`)
- Dashboard routes:
  - `/`
  - `/dashboard`
- API index route:
  - `/api`
- Markdown rendering for view pages (`/p/{id}`) with sanitization
- Safe raw download route (`/api/v1/p/{id}/raw`) with:
  - `Content-Type: application/octet-stream`
  - `Content-Disposition: attachment`
  - `X-Content-Type-Options: nosniff`
- Optional auth token (`X-Paste-Token`)
- Optional CIDR allowlist (checked against real socket peer IP)
- Optional remote push modes (`off`, `best_effort`, `strict`)
- Readiness and health endpoints (`/readyz`, `/healthz`)
- Single-instance daemon lock to prevent duplicate writers on same data dir

## Requirements

- Rust toolchain (edition 2024)
- `git` installed and available in `PATH`

If startup prints `git is required`, install it:

- Debian/Ubuntu: `sudo apt-get install git`
- Fedora: `sudo dnf install git`
- Arch: `sudo pacman -S git`
- macOS: `xcode-select --install`

## Quick Start

### 1) Build

```bash
cargo build --release
```

### 2) Run

```bash
./target/release/lanpaste serve --dir ./data --bind 0.0.0.0:8090
```

### 3) Open

- Dashboard: `http://127.0.0.1:8090/`
- API index: `http://127.0.0.1:8090/api`
- Health: `http://127.0.0.1:8090/healthz`

## CLI Reference

```text
lanpaste serve --dir <DIR> [options]
```

Options:

- `--dir <DIR>`: Base runtime directory (required)
- `--bind <IP:PORT>`: Listen address (default: `0.0.0.0:8090`)
- `--token <TOKEN>`: Require `X-Paste-Token` on create endpoint
- `--max-bytes <N>`: Max paste payload (default: `1048576`)
- `--push <off|best_effort|strict>`: Git push behavior (default: `off`)
- `--remote <NAME>`: Remote name for pushes (default: `origin`)
- `--allow-cidr <CIDR>`: Restrict create requests by client IP; repeatable
- `--git-author-name <NAME>`: Commit author name (default: `LAN Paste`)
- `--git-author-email <EMAIL>`: Commit author email (default: `paste@lan`)

Example (token + CIDR allowlist):

```bash
./target/release/lanpaste serve \
  --dir ./data \
  --bind 0.0.0.0:8090 \
  --token tok \
  --allow-cidr 192.168.1.0/24 \
  --allow-cidr 10.0.0.0/8
```

## Runtime Directory Layout

`--dir` is the base directory. `lanpaste` manages:

```text
<dir>/
  repo/      # git repo with paste files + metadata json
  run/       # daemon.lock + git.lock
  tmp/       # scratch
```

`repo/` structure:

```text
repo/
  pastes/YYYY/MM/DD/<ULID>__<slug>.<ext>
  meta/<ULID>.json
```

## API Overview

### Dashboard + Index

- `GET /` and `GET /dashboard`: HTML dashboard with recent pastes and links
- `GET /api`: JSON index of API endpoints

### Create paste

- `POST /api/v1/paste?name=<filename>&tag=<tag>&msg=<commit-subject>`
- Body: raw bytes
- Header:
  - `X-Paste-Token: <token>` when `--token` is set
  - `Content-Type` optional (used for metadata; markdown detection)

Example:

```bash
curl -sS \
  -H "X-Paste-Token: tok" \
  -H "Content-Type: text/markdown" \
  --data-binary @tests/fixtures/sample.md \
  "http://127.0.0.1:8090/api/v1/paste?name=note.md&tag=test"
```

Example response (`201`):

```json
{
  "id": "01H...",
  "path": "pastes/2026/02/13/01H...__note.md",
  "commit": "abc123def456",
  "raw_url": "/api/v1/p/01H.../raw",
  "view_url": "/p/01H...",
  "meta_url": "/api/v1/p/01H..."
}
```

### Get metadata

- `GET /api/v1/p/{id}`
- Returns metadata JSON, including commit hash and checksum

### Get raw bytes

- `GET /api/v1/p/{id}/raw`
- Always served as download-safe binary (`application/octet-stream`, `attachment`)

### Get recent pastes

- `GET /api/v1/recent?n=50&tag=<tag>`
- `n` defaults to `50`, capped at `500`
- Optional exact tag filter

### Rendered view

- `GET /p/{id}`
- Markdown pastes are rendered and sanitized; non-markdown shown in escaped `<pre>`

### Health and readiness

- `GET /healthz` -> `200 ok` when process is alive
- `GET /readyz` -> `200 ok` when repo is available

## Error Format

Errors are returned as JSON:

```json
{
  "error": "forbidden",
  "message": "client IP not in allowlist"
}
```

Typical statuses:

- `400` bad request
- `401` unauthorized
- `403` forbidden
- `404` not found
- `409` conflict
- `413` payload too large
- `500` internal
- `503` service unavailable

## Git Behavior

For each paste:

1. write paste file + metadata json
2. `git add` paste + metadata
3. `git commit`
4. optional push depending on `--push`

Push modes:

- `off`: never push
- `best_effort`: try push; still return `201` if push fails
- `strict`: push failure aborts request with `500` and rollbacks staged change

## Security Notes

- Use `--token` for write access control on shared LANs
- Use `--allow-cidr` to restrict writers by client network
- CIDR checks use socket peer IP (not `X-Forwarded-For`)
- Raw route intentionally avoids reflecting untrusted MIME types
- Markdown HTML is sanitized before rendering

## Development

Run tests:

```bash
cargo test
```

Run strict lint:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Optional scripts:

```bash
./scripts/smoke.sh
./scripts/coverage.sh
```

## Troubleshooting

### `Conflict("already running")` on startup

Another `lanpaste` process already holds `<dir>/run/daemon.lock`.

Find and stop it:

```bash
lsof -nP -iTCP:8090 -sTCP:LISTEN
pkill -f 'lanpaste serve --dir'
```

### `GET /` returns 404

If this happens, you are likely running an older binary. Rebuild and restart:

```bash
cargo build --release
./target/release/lanpaste serve --dir ./data
```

### `git is required`

Install `git` and ensure it is in `PATH`.
