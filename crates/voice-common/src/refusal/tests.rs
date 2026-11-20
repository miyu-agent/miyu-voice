use miyu_ext::CoreError;
use serde_json::json;

use super::why;

fn refused(reason: &str, message: &str, data: serde_json::Value) -> CoreError {
    CoreError {
        code: Some(-32000),
        reason: Some(reason.to_string()),
        message: message.to_string(),
        data,
    }
}

/// 引擎回了错：照它说的写，不写核心那句「方法执行失败。」（主人 11-20 验收第 6 条）。
#[test]
fn a_program_s_own_words_replace_method_failed() {
    let failed = refused(
        "method_failed",
        "方法执行失败。",
        json!({"reason": "method_failed", "code": "engine_failed",
               "message": "DeepSeek text to speech failed (402: the DeepSeek balance is too low): Insufficient Balance"}),
    );
    assert_eq!(
        why(&failed),
        "DeepSeek text to speech failed (402: the DeepSeek balance is too low): Insufficient Balance"
    );
}

/// 没有程序说的话（核心自己拒的）照旧写原因码和核心的话。
#[test]
fn other_refusals_keep_the_reason_and_the_core_s_words() {
    let busy = refused(
        "program_not_running",
        "Program not running.",
        json!({"reason": "program_not_running"}),
    );
    assert_eq!(why(&busy), "program_not_running: Program not running.");
    let empty = refused(
        "method_failed",
        "Method failed.",
        json!({"reason": "method_failed", "message": " "}),
    );
    assert_eq!(why(&empty), "method_failed: Method failed.");
}
