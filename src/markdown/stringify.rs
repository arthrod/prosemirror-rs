//! Markdown serialization compatible with `mdast-util-to-markdown`.
//!
//! Options match `MarkdownExtension`: `fences: true`, `listItemIndent: "one"`,
//! `rule: "-"`, bullet `*`, emphasis `*`, strong `**`. `joinListItemChildren`
//! forces a blank line where a tight join would change the syntax (a paragraph
//! after a list or blockquote, two blockquotes, a thematic break after a
//! paragraph).

use super::md::{Block, Doc, Inline, Item};

pub(crate) fn stringify_doc(doc: &Doc, gfm: bool) -> String {
    let mut result = container_flow(&doc.blocks, FlowParent::Plain, gfm);
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

#[derive(Clone, Copy)]
enum FlowParent {
    /// Document or blockquote: blocks are separated by a blank line.
    Plain,
    /// List item. `spread` is the item's own spread flag.
    Item { spread: bool },
}

fn container_flow(blocks: &[Block], parent: FlowParent, gfm: bool) -> String {
    let mut out = String::new();
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            out.push_str(join_between(&blocks[index - 1], block, parent));
        }
        out.push_str(&render_block(block, gfm));
    }
    out
}

fn join_between(left: &Block, right: &Block, parent: FlowParent) -> &'static str {
    if let FlowParent::Item { .. } = parent {
        let special = matches!(
            (left, right),
            (
                Block::List { .. } | Block::Blockquote(_),
                Block::Paragraph(_)
            ) | (Block::Blockquote(_), Block::Blockquote(_))
                | (Block::Paragraph(_), Block::Rule)
        );
        if special {
            // `joinListItemChildren` returns 1, which skips the other joiners
            // and falls through to a blank line.
            return "\n\n";
        }
    }
    if let FlowParent::Item { spread } = parent {
        // Two paragraphs are always separated by a blank line. Everything else
        // uses the parent's spread flag: loose → blank line, tight → one newline.
        if matches!(left, Block::Paragraph(_)) && matches!(right, Block::Paragraph(_)) {
            return "\n\n";
        }
        return if spread { "\n\n" } else { "\n" };
    }
    "\n\n"
}

fn render_block(block: &Block, gfm: bool) -> String {
    match block {
        Block::Paragraph(inlines) => protect_line_starts(&render_phrasing(inlines, gfm)),
        Block::Heading { level, children } => {
            let sequence = "#".repeat((*level).clamp(1, 6) as usize);
            let value = render_phrasing(children, gfm);
            if value.is_empty() {
                sequence
            } else {
                format!("{sequence} {value}")
            }
        }
        Block::Blockquote(blocks) => {
            let flow = container_flow(blocks, FlowParent::Plain, gfm);
            indent_lines(&flow, |line, _index, blank| {
                if blank {
                    ">".to_string()
                } else {
                    format!("> {line}")
                }
            })
        }
        Block::Code { value } => {
            let fence = "`".repeat(longest_run(value, '`').max(2) + 1);
            if value.is_empty() {
                format!("{fence}\n{fence}")
            } else {
                format!("{fence}\n{value}\n{fence}")
            }
        }
        Block::Rule => "---".to_string(),
        Block::List {
            ordered,
            start,
            spread,
            items,
        } => render_list(*ordered, *start, *spread, items, gfm),
    }
}

fn render_list(ordered: bool, start: u64, spread: bool, items: &[Item], gfm: bool) -> String {
    let mut out = String::new();
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push_str(if spread { "\n\n" } else { "\n" });
        }
        out.push_str(&render_item(ordered, start, index, item, gfm));
    }
    out
}

fn render_item(ordered: bool, start: u64, index: usize, item: &Item, gfm: bool) -> String {
    let marker = if ordered {
        format!("{}.", start + index as u64)
    } else {
        "*".to_string()
    };
    let size = marker.len() + 1;
    let flow = container_flow(
        &item.children,
        FlowParent::Item {
            spread: item.spread,
        },
        gfm,
    );
    let value = indent_lines(&flow, |line, line_index, blank| {
        if line_index > 0 {
            if blank {
                String::new()
            } else {
                format!("{}{line}", " ".repeat(size))
            }
        } else if blank {
            marker.clone()
        } else {
            format!("{marker}{}{line}", " ".repeat(size - marker.len()))
        }
    });
    match item.checked {
        Some(checked) => inject_checkbox(&value, checked),
        None => value,
    }
}

