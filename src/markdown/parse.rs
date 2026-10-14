//! Markdown source → mdast-like tree.
//!
//! Parsing uses [`pulldown_cmark`](https://docs.rs/pulldown-cmark), the same
//! CommonMark engine as `markdown-remarkable`. GFM strikethrough (including a
//! single tilde), task lists, and extended autolinks are layered on top so the
//! tree matches `prosemirror-remark`'s `GFMExtension`. Raw HTML is dropped, as
//! in the upstream library. Reference definitions are resolved and then
//! discarded; a definition that only exists to loosen a list still marks that
//! item `spread`.

use std::ops::Range;

use pulldown_cmark::{Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};

use super::md::{push_text, Block, Doc, Inline, Item};
use super::schema::Flavor;

pub fn parse_markdown(source: &str, flavor: Flavor) -> Doc {
    let mut options = Options::empty();
    if flavor == Flavor::Gfm {
        options.insert(Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS);
    }
    let parser = Parser::new_ext(source, options);
    let events: Vec<(Event<'_>, Range<usize>)> = parser.into_offset_iter().collect();
    let mut cursor = Cursor {
        source,
        events: &events,
        index: 0,
        gfm: flavor == Flavor::Gfm,
        task_checked: None,
    };
    let mut blocks = cursor.parse_blocks(|end| matches!(end, TagEnd::Item));
    // parse_blocks stops at End(Item) which doesn't occur at the root, so it
    // consumes the whole event stream.
    if blocks.is_empty() {
        blocks.push(Block::Paragraph(Vec::new()));
    }
    Doc { blocks }
}

struct Cursor<'a> {
    source: &'a str,
    events: &'a [(Event<'a>, Range<usize>)],
    index: usize,
    gfm: bool,
    /// Set when a GFM task marker is seen, including inside a loose paragraph.
    task_checked: Option<bool>,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<&'a (Event<'a>, Range<usize>)> {
        self.events.get(self.index)
    }

    fn consume_task_marker(&mut self) -> bool {
        if let Some((Event::TaskListMarker(value), _)) = self.peek() {
            self.task_checked = Some(*value);
            self.next();
            true
        } else {
            false
        }
    }

    fn next(&mut self) -> Option<&'a (Event<'a>, Range<usize>)> {
        let event = self.events.get(self.index)?;
        self.index += 1;
        Some(event)
    }

    fn parse_blocks(&mut self, stop: impl Fn(&TagEnd) -> bool) -> Vec<Block> {
        let mut blocks = Vec::new();
        let mut inlines = Vec::new();
        while let Some((event, _)) = self.peek() {
            match event {
                Event::End(end) if stop(end) => break,
                Event::End(_) => break,
                Event::Start(Tag::Paragraph) => {
                    self.finish_block_inlines(&mut blocks, &mut inlines);
                    blocks.push(self.parse_paragraph());
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    self.finish_block_inlines(&mut blocks, &mut inlines);
                    blocks.push(self.parse_heading(*level));
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    self.finish_block_inlines(&mut blocks, &mut inlines);
                    blocks.push(self.parse_blockquote());
                }
                Event::Start(Tag::CodeBlock(_)) => {
                    self.finish_block_inlines(&mut blocks, &mut inlines);
                    blocks.push(self.parse_code_block());
                }
                Event::Start(Tag::List(start)) => {
                    self.finish_block_inlines(&mut blocks, &mut inlines);
                    blocks.push(self.parse_list(*start));
                }
                Event::Start(Tag::HtmlBlock) => {
                    self.finish_block_inlines(&mut blocks, &mut inlines);
                    self.skip_container(TagEnd::HtmlBlock);
                }
                Event::Start(Tag::Item) | Event::TaskListMarker(_) => break,
                Event::Rule => {
                    self.finish_block_inlines(&mut blocks, &mut inlines);
                    self.next();
                    blocks.push(Block::Rule);
                }
                Event::Html(_) | Event::InlineHtml(_) => {
                    self.next();
                }
                _ => {
                    if let Some(inline) = self.parse_inline() {
                        inlines.push(inline);
                    } else {
                        // Unknown container (footnote, table, …). Skip it so
                        // the cursor still advances.
                        self.next();
                    }
                }
            }
        }
        self.finish_block_inlines(&mut blocks, &mut inlines);
        blocks
    }

    fn finish_block_inlines(&self, blocks: &mut Vec<Block>, inlines: &mut Vec<Inline>) {
        if inlines.is_empty() {
            return;
        }
        let taken = std::mem::take(inlines);
        blocks.push(Block::Paragraph(finish_inlines(taken, self.gfm)));
    }

    fn parse_paragraph(&mut self) -> Block {
        let start = self.next().map(|(_, range)| range.start);
        let inlines = self.parse_inlines(|end| matches!(end, TagEnd::Paragraph));
        let _ = start;
        Block::Paragraph(finish_inlines(inlines, self.gfm))
    }

    fn parse_heading(&mut self, level: HeadingLevel) -> Block {
        self.next();
        let children = finish_inlines(
            self.parse_inlines(|end| matches!(end, TagEnd::Heading(_))),
            self.gfm,
        );
        Block::Heading {
            level: heading_level(level),
            children,
        }
    }

    fn parse_blockquote(&mut self) -> Block {
        self.next();
        let mut blocks = self.parse_blocks(|end| matches!(end, TagEnd::BlockQuote(_)));
        if let Some((Event::End(TagEnd::BlockQuote(_)), _)) = self.peek() {
            self.next();
        }
        if blocks.is_empty() {
            blocks.push(Block::Paragraph(Vec::new()));
        }
        Block::Blockquote(blocks)
    }

    fn parse_code_block(&mut self) -> Block {
        self.next();
        let mut value = String::new();
        while let Some((event, _)) = self.peek() {
            match event {
                Event::End(TagEnd::CodeBlock) => {
                    self.next();
                    break;
                }
                Event::Text(text) => {
                    value.push_str(text);
                    self.next();
                }
                _ => {
                    self.next();
                }
            }
        }
        if value.ends_with('\n') {
            value.pop();
        }
        Block::Code { value }
    }

    fn parse_list(&mut self, start: Option<u64>) -> Block {
        let list_ordered = start.is_some();
        let list_start = start.unwrap_or(1);
        self.next();
        let mut items = Vec::new();
        let mut ranges: Vec<Range<usize>> = Vec::new();
        while let Some((Event::Start(Tag::Item), range)) = self.peek() {
            let item_start = range.start;
            self.next();
            let (mut item, child_ranges) = self.parse_item_body();
            let item_end = if let Some((Event::End(TagEnd::Item), end_range)) = self.peek() {
                let end = end_range.end;
                self.next();
                end
            } else {
                item_start
            };
            let item_range = item_start..item_end;
            item.spread = item_is_spread(self.source, &child_ranges, &item_range);
            ranges.push(item_range);
            items.push(item);
        }
        if let Some((Event::End(TagEnd::List(_)), _)) = self.peek() {
            self.next();
        }
        Block::List {
            ordered: list_ordered,
            start: list_start,
            spread: list_is_spread(self.source, &ranges),
            items,
        }
    }

    fn parse_item_body(&mut self) -> (Item, Vec<Range<usize>>) {
        // Nested items parse through this same field. Restore the parent
        // marker after the body is captured, or a child wipes the checkbox.
        let saved_task = self.task_checked;
        self.task_checked = None;
        let mut blocks = Vec::new();
        let mut child_ranges = Vec::new();
        let mut inlines = Vec::new();
        let mut inline_range: Option<Range<usize>> = None;

        let flush = |blocks: &mut Vec<Block>,
                     child_ranges: &mut Vec<Range<usize>>,
                     inlines: &mut Vec<Inline>,
                     inline_range: &mut Option<Range<usize>>,
                     gfm: bool| {
            if inlines.is_empty() {
                return;
            }
            let taken = std::mem::take(inlines);
            let range = inline_range.take().unwrap_or(0..0);
            child_ranges.push(range);
            blocks.push(Block::Paragraph(finish_inlines(taken, gfm)));
        };

        while let Some((event, range)) = self.peek() {
            let range = range.clone();
            match event {
                Event::End(TagEnd::Item) => break,
                Event::TaskListMarker(_) => {
                    self.consume_task_marker();
                }
                Event::Start(Tag::Paragraph) => {
                    flush(
                        &mut blocks,
                        &mut child_ranges,
                        &mut inlines,
                        &mut inline_range,
                        self.gfm,
                    );
                    child_ranges.push(range);
                    blocks.push(self.parse_paragraph());
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    flush(
                        &mut blocks,
                        &mut child_ranges,
                        &mut inlines,
                        &mut inline_range,
                        self.gfm,
                    );
                    let level = *level;
                    child_ranges.push(range);
                    blocks.push(self.parse_heading(level));
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    flush(
                        &mut blocks,
                        &mut child_ranges,
                        &mut inlines,
                        &mut inline_range,
                        self.gfm,
                    );
                    child_ranges.push(range);
                    blocks.push(self.parse_blockquote());
                }
                Event::Start(Tag::CodeBlock(_)) => {
                    flush(
                        &mut blocks,
                        &mut child_ranges,
                        &mut inlines,
                        &mut inline_range,
                        self.gfm,
                    );
                    child_ranges.push(range);
                    blocks.push(self.parse_code_block());
                }
                Event::Start(Tag::List(start)) => {
                    flush(
                        &mut blocks,
                        &mut child_ranges,
                        &mut inlines,
                        &mut inline_range,
                        self.gfm,
                    );
                    let start = *start;
                    child_ranges.push(range);
                    blocks.push(self.parse_list(start));
                }
                Event::Start(Tag::HtmlBlock) => {
                    flush(
                        &mut blocks,
                        &mut child_ranges,
                        &mut inlines,
                        &mut inline_range,
                        self.gfm,
                    );
                    self.skip_container(TagEnd::HtmlBlock);
                }
                Event::Rule => {
                    flush(
                        &mut blocks,
                        &mut child_ranges,
                        &mut inlines,
                        &mut inline_range,
                        self.gfm,
                    );
                    self.next();
                    child_ranges.push(range);
                    blocks.push(Block::Rule);
                }
                Event::Html(_) | Event::InlineHtml(_) => {
                    self.next();
                }
                _ => {
                    if let Some(inline) = self.parse_inline() {
                        extend_range(&mut inline_range, &range);
                        inlines.push(inline);
                    } else {
                        self.next();
                    }
                }
            }
        }
        flush(
            &mut blocks,
            &mut child_ranges,
            &mut inlines,
            &mut inline_range,
            self.gfm,
        );
        if blocks.is_empty() {
            blocks.push(Block::Paragraph(Vec::new()));
        }
        let item = (
            Item {
                spread: false,
                checked: self.task_checked,
                children: blocks,
            },
            child_ranges,
        );
        self.task_checked = saved_task;
        item
    }

    fn parse_inlines(&mut self, stop: impl Fn(&TagEnd) -> bool) -> Vec<Inline> {
        let mut inlines = Vec::new();
        while let Some((event, _)) = self.peek() {
            if self.consume_task_marker() {
                continue;
            }
            if let Event::End(end) = event {
                if stop(end) {
                    self.next();
                    break;
                }
            }
            if let Some(inline) = self.parse_inline() {
                inlines.push(inline);
            } else if matches!(event, Event::End(_)) {
                break;
            } else {
                self.next();
            }
        }
        // Autolink once, at the paragraph or heading. Doing it here would
        // also turn URLs inside an existing link label into nested links.
        merge_text(inlines)
    }

    fn parse_inline(&mut self) -> Option<Inline> {
        let (event, _) = self.peek()?;
        match event {
            Event::Text(text) => {
                let text = text.clone();
                self.next();
                Some(text_inline(text.as_ref()))
            }
            Event::Code(code) => {
                let code = code.clone();
                self.next();
                Some(Inline::Code(code.to_string()))
            }
            Event::SoftBreak => {
                self.next();
                Some(Inline::Text("\n".to_string()))
            }
            Event::HardBreak => {
                self.next();
                Some(Inline::Break)
            }
            Event::Start(Tag::Emphasis) => {
                self.next();
                Some(Inline::Emphasis(
                    self.parse_inlines(|end| matches!(end, TagEnd::Emphasis)),
                ))
            }
            Event::Start(Tag::Strong) => {
                self.next();
                Some(Inline::Strong(
                    self.parse_inlines(|end| matches!(end, TagEnd::Strong)),
                ))
            }
            Event::Start(Tag::Strikethrough) => {
                self.next();
                Some(Inline::Delete(
                    self.parse_inlines(|end| matches!(end, TagEnd::Strikethrough)),
                ))
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                ..
            }) => {
                let link_type = *link_type;
                let url = destination(link_type, dest_url);
                let title = optional_title(title);
                self.next();
                let children = self.parse_inlines(|end| matches!(end, TagEnd::Link));
                Some(Inline::Link {
                    url,
                    title,
                    children,
                })
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                ..
            }) => {
                let link_type = *link_type;
                let url = destination(link_type, dest_url).unwrap_or_default();
                let title = optional_title(title);
                self.next();
                let alt_inlines = self.parse_inlines(|end| matches!(end, TagEnd::Image));
                let alt = alt_inlines.iter().map(Inline::plain_text).collect();
                Some(Inline::Image { alt, url, title })
            }
            Event::InlineMath(value) | Event::DisplayMath(value) => {
                let value = value.clone();
                self.next();
                Some(text_inline(value.as_ref()))
            }
            Event::FootnoteReference(label) => {
                let label = label.clone();
                self.next();
                Some(text_inline(&format!("[^{label}]")))
            }
            _ => None,
        }
    }

    fn skip_container(&mut self, end: TagEnd) {
        let mut depth = 1;
        self.next();
        while let Some((event, _)) = self.next() {
            match event {
                Event::Start(tag) if tag.to_end() == end => depth += 1,
                Event::End(found) if *found == end => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
}

fn text_inline(text: &str) -> Inline {
    Inline::Text(text.to_string())
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn optional_title(title: &str) -> Option<String> {
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

fn destination(link_type: LinkType, dest: &str) -> Option<String> {
    let unresolved = matches!(
        link_type,
        LinkType::ReferenceUnknown | LinkType::CollapsedUnknown | LinkType::ShortcutUnknown
    ) && dest.is_empty();
    if unresolved {
        return None;
    }
    if link_type == LinkType::Email
        && !dest.is_empty()
        && !dest.to_ascii_lowercase().starts_with("mailto:")
    {
        return Some(format!("mailto:{dest}"));
    }
    Some(dest.to_string())
}

fn merge_text(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut out = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(text) => push_text(&mut out, &text),
            Inline::Emphasis(children) => out.push(Inline::Emphasis(merge_text(children))),
            Inline::Strong(children) => out.push(Inline::Strong(merge_text(children))),
            Inline::Delete(children) => out.push(Inline::Delete(merge_text(children))),
            Inline::Link {
                url,
                title,
                children,
            } => out.push(Inline::Link {
                url,
                title,
                children: merge_text(children),
            }),
            other => out.push(other),
        }
    }
    out
}

fn gap_has_blank_line(gap: &str) -> bool {
    let mut previous_was_newline = false;
    for ch in gap.chars() {
        if ch == '\n' {
            if previous_was_newline {
                return true;
            }
            previous_was_newline = true;
        } else if ch == ' ' || ch == '\t' || ch == '\r' {
            // Whitespace inside a blank line still counts.
        } else {
            previous_was_newline = false;
        }
    }
    false
}

fn finish_inlines(inlines: Vec<Inline>, gfm: bool) -> Vec<Inline> {
    let merged = merge_text(inlines);
    if gfm {
        autolink(merged)
    } else {
        merged
    }
}

fn extend_range(range: &mut Option<Range<usize>>, extra: &Range<usize>) {
    match range {
        None => *range = Some(extra.clone()),
        Some(existing) => {
            existing.start = existing.start.min(extra.start);
            existing.end = existing.end.max(extra.end);
        }
    }
}

/// Position just after the last non-whitespace byte in `range`.
///
/// pulldown-cmark's container spans include the trailing blank lines that
/// actually separate a block from its next sibling. Spread is decided from
/// those gaps, using the same rule as mdast: a blank line is two newlines
/// with only whitespace between them.
fn last_non_ws_end(source: &str, range: &Range<usize>) -> usize {
    let end = range.end.min(source.len());
    let start = range.start.min(end);
    let slice = &source[start..end];
    let trimmed = slice.trim_end_matches([' ', '\t', '\n', '\r']);
    start + trimmed.len()
}

/// Blank line between an item's own children, or between its last child and a
/// trailing reference definition (which never becomes a block).
fn item_is_spread(source: &str, children: &[Range<usize>], item: &Range<usize>) -> bool {
    if children.is_empty() {
        return false;
    }
    let ends: Vec<usize> = children
        .iter()
        .map(|range| last_non_ws_end(source, range))
        .collect();
    for index in 0..children.len() - 1 {
        let gap_start = ends[index];
        let gap_end = children[index + 1].start;
        if gap_start <= gap_end && gap_has_blank_line(&source[gap_start..gap_end]) {
            return true;
        }
    }
    let last = ends[ends.len() - 1];
    let item_content_end = last_non_ws_end(source, item);
    last < item_content_end && gap_has_blank_line(&source[last..item_content_end])
}

fn list_is_spread(source: &str, ranges: &[Range<usize>]) -> bool {
    ranges.windows(2).any(|pair| {
        let end = last_non_ws_end(source, &pair[0]);
        end <= pair[1].start && gap_has_blank_line(&source[end..pair[1].start])
    })
}

/// GFM extended autolinks (`www.`, `http(s)://`, e-mail) inside plain text.
/// Mirrors `ExtendedAutolinkExtension`: the link text is the matched literal
/// and `www.` destinations gain an `http://` prefix.
fn autolink(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut out = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(text) => split_autolinks(&text, &mut out),
            Inline::Emphasis(children) => out.push(Inline::Emphasis(autolink(children))),
            Inline::Strong(children) => out.push(Inline::Strong(autolink(children))),
            Inline::Delete(children) => out.push(Inline::Delete(autolink(children))),
            other => out.push(other),
        }
    }
    out
}

