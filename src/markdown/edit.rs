//! Editing commands, input rules, and keymaps from `prosemirror-remark`.
//!
//! `prosemirror-rs` has no `EditorState` or `EditorView`. [`DocState`] is the
//! document, a selection, and stored marks. [`Editor::press`] runs the keymap
//! `prosemirror-unified` builds: extension commands in dependency order, then
//! the base keymap (`Enter`, `Backspace`, `Delete`). [`Editor::type_text`]
//! inserts text one character at a time and applies input rules, including the
//! Enter hook that feeds a synthetic newline to rules such as `---`.
//!
//! The bindings follow the extension sources:
//!
//! * `HeadingExtension` — `# `…`###### `, `#`, `Backspace`, `Tab`, `Shift-Tab`, `Shift-Mod-1`…`6`
//! * `BlockquoteExtension` — `> `, `Mod->`
//! * `CodeBlockExtension` — fence or four spaces, `Shift-Mod-\`, `Enter` lifts out
//! * `HorizontalRuleExtension` — `---`, `Mod-_`
//! * `ListItemExtension` / `listItemKeymap` — `Enter`, `Tab`, `Shift-Tab`
//! * `OrderedListExtension` / `UnorderedListExtension` — wrapping rules, `Shift-Mod-8` / `9`
//! * `TaskListItemExtension` — `[ ]` / `[x]`, `Backspace` at the start, checkbox click
//! * `BoldExtension`, `ItalicExtension`, `InlineCodeExtension`, `StrikethroughExtension`
//! * `BreakExtension` — `Mod-Enter`, `Shift-Enter`, `Ctrl-Enter`

use serde_json::{json, Value};

use crate::dynamic::types::Dyn;
use crate::dynamic::{DynamicMark, DynamicNode, DynamicSchema};
use crate::model::{ContentMatch, Fragment, Mark, MarkSet, Node, NodeType};
use crate::transform::map::Mappable;
use crate::transform::structure::{can_join, can_split, lift_target, NodeRange, Wrapper};
use crate::transform::{ReplaceAroundStep, Span, Step, Transform};

use super::dom;
use super::schema::Flavor;
use super::Remark;

/// A text selection, or a node selection when [`node`](TextSelection::node) is set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextSelection {
    /// Anchor position.
    pub anchor: usize,
    /// Head (cursor) position.
    pub head: usize,
    /// `true` when this is a `NodeSelection` covering `anchor..head`.
    pub node: bool,
}

impl TextSelection {
    /// A cursor at `pos`.
    #[must_use]
    pub fn cursor(pos: usize) -> Self {
        Self {
            anchor: pos,
            head: pos,
            node: false,
        }
    }

    /// A node selection covering `from..to`.
    #[must_use]
    pub fn node(from: usize, to: usize) -> Self {
        Self {
            anchor: from,
            head: to,
            node: true,
        }
    }

    /// Whether the selection covers no range.
    #[must_use]
    pub fn empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The smaller endpoint.
    #[must_use]
    pub fn from(&self) -> usize {
        self.anchor.min(self.head)
    }

    /// The larger endpoint.
    #[must_use]
    pub fn to(&self) -> usize {
        self.anchor.max(self.head)
    }
}

/// Document, selection, and stored marks.
#[derive(Clone, Debug)]
pub struct DocState {
    /// The document.
    pub doc: DynamicNode,
    /// The selection.
    pub selection: TextSelection,
    /// Marks that the next typed text should receive. `None` uses the marks
    /// at the cursor.
    pub stored_marks: Option<MarkSet<Dyn>>,
}

/// A key the markdown keymap understands.
///
/// Character keys go through input rules. `Mod` is Ctrl on this platform
/// (the base map is `pcBaseKeymap`). `Ctrl-Enter` is the extra macOS binding
/// from `BreakExtension`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A typed character.
    Char(char),
    /// Enter.
    Enter,
    /// Backspace.
    Backspace,
    /// Forward delete.
    Delete,
    /// Tab.
    Tab,
    /// Shift-Tab.
    ShiftTab,
    /// Mod-Enter.
    ModEnter,
    /// Shift-Enter.
    ShiftEnter,
    /// Ctrl-Enter (macOS hard-break binding).
    CtrlEnter,
    /// Mod-b / Mod-B.
    ModB,
    /// Mod-i / Mod-I.
    ModI,
    /// Ctrl-`.
    CtrlBacktick,
    /// Mod->.
    ModGreater,
    /// Mod-_.
    ModUnderscore,
    /// Shift-Mod-1 … Shift-Mod-9.
    ShiftModDigit(u8),
    /// Shift-Mod-\\.
    ShiftModBackslash,
}

/// Commands and input rules for one Markdown flavor.
pub struct Editor {
    remark: Remark,
    schema: DynamicSchema,
}

impl Editor {
    /// Editor bound to `remark`'s schema.
    #[must_use]
    pub fn new(remark: Remark) -> Self {
        Self {
            schema: remark.schema(),
            remark,
        }
    }

    /// The schema commands target.
    #[must_use]
    pub fn schema(&self) -> &DynamicSchema {
        &self.schema
    }

    /// The converter used to read a result back as Markdown.
    #[must_use]
    pub fn remark(&self) -> Remark {
        self.remark
    }

    /// Parse `markdown` and place the cursor at the start of the first textblock.
    #[must_use]
    pub fn open(&self, markdown: &str) -> DocState {
        let parsed = self.remark.parse(markdown);
        let pos = self.schema.with_types(|| first_textblock_pos(&parsed.doc));
        DocState {
            doc: parsed.doc,
            selection: TextSelection::cursor(pos),
            stored_marks: None,
        }
    }

    /// Serialize with the remark options.
    #[must_use]
    pub fn markdown(&self, state: &DocState) -> String {
        self.remark.serialize(&state.doc)
    }

    /// Mini JSON of the document.
    #[must_use]
    pub fn to_json(&self, state: &DocState) -> Value {
        self.schema.with_types(|| state.doc.to_json(true))
    }

    /// Move the cursor to the first occurrence of `needle`.
    ///
    /// Positions are resolved with this editor's schema. A missing needle
    /// leaves the selection where it was.
    #[must_use]
    pub fn cursor_before(&self, state: DocState, needle: &str) -> DocState {
        self.cursor_at(state, needle, 0)
    }

    /// Move the cursor `chars` into the first occurrence of `needle`.
    #[must_use]
    pub fn cursor_at(&self, state: DocState, needle: &str, chars: usize) -> DocState {
        self.schema.with_types(|| {
            let mut state = state;
            if let Some(pos) = find_text(&state.doc, needle) {
                state.selection = TextSelection::cursor(pos + chars);
            }
            state
        })
    }

    /// Insert `text` one character at a time.
    ///
    /// Each character goes through the keymap first (`#` changes a heading),
    /// then input rules. `\n` is Enter: rules such as `---` see a synthetic
    /// newline, then the Enter keymap runs.
    #[must_use]
    pub fn type_text(&self, state: &DocState, text: &str) -> DocState {
        let mut state = state.clone();
        for ch in text.chars() {
            let key = if ch == '\n' {
                Key::Enter
            } else {
                Key::Char(ch)
            };
            state = self.press(&state, key).unwrap_or(state);
        }
        state
    }

    /// Run the keymap binding for `key`.
    ///
    /// `None` means every command declined and the key was not a character.
    #[must_use]
    pub fn press(&self, state: &DocState, key: Key) -> Option<DocState> {
        self.schema.with_types(|| dispatch(self, state, key))
    }

    /// `TaskListItemView`'s checkbox click.
    #[must_use]
    pub fn click_task_checkbox(&self, state: &DocState, pos: usize) -> Option<DocState> {
        let doc = dom::click_task_checkbox(&self.schema, &state.doc, pos)?;
        let mut next = state.clone();
        next.doc = doc;
        Some(next)
    }
}

/// `isAtStart` without a view: an empty selection at `parentOffset == 0`.
///
/// Resolving the cursor needs the schema's type store, which [`Editor`]
/// commands already install. [`is_at_start_in`] installs it for you.
#[must_use]
pub fn is_at_start(state: &DocState) -> bool {
    if !state.selection.empty() || state.selection.node {
        return false;
    }
    state
        .doc
        .resolve(state.selection.head)
        .is_ok_and(|pos| pos.parent_offset == 0)
}

