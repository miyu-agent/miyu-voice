# Miyu 语音

Miyu 的三个语音包，都是可选的：没装就没有，任何界面上都看不到语音的东西。设计在核心仓库的 `docs/designs/20-语音.md`。

| 包 | 装上以后 | 命令 |
|---|---|---|
| `voice-input` | 在这台电脑上把说的话转成字（SenseVoice，CPU 就够）：终端、网页输入框的麦克风，聊天里的语音消息转成字。用这台电脑的麦克风录（终端）时，开始听响上扬的一声、停止听响下沉的一声，设置里「语音输入提示音」能关 | `miyu voice-input` |
| `say` | 用云端的声音（MiniMax、MiMo、OpenAI 兼容）把回复念出来；她能说话、能发语音 | `miyu say` |
| `voice-assistant` | 像 Siri 那样常驻后台，叫她的名字就能说；装它会带上另外两个 | `miyu listen` |

```sh
miyu pkg install voice-assistant   # 全套
miyu pkg install voice-input       # 只要语音转字
```

语音助手装着、没停用，核心开着就一直听唤醒词（默认「未有未有」「密友密友」「miyumiyu」「みゆみゆ」，配置页能改，`miyu listen wake-test <词>` 先试）；不想让它听就在软件包里停用它。`miyu listen` 开关和她的对话（`miyu listen on`、`off` 写明）：开了同喊了唤醒词，响一声就说；关了响另一声，她还在说的停下。对话也会自己结束：叫醒了 30 秒没开口、她说完 30 秒没再说，响结束那一声、弹一条「对话结束」；这之前接着说不用再叫。一路弹系统通知：在听、认成了什么、在想、她说到哪、没听清、哪一步出了错（Linux 上一直是同一条跟着变；Windows、macOS 换不掉上一条，只弹认成了什么、答完、出错）。她说话时你一开口她就停，外放也行：助手把音箱正在放的声音从麦克风收到的里面减掉（回声消除），不把她自己的声音当成你说的；音箱开得比你的声音响很多时听不出你开口，这时喊不动她，等她说完或者 `miyu listen off`。不想被打断的在配置页关掉「说话时可以打断她」，她说完才听你的。快捷键：Windows、macOS 由托盘程序注册；Linux 的 Wayland 不让程序自己抢全局键，在桌面环境的快捷键设置里绑一条 `miyu listen`。

麦克风在配置页里从下拉选，不选用系统默认的。Linux 上有声音服务（PipeWire、PulseAudio）的，收音、放声音都经它（走它的 PulseAudio 接口，不用另装库），下拉里列的就是系统声音设置里的那几个输入，别的程序照样能用同一个麦克风；没有声音服务的直接用 ALSA 的声卡。

想换引擎的不用换包：社区包照 `stt.engine`、`tts.engine` 接进来（设计 `67-总线.md`），装上以后在包的配置页里多一项。

## 第三方

| 用到的 | 怎么用 | 许可证 |
|---|---|---|
| [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)（k2-fsa） | 静态链进 `miyu-voice-input`：本机识别 | Apache-2.0 |
| [ONNX Runtime](https://github.com/microsoft/onnxruntime)（随 sherpa-onnx 的预编译库） | 静态链进 `miyu-voice-input` | MIT |
| [rodio](https://github.com/RustAudio/rodio)、[cpal](https://github.com/RustAudio/cpal) | 编进程序：录音、放音 | MIT 或 Apache-2.0、Apache-2.0 |
| [Symphonia](https://github.com/pdeljanov/Symphonia)（随 rodio） | 编进 `miyu-say`：解 mp3、wav | MPL-2.0 |
| [SenseVoice-Small](https://github.com/FunAudioLLM/SenseVoice)（FunAudioLLM，sherpa-onnx 转的 int8 版） | 不随包发：`miyu-voice-input` 第一次启动时从 sherpa-onnx 的 Release 下载，对照钉死的 SHA-256 校验 | FunASR 模型许可（[MODEL_LICENSE](https://github.com/modelscope/FunASR/blob/main/MODEL_LICENSE)） |

Apache-2.0、MIT、MPL-2.0 和本仓库的 AGPL-3.0 能放在一起发（MPL-2.0 照文件算，改了它的文件要照它公开）。每个包都带 `THIRD-PARTY-LICENSES.txt`：编进程序的每个第三方包、sherpa-onnx 静态链进来的 C++ 库（`licenses/native.txt`）的许可证原文；`cargo xtask notices` 重新生成，`cargo xtask licenses` 查（CI 跑）。

还在施工，没发版。许可证 AGPL-3.0-or-later。