fn split_autolinks(text: &str, out: &mut Vec<Inline>) {
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut chunk_start = 0;
    while index < bytes.len() {
        if let Some(found) = find_autolink(text, index) {
            if found.start > chunk_start {
                push_text(out, &text[chunk_start..found.start]);
            }
            out.push(Inline::Link {
                url: Some(found.url),
                title: None,
                children: vec![Inline::Text(found.label)],
            });
            index = found.end;
            chunk_start = found.end;
        } else {
            // `find_autolink` already scanned the rest of the string.
            break;
        }
    }
    if chunk_start < text.len() {
        push_text(out, &text[chunk_start..]);
    }
}

struct FoundLink {
    start: usize,
    end: usize,
    label: String,
    url: String,
}

fn find_autolink(text: &str, from: usize) -> Option<FoundLink> {
    let bytes = text.as_bytes();
    let mut index = from;
    while index < bytes.len() {
        if !preceded_by_boundary(text, index) {
            index += utf8_len(text, index);
            continue;
        }
        if let Some(found) = match_www(text, index)
            .or_else(|| match_url(text, index))
            .or_else(|| match_email(text, index))
        {
            return Some(found);
        }
        index += utf8_len(text, index);
    }
    None
}

fn utf8_len(text: &str, index: usize) -> usize {
    text[index..]
        .chars()
        .next()
        .map(|ch| ch.len_utf8())
        .unwrap_or(1)
}

