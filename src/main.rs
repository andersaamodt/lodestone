use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
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
    let extra_args: Vec<String> = args.collect();
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
    println!("Usage: lodestone <render|render-md|manifest|verify> FILE [--set KEY=VALUE] [--html-file KEY=PATH]");
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
            unknown => {
                return Err(format!("unknown option: {unknown}"));
            }
        }
        index += 1;
    }
    Ok(())
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
        let posts_json = attr_value(raw, "posts")
            .map(|value| html_unescape(&value))
            .unwrap_or_default();
        render_blog_post_list(&posts_json)
    })
}

fn render_blog_post_list(posts_json: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(posts_json) else {
        return "<p class=\"placeholder\">No posts to show yet.</p>".to_string();
    };
    let posts = if let Some(posts) = value.as_array() {
        posts
    } else if let Some(posts) = value.get("posts").and_then(Value::as_array) {
        posts
    } else {
        return "<p class=\"placeholder\">No posts to show yet.</p>".to_string();
    };
    if posts.is_empty() {
        return "<p class=\"placeholder\">No posts to show yet.</p>".to_string();
    }
    posts
        .iter()
        .map(render_blog_post_card)
        .collect::<Vec<_>>()
        .join("")
}

fn render_blog_post_card(post: &Value) -> String {
    let title = text_field(post, "title");
    let title = if title.trim().is_empty() {
        clean_markdown_text(&text_field(post, "summary"))
    } else {
        title
    };
    let title = if title.trim().is_empty() {
        "Untitled".to_string()
    } else {
        title
    };
    let url = first_nonempty_field(post, &["url", "path"]);
    let url = if url.trim().is_empty() {
        "#".to_string()
    } else {
        url
    };
    let post_type = first_nonempty_field(post, &["type"]);
    let post_type = if post_type.trim().is_empty() {
        "post".to_string()
    } else {
        post_type
    };
    let year = first_nonempty_field(post, &["year"]);
    let year = if year.trim().is_empty() {
        published_year(post).unwrap_or_else(|| "Unknown".to_string())
    } else {
        year
    };
    let author = first_nonempty_field(post, &["author"]);
    let author = if author.trim().is_empty() {
        "Blog Author".to_string()
    } else {
        author
    };
    let date = first_nonempty_field(post, &["published_date", "pub_date", "date"]);
    let date = if date.trim().is_empty() {
        "Unknown date".to_string()
    } else {
        date
    };
    let minutes = number_field(post, "reading_minutes").max(1);
    let comments = number_field(post, "comment_count").max(0);
    let comments_label = if comments == 1 {
        "1 comment".to_string()
    } else {
        format!("{comments} comments")
    };
    let summary = clean_markdown_text(&text_field(post, "summary"));
    let summary_html =
        render_post_summary_html(&summary, &url, bool_field(post, "summary_truncated"));
    let tags_html = post_tags(post)
        .iter()
        .map(|tag| {
            format!(
                "<button type=\"button\" class=\"tag blog-inline-tag\" data-inline-tag=\"{}\" aria-pressed=\"false\">{}</button>",
                html_escape(tag),
                html_escape(tag)
            )
        })
        .collect::<Vec<_>>()
        .join("");
    let offsite = if post_type == "link-share" {
        let link_url = first_nonempty_field(post, &["link_url"]);
        let link_url = if link_url.trim().is_empty() {
            first_markdown_href(&summary)
        } else {
            link_url
        };
        format!(
            "<div class=\"post-offsite-link-note\"><span class=\"post-offsite-link-kind\">Off-site link</span><span>Linked by {}</span>{}</div>",
            html_escape(&author),
            if link_url.trim().is_empty() {
                String::new()
            } else {
                format!(
                    "<a class=\"post-offsite-url\" href=\"{}\" title=\"{}\">{}</a>",
                    html_escape(&link_url),
                    html_escape(&link_url),
                    html_escape(&link_url)
                )
            }
        )
    } else {
        String::new()
    };
    let classes = if post_type == "link-share" {
        "post-item blog-post-item is-link-share"
    } else {
        "post-item blog-post-item"
    };
    format!(
        "<article class=\"{}\" data-lodestone-component=\"blog-post-card\" data-post-url=\"{}\" data-post-type=\"{}\" data-post-year=\"{}\" data-post-tags=\"{}\"><div class=\"post-head\"><div class=\"post-head-main\"><h2 class=\"post-title\"><a href=\"{}\">{}</a></h2>{}<div class=\"post-head-divider\" aria-hidden=\"true\"></div><div class=\"post-byline post-byline-bottom\"><span class=\"post-author\">{}</span><span class=\"post-reading-inline\">{} min read</span><span class=\"post-date\">{}</span></div></div></div>{}<div class=\"post-card-footer\"><div class=\"tags post-card-meta-tags\"><button type=\"button\" class=\"tag blog-type-pill\" data-inline-filter-group=\"types\" data-inline-filter-value=\"{}\" aria-pressed=\"false\" aria-label=\"Filter by {}\">{}</button><button type=\"button\" class=\"tag blog-year-pill\" data-inline-filter-group=\"years\" data-inline-filter-value=\"{}\" aria-pressed=\"false\" aria-label=\"Filter by {}\">{}</button>{}</div><span class=\"post-card-comments-count\">{}</span></div></article>",
        classes,
        html_escape(&url),
        html_escape(&post_type),
        html_escape(&year),
        html_escape(&post_tags(post).join(",")),
        html_escape(&url),
        html_escape(&title),
        offsite,
        html_escape(&author),
        minutes,
        html_escape(&date),
        summary_html,
        html_escape(&post_type),
        html_escape(&format_type(&post_type)),
        html_escape(&format_type(&post_type)),
        html_escape(&year),
        html_escape(&year),
        html_escape(&year),
        tags_html,
        html_escape(&comments_label)
    )
}

