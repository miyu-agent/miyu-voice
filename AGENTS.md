# miyu-voice 仓库的规矩

照核心仓库 `miyu-agent/miyu` 的 `AGENTS.md`，下面是和这三个包有关的几条。

1. **只走协议**：三个包都是第三方写法的软件包，照核心仓库 `docs/authors/README.md` 能拿到的东西做，收发用开发包 `miyu-ext`（`miyu-agent/miyu-sdk`）；缺什么补进核心或开发包，不给这几个包开后门，也不依赖核心的 crate。
2. **设计在核心仓库**：`docs/designs/20-语音.md`（三个包、总线上的名字和每个方法的形状），`docs/designs/67-总线.md`（`provides`、`plugs`、`bus.call`）。这里的 `docs/` 只放施工单和登记簿。
3. **包之间只照名字调**：`miyu-voice-assistant` 调 `stt`、`tts`，不写 `miyu-voice-input`、`miyu-say` 的包名；社区引擎照 `stt.engine`、`tts.engine` 接。头和 IM 也只认名字。
4. **给模型看的字**（`say` 工具的说明、参数、结果）写英文短句，一件工具最多三句；每一处都记进 `docs/prompt-ledger.md`。给人看的字（清单里的名字、配置项）三种语言。
5. **三个平台**：Linux、macOS（M 系列）、Windows 都要能编、能跑；CI 三台。测试不靠跑测试的机器有麦克风、音箱、网络：录音、放音、云端合成都隔在一层后面，测试换成假的。
6. **文件规模**：每个 `.rs` 不超过 500 行（测试也算）；测试放各模块的 `tests.rs`。
7. **一步一个分支**：`step/<编号>-<几个字>`，三台 CI 绿了先交 c0（`miyu-agent-c0`）复核，再交 core1 合。碰头（终端、网页）的交 miyu-heads，碰 IM 的交 miyu-im，这里只出协议形状。
8. **旧版只当证据**：旧版 Miyu 0.7.4 的语音代码（唤醒规程、过滤、麦克风接线的教训）可以照着推，代码不整块搬。
