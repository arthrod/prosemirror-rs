//! DOM serializers and parsers for the markdown schema.
//!
//! These follow each extension's `toDOM` / `parseDOM` spec. There is no
//! browser here: the tree is a plain element model, and task-item checkbox
//! clicks are [`click_task_checkbox`] rather than a `NodeView`.

use serde_json::{json, Value};

use crate::dynamic::types::Dyn;
use crate::dynamic::{DynamicMark, DynamicNode, DynamicSchema};
use crate::model::{Fragment, MarkSet, Node};

/// An element in the DOM model used by `toDOM` and `parseDOM`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomElement {
    /// Tag name (`p`, `h1`, `li`, …).
    pub tag: String,
    /// Attributes in source order. Booleans are `"true"` / `"false"`.
    pub attrs: Vec<(String, String)>,
    /// Child nodes. A content hole is represented by the element's children.
    pub children: Vec<DomNode>,
}

/// A DOM node produced by `toDOM` or accepted by `parseDOM`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomNode {
    /// An element.
    Element(DomElement),
    /// A text node.
    Text(String),
}

impl DomNode {
    fn element(tag: &str, attrs: Vec<(String, String)>, children: Vec<DomNode>) -> Self {
        DomNode::Element(DomElement {
            tag: tag.to_string(),
            attrs,
            children,
        })
    }

    fn plain(tag: &str, children: Vec<DomNode>) -> Self {
        Self::element(tag, Vec::new(), children)
    }
}

/// Render `node` with the extension `toDOM` specs.
///
/// Task items use the static spec (a disabled checkbox), not the node view.
#[must_use]
pub fn to_dom(node: &DynamicNode) -> DomNode {
    render_node(node, false)
}

/// Render a task item the way `TaskListItemView` does: an enabled checkbox
/// outside the editable content. Other nodes match [`to_dom`].
#[must_use]
pub fn to_node_view(node: &DynamicNode) -> DomNode {
    render_node(node, true)
}

fn render_node(node: &DynamicNode, task_view: bool) -> DomNode {
    if node.is_text() {
        let text = node.text_content();
        return wrap_marks(node.marks(), DomNode::Text(text));
    }
    let children = node
        .content()
        .map(|c| {
            c.children()
                .iter()
                .map(|n| render_node(n, task_view))
                .collect()
        })
        .unwrap_or_default();
    let rendered = match node.type_name.as_str() {
        "doc" => DomNode::element("doc", Vec::new(), children),
        "paragraph" => DomNode::plain("p", children),
        "blockquote" => DomNode::plain("blockquote", children),
        "heading" => {
            let level = attr_i64(node, "level", 1).clamp(1, 6);
            DomNode::plain(&format!("h{level}"), children)
        }
        "code_block" => DomNode::plain("pre", vec![DomNode::plain("code", children)]),
        "horizontal_rule" => DomNode::plain("div", vec![DomNode::plain("hr", Vec::new())]),
        "hard_break" => DomNode::plain("br", Vec::new()),
        "image" => DomNode::element("img", image_attrs(node), Vec::new()),
        "bullet_list" => DomNode::element(
            "ul",
            vec![(
                "data-spread".into(),
                bool_attr(attr_bool(node, "spread", false)),
            )],
            children,
        ),
        "ordered_list" => DomNode::element(
            "ol",
            vec![
                (
                    "data-spread".into(),
                    bool_attr(attr_bool(node, "spread", false)),
                ),
                ("start".into(), attr_i64(node, "start", 1).to_string()),
            ],
            children,
        ),
        "regular_list_item" => DomNode::element(
            "li",
            vec![(
                "data-spread".into(),
                bool_attr(attr_bool(node, "spread", false)),
            )],
            children,
        ),
        "task_list_item" => render_task(node, children, task_view),
        _ => DomNode::element(&node.type_name, Vec::new(), children),
    };
    // Marks on non-text inlines (an image inside emphasis, for example).
    if node.is_inline() {
        wrap_marks(node.marks(), rendered)
    } else {
        rendered
    }
}