/// Same as [`is_at_start`], installing `schema`'s types first.
#[must_use]
pub fn is_at_start_in(schema: &DynamicSchema, state: &DocState) -> bool {
    schema.with_types(|| is_at_start(state))
}

/// `isListSpread`: a spread list, or a spread item that has fewer than two children.
#[must_use]
pub fn is_list_spread(spread: bool, items: &[(bool, usize)]) -> bool {
    spread
        || items
            .iter()
            .any(|(item_spread, children)| *item_spread && *children < 2)
}

/// One side of `resolveReferences`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceDefinition {
    /// Destination URL.
    pub url: String,
    /// Optional title. `None` leaves the reference title untouched.
    pub title: Option<String>,
}

/// `resolveReferences`: copy a definition's url (and title, when present) onto
/// every reference stored under that id. Unknown ids are left alone.
pub fn resolve_references(
    references: &mut std::collections::HashMap<
        String,
        Vec<std::collections::HashMap<String, Value>>,
    >,
    definitions: &std::collections::HashMap<String, ReferenceDefinition>,
    url_attribute: &str,
) {
    for (id, refs) in references.iter_mut() {
        let Some(def) = definitions.get(id) else {
            continue;
        };
        for attrs in refs {
            attrs.insert(url_attribute.to_string(), json!(def.url));
            if let Some(title) = &def.title {
                attrs.insert("title".to_string(), json!(title));
            }
        }
    }
}

/// `addMarkToNodes`.
#[must_use]
pub fn add_mark_to_nodes(nodes: &[DynamicNode], mark: &DynamicMark) -> Vec<DynamicNode> {
    nodes
        .iter()
        .map(|node| {
            let existing = node.marks().cloned().unwrap_or_default();
            let combined = mark.add_to_set(std::borrow::Cow::Borrowed(&existing));
            node.mark(combined.into_owned())
        })
        .collect()
}

/// Which micromark extensions `buildUnifiedExtension` turns on.
///
/// Parsing already applies these as pulldown-cmark options and the autolink
/// pass. This is the explicit switch list those extensions register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnifiedExtensions {
    /// `micromark-extension-gfm-strikethrough`.
    pub strikethrough: bool,
    /// `micromark-extension-gfm-task-list-item`.
    pub task_list: bool,
    /// `micromark-extension-gfm-autolink-literal`.
    pub autolink: bool,
}

/// Extensions registered for `flavor`.
#[must_use]
pub fn unified_extensions(flavor: Flavor) -> UnifiedExtensions {
    match flavor {
        Flavor::CommonMark => UnifiedExtensions {
            strikethrough: false,
            task_list: false,
            autolink: false,
        },
        Flavor::Gfm => UnifiedExtensions {
            strikethrough: true,
            task_list: true,
            autolink: true,
        },
    }
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

struct Session<'a> {
    tr: Transform<Dyn>,
    sel: TextSelection,
    stored: Option<MarkSet<Dyn>>,
    mapped_through: usize,
    schema: &'a DynamicSchema,
}

impl<'a> Session<'a> {
    fn new(schema: &'a DynamicSchema, state: &DocState) -> Self {
        Self {
            tr: Transform::new(state.doc.clone()),
            sel: state.selection.clone(),
            stored: state.stored_marks.clone(),
            mapped_through: 0,
            schema,
        }
    }

    fn sync(&mut self) {
        if self.mapped_through < self.tr.steps.len() {
            let mapping = self.tr.mapping.slice(self.mapped_through, None);
            self.sel.anchor = mapping.map(self.sel.anchor, 1);
            self.sel.head = mapping.map(self.sel.head, 1);
            self.mapped_through = self.tr.steps.len();
        }
    }

    fn doc(&mut self) -> &DynamicNode {
        self.sync();
        &self.tr.doc
    }

    fn head(&mut self) -> usize {
        self.sync();
        self.sel.head
    }

    fn from(&mut self) -> usize {
        self.sync();
        self.sel.from()
    }

    fn to(&mut self) -> usize {
        self.sync();
        self.sel.to()
    }

    fn empty(&mut self) -> bool {
        self.sync();
        self.sel.empty() && !self.sel.node
    }

    fn set_sel(&mut self, sel: TextSelection) {
        self.sync();
        self.sel = sel;
    }

    fn node_type(&self, name: &str) -> Option<crate::dynamic::DynamicNodeType> {
        self.schema.node_type(name)
    }

    fn finish(mut self) -> DocState {
        self.sync();
        let size = self.tr.doc.content_size();
        self.sel.anchor = self.sel.anchor.min(size);
        self.sel.head = self.sel.head.min(size);
        DocState {
            doc: self.tr.doc,
            selection: self.sel,
            stored_marks: self.stored,
        }
    }
}

fn dispatch(editor: &Editor, state: &DocState, key: Key) -> Option<DocState> {
    if matches!(key, Key::Enter) {
        if let Some(next) = input_newline(editor, state) {
            // The unified Enter hook dispatches the input rule, then the keymap
            // still runs against the updated state.
            return dispatch(editor, &next, Key::Enter).or(Some(next));
        }
    }
    let mut session = Session::new(&editor.schema, state);
    if run_key(&mut session, key) {
        return Some(session.finish());
    }
    if let Key::Char(ch) = key {
        return Some(insert_char(editor, state, ch));
    }
    None
}

fn run_key(s: &mut Session, key: Key) -> bool {
    match key {
        Key::Enter => {
            code_block_lift(s)
                || split_list_item(s, "regular_list_item", false)
                || lift_empty_nested(s)
                || split_list_item(s, "task_list_item", true)
                || newline_in_code(s)
                || create_paragraph_near(s)
                || lift_empty_block(s)
                || split_block(s)
        }
        Key::Backspace => {
            heading_level(s, -1, true)
                || task_backspace(s)
                || delete_selection(s)
                || delete_char(s, -1)
                || join_backward(s)
        }
        Key::Delete => delete_selection(s) || delete_char(s, 1) || join_forward(s),
        Key::Tab => heading_level(s, 1, false) || sink_any(s),
        Key::ShiftTab => heading_level(s, -1, false) || lift_any(s),
        Key::Char('#') => heading_level(s, 1, true),
        Key::ModB => toggle_mark(s, "strong"),
        Key::ModI => toggle_mark(s, "em"),
        Key::CtrlBacktick => toggle_code(s),
        Key::ModGreater => wrap_in(s, "blockquote", json!({})),
        Key::ModUnderscore => insert_rule(s),
        Key::ShiftModDigit(1..=6) => {
            let Key::ShiftModDigit(n) = key else {
                return false;
            };
            set_block(s, "heading", Some(json!({ "level": n })))
        }
        Key::ShiftModDigit(8) => wrap_in_list(s, "bullet_list", json!({})),
        Key::ShiftModDigit(9) => wrap_in_list(s, "ordered_list", json!({})),
        Key::ShiftModBackslash => set_block(s, "code_block", None),
        Key::ModEnter | Key::ShiftEnter | Key::CtrlEnter => exit_code(s) || insert_hard_break(s),
        Key::Char(_) | Key::ShiftModDigit(_) => false,
    }
}

fn insert_char(editor: &Editor, state: &DocState, ch: char) -> DocState {
    if let Some(next) = try_input_rules(editor, state, &ch.to_string()) {
        return next;
    }
    let mut session = Session::new(&editor.schema, state);
    replace_with_text(&mut session, &ch.to_string());
    session.finish()
}

fn input_newline(editor: &Editor, state: &DocState) -> Option<DocState> {
    try_input_rules(editor, state, "\n")
}

// ---------------------------------------------------------------------------
// Input rules
// ---------------------------------------------------------------------------

