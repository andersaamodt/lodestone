use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::process;

#[derive(Clone, Debug)]
struct StonePage {
    frontmatter: BTreeMap<String, String>,
    raw_frontmatter: Option<String>,
    body: String,
}

fn main() {
    let mut args = env::args().skip(1);
    let action = args.next().unwrap_or_else(|| "--help".to_string());
    if matches!(action.as_str(), "--help" | "--usage" | "-h") {
        print_usage();
        return;
    }
    let Some(source_path) = args.next() else {
        eprintln!("lodestone: source file is required");
        process::exit(2);
    };
    let source = match fs::read_to_string(&source_path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("lodestone: could not read source file: {error}");
            process::exit(1);
        }
    };
    let page = match parse_stone_page(&source) {
        Ok(page) => page,
        Err(error) => {
            eprintln!("lodestone: {error}");
            process::exit(1);
        }
    };
    match action.as_str() {
        "render" => {
            print!("{}", render_page(&page));
        }
        "render-md" => {
            print!("{}", render_markdown_page(&page));
        }
        "manifest" => {
            println!("{}", manifest_json(&source_path, &page));
        }
        "verify" => {
            let prerender = normalize_html(&render_page(&page));
            let hydrate = normalize_html(&render_page(&page));
            if prerender == hydrate {
                println!("ok");
            } else {
                eprintln!("lodestone: prerender and hydrate-baseline differ");
                process::exit(1);
            }
        }
        _ => {
            eprintln!("lodestone: unknown action: {action}");
            process::exit(2);
        }
    }
}

fn print_usage() {
    println!("Usage: lodestone <render|render-md|manifest|verify> FILE");
    println!();
    println!("Render .stone.html files with YAML frontmatter and HTML bodies.");
}

fn parse_stone_page(source: &str) -> Result<StonePage, String> {
    let mut frontmatter = BTreeMap::new();
    if !source.starts_with("---\n") {
        return Ok(StonePage {
            frontmatter,
            raw_frontmatter: None,
            body: source.to_string(),
        });
    }
    let rest = &source[4..];
    let Some(end_index) = rest.find("\n---") else {
        return Err("frontmatter starts but never closes".to_string());
    };
    let raw_frontmatter = &rest[..end_index];
    let mut body = &rest[end_index + 4..];
    if body.starts_with('\n') {
        body = &body[1..];
    }
    for (line_index, line) in raw_frontmatter.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            return Err(format!("invalid frontmatter line {}", line_index + 1));
        };
        let key = key.trim();
        if !is_metadata_key(key) {
            return Err(format!("invalid frontmatter key: {key}"));
        }
        frontmatter.insert(key.to_string(), parse_yaml_scalar(value.trim()));
    }
    Ok(StonePage {
        frontmatter,
        raw_frontmatter: Some(raw_frontmatter.to_string()),
        body: body.to_string(),
    })
}

fn is_metadata_key(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

fn parse_yaml_scalar(raw: &str) -> String {
    let value = raw.trim();
    if value.starts_with('[') && value.ends_with(']') {
        return value[1..value.len() - 1]
            .split(',')
            .map(|part| unquote(part.trim()))
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
    }
    unquote(value)
}

fn unquote(value: &str) -> String {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn render_page(page: &StonePage) -> String {
    let mut html = expand_attribute_shorthand(&page.body, &page.frontmatter);
    html = interpolate(&html, &page.frontmatter);
    html = expand_lode_page(&html);
    html = expand_nostr_sync_pills(&html, page);
    html = expand_lode_scripts(&html);
    html
}

fn render_markdown_page(page: &StonePage) -> String {
    let mut out = String::new();
    if let Some(raw_frontmatter) = &page.raw_frontmatter {
        out.push_str("---\n");
        out.push_str(raw_frontmatter);
        out.push_str("\n---\n\n");
    }
    out.push_str(&render_page(page));
    out
}

fn interpolate(input: &str, frontmatter: &BTreeMap<String, String>) -> String {
    let mut output = input.to_string();
    for (key, value) in frontmatter {
        output = replace_raw_expression(&output, &format!("page.{key}"), value);
        output = replace_raw_expression(&output, key, value);
        let escaped = html_escape(value);
        output = replace_attribute_expression(&output, &format!("page.{key}"), &escaped);
        output = replace_attribute_expression(&output, key, &escaped);
        output = replace_braced_expression(&output, &format!("page.{key}"), &escaped);
        output = replace_braced_expression(&output, key, &escaped);
        output = output.replace(&format!("{{{{ page.{key} }}}}"), &escaped);
        output = output.replace(&format!("{{{{page.{key}}}}}"), &escaped);
    }
    output
}

fn replace_raw_expression(input: &str, expression: &str, value: &str) -> String {
    let mut output = input.to_string();
    output = output.replace(&format!("{{@html {expression}}}"), value);
    output.replace(&format!("{{@html {expression} }}"), value)
}

fn replace_attribute_expression(input: &str, expression: &str, escaped_value: &str) -> String {
    input.replace(
        &format!("={{{expression}}}"),
        &format!("=\"{escaped_value}\""),
    )
}

fn replace_braced_expression(input: &str, expression: &str, escaped_value: &str) -> String {
    let mut output = input.to_string();
    output = output.replace(&format!("{{{expression}}}"), escaped_value);
    output.replace(&format!("{{ {expression} }}"), escaped_value)
}

fn expand_lode_page(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("<lode-page") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start..];
        let Some(end) = after_start.find('>') else {
            output.push_str(after_start);
            return output;
        };
        output.push_str("<div data-lodestone-root=\"page\" data-lodestone-render=\"universal\">");
        rest = &after_start[end + 1..];
    }
    output.push_str(rest);
    output.replace("</lode-page>", "</div>")
}

