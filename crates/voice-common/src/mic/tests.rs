use super::{Found, Listing, pick};

fn found(list: &[(&str, &str, &str)]) -> Vec<Found> {
    list.iter()
        .map(|(id, name, driver)| Found {
            id: (*id).to_string(),
            name: (*name).to_string(),
            driver: (!driver.is_empty()).then(|| (*driver).to_string()),
        })
        .collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(value, name)| ((*value).to_string(), (*name).to_string()))
        .collect()
}

#[test]
fn sound_server_lists_sources_without_monitors() {
    let listed = found(&[
        (
            "alsa_output.pci-0000_66_00.6.analog-stereo.monitor",
            "Monitor of Ryzen HD Audio Controller",
            "",
        ),
        (
            "alsa_input.usb-fifine_Microphone-00.mono-fallback",
            "fifine Microphone 单声道",
            "",
        ),
        (
            "easyeffects_sink.monitor",
            "Monitor of Easy Effects Sink",
            "",
        ),
        ("easyeffects_source", "Easy Effects Source", ""),
        ("easyeffects_source", "Easy Effects Source", ""),
    ]);
    assert_eq!(
        pick(&listed, Listing::Server),
        pairs(&[
            (
                "alsa_input.usb-fifine_Microphone-00.mono-fallback",
                "fifine Microphone 单声道"
            ),
            ("easyeffects_source", "Easy Effects Source"),
        ])
    );
}

#[test]
fn alsa_keeps_one_entry_per_card() {
    let listed = found(&[
        (
            "null",
            "Discard all samples (playback) or generate zero samples (capture)",
            "null",
        ),
        ("sysdefault", "Default Audio Device", "sysdefault"),
        (
            "speexrate",
            "Rate Converter Plugin Using Speex Resampler",
            "speexrate",
        ),
        ("pipewire", "PipeWire Sound Server", "pipewire"),
        (
            "sysdefault:CARD=Microphone",
            "fifine Microphone, USB Audio",
            "sysdefault:CARD=Microphone",
        ),
        (
            "front:CARD=Microphone,DEV=0",
            "fifine Microphone, USB Audio",
            "front:CARD=Microphone,DEV=0",
        ),
        (
            "usbstream:CARD=Microphone",
            "fifine Microphone",
            "usbstream:CARD=Microphone",
        ),
        (
            "sysdefault:CARD=Generic_1",
            "HD-Audio Generic, ALC256 Analog",
            "sysdefault:CARD=Generic_1",
        ),
        (
            "hw:CARD=0,DEV=0",
            "fifine Microphone, USB Audio",
            "hw:CARD=0,DEV=0",
        ),
    ]);
    assert_eq!(
        pick(&listed, Listing::Alsa),
        pairs(&[
            (
                "fifine Microphone, USB Audio",
                "fifine Microphone, USB Audio"
            ),
            (
                "HD-Audio Generic, ALC256 Analog",
                "HD-Audio Generic, ALC256 Analog"
            ),
        ])
    );
}

#[test]
fn other_systems_keep_every_device_once() {
    let listed = found(&[
        ("1", "MacBook Pro Microphone", ""),
        ("2", "USB Mic", "coreaudio"),
        ("3", "USB Mic", ""),
        ("4", "", ""),
    ]);
    assert_eq!(
        pick(&listed, Listing::Plain),
        pairs(&[
            ("MacBook Pro Microphone", "MacBook Pro Microphone"),
            ("USB Mic", "USB Mic")
        ])
    );
}

/// 设备默认 24 位的（主人的 USB 麦克风经 PipeWire）：要得到 f32 的改要 f32，采样率、声道数照默认的（施工 VO-27）。
#[test]
fn a_24_bit_default_is_opened_as_f32_when_offered() {
    use cpal::{
        SampleFormat, SupportedBufferSize, SupportedStreamConfig, SupportedStreamConfigRange,
    };
    let buffer = SupportedBufferSize::Range { min: 1, max: 4096 };
    let default = SupportedStreamConfig::new(1, 48_000, buffer, SampleFormat::I24);
    let range = |channels, format| {
        SupportedStreamConfigRange::new(channels, 8_000, 192_000, buffer, format)
    };
    let offered = [
        range(2, SampleFormat::F32),
        range(1, SampleFormat::I16),
        range(1, SampleFormat::F32),
    ];
    let chosen = super::choose(default, &offered);
    assert_eq!(
        (
            chosen.sample_format(),
            chosen.sample_rate(),
            chosen.channels()
        ),
        (SampleFormat::F32, 48_000, 1)
    );
    // 没有 f32 可要的照默认的格式开（各种格式都收）。
    let kept = super::choose(default, &[range(1, SampleFormat::I16)]);
    assert_eq!(kept.sample_format(), SampleFormat::I24);
}

/// 24 位、无符号这些格式也平均成单声道 f32：满幅接近 1，零点是 0。
#[test]
fn every_sample_format_turns_into_mono_f32() {
    let full = cpal::I24::new((1 << 23) - 1).unwrap();
    let half = cpal::I24::new(1 << 22).unwrap();
    let zero = cpal::I24::new(0).unwrap();
    let got = super::mono(&[full, zero, half, half], 2);
    assert!(
        (got[0] - 0.5).abs() < 1e-3 && (got[1] - 0.5).abs() < 1e-3,
        "{got:?}"
    );
    let unsigned = super::mono(&[128u8, 255u8], 1);
    assert!(
        unsigned[0].abs() < 1e-6 && (unsigned[1] - 1.0).abs() < 0.01,
        "{unsigned:?}"
    );
}
