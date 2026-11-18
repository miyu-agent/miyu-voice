//! 朗读（设计 20 第六节）：云端合成，在总线上提供 tts，她的工具 `say`，命令 `miyu say`。
//!
//! 核心照清单 `[process] args` 带 `serve` 拉起：照包的配置（`say.*`）配引擎，配好了登记 `tts.*`（[`service`]），登记
//! `say` 工具（[`tool`]）。人敲 `miyu say …` 时核心把参数原样交过来：命令当本机的头连核心，经总线找到在跑的 `tts` 去合成、
//! 去放（`voice_common::client`），自己不读 key、不碰声卡。

mod cli;
mod community;
mod engine;
mod http;
mod mimo;
mod minimax;
mod mp3;
mod openai;
mod say;
mod service;
mod settings;
mod tool;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use miyu_ext::{Extension, Method};
use serde_json::{Value, json};

use crate::cli::{Command, USAGE};
use crate::service::Service;
use voice_common::client::Client;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => {
            serve();
            ExitCode::SUCCESS
        }
        Some(_) => match cli::parse(&args) {
            Ok(Command::Help) => {
                println!("{USAGE}");
                ExitCode::SUCCESS
            }
            Ok(asked) => match command(asked) {
                Ok(()) => ExitCode::SUCCESS,
                Err(why) => {
                    eprintln!("miyu say: {why}");
                    ExitCode::FAILURE
                }
            },
            Err(why) => {
                eprintln!("miyu say: {why}");
                ExitCode::FAILURE
            }
        },
        None => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn serve() {
    let service = Service::new(&serde_json::Map::new());
    let mut ext = Extension::new("miyu-say", env!("CARGO_PKG_VERSION"));
    service.register(&mut ext);
    let for_tool = Arc::clone(&service);
    ext.tool(tool::spec(), move |args, call| {
        tool::call(&for_tool, args, call)
    });
    // 只给自己的命令行用（`miyu say --voices` 经 package.call），不上总线。
    let for_voices = Arc::clone(&service);
    ext.method_with(Method::new("voices"), move |_, _, core| {
        let voices: Vec<Value> = for_voices
            .settings()
            .engine(core)
            .voices()
            .into_iter()
            .map(|voice| json!({"id": voice.id, "name": voice.name}))
            .collect();
        Ok(json!({"voices": voices}))
    });
    // 配置页「引擎」下拉的选项（施工 VO-29、VO-32）：内置的几家，后面接着装着的、接进 tts.engine 的包，排在一张列表里。
    ext.method_with(Method::new("engines"), move |_, _, core| {
        let language = core.language();
        let mut choices = settings::built_in_choices(&language);
        choices.extend(
            voice_common::plugs::listed(core, "tts.engine")
                .iter()
                .map(|plugged| voice_common::plugs::choice(plugged, &language)),
        );
        Ok(json!({"choices": choices}))
    });
    let on_ready = Arc::clone(&service);
    ext.ready(move |core| on_ready.refresh(core));
    // 配置推送的处理函数跑在读核心回应的那个线程上（开发包 miyu-ext 0.1）：在里面等核心回 package.methods 会把自己卡死，
    // 另开线程去改登记（2026-11-10 端到端测出来的）。
    let on_config = Arc::clone(&service);
    ext.on_config(move |_, core| {
        let service = Arc::clone(&on_config);
        let core = core.clone();
        std::thread::spawn(move || service.refresh(&core));
    });
    ext.run();
}

/// `miyu say …`：连核心，照参数调 `tts` 或者自己的 `voices`。
fn command(asked: Command) -> Result<(), String> {
    let home = std::env::var_os("MIYU_HOME")
        .map(PathBuf::from)
        .ok_or("MIYU_HOME is not set; run it as `miyu say`.")?;
    let mut core = Client::connect(&home, "miyu-say")?;
    match asked {
        // `main` 连核心以前就印了用法。
        Command::Help => Ok(()),
        Command::Voices => voices(&mut core),
        Command::File(file) => {
            let path = std::fs::canonicalize(&file).map_err(|error| format!("{file}: {error}"))?;
            let stored = core.call("blob.put", &json!({"path": path}))?;
            play(&mut core, &json!({"audio": stored["blob"]}))
        }
        Command::Save { text, out } => save(&mut core, &text, Path::new(&out)),
        Command::Play(text) => play(&mut core, &json!({"text": text})),
    }
}

/// 列引擎的音色，一行一个。
fn voices(core: &mut Client) -> Result<(), String> {
    let answer = core.call(
        "package.call",
        &json!({"package": "say", "method": "voices"}),
    )?;
    for voice in answer["voices"].as_array().into_iter().flatten() {
        println!(
            "{}\t{}",
            voice["id"].as_str().unwrap_or_default(),
            voice["name"].as_str().unwrap_or_default()
        );
    }
    Ok(())
}

fn play(core: &mut Client, params: &Value) -> Result<(), String> {
    core.call(
        "bus.call",
        &json!({"name": "tts", "method": "play", "params": params}),
    )
    .map(drop)
}

/// 合成，存到 `out`：照扩展名挑 mp3 或 wav，经 `blob.get` 一块块读回来。
fn save(core: &mut Client, text: &str, out: &Path) -> Result<(), String> {
    let wav = out
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"));
    let format = if wav { "wav" } else { "mp3" };
    let made = core.call(
        "bus.call",
        &json!({"name": "tts", "method": "synthesize", "params": {"text": text, "format": format}}),
    )?;
    let blob = made["audio"].as_str().unwrap_or_default().to_string();
    let mut bytes = Vec::new();
    loop {
        let part = core.call(
            "blob.get",
            &json!({"blob": blob, "offset": bytes.len(), "length": 512 * 1024}),
        )?;
        let data = base64_decode(part["data"].as_str().unwrap_or_default())?;
        if data.is_empty() {
            break;
        }
        bytes.extend(data);
        if part["size"]
            .as_u64()
            .is_some_and(|size| bytes.len() as u64 >= size)
        {
            break;
        }
    }
    std::fs::write(out, bytes).map_err(|error| format!("{}: {error}", out.display()))
}

fn base64_decode(data: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| format!("The core sent bad base64: {error}."))
}