fn render_task(node: &DynamicNode, children: Vec<DomNode>, node_view: bool) -> DomNode {
    let checked = attr_bool(node, "checked", false);
    let spread = attr_bool(node, "spread", false);
    let mut input_attrs = vec![("type".into(), "checkbox".into())];
    if checked {
        input_attrs.push(("checked".into(), "checked".into()));
    }
    if !node_view {
        input_attrs.push(("disabled".into(), "disabled".into()));
    }
    let checkbox = DomNode::element(
        "span",
        vec![
            (
                "class".into(),
                "prosemirror-remark-task-list-item-checkbox".into(),
            ),
            ("contenteditable".into(), "false".into()),
        ],
        vec![DomNode::element("input", input_attrs, Vec::new())],
    );
    let content = DomNode::element(
        "div",
        vec![(
            "class".into(),
            "prosemirror-remark-task-list-item-content".into(),
        )],
        children,
    );
    DomNode::element(
        "li",
        vec![
            ("class".into(), "prosemirror-remark-task-list-item".into()),
            ("data-checked".into(), bool_attr(checked)),
            ("data-spread".into(), bool_attr(spread)),
        ],
        vec![checkbox, content],
    )
}

fn wrap_marks(marks: Option<&MarkSet<Dyn>>, inner: DomNode) -> DomNode {
    let Some(marks) = marks else {
        return inner;
    };
    let mut out = inner;
    // Lowest rank is the outside mark, matching `DOMSerializer`.
    for mark in marks.iter().rev() {
        out = match mark.type_name.as_str() {
            "strong" => DomNode::plain("strong", vec![out]),
            "em" => DomNode::plain("em", vec![out]),
            "code" => DomNode::plain("code", vec![out]),
            "strikethrough" => DomNode::plain("s", vec![out]),
            "link" => {
                let mut attrs = vec![("href".into(), attr_string(mark, "href"))];
                if let Some(title) = attr_string_opt(mark, "title") {
                    attrs.push(("title".into(), title));
                }
                DomNode::element("a", attrs, vec![out])
            }
            _ => out,
        };
    }
    out
}

/// Where a task-item event landed, for [`task_stops_event`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskEventTarget {
    /// Inside the non-editable checkbox container.
    Checkbox,
    /// Inside the editable content.
    Content,
}

/// `TaskListItemView.stopEvent`: events on the checkbox never reach the editor.
#[must_use]
pub fn task_stops_event(target: TaskEventTarget) -> bool {
    matches!(target, TaskEventTarget::Checkbox)
}

/// Toggle `checked` on the task item that starts at `pos`, as the node view's
/// checkbox click does.
#[must_use]
pub fn click_task_checkbox(
    schema: &DynamicSchema,
    doc: &DynamicNode,
    pos: usize,
) -> Option<DynamicNode> {
    schema.with_types(|| {
        let node = doc.node_at(pos)?;
        if node.type_name != "task_list_item" {
            return None;
        }
        let checked = attr_bool(node, "checked", false);
        let mut tr = crate::transform::Transform::<Dyn>::new(doc.clone());
        tr.set_node_attribute(pos, "checked", json!(!checked));
        Some(tr.doc)
    })
}

/// Parse a DOM tree with the extension `parseDOM` rules.
///
/// A fragment of blocks becomes a `doc`. Unknown elements are transparent:
/// their children are parsed in the same context. `li` rules prefer a task
/// item (priority 60) when the element contains a checkbox.
pub fn parse_dom(schema: &DynamicSchema, dom: &DomNode) -> Result<DynamicNode, String> {
    schema.with_types(|| {
        let blocks = parse_blocks(schema, std::slice::from_ref(dom))?;
        if blocks.len() == 1 && blocks[0].type_name == "doc" {
            return Ok(blocks.into_iter().next().unwrap());
        }
        let frag = Fragment::from(blocks);
        schema
            .node("doc", json!({}), frag, MarkSet::new())
            .map_err(|e| e.to_string())
    })
}

