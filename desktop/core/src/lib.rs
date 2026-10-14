//! Native document engine for the prosemirror-rs desktop shell.
//!
//! The editor UI keeps using the WebAssembly build of this same crate.
//! These functions are what the Tauri process calls, and the unit tests
//! below exercise the step types the basic and collaborative editors rely on.

use prosemirror::dynamic::types::Dyn;
use prosemirror::editor::Editor;
use prosemirror::model::Node;
use prosemirror::transform::{Mapping, Mappable, Step, StepMap};
use serde::Serialize;

pub const ENGINE_NAME: &str = "prosemirror-rs";
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

const SCHEMA: &str = r#"{
  "nodes": {
    "doc": {"content": "block+"},
    "paragraph": {"content": "inline*", "group": "block"},
    "blockquote": {"content": "block+", "group": "block", "defining": true},
    "horizontal_rule": {"group": "block"},
    "heading": {
      "attrs": {"level": {"default": 1}},
      "content": "inline*",
      "group": "block",
      "defining": true
    },
    "code_block": {
      "content": "text*",
      "marks": "",
      "group": "block",
      "code": true,
      "defining": true
    },
    "text": {"group": "inline"},
    "image": {
      "inline": true,
      "attrs": {"src": {}, "alt": {"default": null}, "title": {"default": null}},
      "group": "inline",
      "draggable": true
    },
    "hard_break": {"inline": true, "group": "inline"},
    "ordered_list": {
      "attrs": {"order": {"default": 1}},
      "content": "list_item+",
      "group": "block"
    },
    "bullet_list": {"content": "list_item+", "group": "block"},
    "list_item": {"content": "paragraph block*", "defining": true},
    "table": {"content": "table_row+", "group": "block", "isolating": true},
    "table_row": {"content": "(table_cell | table_header)*"},
    "table_cell": {
      "content": "block+",
      "isolating": true,
      "attrs": {
        "colspan": {"default": 1},
        "rowspan": {"default": 1},
        "colwidth": {"default": null}
      }
    },
    "table_header": {
      "content": "block+",
      "isolating": true,
      "attrs": {
        "colspan": {"default": 1},
        "rowspan": {"default": 1},
        "colwidth": {"default": null}
      }
    }
  },
  "marks": {
    "link": {"attrs": {"href": {}, "title": {"default": null}}, "inclusive": false},
    "em": {},
    "strong": {},
    "code": {}
  }
}"#;

