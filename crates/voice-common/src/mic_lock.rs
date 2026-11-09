//! 整台机器的麦克风锁（设计 20 第九节「一个麦克风两个人用」）：开麦前拿一把锁，放在系统的运行时目录。终端、网页、
//! 命令行听写（`stt.record`）要麦时先立一个「想要」的记号，助手看到就让出锁、暂停；听写完放锁、撤记号，助手接着听。
//! 同一台机器开着两个 Miyu 也只有一个在录。锁是操作系统的文件锁：拿着的进程死了，锁自己放掉。

use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// 听写等助手让出多久。
const YIELD_WAIT: Duration = Duration::from_millis(1500);
/// 等的时候隔多久再试。
const RETRY: Duration = Duration::from_millis(50);

/// 拿着麦克风；丢掉就放锁。
#[derive(Debug)]
pub struct MicLock {
    _file: File,
}

/// 麦克风被别人拿着、等了也没让出来（另一个听写在录）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Busy;

/// 锁和记号放在哪：Linux 是 `XDG_RUNTIME_DIR`（本人的、重启就清），别处和没有它的是临时目录；底下一个 `miyu-voice`。
#[must_use]
pub fn dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|_| cfg!(target_os = "linux"))
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("miyu-voice")
}

impl MicLock {
    /// 听写要麦：立「想要」的记号，等拿着锁的助手让出，最多等 1.5 秒；拿到了撤记号。
    ///
    /// # Errors
    ///
    /// 等完了还被拿着：另一个听写在录。
    pub fn for_dictation() -> Result<MicLock, Busy> {
        MicLock::for_dictation_in(&dir())
    }

    /// 助手要麦：有人想要的时候不拿，拿不到也不等。
    #[must_use]
    pub fn for_assistant() -> Option<MicLock> {
        MicLock::for_assistant_in(&dir())
    }

    /// 有没有听写在等麦：助手录着的时候隔一会儿看一次，看到了就丢掉自己的锁。
    #[must_use]
    pub fn wanted() -> bool {
        want_file(&dir()).exists()
    }

    pub(crate) fn for_dictation_in(dir: &std::path::Path) -> Result<MicLock, Busy> {
        let _ = fs::create_dir_all(dir);
        let want = want_file(dir);
        let _ = File::create(&want);
        let started = Instant::now();
        let got = loop {
            if let Some(lock) = try_take(dir) {
                break Ok(lock);
            }
            if started.elapsed() >= YIELD_WAIT {
                break Err(Busy);
            }
            std::thread::sleep(RETRY);
        };
        let _ = fs::remove_file(&want);
        got
    }

    pub(crate) fn for_assistant_in(dir: &std::path::Path) -> Option<MicLock> {
        if want_file(dir).exists() {
            return None;
        }
        let _ = fs::create_dir_all(dir);
        try_take(dir)
    }
}

fn want_file(dir: &std::path::Path) -> PathBuf {
    dir.join("mic.want")
}

fn try_take(dir: &std::path::Path) -> Option<MicLock> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("mic.lock"))
        .ok()?;
    file.try_lock().ok()?;
    Some(MicLock { _file: file })
}

#[cfg(test)]
mod tests;