fn parse_blocks(schema: &DynamicSchema, nodes: &[DomNode]) -> Result<Vec<DynamicNode>, String> {
    let mut out = Vec::new();
    let mut inline: Vec<DynamicNode> = Vec::new();
    let flush = |inline: &mut Vec<DynamicNode>, out: &mut Vec<DynamicNode>| -> Result<(), String> {
        if inline.is_empty() {
            return Ok(());
        }
        let nodes = std::mem::take(inline);
        let nodes = normalize_normal_whitespace(nodes);
        let frag = Fragment::from_array(nodes);
        out.push(
            schema
                .node("paragraph", json!({}), frag, MarkSet::new())
                .map_err(|e| e.to_string())?,
        );
        Ok(())
    };
    for node in nodes {
        match classify(schema, node)? {
            Parsed::Block(block) => {
                flush(&mut inline, &mut out)?;
                out.push(block);
            }
            Parsed::Inlines(nodes) => inline.extend(nodes),
            Parsed::Transparent(children) => {
                // Could be a mix; parse them in this same block context.
                let nested = parse_blocks(schema, &children)?;
                for child in nested {
                    if child.is_block() && child.type_name != "paragraph" {
                        flush(&mut inline, &mut out)?;
                        out.push(child);
                    } else if child.type_name == "paragraph" {
                        if let Some(content) = child.content() {
                            inline.extend(content.children().iter().cloned());
                        }
                    } else {
                        inline.push(child);
                    }
                }
            }
        }
    }
    flush(&mut inline, &mut out)?;
    Ok(out)
}

enum Parsed {
    Block(DynamicNode),
    Inlines(Vec<DynamicNode>),
    Transparent(Vec<DomNode>),
}

fn classify(schema: &DynamicSchema, node: &DomNode) -> Result<Parsed, String> {
    let DomNode::Element(el) = node else {
        return Ok(Parsed::Inlines(vec![text_node(
            &text_of(node),
            &MarkSet::new(),
        )]));
    };
    let tag = el.tag.to_ascii_lowercase();
    match tag.as_str() {
        "doc" => {
            let blocks = parse_blocks(schema, &el.children)?;
            return Ok(Parsed::Block(
                schema
                    .node("doc", json!({}), Fragment::from(blocks), MarkSet::new())
                    .map_err(|e| e.to_string())?,
            ));
        }
        "p" => {
            return Ok(Parsed::Block(inline_block(
                schema,
                "paragraph",
                json!({}),
                el,
                &MarkSet::new(),
            )?));
        }
        "blockquote" => {
            let blocks = parse_blocks(schema, &el.children)?;
            return Ok(Parsed::Block(
                schema
                    .node(
                        "blockquote",
                        json!({}),
                        Fragment::from(blocks),
                        MarkSet::new(),
                    )
                    .map_err(|e| e.to_string())?,
            ));
        }
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level: i64 = tag[1..].parse().unwrap_or(1);
            return Ok(Parsed::Block(inline_block(
                schema,
                "heading",
                json!({ "level": level }),
                el,
                &MarkSet::new(),
            )?));
        }
        "pre" => {
            let text = text_content(el);
            let content = if text.is_empty() {
                Fragment::new()
            } else {
                Fragment::from(vec![text_node(&text, &MarkSet::new())])
            };
            return Ok(Parsed::Block(
                schema
                    .node("code_block", json!({}), content, MarkSet::new())
                    .map_err(|e| e.to_string())?,
            ));
        }
        "hr" => {
            return Ok(Parsed::Block(
                schema
                    .node(
                        "horizontal_rule",
                        json!({}),
                        Fragment::new(),
                        MarkSet::new(),
                    )
                    .map_err(|e| e.to_string())?,
            ));
        }
        "br" => {
            return Ok(Parsed::Inlines(vec![schema
                .node("hard_break", json!({}), Fragment::new(), MarkSet::new())
                .map_err(|e| e.to_string())?]));
        }
        "img" => {
            if attr(el, "src").is_none() {
                return Ok(Parsed::Transparent(Vec::new()));
            }
            let attrs = json!({
                "src": attr(el, "src").unwrap_or_default(),
                "alt": attr(el, "alt"),
                "title": attr(el, "title"),
            });
            return Ok(Parsed::Inlines(vec![schema
                .node("image", attrs, Fragment::new(), MarkSet::new())
                .map_err(|e| e.to_string())?]));
        }
        "ul" | "ol" => {
            let items = parse_list_items(schema, &el.children)?;
            let name = if tag == "ol" {
                "ordered_list"
            } else {
                "bullet_list"
            };
            let mut attrs = json!({ "spread": attr(el, "data-spread").as_deref() == Some("true") });
            if tag == "ol" {
                let start = attr(el, "start")
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(1);
                attrs["start"] = json!(start);
            }
            return Ok(Parsed::Block(
                schema
                    .node(name, attrs, Fragment::from(items), MarkSet::new())
                    .map_err(|e| e.to_string())?,
            ));
        }
        "li" => {
            // A bare `li` is wrapped by the caller when it is a list child.
            // At the top level, treat it as a list of one item.
            let item = parse_list_item(schema, el)?;
            return Ok(Parsed::Block(item));
        }
        "strong" | "b" | "em" | "i" | "code" | "s" | "del" | "a" | "span" => {
            return Ok(Parsed::Inlines(parse_inlines(
                schema,
                std::slice::from_ref(node),
                &MarkSet::new(),
            )?));
        }
        _ => {}
    }
    Ok(Parsed::Transparent(el.children.clone()))
}

