use std::io::Cursor;

use super::{host, read_capped};

/// 超了上限的回应报错，不悄悄截断（审计 #126）；正好到上限的照收。
#[test]
fn answers_over_the_limit_are_refused_not_cut() {
    let limit = 2 * 1024 * 1024;
    let size = usize::try_from(limit).unwrap();
    let error = read_capped(Cursor::new(vec![0u8; size + 1]), limit, "api.example.com")
        .expect_err("one byte over the limit");
    assert_eq!(
        error.message(),
        "The answer from api.example.com is larger than 2 MB."
    );
    let whole = read_capped(Cursor::new(vec![0u8; size]), limit, "api.example.com").unwrap();
    assert_eq!(whole.len(), size);
}

/// 报错里的主机名不带网址里的账号密码（审计 #126）。
#[test]
fn the_host_drops_user_and_password() {
    assert_eq!(
        host("https://user:secret@tts.example.com/v1/audio"),
        "tts.example.com"
    );
    assert_eq!(host("http://127.0.0.1:8080/v1"), "127.0.0.1:8080");
    assert_eq!(host("https://api.example.com?key=1"), "api.example.com");
}
