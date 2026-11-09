//! 识别放在子进程里（设计 20 第五节「省着用」）：主进程常驻、只占几百 KB 私有内存；要认的时候拉起
//! `miyu-voice-input recognize-worker`，它加载模型、认、空闲 [`IDLE`] 后自己退出，内存整份还给系统。
//!
//! 为什么不在主进程里卸：2026-11-09 实测，丢掉识别器以后常驻内存还剩 316～380 MB（glibc 把还回来的页留着），
//! 调 malloc 的门槛也还剩约 76 MB（onnxruntime 自己的全局东西）；子进程退出才是真放掉。代价是空闲后第一次认要重新加载
//! 模型（约半秒），和在主进程里懒加载一样。
//!
//! 两边一行一条 JSON：请求 `{"language", "wav"}`（`wav` 是 16 位 PCM wav 的 base64），回 `{"heard": {…}}` 或
//! `{"refused": {"kind", "message"}}`。

use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use voice_common::audio::{self, Pcm};

use crate::model::Model;
use crate::recognize::{Heard, Recognizer, Refused};

/// 子进程空闲多久自己退（设计 20 第五节「空闲一会儿卸掉」）。
pub const IDLE: Duration = Duration::from_mins(5);

/// 主进程这一头：要认时拉起子进程，子进程退了下次再拉。一次认一个。
pub struct Workers {
    cache: PathBuf,
    running: Mutex<Option<Running>>,
}

struct Running {
    child: Child,
    input: Option<BufWriter<ChildStdin>>,
    output: BufReader<ChildStdout>,
}

impl Workers {
    /// 模型在 `cache`（`MIYU_CACHE_DIR`）。
    #[must_use]
    pub fn new(cache: PathBuf) -> Workers {
        Workers {
            cache,
            running: Mutex::new(None),
        }
    }

    /// 交给子进程认。子进程刚好空闲退了的，重新拉起再试一次。
    ///
    /// # Errors
    ///
    /// 子进程认不了（太长、语言不对、模型坏了），或者拉不起来、半路死了。
    pub fn transcribe(&self, pcm: &Pcm, language: &str) -> Result<Heard, Refused> {
        let request = json!({
            "language": language,
            "wav": STANDARD.encode(audio::write_wav(&pcm.samples, pcm.rate)),
        })
        .to_string();
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        for _ in 0..2 {
            if running.is_none() {
                *running = Some(self.spawn()?);
            }
            let Some(worker) = running.as_mut() else {
                continue;
            };
            match exchange(worker, &request) {
                Some(answer) => return answer,
                // 退了或者死了：收掉，再拉一个。
                None => {
                    drop(running.take());
                }
            }
        }
        Err(Refused::Engine(
            "The recognizer stopped unexpectedly.".to_string(),
        ))
    }

    fn spawn(&self) -> Result<Running, Refused> {
        let program = std::env::current_exe()
            .map_err(|error| Refused::Engine(format!("Cannot find this program: {error}.")))?;
        let mut child = Command::new(program)
            .arg("recognize-worker")
            .env("MIYU_CACHE_DIR", &self.cache)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| Refused::Engine(format!("Cannot start the recognizer: {error}.")))?;
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(Refused::Engine("The recognizer has no pipes.".to_string()));
        };
        Ok(Running {
            child,
            input: Some(BufWriter::new(input)),
            output: BufReader::new(output),
        })
    }
}

impl Drop for Running {
    /// 主进程收掉子进程：关了它的标准输入，它读到头就退，等它退干净。
    fn drop(&mut self) {
        drop(self.input.take());
        let _ = self.child.wait();
    }
}

/// 发一条、等一条；管道断了交回 `None`。
fn exchange(worker: &mut Running, request: &str) -> Option<Result<Heard, Refused>> {
    let input = worker.input.as_mut()?;
    writeln!(input, "{request}").ok()?;
    input.flush().ok()?;
    let mut line = String::new();
    if worker.output.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let answer: Value = serde_json::from_str(&line).ok()?;
    Some(decode(&answer))
}

fn decode(answer: &Value) -> Result<Heard, Refused> {
    if let Some(heard) = answer.get("heard") {
        return Ok(Heard {
            text: heard["text"].as_str().unwrap_or_default().to_string(),
            language: heard["language"].as_str().unwrap_or_default().to_string(),
            duration_ms: heard["duration_ms"].as_u64().unwrap_or(0),
            clear: heard["clear"].as_bool().unwrap_or(false),
        });
    }
    let refused = &answer["refused"];
    let message = refused["message"].as_str().unwrap_or_default().to_string();
    Err(match refused["kind"].as_str() {
        Some("too_long") => Refused::TooLong,
        Some("language") => Refused::Language(message),
        _ => Refused::Engine(message),
    })
}

fn encode(answer: Result<Heard, Refused>) -> Value {
    match answer {
        Ok(heard) => json!({"heard": {
            "text": heard.text,
            "language": heard.language,
            "duration_ms": heard.duration_ms,
            "clear": heard.clear,
        }}),
        Err(Refused::TooLong) => json!({"refused": {"kind": "too_long", "message": ""}}),
        Err(Refused::Language(language)) => {
            json!({"refused": {"kind": "language", "message": language}})
        }
        Err(Refused::Engine(message)) => {
            json!({"refused": {"kind": "engine", "message": message}})
        }
    }
}

/// 子进程这一头（`recognize-worker`）：模型用时才加载，一条条认，空闲 [`IDLE`] 或者主进程关了管道就退。
pub fn serve(model: Model) {
    let (lines, incoming) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    let recognizer = Recognizer::new(model);
    let mut out = std::io::stdout().lock();
    while let Ok(line) = incoming.recv_timeout(IDLE) {
        let answer = match serde_json::from_str::<Value>(&line) {
            Ok(request) => encode(handle(&recognizer, &request)),
            Err(error) => encode(Err(Refused::Engine(format!("Bad request: {error}.")))),
        };
        if writeln!(out, "{answer}")
            .and_then(|()| out.flush())
            .is_err()
        {
            break;
        }
    }
}

fn handle(recognizer: &Recognizer, request: &Value) -> Result<Heard, Refused> {
    let wav = STANDARD
        .decode(request["wav"].as_str().unwrap_or_default())
        .map_err(|error| Refused::Engine(format!("Bad audio encoding: {error}.")))?;
    let pcm = audio::read_wav(&wav).map_err(|why| Refused::Engine(why.0))?;
    recognizer.transcribe(&pcm, request["language"].as_str().unwrap_or("zh"))
}

#[cfg(test)]
mod tests;