fn parse_list_items(
    schema: &DynamicSchema,
    children: &[DomNode],
) -> Result<Vec<DynamicNode>, String> {
    let mut items = Vec::new();
    for child in children {
        let DomNode::Element(el) = child else {
            continue;
        };
        if el.tag.eq_ignore_ascii_case("li") {
            items.push(parse_list_item(schema, el)?);
        }
    }
    Ok(items)
}

fn parse_list_item(schema: &DynamicSchema, el: &DomElement) -> Result<DynamicNode, String> {
    // Priority 60: a checkbox makes this a task item. Otherwise a regular item.
    if let Some(checked) = task_checkbox_checked(el) {
        let spread = attr(el, "data-spread").as_deref() == Some("true");
        let content_el = el.children.iter().find_map(|n| match n {
            DomNode::Element(child)
                if child
                    .attrs
                    .iter()
                    .any(|(k, v)| k == "class" && v.contains("task-list-item-content")) =>
            {
                Some(child)
            }
            _ => None,
        });
        let source = content_el
            .map(|c| c.children.as_slice())
            .unwrap_or(&el.children);
        let mut blocks = parse_blocks(schema, source)?;
        if blocks.is_empty() {
            blocks.push(
                schema
                    .node("paragraph", json!({}), Fragment::new(), MarkSet::new())
                    .map_err(|e| e.to_string())?,
            );
        }
        // Task content starts with a paragraph. Promote a leading non-paragraph
        // by leaving parse_blocks' result; callers pass paragraphs.
        return schema
            .node(
                "task_list_item",
                json!({ "checked": checked, "spread": spread }),
                Fragment::from(blocks),
                MarkSet::new(),
            )
            .map_err(|e| e.to_string());
    }
    let spread = attr(el, "data-spread").as_deref() == Some("true");
    let mut blocks = parse_blocks(schema, &el.children)?;
    if blocks.is_empty() {
        blocks.push(
            schema
                .node("paragraph", json!({}), Fragment::new(), MarkSet::new())
                .map_err(|e| e.to_string())?,
        );
    }
    schema
        .node(
            "regular_list_item",
            json!({ "spread": spread }),
            Fragment::from(blocks),
            MarkSet::new(),
        )
        .map_err(|e| e.to_string())
}

fn task_checkbox_checked(el: &DomElement) -> Option<bool> {
    for child in &el.children {
        let DomNode::Element(child) = child else {
            continue;
        };
        if child.tag.eq_ignore_ascii_case("input")
            && attr(child, "type").as_deref() == Some("checkbox")
        {
            return Some(child.attrs.iter().any(|(k, _)| k == "checked"));
        }
        if child.tag.eq_ignore_ascii_case("span") {
            for grand in &child.children {
                if let DomNode::Element(input) = grand {
                    if input.tag.eq_ignore_ascii_case("input")
                        && attr(input, "type").as_deref() == Some("checkbox")
                    {
                        return Some(input.attrs.iter().any(|(k, _)| k == "checked"));
                    }
                }
            }
        }
    }
    None
}

