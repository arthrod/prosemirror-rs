use super::{MarkdownDoc, Remark};
use crate::dynamic::DynamicSchema;
use crate::editor::Editor;
use crate::model::Node;
use serde_json::{json, Value};

fn cm() -> Remark {
    Remark::commonmark()
}

fn gfm() -> Remark {
    Remark::gfm()
}

fn round_trip(remark: Remark, markdown: &str) -> MarkdownDoc {
    let parsed = remark.parse(markdown);
    let serialized = remark.serialize(&parsed.doc);
    let again = remark.parse(&serialized);
    assert_eq!(
        parsed.doc, again.doc,
        "round trip changed the document for {markdown:?}\n{serialized}"
    );
    parsed
}

fn assert_serialize(remark: Remark, markdown: &str, expected: &str) {
    let parsed = remark.parse(markdown);
    assert_eq!(
        remark.serialize(&parsed.doc),
        expected,
        "input {markdown:?}"
    );
}

fn assert_canonical(remark: Remark, markdown: &str) {
    let expected = if markdown.ends_with('\n') {
        markdown.to_string()
    } else {
        format!("{markdown}\n")
    };
    let parsed = round_trip(remark, markdown);
    assert_eq!(remark.serialize(&parsed.doc), expected);
}

fn node(schema: &DynamicSchema, value: Value) -> crate::dynamic::DynamicNode {
    schema.node_from_json(&value).expect("node json")
}

fn marks_of(doc: &MarkdownDoc) -> Vec<String> {
    fn walk(value: &Value, out: &mut Vec<String>) {
        if let Some(marks) = value.get("marks").and_then(Value::as_array) {
            for mark in marks {
                if let Some(name) = mark.get("type").and_then(Value::as_str) {
                    out.push(name.to_string());
                }
            }
        }
        if let Some(content) = value.get("content").and_then(Value::as_array) {
            for child in content {
                walk(child, out);
            }
        }
    }
    let mut names = Vec::new();
    walk(&doc.to_json(), &mut names);
    names
}

const COMMONMARK_SOURCE: &str = concat!(
    "> Inside a blockquote\n",
    "\n",
    "Paragraph with a  \n",
    "hard break\n",
    "```\nCode\n```\n",
    "# Hello\n",
    "***\n",
    "![Awesome image](https://example.test)\n",
    "\n",
    "![Image 2][img2]\n",
    "\n",
    "[img2]: https://img2.test\n",
    "\n",
    "1. Ordered list\n",
    "\n",
    "- Unordered list\n",
    "\n",
    "A text with a **bold part**, some `inline code`, a bit *that is italic*, one [link](https://example.test) and another [type of link][link2].\n",
    "\n",
    "[link2]: https://link2.test",
);

const COMMONMARK_SERIALIZED: &str = concat!(
    "> Inside a blockquote\n",
    "\n",
    "Paragraph with a\\\n",
    "hard break\n",
    "\n",
    "```\n",
    "Code\n",
    "```\n",
    "\n",
    "# Hello\n",
    "\n",
    "---\n",
    "\n",
    "![Awesome image](https://example.test)\n",
    "\n",
    "![Image 2](https://img2.test)\n",
    "\n",
    "1. Ordered list\n",
    "\n",
    "* Unordered list\n",
    "\n",
    "A text with a **bold part**, some `inline code`, a bit *that is italic*, one [link](https://example.test) and another [type of link](https://link2.test).\n",
);

#[test]
fn commonmark_fixture_parses_and_serializes() {
    assert_serialize(cm(), COMMONMARK_SOURCE, COMMONMARK_SERIALIZED);
    round_trip(cm(), COMMONMARK_SERIALIZED);
}

#[test]
fn commonmark_reduced_fixture_serializes() {
    let source = concat!(
        "> Inside a blockquote\n",
        "\n",
        "Paragraph with a\\\n",
        "hard break\n",
        "\n",
        "```\n",
        "Code\n",
        "```\n",
        "\n",
        "# Hello\n",
        "\n",
        "---\n",
        "\n",
        "![Awesome image](https://example.test)\n",
        "\n",
        "1. Ordered list\n",
        "\n",
        "* Unordered list\n",
        "\n",
        "A text with a **bold part**, some `inline code`, a bit *that is italic*, one [link](https://example.test).\n",
    );
    assert_canonical(cm(), source);
}

