# Lanpaste feature-rich viewer test

## Checklist
- [x] headings
- [x] lists
- [x] task lists
- [x] links
- [x] code blocks
- [x] tables
- [x] blockquotes
- [x] inline code
- [x] `<details>` collapsible

## Links
- plain: https://example.com
- markdown: [IANA reserved domains](https://www.iana.org/domains/reserved)

## Inline code
Use `curl --data-binary` for pastes.

## Blockquote
> This is a blockquote.
> Second line.

## Code blocks
### bash
```bash
set -euo pipefail
echo "hello"
```

### json
```json
{"ok": true, "n": 123}
```

## Table
| colA | colB | colC |
| --- | --- | --- |
| 1 | 2 | 3 |
| long long long long long | wraps? | scroll? |

## Collapsible raw section
<details>
<summary>Raw candidates (simulated)</summary>

- Candidate A — https://news.ycombinator.com
- Candidate B — https://github.com

```text
raw extracted blob line 1
raw extracted blob line 2
```

</details>

## Big paragraph (wrap test)
Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
