//! Markdown schema and CommonMark / GFM conversion.
//!
//! This module ports [prosemirror-remark](https://github.com/arthrod/prosemirror-remark)
//! onto the dynamic document model. Parsing uses
//! [pulldown-cmark](https://docs.rs/pulldown-cmark), the same engine as
//! [markdown-remarkable](https://github.com/arthrod/markdown-remarkable).
//! Serialization follows `remark-stringify` with the options that library sets:
//! fenced code, `listItemIndent: "one"`, thematic breaks as `---`, and `*` bullets.
//!
//! ```
//! use prosemirror::markdown::Remark;
//!
//! let remark = Remark::commonmark();
//! let parsed = remark.parse("**bold**");
//! assert_eq!(remark.serialize(&parsed.doc), "**bold**\n");
//! ```
//!
//! # Schema
//!
//! Node order matches `MarkdownExtension`, so the default block is `paragraph`
//! and mark rank is `link`, `code`, `em`, `strong`, then (GFM) `strikethrough`.
//! That rank is what makes a link wrap the other marks when they cover the same
//! text, and what makes `***text***` serialize with emphasis outside strong.
//!
//! # Editor features
//!
//! Input rules, keymaps, and the task-list checkbox node view are not part of
//! this crate. They depend on `prosemirror-state`, `prosemirror-view`,
//! `prosemirror-commands`, `prosemirror-inputrules`, and `prosemirror-schema-list`.
//! The node and mark names match upstream, so those bindings can target this schema:
//!
//! * Headings: input `# `–`###### `; `Shift-Mod-1`…`6`, `Tab` / `Shift-Tab`.
//! * Blockquotes: `> `, `Mod->`.
//! * Code blocks: fence or four spaces, `Shift-Mod-\`, `Enter` to lift out.
//! * Rules: `---`, `***`, `___`, `Mod-_`.
//! * Lists: `Shift-Mod-8` / `Shift-Mod-9`, `Enter`, `Tab`, `Shift-Tab`.
//! * Bold `Mod-b`, italic `Mod-i`, code `` Ctrl-` ``.
//! * Task items: `[ ]` / `[x]` at the start of an item; `Backspace` there turns
//!   a task item back into a regular item.
//!
//! # Known gaps
//!
//! Same as upstream: raw HTML is dropped, a fenced code block's info string is
//! not stored, and tables and footnotes are not in the schema.

mod convert;
mod md;
mod parse;
mod schema;
mod stringify;

pub use schema::Flavor;

use crate::dynamic::{DynamicNode, DynamicSchema};

/// A parsed Markdown document and the schema its node types belong to.
///
/// [`doc`](MarkdownDoc::doc)'s type indexes are valid for [`schema`](MarkdownDoc::schema).
/// Schema-aware model operations need [`DynamicSchema::with_types`].
pub struct MarkdownDoc {
    /// Schema used to build [`doc`](MarkdownDoc::doc).
    pub schema: DynamicSchema,
    /// The ProseMirror document.
    pub doc: DynamicNode,
}

impl MarkdownDoc {
    /// ProseMirror JSON, omitting attributes that match their schema default.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        self.schema.with_types(|| self.doc.to_json(true))
    }
}

/// CommonMark or GitHub Flavored Markdown parser and serializer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Remark {
    flavor: Flavor,
}

impl Remark {
    /// CommonMark, without GFM strikethrough, task lists, or extended autolinks.
    #[must_use]
    pub fn commonmark() -> Self {
        Self {
            flavor: Flavor::CommonMark,
        }
    }

    /// CommonMark plus GFM strikethrough, task lists, and extended autolinks.
    #[must_use]
    pub fn gfm() -> Self {
        Self {
            flavor: Flavor::Gfm,
        }
    }

    /// Which Markdown syntax this converter accepts.
    #[must_use]
    pub fn flavor(self) -> Flavor {
        self.flavor
    }

    /// A fresh schema for this flavor.
    ///
    /// Two schemas from the same flavor number their types identically, so
    /// documents parsed with one can be serialized with another of the same flavor.
    #[must_use]
    pub fn schema(self) -> DynamicSchema {
        schema::schema_for(self.flavor)
    }

    /// Parse `markdown` into a ProseMirror document.
    ///
    /// Parsing does not fail. Empty input becomes one empty paragraph, which
    /// satisfies `doc`'s `block+` content. Reference definitions are resolved
    /// and omitted; an unresolved reference stays literal text.
    #[must_use]
    pub fn parse(self, markdown: &str) -> MarkdownDoc {
        let schema = self.schema();
        let ast = parse::parse_markdown(markdown, self.flavor);
        let doc = convert::ast_to_node(&schema, &ast);
        MarkdownDoc { schema, doc }
    }

    /// Serialize `doc` with the remark options used by `prosemirror-remark`.
    ///
    /// The string ends with a newline. Hard breaks are `\\\n`, rules are `---`,
    /// and task items keep their checkbox. A single-child spread item loosens
    /// the whole list, matching `isListSpread`.
    #[must_use]
    pub fn serialize(self, doc: &DynamicNode) -> String {
        stringify::stringify_doc(&convert::node_to_doc(doc))
    }
}

#[cfg(test)]
mod tests;
