//! Document engine for the vendored markdown-remarkable viewer.
//!
//! markdown-remarkable still renders HTML with its own pulldown-cmark pass.
//! Review anchors are hashes of those source slices, so that renderer is not
//! replaced. This crate is the backend document authority: Markdown is parsed
//! with [`prosemirror::markdown::Remark`] and every step goes through
//! [`prosemirror::editor::Editor`]. Plain text is a paragraph-per-line
//! document on the same editor, with no Markdown syntax interpreted.
//!
//! Applying steps does not write the file. The GFM serializer drops tables,
//! footnotes, and raw HTML, and writing it back would move the review hashes.

use prosemirror::editor::Editor;
use prosemirror::markdown::Remark;
use prosemirror::model::Node;
use serde::Serialize;
use serde_json::Value;

/// Name reported on `/engine` and `X-Prosemirror-Engine`.
pub const ENGINE_NAME: &str = "prosemirror-rs";

/// Crate version this engine was built against.
pub const ENGINE_VERSION: &str = "0.5.2";

const PLAIN_SCHEMA: &str = r#"{"nodes":{"doc":{"content":"block+"},"paragraph":{"content":"inline*","group":"block"},"text":{"group":"inline"}},"marks":{}}"#;

/// Which remarkable file the engine is opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// `.md` / `.markdown`, parsed as GFM into the remark schema.
    Markdown,
    /// `.txt`. Each line is one paragraph. Markup is literal text.
    PlainText,
}

/// A document held by [`Editor`], plus the fields the viewer serves as JSON.
#[derive(Debug, Clone, Serialize)]
pub struct DocumentEngine {
    /// Always [`ENGINE_NAME`].
    pub engine: &'static str,
    /// Always [`ENGINE_VERSION`].
    pub version_name: &'static str,
    /// `true` when every step applied. `false` leaves `doc` unchanged.
    pub ok: bool,
    /// Steps applied since this document was parsed. Parsing starts at 0.
    pub version: usize,
    /// `"gfm"` or `"plain"`.
    pub kind: &'static str,
    /// ProseMirror JSON, with default attributes omitted.
    pub doc: Value,
    /// Concatenated text content. Not the source Markdown.
    pub text: String,
    /// GFM serialization of `doc`. `None` for plain text, which is not Markdown.
    pub markdown: Option<String>,
}

/// Parse `source` into an editor document. Does not apply steps.
pub fn open(kind: SourceKind, source: &str) -> Result<DocumentEngine, String> {
    project(kind, source, None)
}

/// Parse `source`, then apply `steps_json` (a JSON array of ProseMirror steps).
///
/// An empty array is a no-op. A step that does not apply returns `ok: false`
/// and the document from before the batch. Invalid JSON is an error.
pub fn apply_steps(
    kind: SourceKind,
    source: &str,
    steps_json: &str,
) -> Result<DocumentEngine, String> {
    let trimmed = steps_json.trim();
    if trimmed.is_empty() {
        return Err("steps must be a JSON array".into());
    }
    project(kind, source, Some(trimmed))
}

fn project(
    kind: SourceKind,
    source: &str,
    steps_json: Option<&str>,
) -> Result<DocumentEngine, String> {
    let (mut editor, kind_name, remark) = session(kind, source)?;
    let mut ok = true;
    if let Some(steps_json) = steps_json {
        ok = editor.apply_steps_json(steps_json)?;
    }
    snapshot(editor, kind_name, remark, ok)
}

fn session(
    kind: SourceKind,
    source: &str,
) -> Result<(Editor, &'static str, Option<Remark>), String> {
    match kind {
        SourceKind::Markdown => {
            let remark = Remark::gfm();
            let parsed = remark.parse(source);
            let doc_json = serde_json::to_string(&parsed.to_json())
                .map_err(|err| format!("document JSON: {err}"))?;
            let editor = Editor::new(&remark.schema_json(), &doc_json)?;
            Ok((editor, "gfm", Some(remark)))
        }
        SourceKind::PlainText => {
            let editor = Editor::new(PLAIN_SCHEMA, &plain_doc_json(source)?)?;
            Ok((editor, "plain", None))
        }
    }
}

