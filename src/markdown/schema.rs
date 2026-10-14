//! ProseMirror schema matching `prosemirror-remark`'s extensions.
//!
//! Node order follows `MarkdownExtension` / `GFMExtension` dependency order so
//! the first block is `paragraph` and mark rank is `link`, `code`, `em`,
//! `strong`, then (GFM) `strikethrough`. That rank is what makes a link wrap
//! other marks when they cover the same text, and what makes `***text***`
//! serialize with emphasis outside strong. Headings take `inline*` content,
//! so an image or a hard break in a heading stays a valid document.

use crate::dynamic::DynamicSchema;

/// Which Markdown syntax the schema and parser accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    /// CommonMark constructs from `MarkdownExtension`.
    CommonMark,
    /// CommonMark plus strikethrough, task lists, and extended autolinks.
    Gfm,
}

/// JSON `SchemaSpec` for `flavor`, in the node and mark order
/// [`schema_for`] loads.
///
/// `prosemirror::editor::Editor` keys its schema cache on this exact string,
/// so callers that parse with [`super::Remark`] and then apply steps must
/// pass [`super::Remark::schema_json`], not a hand-written spec.
pub fn spec_for(flavor: Flavor) -> serde_json::Value {
    let mut nodes = serde_json::json!({
        "paragraph": { "content": "inline*", "group": "block" },
        "blockquote": { "content": "block+", "group": "block" },
        "hard_break": { "group": "inline", "inline": true, "selectable": false },
        "code_block": {
            "code": true,
            "content": "text*",
            "defining": true,
            "group": "block",
            "marks": ""
        },
        "heading": {
            "attrs": { "level": { "default": 1 } },
            "content": "inline*",
            "defining": true,
            "group": "block"
        },
        "horizontal_rule": { "group": "block" },
        "image": {
            "attrs": {
                "alt": { "default": null },
                "src": {},
                "title": { "default": null }
            },
            "draggable": true,
            "group": "inline",
            "inline": true
        },
        "regular_list_item": {
            "attrs": { "spread": { "default": false } },
            "content": "block+",
            "defining": true,
            "group": "list_item"
        },
        "ordered_list": {
            "attrs": { "spread": { "default": false }, "start": { "default": 1 } },
            "content": "list_item+",
            "group": "block"
        },
        "doc": { "content": "block+" },
        "text": { "group": "inline" },
        "bullet_list": {
            "attrs": { "spread": { "default": false } },
            "content": "list_item+",
            "group": "block"
        }
    });

    let mut marks = serde_json::json!({
        "link": {
            "attrs": { "href": {}, "title": { "default": null } },
            "inclusive": false
        },
        "code": { "inclusive": false },
        "em": {},
        "strong": {}
    });

    if flavor == Flavor::Gfm {
        nodes["task_list_item"] = serde_json::json!({
            "attrs": { "checked": { "default": false }, "spread": { "default": false } },
            "content": "paragraph block*",
            "defining": true,
            "group": "list_item"
        });
        marks["strikethrough"] = serde_json::json!({});
    }

    serde_json::json!({
        "nodes": nodes,
        "marks": marks
    })
}

/// Build the dynamic schema for `flavor`.
pub fn schema_for(flavor: Flavor) -> DynamicSchema {
    DynamicSchema::from_json(&spec_for(flavor)).expect("markdown schema is valid")
}
