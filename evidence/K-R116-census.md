# `K-R116` 人群普查 —— 「`daemon` 出现几次」与「该改几处」是两个数

> 量于 **2026-09-14**，工作树 `.claude/worktrees/k-r116`（分支 `track/k-r116`，基点主干 `897afec`）。
> 量具 `evidence/K-R116-ruler.py`（被测树 = 它自己所在的那棵工作树，跑的时候印出来）。
> 单位一律是**出现次数**（大小写不敏感），**不是行数** —— 一行两处算两处。
> ⚠ 每个读数带「量于哪一刻 · 用什么量的」；带轮次 / 分支尖的句子下一轮自动变成假话。

## §1 两个数

| | 数 | 怎么来的 |
|---|---|---|
| **出现次数**（`grep` 数得到的那个） | **2111** | 人群 = `git ls-files '*.md'` **现打 96 份**（有命中 86 份），逐份数 `(?i)daemon` |
| 🔴 **该改几处**（工作量） | **358** | 上面那 2111 里，落在「该改」那一档的 |
| 两个数**差** | **1753** | ＝ 冻结 1478 ＋ 写区外 121 ＋ 代码标识符 139 ＋ 登记例外 15 |

🔴 **派工单里那个 1828 是 `K-R107` 09-13 量的，本轮复量得 2111。**
两个数**都不是错的，它们量于不同的尖**：09-13 之后 `K-R108`…`K-R118` 十一件各自往 `evidence/`
落了留档（而 `evidence/` 正是最大那一档）。⇒ **1828 不是工作量，2111 也不是** —— 工作量是 **358**。

## §2 逐档（闭集 5 档，现算见 `K-R116-ruler.py::VERDICTS`）

| 档 | 处数 | 占比 | 判 |
|---|---|---|---|
| 🔴 `冻结·不许改` | **1478** | 70.0% | `evidence/**` **1388** ＋ `CHANGELOG.md` **90** —— 历史读数与墓碑，`KR116D2` 守的就是它 |
| `写区外·未改` | **121** | 5.7% | 被跟踪的 `.md`，但不在本轮 `.dispatch.json` 的写区里 ⇒ 本轮不动，逐处点名见 §4 |
| `标识符·不改` | **139** | 6.6% | `remote-daemon-proto` · `daemon_send_keys.rs` · `--daemon-probe` · `daemonPath` · `daemon-gate2`… |
| `登记例外·不改` | **15** | 0.7% | 写区里裸着、而一个字都不许动的那几处，逐条带理由（住 `doc_claim_registry::EXEMPT`） |
| 🔴 **`该改`** | **358** | 17.0% | **这个数就是工作量** |

改完之后同一把尺子再打一趟：总数 **2111 → 1753**，`该改` 档 **358 → 0**
（`python3 evidence/K-R116-ruler.py --verify` ⇒ `KR116D1: OK`）。
其余四档**逐档一处没动**（1478 / 121 / 139 / 15 前后相同）—— 那正是「只动该动的」的读数面。

## §3 写区那 9 份，逐份

| 文件 | 合计 | 标识符 | 例外 | **该改** |
|---|---|---|---|---|
| `doc/IPC-PROTOCOL.md` | 159 | 31 | 3 | **125** |
| `doc/INVARIANTS.md` | 158 | 38 | 5 | **115** |
| `README.md` | 37 | 7 | 1 | **29** |
| `doc/ARCHITECTURE.md` | 27 | 5 | 0 | **22** |
| `e2e/README.md` | 56 | 32 | 1 | **23** |
| `README.en.md` | 24 | 4 | 0 | **20** |
| `src-tauri/README.md` | 30 | 11 | 4 | **15** |
| `remote-daemon-proto/README.md` | 7 | 2 | 0 | **5** |
| `doc/CONTRIBUTING.md` | 14 | 9 | 1 | **4** |
| **合计** | **512** | **139** | **15** | **358** |

⚠ **`e2e/README.md` 那 56 处里 32 处是标识符**（`daemon-gate2` / `graylight-daemon-frames` /
`daemon-wrapper.sh` / `CCM_E2E_DAEMON` 这一族 e2e 套件名与环境变量）—— 它是本表里
「出现次数很高、而该改的很少」最极端的一份。**这一格就是 `KR116D1` 那句话的活体。**

## §4 `写区外·未改` 121 处，逐份点名（交回 PM 派下一件）

| 文件 | 处数 | 它是什么 |
|---|---|---|
| `doc/REMOTE-PHASE0-DEPLOY.md` | 54 | 🔴 **`doc/` 里最大的一份漏网** —— 部署 runbook，产品面，该改而不在本轮写区 |
| `doc/远端支持方案-agent查看器与代码全景图.md` | 26 | 方案文档，该改 |
| `doc/RELEASING.md` | 13 | 发版 runbook，该改 |
| `PHASE-G-REPORT.md` | 10 | 带日期的历史报告 —— 与 `evidence/` 同族，**建议归冻结档** |
| `doc/账号用量-usage抓取方案.md` | 7 | 方案文档，该改 |
| `src/README.md` | 5 | 前端目录 README，该改 |
| `doc/DEVELOPMENT.md` · `doc/STATE-MATRIX.md` · `scripts/README.md` · `shared/cc-bus/SKILL.md` · `.github/SECURITY.md` · `项目审阅报告-PhaseG-2026-07-29.md` | 各 1（共 6） | 前四份该改；后两份是**带日期的历史报告**，同 `PHASE-G-REPORT.md` |