fn try_input_rules(editor: &Editor, state: &DocState, inserted: &str) -> Option<DocState> {
    editor.schema.with_types(|| {
        if state.selection.node || !state.selection.empty() {
            return None;
        }
        let doc = &state.doc;
        let pos = state.selection.head;
        let at_pos = doc.resolve(pos).ok()?;
        let parent = at_pos.parent();
        let in_code = parent.r#type().whitespace().as_deref() == Some("pre");
        if in_code {
            return None;
        }
        let offset = at_pos.parent_offset;
        let before =
            parent.text_between(offset.saturating_sub(500), offset, None, Some("\u{fffc}"));
        let text_before = format!("{before}{inserted}");
        let full_from_start = offset <= 500;
        let block_text = if full_from_start {
            text_before.clone()
        } else {
            String::new()
        };
        apply_rule(editor, state, inserted, &text_before, &block_text, pos)
    })
}

fn apply_rule(
    editor: &Editor,
    state: &DocState,
    inserted: &str,
    text_before: &str,
    block_text: &str,
    cursor: usize,
) -> Option<DocState> {
    let gfm = editor.remark.flavor() == Flavor::Gfm;
    // Node rules, then mark rules, in extension order.
    if let Some(hit) = match_anchored(block_text, heading_match) {
        return textblock_rule(
            editor,
            state,
            cursor,
            &hit,
            inserted,
            "heading",
            json!({ "level": hit.groups }),
        );
    }
    if let Some(hit) = match_anchored(block_text, fence_match) {
        let _ = hit;
        return textblock_rule(
            editor,
            state,
            cursor,
            &hit_of(block_text),
            inserted,
            "code_block",
            json!({}),
        );
    }
    if let Some(hit) = match_anchored(block_text, spaces_match) {
        let _ = hit;
        return textblock_rule(
            editor,
            state,
            cursor,
            &hit_of(block_text),
            inserted,
            "code_block",
            json!({}),
        );
    }
    if block_text.len() == text_before.len() {
        if let Some(rule_match) = rule_match(block_text) {
            let start = cursor - (rule_match - inserted.chars().count());
            return Some(replace_with_node(
                editor,
                state,
                start,
                cursor,
                "horizontal_rule",
            ));
        }
    }
    if let Some(hit) = match_anchored(block_text, quote_match) {
        let _ = hit;
        return wrapping_rule(
            editor,
            state,
            cursor,
            &hit_of(block_text),
            inserted,
            "blockquote",
            json!({}),
            None::<fn(&DynamicNode) -> bool>,
        );
    }
    if let Some((bullet_len,)) = bullet_match(block_text) {
        let _ = bullet_len;
        return wrapping_rule(
            editor,
            state,
            cursor,
            &hit_of(block_text),
            inserted,
            "bullet_list",
            json!({}),
            None::<fn(&DynamicNode) -> bool>,
        );
    }
    if let Some(start_num) = ordered_match(block_text) {
        let attrs = json!({ "start": start_num });
        let predicate = move |node: &DynamicNode| {
            let child_count = node.child_count() as i64;
            let start = node.attrs.get("start").and_then(Value::as_i64).unwrap_or(1);
            child_count + start == start_num
        };
        return wrapping_rule(
            editor,
            state,
            cursor,
            &hit_of(block_text),
            inserted,
            "ordered_list",
            attrs,
            Some(predicate),
        );
    }
    if gfm {
        if let Some(checked) = task_match(block_text) {
            return task_rule(
                editor,
                state,
                cursor,
                inserted,
                block_text.chars().count(),
                checked,
            );
        }
    }
    // Marks. `text_before` is a suffix; the match must end at the cursor.
    let mark_rules: &[(&str, MarkMatcher)] = &[
        ("code", code_mark_match),
        ("em", em_star_match),
        ("em", em_under_match),
        ("strong", strong_star_match),
        ("strong", strong_under_match),
    ];
    for (name, matcher) in mark_rules {
        if let Some(hit) = matcher(text_before) {
            if hit.full_chars >= inserted.chars().count() {
                return mark_rule(editor, state, cursor, inserted, &hit, name);
            }
        }
    }
    if gfm {
        for matcher in [strike_single_match, strike_double_match] {
            if let Some(hit) = matcher(text_before) {
                if hit.full_chars >= inserted.chars().count() {
                    return mark_rule(editor, state, cursor, inserted, &hit, "strikethrough");
                }
            }
        }
    }
    None
}

struct Anchored {
    chars: usize,
    groups: i64,
}

fn hit_of(text: &str) -> Anchored {
    Anchored {
        chars: text.chars().count(),
        groups: 0,
    }
}

fn match_anchored(text: &str, f: impl Fn(&str) -> Option<i64>) -> Option<Anchored> {
    let groups = f(text)?;
    Some(Anchored {
        chars: text.chars().count(),
        groups,
    })
}

fn heading_match(text: &str) -> Option<i64> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut spaces = 0;
    while i < chars.len() && chars[i] == ' ' && spaces < 3 {
        spaces += 1;
        i += 1;
    }
    let start = i;
    while i < chars.len() && chars[i] == '#' {
        i += 1;
    }
    let hashes = i - start;
    if !(1..=6).contains(&hashes) || i >= chars.len() || chars[i] != ' ' || i + 1 != chars.len() {
        return None;
    }
    Some(hashes as i64)
}

fn fence_match(text: &str) -> Option<i64> {
    let t = text.trim_start_matches(' ');
    let leading = text.len() - t.len();
    if leading <= 3 && t == "```" {
        Some(0)
    } else {
        None
    }
}

fn spaces_match(text: &str) -> Option<i64> {
    if text == "    " {
        Some(0)
    } else {
        None
    }
}

fn quote_match(text: &str) -> Option<i64> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() && chars[i] == ' ' && i < 3 {
        i += 1;
    }
    if i < chars.len() && chars[i] == '>' && i + 2 == chars.len() && chars[i + 1] == ' ' {
        Some(0)
    } else {
        None
    }
}

fn bullet_match(text: &str) -> Option<(usize,)> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() && chars[i] == ' ' && i < 3 {
        i += 1;
    }
    if i < chars.len()
        && matches!(chars[i], '-' | '+' | '*')
        && i + 2 == chars.len()
        && chars[i + 1] == ' '
    {
        Some((i + 2,))
    } else {
        None
    }
}

fn ordered_match(text: &str) -> Option<i64> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() && chars[i] == ' ' && i < 3 {
        i += 1;
    }
    let start = i;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i == start || i + 2 != chars.len() || chars[i] != '.' || chars[i + 1] != ' ' {
        return None;
    }
    chars[start..i].iter().collect::<String>().parse().ok()
}

fn rule_match(text: &str) -> Option<usize> {
    let chars: Vec<char> = text.chars().collect();
    if chars.last() != Some(&'\n') {
        return None;
    }
    let body: String = chars[..chars.len() - 1].iter().collect();
    let trimmed = body.trim_start_matches(' ');
    let leading = body.len() - trimmed.len();
    if leading <= 3 && matches!(trimmed, "***" | "---" | "___") {
        Some(chars.len())
    } else {
        None
    }
}

fn task_match(text: &str) -> Option<bool> {
    let chars: Vec<char> = text.chars().collect();
    if chars.first() != Some(&'[') || chars.len() < 3 {
        return None;
    }
    // `^\[([xX\s]?)\][\s\S]$`
    let close = chars.iter().position(|c| *c == ']')?;
    if close + 2 != chars.len() {
        return None;
    }
    let inside: String = chars[1..close].iter().collect();
    if inside.is_empty() || inside == " " || inside == "x" || inside == "X" {
        Some(inside.eq_ignore_ascii_case("x"))
    } else {
        None
    }
}

struct MarkHit {
    content: String,
    trailing: String,
    full_chars: usize,
}

type MarkMatcher = fn(&str) -> Option<MarkHit>;

