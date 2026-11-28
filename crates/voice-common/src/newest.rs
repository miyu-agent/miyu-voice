//! 有上限的队列，满了丢最旧的（施工 VO-49）：收音回调交给收麦线程的一块块声音（[`crate::mic`]）、助手切出来交去认字的一段段
//! 人声都走它。原来是 `std::sync::mpsc::channel`，没有上限：收的那一头一卡住（整机满负荷抢不到 CPU、头一次加载模型、整段过
//! 唤醒词模型），交的那一头照常交，就一直堆；ALSA 的空设备 `poll()` 永远就绪，2 秒涨到 1 GB。
//!
//! 为什么不用 `sync_channel` 加 `try_send`：满了的时候交的那一头只能丢新来的，丢不了最旧的（旧的在收的那一头手里），卡完再收
//! 到的是卡之前那 2 秒。这里丢最旧的，卡完收到的总是最新的声音。
//!
//! 为什么回调里用 [`Sender::send_without_waiting`]：cpal 的收音回调不能等。它只 `try_lock`，拿不到（收的那一头正拿着锁取一块，
//! 几微秒）就先放在自己手里，下一块来时一起交；一块不丢、也不等。别的线程用 [`Sender::send`]，等锁那几微秒，交了马上能收到。
//!
//! 丢了的块数记一行日志，一秒最多一行，在收的那一头记（回调里不写日志）。

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, TryLockError};
use std::time::{Duration, Instant};

/// 丢块的日志多久最多记一行。
const REPORT_EVERY: Duration = Duration::from_secs(1);

/// 开一条：队列里的东西照 `weigh` 称重，合起来不超过 `limit`（最新的一件比 `limit` 还重的照样留着）；`what` 写在丢块的日志前头。
#[must_use]
pub fn channel<T>(
    limit: usize,
    weigh: fn(&T) -> usize,
    what: &'static str,
) -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared {
        queue: Mutex::new(Queue {
            items: VecDeque::new(),
            weight: 0,
            dropped: 0,
            reported: None,
            sender_gone: false,
            receiver_gone: false,
        }),
        ready: Condvar::new(),
        limit,
        weigh,
        what,
    });
    (
        Sender {
            shared: Arc::clone(&shared),
            held: Held {
                items: VecDeque::new(),
                weight: 0,
                dropped: 0,
            },
        },
        Receiver { shared },
    )
}

struct Shared<T> {
    queue: Mutex<Queue<T>>,
    ready: Condvar,
    limit: usize,
    weigh: fn(&T) -> usize,
    what: &'static str,
}

struct Queue<T> {
    items: VecDeque<T>,
    /// `items` 合起来多重。
    weight: usize,
    /// 丢了、还没记进日志的件数。
    dropped: usize,
    /// 上一次记丢块的日志是什么时候。
    reported: Option<Instant>,
    sender_gone: bool,
    receiver_gone: bool,
}

/// 交的那一头。
pub struct Sender<T> {
    shared: Arc<Shared<T>>,
    /// [`Sender::send_without_waiting`] 拿不到锁时先放在手里的，下次一起交。
    held: Held<T>,
}

struct Held<T> {
    items: VecDeque<T>,
    weight: usize,
    /// 手里放满了丢掉的件数，交进队列时一起记。
    dropped: usize,
}

impl<T> Sender<T> {
    /// 交一件：等锁（收的那一头只拿几微秒）。收的那一头走了的丢掉。
    pub fn send(&mut self, item: T) {
        let shared = &self.shared;
        keep(shared, &mut self.held, item);
        let queue = shared.queue.lock().unwrap_or_else(PoisonError::into_inner);
        hand_over(shared, &mut self.held, queue);
    }

    /// 交一件、不等（cpal 的回调里用）：拿不到锁的先放在手里，下一件来时一起交；手里的也照上限丢最旧的。
    pub fn send_without_waiting(&mut self, item: T) {
        let shared = &self.shared;
        keep(shared, &mut self.held, item);
        let queue = match shared.queue.try_lock() {
            Ok(queue) => queue,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return,
        };
        hand_over(shared, &mut self.held, queue);
    }
}