fn snapshot(
    editor: Editor,
    kind_name: &'static str,
    remark: Option<Remark>,
    ok: bool,
) -> Result<DocumentEngine, String> {
    let text = editor.schema().with_types(|| editor.doc().text_content());
    let doc = serde_json::from_str(&editor.doc_json(true)?)
        .map_err(|err| format!("document JSON: {err}"))?;
    let markdown = remark.map(|remark| remark.serialize(editor.doc()));
    Ok(DocumentEngine {
        engine: ENGINE_NAME,
        version_name: ENGINE_VERSION,
        ok,
        version: editor.version(),
        kind: kind_name,
        doc,
        text,
        markdown,
    })
}

fn plain_doc_json(source: &str) -> Result<String, String> {
    let mut content: Vec<Value> = Vec::new();
    if source.is_empty() {
        content.push(serde_json::json!({ "type": "paragraph" }));
    } else {
        for line in source.lines() {
            if line.is_empty() {
                content.push(serde_json::json!({ "type": "paragraph" }));
            } else {
                content.push(serde_json::json!({
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": line }]
                }));
            }
        }
        if content.is_empty() {
            content.push(serde_json::json!({ "type": "paragraph" }));
        }
    }
    serde_json::to_string(&serde_json::json!({
        "type": "doc",
        "content": content
    }))
    .map_err(|err| format!("plain document JSON: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gfm_bold_round_trips_through_the_editor() {
        let doc = open(SourceKind::Markdown, "**bold**\n").expect("parse");
        assert_eq!(doc.engine, "prosemirror-rs");
        assert_eq!(doc.kind, "gfm");
        assert!(doc.ok);
        assert_eq!(doc.version, 0);
        assert_eq!(doc.text, "bold");
        assert_eq!(doc.markdown.as_deref(), Some("**bold**\n"));
        let rendered = serde_json::to_string(&doc.doc).unwrap();
        assert!(rendered.contains("strong"), "{rendered}");
    }

    #[test]
    fn gfm_task_list_keeps_the_checkbox() {
        let doc = open(SourceKind::Markdown, "- [x] done\n").expect("parse");
        assert_eq!(doc.markdown.as_deref(), Some("* [x] done\n"));
        assert!(doc.text.contains("done"));
    }

    #[test]
    fn empty_markdown_is_one_paragraph() {
        let doc = open(SourceKind::Markdown, "").expect("parse");
        assert_eq!(doc.text, "");
        assert_eq!(doc.markdown.as_deref(), Some("\n"));
    }

    #[test]
    fn plain_text_does_not_interpret_markdown() {
        let doc = open(SourceKind::PlainText, "**not bold**\n").expect("parse");
        assert_eq!(doc.kind, "plain");
        assert_eq!(doc.text, "**not bold**");
        assert!(doc.markdown.is_none());
        let rendered = serde_json::to_string(&doc.doc).unwrap();
        assert!(!rendered.contains("strong"), "{rendered}");
    }

    #[test]
    fn replace_step_types_through_the_editor() {
        let doc = apply_steps(
            SourceKind::Markdown,
            "Hi\n",
            r#"[{"stepType":"replace","from":3,"to":3,"slice":{"content":[{"type":"text","text":"!"}]}}]"#,
        )
        .expect("apply");
        assert!(doc.ok);
        assert_eq!(doc.version, 1);
        assert_eq!(doc.text, "Hi!");
        assert_eq!(doc.markdown.as_deref(), Some("Hi!\n"));
    }

    #[test]
    fn rejected_step_does_not_change_the_document() {
        let doc = apply_steps(
            SourceKind::Markdown,
            "Hi\n",
            r#"[{"stepType":"replace","from":9999,"to":9999,"slice":{"content":[{"type":"text","text":"x"}]}}]"#,
        )
        .expect("apply");
        assert!(!doc.ok);
        assert_eq!(doc.version, 0);
        assert_eq!(doc.text, "Hi");
    }

    #[test]
    fn invalid_steps_are_an_error() {
        let err = apply_steps(SourceKind::Markdown, "Hi\n", "not-json").unwrap_err();
        assert!(err.contains("Invalid steps JSON"), "{err}");
    }
}
