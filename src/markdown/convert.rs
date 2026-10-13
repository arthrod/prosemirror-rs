//! ProseMirror nodes ↔ the markdown AST.
//!
//! Marks are not nested in ProseMirror. On the way out, the mark that covers
//! the longest run of neighbouring nodes is wrapped outermost, and ties keep
//! the lowest rank (`link`, then `code`, `em`, `strong`, `strikethrough`).
//! `code` is a leaf mark: it becomes an inline code node and is dropped when
//! it sits on a hard break or an image, matching `InlineCodeExtension`.

use crate::dynamic::{DynamicMark, DynamicNode, DynamicSchema};
use crate::model::{Fragment, MarkSet, Node};
use serde_json::{Map, Value};

use super::md::{Block, Doc, Inline, Item};

pub(crate) fn ast_to_node(schema: &DynamicSchema, doc: &Doc) -> DynamicNode {
    schema.with_types(|| {
        let children = blocks_to_nodes(schema, &doc.blocks);
        element(schema, "doc", Value::Object(Map::new()), children)
    })
}

pub(crate) fn node_to_doc(node: &DynamicNode) -> Doc {
    if node.type_name == "doc" {
        Doc {
            blocks: child_blocks(node),
        }
    } else if let Some(block) = node_to_block(node) {
        Doc {
            blocks: vec![block],
        }
    } else {
        Doc {
            blocks: vec![Block::Paragraph(phrasing(node))],
        }
    }
}

fn blocks_to_nodes(schema: &DynamicSchema, blocks: &[Block]) -> Vec<DynamicNode> {
    let nodes: Vec<DynamicNode> = blocks
        .iter()
        .map(|block| block_to_node(schema, block))
        .collect();
    if nodes.is_empty() {
        vec![element(
            schema,
            "paragraph",
            Value::Object(Map::new()),
            Vec::new(),
        )]
    } else {
        nodes
    }
}

fn block_to_node(schema: &DynamicSchema, block: &Block) -> DynamicNode {
    match block {
        Block::Paragraph(inlines) => element(
            schema,
            "paragraph",
            Value::Object(Map::new()),
            inline_nodes(schema, inlines),
        ),
        Block::Heading { level, children } => element(
            schema,
            "heading",
            serde_json::json!({ "level": level }),
            inline_nodes(schema, children),
        ),
        Block::Blockquote(blocks) => element(
            schema,
            "blockquote",
            Value::Object(Map::new()),
            blocks_to_nodes(schema, blocks),
        ),
        Block::Code { value } => {
            let mut children = Vec::new();
            if let Some(text) = text_node(schema, value, MarkSet::new()) {
                children.push(text);
            }
            element(schema, "code_block", Value::Object(Map::new()), children)
        }
        Block::Rule => element(
            schema,
            "horizontal_rule",
            Value::Object(Map::new()),
            Vec::new(),
        ),
        Block::List {
            ordered,
            start,
            spread,
            items,
        } => {
            let mut attrs = Map::new();
            attrs.insert("spread".to_string(), Value::Bool(*spread));
            if *ordered {
                attrs.insert("start".to_string(), serde_json::json!(start));
            }
            let children = items
                .iter()
                .map(|item| item_to_node(schema, item))
                .collect();
            element(
                schema,
                if *ordered {
                    "ordered_list"
                } else {
                    "bullet_list"
                },
                Value::Object(attrs),
                children,
            )
        }
    }
}

fn item_to_node(schema: &DynamicSchema, item: &Item) -> DynamicNode {
    let mut attrs = Map::new();
    attrs.insert("spread".to_string(), Value::Bool(item.spread));
    let name = if let Some(checked) = item.checked {
        attrs.insert("checked".to_string(), Value::Bool(checked));
        "task_list_item"
    } else {
        "regular_list_item"
    };
    element(
        schema,
        name,
        Value::Object(attrs),
        blocks_to_nodes(schema, &item.children),
    )
}

fn inline_nodes(schema: &DynamicSchema, inlines: &[Inline]) -> Vec<DynamicNode> {
    let mut nodes = Vec::new();
    for inline in inlines {
        nodes.extend(inline_to_nodes(schema, inline));
    }
    nodes
}

