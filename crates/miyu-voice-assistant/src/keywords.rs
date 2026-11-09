//! 唤醒词编码成唤醒模型的一行（设计 20 第九节「怎么叫她」）：wenetspeech 的 KWS 模型的建模单元是「声母 + 带调韵母」
//! （`m ì y ǒu @密友`）。每个字的带调全拼照模型 `tokens.txt` 里真有的单元切，不硬写汉语音系，词表换了也跟着走。
//! 照旧版 0.7.4 的写法重写（「密友密友」实测 5/5）。
//!
//! | 写法 | 例子 | 怎么编 |
//! |---|---|---|
//! | 汉字 | `未有未有` | 每个字注带调拼音，一行 |
//! | 假名、拉丁字母 | `みゆみゆ`、`miyumiyu` | 假名先转罗马音，再照日语音节切、换成最近的拼音；不知道声调，轻声和一到四声各编一行，哪行中了都算 |
//! | 写明拼音 | `mi1 yu2 mi1 yu2` | 空格分开的「拼音+声调数字」（0、5 或不写是轻声），照写的编 |
//!
//! 一个词里能混写（`小miyu`）：汉字定调，其余展开声调。模型是普通话的，非中文只是近似发音。

use std::collections::HashSet;

use pinyin::ToPinyin;

/// 声母，长的在前（zh、ch、sh 先于 z、c、s）。
const INITIALS: [&str; 23] = [
    "zh", "ch", "sh", "b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q", "x", "r",
    "z", "c", "s", "y", "w",
];

/// 不知道声调的音节展开的先后：一到四声，0 是轻声。
const TONES: [u8; 5] = [1, 2, 3, 4, 0];

/// 日语音节（罗马音）→ 最近的拼音，候选照先后试，第一个词表里有的算。拉丁字母也走这张表（`miyu` → mi yu）。
const ROMAJI: &[(&str, &[&str])] = &[
    ("shi", &["xi"]),
    ("chi", &["qi"]),
    ("tsu", &["cu"]),
    ("kya", &["qia"]),
    ("kyu", &["qiu"]),
    ("kyo", &["qiu"]),
    ("sha", &["xia"]),
    ("shu", &["xiu"]),
    ("sho", &["xiu"]),
    ("cha", &["qia"]),
    ("chu", &["qiu"]),
    ("cho", &["qiu"]),
    ("hya", &["xia"]),
    ("hyu", &["xiu"]),
    ("hyo", &["xiu"]),
    ("rya", &["lia"]),
    ("ryu", &["liu"]),
    ("ryo", &["liu"]),
    ("gya", &["jia"]),
    ("gyu", &["jiu"]),
    ("gyo", &["jiu"]),
    ("nya", &["nia"]),
    ("nyu", &["niu"]),
    ("nyo", &["niu"]),
    ("mya", &["mia"]),
    ("myu", &["miu"]),
    ("myo", &["miu"]),
    ("bya", &["bia"]),
    ("byu", &["biu"]),
    ("byo", &["biu"]),
    ("pya", &["pia"]),
    ("pyu", &["piu"]),
    ("pyo", &["piu"]),
    ("ka", &["ka"]),
    ("ki", &["qi"]),
    ("ku", &["ku"]),
    ("ke", &["kei", "ke"]),
    ("ko", &["kou", "ke"]),
    ("sa", &["sa"]),
    ("su", &["su"]),
    ("se", &["sei", "se"]),
    ("so", &["sou"]),
    ("ta", &["ta"]),
    ("te", &["tei", "te"]),
    ("to", &["tou"]),
    ("na", &["na"]),
    ("ni", &["ni"]),
    ("nu", &["nu"]),
    ("ne", &["nei", "ne"]),
    ("no", &["nou"]),
    ("ha", &["ha"]),
    ("hi", &["xi"]),
    ("fu", &["fu"]),
    ("he", &["hei", "he"]),
    ("ho", &["hou"]),
    ("ma", &["ma"]),
    ("mi", &["mi"]),
    ("mu", &["mu"]),
    ("me", &["mei", "me"]),
    ("mo", &["mou"]),
    ("ya", &["ya"]),
    ("yu", &["you", "yu"]),
    ("yo", &["you"]),
    ("ra", &["la"]),
    ("ri", &["li"]),
    ("ru", &["lu"]),
    ("re", &["lei", "le"]),
    ("ro", &["lou"]),
    ("la", &["la"]),
    ("li", &["li"]),
    ("lu", &["lu"]),
    ("le", &["lei", "le"]),
    ("lo", &["lou"]),
    ("wa", &["wa"]),
    ("wo", &["wo"]),
    ("wu", &["wu"]),
    ("wi", &["wei"]),
    ("we", &["wei"]),
    ("ga", &["ga"]),
    ("gi", &["ji"]),
    ("gu", &["gu"]),
    ("ge", &["gei", "ge"]),
    ("go", &["gou"]),
    ("za", &["za"]),
    ("ji", &["ji"]),
    ("zu", &["zu"]),
    ("ze", &["zei", "ze"]),
    ("zo", &["zou"]),
    ("da", &["da"]),
    ("di", &["di"]),
    ("du", &["du"]),
    ("de", &["dei", "de"]),
    ("do", &["dou"]),
    ("ba", &["ba"]),
    ("bi", &["bi"]),
    ("bu", &["bu"]),
    ("be", &["bei"]),
    ("bo", &["bo"]),
    ("pa", &["pa"]),
    ("pi", &["pi"]),
    ("pu", &["pu"]),
    ("pe", &["pei"]),
    ("po", &["po"]),
    ("ja", &["jia"]),
    ("ju", &["jiu"]),
    ("jo", &["jiu"]),
    ("xi", &["xi"]),
    ("qi", &["qi"]),
    ("yi", &["yi"]),
    ("a", &["a"]),
    ("i", &["yi"]),
    ("u", &["wu"]),
    ("e", &["e", "ei"]),
    ("o", &["wo", "ou"]),
    ("n", &["en"]),
];

