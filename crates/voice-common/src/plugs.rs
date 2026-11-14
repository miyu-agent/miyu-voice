//! 社区引擎（核心 `docs/reference/bus.md` 的 `stt.engine`、`tts.engine`，施工 VO-29）：问总线这台机器上装着哪些接进来的包，
//! 给配置页「引擎」的下拉（核心 SG-2 的 `choices_from`）。

use miyu_ext::Core;
use serde_json::{Value, json};

/// 接进来的一家。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugged {
    /// 包的编号（`bus.call` 的 `package` 写它）。
    pub package: String,
    /// 程序连着、方法都登记了。
    pub ready: bool,
}

/// 接进 `name`（`stt.engine`、`tts.engine`）的包，照 `bus.list` 的先后；问不到的交回空的。
#[must_use]
pub fn listed(core: &Core, name: &str) -> Vec<Plugged> {
    core.call("bus.list", json!({"name": name}))
        .map(|answer| listed_in(&answer, name))
        .unwrap_or_default()
}

/// 从 `bus.list` 的回应里挑出接进 `name` 的包。
#[must_use]
pub fn listed_in(answer: &Value, name: &str) -> Vec<Plugged> {
    answer["names"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["name"] == name && entry["kind"] == "plugs")
        .filter_map(|entry| {
            Some(Plugged {
                package: entry["package"].as_str()?.to_string(),
                ready: entry["ready"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

/// 下拉里的一项：值、名字都是包的编号；没在运行的暗色写一句。`language` 是界面语言。
#[must_use]
pub fn choice(plugged: &Plugged, language: &str) -> Value {
    let mut choice = json!({"value": plugged.package, "name": plugged.package});
    if !plugged.ready {
        choice["description"] = match language {
            "zh" => "没在运行",
            "ja" => "実行されていません",
            _ => "Not running",
        }
        .into();
    }
    choice
}

#[cfg(test)]
mod tests;
