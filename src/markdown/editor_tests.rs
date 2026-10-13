//! Commands, input rules, and DOM specs from the prosemirror-remark extensions.
//!
//! Parse and serialize are covered in [`super::tests`]. These tests follow the
//! extension keymap / input-rule / `parseDOM` cases.

use std::collections::HashMap;

use serde_json::{json, Value};

use super::{
    add_mark_to_nodes, is_at_start_in, is_list_spread, parse_dom, resolve_references,
    task_stops_event, to_dom, to_node_view, unified_extensions, DomElement, DomNode, Editor,
    Flavor, Key, ReferenceDefinition, Remark, TaskEventTarget, TextSelection,
};
use crate::model::Node;

fn gfm() -> Editor {
    Editor::new(Remark::gfm())
}

fn cm() -> Editor {
    Editor::new(Remark::commonmark())
}

fn type_md(editor: &Editor, text: &str) -> String {
    editor.markdown(&editor.type_text(&editor.open(""), text))
}

fn type_at(markdown: &str, needle: &str, chars: usize, text: &str) -> String {
    let editor = gfm();
    let state = editor.cursor_at(editor.open(markdown), needle, chars);
    editor.markdown(&editor.type_text(&state, text))
}

fn press_at(markdown: &str, needle: &str, chars: usize, key: Key) -> String {
    let editor = gfm();
    let state = editor.cursor_at(editor.open(markdown), needle, chars);
    let next = editor.press(&state, key).unwrap_or(state);
    editor.markdown(&next)
}

fn json_after_type(text: &str) -> Value {
    let editor = gfm();
    editor.to_json(&editor.type_text(&editor.open(""), text))
}

