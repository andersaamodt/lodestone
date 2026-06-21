# Lodestone AI Project Instructions

- Follow Wizardry core standards in `~/.wizardry/.github`.
- Keep the user-facing lodestone surface POSIX sh-first.
- Keep the render engine in Rust; shell should orchestrate, not parse the language.
- HTML is the language; do not invent directive-heavy template syntax.
- YAML frontmatter is metadata only.
- Built-in elements must compile to ordinary inspectable HTML.
- Keep prerender and hydrate-baseline rendering on the same code path.
- Tests must write temporary output under `${TMPDIR:-/tmp}`.
- Do not add repo-local runtime state, caches, logs, or generated build output.