fn inline_to_nodes(schema: &DynamicSchema, inline: &Inline) -> Vec<DynamicNode> {
    match inline {
        Inline::Text(text) => text_node(schema, text, MarkSet::new())
            .into_iter()
            .collect(),
        Inline::Code(text) => {
            let mut marks = MarkSet::new();
            marks.add(&simple_mark("code"));
            text_node(schema, text, marks).into_iter().collect()
        }
        Inline::Break => vec![element(
            schema,
            "hard_break",
            Value::Object(Map::new()),
            Vec::new(),
        )],
        Inline::Image { alt, url, title } => {
            let mut attrs = Map::new();
            attrs.insert("alt".to_string(), Value::String(alt.clone()));
            attrs.insert("src".to_string(), Value::String(url.clone()));
            if let Some(title) = title {
                attrs.insert("title".to_string(), Value::String(title.clone()));
            }
            vec![element(schema, "image", Value::Object(attrs), Vec::new())]
        }
        Inline::Emphasis(children) => add_mark(inline_nodes(schema, children), simple_mark("em")),
        Inline::Strong(children) => add_mark(inline_nodes(schema, children), simple_mark("strong")),
        Inline::Delete(children) => {
            add_mark(inline_nodes(schema, children), simple_mark("strikethrough"))
        }
        Inline::Link {
            url,
            title,
            children,
        } => add_mark(
            inline_nodes(schema, children),
            link_mark(url.clone(), title.clone()),
        ),
    }
}

fn add_mark(nodes: Vec<DynamicNode>, mark: DynamicMark) -> Vec<DynamicNode> {
    nodes
        .into_iter()
        .map(|node| {
            let mut marks = node.marks().cloned().unwrap_or_default();
            marks.add(&mark);
            node.mark(marks)
        })
        .collect()
}

fn text_node(
    schema: &DynamicSchema,
    text: &str,
    marks: MarkSet<crate::dynamic::types::Dyn>,
) -> Option<DynamicNode> {
    if text.is_empty() {
        return None;
    }
    Some(schema.text(text).mark(marks))
}

fn element(
    schema: &DynamicSchema,
    name: &str,
    attrs: Value,
    children: Vec<DynamicNode>,
) -> DynamicNode {
    schema
        .node(name, attrs, Fragment::from_array(children), MarkSet::new())
        .unwrap_or_else(|err| panic!("failed to create {name}: {err}"))
}

fn simple_mark(name: &str) -> DynamicMark {
    DynamicMark {
        type_name: name.to_string(),
        attrs: Value::Object(Map::new()),
    }
}

fn link_mark(url: Option<String>, title: Option<String>) -> DynamicMark {
    let mut attrs = Map::new();
    attrs.insert(
        "href".to_string(),
        match url {
            Some(url) => Value::String(url),
            None => Value::Null,
        },
    );
    if let Some(title) = title {
        attrs.insert("title".to_string(), Value::String(title));
    }
    DynamicMark {
        type_name: "link".to_string(),
        attrs: Value::Object(attrs),
    }
}

fn child_blocks(node: &DynamicNode) -> Vec<Block> {
    let mut blocks = Vec::new();
    for index in 0..node.child_count() {
        if let Some(block) = node.child(index).and_then(node_to_block) {
            blocks.push(block);
        }
    }
    blocks
}

fn node_to_block(node: &DynamicNode) -> Option<Block> {
    match node.type_name.as_str() {
        "paragraph" => Some(Block::Paragraph(phrasing(node))),
        "heading" => Some(Block::Heading {
            level: attr_u64(node, "level", 1).clamp(1, 6) as u8,
            children: phrasing(node),
        }),
        "blockquote" => {
            let mut blocks = child_blocks(node);
            if blocks.is_empty() {
                blocks.push(Block::Paragraph(Vec::new()));
            }
            Some(Block::Blockquote(blocks))
        }
        "code_block" => Some(Block::Code {
            value: plain_text(node),
        }),
        "horizontal_rule" => Some(Block::Rule),
        "bullet_list" | "ordered_list" => Some(list_block(node)),
        _ => None,
    }
}

fn list_block(node: &DynamicNode) -> Block {
    let ordered = node.type_name == "ordered_list";
    let mut items = Vec::new();
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            if child.type_name == "regular_list_item" || child.type_name == "task_list_item" {
                items.push(item_block(child));
            }
        }
    }
    let spread = attr_bool(node, "spread", false)
        || items
            .iter()
            .any(|item| item.spread && item.children.len() < 2);
    Block::List {
        ordered,
        start: if ordered {
            attr_u64(node, "start", 1)
        } else {
            1
        },
        spread,
        items,
    }
}

fn item_block(node: &DynamicNode) -> Item {
    let mut children = child_blocks(node);
    if children.is_empty() {
        children.push(Block::Paragraph(Vec::new()));
    }
    Item {
        spread: attr_bool(node, "spread", false),
        checked: if node.type_name == "task_list_item" {
            Some(attr_bool(node, "checked", false))
        } else {
            None
        },
        children,
    }
}

