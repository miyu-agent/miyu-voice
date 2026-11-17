//! 社区引擎（核心 `docs/reference/bus.md` 的 `stt.engine`、`tts.engine`，施工 VO-29）：问总线这台机器上装着哪些接进来的包，
//! 给配置页「引擎」的下拉（核心 SG-2 的 `choices_from`）。装着的引擎包和本机的、云端的引擎排在同一张列表里，一项写包的名字
//! （`package.info` 照界面语言给的，例如「DeepSeek 功能全家桶」），不是包的编号（施工 VO-32，项目主人 11-17 验收第 9 条：
//! 「不要先选社区再选包」）。

use miyu_ext::Core;
use serde_json::{Value, json};

/// 接进来的一家。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugged {
    /// 包的编号（`bus.call` 的 `package` 写它）。
    pub package: String,
    /// 程序连着、方法都登记了。
    pub ready: bool,
    /// 包的名字（`package.info` 照连接的语言给的）；问不到的没有，下拉里写编号。
    pub name: Option<String>,
}

/// 接进 `name`（`stt.engine`、`tts.engine`）的包，照 `bus.list` 的先后；问不到的交回空的。
#[must_use]
pub fn listed(core: &Core, name: &str) -> Vec<Plugged> {
    let mut plugged = core
        .call("bus.list", json!({"name": name}))
        .map(|answer| listed_in(&answer, name))
        .unwrap_or_default();
    for one in &mut plugged {
        one.name = core
            .call(
                "package.info",
                json!({"package": one.package, "source": false}),
            )
            .ok()
            .and_then(|info| name_in(&info));
    }
    plugged
}

/// `package.info` 回应里的名字；没写、是空的交回空的。
#[must_use]
pub fn name_in(info: &Value) -> Option<String> {
    info["name"]
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
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
                name: None,
            })
        })
        .collect()
}

/// 下拉里的一项：值是包的编号，名字是包的名字（问不到的写编号）；没在运行的暗色写一句。`language` 是界面语言。
#[must_use]
pub fn choice(plugged: &Plugged, language: &str) -> Value {
    let name = plugged.name.as_deref().unwrap_or(&plugged.package);
    let mut choice = json!({"value": plugged.package, "name": name});
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