#[test]
fn nested_marks_share_one_set() {
    for markdown in ["**_nested_**", "_**nested**_"] {
        let parsed = cm().parse(markdown);
        let mut marks = marks_of(&parsed);
        marks.sort();
        assert_eq!(
            marks,
            vec!["em".to_string(), "strong".to_string()],
            "{markdown}"
        );
    }
    let parsed = cm().parse("***`code`***");
    let mut marks = marks_of(&parsed);
    marks.sort();
    assert_eq!(
        marks,
        vec!["code".to_string(), "em".to_string(), "strong".to_string()]
    );
}

#[test]
fn inline_code_on_non_text_is_dropped() {
    let remark = cm();
    let schema = remark.schema();
    let cases = [
        (
            json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [
                        {"type": "text", "text": "a", "marks": [{"type": "code"}]},
                        {"type": "hard_break", "marks": [{"type": "code"}]},
                        {"type": "text", "text": "b", "marks": [{"type": "code"}]}
                    ]
                }]
            }),
            "`a`\\\n`b`\n",
        ),
        (
            json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [
                        {"type": "text", "text": "a"},
                        {"type": "hard_break", "marks": [{"type": "code"}]},
                        {"type": "text", "text": "b"}
                    ]
                }]
            }),
            "a\\\nb\n",
        ),
        (
            json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [
                        {"type": "text", "text": "a", "marks": [{"type": "code"}]},
                        {"type": "image", "attrs": {"alt": "x", "src": "https://i.test"}, "marks": [{"type": "code"}]}
                    ]
                }]
            }),
            "`a`![x](https://i.test)\n",
        ),
        (
            json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [
                        {"type": "image", "attrs": {"alt": "x", "src": "https://i.test"}, "marks": [{"type": "code"}]}
                    ]
                }]
            }),
            "![x](https://i.test)\n",
        ),
    ];
    for (value, expected) in cases {
        let doc = node(&schema, value);
        assert_eq!(remark.serialize(&doc), expected);
    }
}

#[test]
fn mark_runs_round_trip() {
    let cases = [
        "[a **bold** part](https://example.test)",
        "[an *italic* part](https://example.test)",
        "[some `code` here](https://example.test)",
        "[**bold** and *italic*](https://example.test)",
        "[a **bold** part](https://example.test \"Title\")",
        "[an ![image](https://img.test)](https://example.test)",
        "[a\\\nbreak](https://example.test)",
        "**a *nested* part**",
        "*a **nested** part*",
        "**a [link](https://example.test) part**",
        "**a\\\nbreak**",
        "# [a **bold** part](https://example.test)",
        "[one](https://one.test)[two](https://two.test)",
        "[one](https://example.test \"Title\")[two](https://example.test)",
        "**`code`**",
        "*`code`*",
        "[`code`](https://example.test)",
        "***`code`***",
        "[**`code`**](https://example.test)",
        "**a `code` part**",
    ];
    for markdown in cases {
        assert_canonical(cm(), markdown);
    }
}

#[test]
fn references_become_inline_links() {
    assert_serialize(
        cm(),
        "[a][x] and [b][x]\n\n[x]: https://example.test",
        "[a](https://example.test) and [b](https://example.test)\n",
    );
    assert_serialize(
        cm(),
        "![a][x] ![b][x]\n\n[x]: https://example.test",
        "![a](https://example.test) ![b](https://example.test)\n",
    );
    assert_serialize(
        cm(),
        "[a][x][b][y]\n\n[x]: https://x.test\n[y]: https://y.test",
        "[a](https://x.test)[b](https://y.test)\n",
    );
    let unresolved = cm().parse("[missing][nope]\n");
    assert_eq!(remark_text(&unresolved), "[missing][nope]");
    assert!(marks_of(&unresolved).is_empty());
}

fn remark_text(doc: &MarkdownDoc) -> String {
    doc.schema.with_types(|| doc.doc.text_content())
}

