//! 人敲的 `miyu say` 连核心（核心 `docs/reference/connection.md`「本机的头」）：数据根的 `run/socket` 里是套接字的路径
//! （Windows 是命名管道的名字），`run/token` 里是本机令牌；握手以后经总线 `bus.call` 找到在跑的 `tts` 去合成、去放，
//! 所以命令不用自己知道 key、不用碰声卡（设计 20 第六节）。只读的会话里数据根藏着，连不上，照实说（core1 11-10）。

use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use serde_json::{Value, json};

/// 连接的两头：读回应、写请求。
type Pipe = (Box<dyn BufRead>, Box<dyn Write>);

/// 连着的核心。
pub struct Client {
    reader: Box<dyn BufRead>,
    writer: Box<dyn Write>,
    next: u64,
    prefix: String,
}

impl Client {
    /// 照数据根 `home` 连上、握手。
    ///
    /// # Errors
    ///
    /// 核心没在跑、读不到套接字或令牌（只读的会话里是这样）、握手被拒。
    pub fn connect(home: &Path) -> Result<Client, String> {
        let unreachable =
            |why: String| format!("Cannot reach the Miyu core ({why}). Is it running?");
        let address = std::fs::read_to_string(home.join("run/socket"))
            .map_err(|error| unreachable(format!("run/socket: {error}")))?;
        let token = std::fs::read_to_string(home.join("run/token"))
            .map_err(|error| unreachable(format!("run/token: {error}")))?;
        let (reader, writer) = open(address.trim()).map_err(unreachable)?;
        let mut client = Client {
            reader,
            writer,
            next: 0,
            prefix: format!("say-{}-", std::process::id()),
        };
        client.call(
            "hello",
            &json!({"protocol": [1, 1], "head": {"kind": "miyu-say", "version": env!("CARGO_PKG_VERSION")}, "token": token.trim()}),
        )?;
        Ok(client)
    }

    /// 发一个请求、等它的回应（推送跳过）；被拒的交回原因码和那一句。
    ///
    /// # Errors
    ///
    /// 核心拒了，或者连接断了。
    pub fn call(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        self.next += 1;
        let id = format!("{}{}", self.prefix, self.next);
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(self.writer, "{line}")
            .and_then(|()| self.writer.flush())
            .map_err(|error| format!("The core connection broke: {error}."))?;
        loop {
            let mut answer = String::new();
            if self
                .reader
                .read_line(&mut answer)
                .map_err(|error| error.to_string())?
                == 0
            {
                return Err("The core closed the connection.".to_string());
            }
            let Ok(answer) = serde_json::from_str::<Value>(&answer) else {
                continue;
            };
            if answer["id"] != id {
                continue;
            }
            if let Some(error) = answer.get("error") {
                let code = error["data"]["code"]
                    .as_str()
                    .or_else(|| error["data"]["reason"].as_str())
                    .unwrap_or("error");
                let message = error["data"]["message"]
                    .as_str()
                    .or_else(|| error["message"].as_str())
                    .unwrap_or_default();
                return Err(format!("{code}: {message}"));
            }
            return Ok(answer["result"].clone());
        }
    }
}

#[cfg(unix)]
fn open(address: &str) -> Result<Pipe, String> {
    let stream =
        std::os::unix::net::UnixStream::connect(address).map_err(|error| error.to_string())?;
    let reader = stream.try_clone().map_err(|error| error.to_string())?;
    Ok((Box::new(BufReader::new(reader)), Box::new(stream)))
}

#[cfg(windows)]
fn open(address: &str) -> Result<Pipe, String> {
    // 命名管道照文件打开就是一条双向的连接。
    let pipe = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(address)
        .map_err(|error| error.to_string())?;
    let reader = pipe.try_clone().map_err(|error| error.to_string())?;
    Ok((Box::new(BufReader::new(reader)), Box::new(pipe)))
}
