use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use serde_json::{json, Value};

#[derive(Clone, Debug)]
struct StonePage {
    frontmatter: BTreeMap<String, String>,
    override_keys: BTreeSet<String>,
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
    let mut extra_args: Vec<String> = args.collect();
    let source = match fs::read_to_string(&source_path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("lodestone: could not read source file: {error}");
            process::exit(1);
        }
    };
    let mut page = match parse_stone_page(&source) {
        Ok(page) => page,
        Err(error) => {
            eprintln!("lodestone: {error}");
            process::exit(1);
        }
    };
    let expected_output_path = if action == "verify-output" {
        if extra_args.is_empty() {
            eprintln!("lodestone: verify-output requires OUTPUT_FILE");
            process::exit(2);
        }
        Some(extra_args.remove(0))
    } else {
        None
    };
    if let Err(error) = apply_overrides(&mut page, &extra_args) {
        eprintln!("lodestone: {error}");
        process::exit(2);
    }
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
            let _ = render_page(&page);
            println!("ok");
        }
        "verify-output" => {
            let expected_output_path = expected_output_path.expect("checked expected output path");
            let expected = match fs::read_to_string(&expected_output_path) {
                Ok(expected) => expected,
                Err(error) => {
                    eprintln!("lodestone: could not read expected output file: {error}");
                    process::exit(1);
                }
            };
            if normalize_html(&render_page(&page)) == normalize_html(&expected) {
                println!("ok");
            } else {
                eprintln!("lodestone: rendered output differs from expected output");
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
    println!("Usage: lodestone <render|render-md|manifest|verify> FILE [--set KEY=VALUE] [--html-file KEY=PATH] [--html-map PATH]");
    println!("       lodestone verify-output FILE OUTPUT_FILE [--set KEY=VALUE] [--html-file KEY=PATH] [--html-map PATH]");
    println!();
    println!("Render .stone.html files with YAML frontmatter and HTML bodies.");
}

fn apply_overrides(page: &mut StonePage, args: &[String]) -> Result<(), String> {
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--set" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--set requires KEY=VALUE".to_string());
                };
                let (key, value) = parse_key_value(raw)?;
                page.override_keys.insert(key.clone());
                page.frontmatter.insert(key, value);
            }
            "--html-file" => {
                index += 1;
                let Some(raw) = args.get(index) else {
                    return Err("--html-file requires KEY=PATH".to_string());
                };
                let (key, path) = parse_key_value(raw)?;
                let value = fs::read_to_string(&path)
                    .map_err(|error| format!("could not read html file for {key}: {error}"))?;
                page.frontmatter.insert(key, value);
            }
            "--html-map" => {
                index += 1;
                let Some(path) = args.get(index) else {
                    return Err("--html-map requires PATH".to_string());
                };
                apply_html_map(page, path)?;
            }
            unknown => {
                return Err(format!("unknown option: {unknown}"));
            }
        }
        index += 1;
    }
    Ok(())
}

fn apply_html_map(page: &mut StonePage, path: &str) -> Result<(), String> {
    let map_source =
        fs::read_to_string(path).map_err(|error| format!("could not read html map: {error}"))?;
    let map: Value =
        serde_json::from_str(&map_source).map_err(|error| format!("invalid html map: {error}"))?;
    let Some(entries) = map.as_object() else {
        return Err("html map must be a JSON object of metadata keys to file paths".to_string());
    };
    let base_dir = Path::new(path).parent().unwrap_or_else(|| Path::new(""));
    for (key, raw_path) in entries {
        if !is_metadata_key(key) {
            return Err(format!("invalid metadata key in html map: {key}"));
        }
        let Some(raw_path) = raw_path.as_str() else {
            return Err(format!(
                "html map value for {key} must be a file path string"
            ));
        };
        let fragment_path = resolve_map_path(base_dir, raw_path);
        let value = fs::read_to_string(&fragment_path).map_err(|error| {
            format!(
                "could not read html map file for {key} ({}): {error}",
                fragment_path.display()
            )
        })?;
        page.frontmatter.insert(key.clone(), value);
    }
    Ok(())
}