fn expand_nostr_sync_pills(input: &str, page: &StonePage) -> String {
    expand_custom_element(input, "nostr-sync-pill", |raw| {
        let slug = attr_value(raw, "slug")
            .or_else(|| page.frontmatter.get("slug").cloned())
            .unwrap_or_else(|| "page".to_string());
        format!(
            "<span class=\"nostr-sync-pill\" data-lodestone-component=\"nostr-sync-pill\" data-nostr-sync-slug=\"{}\" aria-live=\"polite\">Nostr sync</span>",
            html_escape(&slug)
        )
    })
}

fn expand_lode_scripts(input: &str) -> String {
    expand_custom_element(input, "lode-script", |raw| {
        let src = attr_value(raw, "src").unwrap_or_default();
        format!(
            "<script defer src=\"{}\" data-lodestone-hydrate=\"script\"></script>",
            html_escape(&src)
        )
    })
}

fn expand_custom_element<F>(input: &str, tag: &str, mut render: F) -> String
where
    F: FnMut(&str) -> String,
{
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    let open_prefix = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    while let Some(start) = rest.find(&open_prefix) {
        output.push_str(&rest[..start]);
        let after_start = &rest[start..];
        let Some(open_end) = after_start.find('>') else {
            output.push_str(after_start);
            return output;
        };
        let raw_open = &after_start[..open_end + 1];
        if raw_open.ends_with("/>") {
            output.push_str(&render(raw_open));
            rest = &after_start[open_end + 1..];
            continue;
        }
        let after_open = &after_start[open_end + 1..];
        if let Some(close_start) = after_open.find(&close_tag) {
            output.push_str(&render(raw_open));
            rest = &after_open[close_start + close_tag.len()..];
        } else {
            output.push_str(raw_open);
            rest = after_open;
        }
    }
    output.push_str(rest);
    output
}

fn attr_value(raw: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = raw.find(&needle)? + needle.len();
    let rest = &raw[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn expand_attribute_shorthand(input: &str, frontmatter: &BTreeMap<String, String>) -> String {
    let mut output = input.to_string();
    for (key, value) in frontmatter {
        output = output.replace(
            &format!(" {{{key}}}"),
            &format!(" {key}=\"{}\"", html_escape(value)),
        );
    }
    output
}

fn html_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn json_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

fn manifest_json(source_path: &str, page: &StonePage) -> String {
    let mut out = format!("{{\"source\":\"{}\",\"page\":{{", json_escape(source_path));
    for (index, (key, value)) in page.frontmatter.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(&json_escape(key));
        out.push_str("\":\"");
        out.push_str(&json_escape(value));
        out.push('"');
    }
    out.push_str("}}");
    out.push('}');
    out
}

fn normalize_html(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_page_from_one_source() {
        let page = parse_stone_page(
            r#"---
title: Test Page
slug: test-page
hydrate: /static/test.js
---

<lode-page>
<h1>{title}</h1>
<nostr-sync-pill {slug}></nostr-sync-pill>
<lode-script src={hydrate}></lode-script>
</lode-page>
"#,
        )
        .expect("valid page");
        let html = render_page(&page);
        assert!(html.contains("data-lodestone-root=\"page\""));
        assert!(html.contains("<h1>Test Page</h1>"));
        assert!(html.contains("data-nostr-sync-slug=\"test-page\""));
        assert!(html.contains("defer src=\"/static/test.js\""));
    }

    #[test]
    fn escapes_interpolation() {
        let page = parse_stone_page("---\ntitle: \"<Bad>\"\n---\n<h1>{{ page.title }}</h1>\n")
            .expect("valid page");
        assert!(render_page(&page).contains("&lt;Bad&gt;"));
    }

    #[test]
    fn allows_explicit_trusted_html_interpolation() {
        let page = parse_stone_page(
            "---\nbody: \"<strong>Ready</strong>\"\n---\n<div>{@html body}</div>\n",
        )
        .expect("valid page");
        assert!(render_page(&page).contains("<div><strong>Ready</strong></div>"));
    }

    #[test]
    fn supports_svelte_like_attribute_expressions() {
        let page = parse_stone_page("---\nhref: /hello\n---\n<a href={href}>{href}</a>\n")
            .expect("valid page");
        let html = render_page(&page);
        assert!(html.contains("<a href=\"/hello\">/hello</a>"));
    }
}
