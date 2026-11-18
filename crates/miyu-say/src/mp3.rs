//! 几段 mp3 接成一段之前去掉每段开头的「头」（审计 #126）：`ID3v2` 标签、第一帧的 Xing/Info/VBRI 头帧。头帧里写着这一段有
//! 几帧、多长，硬拼的话播放器照第一段的头算总长，播完第一段就停；后面几段的标签夹在中间也会让有的播放器停下。
//!
//! 只认得出来的才去，认不出的照原样留着（不是 mp3、头坏了的不动）。

/// `ID3v2` 标签头多长。
const ID3_HEADER: usize = 10;

/// 一段 mp3 去掉开头的 `ID3v2` 标签和 Xing/Info/VBRI 头帧以后剩下的。
#[must_use]
pub fn bare(bytes: &[u8]) -> &[u8] {
    let mut rest = bytes;
    if let Some(tag) = id3v2_len(rest).filter(|tag| *tag <= rest.len()) {
        rest = &rest[tag..];
    }
    if let Some(frame) = info_frame_len(rest) {
        rest = &rest[frame..];
    }
    rest
}

/// 开头是 `ID3v2` 标签的，交回整个标签多长（头 + 内容，带尾巴的加尾巴）。
fn id3v2_len(bytes: &[u8]) -> Option<usize> {
    let header = bytes.get(..ID3_HEADER)?;
    if &header[..3] != b"ID3" || header[3] == 0xFF || header[4] == 0xFF {
        return None;
    }
    // 长度是 4 个「同步安全」字节：每个只用低 7 位。
    let size = header[6..10].iter().try_fold(0usize, |size, &byte| {
        (byte < 0x80).then_some((size << 7) | usize::from(byte))
    })?;
    let footer = if header[5] & 0x10 == 0 { 0 } else { ID3_HEADER };
    Some(ID3_HEADER + size + footer)
}

/// 开头一帧是 Xing/Info/VBRI 头帧、整帧都在的，交回这一帧多长。
fn info_frame_len(bytes: &[u8]) -> Option<usize> {
    let frame = Frame::parse(bytes)?;
    let data = bytes.get(..frame.len)?;
    let tagged = |at: usize, tags: &[&[u8; 4]]| {
        data.get(at..at + 4)
            .is_some_and(|found| tags.iter().any(|tag| found == *tag))
    };
    (tagged(4 + frame.side_info, &[b"Xing", b"Info"]) || tagged(4 + 32, &[b"VBRI"]))
        .then_some(frame.len)
}

/// 一个 MPEG 音频帧头读出来的：整帧多长、帧头后面的边信息多长（Xing/Info 紧跟在边信息后面）。
struct Frame {
    len: usize,
    side_info: usize,
}

impl Frame {
    fn parse(bytes: &[u8]) -> Option<Frame> {
        let header = bytes.get(..4)?;
        if header[0] != 0xFF || header[1] & 0xE0 != 0xE0 {
            return None;
        }
        // 版本：3 是 MPEG-1，2 是 MPEG-2，0 是 MPEG-2.5，1 保留。层：3 是 I，2 是 II，1 是 III，0 保留。
        let version = (header[1] >> 3) & 0b11;
        let layer = (header[1] >> 1) & 0b11;
        let bitrate_index = usize::from(header[2] >> 4);
        let rate_index = usize::from((header[2] >> 2) & 0b11);
        let padding = usize::from((header[2] >> 1) & 1);
        let mono = header[3] >> 6 == 0b11;
        if version == 1
            || layer == 0
            || bitrate_index == 0
            || bitrate_index == 15
            || rate_index == 3
        {
            return None;
        }
        let mpeg1 = version == 3;
        let kbps: usize = match (mpeg1, layer) {
            (true, 3) => [
                0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448,
            ],
            (true, 2) => [
                0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384,
            ],
            (true, _) => [
                0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
            ],
            (false, 3) => [
                0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256,
            ],
            (false, _) => [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
        }[bitrate_index];
        let rate: usize = match version {
            3 => [44_100, 48_000, 32_000],
            2 => [22_050, 24_000, 16_000],
            _ => [11_025, 12_000, 8_000],
        }[rate_index];
        let bits = kbps * 1000;
        let len = match layer {
            3 => (12 * bits / rate + padding) * 4,
            1 if !mpeg1 => 72 * bits / rate + padding,
            _ => 144 * bits / rate + padding,
        };
        let side_info = match (mpeg1, mono) {
            (true, false) => 32,
            (true, true) | (false, false) => 17,
            (false, true) => 9,
        };
        Some(Frame { len, side_info })
    }
}

#[cfg(test)]
pub(crate) mod tests;