fn resolve_map_path(base_dir: &Path, raw_path: &str) -> PathBuf {
    let path = Path::new(raw_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

fn parse_key_value(raw: &str) -> Result<(String, String), String> {
    let Some((key, value)) = raw.split_once('=') else {
        return Err("expected KEY=VALUE".to_string());
    };
    if !is_metadata_key(key) {
        return Err(format!("invalid metadata key: {key}"));
    }
    Ok((key.to_string(), value.to_string()))
}

fn parse_stone_page(source: &str) -> Result<StonePage, String> {
    let mut frontmatter = BTreeMap::new();
    let Some(rest) = source
        .strip_prefix("---\n")
        .or_else(|| source.strip_prefix("---\r\n"))
    else {
        return Ok(StonePage {
            frontmatter,
            override_keys: BTreeSet::new(),
            raw_frontmatter: None,
            body: source.to_string(),
        });
    };
    let Some((raw_frontmatter, after_frontmatter)) = split_frontmatter(rest) else {
        return Err("frontmatter starts but never closes".to_string());
    };
    let body = after_frontmatter
        .strip_prefix("\r\n")
        .or_else(|| after_frontmatter.strip_prefix('\n'))
        .unwrap_or(after_frontmatter);
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
        override_keys: BTreeSet::new(),
        raw_frontmatter: Some(raw_frontmatter.to_string()),
        body: body.to_string(),
    })
}

fn split_frontmatter(rest: &str) -> Option<(&str, &str)> {
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" {
            return Some((&rest[..offset], &rest[offset + line.len()..]));
        }
        offset += line.len();
    }
    if rest[offset..].trim_end_matches('\r') == "---" {
        return Some((&rest[..offset], ""));
    }
    None
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
    html = expand_lode_blog_post_lists(&html);
    html = expand_nostr_sync_pills(&html, page);
    html = expand_lode_scripts(&html);
    html
}

fn render_markdown_page(page: &StonePage) -> String {
    let mut out = String::new();
    if let Some(raw_frontmatter) = &page.raw_frontmatter {
        out.push_str("---\n");
        out.push_str(&render_frontmatter(page, raw_frontmatter));
        out.push_str("\n---\n\n");
    }
    let rendered_body = render_page(page);
    out.push_str(rendered_body.trim_start_matches('\n'));
    out
}

