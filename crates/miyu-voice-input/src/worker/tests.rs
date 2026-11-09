use super::{decode, encode};
use crate::recognize::{Heard, Refused};

#[test]
fn answers_round_trip_between_the_two_processes() {
    let heard = Heard {
        text: "开会".to_string(),
        language: "zh".to_string(),
        duration_ms: 1200,
        clear: true,
    };
    assert_eq!(decode(&encode(Ok(heard.clone()))), Ok(heard));
    for refused in [
        Refused::TooLong,
        Refused::Language("xx".to_string()),
        Refused::Engine("broken".to_string()),
    ] {
        assert_eq!(decode(&encode(Err(refused.clone()))), Err(refused));
    }
}
