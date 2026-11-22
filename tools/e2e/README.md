# 对真实核心的端到端测试和量尺

> <sub>本文档由 Anthropic 的 Claude Fable 5.2 生成。</sub>

这个目录里的测试不进 CI，因为它们需要编译好的核心和下载好的模型。如果你修改了语音助手、朗读或语音输入的连接部分，提交之前要在本机运行一遍。
整个过程不会使用真实的麦克风，也不会发出声音：声音输出设为 `ALSA_CONFIG_PATH=/dev/null`（或者 `alsa-null.conf`，它可以播放，并且会立即播放完）；
语音助手的“麦克风”是一个 wav 文件（`MIYU_VOICE_TEST_MIC`）；模型和语音合成都是假的服务（`fake_chat.py`、`fake_tts.py`），所以不花钱。

运行之前需要做一次准备：

- `MIYU_CORE`：一份已经编译好 release 版本的核心检出（`cargo build --release -p miyu`）。
- `MIYU_VOICE_E2E`：一个暂存目录，里面放这些文件：
  - `vi-cache/sense-voice-int8-2024-07-17/`：语音输入的模型（运行一次 `miyu voice-input transcribe` 下载好的那一份）；
  - `va-cache/kws-zipformer-wenetspeech-3.3M-2024-01-01/`、`va-cache/silero-vad/`：语音助手的模型（运行 `miyu listen wake-test 你好你好` 下载好的那一份）；
  - `kws-dl/x/sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01/test_wavs/`：唤醒词模型压缩包里附带的样例；
  - `model-dl/x/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17/test_wavs/`：SenseVoice 压缩包（和 `vi-cache` 里的是同一个版本）里附带的样例（`zh.wav`、`en.wav`）；
  - `silence60.wav`：60 秒的 16 kHz 单声道静音（用来测量 CPU 占用）。
- 在本仓库里运行 `cargo build --release`。

各个脚本测试的内容如下：

| 脚本 | 测试内容 |
|---|---|
| `run-input.sh` | 语音输入：在总线上是否可用；界面交来 blob 之后能否识别成文字；错误码（整段数字静音时返回 `silent_audio`）；同一个连接上有一个很长的 `transcribe` 正在进行时，`stop` 是否先返回；关闭以后是否变为不可用 |
| `run-assistant.sh` | 语音助手：安装之后是否会打开麦克风（声卡没有打开时只报告一次）；`miyu listen` 能否开始和结束对话（切换、`on`、`off`）；不认识的参数；重启以后不保留对话 |
| `run-turn.sh` | 完整的一轮：唤醒（唤醒词“周望军”，使用 KWS 的 `5.wav`）→ 识别成文字 → `voice` 场所 → 假模型流式返回 → 逐句交给 `tts`。可以使用的开关：`MIYU_VOICE_TEST_MIC_CUT_IN=<wav>` 加上 `TTS_DELAY=6` 测试一开口就打断（再加上 `HALF_DUPLEX=1` 会关闭 `说话时可以打断 AI`，这时无法打断）；`FAKE_CHAT_TOOL=say PERSONA_VOICE=nova` 测试按人格选择音色；`MIYU_VOICE_MEASURE=1` 输出时延；`BY_COMMAND=1` 不说唤醒词，而是用 `miyu listen` 开始对话、结束后关闭（假麦克风换成 `model-dl` 里的 `zh.wav`） |
| `gap.sh` | 句子和句子之间是否断开：使用假的实时音箱（ALSA 的 file 插件把声音写进管道，`pace.py` 按实时速度读取），每句 2 秒、合成慢 1 秒，输出声卡打开了几次、每次播放了多久、中间静音了多久 |
| `measure.sh` | 量尺：三个程序空闲时的 RSS、PSS 和私有匿名内存，识别在冷启动和热启动时的时延，识别子进程的峰值，语音助手开着并监听静音时的内存和 CPU 占用 |

测量出来的数据记在 `docs/perf.md` 里，只和自己的上一个版本比较。
