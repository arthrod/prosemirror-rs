//! Markdown AST used between pulldown-cmark and ProseMirror.
//!
//! The shape follows mdast closely enough that stringify can copy
//! `mdast-util-to-markdown` with the options `prosemirror-remark` sets
//! (`fences: true`, `listItemIndent: "one"`, `rule: "-"`, bullet `*`).

/// A markdown document.
#[derive(Debug, Clone, PartialEq)]
pub struct Doc {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading {
        level: u8,
        children: Vec<Inline>,
    },
    Blockquote(Vec<Block>),
    Code {
        value: String,
    },
    Rule,
    List {
        ordered: bool,
        start: u64,
        /// Blank line between items, after `isListSpread` promotion.
        spread: bool,
        items: Vec<Item>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Blank line between this item's own children.
    pub spread: bool,
    /// `Some` for a GFM task list item.
    pub checked: Option<bool>,
    pub children: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Code(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Delete(Vec<Inline>),
    Link {
        /// `None` is an unresolved reference (`href: null`).
        url: Option<String>,
        title: Option<String>,
        children: Vec<Inline>,
    },
    Image {
        alt: String,
        url: String,
        title: Option<String>,
    },
    Break,
}

impl Inline {
    pub fn plain_text(&self) -> String {
        match self {
            Inline::Text(value) | Inline::Code(value) => value.clone(),
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Delete(children)
            | Inline::Link { children, .. } => children.iter().map(Inline::plain_text).collect(),
            Inline::Image { alt, .. } => alt.clone(),
            Inline::Break => "\n".to_string(),
        }
    }
}

pub fn push_text(inlines: &mut Vec<Inline>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Inline::Text(existing)) = inlines.last_mut() {
        existing.push_str(text);
    } else {
        inlines.push(Inline::Text(text.to_string()));
    }
}
