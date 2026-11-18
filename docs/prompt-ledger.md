# 给模型看的字的登记簿

照核心仓库 `docs/designs/26-提示词.md` 的规矩：这三个包里每一处会进模型上下文的字（工具说明、参数说明、工具结果、报错）记一行，改了重量 token。token 照 o200k 数（tiktoken，`o200k_base`），带行尾的照实。

| 哪里 | 文件 | 什么时候进上下文 | token | 为什么这么写 |
|---|---|---|---|---|
| `say` 的说明、参数 | `crates/miyu-say/src/tool.rs` | 每一轮跟着工具面（预设开着 say 时） | 93（整份 `{name, description, input_schema}` 的紧凑 JSON；说明 57） | 施工 VO-35（项目主人 11-17 验收第 16、26 条）改成默认交给人，三句各管一件主人定的事：说出来、人在哪就在哪听到、聊天里是语音消息（她才知道这是「她说话」、不用再自己去发文件）；没人在场的轮（定时、循环）要不要出声由她选（主人定「照参数」，不按轮的来源替她决定）；`file_only` 只存文件、交回路径（要文件的场合）。不写终端、网页、im 各自怎么放：那是头和桥的事（26「描述不指挥」）。参数 `text`、`file_only` 名字说清了，不写说明 |
| 结果：说出去了 | 同上 | 调了以后 | 8 | `Spoken (2.3 s).`：交给人了、多长；音频在 `sent` 里，不给她看 |
| 结果：存好了（`file_only`） | 同上 | 调了以后 | 约 25 | `Saved audio/say-<哈希>.mp3 (2.3 s).`：相对工作区的路径（她接着发、展示要用）、时长；扩展名照真实格式（DeepSeek 引擎是 `.wav`）；文件名用内容哈希，同样的字同样的声音不重复存 |
| 报错：超了场所的语音上限 | 同上 | 场所会话里、合成出来长过 `voice_max_ms` 时 | 约 32 | `Too long for a voice message here: 75.2 s, the limit is 60.0 s. Say it again in parts or more briefly.`：主人定不退回发文件、不切，告诉她这段多长、上限多少，让她分几次说或者重新组织；「分几次、说短点」是她能做的两条路，不是替她决定说什么 |
| 报错：参数、工作区、交不出去 | 同上 | 调错时 | 3、6、约 9 | `Give text.`、`No workspace for this turn.`（只有 `file_only` 要工作区）、`Cannot hand the audio over: <原因>.`（存 blob 失败） |
| 报错：合成 | `crates/miyu-say/src/say.rs`、`engine.rs`、各家引擎 | 合成不了时 | 约 9～15 | `There is nothing to say in this text.`（转写后是空的）、`The text is longer than 5000 characters.`、`The MiniMax API key is not set.` 这类没配好的、云端回的原因照转（只带主机名和对方的话，不带 key；主机名去掉网址里的 `user:pass@`，VO-37）；回应超过 32 MB 的 `The answer from <主机> is larger than 32 MB.`（约 14，VO-37：原来悄悄截成半段音频，她以为念完了） |
| 助手的场所里「念出来」的事实 | `crates/miyu-voice-assistant/src/conversation.rs` 的 `SPOKEN_FACT` | 场所会话第一次造的时候记一条（`session.note`，kind `voice`），之后一直在上下文里 | 22 | 设计 20 第九节「告诉她这一轮是念出来的」：只客观写明回答会念给对方听、对方看不到文字（26「描述不指挥」），不写「说短一点」「别用列表」这类，那归人格。没写的话她会照屏幕的习惯写表格、代码块：念的那一头只能念「一张表」「一段代码」。只记一次、不每轮重复，前缀缓存不动 |
| 开口就打断时补的事实 | `crates/miyu-voice-assistant/src/conversation.rs` 的 `INTERRUPTED_FACT` | 全双工、她念着你开口时记一条（`session.note`，kind `voice`），下一轮她看得到 | 24 加上念完的最后一句（一句没念完的是 `nothing.`，2） | 设计 20 第九节第 6 步「打断补事实，写明念到哪一句、后面的没念出来」（一稿第四节第 6 条）：她屏幕上写完了整段，可人只听到前半截；不写的话她以为你都听到了，接着往下说。只写事实，不写「重说一遍」这类指使（26「描述不指挥」） |
