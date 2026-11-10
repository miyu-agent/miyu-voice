use super::pick;

fn found(list: &[(&str, &str)]) -> Vec<(String, Option<String>)> {
    list.iter()
        .map(|(name, driver)| ((*name).to_string(), Some((*driver).to_string())))
        .collect()
}

#[test]
fn alsa_keeps_one_entry_per_card() {
    let listed = found(&[
        (
            "Discard all samples (playback) or generate zero samples (capture)",
            "null",
        ),
        ("Default Audio Device", "sysdefault"),
        ("Rate Converter Plugin Using Speex Resampler", "speexrate"),
        ("PipeWire Sound Server", "pipewire"),
        ("fifine Microphone, USB Audio", "sysdefault:CARD=Microphone"),
        (
            "fifine Microphone, USB Audio",
            "front:CARD=Microphone,DEV=0",
        ),
        ("fifine Microphone", "usbstream:CARD=Microphone"),
        (
            "HD-Audio Generic, ALC256 Analog",
            "sysdefault:CARD=Generic_1",
        ),
        ("fifine Microphone, USB Audio", "hw:CARD=0,DEV=0"),
        ("fifine Microphone, USB Audio", "plughw:CARD=0,DEV=0"),
    ]);
    assert_eq!(
        pick(&listed, true),
        [
            "fifine Microphone, USB Audio",
            "HD-Audio Generic, ALC256 Analog"
        ]
    );
}

#[test]
fn other_systems_keep_every_device_once() {
    let listed = vec![
        ("MacBook Pro Microphone".to_string(), None),
        ("USB Mic".to_string(), Some("coreaudio".to_string())),
        ("USB Mic".to_string(), None),
        (String::new(), None),
    ];
    assert_eq!(pick(&listed, false), ["MacBook Pro Microphone", "USB Mic"]);
}
