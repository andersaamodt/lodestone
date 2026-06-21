# lodestone

lodestone is a tiny wizardry web renderer with a Rust core and a POSIX spell
surface.

HTML is the language. YAML frontmatter is page metadata. Custom elements are the
small boundary where static HTML and hydrated browser behavior meet.

## Shape

```html
---
title: Blog
theme: lapidarist
hydrate: /static/blog-page.js
---

<lode-page>
  <h1>{{ page.title }}</h1>
  <nostr-sync-pill slug="blog"></nostr-sync-pill>
  <main>
    HTML stays HTML.
  </main>
</lode-page>
```

## Commands

```sh
spells/lodestone render source.stone.html > page.html
spells/lodestone manifest source.stone.html
spells/lodestone verify source.stone.html
```

`scripts/lodestone-cargo` keeps Cargo output under
`${XDG_STATE_HOME:-$HOME/.local/state}/lodestone/cargo-target`.

`verify` renders once as prerender and once as hydrate-baseline and compares the
normalized HTML. If they differ, the page has more than one source of truth.

## Language Rules

- Source files use `.stone.html` by convention.
- A leading `---` frontmatter block contains minimal YAML.
- Frontmatter supports `key: value` and simple `[a, b, c]` lists.
- The remaining body is ordinary HTML.
- Text interpolation uses `{{ page.key }}`.
- Attribute interpolation uses the same syntax inside attribute values.
- Unknown custom elements pass through unchanged.
- Built-in custom elements expand to ordinary HTML with stable hydrate metadata.

## Built-Ins

- `<lode-page>...</lode-page>` wraps the page body in a stable lodestone root.
- `<nostr-sync-pill slug="..."></nostr-sync-pill>` emits the standard sync pill.
- `<lode-script src="..."></lode-script>` emits a deferred script tag.

## Storage

lodestone writes no durable state by default. Tests and callers should put
transient output under `${TMPDIR:-/tmp}` or an XDG state path.