fn inline_block(
    schema: &DynamicSchema,
    name: &str,
    attrs: Value,
    el: &DomElement,
    marks: &MarkSet<Dyn>,
) -> Result<DynamicNode, String> {
    let mut inlines = parse_inlines(schema, &el.children, marks)?;
    if name == "paragraph" || name == "heading" {
        inlines = normalize_normal_whitespace(inlines);
    }
    schema
        .node(name, attrs, Fragment::from_array(inlines), MarkSet::new())
        .map_err(|e| e.to_string())
}

fn parse_inlines(
    schema: &DynamicSchema,
    nodes: &[DomNode],
    marks: &MarkSet<Dyn>,
) -> Result<Vec<DynamicNode>, String> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            DomNode::Text(text) => {
                if !text.is_empty() {
                    out.push(text_node(text, marks));
                }
            }
            DomNode::Element(el) => {
                let tag = el.tag.to_ascii_lowercase();
                if let Some(mark) = mark_for(schema, el, &tag) {
                    let mut next = marks.clone();
                    next.add(&mark);
                    out.extend(parse_inlines(schema, &el.children, &next)?);
                    continue;
                }
                match tag.as_str() {
                    "br" => out.push(
                        schema
                            .node("hard_break", json!({}), Fragment::new(), marks.clone())
                            .map_err(|e| e.to_string())?,
                    ),
                    "img" => {
                        if attr(el, "src").is_some() {
                            let mut image = schema
                                .node(
                                    "image",
                                    json!({
                                        "src": attr(el, "src").unwrap_or_default(),
                                        "alt": attr(el, "alt"),
                                        "title": attr(el, "title"),
                                    }),
                                    Fragment::new(),
                                    MarkSet::new(),
                                )
                                .map_err(|e| e.to_string())?;
                            image = image.mark(marks.clone());
                            out.push(image);
                        }
                    }
                    "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        out.extend(parse_inlines(schema, &el.children, marks)?);
                    }
                    _ => out.extend(parse_inlines(schema, &el.children, marks)?),
                }
            }
        }
    }
    Ok(out)
}

fn mark_for(schema: &DynamicSchema, el: &DomElement, tag: &str) -> Option<DynamicMark> {
    let name = match tag {
        "strong" | "b" => "strong",
        "em" | "i" => "em",
        "code" => "code",
        "s" | "del" => "strikethrough",
        "a" => "link",
        "span" => return mark_from_style(schema, el),
        _ => return None,
    };
    schema.mark_type(name)?;
    if name == "link" {
        let href = attr(el, "href")?;
        return Some(DynamicMark {
            type_name: "link".into(),
            attrs: json!({ "href": href, "title": attr(el, "title") }),
        });
    }
    Some(DynamicMark {
        type_name: name.into(),
        attrs: json!({}),
    })
}

fn mark_from_style(schema: &DynamicSchema, el: &DomElement) -> Option<DynamicMark> {
    let style = attr(el, "style")?;
    if let Some(weight) = style_value(&style, "font-weight") {
        if schema.mark_type("strong").is_some() && font_weight_is_bold(weight) {
            return Some(DynamicMark {
                type_name: "strong".into(),
                attrs: json!({}),
            });
        }
    }
    if let Some(font) = style_value(&style, "font-style") {
        if schema.mark_type("em").is_some() && font == "italic" {
            return Some(DynamicMark {
                type_name: "em".into(),
                attrs: json!({}),
            });
        }
    }
    if let Some(deco) = style_value(&style, "text-decoration") {
        if schema.mark_type("strikethrough").is_some() && has_line_through(deco) {
            return Some(DynamicMark {
                type_name: "strikethrough".into(),
                attrs: json!({}),
            });
        }
    }
    None
}

fn font_weight_is_bold(value: &str) -> bool {
    let value = value.trim();
    if value == "bold" || value == "bolder" {
        return true;
    }
    value.parse::<u32>().is_ok_and(|n| n >= 500)
}