#[test]
fn autolinks_follow_remark() {
    let cases = [
        ("<https://example.test>", "<https://example.test>\n"),
        ("<foo@bar.example.com>", "<foo@bar.example.com>\n"),
        (
            "<mailto:foo@bar.example.com>",
            "<mailto:foo@bar.example.com>\n",
        ),
        (
            "Visit <https://example.test/a?b=c> or <foo@bar.example.com>.",
            "Visit <https://example.test/a?b=c> or <foo@bar.example.com>.\n",
        ),
        (
            "[https://example.test](https://example.test)",
            "<https://example.test>\n",
        ),
        (
            "[https://example.test](https://example.test \"Title\")",
            "[https://example.test](https://example.test \"Title\")\n",
        ),
        (
            "[example.test](https://example.test)",
            "[example.test](https://example.test)\n",
        ),
        (
            "**<https://example.test>**",
            "[**https://example.test**](https://example.test)\n",
        ),
        ("https://example.test", "https://example.test\n"),
        ("foo@bar.example.com", "foo@bar.example.com\n"),
    ];
    for (markdown, serialized) in cases {
        let parsed = cm().parse(markdown);
        assert_eq!(cm().serialize(&parsed.doc), serialized, "{markdown}");
        let again = cm().parse(serialized);
        assert_eq!(parsed.doc, again.doc, "{markdown}");
    }
    let mail = cm().parse("<foo@bar.example.com>");
    let json = mail.to_json().to_string();
    assert!(json.contains("mailto:foo@bar.example.com"), "{json}");
}

#[test]
fn definitions_loosen_lists() {
    let cases = [
        ("- a\n- b\n\n  [ref]: /url\n- d\n", "* a\n\n* b\n\n* d\n"),
        ("- a\n- b\n\n  [ref]: /url\n", "* a\n\n* b\n"),
        (
            "1. a\n2. b\n\n   [ref]: /url\n3. d\n",
            "1. a\n\n2. b\n\n3. d\n",
        ),
        (
            "- a\n  - b\n  - c\n\n    [ref]: /url\n- d\n",
            "* a\n  * b\n\n  * c\n* d\n",
        ),
    ];
    for (markdown, serialized) in cases {
        assert_serialize(cm(), markdown, serialized);
    }
}

#[test]
fn spread_single_child_loosens_the_list() {
    let remark = cm();
    let schema = remark.schema();
    let doc = node(
        &schema,
        json!({
            "type": "doc",
            "content": [{
                "type": "bullet_list",
                "content": [
                    {"type": "regular_list_item", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "a"}]}]},
                    {"type": "regular_list_item", "attrs": {"spread": true}, "content": [{"type": "paragraph", "content": [{"type": "text", "text": "b"}]}]},
                    {"type": "regular_list_item", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "c"}]}]}
                ]
            }]
        }),
    );
    assert_eq!(remark.serialize(&doc), "* a\n\n* b\n\n* c\n");
    let ordered = node(
        &schema,
        json!({
            "type": "doc",
            "content": [{
                "type": "ordered_list",
                "content": [
                    {"type": "regular_list_item", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "a"}]}]},
                    {"type": "regular_list_item", "attrs": {"spread": true}, "content": [{"type": "paragraph", "content": [{"type": "text", "text": "b"}]}]},
                    {"type": "regular_list_item", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "c"}]}]}
                ]
            }]
        }),
    );
    assert_eq!(remark.serialize(&ordered), "1. a\n\n2. b\n\n3. c\n");
}

#[test]
fn tight_item_joins_that_would_change_syntax_reparse_as_spread() {
    let remark = cm();
    let schema = remark.schema();
    let contents = [
        json!([
            {"type": "paragraph", "content": [{"type": "text", "text": "foo"}]},
            {"type": "bullet_list", "content": [{"type": "regular_list_item", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "bar"}]}]}]},
            {"type": "paragraph", "content": [{"type": "text", "text": "baz"}]}
        ]),
        json!([
            {"type": "paragraph", "content": [{"type": "text", "text": "foo"}]},
            {"type": "blockquote", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "q"}]}]},
            {"type": "paragraph", "content": [{"type": "text", "text": "bar"}]}
        ]),
        json!([
            {"type": "paragraph", "content": [{"type": "text", "text": "foo"}]},
            {"type": "blockquote", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "q"}]}]},
            {"type": "blockquote", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "r"}]}]}
        ]),
        json!([
            {"type": "paragraph", "content": [{"type": "text", "text": "foo"}]},
            {"type": "horizontal_rule"}
        ]),
    ];
    for content in contents {
        let tight = node(
            &schema,
            json!({
                "type": "doc",
                "content": [{
                    "type": "bullet_list",
                    "content": [{
                        "type": "regular_list_item",
                        "content": content
                    }]
                }]
            }),
        );
        let parsed = remark.parse(&remark.serialize(&tight));
        let item = &parsed.to_json()["content"][0]["content"][0];
        assert_eq!(
            item["attrs"]["spread"],
            json!(true),
            "{}",
            remark.serialize(&tight)
        );
    }
}