fn text_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn first_nonempty_field(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .map(|key| text_field(value, key))
        .find(|item| !item.trim().is_empty())
        .unwrap_or_default()
}

fn number_field(value: &Value, key: &str) -> i64 {
    value
        .get(key)
        .and_then(|item| {
            item.as_i64()
                .or_else(|| item.as_f64().map(|num| num as i64))
        })
        .unwrap_or(0)
}

fn bool_field(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn post_tags(value: &Value) -> Vec<String> {
    match value.get("tags") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToString::to_string)
            .collect(),
        Some(Value::String(raw)) => raw
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToString::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn published_year(value: &Value) -> Option<String> {
    let year = first_nonempty_field(value, &["published_at", "created_at"])
        .chars()
        .take(4)
        .collect::<String>()
        .trim()
        .to_string();
    if year.is_empty() {
        None
    } else {
        Some(year)
    }
}

fn format_type(value: &str) -> String {
    match value {
        "longform" | "post" => "post".to_string(),
        "link-share" => "link".to_string(),
        "capture-media" => "capture".to_string(),
        "upload-media" => "media".to_string(),
        "audio-note" => "audio".to_string(),
        "go-live" => "go live".to_string(),
        other => other.replace(['_', '-'], " "),
    }
}

fn clean_markdown_text(value: &str) -> String {
    value.replace("\\\"", "\"").replace("\\'", "'")
}

fn render_post_summary_html(summary: &str, url: &str, truncated: bool) -> String {
    let text = summary.trim();
    if text.is_empty() {
        return String::new();
    }
    let read_more = if truncated && !url.trim().is_empty() {
        format!(
            "<a class=\"post-summary-read-more\" href=\"{}\">Read more...</a>",
            html_escape(url)
        )
    } else {
        String::new()
    };
    format!(
        "<div class=\"post-summary\"><p>{}</p>{}</div>",
        markdown_inline_fallback(text).replace('\n', "<br>"),
        read_more
    )
}

fn markdown_inline_fallback(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('[') {
        let Some(label_end) = rest[start + 1..].find("](") else {
            break;
        };
        let label_end = start + 1 + label_end;
        let href_start = label_end + 2;
        let Some(href_end_rel) = markdown_href_end(&rest[href_start..]) else {
            break;
        };
        let href_end = href_start + href_end_rel;
        out.push_str(&html_escape(&rest[..start]));
        let label = &rest[start + 1..label_end];
        let href = safe_markdown_href(&rest[href_start..href_end]);
        if href.is_empty() {
            out.push_str(&html_escape(label));
        } else {
            out.push_str(&format!(
                "<a href=\"{}\">{}</a>",
                html_escape(&href),
                html_escape(label)
            ));
        }
        rest = &rest[href_end + 1..];
    }
    out.push_str(&html_escape(rest));
    out
}

fn safe_markdown_href(raw: &str) -> String {
    let href = raw.trim().trim_matches(['<', '>']);
    if href.is_empty() {
        return String::new();
    }
    if href.starts_with("//") {
        return String::new();
    }
    let lower = href.to_ascii_lowercase();
    if lower.starts_with("http:")
        || lower.starts_with("https:")
        || lower.starts_with("mailto:")
        || (href.starts_with('/') && !href.starts_with("//"))
        || href.starts_with('#')
    {
        return href.to_string();
    }
    if href.contains(':') {
        return String::new();
    }
    href.to_string()
}

fn first_markdown_href(value: &str) -> String {
    let Some(open) = value.find("](") else {
        return String::new();
    };
    let rest = &value[open + 2..];
    let Some(close) = markdown_href_end(rest) else {
        return String::new();
    };
    safe_markdown_href(&rest[..close])
}

fn markdown_href_end(raw: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, ch) in raw.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(index),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
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

fn html_unescape(raw: &str) -> String {
    raw.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
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
    fn renders_blog_post_list_from_lodestone_data() {
        let mut page = parse_stone_page(
            "---\ntitle: Blog\n---\n<lode-blog-post-list posts={posts_json}></lode-blog-post-list>\n",
        )
        .expect("valid page");
        page.frontmatter.insert(
            String::from("posts_json"),
            r#"[{"title":"Hello","url":"/posts/hello","type":"longform","year":"2026","tags":["writing"],"summary":"Read [this](/this)","summary_truncated":true,"reading_minutes":2,"author":"Ander","published_date":"June 1, 2026","comment_count":1}]"#.to_string(),
        );
        let rendered = render_page(&page);
        assert!(rendered.contains("data-lodestone-component=\"blog-post-card\""));
        assert!(rendered.contains("<a href=\"/posts/hello\">Hello</a>"));
        assert!(rendered.contains("data-post-tags=\"writing\""));
        assert!(rendered.contains("Read <a href=\"/this\">this</a>"));
        assert!(rendered.contains("1 comment"));
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
    fn strips_unsafe_markdown_links_without_losing_label_text() {
        assert_eq!(
            markdown_inline_fallback("See [bad](javascript:alert(1)) and [also](//example.test)."),
            "See bad and also."
        );
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
}