fn preceded_by_boundary(text: &str, index: usize) -> bool {
    if index == 0 {
        return true;
    }
    let previous = text[..index].chars().next_back().unwrap_or('\0');
    previous.is_whitespace() || matches!(previous, '(' | '*' | '_' | '~' | '"' | '\'' | '[')
}

fn match_www(text: &str, index: usize) -> Option<FoundLink> {
    let rest = text.get(index..)?;
    if !rest.starts_with("www.") {
        return None;
    }
    let label = take_url_body(rest);
    if !label[4..].contains('.') {
        return None;
    }
    let (label, _trimmed) = trim_trailing_punctuation(label);
    if label.len() <= 4 {
        return None;
    }
    let end = index + label.len();
    Some(FoundLink {
        start: index,
        end,
        url: format!("http://{label}"),
        label: label.to_string(),
    })
}

fn match_url(text: &str, index: usize) -> Option<FoundLink> {
    let rest = text.get(index..)?;
    let scheme_len = if rest.starts_with("https://") {
        8
    } else if rest.starts_with("http://") {
        7
    } else {
        return None;
    };
    let label = take_url_body(rest);
    if label.len() <= scheme_len {
        return None;
    }
    let (label, _) = trim_trailing_punctuation(label);
    if label.len() <= scheme_len {
        return None;
    }
    Some(FoundLink {
        start: index,
        end: index + label.len(),
        url: label.to_string(),
        label: label.to_string(),
    })
}

