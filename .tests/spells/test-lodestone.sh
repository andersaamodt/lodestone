#!/bin/sh
# Test lodestone rendering.

PATH=/usr/bin:/bin:/usr/sbin:/sbin${PATH:+:$PATH}
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
body: "<strong>Trusted</strong>"
---

<lode-page>
  <h1>{title}</h1>
  <div>{@html body}</div>
  <nostr-sync-pill {slug}></nostr-sync-pill>
  <lode-script src={hydrate}></lode-script>
</lode-page>
EOF

"$repo_root/spells/lodestone" render "$source_file" > "$tmpdir/page.html"
"$repo_root/spells/lodestone" render-md "$source_file" > "$tmpdir/page.md"
"$repo_root/spells/lodestone" render "$source_file" --set title=Override > "$tmpdir/override.html"

grep -F 'data-lodestone-root="page"' "$tmpdir/page.html" >/dev/null || {
  printf '%s\n' "missing lodestone root" >&2
  exit 1
}
grep -F '<h1>Test Page</h1>' "$tmpdir/page.html" >/dev/null || {
  printf '%s\n' "missing interpolated title" >&2
  exit 1
}
grep -F '<h1>Override</h1>' "$tmpdir/override.html" >/dev/null || {
  printf '%s\n' "missing CLI override title" >&2
  exit 1
}
grep -F 'data-nostr-sync-slug="test-page"' "$tmpdir/page.html" >/dev/null || {
  printf '%s\n' "missing Nostr sync pill" >&2
  exit 1
}
grep -F '<div><strong>Trusted</strong></div>' "$tmpdir/page.html" >/dev/null || {
  printf '%s\n' "missing trusted HTML interpolation" >&2
  exit 1
}
grep -F 'defer src="/static/test.js"' "$tmpdir/page.html" >/dev/null || {
  printf '%s\n' "missing hydrate script" >&2
  exit 1
}

"$repo_root/spells/lodestone" verify "$source_file" >/dev/null
"$repo_root/spells/lodestone" render-md "$source_file" | grep -F -- '---' >/dev/null || {
  printf '%s\n' "missing markdown frontmatter" >&2
  exit 1
}
"$repo_root/spells/lodestone" manifest "$source_file" | grep -F '"title":"Test Page"' >/dev/null || {
  printf '%s\n' "missing manifest title" >&2
  exit 1
}

space_dir="$tmpdir/path with spaces"
mkdir -p "$space_dir"
space_source="$space_dir/-option-like page.stone.html"
fragment_file="$space_dir/fragment.html"
cat > "$space_source" <<'EOF'
---
title: Space Page
---
<lode-page><h1>{title}</h1><div>{@html body}</div></lode-page>
EOF
printf '%s\n' '<em>Fragment</em>' > "$fragment_file"

"$repo_root/spells/lodestone" render "$space_source" --html-file body="$fragment_file" > "$tmpdir/space-page.html"
grep -F '<h1>Space Page</h1>' "$tmpdir/space-page.html" >/dev/null || {
  printf '%s\n' "failed to render path with spaces" >&2
  exit 1
}
grep -F '<div><em>Fragment</em>' "$tmpdir/space-page.html" >/dev/null || {
  printf '%s\n' "failed to include html-file fragment" >&2
  exit 1
}

if "$repo_root/spells/lodestone" render "$source_file" --set 'bad/key=value' >"$tmpdir/bad-key.out" 2>"$tmpdir/bad-key.err"; then
  printf '%s\n' "invalid override key unexpectedly succeeded" >&2
  exit 1
fi
grep -F 'invalid metadata key' "$tmpdir/bad-key.err" >/dev/null || {
  printf '%s\n' "invalid override key error was not descriptive" >&2
  exit 1
}

if "$repo_root/spells/lodestone" render >"$tmpdir/missing-source.out" 2>"$tmpdir/missing-source.err"; then
  printf '%s\n' "missing source unexpectedly succeeded" >&2
  exit 1
fi
grep -F 'source file is required' "$tmpdir/missing-source.err" >/dev/null || {
  printf '%s\n' "missing source error was not descriptive" >&2
  exit 1
}

printf '%s\n' "ok lodestone"