fn block_types(value: &Value) -> Vec<&str> {
    value["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["type"].as_str().unwrap())
        .collect()
}

fn el(tag: &str, attrs: &[(&str, &str)], children: Vec<DomNode>) -> DomNode {
    DomNode::Element(DomElement {
        tag: tag.into(),
        attrs: attrs
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
        children,
    })
}

fn text(value: &str) -> DomNode {
    DomNode::Text(value.into())
}

fn parsed_json(dom: &DomNode) -> Value {
    let editor = gfm();
    let doc = parse_dom(editor.schema(), dom).expect("dom");
    editor.schema().with_types(|| doc.to_json(true))
}

#[test]
fn checklist_helpers_match_the_typescript_utils() {
    let editor = gfm();
    let start = editor.open("Hello");
    assert!(is_at_start_in(editor.schema(), &start));
    let mid = editor.cursor_at(start.clone(), "Hello", 2);
    assert!(!is_at_start_in(editor.schema(), &mid));
    let mut ranged = start;
    ranged.selection = TextSelection {
        anchor: 1,
        head: 3,
        node: false,
    };
    assert!(!is_at_start_in(editor.schema(), &ranged));

    assert!(!is_list_spread(false, &[(false, 1)]));
    assert!(is_list_spread(true, &[]));
    assert!(is_list_spread(false, &[(true, 1)]));
    assert!(!is_list_spread(false, &[(true, 2)]));

    let mut refs = HashMap::from([(
        "img".into(),
        vec![HashMap::from([("alt".into(), json!("x"))])],
    )]);
    let defs = HashMap::from([(
        "img".into(),
        ReferenceDefinition {
            url: "https://example.test".into(),
            title: Some("t".into()),
        },
    )]);
    resolve_references(&mut refs, &defs, "src");
    assert_eq!(refs["img"][0]["src"], json!("https://example.test"));
    assert_eq!(refs["img"][0]["title"], json!("t"));

    let plain = unified_extensions(Flavor::CommonMark);
    assert!(!plain.strikethrough && !plain.task_list && !plain.autolink);
    let flavored = unified_extensions(Flavor::Gfm);
    assert!(flavored.strikethrough && flavored.task_list && flavored.autolink);

    let node = editor.schema().text("Hi");
    let mark = editor
        .schema()
        .mark_from_json(&json!({"type": "em"}))
        .unwrap();
    let marked = add_mark_to_nodes(std::slice::from_ref(&node), &mark);
    assert!(marked[0]
        .marks()
        .unwrap()
        .iter()
        .any(|m| m.type_name == "em"));
}

#[test]
fn heading_input_rule_and_level_keys() {
    assert_eq!(type_md(&gfm(), "# Hello"), "# Hello\n");
    assert_eq!(type_md(&gfm(), "### Hello"), "### Hello\n");
    assert_eq!(type_md(&gfm(), "   # Hello"), "# Hello\n");
    assert_eq!(type_md(&gfm(), "####### Hello"), "####### Hello\n");

    assert_eq!(
        press_at("## Hello\n", "Hello", 0, Key::Char('#')),
        "### Hello\n"
    );
    assert_eq!(
        press_at("###### Hello\n", "Hello", 0, Key::Char('#')),
        "###### #Hello\n"
    );
    assert_eq!(
        press_at("## Hello\n", "Hello", 2, Key::Char('#')),
        "## He#llo\n"
    );
    assert_eq!(
        press_at("## Hello\n", "Hello", 0, Key::ShiftTab),
        "# Hello\n"
    );
    assert_eq!(press_at("# Hello\n", "Hello", 0, Key::ShiftTab), "Hello\n");
    assert_eq!(press_at("# Hello\n", "Hello", 0, Key::Backspace), "Hello\n");
    assert_eq!(press_at("# Hello\n", "Hello", 0, Key::Tab), "## Hello\n");
    assert_eq!(press_at("Hello\n", "Hello", 2, Key::Tab), "Hello\n");
    assert_eq!(
        press_at("Hello\n", "Hello", 0, Key::ShiftModDigit(3)),
        "### Hello\n"
    );
}

#[test]
fn mark_input_rules_need_a_trailing_character() {
    assert_eq!(type_md(&gfm(), "**Test**END"), "**Test**END\n");
    assert_eq!(type_md(&gfm(), "__Test__END"), "**Test**END\n");
    assert_eq!(type_md(&gfm(), "*Test*END"), "*Test*END\n");
    assert_eq!(type_md(&gfm(), "_Test_END"), "*Test*END\n");
    assert_eq!(type_md(&gfm(), "`Test`END"), "`Test`END\n");
    assert_eq!(type_md(&gfm(), "~~Test~~END"), "~~Test~~END\n");
    // Single tilde is the same strikethrough mark; stringify always uses `~~`.
    assert_eq!(type_md(&gfm(), "~Test~END"), "~~Test~~END\n");
    assert_eq!(type_md(&cm(), "~Test~END"), "~Test~END\n");
    assert_eq!(type_md(&gfm(), "**Test__END"), "\\*\\*Test\\_\\_END\n");
}

#[test]
fn mark_toggles_wrap_a_selection() {
    let editor = gfm();
    let mut state = editor.open("abcdef");
    state.selection = TextSelection {
        anchor: 3,
        head: 5,
        node: false,
    };
    assert_eq!(
        editor.markdown(&editor.press(&state, Key::ModB).unwrap()),
        "ab**cd**ef\n"
    );
    assert_eq!(
        editor.markdown(&editor.press(&state, Key::ModI).unwrap()),
        "ab*cd*ef\n"
    );
    assert_eq!(
        editor.markdown(&editor.press(&state, Key::CtrlBacktick).unwrap()),
        "ab`cd`ef\n"
    );
}

#[test]
fn block_input_rules() {
    assert_eq!(type_md(&gfm(), "> quote"), "> quote\n");
    assert_eq!(type_md(&gfm(), "- item"), "* item\n");
    assert_eq!(type_md(&gfm(), "* item"), "* item\n");
    assert_eq!(type_md(&gfm(), "1. item"), "1. item\n");
    assert_eq!(type_md(&gfm(), "42. item"), "42. item\n");
    assert_eq!(block_types(&json_after_type("```")), ["code_block"]);
    assert_eq!(block_types(&json_after_type("    ")), ["code_block"]);
    assert_eq!(
        block_types(&json_after_type("---\n")),
        ["paragraph", "horizontal_rule", "paragraph"]
    );
    assert_eq!(
        block_types(&json_after_type("***\n")),
        ["paragraph", "horizontal_rule", "paragraph"]
    );
    assert_eq!(type_md(&gfm(), "a***\n"), "a\\*\\*\\*\n\n");
}

#[test]
fn horizontal_rule_key_splits_the_paragraph() {
    let editor = gfm();
    let state = editor.cursor_at(editor.open("abcdef"), "abcdef", 3);
    let next = editor.press(&state, Key::ModUnderscore).unwrap();
    assert_eq!(
        block_types(&editor.to_json(&next)),
        ["paragraph", "horizontal_rule", "paragraph"]
    );
    assert_eq!(editor.markdown(&next), "abc\n\n---\n\ndef\n");
}

#[test]
fn lists_split_sink_lift_and_join_on_a_continuing_number() {
    assert_eq!(type_md(&gfm(), "1. Hello\nSecond"), "1. Hello\n2. Second\n");
    assert_eq!(type_md(&gfm(), "1. a\n\n2. b"), "1. a\n2. b\n");
    let separate = json_after_type("1. a\n\n7. b");
    assert_eq!(block_types(&separate), ["ordered_list", "ordered_list"]);
    assert_eq!(separate["content"][1]["attrs"]["start"], json!(7));

    assert_eq!(
        press_at("* Hello\n* World\n", "World", 0, Key::Tab),
        "* Hello\n  * World\n"
    );
    assert_eq!(
        press_at("* Hello\n  * World\n", "World", 0, Key::ShiftTab),
        "* Hello\n* World\n"
    );
    assert_eq!(press_at("* Helo\n", "Helo", 3, Key::Enter), "* Hel\n* o\n");
    assert_eq!(
        press_at("* Hello\n* World\n", "World", 0, Key::Backspace),
        "* Hello\n\n  World\n"
    );
    assert_eq!(
        press_at("Hello\n\nWorld\n", "World", 0, Key::Backspace),
        "HelloWorld\n"
    );
}

#[test]
fn task_items_follow_the_extension_keymap_and_input_rule() {
    assert_eq!(type_md(&gfm(), "- [ ] Hello"), "* [ ] Hello\n");
    assert_eq!(type_md(&gfm(), "- [x] Hello"), "* [x] Hello\n");
    assert_eq!(type_md(&gfm(), "- [X] Hello"), "* [x] Hello\n");
    assert_eq!(type_md(&gfm(), "- [] Hello"), "* [ ] Hello\n");
    assert_eq!(type_md(&cm(), "- [ ] Hello"), "* \\[ \\] Hello\n");
    assert_eq!(type_md(&gfm(), "[ ] Hello"), "\\[ \\] Hello\n");
    assert_eq!(
        type_at("* Hello\n\n  World\n", "World", 0, "[ ] "),
        "* Hello\n\n  \\[ \\] World\n"
    );
    assert_eq!(
        type_at("* Hello\n* World\n", "World", 0, "[x] X"),
        "* Hello\n* [x] XWorld\n"
    );

    assert_eq!(
        press_at("* [ ] Hello\n* [ ] World\n", "Hello", 0, Key::Backspace),
        "* Hello\n* [ ] World\n"
    );
    assert_eq!(
        press_at("* [ ] Hello\n\n  World\n", "World", 0, Key::Backspace),
        "* [ ] HelloWorld\n"
    );
    assert_eq!(
        press_at("* [ ] Hello\n", "Hello", 2, Key::Backspace),
        "* [ ] Hllo\n"
    );
    assert_eq!(
        press_at("* [ ] Helo\n", "Helo", 3, Key::Enter),
        "* [ ] Hel\n* [ ] o\n"
    );
    assert_eq!(
        press_at("* [x] Helo\n", "Helo", 3, Key::Enter),
        "* [x] Hel\n* [ ] o\n"
    );
    assert_eq!(
        press_at("* [ ] Hello\n* [ ] World\n", "World", 0, Key::Tab),
        "* [ ] Hello\n  * [ ] World\n"
    );
    assert_eq!(
        press_at("* [ ] Hello\n  * [ ] World\n", "World", 0, Key::ShiftTab),
        "* [ ] Hello\n* [ ] World\n"
    );
    assert_eq!(
        press_at("* [ ] Hello\n", "Hello", 0, Key::ShiftTab),
        "Hello\n"
    );
}

#[test]
fn hard_break_blockquote_lists_and_code_exit() {
    assert_eq!(press_at("Hello\n", "Hello", 5, Key::ModEnter), "Hello\\\n");
    assert_eq!(
        press_at("Hello\n", "Hello", 0, Key::ModGreater),
        "> Hello\n"
    );
    assert_eq!(
        press_at("Hello\n", "Hello", 0, Key::ShiftModDigit(8)),
        "* Hello\n"
    );
    assert_eq!(
        press_at("Hello\n", "Hello", 0, Key::ShiftModDigit(9)),
        "1. Hello\n"
    );
    assert_eq!(
        press_at("Hello\n", "Hello", 0, Key::ShiftModBackslash),
        "```\nHello\n```\n"
    );

    let editor = gfm();
    let state = editor.type_text(&editor.open(""), "```code\n\n\n");
    let json = editor.to_json(&state);
    assert_eq!(block_types(&json), ["code_block", "paragraph"]);
    assert_eq!(json["content"][0]["content"][0]["text"], json!("code"));
}

#[test]
fn task_checkbox_click_and_node_view() {
    let editor = gfm();
    let state = editor.open("* [ ] a\n");
    let next = editor.click_task_checkbox(&state, 1).expect("task");
    assert_eq!(editor.markdown(&next), "* [x] a\n");
    assert!(task_stops_event(TaskEventTarget::Checkbox));
    assert!(!task_stops_event(TaskEventTarget::Content));

    assert!(format!("{:?}", to_dom(&state.doc)).contains("disabled"));
    let view = format!("{:?}", to_node_view(&next.doc));
    assert!(view.contains("checked"));
    assert!(!view.contains("disabled"));
}

#[test]
fn dom_round_trip_and_parse_rules() {
    let editor = gfm();
    for md in [
        "Hello\n",
        "# Hello\n",
        "**bold** *em* `code` ~~nope~~\n",
        "> quote\n",
        "```\ncode\n```\n",
        "---\n",
        "* item\n",
        "1. item\n",
        "* [x] done\n",
        "![alt](https://example.test \"t\")\n",
        "[link](https://example.test)\n",
        "a\\\nb\n",
    ] {
        let parsed = editor.remark().parse(md);
        let again = parse_dom(editor.schema(), &to_dom(&parsed.doc)).unwrap();
        let left = editor.schema().with_types(|| parsed.doc.to_json(true));
        let right = editor.schema().with_types(|| again.to_json(true));
        assert_eq!(left, right, "dom round trip {md:?}");
    }

    let strong = parsed_json(&el(
        "p",
        &[],
        vec![el(
            "span",
            &[("style", "font-weight: 700")],
            vec![text("Hello")],
        )],
    ));
    assert_eq!(
        strong["content"][0]["content"][0]["marks"][0]["type"],
        json!("strong")
    );

    let italic = parsed_json(&el(
        "p",
        &[],
        vec![el(
            "span",
            &[("style", "font-style: italic")],
            vec![text("Hello")],
        )],
    ));
    assert_eq!(
        italic["content"][0]["content"][0]["marks"][0]["type"],
        json!("em")
    );

    let oblique = parsed_json(&el(
        "p",
        &[],
        vec![el(
            "span",
            &[("style", "font-style: oblique")],
            vec![text("Hello")],
        )],
    ));
    assert!(oblique["content"][0]["content"][0].get("marks").is_none());

    let strike = parsed_json(&el(
        "p",
        &[],
        vec![el(
            "span",
            &[("style", "text-decoration: line-through")],
            vec![text("Hello")],
        )],
    ));
    assert_eq!(
        strike["content"][0]["content"][0]["marks"][0]["type"],
        json!("strikethrough")
    );

    let task = parsed_json(&el(
        "ul",
        &[],
        vec![el(
            "li",
            &[("class", "task-list-item")],
            vec![
                el(
                    "input",
                    &[("type", "checkbox"), ("checked", "checked")],
                    vec![],
                ),
                text(" a"),
            ],
        )],
    ));
    assert_eq!(
        task["content"][0]["content"][0]["type"],
        json!("task_list_item")
    );
    assert_eq!(
        task["content"][0]["content"][0]["attrs"]["checked"],
        json!(true)
    );
    assert_eq!(
        task["content"][0]["content"][0]["content"][0]["content"][0]["text"],
        json!("a")
    );

    let ordered = parsed_json(&el(
        "ol",
        &[("start", "5")],
        vec![el("li", &[], vec![el("p", &[], vec![text("Hello")])])],
    ));
    assert_eq!(ordered["content"][0]["attrs"]["start"], json!(5));
}
