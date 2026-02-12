# lanpaste

LAN reachable git-backed paste daemon.

## Run

```bash
cargo run -- serve --dir /tmp/lanpaste --bind 0.0.0.0:8090
```

## Create paste

```bash
curl -H "X-Paste-Token: tok" \
     -H "X-Forwarded-For: 127.0.0.1" \
     -H "Content-Type: text/markdown" \
     --data-binary @tests/fixtures/sample.md \
     "http://127.0.0.1:8090/api/v1/paste?name=note.md&tag=test"
```

## Troubleshooting

If startup prints `git is required`, install git:

- Debian/Ubuntu: `sudo apt-get install git`
- Fedora: `sudo dnf install git`
- Arch: `sudo pacman -S git`
- macOS: `xcode-select --install`
