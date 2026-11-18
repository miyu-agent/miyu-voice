use super::{Command, parse};

fn args(line: &[&str]) -> Vec<String> {
    line.iter().map(ToString::to_string).collect()
}

#[test]
fn text_options_and_voices_are_read() {
    assert_eq!(
        parse(&args(&["你好"])),
        Ok(Command::Play("你好".to_string()))
    );
    assert_eq!(
        parse(&args(&["-o", "a.mp3", "你", "好"])),
        Ok(Command::Save {
            text: "你 好".to_string(),
            out: "a.mp3".to_string()
        })
    );
    assert_eq!(
        parse(&args(&["--file", "a.wav"])),
        Ok(Command::File("a.wav".to_string()))
    );
    assert_eq!(parse(&args(&["--voices"])), Ok(Command::Voices));
    assert!(parse(&args(&[])).is_err());
}

/// `-o`、`--file` 后面没跟路径的报错，不悄悄外放（审计 #126）。
#[test]
fn an_option_without_its_path_is_refused() {
    let error = parse(&args(&["hi", "-o"])).expect_err("-o needs a path");
    assert!(error.contains("usage:"), "{error}");
    assert!(parse(&args(&["--file"])).is_err(), "--file needs a path");
}

/// `-h`、`--help`、只写一个 `help` 是看用法，不连核心、不拿去合成（审计 #126）；`help` 夹在话里照念。
#[test]
fn help_shows_the_usage() {
    for line in [&["--help"][..], &["-h"], &["help"], &["hi", "--help"]] {
        assert_eq!(parse(&args(line)), Ok(Command::Help), "{line:?}");
    }
    assert_eq!(
        parse(&args(&["help", "me"])),
        Ok(Command::Play("help me".to_string()))
    );
    assert_eq!(
        parse(&args(&["-o", "--help", "hi"])),
        Ok(Command::Save {
            text: "hi".to_string(),
            out: "--help".to_string()
        }),
        "a path is a path"
    );
}