fn phrasing(node: &DynamicNode) -> Vec<Inline> {
    let mut pending = Vec::new();
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            pending.push(convert_child(child));
        }
    }
    apply_marks(pending)
}

fn plain_text(node: &DynamicNode) -> String {
    let mut value = String::new();
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            if let Some(text) = child.text_node() {
                value.push_str(text.text.as_str());
            }
        }
    }
    value
}

struct Pending {
    nodes: Vec<Inline>,
    marks: Vec<DynamicMark>,
}

fn convert_child(node: &DynamicNode) -> Pending {
    let mut leaf = false;
    let mut marks = Vec::new();
    if let Some(set) = node.marks() {
        for mark in set {
            if mark.type_name == "code" {
                leaf = true;
            } else {
                marks.push(mark.clone());
            }
        }
    }
    let mut nodes = convert_inline(node);
    if leaf {
        nodes = nodes
            .into_iter()
            .map(|inline| match inline {
                Inline::Text(value) => Inline::Code(value),
                other => other,
            })
            .collect();
    }
    Pending { nodes, marks }
}

fn convert_inline(node: &DynamicNode) -> Vec<Inline> {
    if node.is_text() {
        let text = node
            .text_node()
            .map(|text| text.text.as_str().to_string())
            .unwrap_or_default();
        return vec![Inline::Text(text)];
    }
    match node.type_name.as_str() {
        "hard_break" => vec![Inline::Break],
        "image" => vec![Inline::Image {
            alt: attr_string(node, "alt").unwrap_or_default(),
            url: attr_string(node, "src").unwrap_or_default(),
            title: attr_string(node, "title"),
        }],
        _ => {
            let text = node.text_content();
            if text.is_empty() {
                Vec::new()
            } else {
                vec![Inline::Text(text)]
            }
        }
    }
}

fn apply_marks(nodes: Vec<Pending>) -> Vec<Inline> {
    let mut result = Vec::new();
    let mut start = 0;
    while start < nodes.len() {
        if nodes[start].marks.is_empty() {
            result.extend(nodes[start].nodes.clone());
            start += 1;
            continue;
        }
        let (mark, end) = longest_mark(&nodes, start);
        let run = nodes[start..end]
            .iter()
            .map(|pending| without(pending, &mark))
            .collect();
        result.extend(wrap_mark(&mark, apply_marks(run)));
        start = end;
    }
    result
}

fn longest_mark(nodes: &[Pending], start: usize) -> (DynamicMark, usize) {
    let mut best_mark = None;
    let mut best_end = 0;
    for mark in &nodes[start].marks {
        let end = mark_run_end(nodes, start, mark);
        if best_mark.is_none() || end > best_end {
            best_mark = Some(mark.clone());
            best_end = end;
        }
    }
    (best_mark.expect("pending mark"), best_end)
}

fn mark_run_end(nodes: &[Pending], start: usize, mark: &DynamicMark) -> usize {
    let mut end = start;
    while end < nodes.len() && nodes[end].marks.iter().any(|pending| pending == mark) {
        end += 1;
    }
    end
}

fn without(pending: &Pending, mark: &DynamicMark) -> Pending {
    Pending {
        nodes: pending.nodes.clone(),
        marks: pending
            .marks
            .iter()
            .filter(|candidate| *candidate != mark)
            .cloned()
            .collect(),
    }
}

fn wrap_mark(mark: &DynamicMark, children: Vec<Inline>) -> Vec<Inline> {
    match mark.type_name.as_str() {
        "em" => vec![Inline::Emphasis(children)],
        "strong" => vec![Inline::Strong(children)],
        "strikethrough" => vec![Inline::Delete(children)],
        "link" => vec![Inline::Link {
            url: match mark.attrs.get("href") {
                Some(Value::String(url)) => Some(url.clone()),
                _ => None,
            },
            title: match mark.attrs.get("title") {
                Some(Value::String(title)) => Some(title.clone()),
                _ => None,
            },
            children,
        }],
        _ => children,
    }
}

fn attr_bool(node: &DynamicNode, key: &str, default: bool) -> bool {
    node.attrs
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn attr_u64(node: &DynamicNode, key: &str, default: u64) -> u64 {
    node.attrs
        .get(key)
        .and_then(Value::as_u64)
        .unwrap_or(default)
}

fn attr_string(node: &DynamicNode, key: &str) -> Option<String> {
    match node.attrs.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}