#[derive(Debug, Clone, Serialize)]
pub struct EngineInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub tauri: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyOutcome {
    pub ok: bool,
    pub version: usize,
    pub doc_json: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfTestReport {
    pub passed: Vec<String>,
    pub engine: &'static str,
    pub version: &'static str,
}

pub fn engine_info() -> EngineInfo {
    EngineInfo {
        name: ENGINE_NAME,
        version: ENGINE_VERSION,
        tauri: "",
    }
}

pub fn apply_document(
    schema_json: &str,
    doc_json: &str,
    steps_json: &str,
) -> Result<ApplyOutcome, String> {
    let mut editor = Editor::new(schema_json, doc_json)?;
    let trimmed = steps_json.trim();
    let ok = if trimmed.is_empty() || trimmed == "[]" {
        true
    } else {
        editor.apply_steps_json(trimmed)?
    };
    let text = editor.schema().with_types(|| editor.doc().text_content());
    Ok(ApplyOutcome {
        ok,
        version: editor.version(),
        doc_json: editor.doc_json(false)?,
        text,
    })
}

pub fn self_test() -> Result<SelfTestReport, String> {
    let mut passed = Vec::new();

    let doc = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Hi"}]}]}"#;
    let mut editor = Editor::new(SCHEMA, doc)?;
    passed.push("schema-and-document".into());

    let typed = apply_document(
        SCHEMA,
        doc,
        r#"[{"stepType":"replace","from":3,"to":3,"slice":{"content":[{"type":"text","text":"!"}]}}]"#,
    )?;
    if !typed.ok || typed.text != "Hi!" || typed.version != 1 {
        return Err(format!("typing failed: {typed:?}"));
    }
    passed.push("replace-step-typing".into());

    let marked = apply_document(
        SCHEMA,
        doc,
        r#"[{"stepType":"addMark","mark":{"type":"strong"},"from":1,"to":3}]"#,
    )?;
    if !marked.ok || !marked.doc_json.contains("\"strong\"") {
        return Err(format!("addMark failed: {}", marked.doc_json));
    }
    passed.push("add-mark".into());

    let unmarked = apply_document(
        SCHEMA,
        &marked.doc_json,
        r#"[{"stepType":"removeMark","mark":{"type":"strong"},"from":1,"to":3}]"#,
    )?;
    if !unmarked.ok || unmarked.doc_json.contains("\"strong\"") {
        return Err(format!("removeMark failed: {}", unmarked.doc_json));
    }
    passed.push("remove-mark".into());

    let before = editor.doc_json(false)?;
    let rejected = editor.apply_step(
        r#"{"stepType":"replace","from":9999,"to":9999,"slice":{"content":[{"type":"text","text":"x"}]}}"#,
    )?;
    if rejected || editor.doc_json(false)? != before || editor.version() != 0 {
        return Err("rejected step mutated the document".into());
    }
    passed.push("rejected-step-is-atomic".into());

    editor.reset(doc)?;
    if editor.version() != 0 {
        return Err("reset did not clear the version".into());
    }
    passed.push("reset".into());

    let heading = r#"{"type":"doc","content":[{"type":"heading","attrs":{"level":1},"content":[{"type":"text","text":"Title"}]}]}"#;
    let headed = apply_document(
        SCHEMA,
        heading,
        r#"[{"stepType":"attr","pos":0,"attr":"level","value":2}]"#,
    )?;
    if !headed.ok || !headed.doc_json.contains("\"level\":2") {
        return Err(format!("attr step failed: {}", headed.doc_json));
    }
    passed.push("heading-attr".into());

    let quoted = apply_document(
        SCHEMA,
        r#"{"type":"doc","content":[{"type":"blockquote","content":[{"type":"paragraph","content":[{"type":"text","text":"quoted"}]}]}]}"#,
        "[]",
    )?;
    if quoted.text != "quoted" {
        return Err(format!("blockquote text: {}", quoted.text));
    }
    passed.push("blockquote".into());

    let listed = apply_document(
        SCHEMA,
        r#"{"type":"doc","content":[{"type":"bullet_list","content":[{"type":"list_item","content":[{"type":"paragraph","content":[{"type":"text","text":"one"}]}]},{"type":"list_item","content":[{"type":"paragraph","content":[{"type":"text","text":"two"}]}]}]}]}"#,
        "[]",
    )?;
    if listed.text != "onetwo" {
        return Err(format!("list text: {}", listed.text));
    }
    passed.push("bullet-list".into());

    let table = apply_document(
        SCHEMA,
        r#"{"type":"doc","content":[{"type":"table","content":[{"type":"table_row","content":[{"type":"table_header","content":[{"type":"paragraph","content":[{"type":"text","text":"A"}]}]},{"type":"table_header","content":[{"type":"paragraph","content":[{"type":"text","text":"B"}]}]}]},{"type":"table_row","content":[{"type":"table_cell","content":[{"type":"paragraph","content":[{"type":"text","text":"1"}]}]},{"type":"table_cell","content":[{"type":"paragraph","content":[{"type":"text","text":"2"}]}]}]}]}]}"#,
        "[]",
    )?;
    if table.text != "AB12" {
        return Err(format!("table text: {}", table.text));
    }
    passed.push("table".into());

    let code = apply_document(
        SCHEMA,
        r#"{"type":"doc","content":[{"type":"code_block","content":[{"type":"text","text":"let x = 1;"}]}]}"#,
        "[]",
    )?;
    if code.text != "let x = 1;" {
        return Err(format!("code block text: {}", code.text));
    }
    passed.push("code-block".into());

    // Undo: a replace step inverted and reapplied restores the original JSON.
    {
        let start = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Hi"}]}]}"#;
        let live = Editor::new(SCHEMA, start)?;
        let schema = std::sync::Arc::clone(live.schema());
        let step: Step<Dyn> = schema.with_types(|| {
            serde_json::from_str(
                r#"{"stepType":"replace","from":3,"to":3,"slice":{"content":[{"type":"text","text":"!"}]}}"#,
            )
            .map_err(|err| err.to_string())
        })?;
        let original = schema.with_types(|| live.doc().to_json(false));
        let restored = schema.with_types(|| {
            let current = live.doc();
            let next = step.apply(current).map_err(|err| err.to_string())?;
            let inverse = step.invert(current);
            let back = inverse.apply(&next).map_err(|err| err.to_string())?;
            Ok::<_, String>(back.to_json(false))
        })?;
        if restored != original {
            return Err("invert did not restore the document".into());
        }
        passed.push("undo-via-invert".into());
    }

    // Two editors replay the same steps, matching the collaborative demo.
    {
        let start = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Hi"}]}]}"#;
        let steps =
            r#"[{"stepType":"replace","from":3,"to":3,"slice":{"content":[{"type":"text","text":" there"}]}}]"#;
        let left = apply_document(SCHEMA, start, steps)?;
        let right = apply_document(SCHEMA, start, steps)?;
        if left.doc_json != right.doc_json || left.text != "Hi there" {
            return Err("collaborative replay diverged".into());
        }
        passed.push("collab-replay".into());
    }

    let insert = StepMap::new(vec![2, 0, 4]);
    if insert.map(0, 1) != 0 || insert.map(3, 1) != 7 {
        return Err("step map shifted positions incorrectly".into());
    }
    let mut mapping = Mapping::new();
    mapping.append_map(StepMap::new(vec![2, 0, 4]), None);
    mapping.append_map(StepMap::new(vec![10, 3, 0]), None);
    if mapping.map(3, 1) != 7 {
        return Err(format!("composed mapping returned {}", mapping.map(3, 1)));
    }
    passed.push("position-mapping".into());

    let _ = editor;
    Ok(SelfTestReport {
        passed,
        engine: ENGINE_NAME,
        version: ENGINE_VERSION,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_reports_the_crate_version() {
        let info = engine_info();
        assert_eq!(info.name, "prosemirror-rs");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert!(info.tauri.is_empty());
    }

    #[test]
    fn self_test_covers_the_editor_surface() {
        let report = self_test().expect("native engine self-test");
        assert!(report.passed.len() >= 12, "{:?}", report.passed);
        assert!(report.passed.iter().any(|name| name == "table"));
        assert!(report.passed.iter().any(|name| name == "collab-replay"));
        assert!(report.passed.iter().any(|name| name == "undo-via-invert"));
    }
}
