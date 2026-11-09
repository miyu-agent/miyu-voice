# 给模型看的字的登记簿

照核心仓库 `docs/designs/26-提示词.md` 的规矩：这三个包里每一处会进模型上下文的字（工具说明、参数说明、工具结果、报错）记一行，改了重量 token。token 照 o200k 数（tiktoken，`o200k_base`），带行尾的照实。

| 哪里 | 文件 | 什么时候进上下文 | token | 为什么这么写 |
|---|---|---|---|---|
| `say` 的说明、参数 | `crates/miyu-say/src/tool.rs` | 每一轮跟着工具面（预设开着 miyu-say 时） | 55（整份 `{name, description, input_schema}` 的紧凑 JSON；说明 27） | 两句只说做什么、结果在哪：合成成你的声音、存成 MP3（她才知道这是「她说话」，不是读别人的字）；存了不等于发了，没人听得到，要她接着发或者展示（最容易踩的坑，同 render_image）。不写 IM 用 `send_voice`、网页用 `show`：那是桥、网页的工具说明的事，这里写了就是替她做决定（26 定的「描述不指挥」）。参数 `text` 名字说清了，不写说明 |
| 结果：存好了 | 同上 | 调了以后 | 约 25 | `Saved audio/say-<哈希>.mp3 (2.3 s).`：相对工作区的路径（她接着发、展示要用）、时长（她判断长不长）；文件名用内容哈希，同样的字同样的声音不重复存 |
| 报错：参数、工作区 | 同上 | 调错时 | 3、6 | `Give text.`、`No workspace for this turn.` |
| 报错：合成 | `crates/miyu-say/src/say.rs`、`engine.rs`、各家引擎 | 合成不了时 | 约 9～15 | `There is nothing to say in this text.`（转写后是空的）、`The text is longer than 5000 characters.`、`The MiniMax API key is not set.` 这类没配好的、云端回的原因照转（只带主机名和对方的话，不带 key） |
| 助手的场所里「念出来」的事实 | `crates/miyu-voice-assistant/src/conversation.rs` 的 `SPOKEN_FACT` | 场所会话第一次造的时候记一条（`session.note`，kind `voice`），之后一直在上下文里 | 22 | 设计 20 第九节「告诉她这一轮是念出来的」：只客观写明回答会念给对方听、对方看不到文字（26「描述不指挥」），不写「说短一点」「别用列表」这类，那归人格。没写的话她会照屏幕的习惯写表格、代码块：念的那一头只能念「一张表」「一段代码」。只记一次、不每轮重复，前缀缓存不动 |
| 开口就打断时补的事实 | `crates/miyu-voice-assistant/src/conversation.rs` 的 `INTERRUPTED_FACT` | 全双工、她念着你开口时记一条（`session.note`，kind `voice`），下一轮她看得到 | 24 加上念完的最后一句（一句没念完的是 `nothing.`，2） | 设计 20 第九节第 6 步「打断补事实，写明念到哪一句、后面的没念出来」（一稿第四节第 6 条）：她屏幕上写完了整段，可人只听到前半截；不写的话她以为你都听到了，接着往下说。只写事实，不写「重说一遍」这类指使（26「描述不指挥」） |
