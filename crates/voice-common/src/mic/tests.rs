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