fn mark_rule(
    editor: &Editor,
    state: &DocState,
    cursor: usize,
    inserted: &str,
    hit: &MarkHit,
    mark_name: &str,
) -> Option<DocState> {
    let start = cursor - (hit.full_chars - inserted.chars().count());
    let end = cursor;
    let mut session = Session::new(&editor.schema, state);
    let doc = session.doc().clone();
    let at_start = doc.resolve(start).ok()?;
    let parent = at_start.parent();
    if !parent
        .r#type()
        .allows_mark_type(editor.schema.mark_type(mark_name)?)
    {
        return None;
    }
    let existing = doc
        .node_at(start)
        .and_then(|n| n.marks())
        .cloned()
        .unwrap_or_default();
    let mut mark_names: Vec<String> = existing.iter().map(|m| m.type_name.clone()).collect();
    mark_names.push(mark_name.to_string());
    let text = DynamicNode::text(&hit.content);
    session
        .tr
        .replace_with(start, end, Fragment::from(vec![text]));
    session.sync();
    let from = session.tr.mapping.map(start, 1);
    let to = session.tr.mapping.map(end, 1);
    for name in &mark_names {
        let mark = DynamicMark {
            type_name: name.clone(),
            attrs: json!({}),
        };
        session.tr.add_mark(from, to, mark);
    }
    session.sync();
    let mut stored = marks_at(&session.tr.doc, session.sel.head);
    for name in &mark_names {
        stored = remove_mark_name(stored, name);
    }
    session.stored = Some(stored);
    if !hit.trailing.is_empty() && hit.trailing != "\n" {
        replace_with_text(&mut session, &hit.trailing);
    }
    Some(session.finish())
}

fn matched_start(cursor: usize, match_chars: usize, inserted: &str) -> usize {
    cursor - (match_chars - inserted.chars().count())
}

/// `block_range` returns the model range. Lift and wrap take the transform one.
fn to_structure<'a>(range: crate::model::NodeRange<'a, Dyn>) -> NodeRange<'a, Dyn> {
    NodeRange::new(range.from, range.to, range.depth)
}

fn can_split_here(doc: &DynamicNode, pos: usize, depth: Option<usize>) -> bool {
    can_split::<Dyn>(doc, pos, depth, None::<&[crate::dynamic::DynamicNodeType]>)
}

