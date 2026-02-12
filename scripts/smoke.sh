#!/usr/bin/env bash
set -euo pipefail

BIN=${1:-target/debug/lanpaste}
TMP=$(mktemp -d)
PORT=${LANPASTE_SMOKE_PORT:-18090}

cleanup() {
  if [[ -n "${PID:-}" ]]; then
    kill "$PID" >/dev/null 2>&1 || true
    wait "$PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP"
}
trap cleanup EXIT

"$BIN" serve --dir "$TMP" --bind "127.0.0.1:${PORT}" --token tok >/tmp/lanpaste-smoke.log 2>&1 &
PID=$!

for _ in $(seq 1 40); do
  if curl -fsS "http://127.0.0.1:${PORT}/healthz" >/dev/null; then
    break
  fi
  sleep 0.25
done

RESP=$(curl -fsS \
  -H "X-Paste-Token: tok" \
  -H "X-Forwarded-For: 127.0.0.1" \
  -H "Content-Type: text/markdown" \
  --data-binary @tests/fixtures/sample.md \
  "http://127.0.0.1:${PORT}/api/v1/paste?name=sample.md&tag=smoke")
ID=$(echo "$RESP" | jq -r '.id')

curl -fsS "http://127.0.0.1:${PORT}/api/v1/p/${ID}" >/dev/null
curl -fsS "http://127.0.0.1:${PORT}/api/v1/p/${ID}/raw" >/dev/null
curl -fsS "http://127.0.0.1:${PORT}/p/${ID}" >/dev/null
curl -fsS "http://127.0.0.1:${PORT}/readyz" >/dev/null

if [[ ! -f "$TMP/repo/meta/${ID}.json" ]]; then
  echo "missing meta file"
  exit 1
fi

COUNT=$(git -C "$TMP/repo" rev-list --count HEAD)
if [[ "$COUNT" -lt 2 ]]; then
  echo "expected init + paste commits"
  exit 1
fi

echo "smoke ok"