fn has_line_through(value: &str) -> bool {
    value.split_whitespace().any(|part| part == "line-through")
        || value
            .split(|c: char| c.is_whitespace())
            .any(|part| part == "line-through")
}

fn style_value<'a>(style: &'a str, name: &str) -> Option<&'a str> {
    for decl in style.split(';') {
        let mut parts = decl.splitn(2, ':');
        let key = parts.next()?.trim();
        let value = parts.next()?.trim();
        if key.eq_ignore_ascii_case(name) {
            return Some(value);
        }
    }
    None
}

fn text_node(text: &str, marks: &MarkSet<Dyn>) -> DynamicNode {
    let mut node = DynamicNode::text(text);
    node = node.mark(marks.clone());
    node
}

/// Paragraph and heading `parseDOM` use normal whitespace: collapse runs, and
/// drop spaces at the edges of the block (the space GitHub puts after a checkbox).
fn normalize_normal_whitespace(nodes: Vec<DynamicNode>) -> Vec<DynamicNode> {
    let mut next = Vec::with_capacity(nodes.len());
    for node in nodes {
        if let Some(text) = node.text_node() {
            let collapsed = collapse_ws(text.text.as_str());
            if collapsed.is_empty() {
                continue;
            }
            next.push(text_node(&collapsed, &text.marks));
        } else {
            next.push(node);
        }
    }
    trim_edge_spaces(&mut next, true);
    trim_edge_spaces(&mut next, false);
    next
}

fn trim_edge_spaces(nodes: &mut Vec<DynamicNode>, leading: bool) {
    loop {
        if nodes.is_empty() {
            return;
        }
        let idx = if leading { 0 } else { nodes.len() - 1 };
        if !nodes[idx].is_text() {
            return;
        }
        let (trimmed, marks, raw_len) = {
            let text = nodes[idx].text_node().expect("text");
            let raw = text.text.as_str();
            let trimmed = if leading {
                raw.trim_start_matches(' ')
            } else {
                raw.trim_end_matches(' ')
            };
            (trimmed.to_string(), text.marks.clone(), raw.len())
        };
        if trimmed.is_empty() {
            nodes.remove(idx);
            continue;
        }
        if trimmed.len() != raw_len {
            nodes[idx] = text_node(&trimmed, &marks);
        }
        return;
    }
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            prev_space = false;
            out.push(c);
        }
    }
    out
}

fn text_of(node: &DomNode) -> String {
    match node {
        DomNode::Text(text) => text.clone(),
        DomNode::Element(el) => text_content(el),
    }
}

fn text_content(el: &DomElement) -> String {
    let mut out = String::new();
    for child in &el.children {
        match child {
            DomNode::Text(text) => out.push_str(text),
            DomNode::Element(child) => out.push_str(&text_content(child)),
        }
    }
    out
}

fn attr(el: &DomElement, name: &str) -> Option<String> {
    el.attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

fn image_attrs(node: &DynamicNode) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    if let Some(alt) = json_str(node, "alt") {
        attrs.push(("alt".into(), alt));
    }
    if let Some(src) = json_str(node, "src") {
        attrs.push(("src".into(), src));
    }
    if let Some(title) = json_str(node, "title") {
        attrs.push(("title".into(), title));
    }
    attrs
}

fn json_str(node: &DynamicNode, name: &str) -> Option<String> {
    match node.attrs.get(name)? {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn attr_bool(node: &DynamicNode, name: &str, default: bool) -> bool {
    node.attrs
        .get(name)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn attr_i64(node: &DynamicNode, name: &str, default: i64) -> i64 {
    node.attrs
        .get(name)
        .and_then(Value::as_i64)
        .unwrap_or(default)
}

fn attr_string(mark: &DynamicMark, name: &str) -> String {
    mark.attrs
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn attr_string_opt(mark: &DynamicMark, name: &str) -> Option<String> {
    match mark.attrs.get(name)? {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn bool_attr(value: bool) -> String {
    if value { "true" } else { "false" }.into()
}