#[test]
fn empty_input_is_an_empty_paragraph() {
    let parsed = cm().parse("");
    assert_eq!(parsed.to_json()["content"][0]["type"], json!("paragraph"));
    assert!(parsed.to_json()["content"][0].get("content").is_none());
    assert_eq!(cm().serialize(&parsed.doc), "\n");
}

#[test]
fn html_and_code_language_are_dropped() {
    assert_eq!(
        cm().parse("<div>\nhello\n</div>\n").to_json()["content"][0]["type"],
        json!("paragraph")
    );
    let code = cm().parse("```js\nCode\n```\n");
    assert_eq!(cm().serialize(&code.doc), "```\nCode\n```\n");
}

#[test]
fn gfm_fixture() {
    let source = concat!(
        "> Inside a blockquote\n",
        "\n",
        "Paragraph with a  \n",
        "hard break\n",
        "```\nCode\n```\n",
        "# Hello\n",
        "***\n",
        "![Awesome image](https://example.test)\n",
        "\n",
        "![Image 2][img2]\n",
        "\n",
        "[img2]: https://img2.test\n",
        "\n",
        "1. Ordered list\n",
        "\n",
        "- Unordered list\n",
        "- [ ] Task list\n",
        "\n",
        "A text with a **bold part**, some `inline code`, a bit *that is italic*, one that is ~striked through~, one [link](https://example.test), one simple link to www.github.com and another [type of link][link2].\n",
        "\n",
        "[link2]: https://link2.test",
    );
    let expected = concat!(
        "> Inside a blockquote\n",
        "\n",
        "Paragraph with a\\\n",
        "hard break\n",
        "\n",
        "```\n",
        "Code\n",
        "```\n",
        "\n",
        "# Hello\n",
        "\n",
        "---\n",
        "\n",
        "![Awesome image](https://example.test)\n",
        "\n",
        "![Image 2](https://img2.test)\n",
        "\n",
        "1. Ordered list\n",
        "\n",
        "* Unordered list\n",
        "* [ ] Task list\n",
        "\n",
        "A text with a **bold part**, some `inline code`, a bit *that is italic*, one that is ~~striked through~~, one [link](https://example.test), one simple link to [www.github.com](http://www.github.com) and another [type of link](https://link2.test).\n",
    );
    assert_serialize(gfm(), source, expected);
    round_trip(gfm(), expected);
}

#[test]
fn gfm_marks_and_task_definitions() {
    for markdown in [
        "[a ~~striked~~ part](https://example.test)",
        "~~a [link](https://example.test) part~~",
        "~~a *nested* part~~",
        "~~`code`~~",
        "**~~`code`~~**",
    ] {
        assert_canonical(gfm(), markdown);
    }
    assert_serialize(
        gfm(),
        "- [ ] a\n- [x] b\n\n  [ref]: /url\n- [ ] d\n",
        "* [ ] a\n\n* [x] b\n\n* [ ] d\n",
    );
    assert_serialize(
        gfm(),
        "- a\n- [x] b\n\n  [ref]: /url\n- d\n",
        "* a\n\n* [x] b\n\n* d\n",
    );
    let plain = cm().parse("- [ ] text\n");
    assert_eq!(remark_text(&plain), "[ ] text");
    assert!(!plain.to_json().to_string().contains("task_list_item"));
    let linked = gfm().parse("https://example.test");
    assert_eq!(gfm().serialize(&linked.doc), "<https://example.test>\n");
    let mail = gfm().parse("foo@bar.example.com");
    assert_eq!(gfm().serialize(&mail.doc), "<foo@bar.example.com>\n");
}

