use serde_json::json;

use super::only;

#[test]
fn unknown_fields_are_bad_params() {
    assert!(
        only(
            &json!({"text": "x", "persona": "p"}),
            &["text", "persona", "format"]
        )
        .is_ok()
    );
    let error = only(&json!({"txt": "x"}), &["text"]).unwrap_err();
    assert_eq!(error.code.as_deref(), Some("bad_params"));
    assert_eq!(error.message, "Unknown field txt.");
}
