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

还在施工，没发版。许可证 AGPL-3.0-or-later。
