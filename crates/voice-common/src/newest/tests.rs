use std::time::Duration;

use super::channel;

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "称重的函数照 channel 要的形状收引用"
)]
fn one(_: &u32) -> usize {
    1
}

#[test]
fn a_full_queue_drops_the_oldest_and_keeps_the_newest() {
    let (mut sender, receiver) = channel::<u32>(3, one, "test");
    for n in 0..10 {
        sender.send(n);
    }
    assert_eq!(receiver.queued(), 3);
    let kept: Vec<u32> = std::iter::from_fn(|| receiver.try_recv()).collect();
    assert_eq!(kept, [7, 8, 9]);
}

#[test]
fn weight_bounds_the_queue_but_the_newest_is_kept_even_when_heavier() {
    let (mut sender, receiver) = channel::<Vec<f32>>(100, Vec::len, "test");
    for n in 0..10u8 {
        sender.send(vec![f32::from(n); 30]);
    }
    assert_eq!(receiver.queued(), 90);
    sender.send(vec![0.5; 250]);
    assert_eq!(receiver.queued(), 250);
    assert_eq!(receiver.try_recv().map(|block| block.len()), Some(250));
    assert_eq!(receiver.try_recv(), None);
}

#[test]
fn without_waiting_holds_what_it_cannot_hand_over_and_hands_it_over_next_time() {
    let (mut sender, receiver) = channel::<u32>(3, one, "test");
    {
        // 收的那一头拿着锁（正在取一块）：交的这一头不等，先放在手里。
        let _busy = receiver.lock();
        sender.send_without_waiting(1);
        sender.send_without_waiting(2);
    }
    assert_eq!(receiver.queued(), 0);
    sender.send_without_waiting(3);
    let kept: Vec<u32> = std::iter::from_fn(|| receiver.try_recv()).collect();
    assert_eq!(kept, [1, 2, 3]);
    {
        let _busy = receiver.lock();
        for n in 10..20 {
            sender.send_without_waiting(n);
        }
    }
    sender.send_without_waiting(20);
    let kept: Vec<u32> = std::iter::from_fn(|| receiver.try_recv()).collect();
    assert_eq!(kept, [18, 19, 20]);
}

#[test]
fn receiving_waits_for_the_sender_and_ends_when_it_is_gone() {
    let (mut sender, receiver) = channel::<u32>(30, one, "test");
    let sending = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        sender.send(7);
    });
    assert_eq!(receiver.recv(), Some(7));
    sending.join().unwrap();
    assert_eq!(receiver.recv(), None);
    assert_eq!(receiver.recv_timeout(Duration::from_secs(5)), None);
}

#[test]
fn receive_timeout_gives_up_after_the_wait() {
    let (_sender, receiver) = channel::<u32>(30, one, "test");
    assert_eq!(receiver.recv_timeout(Duration::from_millis(10)), None);
}