fn render_frontmatter(page: &StonePage, raw_frontmatter: &str) -> String {
    let mut out = String::new();
    for line in raw_frontmatter.lines() {
        let trimmed = line.trim();
        if let Some((key, _value)) = trimmed.split_once(':') {
            let key = key.trim();
            if page.override_keys.contains(key) {
                out.push_str(key);
                out.push_str(": ");
                out.push_str(&yaml_value(
                    &page.frontmatter.get(key).cloned().unwrap_or_default(),
                ));
                out.push('\n');
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim_end_matches('\n').to_string()
}

fn yaml_value(value: &str) -> String {
    if value.starts_with('[') && value.ends_with(']') {
        return value.to_string();
    }
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
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
    let token = format!("={{{expression}}}");
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(index) = rest.find(&token) {
        output.push_str(&rest[..index]);
        let after_token = &rest[index + token.len()..];
        let next = after_token.chars().next();
        if matches!(next, None | Some(' ' | '\n' | '\r' | '\t' | '/' | '>')) {
            output.push_str("=\"");
            output.push_str(escaped_value);
            output.push('"');
        } else {
            output.push_str(&token);
        }
        rest = after_token;
    }
    output.push_str(rest);
    output
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
        if !has_tag_name_boundary(after_start, "lode-page") {
            output.push_str("<lode-page");
            rest = &after_start["<lode-page".len()..];
            continue;
        }
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
            "<span class=\"page-sync-status-pill status-unknown nostr-sync-pill\" data-lodestone-component=\"nostr-sync-pill\" data-nostr-sync-slug=\"{}\" aria-live=\"polite\">Nostr sync</span>",
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

fn expand_lode_blog_post_lists(input: &str) -> String {
    expand_custom_element(input, "lode-blog-post-list", |raw| {
        let posts_raw = attr_value(raw, "posts")
            .map(|value| html_unescape(&value))
            .unwrap_or_else(|| "[]".to_string());
        let posts: Vec<Value> = serde_json::from_str(&posts_raw).unwrap_or_default();
        render_blog_post_list(&posts)
    })
}

fn render_blog_post_list(posts: &[Value]) -> String {
    if posts.is_empty() {
        return "<p class=\"placeholder\" data-lodestone-component=\"lode-blog-post-list\">No posts to show yet.</p>".to_string();
    }
    let mut out = String::new();
    out.push_str("<div data-lodestone-component=\"lode-blog-post-list\">\n");
    for post in posts {
        render_blog_post_card(&mut out, post);
    }
    out.push_str("</div>");
    out
}

fn render_blog_post_card(out: &mut String, post: &Value) {
    let title = post_title(post);
    let url = string_value(post, "url").unwrap_or_default();
    let post_type = string_value_or(post, "type", "post");
    let author = string_value_or(post, "author", "Blog Author");
    let read_minutes = integer_value(post, "reading_minutes").max(1);
    let published_date = string_value(post, "published_date")
        .or_else(|| string_value(post, "pub_date"))
        .unwrap_or_else(|| "Unknown date".to_string());
    let published_timestamp = string_value(post, "published_timestamp")
        .or_else(|| string_value(post, "published_at"))
        .unwrap_or_default();
    let year = string_value_or(post, "year", "Unknown");
    let tags = tags_value(post);
    let comments = integer_value(post, "comment_count").max(0);
    let comments_label = if comments == 1 {
        "1 comment".to_string()
    } else {
        format!("{comments} comments")
    };

    out.push_str("<article class=\"post-item blog-post-item");
    if post_type == "link-share" {
        out.push_str(" is-link-share");
    }
    out.push_str("\">\n");
    out.push_str("<div class=\"post-head\">\n");
    out.push_str("<div class=\"post-head-main\">\n");
    out.push_str("<h2 class=\"post-title\"><a href=\"");
    out.push_str(&html_escape(&url));
    out.push_str("\">");
    out.push_str(&html_escape(&title));
    out.push_str("</a></h2>\n");
    if post_type == "link-share" {
        render_blog_link_note(out, post, &author);
    }
    out.push_str("<div class=\"post-head-divider\" aria-hidden=\"true\"></div>\n");
    out.push_str(
        "<div class=\"post-byline post-byline-bottom\"><span class=\"post-author\">",
    );
    out.push_str(&html_escape(&author));
    out.push_str("</span><span class=\"post-reading-inline\">");
    out.push_str(&html_escape(&read_minutes.to_string()));
    out.push_str(" min read</span><span class=\"post-date\"");
    if !published_timestamp.is_empty() {
        out.push_str(" title=\"");
        out.push_str(&html_escape(&published_timestamp));
        out.push('"');
    }
    out.push('>');
    out.push_str(&html_escape(&published_date));
    out.push_str("</span></div>\n");
    out.push_str("</div>\n");
    out.push_str("</div>\n");
    render_blog_post_summary(out, post, &url);
    out.push_str("<div class=\"post-card-footer\"><div class=\"tags post-card-meta-tags\">");
    render_blog_filter_pill(
        out,
        "tag blog-type-pill",
        "types",
        &post_type,
        &format_post_type(&post_type),
    );
    render_blog_filter_pill(out, "tag blog-year-pill", "years", &year, &year);
    for tag in tags {
        out.push_str("<button type=\"button\" class=\"tag blog-inline-tag\" data-inline-tag=\"");
        out.push_str(&html_escape(&tag));
        out.push_str("\" aria-pressed=\"false\">");
        out.push_str(&html_escape(&tag));
        out.push_str("</button>");
    }
    out.push_str("</div><span class=\"post-card-comments-count\">");
    out.push_str(&html_escape(&comments_label));
    out.push_str("</span></div>\n");
    out.push_str("</article>\n");
}

fn render_blog_link_note(out: &mut String, post: &Value, author: &str) {
    let link_url = string_value(post, "link_url").unwrap_or_default();
    out.push_str("<div class=\"post-offsite-link-note\"><span class=\"post-offsite-link-kind\">Off-site link</span><span>Linked by ");
    out.push_str(&html_escape(author));
    out.push_str("</span>");
    if !link_url.is_empty() {
        out.push_str("<a class=\"post-offsite-url\" href=\"");
        out.push_str(&html_escape(&link_url));
        out.push_str("\" title=\"");
        out.push_str(&html_escape(&link_url));
        out.push_str("\">");
        out.push_str(&html_escape(&link_url));
        out.push_str("</a>");
    }
    out.push_str("</div>\n");
}

fn render_blog_post_summary(out: &mut String, post: &Value, url: &str) {
    let summary = string_value(post, "summary").unwrap_or_default();
    if summary.trim().is_empty() {
        return;
    }
    out.push_str("<div class=\"post-summary\"><p>");
    out.push_str(&html_escape(summary.trim()));
    out.push_str("</p>");
    if bool_value(post, "summary_truncated") && !url.is_empty() {
        out.push_str("<a class=\"post-summary-read-more\" href=\"");
        out.push_str(&html_escape(url));
        out.push_str("\">Read more...</a>");
    }
    out.push_str("</div>\n");
}

fn render_blog_filter_pill(
    out: &mut String,
    class_name: &str,
    group: &str,
    value: &str,
    label: &str,
) {
    out.push_str("<button type=\"button\" class=\"");
    out.push_str(&html_escape(class_name));
    out.push_str("\" data-inline-filter-group=\"");
    out.push_str(&html_escape(group));
    out.push_str("\" data-inline-filter-value=\"");
    out.push_str(&html_escape(value));
    out.push_str("\" aria-pressed=\"false\" aria-label=\"Filter by ");
    out.push_str(&html_escape(label));
    out.push_str("\">");
    out.push_str(&html_escape(label));
    out.push_str("</button>");
}

fn post_title(post: &Value) -> String {
    string_value(post, "title")
        .filter(|value| !value.trim().is_empty())
        .or_else(|| string_value(post, "summary").filter(|value| !value.trim().is_empty()))
        .unwrap_or_else(|| "Untitled".to_string())
}

fn string_value(post: &Value, key: &str) -> Option<String> {
    post.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn string_value_or(post: &Value, key: &str, fallback: &str) -> String {
    string_value(post, key).unwrap_or_else(|| fallback.to_string())
}

fn integer_value(post: &Value, key: &str) -> i64 {
    match post.get(key) {
        Some(Value::Number(number)) => number.as_i64().unwrap_or(0),
        Some(Value::String(text)) => text.parse::<i64>().unwrap_or(0),
        _ => 0,
    }
}

fn bool_value(post: &Value, key: &str) -> bool {
    post.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn tags_value(post: &Value) -> Vec<String> {
    post.get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn format_post_type(value: &str) -> String {
    value
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
        if !has_tag_name_boundary(after_start, tag) {
            output.push_str(&open_prefix);
            rest = &after_start[open_prefix.len()..];
            continue;
        }
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

fn has_tag_name_boundary(raw: &str, tag: &str) -> bool {
    raw.as_bytes()
        .get(tag.len() + 1)
        .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t' | b'/' | b'>'))
}

fn attr_value(raw: &str, name: &str) -> Option<String> {
    let double_needle = format!("{name}=\"");
    let single_needle = format!("{name}='");
    let (start, quote) = if let Some(index) = raw.find(&double_needle) {
        (index + double_needle.len(), '"')
    } else if let Some(index) = raw.find(&single_needle) {
        (index + single_needle.len(), '\'')
    } else {
        return None;
    };
    let rest = &raw[start..];
    let end = rest.find(quote)?;
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

fn html_unescape(raw: &str) -> String {
    raw.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}

fn manifest_json(source_path: &str, page: &StonePage) -> String {
    json!({
        "source": source_path,
        "page": &page.frontmatter,
    })
    .to_string()
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

    #[test]
    fn preserves_quoted_attribute_text_expressions() {
        let page = parse_stone_page(
            "---\nversion: 123\n---\n<script src=\"/app.js?v={version}\"></script>\n",
        )
        .expect("valid page");
        assert!(render_page(&page).contains("src=\"/app.js?v=123\""));
    }

    #[test]
    fn applies_cli_style_overrides() {
        let mut page =
            parse_stone_page("---\ntitle: Old\n---\n<h1>{title}</h1>\n").expect("valid page");
        apply_overrides(
            &mut page,
            &[String::from("--set"), String::from("title=New")],
        )
        .expect("override");
        assert!(render_page(&page).contains("<h1>New</h1>"));
        assert!(render_markdown_page(&page).contains("title: \"New\""));
    }

    #[test]
    fn applies_html_map_fragment_inputs() {
        let temp_dir = unique_test_dir("lodestone-html-map");
        fs::create_dir_all(&temp_dir).expect("temp dir");
        let fragment_path = temp_dir.join("fragment.html");
        let map_path = temp_dir.join("fragments.json");
        fs::write(&fragment_path, "<strong>Mapped fragment</strong>").expect("fragment");
        fs::write(&map_path, r#"{"body_html":"fragment.html"}"#).expect("map");
        let mut page = parse_stone_page("<main>{@html body_html}</main>").expect("valid page");

        apply_overrides(
            &mut page,
            &[
                String::from("--html-map"),
                map_path.to_string_lossy().to_string(),
            ],
        )
        .expect("html map");

        assert_eq!(
            render_page(&page),
            "<main><strong>Mapped fragment</strong></main>"
        );
        fs::remove_dir_all(&temp_dir).expect("cleanup");
    }

    #[test]
    fn rendered_output_verification_compares_normalized_html() {
        let page = parse_stone_page("<main>\n<h1>Same</h1>\n</main>").expect("valid page");
        let expected = "<main> <h1>Same</h1> </main>";

        assert!(rendered_output_matches(&page, expected));
        assert!(!rendered_output_matches(
            &page,
            "<main><h1>Different</h1></main>"
        ));
    }

    #[test]
    fn keeps_render_only_values_out_of_frontmatter() {
        let mut page = parse_stone_page("---\ntitle: Page\n---\n<div>{@html body}</div>\n")
            .expect("valid page");
        page.frontmatter
            .insert(String::from("body"), String::from("<strong>Body</strong>"));
        let rendered = render_markdown_page(&page);
        assert!(rendered.contains("<strong>Body</strong>"));
        assert!(!rendered.contains("body:"));
    }

    #[test]
    fn renders_nostr_sync_pill_with_shared_status_class() {
        let page = parse_stone_page(
            "---\nslug: oeuvre\n---\n<nostr-sync-pill slug={slug}></nostr-sync-pill>\n",
        )
        .expect("valid page");
        let rendered = render_page(&page);
        assert!(rendered.contains("page-sync-status-pill status-unknown nostr-sync-pill"));
        assert!(rendered.contains("data-nostr-sync-slug=\"oeuvre\""));
    }

    #[test]
    fn renders_blog_post_list_built_in() {
        let page = parse_stone_page(
            r#"<lode-blog-post-list posts='[{"url":"/posts/one","title":"One & Two","author":"Anders","published_date":"July 3, 2026","published_timestamp":"2026-07-03T00:00:00Z","summary":"Short summary","summary_truncated":true,"type":"longform","year":"2026","tags":["essay"],"reading_minutes":3,"comment_count":1}]'></lode-blog-post-list>"#,
        )
        .expect("valid page");
        let html = render_page(&page);

        assert!(html.contains("data-lodestone-component=\"lode-blog-post-list\""));
        assert!(html.contains("<article class=\"post-item blog-post-item\">"));
        assert!(html.contains("One &amp; Two"));
        assert!(html.contains("3 min read"));
        assert!(html.contains("Read more..."));
        assert!(html.contains("1 comment"));
        assert!(html.contains("data-inline-filter-value=\"longform\""));
        assert!(html.contains("data-inline-tag=\"essay\""));
    }

    #[test]
    fn renders_empty_blog_post_list_built_in() {
        let page = parse_stone_page("<lode-blog-post-list posts=\"[]\"></lode-blog-post-list>")
            .expect("valid page");

        assert_eq!(
            render_page(&page),
            "<p class=\"placeholder\" data-lodestone-component=\"lode-blog-post-list\">No posts to show yet.</p>"
        );
    }

    #[test]
    fn frontmatter_closes_only_on_delimiter_line() {
        let page = parse_stone_page(
            "---\ntitle: Dash Page\n---not-a-delimiter: true\n---\n<p>Body with --- text</p>\n",
        )
        .expect("valid page");

        assert_eq!(
            page.frontmatter.get("title").map(String::as_str),
            Some("Dash Page")
        );
        assert_eq!(
            page.frontmatter
                .get("---not-a-delimiter")
                .map(String::as_str),
            Some("true")
        );
        assert!(page.body.contains("Body with --- text"));
    }

    #[test]
    fn supports_crlf_frontmatter_delimiters() {
        let page = parse_stone_page("---\r\ntitle: CRLF\r\n---\r\n<h1>{title}</h1>\r\n")
            .expect("valid page");

        assert_eq!(
            page.frontmatter.get("title").map(String::as_str),
            Some("CRLF")
        );
        assert!(render_page(&page).contains("<h1>CRLF</h1>"));
    }

    #[test]
    fn leaves_similar_custom_element_names_untouched() {
        let page = parse_stone_page(
            "<lode-pagelet><nostr-sync-pillow slug=\"x\"></nostr-sync-pillow><lode-scripture src=\"/x.js\"></lode-scripture></lode-pagelet>",
        )
        .expect("valid page");
        let rendered = render_page(&page);

        assert!(rendered.contains("<lode-pagelet>"));
        assert!(rendered.contains("<nostr-sync-pillow slug=\"x\"></nostr-sync-pillow>"));
        assert!(rendered.contains("<lode-scripture src=\"/x.js\"></lode-scripture>"));
        assert!(!rendered.contains("data-lodestone-component=\"nostr-sync-pill\""));
    }

    #[test]
    fn manifest_is_structured_json_with_escaped_values() {
        let page =
            parse_stone_page("---\ntitle: \"A \\\"quoted\\\" page\"\nsummary: line one\n---\n")
                .expect("valid page");
        let manifest: Value =
            serde_json::from_str(&manifest_json("odd path/\"page\".stone.html", &page))
                .expect("valid json");

        assert_eq!(manifest["source"], "odd path/\"page\".stone.html");
        assert_eq!(manifest["page"]["title"], "A \\\"quoted\\\" page");
        assert_eq!(manifest["page"]["summary"], "line one");
    }

    #[test]
    fn rejects_invalid_metadata_keys_in_overrides() {
        let mut page = parse_stone_page("<h1>{title}</h1>").expect("valid page");
        let error = apply_overrides(
            &mut page,
            &[String::from("--set"), String::from("bad/key=value")],
        )
        .expect_err("invalid key rejected");

        assert!(error.contains("invalid metadata key"));
    }

    fn rendered_output_matches(page: &StonePage, expected: &str) -> bool {
        normalize_html(&render_page(page)) == normalize_html(expected)
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        env::temp_dir().join(format!("{prefix}-{}-{nanos}", process::id()))
    }
}
