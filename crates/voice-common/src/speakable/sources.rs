//! 末尾的参考链接那一段不念（todolist 第 14 条，规则同网页 `web-demo/packages/voice/readable.js` 的 `withoutSources`，W-1205）：
//! 从末尾往上只有链接的几行（列表、裸网址、`[1]: 网址` 这种），连同它上面那一行「参考链接」「来源」「Sources」这类小标题；
//! 有小标题的，带说明的链接行也一起不念。整段回答都是链接的照念（问的就是链接）。按行看原文，在交给 Markdown 解析以前。

/// 去掉末尾的参考链接那一段；没有的原样交回。
pub(super) fn without(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let end = lines.len();
    // 从末尾往上、只有链接的那几行从哪一行起（`end` 是还没有）。
    let mut pure = end;
    let mut all_pure = true;
    let mut cut = None;
    let mut j = end;
    while j > 0 {
        let line = lines[j - 1];
        if line.trim().is_empty() || is_rule(line) {
            j -= 1;
            continue;
        }
        if is_fence(line) || !has_link(line) {
            break;
        }
        // 一行既是小标题又带链接（「来源：[甲](…)、[乙](…)」）：这一段从这里起。
        if is_label(line) {
            cut = Some(j - 1);
            break;
        }
        if all_pure && is_pure(line) {
            pure = j - 1;
        } else {
            all_pure = false;
        }
        j -= 1;
    }
    if cut.is_none() && j > 0 && is_label(lines[j - 1]) && lines[j..].iter().any(|l| has_link(l)) {
        cut = Some(j - 1);
    }
    let Some(cut) = cut.or((pure < end).then_some(pure)) else {
        return text.to_string();
    };
    // 这一段上面紧挨着的空行、分隔线一起去掉。
    let mut keep = cut;
    while keep > 0 && (lines[keep - 1].trim().is_empty() || is_rule(lines[keep - 1])) {
        keep -= 1;
    }
    lines[..keep].join("\n")
}

/// 开头最多三个空格。
fn indented(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches(' ');
    (line.len() - rest.len() <= 3).then_some(rest)
}

/// 分隔线：三个以上同一种 `-`、`*`、`_`，中间可以有空白。
fn is_rule(line: &str) -> bool {
    let Some(rest) = indented(line) else {
        return false;
    };
    let marks: Vec<char> = rest.chars().filter(|c| !c.is_whitespace()).collect();
    marks.len() >= 3 && matches!(marks[0], '-' | '*' | '_') && marks.iter().all(|c| *c == marks[0])
}

/// 围起来的代码块的开头、收尾。
fn is_fence(line: &str) -> bool {
    indented(line).is_some_and(|rest| rest.starts_with("```") || rest.starts_with("~~~"))
}

/// `[1]: 网址` 这种链接定义（`[^1]: …` 脚注定义也算）。
fn is_definition(line: &str) -> bool {
    let Some(rest) = indented(line).and_then(|rest| rest.strip_prefix('[')) else {
        return false;
    };
    let Some((label, after)) = rest.split_once("]:") else {
        return false;
    };
    !label.is_empty() && !label.contains(']') && !after.trim().is_empty()
}

