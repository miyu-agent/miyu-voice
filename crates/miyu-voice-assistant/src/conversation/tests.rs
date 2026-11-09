use std::time::{Duration, Instant};

use super::Window;

#[test]
fn the_window_is_open_during_a_turn_and_for_a_while_after() {
    let mut window = Window::default();
    assert!(!window.open(), "closed at first");
    window.in_turn = true;
    assert!(window.open(), "she is thinking or talking: no timer");
    window.in_turn = false;
    window.until = Some(Instant::now() + Duration::from_secs(30));
    assert!(window.open(), "follow-up window");
    window.until = Instant::now().checked_sub(Duration::from_millis(1));
    assert!(!window.open(), "timed out");
}
