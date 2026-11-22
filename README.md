# Miyu 语音

这个仓库里是 Miyu 的三个语音包。三个包都是可选的：如果你没有安装，Miyu 的任何界面上都不会出现和语音有关的内容。

| 包 | 安装之后可以做什么 | 命令 |
|---|---|---|
| `voice-input`（语音输入） | 在这台电脑上把你说的话转成文字，用于终端和网页输入框的麦克风按钮，以及聊天里的语音消息。识别在本机的 CPU 上运行，使用 SenseVoice 模型。 | `miyu voice-input` |
| `say`（朗读） | 用云端的语音合成（MiniMax、MiMo 或兼容 OpenAI 接口的服务）把回复念出来。AI 也可以主动说话和发送语音消息。 | `miyu say` |
| `voice-assistant`（语音助手） | 像 Siri 一样在后台常驻，你说出唤醒词就可以和 AI 对话。安装它时会同时安装另外两个包。 | `miyu listen` |

安装：

```sh
miyu pkg install voice-assistant   # 安装全部三个包
miyu pkg install voice-input       # 只安装语音输入
```

## 语音输入

在终端里用麦克风说话时，开始听会播放一声上扬的提示音，停止听会播放一声下沉的提示音。如果不需要提示音，可以在设置里关闭 `语音输入提示音`。

如果整段录音完全没有声音（例如麦克风被静音、没有插好，或者系统默认的输入设备没有声音），语音输入会报告 `silent_audio`，而不是提示没听清。此时建议检查麦克风，或者在设置的 `麦克风` 下拉里换一个输入设备。

## 语音助手

语音助手在安装并启用之后，只要核心在运行，就会一直监听唤醒词。默认的唤醒词是 `你好你好`，你可以在设置页修改，修改之后马上生效，不需要重新启动语音助手。想先确认某个词能不能被听出来，可以运行 `miyu listen wake-test <唤醒词>`。如果不想让语音助手监听，在软件包里停用它即可。

语音对话使用哪个人格和预设，可以在设置页的 `人格` 和 `预设` 里选择；不选择时使用默认的人格和预设。修改之后，如果你正在和 AI 对话，这次对话不会被打断，对话结束之后语音助手会开始一个新的会话；原来的会话仍然保留，可以在终端和网页里看到。

`miyu listen` 用来开始或结束一次对话，也可以写明 `miyu listen on` 或 `miyu listen off`：

- 开始对话的效果和说出唤醒词一样，提示音响过之后就可以说话；
- 结束对话时会播放另一声提示音，如果 AI 还在说话，也会停下来。

对话也会自动结束：如果唤醒之后 30 秒内你没有开口，或者 AI 说完之后 30 秒内你没有再说话，语音助手会播放结束的提示音，并弹出一条 `对话结束` 通知。在对话结束之前，你可以直接接着说，不需要再说唤醒词。

对话过程中，语音助手会用系统通知显示当前的状态：正在听、识别出了什么、正在思考、AI 说到了哪里、没听清，以及哪一步出了错。在 Linux 上，这些状态显示在同一条通知里并随时更新。Windows 和 macOS 不能替换已经弹出的通知，所以只会弹出识别结果、回答完成和出错这几条。

**AI 说话的时候，你一开口它就会停下，用外放音箱时也一样。** 这是因为语音助手会从麦克风收到的声音里减去音箱正在播放的声音（回声消除），所以不会把 AI 自己的声音当成你说的话。但是，如果音箱的音量比你的声音大很多，语音助手就听不出你开口了。此时可以等 AI 说完，或者运行 `miyu listen off`。如果你不希望被打断，可以在设置里关闭 `说话时可以打断 AI`，这样 AI 说完之后才会开始听你说话。

快捷键的设置方法因系统而异：

- Windows 和 macOS：由系统托盘程序注册快捷键。
- Linux：Wayland 不允许程序自己注册全局快捷键，所以需要在桌面环境的快捷键设置里添加一条运行 `miyu listen` 的快捷键。

## 选择麦克风

语音输入和语音助手都可以在设置的 `麦克风` 下拉里选择麦克风；不选择时使用系统默认的输入设备。

- Linux：如果系统里有声音服务（PipeWire 或 PulseAudio），录音和播放都通过声音服务进行（使用它的 PulseAudio 接口，不需要另外安装库）。下拉里列出的就是系统声音设置里的输入设备，其他程序也可以同时使用同一个麦克风。如果没有声音服务，就直接使用 ALSA 声卡。
- macOS：通过 Core Audio 录音和播放。第一次录音时，macOS 会询问是否允许使用麦克风，允许之后才能录音。
- Windows：通过 WASAPI 录音和播放。

## 更换引擎

如果想换一个语音识别或语音合成的引擎，不需要更换语音包。提供引擎的软件包（例如 DeepSeek 功能全家桶）安装之后，语音输入和朗读设置里的 `引擎` 下拉会多出一项，显示这个包的名字，和内置的引擎排在一起。选择这个引擎之后：

- 语音输入不需要下载本机 163 MB 的识别模型；
- 朗读会把较长的文字按句子切开，逐段交给这个引擎合成，再拼成一整段音频。

> 开发者可以看核心仓库的 `docs/designs/20-语音.md`（语音的设计）和 `docs/designs/67-总线.md`（引擎怎么接入 `stt.engine`、`tts.engine`）。

## 第三方组件

| 组件 | 用途 | 许可证 |
|---|---|---|
| [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)（k2-fsa） | 静态链接进 `miyu-voice-input`，用于本机语音识别 | Apache-2.0 |
| [ONNX Runtime](https://github.com/microsoft/onnxruntime)（sherpa-onnx 附带的预编译库） | 静态链接进 `miyu-voice-input` | MIT |
| [rodio](https://github.com/RustAudio/rodio)、[cpal](https://github.com/RustAudio/cpal) | 编译进程序，用于录音和播放 | MIT 或 Apache-2.0、Apache-2.0 |
| [Symphonia](https://github.com/pdeljanov/Symphonia)（rodio 附带） | 编译进 `miyu-say`，用于解码 mp3 和 wav | MPL-2.0 |
| [SenseVoice-Small](https://github.com/FunAudioLLM/SenseVoice)（FunAudioLLM，sherpa-onnx 转换的 int8 版本） | 不随包发布。`miyu-voice-input` 第一次启动时从 sherpa-onnx 的发行版页面下载，并用固定的 SHA-256 校验 | FunASR 模型许可（[MODEL_LICENSE](https://github.com/modelscope/FunASR/blob/main/MODEL_LICENSE)） |

Apache-2.0、MIT、MPL-2.0 都可以和本仓库的 AGPL-3.0 一起发布。MPL-2.0 按文件生效，如果修改了它的文件，就需要按 MPL-2.0 公开修改。

每个包都附带 `THIRD-PARTY-LICENSES.txt`，里面是编译进程序的每个第三方库，以及 sherpa-onnx 静态链接进来的 C++ 库（`licenses/native.txt`）的许可证原文。运行 `cargo xtask notices` 可以重新生成这个文件，运行 `cargo xtask licenses` 可以检查它是否最新（CI 会运行这项检查）。

## 许可证

AGPL-3.0-or-later。
