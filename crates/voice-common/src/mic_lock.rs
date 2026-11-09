//! 整台机器的麦克风锁（设计 20 第九节「一个麦克风两个人用」）：开麦前拿一把锁，放在系统的运行时目录。终端、网页、
//! 命令行听写（`stt.record`）要麦时先立一个「想要」的记号，助手看到就让出锁、暂停；听写完放锁、撤记号，助手接着听。
//! 同一台机器开着两个 Miyu 也只有一个在录。锁是操作系统的文件锁：拿着的进程死了，锁自己放掉。
//!
//! 放锁的目录要是自己的（施工 VO-6补，core1 复核时提的）：Linux 没有 `XDG_RUNTIME_DIR` 时退到临时目录下带用户编号的
//! `miyu-voice-<uid>`，不用大家共用的 `/tmp/miyu-voice`（别的用户能抢先建它、换成链接）；Unix 上建好以后查它不是链接、
//! 属主是自己，不对就不用；是自己的、权限开大了的收紧到 0700。「想要」的记号超过 3 秒的当残留删掉：听写死在等锁的那 1.5 秒里，记号会一直在，
//! 助手就从此不拿麦了。

use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// 听写等助手让出多久。
const YIELD_WAIT: Duration = Duration::from_millis(1500);
/// 「想要」的记号放了多久算残留：比听写最多等的 1.5 秒长一倍。
const STALE_WANT: Duration = Duration::from_secs(3);
/// 等的时候隔多久再试。
const RETRY: Duration = Duration::from_millis(50);

/// 拿着麦克风；丢掉就放锁。
#[derive(Debug)]
pub struct MicLock {
    _file: File,
}

/// 拿不到麦克风。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockError {
    /// 被别人拿着、等了也没让出来（另一个听写在录，`mic_busy`）。
    Busy,
    /// 放锁的目录不是自己的、别人能动，不用（报 `no_microphone`，说清是哪个目录）。
    Unsafe(String),
}

/// 锁和记号放在哪：Linux 是 `XDG_RUNTIME_DIR/miyu-voice`（本人的、重启就清），没有它的退到临时目录下的
/// `miyu-voice-<uid>`；macOS、Windows 的临时目录本来就是每人一份，底下一个 `miyu-voice`。
#[must_use]
pub fn dir() -> PathBuf {
    if cfg!(target_os = "linux") {
        if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
            return PathBuf::from(runtime).join("miyu-voice");
        }
        return std::env::temp_dir().join(format!("miyu-voice-{}", uid().unwrap_or(0)));
    }
    std::env::temp_dir().join("miyu-voice")
}

impl MicLock {
    /// 听写要麦：立「想要」的记号，等拿着锁的助手让出，最多等 1.5 秒；拿到了撤记号。
    ///
    /// # Errors
    ///
    /// 等完了还被拿着：另一个听写在录；放锁的目录不是自己的。
    pub fn for_dictation() -> Result<MicLock, LockError> {
        MicLock::for_dictation_in(&dir())
    }

    /// 助手要麦：有人想要的时候不拿，拿不到也不等。
    #[must_use]
    pub fn for_assistant() -> Option<MicLock> {
        MicLock::for_assistant_in(&dir())
    }

    /// 有没有听写在等麦：助手录着的时候隔一会儿看一次，看到了就丢掉自己的锁。超过 3 秒的记号当残留删掉。
    #[must_use]
    pub fn wanted() -> bool {
        wanted_in(&dir())
    }

    pub(crate) fn for_dictation_in(dir: &std::path::Path) -> Result<MicLock, LockError> {
        own_dir(dir)?;
        let want = want_file(dir);
        let _ = File::create(&want);
        let started = Instant::now();
        let got = loop {
            if let Some(lock) = try_take(dir) {
                break Ok(lock);
            }
            if started.elapsed() >= YIELD_WAIT {
                break Err(LockError::Busy);
            }
            std::thread::sleep(RETRY);
        };
        let _ = fs::remove_file(&want);
        got
    }

    pub(crate) fn for_assistant_in(dir: &std::path::Path) -> Option<MicLock> {
        if wanted_in(dir) {
            return None;
        }
        own_dir(dir).ok()?;
        try_take(dir)
    }
}

fn want_file(dir: &std::path::Path) -> PathBuf {
    dir.join("mic.want")
}

/// 有没有新鲜的「想要」记号；超过 [`STALE_WANT`] 的删掉、当没有。
fn wanted_in(dir: &std::path::Path) -> bool {
    let want = want_file(dir);
    let Ok(modified) = fs::metadata(&want).and_then(|meta| meta.modified()) else {
        return false;
    };
    if modified.elapsed().unwrap_or_default() > STALE_WANT {
        let _ = fs::remove_file(&want);
        return false;
    }
    true
}

/// 建好放锁的目录（Unix 上只有自己能进），查它是自己的。
fn own_dir(dir: &std::path::Path) -> Result<(), LockError> {
    let unsafe_dir = |why: &str| LockError::Unsafe(format!("{}: {why}", dir.display()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        let _ = fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir);
        let meta = fs::symlink_metadata(dir).map_err(|error| unsafe_dir(&error.to_string()))?;
        if !meta.file_type().is_dir() {
            return Err(unsafe_dir("not a folder (a link?)"));
        }
        if uid().is_some_and(|mine| meta.uid() != mine) {
            return Err(unsafe_dir("owned by another user"));
        }
        // 是自己的、只是开得太大（VO-6 那一版建的是 0755）：自己收紧。
        if meta.mode() & 0o077 != 0 {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
                .map_err(|error| unsafe_dir(&format!("other users can open it: {error}")))?;
        }
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(dir).map_err(|error| unsafe_dir(&error.to_string()))?;
    }
    Ok(())
}

/// 本进程的用户编号：Linux 读 `/proc/self`（不用 unsafe 调 `getuid`），别处没有。
fn uid() -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(meta) = fs::metadata("/proc/self") {
            return Some(meta.uid());
        }
        // macOS 没有 /proc：看自己新建的临时文件属主。
        let probe = std::env::temp_dir().join(format!("miyu-voice-uid-{}", std::process::id()));
        let uid = File::create(&probe)
            .and_then(|_| fs::metadata(&probe))
            .map(|meta| meta.uid())
            .ok();
        let _ = fs::remove_file(&probe);
        uid
    }
    #[cfg(not(unix))]
    {
        None
    }
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
