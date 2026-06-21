#!/bin/sh
# Test lodestone rendering.

set -eu

repo_root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd -P)
tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/lodestone-test.XXXXXX")
trap 'rm -rf "$tmpdir"' EXIT HUP INT TERM

source_file="$tmpdir/page.stone.html"
cat > "$source_file" <<'EOF'
---
title: Test Page
slug: test-page
hydrate: /static/test.js
---

<lode-page>
  <h1>{{ page.title }}</h1>
  <nostr-sync-pill slug="{{ page.slug }}"></nostr-sync-pill>
  <lode-script src="{{ page.hydrate }}"></lode-script>
</lode-page>
EOF

"$repo_root/spells/lodestone" render "$source_file" > "$tmpdir/page.html"

grep -F 'data-lodestone-root="page"' "$tmpdir/page.html" >/dev/null || {
  printf '%s\n' "missing lodestone root" >&2
  exit 1
}
grep -F '<h1>Test Page</h1>' "$tmpdir/page.html" >/dev/null || {
  printf '%s\n' "missing interpolated title" >&2
  exit 1
}
grep -F 'data-nostr-sync-slug="test-page"' "$tmpdir/page.html" >/dev/null || {
  printf '%s\n' "missing Nostr sync pill" >&2
  exit 1
}
grep -F 'defer src="/static/test.js"' "$tmpdir/page.html" >/dev/null || {
  printf '%s\n' "missing hydrate script" >&2
  exit 1
}

"$repo_root/spells/lodestone" verify "$source_file" >/dev/null
"$repo_root/spells/lodestone" manifest "$source_file" | grep -F '"title":"Test Page"' >/dev/null || {
  printf '%s\n' "missing manifest title" >&2
  exit 1
}

printf '%s\n' "ok lodestone"
