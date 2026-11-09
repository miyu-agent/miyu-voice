//! 长的声音切成一段段（设计 20 第三节「形状」：最长 10 分钟，长的由识别的一方切段再拼）：SenseVoice 一次认 30 秒以内
//! 的最好。每段在 20 到 30 秒之间找最安静的 0.1 秒切开，切不进字里；不到 30 秒的整段认。人声检测的模型归助手
//! （设计 20 第五节），这里只看音量就够。

use std::ops::Range;

/// 一段最长多少秒。
const LONGEST_S: usize = 30;
/// 一段至少多少秒才开始找切口。
const SHORTEST_S: usize = 20;
/// 比安静时看多长的窗（秒的十分之一）。
const WINDOW_TENTHS: usize = 1;

/// 切成几段，每段是样本的下标范围，首尾相接、盖住全部。
#[must_use]
pub fn split(samples: &[f32], rate: u32) -> Vec<Range<usize>> {
    let rate = rate as usize;
    let longest = LONGEST_S * rate;
    let shortest = SHORTEST_S * rate;
    let window = (WINDOW_TENTHS * rate / 10).max(1);
    let mut cuts = Vec::new();
    let mut start = 0;
    while samples.len() - start > longest {
        let from = start + shortest;
        let to = start + longest - window;
        let cut = quietest(samples, from, to, window);
        cuts.push(start..cut);
        start = cut;
    }
    cuts.push(start..samples.len());
    cuts
}

/// `from` 到 `to` 之间，`window` 长的窗里能量最小的那一处的中间。
fn quietest(samples: &[f32], from: usize, to: usize, window: usize) -> usize {
    let mut best = from;
    let mut best_energy = f32::INFINITY;
    let mut at = from;
    while at <= to {
        let energy: f32 = samples[at..at + window].iter().map(|s| s * s).sum();
        if energy < best_energy {
            best_energy = energy;
            best = at + window / 2;
        }
        at += window / 2;
    }
    best
}

#[cfg(test)]
mod tests;
