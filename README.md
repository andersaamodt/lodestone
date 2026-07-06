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
  <h1>{title}</h1>
  <site-callout slug="blog" />
  <main>
    HTML stays HTML.
  </main>
  <lode-script src={hydrate} />
</lode-page>
```

## Commands

```sh
spells/lodestone render source.stone.html > page.html
spells/lodestone render-md source.stone.html > page.md
spells/lodestone render source.stone.html --component-command ./site-components > page.html
spells/lodestone render source.stone.html --html-map /tmp/fragments.json > page.html
spells/lodestone render-md source.stone.html --set title=Blog --html-file body=/tmp/body.html
spells/lodestone manifest source.stone.html
spells/lodestone verify source.stone.html
spells/lodestone verify-output source.stone.html page.html --html-map /tmp/fragments.json --component-command ./site-components
```

The Lodestone spell keeps Cargo output under a source-signatured directory at
`${XDG_STATE_HOME:-$HOME/.local/state}/lodestone/cargo-target/`.

`verify` checks that a source can be rendered. `verify-output` renders the source
with the same inputs and compares normalized HTML against an output artifact. Use
it to prove that generated or served pages still come directly from Lodestone
rather than a post-render patcher.

## Language Rules

- Source files use `.stone.html` by convention.
- A leading `---` frontmatter block contains minimal YAML.
- Frontmatter supports `key: value` and simple `[a, b, c]` lists.
- The remaining body is ordinary HTML.
- Text interpolation uses `{title}` or `{page.title}`.
- Attribute interpolation uses `href={href}` or `href={page.href}`.
- Attribute shorthand uses `{slug}`.
- Trusted server fragments use `{@html body}` or `{@html page.body}`.
- Trusted fragments can be loaded one at a time with `--html-file KEY=PATH` or
  as a declared JSON map with `--html-map PATH`.
- `--component-command PATH` lets the caller own site-specific custom elements
  without teaching Lodestone their semantics.
- Legacy `{{ page.key }}` interpolation remains supported for migration.
- Unknown custom elements pass through unchanged.
- Built-in custom elements expand to ordinary HTML with stable hydrate metadata.

An `--html-map` file is a JSON object whose keys are metadata names and whose
values are fragment file paths. Relative paths resolve from the map file's
directory:

```json
{
  "page_content": "fragments/page-content.html",
  "navigation_html": "fragments/navigation.html"
}
```

## Built-Ins

- `<lode-page>...</lode-page>` wraps the page body in a stable lodestone root.
- `<lode-script src="..."></lode-script>` emits a deferred script tag.

Site-specific component ownership stays outside Lodestone. When a rendered page
uses custom elements such as `<site-callout>`, pass `--component-command` and
let that site command render them or return exit status `3` to leave them
unchanged.

## Storage

lodestone writes no durable state by default. Tests and callers should put
transient output under `${TMPDIR:-/tmp}` or an XDG state path.