/// 先放进手里，超了上限丢手里最旧的。
fn keep<T>(shared: &Shared<T>, held: &mut Held<T>, item: T) {
    held.weight += (shared.weigh)(&item);
    held.items.push_back(item);
    while held.weight > shared.limit && held.items.len() > 1 {
        if let Some(old) = held.items.pop_front() {
            held.weight -= (shared.weigh)(&old);
            held.dropped += 1;
        }
    }
}

/// 手里的全交进队列，超了上限丢队列里最旧的，叫醒收的那一头。
fn hand_over<T>(shared: &Shared<T>, held: &mut Held<T>, mut queue: MutexGuard<'_, Queue<T>>) {
    queue.dropped += std::mem::take(&mut held.dropped);
    held.weight = 0;
    if queue.receiver_gone {
        held.items.clear();
        return;
    }
    for item in held.items.drain(..) {
        queue.weight += (shared.weigh)(&item);
        queue.items.push_back(item);
    }
    while queue.weight > shared.limit && queue.items.len() > 1 {
        if let Some(old) = queue.items.pop_front() {
            queue.weight -= (shared.weigh)(&old);
            queue.dropped += 1;
        }
    }
    drop(queue);
    shared.ready.notify_one();
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let shared = &self.shared;
        let mut queue = shared.queue.lock().unwrap_or_else(PoisonError::into_inner);
        queue.sender_gone = true;
        hand_over(shared, &mut self.held, queue);
        shared.ready.notify_all();
    }
}

/// 收的那一头。
pub struct Receiver<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Receiver<T> {
    /// 等最多 `wait`，交回最旧的一件；没等到、交的那一头走了的交回 `None`。
    pub fn recv_timeout(&self, wait: Duration) -> Option<T> {
        let deadline = Instant::now() + wait;
        let mut queue = self.lock();
        loop {
            if let Some(item) = self.take(&mut queue) {
                return Some(item);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if queue.sender_gone || left.is_zero() {
                return None;
            }
            queue = self
                .shared
                .ready
                .wait_timeout(queue, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// 一直等到有一件；交的那一头走了、队列也空了的交回 `None`。
    pub fn recv(&self) -> Option<T> {
        let mut queue = self.lock();
        loop {
            if let Some(item) = self.take(&mut queue) {
                return Some(item);
            }
            if queue.sender_gone {
                return None;
            }
            queue = self
                .shared
                .ready
                .wait(queue)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// 不等：有就交回最旧的一件。
    #[must_use]
    pub fn try_recv(&self) -> Option<T> {
        let mut queue = self.lock();
        self.take(&mut queue)
    }

    /// 队列里现在合起来多重（同 `weigh` 称的）：测试量有没有超上限。
    #[must_use]
    pub fn queued(&self) -> usize {
        self.lock().weight
    }

    fn lock(&self) -> MutexGuard<'_, Queue<T>> {
        self.shared
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn take(&self, queue: &mut Queue<T>) -> Option<T> {
        let item = queue.items.pop_front()?;
        queue.weight -= (self.shared.weigh)(&item);
        if queue.dropped > 0 && queue.reported.is_none_or(|at| at.elapsed() >= REPORT_EVERY) {
            report(self.shared.what, std::mem::take(&mut queue.dropped));
            queue.reported = Some(Instant::now());
        }
        Some(item)
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut queue = self.lock();
        queue.receiver_gone = true;
        queue.items.clear();
        queue.weight = 0;
        if queue.dropped > 0 {
            report(self.shared.what, std::mem::take(&mut queue.dropped));
        }
    }
}

fn report(what: &str, dropped: usize) {
    eprintln!("{what}: the reader fell behind, dropped the {dropped} oldest");
}

#[cfg(test)]
mod tests;
