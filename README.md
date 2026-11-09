# Miyu 语音

Miyu 的三个语音包，都是可选的：没装就没有，任何界面上都看不到语音的东西。设计在核心仓库的 `docs/designs/20-语音.md`。

| 包 | 装上以后 | 命令 |
|---|---|---|
| `miyu-voice-input` | 在这台电脑上把说的话转成字（SenseVoice，CPU 就够）：终端、网页输入框的麦克风，聊天里的语音消息转成字 | `miyu voice-input` |
| `miyu-say` | 用云端的声音（MiniMax、MiMo、OpenAI 兼容）把回复念出来；她能说话、能发语音 | `miyu say` |
| `miyu-voice-assistant` | 像 Siri 那样常驻后台，叫她的名字就能说；装它会带上另外两个 | `miyu listen` |

```sh
miyu pkg install miyu-voice-assistant   # 全套
miyu pkg install miyu-voice-input       # 只要语音转字
```

想换引擎的不用换包：社区包照 `stt.engine`、`tts.engine` 接进来（设计 `67-总线.md`），装上以后在包的配置页里多一项。

## 第三方

| 用到的 | 怎么用 | 许可证 |
|---|---|---|
| [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)（k2-fsa） | 静态链进 `miyu-voice-input`：本机识别 | Apache-2.0 |
| [ONNX Runtime](https://github.com/microsoft/onnxruntime)（随 sherpa-onnx 的预编译库） | 静态链进 `miyu-voice-input` | MIT |
| [rodio](https://github.com/RustAudio/rodio)、[cpal](https://github.com/RustAudio/cpal) | 编进程序：录音、放音 | MIT 或 Apache-2.0、Apache-2.0 |
| [Symphonia](https://github.com/pdeljanov/Symphonia)（随 rodio） | 编进 `miyu-say`：解 mp3、wav | MPL-2.0 |
| [SenseVoice-Small](https://github.com/FunAudioLLM/SenseVoice)（FunAudioLLM，sherpa-onnx 转的 int8 版） | 不随包发：`miyu-voice-input` 第一次启动时从 sherpa-onnx 的 Release 下载，对照钉死的 SHA-256 校验 | FunASR 模型许可（[MODEL_LICENSE](https://github.com/modelscope/FunASR/blob/main/MODEL_LICENSE)） |

Apache-2.0、MIT、MPL-2.0 和本仓库的 AGPL-3.0 能放在一起发（MPL-2.0 照文件算，改了它的文件要照它公开）；发包时把它们的许可证文本一起带上。

还在施工，没发版。许可证 AGPL-3.0-or-later。