/// 假名 → 罗马音，一对一段：假名紧接着它的罗马音。
const KANA: &str = "あa いi うu えe おo かka きki くku けke こko さsa しshi すsu せse そso たta ちchi つtsu てte とto \
    なna にni ぬnu ねne のno はha ひhi ふfu へhe ほho まma みmi むmu めme もmo やya ゆyu よyo らra りri るru れre ろro \
    わwa をwo んn がga ぎgi ぐgu げge ごgo ざza じji ずzu ぜze ぞzo だda ぢji づzu でde どdo ばba びbi ぶbu べbe ぼbo \
    ぱpa ぴpi ぷpu ぺpe ぽpo";

/// 标调：元音、它的一到四声。
const MARKS: [(char, [char; 4]); 6] = [
    ('a', ['ā', 'á', 'ǎ', 'à']),
    ('e', ['ē', 'é', 'ě', 'è']),
    ('i', ['ī', 'í', 'ǐ', 'ì']),
    ('o', ['ō', 'ó', 'ǒ', 'ò']),
    ('u', ['ū', 'ú', 'ǔ', 'ù']),
    ('ü', ['ǖ', 'ǘ', 'ǚ', 'ǜ']),
];

/// 编不了的原因，给人看的一句（配置页、日志）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadKeyword(pub String);