fn textblock_rule(
    editor: &Editor,
    state: &DocState,
    cursor: usize,
    hit: &Anchored,
    inserted: &str,
    node_name: &str,
    attrs: Value,
) -> Option<DocState> {
    let start = matched_start(cursor, hit.chars, inserted);
    let doc = &state.doc;
    let at_start = doc.resolve(start).ok()?;
    let ty = editor.schema.node_type(node_name)?;
    let parent = at_start.node(at_start.depth.saturating_sub(1));
    let index = at_start.index(at_start.depth.saturating_sub(1));
    if at_start.depth == 0 || !parent.can_replace_with(index, index + 1, ty) {
        return None;
    }
    let mut session = Session::new(&editor.schema, state);
    session.tr.delete(start, cursor);
    session.tr.set_block_type(start, start, ty, Some(attrs));
    if session.tr.doc_changed() {
        Some(session.finish())
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn wrapping_rule(
    editor: &Editor,
    state: &DocState,
    cursor: usize,
    hit: &Anchored,
    inserted: &str,
    node_name: &str,
    attrs: Value,
    join_if: Option<impl Fn(&DynamicNode) -> bool>,
) -> Option<DocState> {
    let start = matched_start(cursor, hit.chars, inserted);
    let ty = editor.schema.node_type(node_name)?;
    let mut session = Session::new(&editor.schema, state);
    session.tr.delete(start, cursor);
    session.sync();
    let doc = session.tr.doc.clone();
    let at_start = doc.resolve(start).ok()?;
    let range = to_structure(at_start.block_range(None, None)?);
    let mut wrapping = crate::transform::structure::find_wrapping(&range, ty, |_| true)?;
    for wrap in &mut wrapping {
        if wrap.node_type == ty {
            wrap.attrs = attrs.clone();
        }
    }
    let start_pos = range.start();
    let end_pos = range.end();
    drop(range);
    apply_wrap(&mut session, start_pos, end_pos, &wrapping)?;
    session.sync();
    if start > 0 {
        let doc = session.tr.doc.clone();
        if let Ok(before_pos) = doc.resolve(start - 1) {
            if let Some(before) = before_pos.node_before() {
                let same = before.r#type() == ty;
                let join_ok = join_if.as_ref().is_none_or(|pred| pred(before.as_ref()));
                if same && join_ok && can_join::<Dyn>(&doc, start - 1) == Some(true) {
                    session.tr.join(start - 1, None);
                }
            }
        }
    }
    Some(session.finish())
}

fn task_rule(
    editor: &Editor,
    state: &DocState,
    cursor: usize,
    inserted: &str,
    chars: usize,
    checked: bool,
) -> Option<DocState> {
    let start = matched_start(cursor, chars, inserted);
    let doc = &state.doc;
    let at_start = doc.resolve(start).ok()?;
    if at_start.depth < 2 {
        return None;
    }
    let item = at_start.node(at_start.depth - 1);
    if item.type_name != "regular_list_item"
        || at_start.parent().type_name != "paragraph"
        || at_start.index(at_start.depth - 1) != 0
    {
        return None;
    }
    let spread = item
        .attrs
        .get("spread")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let item_pos = at_start.before(at_start.depth - 1)?;
    let ty = editor.schema.node_type("task_list_item")?;
    let mut session = Session::new(&editor.schema, state);
    session.tr.delete(start, cursor);
    session.tr.set_node_markup(
        item_pos,
        Some(ty),
        Some(json!({ "checked": checked, "spread": spread })),
        None,
    );
    Some(session.finish())
}

fn replace_with_node(
    editor: &Editor,
    state: &DocState,
    start: usize,
    end: usize,
    name: &str,
) -> DocState {
    let mut session = Session::new(&editor.schema, state);
    let node = session
        .node_type(name)
        .unwrap()
        .create(json!({}), None, None);
    session
        .tr
        .replace_with(start, end, Fragment::from(vec![node.clone()]));
    session.sync();
    // replaceWith of a leaf block leaves it node-selected (see the HR rule).
    if let Some((from, to)) = find_node_covering(&session.tr.doc, name, start) {
        session.set_sel(TextSelection::node(from, to));
    }
    session.finish()
}

fn find_node_covering(doc: &DynamicNode, name: &str, near: usize) -> Option<(usize, usize)> {
    let mut found = None;
    if let Some(content) = doc.content() {
        content.nodes_between(
            0,
            content.size(),
            &mut |node, pos, _, _| {
                if node.type_name == name {
                    let end = pos + node.node_size();
                    if pos <= near + 2 && end >= near.saturating_sub(2) {
                        found = Some((pos, end));
                    }
                    return false;
                }
                true
            },
            0,
            None,
        );
    }
    found
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn heading_level(s: &mut Session, delta: i64, only_at_start: bool) -> bool {
    if only_at_start && !cursor_at_start(s) {
        return false;
    }
    let doc = s.doc().clone();
    let Ok(pos) = doc.resolve(s.head()) else {
        return false;
    };
    if pos.parent().type_name != "heading" {
        return false;
    }
    let level = attr_i64(pos.parent(), "level", 1) + delta;
    if !(0..=6).contains(&level) {
        return false;
    }
    let Some(at) = pos.before(pos.depth) else {
        return false;
    };
    if level == 0 {
        let Some(ty) = s.node_type("paragraph") else {
            return false;
        };
        s.tr.set_node_markup(at, Some(ty), Some(json!({})), None);
    } else {
        s.tr.set_node_attribute(at, "level", json!(level));
    }
    s.tr.doc_changed()
}

fn cursor_at_start(s: &mut Session) -> bool {
    if !s.empty() {
        return false;
    }
    let doc = s.doc().clone();
    let head = s.head();
    doc.resolve(head).is_ok_and(|p| p.parent_offset == 0)
}

fn task_backspace(s: &mut Session) -> bool {
    if !cursor_at_start(s) {
        return false;
    }
    let doc = s.doc().clone();
    let Ok(pos) = doc.resolve(s.head()) else {
        return false;
    };
    if pos.depth < 1 {
        return false;
    }
    let item = pos.node(pos.depth - 1);
    if item.type_name != "task_list_item" || pos.index(pos.depth - 1) != 0 {
        return false;
    }
    let Some(at) = pos.before(pos.depth - 1) else {
        return false;
    };
    let spread = item
        .attrs
        .get("spread")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(ty) = s.node_type("regular_list_item") else {
        return false;
    };
    s.tr.set_node_markup(at, Some(ty), Some(json!({ "spread": spread })), None);
    s.tr.doc_changed()
}

fn toggle_mark(s: &mut Session, name: &str) -> bool {
    let Some(ty) = s.schema.mark_type(name) else {
        return false;
    };
    let mark = DynamicMark {
        type_name: name.into(),
        attrs: json!({}),
    };
    if s.empty() {
        let doc = s.doc().clone();
        let head = s.head();
        let current = if let Some(stored) = s.stored.clone() {
            stored
        } else {
            doc.resolve(head)
                .map(|p| marks_from_resolved(&p))
                .unwrap_or_default()
        };
        let mut next = current.clone();
        if current.iter().any(|m| m.r#type() == ty) {
            next = remove_mark_name(next, name);
        } else {
            next.add(&mark);
        }
        s.stored = Some(next);
        return true;
    }
    let from = s.from();
    let to = s.to();
    let doc = s.doc().clone();
    if doc.range_has_mark(from, to, ty) {
        s.tr.remove_mark(from, to, Some(crate::transform::MarkOrType::MarkType(ty)));
    } else {
        s.tr.add_mark(from, to, mark);
    }
    true
}

fn toggle_code(s: &mut Session) -> bool {
    if !toggle_mark(s, "code") {
        return false;
    }
    let Some(ty) = s.schema.mark_type("code") else {
        return true;
    };
    s.sync();
    let from = s.sel.from();
    let to = s.sel.to();
    let doc = s.tr.doc.clone();
    let mut removals = Vec::new();
    if let Some(content) = doc.content() {
        content.nodes_between(
            from,
            to,
            &mut |node, pos, _, _| {
                if node.is_inline()
                    && !node.is_text()
                    && node
                        .marks()
                        .is_some_and(|m| m.iter().any(|mk| mk.r#type() == ty))
                {
                    removals.push(pos);
                }
                true
            },
            0,
            None,
        );
    }
    for pos in removals {
        s.tr.remove_node_mark(pos, crate::transform::MarkOrType::MarkType(ty));
    }
    true
}

fn set_block(s: &mut Session, name: &str, attrs: Option<Value>) -> bool {
    let Some(ty) = s.node_type(name) else {
        return false;
    };
    let from = s.from();
    let to = s.to();
    s.tr.set_block_type(from, to.max(from), ty, attrs);
    s.tr.doc_changed()
}

fn wrap_in(s: &mut Session, name: &str, attrs: Value) -> bool {
    let Some(ty) = s.node_type(name) else {
        return false;
    };
    let doc = s.doc().clone();
    let Ok(at_from) = doc.resolve(s.from()) else {
        return false;
    };
    let Ok(at_to) = doc.resolve(s.to()) else {
        return false;
    };
    let Some(range) = at_from.block_range(Some(&at_to), None).map(to_structure) else {
        return false;
    };
    let Some(mut wrapping) = crate::transform::structure::find_wrapping(&range, ty, |_| true)
    else {
        return false;
    };
    for wrap in &mut wrapping {
        if wrap.node_type == ty {
            wrap.attrs = attrs.clone();
        }
    }
    let start = range.start();
    let end = range.end();
    drop(range);
    apply_wrap(s, start, end, &wrapping).is_some()
}

fn wrap_in_list(s: &mut Session, name: &str, attrs: Value) -> bool {
    // `wrapInList` also joins an item into the previous list. The plain wrap
    // covers Shift-Mod-8/9 on a top-level paragraph, which is the binding.
    wrap_in(s, name, attrs)
}

fn insert_rule(s: &mut Session) -> bool {
    let Some(ty) = s.node_type("horizontal_rule") else {
        return false;
    };
    let node = ty.create(json!({}), None, None);
    let from = s.from();
    let to = s.to();
    s.tr.replace_with(from, to, Fragment::from(vec![node]));
    true
}

fn insert_hard_break(s: &mut Session) -> bool {
    let Some(ty) = s.node_type("hard_break") else {
        return false;
    };
    let node = ty.create(json!({}), None, None);
    let from = s.from();
    let to = s.to();
    s.tr.replace_with(from, to, Fragment::from(vec![node]));
    s.tr.doc_changed()
}

fn exit_code(s: &mut Session) -> bool {
    let doc = s.doc().clone();
    let Ok(at_head) = doc.resolve(s.head()) else {
        return false;
    };
    let Ok(at_anchor) = doc.resolve(s.from().min(s.to())) else {
        return false;
    };
    if at_head.parent().r#type().whitespace().as_deref() != Some("pre")
        || !at_head.same_parent(&at_anchor)
    {
        return false;
    }
    if at_head.depth == 0 {
        return false;
    }
    let above = at_head.node(at_head.depth - 1);
    let after = at_head.index_after(at_head.depth - 1);
    let Ok(match_) = above.content_match_at(after) else {
        return false;
    };
    let Some(ty) = default_textblock(&match_) else {
        return false;
    };
    if !above.can_replace_with(after, after, ty) {
        return false;
    }
    let Some(pos) = at_head.after(at_head.depth) else {
        return false;
    };
    let Some(node) = ty.create_and_fill(json!({}), None, None) else {
        return false;
    };
    s.tr.insert(pos, Fragment::from(vec![node]));
    s.set_sel(TextSelection::cursor(pos + 1));
    true
}

fn newline_in_code(s: &mut Session) -> bool {
    let doc = s.doc().clone();
    let Ok(at_head) = doc.resolve(s.head()) else {
        return false;
    };
    if at_head.parent().r#type().whitespace().as_deref() != Some("pre") {
        return false;
    }
    if !s.empty() {
        let from = s.from();
        let to = s.to();
        s.tr.delete(from, to);
        s.sync();
    }
    replace_with_text(s, "\n");
    true
}

fn code_block_lift(s: &mut Session) -> bool {
    let doc = s.doc().clone();
    let Ok(at_from) = doc.resolve(s.head()) else {
        return false;
    };
    let Ok(at_to) = doc.resolve(s.to()) else {
        return false;
    };
    if !at_from.same_parent(&at_to) || at_from.parent().type_name != "code_block" {
        return false;
    }
    if at_from.parent_offset != at_from.parent().content_size() {
        return false;
    }
    let text = at_from.parent().text_content();
    if !text.ends_with("\n\n") {
        return false;
    }
    let pos = at_from.pos;
    s.tr.delete(pos - 2, pos);
    let Some(ty) = s.node_type("paragraph") else {
        return false;
    };
    let Some(para) = ty.create_and_fill(json!({}), None, None) else {
        return false;
    };
    s.tr.insert(pos - 1, Fragment::from(vec![para]));
    s.set_sel(TextSelection::cursor(pos));
    true
}

fn create_paragraph_near(s: &mut Session) -> bool {
    if s.empty() {
        return false;
    }
    let doc = s.doc().clone();
    let Ok(at_from) = doc.resolve(s.from()) else {
        return false;
    };
    let Ok(at_to) = doc.resolve(s.to()) else {
        return false;
    };
    if at_from.parent().inline_content() || at_to.parent().inline_content() {
        return false;
    }
    let after = at_to.index_after(at_to.depth);
    let Ok(match_) = at_to.parent().content_match_at(after) else {
        return false;
    };
    let Some(ty) = default_textblock(&match_) else {
        return false;
    };
    let side =
        if at_from.parent_offset == 0 && at_to.index(at_to.depth) < at_to.parent().child_count() {
            at_from.pos
        } else {
            at_to.pos
        };
    let Some(node) = ty.create_and_fill(json!({}), None, None) else {
        return false;
    };
    s.tr.insert(side, Fragment::from(vec![node]));
    s.set_sel(TextSelection::cursor(side + 1));
    true
}

fn lift_empty_block(s: &mut Session) -> bool {
    if !s.empty() {
        return false;
    }
    let doc = s.doc().clone();
    let Ok(at_cursor) = doc.resolve(s.head()) else {
        return false;
    };
    if at_cursor.parent().content_size() > 0 {
        return false;
    }
    if at_cursor.depth > 1 {
        let after = at_cursor.after(at_cursor.depth);
        let end = at_cursor.end(at_cursor.depth - 1);
        if after.is_some_and(|a| a != end) {
            if let Some(before) = at_cursor.before(at_cursor.depth) {
                if can_split_here(&doc, before, None) {
                    let _ = s.tr.split(before, None, None);
                    return true;
                }
            }
        }
    }
    let Some(range) = at_cursor.block_range(None, None).map(to_structure) else {
        return false;
    };
    let Some(target) = lift_target(&range) else {
        return false;
    };
    let depth = range.depth;
    let start_pos = at_cursor.pos;
    drop(range);
    lift_depth(s, start_pos, start_pos, depth, target)
}

fn split_block(s: &mut Session) -> bool {
    let doc = s.doc().clone();
    let Ok(at_from) = doc.resolve(s.head()) else {
        return false;
    };
    if at_from.depth == 0 || !at_from.parent().is_textblock() {
        return false;
    }
    if !s.empty() {
        let from = s.from();
        let to = s.to();
        s.tr.delete(from, to);
        s.sync();
    }
    let pos = s.head();
    if !can_split_here(s.doc(), pos, Some(1)) {
        return false;
    }
    s.tr.split(pos, Some(1), None).is_ok()
}

fn delete_selection(s: &mut Session) -> bool {
    if s.empty() {
        return false;
    }
    let from = s.from();
    let to = s.to();
    s.tr.delete(from, to);
    s.sel.node = false;
    s.tr.doc_changed()
}

fn delete_char(s: &mut Session, dir: i32) -> bool {
    if !s.empty() {
        return false;
    }
    let doc = s.doc().clone();
    let head = s.head();
    let Ok(pos) = doc.resolve(head) else {
        return false;
    };
    if !pos.parent().inline_content() {
        return false;
    }
    let (from, to) = if dir < 0 {
        if pos.parent_offset == 0 {
            return false;
        }
        let len = prev_unit(&pos);
        (head - len, head)
    } else {
        if pos.parent_offset >= pos.parent().content_size() {
            return false;
        }
        let len = next_unit(&pos);
        (head, head + len)
    };
    s.tr.delete(from, to);
    s.tr.doc_changed()
}

fn join_backward(s: &mut Session) -> bool {
    join_dir(s, -1)
}

fn join_forward(s: &mut Session) -> bool {
    join_dir(s, 1)
}

fn join_dir(s: &mut Session, dir: i32) -> bool {
    if !s.empty() {
        return false;
    }
    let doc = s.doc().clone();
    let head = s.head();
    let Ok(at_cursor) = doc.resolve(head) else {
        return false;
    };
    if !at_cursor.parent().is_textblock() {
        return false;
    }
    let at_edge = if dir < 0 {
        at_cursor.parent_offset == 0
    } else {
        at_cursor.parent_offset >= at_cursor.parent().content_size()
    };
    if !at_edge {
        return false;
    }
    let cut = if dir < 0 {
        find_cut_before(&at_cursor)
    } else {
        find_cut_after(&at_cursor)
    };
    let Some(cut_pos) = cut else {
        return dir < 0 && lift_cursor_block(s);
    };
    let Ok(at_cut) = doc.resolve(cut_pos) else {
        return false;
    };
    let Some(before) = at_cut.node_before() else {
        return false;
    };
    let Some(after) = at_cut.node_after() else {
        return false;
    };
    if before.r#type().is_isolating() || after.r#type().is_isolating() {
        return false;
    }
    let index = at_cut.index(at_cut.depth);
    if can_append(before.as_ref(), after.as_ref())
        && at_cut
            .parent()
            .can_replace(index, index + 1, None, ..)
            .unwrap_or(false)
    {
        s.tr.join(cut_pos, None);
        return s.tr.doc_changed();
    }
    false
}

fn lift_cursor_block(s: &mut Session) -> bool {
    let doc = s.doc().clone();
    let head = s.head();
    let Ok(at_cursor) = doc.resolve(head) else {
        return false;
    };
    let Some(range) = at_cursor.block_range(None, None).map(to_structure) else {
        return false;
    };
    let Some(target) = lift_target(&range) else {
        return false;
    };
    let depth = range.depth;
    drop(range);
    lift_depth(s, head, head, depth, target)
}

fn can_append(a: &DynamicNode, b: &DynamicNode) -> bool {
    if a.is_leaf() || b.is_leaf() {
        return false;
    }
    let Ok(mut matched) = a.content_match_at(a.child_count()) else {
        return false;
    };
    let Some(content) = b.content() else {
        return false;
    };
    for child in content.children() {
        let Some(next) = matched.match_type(child.r#type()) else {
            return false;
        };
        matched = next;
    }
    matched.valid_end()
}

fn find_cut_before(pos: &crate::model::ResolvedPos<'_, Dyn>) -> Option<usize> {
    if pos.parent().r#type().is_isolating() {
        return None;
    }
    for i in (0..pos.depth).rev() {
        if pos.index(i) > 0 {
            return pos.before(i + 1);
        }
        if pos.node(i).r#type().is_isolating() {
            break;
        }
    }
    None
}

fn find_cut_after(pos: &crate::model::ResolvedPos<'_, Dyn>) -> Option<usize> {
    if pos.parent().r#type().is_isolating() {
        return None;
    }
    for i in (0..pos.depth).rev() {
        if pos.index(i) + 1 < pos.node(i).child_count() {
            return pos.after(i + 1);
        }
        if pos.node(i).r#type().is_isolating() {
            break;
        }
    }
    None
}