fn inject_checkbox(value: &str, checked: bool) -> String {
    let checkbox = if checked { "[x] " } else { "[ ] " };
    let bytes = value.as_bytes();
    let mut index = 0;
    if matches!(bytes.first(), Some(b'*' | b'+' | b'-')) {
        index = 1;
    } else {
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index < bytes.len() && (bytes[index] == b'.' || bytes[index] == b')') {
            index += 1;
        } else {
            return value.to_string();
        }
    }
    if index < bytes.len() && (bytes[index] == b'\n' || bytes[index] == b'\r') {
        let eol = if bytes[index] == b'\r' && index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
            2
        } else {
            1
        };
        let mut out = String::new();
        out.push_str(&value[..index + eol]);
        out.push_str(checkbox);
        out.push_str(&value[index + eol..]);
        return out;
    }
    let mut spaces = 0;
    while index + spaces < bytes.len() && bytes[index + spaces] == b' ' && spaces < 3 {
        spaces += 1;
    }
    if spaces == 0 {
        // A blank first line is just the marker (`*`). Still emit the box.
        let mut out = String::new();
        out.push_str(&value[..index]);
        out.push(' ');
        out.push_str(checkbox);
        out.push_str(&value[index..]);
        return out;
    }
    let mut out = String::new();
    out.push_str(&value[..index + spaces]);
    out.push_str(checkbox);
    out.push_str(&value[index + spaces..]);
    out
}

fn render_phrasing(inlines: &[Inline], gfm: bool) -> String {
    let mut out = String::new();
    for inline in inlines {
        out.push_str(&render_inline(inline, gfm));
    }
    out
}

fn render_inline(inline: &Inline, gfm: bool) -> String {
    match inline {
        Inline::Text(text) => escape_text(text, gfm),
        Inline::Code(value) => render_inline_code(value),
        Inline::Emphasis(children) => format!("*{}*", render_phrasing(children, gfm)),
        Inline::Strong(children) => format!("**{}**", render_phrasing(children, gfm)),
        Inline::Delete(children) => format!("~~{}~~", render_phrasing(children, gfm)),
        Inline::Break => "\\\n".to_string(),
        Inline::Image { alt, url, title } => render_resource("!", alt, url, title.as_deref()),
        Inline::Link {
            url,
            title,
            children,
        } => {
            if let Some(autolink) = autolink_text(url, title, children) {
                return format!("<{autolink}>");
            }
            let inner = render_phrasing(children, gfm);
            render_resource("", &inner, url.as_deref().unwrap_or(""), title.as_deref())
        }
    }
}

fn autolink_text<'a>(
    url: &'a Option<String>,
    title: &Option<String>,
    children: &'a [Inline],
) -> Option<&'a str> {
    if title.is_some() {
        return None;
    }
    let url = url.as_deref()?;
    if children.len() != 1 {
        return None;
    }
    let Inline::Text(text) = &children[0] else {
        return None;
    };
    if text != url && format!("mailto:{text}") != url {
        return None;
    }
    if !has_protocol(url)
        || url
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, ' ' | '<' | '>'))
    {
        return None;
    }
    Some(text.as_str())
}

fn has_protocol(url: &str) -> bool {
    let mut chars = url.chars();
    if !chars.next().is_some_and(|ch| ch.is_ascii_alphabetic()) {
        return false;
    }
    for ch in chars {
        if ch == ':' {
            return true;
        }
        if !(ch.is_ascii_alphabetic() || matches!(ch, '+' | '.' | '-')) {
            return false;
        }
    }
    false
}

fn render_resource(prefix: &str, label: &str, url: &str, title: Option<&str>) -> String {
    let label = if prefix == "!" {
        label.replace('\\', "\\\\").replace(']', "\\]")
    } else {
        // Link labels are already rendered phrasing, which escapes on its own.
        label.to_string()
    };
    let mut out = format!("{prefix}[{label}]({}", destination(url));
    if let Some(title) = title {
        out.push_str(" \"");
        out.push_str(&title.replace('\\', "\\\\").replace('"', "\\\""));
        out.push('"');
    }
    out.push(')');
    out
}

/// Angle-bracket a destination that a raw `()` would end early.
fn destination(url: &str) -> String {
    let awkward = url
        .chars()
        .any(|ch| matches!(ch, ' ' | '\t' | '(' | ')' | '<' | '\n' | '\r'));
    if awkward && !url.contains(['>', '\n', '\r']) {
        format!("<{url}>")
    } else {
        url.replace('(', "\\(").replace(')', "\\)")
    }
}