/// 唤醒模型的建模单元表（`tokens.txt` 每行第一段）。
#[must_use]
pub fn tokens(text: &str) -> HashSet<String> {
    text.lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

/// 把一个唤醒词编成唤醒模型认的几行：`单元 单元 … @原文`。汉字、写明拼音的一行；带假名、拉丁的照声调展开几行。
///
/// # Errors
///
/// 空的；有汉字、假名、拉丁字母以外的字；音节在词表里拼不出来。
pub fn encode(keyword: &str, tokens: &HashSet<String>) -> Result<Vec<String>, BadKeyword> {
    let keyword = keyword.trim();
    if keyword.is_empty() {
        return Err(BadKeyword("The wake word is empty.".to_string()));
    }
    let tag = keyword.split_whitespace().collect::<Vec<_>>().join("_");
    if let Some(syllables) = explicit_pinyin(keyword) {
        let mut parts = Vec::new();
        for (base, tone) in syllables {
            parts.extend(split(&mark(&base, tone), tokens)?);
        }
        return Ok(vec![format!("{} @{tag}", parts.join(" "))]);
    }
    let units = units(keyword)?;
    let toneless = units.iter().any(|unit| matches!(unit, Unit::Toneless(_)));
    let tones: &[u8] = if toneless { &TONES } else { &[0] };
    let mut lines = Vec::new();
    let mut last = None;
    for &tone in tones {
        let mut parts = Vec::new();
        let encoded = units.iter().try_for_each(|unit| {
            let syllable = match unit {
                Unit::Fixed(syllable) => syllable.clone(),
                Unit::Toneless(candidates) => candidates
                    .split('|')
                    .map(|candidate| mark(candidate, tone))
                    .collect::<Vec<_>>()
                    .join("|"),
            };
            parts.extend(split(&syllable, tokens)?);
            Ok::<(), BadKeyword>(())
        });
        match encoded {
            Ok(()) => {
                let line = format!("{} @{tag}", parts.join(" "));
                if !lines.contains(&line) {
                    lines.push(line);
                }
            }
            Err(error) => last = Some(error),
        }
    }
    if lines.is_empty() {
        return Err(
            last.unwrap_or_else(|| BadKeyword("The wake word cannot be encoded.".to_string()))
        );
    }
    Ok(lines)
}

/// 一个音节：汉字的带调全拼定了；假名、拉丁的不知道声调，是 `a|b` 形式的候选。
enum Unit {
    Fixed(String),
    Toneless(String),
}

/// 一个词逐字分成音节。
fn units(keyword: &str) -> Result<Vec<Unit>, BadKeyword> {
    let mut units = Vec::new();
    let mut latin = String::new();
    for ch in keyword.chars() {
        if let Some(pinyin) = ch.to_pinyin() {
            flush(&mut latin, &mut units)?;
            units.push(Unit::Fixed(pinyin.with_tone().to_string()));
        } else if let Some(kana) = kana(ch) {
            match kana {
                Kana::Syllable(text) => latin.push_str(text),
                // 拗音：前一个 i 段音节去掉 i 接 ya/yu/yo（ki+ya=kya，shi+ya=sha）。
                Kana::Small(suffix) => {
                    let trimmed = latin.trim_end_matches('i').len();
                    latin.truncate(trimmed);
                    if latin.ends_with("sh") || latin.ends_with("ch") || latin.ends_with('j') {
                        latin.push_str(&suffix[1..]);
                    } else {
                        latin.push_str(suffix);
                    }
                }
                Kana::Skip => {}
            }
        } else if ch.is_ascii_alphabetic() {
            latin.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() {
            flush(&mut latin, &mut units)?;
        } else if !matches!(ch, '-' | '\'' | 'ー' | '・') {
            return Err(BadKeyword(format!(
                "Wake words take Chinese characters, kana and Latin letters; \"{ch}\" can't be read."
            )));
        }
    }
    flush(&mut latin, &mut units)?;
    if units.is_empty() {
        return Err(BadKeyword("The wake word has nothing to say.".to_string()));
    }
    Ok(units)
}

/// 攒着的罗马音切成音节放进去。
fn flush(latin: &mut String, units: &mut Vec<Unit>) -> Result<(), BadKeyword> {
    let mut rest = latin.as_str();
    while !rest.is_empty() {
        let bytes = rest.as_bytes();
        // 双写的辅音（っ 的罗马音，如 tt）丢掉前一个。
        if bytes.len() >= 2 && bytes[0] == bytes[1] && !b"aeioun".contains(&bytes[0]) {
            rest = &rest[1..];
            continue;
        }
        let Some((romaji, candidates)) = ROMAJI
            .iter()
            .filter(|(romaji, _)| rest.starts_with(romaji))
            .max_by_key(|(romaji, _)| romaji.len())
        else {
            return Err(BadKeyword(format!(
                "\"{rest}\" can't be split into syllables (write Japanese romaji, e.g. miyu, shiro)."
            )));
        };
        units.push(Unit::Toneless(candidates.join("|")));
        rest = &rest[romaji.len()..];
    }
    latin.clear();
    Ok(())
}

enum Kana {
    Syllable(&'static str),
    /// 小写的 ゃゅょ：和前一个音节拼成拗音。
    Small(&'static str),
    /// 促音、小写元音：不单独发音。
    Skip,
}

/// 假名 → 罗马音（平假名、片假名同一张表）。
fn kana(ch: char) -> Option<Kana> {
    let code = u32::from(ch);
    // 片假名折到平假名（U+30A1..U+30F6 ↔ U+3041..U+3096）。
    let ch = if (0x30A1..=0x30F6).contains(&code) {
        char::from_u32(code - 0x60).unwrap_or(ch)
    } else {
        ch
    };
    match ch {
        'ゃ' => return Some(Kana::Small("ya")),
        'ゅ' => return Some(Kana::Small("yu")),
        'ょ' => return Some(Kana::Small("yo")),
        'っ' | 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'ゎ' => return Some(Kana::Skip),
        _ => {}
    }
    KANA.split_whitespace().find_map(|pair| {
        let mut chars = pair.chars();
        (chars.next() == Some(ch)).then(|| Kana::Syllable(&pair[ch.len_utf8()..]))
    })
}

/// 写明拼音的写法：全是 ASCII、有空格或声调数字，每段 `拼音[0-5]`。交回（不带调的音节，声调）。
fn explicit_pinyin(keyword: &str) -> Option<Vec<(String, u8)>> {
    if !keyword.is_ascii()
        || (!keyword.contains(' ') && !keyword.chars().any(|c| c.is_ascii_digit()))
    {
        return None;
    }
    keyword
        .split_whitespace()
        .map(|part| {
            let (base, tone) = match part.char_indices().find(|(_, c)| c.is_ascii_digit()) {
                Some((at, digit)) => {
                    let tone = u8::try_from(digit.to_digit(10)?).ok()?;
                    if tone > 5 || at + 1 != part.len() {
                        return None;
                    }
                    (&part[..at], if tone == 5 { 0 } else { tone })
                }
                None => (part, 0),
            };
            (!base.is_empty() && base.chars().all(|c| c.is_ascii_lowercase()))
                .then(|| (base.replace('v', "ü"), tone))
        })
        .collect()
}

/// 给不带调的音节标调（0 不标）：有 a、e 标在 a、e 上；ou 标 o；别的标最后一个元音。
fn mark(base: &str, tone: u8) -> String {
    if tone == 0 {
        return base.to_string();
    }
    let chars: Vec<char> = base.chars().collect();
    let at = chars
        .iter()
        .position(|c| matches!(c, 'a' | 'e'))
        .or_else(|| base.find("ou").map(|byte| base[..byte].chars().count()))
        .or_else(|| {
            chars
                .iter()
                .rposition(|c| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'ü'))
        });
    let Some(at) = at else {
        return base.to_string();
    };
    let toned = MARKS
        .iter()
        .find(|(plain, _)| *plain == chars[at])
        .map_or(chars[at], |(_, marked)| marked[usize::from(tone - 1)]);
    chars
        .iter()
        .enumerate()
        .map(|(n, c)| if n == at { toned } else { *c })
        .collect()
}

/// 一个音节（可以是 `a|b` 候选）切成建模单元：声母 + 韵母，零声母的整个一个；候选照先后试。
fn split(syllable: &str, tokens: &HashSet<String>) -> Result<Vec<String>, BadKeyword> {
    for candidate in syllable.split('|') {
        for initial in INITIALS {
            if let Some(rest) = candidate.strip_prefix(initial)
                && !rest.is_empty()
                && tokens.contains(initial)
                && tokens.contains(rest)
            {
                return Ok(vec![initial.to_string(), rest.to_string()]);
            }
        }
        if tokens.contains(candidate) {
            return Ok(vec![candidate.to_string()]);
        }
    }
    Err(BadKeyword(format!(
        "\"{syllable}\" is not in the wake word model's vocabulary."
    )))
}

#[cfg(test)]
mod tests;