fn match_email(text: &str, index: usize) -> Option<FoundLink> {
    let rest = text.get(index..)?;
    let bytes = rest.as_bytes();
    let mut at = None;
    for (offset, byte) in bytes.iter().enumerate() {
        match byte {
            b' ' | b'\t' | b'\n' | b'\r' | b'<' | b'>' | b'(' | b')' | b'[' | b']' => break,
            b'@' if offset > 0 => {
                at = Some(offset);
                break;
            }
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'.'
            | b'!'
            | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'/'
            | b'='
            | b'?'
            | b'^'
            | b'_'
            | b'`'
            | b'{'
            | b'|'
            | b'}'
            | b'~'
            | b'-' => {}
            _ => break,
        }
    }
    let at = at?;
    let mut end = at + 1;
    let mut dots = 0;
    while end < bytes.len() {
        match bytes[end] {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' => end += 1,
            b'.' => {
                dots += 1;
                end += 1;
            }
            _ => break,
        }
    }
    if dots == 0 || end == at + 1 {
        return None;
    }
    let mut label = &rest[..end];
    if label.ends_with('.') {
        label = &label[..label.len() - 1];
    }
    if !label.contains('.') || label.ends_with('@') {
        return None;
    }
    let domain = &label[at + 1..];
    if domain.len() < 3 || !domain.contains('.') {
        return None;
    }
    Some(FoundLink {
        start: index,
        end: index + label.len(),
        url: format!("mailto:{label}"),
        label: label.to_string(),
    })
}

fn take_url_body(rest: &str) -> &str {
    let bytes = rest.as_bytes();
    let mut end = 0;
    while end < bytes.len() {
        match bytes[end] {
            b' ' | b'\t' | b'\n' | b'\r' | b'<' => break,
            _ => end += 1,
        }
    }
    &rest[..end]
}

fn trim_trailing_punctuation(mut label: &str) -> (&str, usize) {
    let original = label.len();
    while let Some(last) = label.chars().next_back() {
        if matches!(
            last,
            '.' | ',' | '!' | '?' | ':' | ';' | '*' | '_' | '~' | ')' | '\'' | '"'
        ) {
            // Keep a closing paren when the label opened one that is still unmatched.
            if last == ')' {
                let open = label.chars().filter(|ch| *ch == '(').count();
                let close = label.chars().filter(|ch| *ch == ')').count();
                if close <= open {
                    break;
                }
            }
            label = &label[..label.len() - last.len_utf8()];
        } else {
            break;
        }
    }
    (label, original - label.len())
}
