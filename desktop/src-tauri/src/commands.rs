use prosemirror_desktop_core::{
    apply_document as apply, engine_info as info, self_test as run_self_test, ApplyOutcome,
    EngineInfo, SelfTestReport,
};

#[tauri::command]
pub fn engine_info() -> EngineInfo {
    info()
}

#[tauri::command]
pub fn self_test() -> Result<SelfTestReport, String> {
    run_self_test()
}

#[tauri::command]
pub fn apply_document(
    schema_json: String,
    doc_json: String,
    steps_json: String,
) -> Result<ApplyOutcome, String> {
    apply(&schema_json, &doc_json, &steps_json)
}