fn split_list_item(s: &mut Session, item_name: &str, reset_checked: bool) -> bool {
    if s.sel.node {
        return false;
    }
    let doc = s.doc().clone();
    let from = s.from();
    let to = s.to();
    let Ok(at_from) = doc.resolve(from) else {
        return false;
    };
    let Ok(at_to) = doc.resolve(to) else {
        return false;
    };
    if at_from.depth < 2 || !at_from.same_parent(&at_to) {
        return false;
    }
    let grand = at_from.node(at_from.depth - 1);
    if grand.type_name != item_name {
        return false;
    }
    if at_from.parent().content_size() == 0
        && grand.child_count() == at_from.index_after(at_from.depth - 1)
    {
        return false;
    }
    if from != to {
        s.tr.delete(from, to);
        s.sync();
    }
    let pos = s.head();
    if !can_split_here(s.doc(), pos, Some(2)) {
        return false;
    }
    if s.tr.split(pos, Some(2), None).is_err() {
        return false;
    }
    s.sync();
    if reset_checked {
        let doc = s.tr.doc.clone();
        if let Ok(at_head) = doc.resolve(s.sel.head) {
            for depth in (1..=at_head.depth).rev() {
                let node = at_head.node(depth);
                if node.type_name == item_name {
                    if let Some(at) = at_head.before(depth) {
                        let mut attrs = node.attrs.clone();
                        if let Value::Object(map) = &mut attrs {
                            map.insert("checked".into(), json!(false));
                        }
                        s.tr.set_node_markup(at, None, Some(attrs), None);
                    }
                    break;
                }
            }
        }
    }
    true
}

fn lift_empty_nested(s: &mut Session) -> bool {
    if !s.empty() {
        return false;
    }
    let doc = s.doc().clone();
    let Ok(at_from) = doc.resolve(s.head()) else {
        return false;
    };
    if at_from.depth < 3
        || at_from.parent().content_size() > 0
        || !is_list_item(at_from.node(at_from.depth - 1))
    {
        return false;
    }
    let item = at_from.node(at_from.depth - 1);
    if at_from.index_after(at_from.depth - 1) < item.child_count() {
        return false;
    }
    if !is_list_item(at_from.node(at_from.depth - 3)) {
        return false;
    }
    let nested = at_from.node(at_from.depth - 2);
    if at_from.index_after(at_from.depth - 2) < nested.child_count() {
        return false;
    }
    lift_any(s)
}

struct ItemRange {
    start: usize,
    end: usize,
    depth: usize,
    start_index: usize,
    end_index: usize,
    from_pos: usize,
}

