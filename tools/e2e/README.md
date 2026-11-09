# 对真核心的端到端和量尺

不进 CI（要编好的核心、要下好的模型），改了助手、朗读、语音输入的连接部分，交之前在本机跑一遍。全程不碰真麦克风、
不出声：`ALSA_CONFIG_PATH=/dev/null`（或 `alsa-null.conf`，能放、马上放完），助手的「麦克风」是一个 wav
（`MIYU_VOICE_TEST_MIC`），模型和合成是假的服务（`fake_chat.py`、`fake_tts.py`），不花钱。

准备（一次）：

- `MIYU_CORE`：编好 release 的核心检出（`cargo build --release -p miyu`）。
- `MIYU_VOICE_E2E`：一个暂存目录，放：
  - `vi-cache/sense-voice-int8-2024-07-17/`：语音输入的模型（跑一次 `miyu voice-input transcribe` 下好的那份）；
  - `va-cache/kws-zipformer-wenetspeech-3.3M-2024-01-01/`、`va-cache/silero-vad/`：助手的模型（`miyu listen wake-test 未有未有` 下好的）；
  - `kws-dl/x/sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01/test_wavs/`：唤醒词模型压缩包里带的样例；
  - `model-dl/x/…/test_wavs/`：SenseVoice 压缩包里带的样例（`zh.wav`、`en.wav`）；
  - `silence60.wav`：60 秒 16 kHz 单声道静音（量 CPU 用）。
- 本仓库 `cargo build --release`。

| 脚本 | 测什么 |
|---|---|
| `run-input.sh` | 语音输入：总线上可用、头交 blob 认字、错误码、同一个连接上长的 transcribe 挂着时 stop 先回、关掉以后不可用 |
| `run-assistant.sh` | 助手：默认关、`miyu listen` 开关、状态文件、重启以后照旧、声卡没开 |
| `run-turn.sh` | 一整轮：唤醒（「周望军」，KWS 的 5.wav）→ 认字 → voice 场所 → 假模型流式回 → 一句句交 tts。开关：`FULL_DUPLEX=1` 加 `MIYU_VOICE_TEST_MIC_CUT_IN=<wav>` 加 `TTS_DELAY=6` 测开口就打断；`FAKE_CHAT_TOOL=say PERSONA_VOICE=nova` 测照人格挑音色；`MIYU_VOICE_MEASURE=1` 印时延 |
| `measure.sh` | 量尺：三个程序空闲的 RSS、PSS、私有匿名，识别冷、热的时延，识别子进程的峰值，助手开着听静音时的内存和 CPU |

量出来的数记在 `docs/perf.md`，和上一版自己比。