⇒ **本轮之后 `.md` 那一面还欠 ~109 处**（121 减掉两份历史报告的 12 处里该冻结的那部分，
逐份判见上表）。`doc/` 今天 11 份，写区只点了 4 份 —— **这不是漏了，是本轮口径就这么给的**。

## §5 `.md` 之外那一面（本轮**一处都没碰**，给下一件一个分母）

同一把 `(?i)daemon` 尺子，人群 = `git ls-files` 里**非 `.md`** 的被跟踪文件，量于 `897afec`：

| 面 | 处数 | 判 |
|---|---|---|
| `src-tauri/**`（Rust） | 4445 | 绝大多数是符号名 / 模块名；散文注释那一半归另一件 |
| `evidence/**`（`.py` 量具） | 1745 | **冻结面**（同 `evidence/*.md`） |
| `remote-daemon-proto/**`（Rust） | 888 | crate 名与符号名，`R61` 明确不改 |
| 🔴 `src/**`（TS，**含界面文案**） | 862 | **里面有真的界面文案**：`src/settings/machine-card.ts` 那两个按钮逐字写着「安装 daemon」「卸载 daemon」 |
| `e2e/**` | 529 | 套件名 / 脚本名 |
| `.github/**` | 159 | CI **job 名**（`daemon`），改它 CI 对不上 |
| 其它（`scripts/` `hooks/` `shared/` 配置） | 117 | — |
| `vendor/` | 4 | 无权改（`C7`） |
| **合计** | **8749** | 全仓（含 `.md`）改前现打 **10860**、改后 **10502** |

🔴 **界面文案那一档是本轮逼出来的一笔账**：`src-tauri/README.md` 里那两句
「设置面板『安装 daemon』/『卸载 daemon』」是**逐字引用界面按钮**的，
只改文档不改界面 = 文档当场说假话 ⇒ 本轮把那两处登进 `EXEMPT`，
**UI 文案那一档整体交回 PM 另派**（`tool_registry::Why::Wording` 那张存量账
逐字写着同一条：「订正措辞要连前端那一面一起过，只改 Rust 半边会让两边说两种话」）。

## §6 `标识符·不改` 那 139 处，抽样（证明尺子分得开两者）

`remote-daemon-proto`（crate 名）· `remote-daemon-proto/src/observe/watcher.rs`（路径）·
`daemon_send_keys.rs` / `daemon_launch.rs` / `daemon_kill.rs` / `daemon_route.rs`（文件名）·
`--daemon-probe`（子命令）· `daemonPath` / `daemonless` / `RemoteHostConfig.daemonless`（字段名）·
`DaemonHello` / `DaemonTransport`（类型名）· `DAEMON_BUILD_ID` / `EXPECTED_DAEMON_BUILD_ID` /
`CCM_NO_DAEMON` / `CCM_E2E_DAEMON`（环境变量与常量）· `ensure_daemon_deployed` /
`deploy_remote_daemon` / `daemon_send_into`（函数名）· `kill_now_routes_through_the_daemon` /
`every_daemon_file_strips_clean` / `the_daemonless_remote_still_needs_the_ts_fallback_renderer`（判据名）·
`daemon-gate2` / `graylight-daemon-frames` / `daemon-fork` / `daemon-frame`（e2e 套件与帧名）·
`daemon-split`（`planned-build` 工作区名）· `daemon-02` / `daemon-08` / `daemon-09`（阶段号）·
`src-tauri/embedded-daemons`（目录）。

## §7 ⚠ 尺子自己的两个已知漏（两侧都写出来）

1. **中文夹缝里的标识符会被切成裸词** —— `-` 后面跟汉字时 token 扩不过去。
   活体两处：`daemon-协议-v1`（仓外 aterm 冻结的契约文档名）·
   `#发版构建交叉编译--内嵌-daemon-二进制f08b`（markdown 锚点）。
   **今天靠 `EXEMPT` 逐条兜，不是靠 token 规则。** 新长出同形的，尺子看不见。
2. **英文连字符形容词会被当成标识符放过** —— `daemon-spawned plugins` 这一形。
   本轮那 3 处（`daemon-spawned` · `daemon/client` ×2）由**整句改写登记**（`REWRITES`）逐处改掉；
   同形的新增**这道闸看不见**。这是**漏**，不是「没有」。

🔴 **还有一处两住址**：token 规则今天有**两份实现** —— 量具 `K-R116-ruler.py::token_at`（Python）
与判据 `doc_claim_registry::daemon_wording_registry::token_at`（Rust），**没有任何东西钉它们同步**。
本轮用 `d1-1` / `d1-1b` 两刀各切一次，证明两侧都真的在按这条规则判；
**但「两侧永远一致」今天没有闸**。闭集（写区 9 份 · 例外 15 条）那一半已经收成一个住址
（量具**解析**判据里的 `SITES` / `EXEMPT`，不抄第二份），**算法那一半没有**。
