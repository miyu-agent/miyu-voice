//! 人敲的 `miyu say …` 的参数：读成要做的一件事，连核心以前就分好（`--help` 不连核心、不合成）。

/// 用法，`--help` 印到标准输出，参数不对的印到标准错误。
pub const USAGE: &str = "usage: miyu say <text>            read it aloud in the current voice
       miyu say -o FILE <text>    save it to FILE (.mp3 or .wav) instead
       miyu say --file AUDIO      play an audio file
       miyu say --voices          list the engine's voices
       miyu say --help            show this";

/// 参数读出来要做的事。
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// 印用法。
    Help,
    /// 列音色。
    Voices,
    /// 放一个音频文件。
    File(String),
    /// 合成，存进文件。
    Save {
        /// 要念的字。
        text: String,
        /// 存到哪。
        out: String,
    },
    /// 合成，放出来。
    Play(String),
}

/// 读参数。
///
/// # Errors
///
/// 没有要念的字；`-o`、`--file` 后面没跟路径。
pub fn parse(args: &[String]) -> Result<Command, String> {
    if matches!(args, [only] if only == "help")
        || options(args).any(|arg| matches!(arg, "-h" | "--help"))
    {
        return Ok(Command::Help);
    }
    // 跟路径的选项在最后、后面没东西的报错（审计 #126：原来 `-o` 没跟路径悄悄外放）。
    if let Some(name @ ("-o" | "--file")) = args.last().map(String::as_str)
        && options(args).last() == Some(name)
    {
        return Err(format!("{name} needs a path.\n{USAGE}"));
    }
    if args.iter().any(|arg| arg == "--voices") {
        return Ok(Command::Voices);
    }
    if let Some(file) = option(args, "--file") {
        return Ok(Command::File(file.to_string()));
    }
    let text = words(args);
    if text.is_empty() {
        return Err(USAGE.to_string());
    }
    Ok(match option(args, "-o") {
        Some(out) => Command::Save {
            text,
            out: out.to_string(),
        },
        None => Command::Play(text),
    })
}

/// 不是 `-o`、`--file` 的值的那些参数（`-o --help` 的 `--help` 是文件名）。
fn options(args: &[String]) -> impl Iterator<Item = &str> {
    let mut value = false;
    args.iter().filter_map(move |arg| {
        let skipped = std::mem::replace(&mut value, matches!(arg.as_str(), "-o" | "--file"));
        (!skipped).then_some(arg.as_str())
    })
}

/// `--name value` 的 value。
fn option<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let at = args.iter().position(|arg| arg == name)?;
    args.get(at + 1).map(String::as_str)
}

/// 不是选项、也不是选项的值的那些词，拼成要念的字。
fn words(args: &[String]) -> String {
    let mut out = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
        } else if matches!(arg.as_str(), "-o" | "--file") {
            skip = true;
        } else if arg != "--voices" {
            out.push(arg.as_str());
        }
    }
    out.join(" ")
}

#[cfg(test)]
mod tests;