#[test]
fn ordered_list_keeps_start() {
    assert_canonical(cm(), "3. a\n4. b");
    let parsed = cm().parse("3. c\n");
    assert_eq!(parsed.to_json()["content"][0]["attrs"]["start"], json!(3));
}

#[test]
fn nested_task_keeps_the_parent_checkbox() {
    assert_serialize(
        gfm(),
        "- [x] parent\n  - [ ] child\n",
        "* [x] parent\n  * [ ] child\n",
    );
}

#[test]
fn empty_task_keeps_the_checkbox() {
    let remark = gfm();
    let schema = remark.schema();
    let doc = node(
        &schema,
        json!({
            "type": "doc",
            "content": [{
                "type": "bullet_list",
                "content": [{
                    "type": "task_list_item",
                    "attrs": { "checked": true },
                    "content": [{ "type": "paragraph" }]
                }]
            }]
        }),
    );
    let serialized = remark.serialize(&doc);
    assert!(serialized.contains("[x]"), "{serialized}");
    let again = remark.parse(&serialized);
    assert!(again.to_json().to_string().contains("task_list_item"));
}

#[test]
fn gfm_autolink_does_not_nest_inside_a_link() {
    let parsed = gfm().parse("[see https://example.test](https://outer.test)\n");
    let json = parsed.to_json().to_string();
    assert_eq!(json.matches("\"link\"").count(), 1, "{json}");
    assert_eq!(
        gfm().serialize(&parsed.doc),
        "[see https://example.test](https://outer.test)\n"
    );
}

#[test]
fn link_destination_with_a_paren_round_trips() {
    let remark = cm();
    let schema = remark.schema();
    let doc = node(
        &schema,
        json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "a",
                    "marks": [{
                        "type": "link",
                        "attrs": { "href": "https://example.test/a)b" }
                    }]
                }]
            }]
        }),
    );
    let serialized = remark.serialize(&doc);
    assert_eq!(serialized, "[a](<https://example.test/a)b>)\n");
    let again = remark.parse(&serialized);
    let href = again.to_json()["content"][0]["content"][0]["marks"][0]["attrs"]["href"].clone();
    assert_eq!(href, json!("https://example.test/a)b"));
}

#[test]
fn literal_markers_stay_text() {
    assert_serialize(cm(), "\\# not a heading\n", "\\# not a heading\n");
    assert_serialize(cm(), "\\<tag> and AT\\&T\n", "\\<tag\\> and AT\\&T\n");
    assert_serialize(
        gfm(),
        "\\~\\~not struck\\~\\~\n",
        "\\~\\~not struck\\~\\~\n",
    );
    assert_serialize(cm(), "\\- still a paragraph\n", "\\- still a paragraph\n");
    assert_serialize(cm(), "1\\. not a list\n", "1\\. not a list\n");
}

#[test]
fn heading_with_an_image_is_schema_valid() {
    let parsed = cm().parse("# Hello ![a](https://example.test)\n");
    let json = parsed.to_json();
    assert_eq!(json["content"][0]["type"], json!("heading"));
    assert!(json.to_string().contains("image"), "{json}");
    parsed
        .schema
        .with_types(|| parsed.doc.check())
        .expect("heading content is inline*");
    let editor = Editor::new(&cm().schema_json(), &json.to_string()).expect("editor");
    assert_eq!(editor.version(), 0);
}

#[test]
fn code_mark_containing_a_newline_serializes_as_a_fenced_block() {
    let remark = cm();
    let schema = remark.schema();
    let doc = node(
        &schema,
        json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "a\nb",
                    "marks": [{ "type": "code" }]
                }]
            }]
        }),
    );
    let serialized = remark.serialize(&doc);
    assert!(serialized.starts_with("```\n"), "{serialized}");
    assert!(serialized.contains("a\nb"), "{serialized}");
    assert!(!serialized.contains("`a"), "{serialized}");
    let again = remark.parse(&serialized);
    assert_eq!(again.to_json()["content"][0]["type"], json!("code_block"));
    assert_eq!(
        again.to_json()["content"][0]["content"][0]["text"],
        json!("a\nb")
    );
}