fn escape_text(text: &str, gfm: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    for (index, ch) in chars.iter().copied().enumerate() {
        // `<`, `>`, and `&` reparse as HTML or entities. A `~` is strikethrough
        // in GFM, except a single tilde sitting inside a word (`a~b`).
        let escape_tilde = gfm && ch == '~' && !tilde_inside_word(&chars, index);
        if matches!(ch, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '&') || escape_tilde {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn tilde_inside_word(chars: &[char], index: usize) -> bool {
    let prev_word = index > 0 && chars[index - 1].is_alphanumeric();
    let next_word = chars.get(index + 1).is_some_and(|ch| ch.is_alphanumeric());
    prev_word && next_word
}

fn render_inline_code(value: &str) -> String {
    // A raw newline ends the code span. CommonMark turns it into a space.
    let value = value.replace("\r\n", " ").replace(['\n', '\r'], " ");
    let value = value.as_str();
    let mut sequence = "`".to_string();
    while fence_occurs(value, &sequence) {
        sequence.push('`');
    }
    let pad = value.chars().any(|ch| !matches!(ch, ' ' | '\r' | '\n'))
        && ((value.starts_with([' ', '\r', '\n']) && value.ends_with([' ', '\r', '\n']))
            || value.starts_with('`')
            || value.ends_with('`'));
    if pad {
        format!("{sequence} {value} {sequence}")
    } else {
        format!("{sequence}{value}{sequence}")
    }
}

fn fence_occurs(value: &str, sequence: &str) -> bool {
    let bytes = value.as_bytes();
    let seq = sequence.as_bytes();
    if seq.is_empty() || bytes.len() < seq.len() {
        return false;
    }
    let mut index = 0;
    while index + seq.len() <= bytes.len() {
        if &bytes[index..index + seq.len()] == seq {
            let before_ok = index == 0 || bytes[index - 1] != b'`';
            let after = index + seq.len();
            let after_ok = after == bytes.len() || bytes[after] != b'`';
            if before_ok && after_ok {
                return true;
            }
        }
        index += 1;
    }
    false
}

fn longest_run(value: &str, marker: char) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for ch in value.chars() {
        if ch == marker {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn protect_line_starts(value: &str) -> String {
    let mut out = String::new();
    let bytes = value.as_bytes();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\n' || bytes[index] == b'\r' {
            let eol_len =
                if bytes[index] == b'\r' && index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    2
                } else {
                    1
                };
            out.push_str(&protect_line(&value[start..index]));
            out.push_str(&value[index..index + eol_len]);
            index += eol_len;
            start = index;
            continue;
        }
        index += 1;
    }
    out.push_str(&protect_line(&value[start..]));
    out
}

/// Escape a line that Markdown would read as a list, rule, or indented code.
fn protect_line(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut indent = 0;
    while indent < bytes.len() && (bytes[indent] == b' ' || bytes[indent] == b'\t') {
        indent += 1;
    }
    if indent >= 4 && indent < bytes.len() {
        return format!("\\{line}");
    }
    let rest = &line[indent..];
    if rest.starts_with(['-', '+']) || atx_heading(rest) {
        let mut out = line[..indent].to_string();
        out.push('\\');
        out.push_str(rest);
        return out;
    }
    if let Some(marker) = ordered_marker_dot(rest) {
        let mut out = line[..indent].to_string();
        out.push_str(&rest[..marker]);
        out.push('\\');
        out.push_str(&rest[marker..]);
        return out;
    }
    line.to_string()
}

fn atx_heading(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    let mut hashes = 0;
    while hashes < bytes.len() && bytes[hashes] == b'#' {
        hashes += 1;
    }
    if hashes == 0 || hashes > 6 {
        return false;
    }
    hashes == bytes.len() || bytes[hashes] == b' ' || bytes[hashes] == b'\t'
}

fn ordered_marker_dot(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut index = 0;
    while index < bytes.len() && index < 9 && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == 0 || index >= bytes.len() {
        return None;
    }
    if bytes[index] != b'.' && bytes[index] != b')' {
        return None;
    }
    let after = index + 1;
    if after == bytes.len() || bytes[after] == b' ' || bytes[after] == b'\t' {
        Some(index)
    } else {
        None
    }
}

fn indent_lines(value: &str, mut map: impl FnMut(&str, usize, bool) -> String) -> String {
    let mut out = String::new();
    let bytes = value.as_bytes();
    let mut start = 0;
    let mut line = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\n' || bytes[index] == b'\r' {
            let eol_len =
                if bytes[index] == b'\r' && index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    2
                } else {
                    1
                };
            out.push_str(&map(&value[start..index], line, start == index));
            out.push_str(&value[index..index + eol_len]);
            index += eol_len;
            start = index;
            line += 1;
            continue;
        }
        index += 1;
    }
    out.push_str(&map(&value[start..], line, start == value.len()));
    out
}