fn list_item_range(s: &mut Session) -> Option<ItemRange> {
    let doc = s.doc().clone();
    let at_from = doc.resolve(s.from()).ok()?;
    let at_to = doc.resolve(s.to()).ok()?;
    let range = at_from.block_range(
        Some(&at_to),
        Some(&|node: &DynamicNode| node.first_child().is_some_and(is_list_item)),
    )?;
    Some(ItemRange {
        start: range.start(),
        end: range.end(),
        depth: range.depth,
        start_index: range.start_index(),
        end_index: range.end_index(),
        from_pos: at_from.pos,
    })
}

fn lift_any(s: &mut Session) -> bool {
    let Some(range) = list_item_range(s) else {
        return false;
    };
    let doc = s.doc().clone();
    let Ok(at_from) = doc.resolve(range.from_pos) else {
        return false;
    };
    if range.depth == 0 {
        return false;
    }
    let outside = at_from.node(range.depth - 1);
    if !is_list_item(outside) {
        return lift_out(s, &range);
    }
    lift_to_outer(s, &range)
}

fn sink_any(s: &mut Session) -> bool {
    let Some(range) = list_item_range(s) else {
        return false;
    };
    if range.start_index == 0 {
        return false;
    }
    let doc = s.doc().clone();
    let Ok(at_from) = doc.resolve(range.from_pos) else {
        return false;
    };
    let parent = at_from.node(range.depth);
    let Some(node_before) = parent.child(range.start_index - 1) else {
        return false;
    };
    let nested = node_before
        .last_child()
        .filter(|n| n.r#type() == parent.r#type())
        .and_then(|list| list.last_child());
    let open_start: usize = if nested.is_some() { 3 } else { 1 };
    let inner = if let Some(nested) = nested {
        Fragment::from(vec![nested.r#type().create(json!({}), None, None)])
    } else {
        Fragment::new()
    };
    let list = parent.r#type().create(json!({}), Some(&inner), None);
    let item = node_before
        .r#type()
        .create(json!({}), Some(&Fragment::from(vec![list])), None);
    let slice = crate::model::Slice::new(Fragment::from(vec![item]), open_start, 0);
    let step = Step::ReplaceAround(ReplaceAroundStep {
        span: Span {
            from: range.start - open_start,
            to: range.end,
        },
        gap_from: range.start,
        gap_to: range.end,
        slice,
        insert: 1,
        structure: true,
    });
    s.tr.step(step).is_ok()
}

fn lift_to_outer(s: &mut Session, range: &ItemRange) -> bool {
    let doc = s.doc().clone();
    let Ok(at_to) = doc.resolve(s.to()) else {
        return false;
    };
    let end_of_list = at_to.end(range.depth);
    if range.end < end_of_list {
        let Ok(at_from) = doc.resolve(range.from_pos) else {
            return false;
        };
        let parent = at_from.node(range.depth);
        let Some(last_item) = parent.child(range.end_index - 1) else {
            return false;
        };
        let wrapped = last_item.copy(|_| Fragment::from(vec![parent.copy(|_| Fragment::new())]));
        let slice = crate::model::Slice::new(Fragment::from(vec![wrapped]), 1, 0);
        let step = Step::ReplaceAround(ReplaceAroundStep {
            span: Span {
                from: range.end - 1,
                to: end_of_list,
            },
            gap_from: range.end,
            gap_to: end_of_list,
            slice,
            insert: 1,
            structure: true,
        });
        if s.tr.step(step).is_err() {
            return false;
        }
    }
    s.sync();
    let doc = s.tr.doc.clone();
    let from_pos = range.from_pos;
    let depth = range.depth;
    let end = if range.end < end_of_list {
        s.tr.mapping.map(range.end, -1)
    } else {
        end_of_list
    };
    let Ok(at_from) = doc.resolve(from_pos) else {
        return false;
    };
    let Ok(at_end) = doc.resolve(end.min(doc.content_size())) else {
        return false;
    };
    let struct_range = NodeRange::new(at_from, at_end, depth);
    let Some(target) = lift_target(&struct_range) else {
        return false;
    };
    drop(struct_range);
    if !lift_depth(s, from_pos, end, depth, target) {
        return false;
    }
    s.sync();
    let mapped_end = s.tr.mapping.map(range.end, -1);
    if mapped_end > 0 {
        let doc = s.tr.doc.clone();
        let join_at = mapped_end - 1;
        if can_join::<Dyn>(&doc, join_at) == Some(true) {
            if let Ok(at_after) = doc.resolve(join_at) {
                let same = at_after
                    .node_before()
                    .zip(at_after.node_after())
                    .is_some_and(|(a, b)| a.r#type() == b.r#type());
                if same {
                    s.tr.join(join_at, None);
                }
            }
        }
    }
    true
}

fn lift_out(s: &mut Session, range: &ItemRange) -> bool {
    let doc = s.doc().clone();
    let Ok(at_from) = doc.resolve(range.from_pos) else {
        return false;
    };
    let list = at_from.node(range.depth).clone();
    let mut pos = range.end;
    for i in (range.start_index + 1..range.end_index).rev() {
        let Some(child) = list.child(i) else {
            return false;
        };
        pos -= child.node_size();
        s.tr.delete(pos - 1, pos + 1);
    }
    s.sync();
    let doc = s.tr.doc.clone();
    let Ok(at_item) = doc.resolve(range.start) else {
        return false;
    };
    let Some(item) = at_item.node_after() else {
        return false;
    };
    let at_beginning = range.start_index == 0;
    let at_end = range.end_index == list.child_count();
    let start = at_item.pos;
    let end = start + item.node_size();
    let empty_list = list.copy(|_| Fragment::new());
    let mut frag_nodes = Vec::new();
    if !at_beginning {
        frag_nodes.push(empty_list.clone());
    }
    if !at_end {
        frag_nodes.push(empty_list);
    }
    let slice = crate::model::Slice::new(
        Fragment::from(frag_nodes),
        if at_beginning { 0 } else { 1 },
        if at_end { 0 } else { 1 },
    );
    let step = Step::ReplaceAround(ReplaceAroundStep {
        span: Span {
            from: start - if at_beginning { 1 } else { 0 },
            to: end + if at_end { 1 } else { 0 },
        },
        gap_from: start + 1,
        gap_to: end - 1,
        slice,
        insert: if at_beginning { 0 } else { 1 },
        structure: true,
    });
    s.tr.step(step).is_ok()
}

fn lift_depth(s: &mut Session, from: usize, to: usize, depth: usize, target: usize) -> bool {
    let doc = s.tr.doc.clone();
    let Ok(at_from) = doc.resolve(from) else {
        return false;
    };
    let Ok(at_to) = doc.resolve(to.min(doc.content_size())) else {
        return false;
    };
    let range = NodeRange::new(at_from, at_to, depth);
    let step = build_lift(&range, target);
    drop(range);
    let Some(step) = step else { return false };
    s.tr.step(step).is_ok()
}

fn build_lift(range: &NodeRange<'_, Dyn>, target: usize) -> Option<Step<Dyn>> {
    let from = &range.from;
    let to = &range.to;
    let depth = range.depth;
    let gap_start = from.before(depth + 1)?;
    let gap_end = to.after(depth + 1)?;
    let mut start = gap_start;
    let mut end = gap_end;
    let mut before = Fragment::new();
    let mut open_start = 0;
    let mut d = depth;
    let mut splitting = false;
    while d > target {
        if splitting || from.index(d) > 0 {
            splitting = true;
            before = Fragment::from(vec![from.node(d).copy(|_| before)]);
            open_start += 1;
        } else {
            start -= 1;
        }
        d -= 1;
    }
    let mut after = Fragment::new();
    let mut open_end = 0;
    d = depth;
    splitting = false;
    while d > target {
        let after_pos = to.after(d + 1).unwrap_or(0);
        let end_d = to.end(d);
        if splitting || after_pos < end_d {
            splitting = true;
            after = Fragment::from(vec![to.node(d).copy(|_| after)]);
            open_end += 1;
        } else {
            end += 1;
        }
        d -= 1;
    }
    let before_size = before.size();
    let combined = before.append(after);
    Some(Step::ReplaceAround(ReplaceAroundStep {
        span: Span {
            from: start,
            to: end,
        },
        gap_from: gap_start,
        gap_to: gap_end,
        slice: crate::model::Slice::new(combined, open_start, open_end),
        insert: before_size - open_start,
        structure: true,
    }))
}

fn apply_wrap(s: &mut Session, start: usize, end: usize, wrappers: &[Wrapper<Dyn>]) -> Option<()> {
    let mut content = Fragment::new();
    for wrap in wrappers.iter().rev() {
        content = Fragment::from(vec![wrap.node_type.create(
            wrap.attrs.clone(),
            Some(&content),
            None,
        )]);
    }
    let step = Step::ReplaceAround(ReplaceAroundStep {
        span: Span {
            from: start,
            to: end,
        },
        gap_from: start,
        gap_to: end,
        slice: crate::model::Slice::new(content, 0, 0),
        insert: wrappers.len(),
        structure: true,
    });
    s.tr.step(step).ok().map(|_| ())
}

fn replace_with_text(s: &mut Session, text: &str) {
    if text.is_empty() {
        return;
    }
    s.sync();
    let from = s.sel.from();
    let to = s.sel.to();
    let doc = s.tr.doc.clone();
    let marks = if let Some(stored) = &s.stored {
        stored.clone()
    } else {
        doc.resolve(s.sel.head)
            .map(|p| marks_from_resolved(&p))
            .unwrap_or_default()
    };
    let mut node = DynamicNode::text(text);
    node = node.mark(marks);
    s.tr.replace_with(from, to, Fragment::from(vec![node]));
    s.sel.node = false;
}

// ---------------------------------------------------------------------------
// Mark matchers. Each pattern is the extension's `MarkInputRule`, anchored
// at the end of the text in front of the cursor.
// ---------------------------------------------------------------------------

fn code_mark_match(text: &str) -> Option<MarkHit> {
    wrap_match(
        text,
        "`",
        "`",
        |_| true,
        |content| content_edges(content, |_| true),
        |_| true,
    )
}

fn em_star_match(text: &str) -> Option<MarkHit> {
    wrap_match(
        text,
        "*",
        "*",
        |c| c != '*',
        |content| content_edges(content, |c| c != '*'),
        |before| !before.ends_with('*'),
    )
}

fn em_under_match(text: &str) -> Option<MarkHit> {
    wrap_match(
        text,
        "_",
        "_",
        |c| c != '_',
        |content| content_edges(content, |c| c != '_'),
        |before| !before.ends_with('_'),
    )
}

fn strong_star_match(text: &str) -> Option<MarkHit> {
    wrap_match(
        text,
        "**",
        "**",
        |_| true,
        |c| content_edges(c, |_| true),
        |_| true,
    )
}

fn strong_under_match(text: &str) -> Option<MarkHit> {
    wrap_match(
        text,
        "__",
        "__",
        |_| true,
        |c| content_edges(c, |_| true),
        |_| true,
    )
}

fn strike_single_match(text: &str) -> Option<MarkHit> {
    wrap_match(
        text,
        "~",
        "~",
        |c| c != '~',
        |content| {
            let mut chars = content.chars();
            let Some(first) = chars.next() else {
                return false;
            };
            if first.is_whitespace() {
                return false;
            }
            let last = content.chars().last().unwrap_or(first);
            if content.chars().count() > 1 && (last.is_whitespace() || last == '~') {
                return false;
            }
            true
        },
        |_| true,
    )
}

fn strike_double_match(text: &str) -> Option<MarkHit> {
    wrap_match(
        text,
        "~~",
        "~~",
        |_| true,
        |c| content_edges(c, |_| true),
        |_| true,
    )
}

fn content_edges(content: &str, first_ok: impl Fn(char) -> bool) -> bool {
    let mut chars = content.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first.is_whitespace() || !first_ok(first) {
        return false;
    }
    let last = content.chars().last().unwrap_or(first);
    !last.is_whitespace()
}

fn wrap_match(
    text: &str,
    open: &str,
    close: &str,
    trail_ok: impl Fn(char) -> bool,
    content_ok: impl Fn(&str) -> bool,
    behind_ok: impl Fn(&str) -> bool,
) -> Option<MarkHit> {
    let chars: Vec<char> = text.chars().collect();
    let trailing = *chars.last()?;
    if !trail_ok(trailing) {
        return None;
    }
    let body: String = chars[..chars.len() - 1].iter().collect();
    if !body.ends_with(close) {
        return None;
    }
    let inner = &body[..body.len() - close.len()];
    let mut search_from = 0;
    while let Some(rel) = inner[search_from..].find(open) {
        let idx = search_from + rel;
        let before = &inner[..idx];
        if behind_ok(before) {
            let content = &inner[idx + open.len()..];
            if content_ok(content) {
                let full_chars = inner[idx..].chars().count() + close.chars().count() + 1;
                return Some(MarkHit {
                    content: content.to_string(),
                    trailing: trailing.to_string(),
                    full_chars,
                });
            }
        }
        search_from = idx + open.len();
    }
    None
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn is_list_item(node: &DynamicNode) -> bool {
    crate::dynamic::types::with_types(|store| {
        store
            .node_types
            .get(node.type_idx)
            .is_some_and(|data| data.groups.iter().any(|g| g == "list_item"))
    })
    .unwrap_or(false)
}

fn default_textblock(
    match_: &crate::dynamic::types::DynamicContentMatch,
) -> Option<crate::dynamic::DynamicNodeType> {
    for i in 0..match_.edge_count() {
        if let Some((ty, _)) = match_.edge(i) {
            if ty.is_textblock() && !ty.has_required_attrs() {
                return Some(ty);
            }
        }
    }
    None
}

fn marks_at(doc: &DynamicNode, pos: usize) -> MarkSet<Dyn> {
    doc.resolve(pos)
        .map(|p| marks_from_resolved(&p))
        .unwrap_or_default()
}

fn marks_from_resolved(pos: &crate::model::ResolvedPos<'_, Dyn>) -> MarkSet<Dyn> {
    let mut set = MarkSet::new();
    for mark in pos.marks() {
        set.add(&mark);
    }
    set
}

fn remove_mark_name(mut set: MarkSet<Dyn>, name: &str) -> MarkSet<Dyn> {
    let doomed: Vec<_> = set
        .iter()
        .filter(|m| m.type_name == name)
        .cloned()
        .collect();
    for mark in doomed {
        set.remove(&mark);
    }
    set
}

fn attr_i64(node: &DynamicNode, name: &str, default: i64) -> i64 {
    node.attrs
        .get(name)
        .and_then(Value::as_i64)
        .unwrap_or(default)
}

fn prev_unit(pos: &crate::model::ResolvedPos<'_, Dyn>) -> usize {
    let Some(before) = pos.node_before() else {
        return 1;
    };
    if let Some(text) = before.text_node() {
        let s = text.text.as_str();
        s.chars().last().map(|c| c.len_utf16()).unwrap_or(1)
    } else {
        before.node_size()
    }
}

fn next_unit(pos: &crate::model::ResolvedPos<'_, Dyn>) -> usize {
    let Some(after) = pos.node_after() else {
        return 1;
    };
    if let Some(text) = after.text_node() {
        text.text
            .as_str()
            .chars()
            .next()
            .map(|c| c.len_utf16())
            .unwrap_or(1)
    } else {
        1
    }
}

fn first_textblock_pos(doc: &DynamicNode) -> usize {
    let mut found = 0;
    if let Some(content) = doc.content() {
        content.nodes_between(
            0,
            content.size(),
            &mut |node, pos, _, _| {
                if found == 0 && node.is_textblock() {
                    found = pos + 1;
                    return false;
                }
                found == 0
            },
            0,
            None,
        );
    }
    found
}

fn find_text(doc: &DynamicNode, needle: &str) -> Option<usize> {
    let mut found = None;
    let content = doc.content()?;
    content.nodes_between(
        0,
        content.size(),
        &mut |node, pos, _, _| {
            if found.is_some() {
                return false;
            }
            if let Some(text) = node.text_node() {
                if let Some(byte) = text.text.as_str().find(needle) {
                    let chars = text.text.as_str()[..byte].chars().count();
                    found = Some(pos + chars);
                    return false;
                }
            }
            true
        },
        0,
        None,
    );
    found
}
