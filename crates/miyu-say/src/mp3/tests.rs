use super::bare;

/// MPEG-1 第三层、128 kbps、44.1 kHz、立体声、不补位的一帧：417 字节。
const MPEG1_HEADER: [u8; 4] = [0xFF, 0xFB, 0x90, 0x00];
const MPEG1_LEN: usize = 417;

/// 一帧：帧头后面填 `fill`，`tag` 写在 `at` 那里。
pub(crate) fn frame(
    header: [u8; 4],
    len: usize,
    fill: u8,
    tag: Option<(usize, &[u8; 4])>,
) -> Vec<u8> {
    let mut frame = vec![fill; len];
    frame[..4].copy_from_slice(&header);
    if let Some((at, tag)) = tag {
        frame[at..at + 4].copy_from_slice(tag);
    }
    frame
}

/// 一个 ID3v2.4 标签，内容 `size` 个字节（长度照同步安全写）。
pub(crate) fn id3(size: usize) -> Vec<u8> {
    let mut tag = b"ID3\x04\x00\x00".to_vec();
    tag.extend([21, 14, 7, 0].map(|shift| u8::try_from((size >> shift) & 0x7F).unwrap()));
    tag.extend(std::iter::repeat_n(b'x', size));
    tag
}

#[test]
fn the_id3_tag_and_the_xing_frame_are_taken_off() {
    let audio = frame(MPEG1_HEADER, MPEG1_LEN, 1, None);
    let mut piece = id3(300);
    piece.extend(frame(MPEG1_HEADER, MPEG1_LEN, 0, Some((36, b"Xing"))));
    piece.extend(&audio);
    assert_eq!(bare(&piece), audio.as_slice());
    // VBRI 在帧头后面 32 字节，和声道无关。
    let mut piece = frame(MPEG1_HEADER, MPEG1_LEN, 0, Some((36, b"VBRI")));
    piece.extend(&audio);
    assert_eq!(bare(&piece), audio.as_slice());
}

/// MPEG-2 第三层、64 kbps、22.05 kHz、单声道：一帧 208 字节，Info 在帧头后面 9 字节的边信息之后。
#[test]
fn an_mpeg2_mono_info_frame_is_taken_off() {
    let header = [0xFF, 0xF3, 0x80, 0xC0];
    let audio = frame(header, 208, 1, None);
    let mut piece = frame(header, 208, 0, Some((13, b"Info")));
    piece.extend(&audio);
    assert_eq!(bare(&piece), audio.as_slice());
}

#[test]
fn what_is_not_recognised_is_left_alone() {
    assert_eq!(bare(b"ab"), b"ab");
    // 第一帧是普通的音频帧，不去。
    let audio = frame(MPEG1_HEADER, MPEG1_LEN, 1, None);
    assert_eq!(bare(&audio), audio.as_slice());
    // 头帧不完整（截断了）的不去。
    let short = frame(MPEG1_HEADER, MPEG1_LEN, 0, Some((36, b"Xing")));
    assert_eq!(bare(&short[..100]), &short[..100]);
    // 长度字节不是同步安全的不当 ID3。
    let mut odd = id3(4);
    odd[9] = 0x80;
    assert_eq!(bare(&odd), odd.as_slice());
}