/// 一行里的链接、网址、脚注角标都换成空格，交回剩下的和换掉了几个链接、网址。
fn strip_links(line: &str) -> (String, usize) {
    let mut out = String::new();
    let mut found = 0;
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        if let Some(after) = link_end(rest) {
            found += 1;
            out.push(' ');
            rest = after;
        } else if let Some(after) = autolink_end(rest) {
            found += 1;
            out.push(' ');
            rest = after;
        } else if let Some(len) = super::bare_url_len(rest) {
            found += 1;
            out.push(' ');
            rest = &rest[len..];
        } else if let Some(after) = footnote_end(rest) {
            out.push(' ');
            rest = after;
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    (out, found)
}

/// `[字](网址)`、`![替代文字](网址)` 打头的，交回它后面的。
fn link_end(text: &str) -> Option<&str> {
    let text = text.strip_prefix('!').unwrap_or(text).strip_prefix('[')?;
    let close = text.find(']')?;
    if text[..close].contains('\n') {
        return None;
    }
    let after = text[close + 1..].strip_prefix('(')?;
    let end = after.find(')')?;
    Some(&after[end + 1..])
}

/// `<https://…>` 打头的，交回它后面的。
fn autolink_end(text: &str) -> Option<&str> {
    let inner = text.strip_prefix('<')?;
    if !(inner.starts_with("http://") || inner.starts_with("https://")) {
        return None;
    }
    let end = inner.find(|c: char| c == '>' || c.is_whitespace())?;
    inner[end..].strip_prefix('>')
}

/// `[^1]` 脚注角标打头的，交回它后面的。
pub(super) fn footnote_end(text: &str) -> Option<&str> {
    let inner = text.strip_prefix("[^")?;
    let end = inner.find(']')?;
    (!inner[..end].contains('\n')).then(|| &inner[end + 1..])
}

/// 这一行带不带链接、网址。
fn has_link(line: &str) -> bool {
    is_definition(line) || strip_links(line).1 > 0
}

/// 拿掉链接、网址、角标、列表记号、引用记号以后剩下的字（只留字母、数字，小写）。
fn residual(line: &str) -> String {
    if is_definition(line) {
        return String::new();
    }
    let (rest, _) = strip_links(line);
    let mut rest = rest.trim_start();
    while let Some(after) = rest.strip_prefix('>') {
        rest = after.trim_start();
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let marker = if rest.starts_with(['-', '*', '+']) {
        1
    } else if digits > 0 && rest[digits..].starts_with(['.', ')']) {
        digits + 1
    } else {
        0
    };
    if marker > 0 && rest[marker..].starts_with(char::is_whitespace) {
        rest = &rest[marker..];
    }
    rest.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// 只有链接的一行（列表的一项、一个裸网址、一行几个链接）：拿掉链接以后不剩字，编号不算。
fn is_pure(line: &str) -> bool {
    residual(line).chars().all(|c| c.is_ascii_digit())
}

/// 「参考链接」「来源：」「## Sources」这种小标题（不带链接的那几个字整个就是它）。
fn is_label(line: &str) -> bool {
    let words: String = residual(line)
        .chars()
        .filter(|c| !c.is_ascii_digit())
        .collect();
    !words.is_empty() && words.chars().count() <= 20 && (label_zh(&words) || label_other(&words))
}

/// 中文的小标题：可有的前缀 + 一个说法 + 可有的后缀，整个就是它（同 `readable.js` 的 `LABEL_ZH`）。
fn label_zh(words: &str) -> bool {
    const PREFIXES: [&str; 18] = [
        "",
        "以下",
        "以下是",
        "附",
        "附上",
        "主要",
        "相关",
        "更多",
        "延伸",
        "扩展",
        "信息",
        "资料",
        "数据",
        "搜索",
        "搜索结果",
        "网页",
        "原文",
        "参考",
    ];
    const CORES: [&str; 10] = [
        "参考", "链接", "资料", "来源", "出处", "文献", "阅读", "网址", "引用", "原文",
    ];
    const SUFFIXES: [&str; 7] = ["", "来源", "链接", "资料", "文献", "列表", "如下"];
    PREFIXES.iter().any(|prefix| {
        words.strip_prefix(prefix).is_some_and(|rest| {
            CORES.iter().any(|core| {
                rest.strip_prefix(core)
                    .is_some_and(|tail| SUFFIXES.contains(&tail))
            })
        })
    })
}

/// 英文、日文的小标题（同 `readable.js` 的 `LABEL_OTHER`）。
fn label_other(words: &str) -> bool {
    const PLURAL: [&str; 6] = [
        "source",
        "reference",
        "link",
        "citation",
        "relatedlink",
        "usefullink",
    ];
    const EXACT: [&str; 11] = [
        "furtherreading",
        "readmore",
        "learnmore",
        "seealso",
        "bibliography",
        "参考文献",
        "出典",
        "参考リンク",
        "リンク",
        "情報源",
        "ソース",
    ];
    EXACT.contains(&words)
        || PLURAL
            .iter()
            .any(|word| words == *word || words.strip_suffix('s') == Some(word))
}
