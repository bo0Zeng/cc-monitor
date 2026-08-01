# Phase A 摸底 D — 会被推翻的文档断言 / 必须保住的约束（2026-08-01）

> 四份摸底之一。**这份直接就是用户决策 #3「记得全部搞一下文档」的清单底稿。**
> 标注 ✅ = 我（主线程）亲自复核过。

## 一、必须改（A–J）

### A. `INVARIANTS §40`「本地 = 不走 ssh 的远端」—— **整节的例外结构被决策 #2 拆掉**

| 行 | 原文 | 处置 |
|---|---|---|
| `:1072` | `\| Windows 本地 \| {kind:"local"} \| PowerShell + wt.exe（**唯一的例外，见下**） \|` | 第三行要么消失，要么变成「经本机 daemon 执行」 |
| `:1079-1084` | 「**Windows 本地那两套 PowerShell 是唯一被允许的例外**，理由是那台机器上没有 tmux……」+「2. **不允许再长出第三套**」 | 「没有 tmux」这条**理由不会消失**（Windows 仍无 tmux），但结论要反过来 |
| `:1088-1089` | 「**不改 `shared/ccm` 本体**：它在 POSIX 本地上已经够用」 | **整条重写**（连同 `local-as-remote/MASTERPLAN.md:62`、`unify-launch/MASTERPLAN.md:45` 同款） |
| `:1090` | 「**daemon 零改**：本条只涉及启动路径，不涉及会话监视」 | **语义反转** —— 启动路径恰恰是 daemon 要长出的新面 |
| `:1127` | 「天然不对称白名单（不是欠账，不必补）：**daemon 部署 / 版本协商** —— 本地会话由 `watcher.rs` 直接读 jsonl，不需要 daemon」 | 与决策 #1 正面冲突，白名单要删这一项 |

### B. `INVARIANTS §31`「一端起的会话另一端必须能接」—— **阶段② 到了**

- `:643-644`「前端绝不硬编码后端命令……**问一层要**（阶段②问 daemon，阶段①问前端座 `session-backend.ts`）」
- `:648-653`「**阶段① 落地（F90，2026-07-17）**：唯一后端 = tmux……**本阶段不做后端探测/协商**」
  ⇒ 时态整段改写，`session-backend.ts` 从「唯一命令来源」降级为「阶段① 的形状占位」。
- `:639-640`「会话后端**扶着**跑着的交互程序；后台程序（daemon）是**旁观者**……**协议可合、进程不能合**」
  ⇒ **「旁观者」这个定性要重写；「协议可合、进程不能合」这半句要保住**（daemon 仍不该变成容器本身）。

### C. `INVARIANTS §30`「tmux↔sid 靠 `@ccm_sid`」—— 身份回填的归属要重定

`:589-591`（poller「住 `shared/ccm` 内部」）· `:604-605`（`@ccm_sid` 是「阶段② daemon
`session.status()` RPC 的**先声**」→ 变成「现在就要兑现」）· `:621-628`（`_expect` 段括号里
「守 daemon 零改动的范围排除」这个**理由**作废，但**结论**——`_expect` 不进六列格式——建议保住）。

### D. `INVARIANTS §33` 双渲染器 + `§36` 本地不经 IR

- `:713-715` 渲染目标从「对 `ccm` 的一次调用」变成「对 daemon 的一次 RPC」。
  而 `unify-launch/MASTERPLAN.md:296`「**CLI 未装 → 兜底渲染器**，逐字节等于今天。CLI 是增强，
  不是硬依赖」在「必须装 daemon」之后**前提消失** ⇒ 兜底渲染器留不留要显式裁定
  （留着就是第二条会漂移的路）。
- `:725-732`（铁律②，`send-into` 是「对 `ccm` 当前能力的事实陈述」）⇒ 重新判定；
  **#76 那条防线（防 attach 进空 shell）必须以新形式保住**。
- `:844-846` / `:856-862` §36 整节被决策 #2 推翻。
  ⚠ `:873` 的 L2 复测段（**2026-07-30，就在两天前**）刚刚**再次确认**这条铁律并据此否决了 L2 原方案
  ⇒ **必须写下「为什么这次不同」**（答：不再是"给 PowerShell 渲染器补一段读 env 的代码"，
  而是"Windows 本地整条换成 daemon 执行"，两件事不同）。**不写这一句，会被读成随手推翻自己的审计结论。**

### E. `INVARIANTS §26 / §41 / §41.6`

- `:496`（流模式 flag 必须在查询判定之前剥离）⇒ argv 面从**二分**变三类，判定顺序 +
  `every_capability_token_is_strippable` 护栏都要扩。**这是埋死循环的地方。**
- `:1136-1139` §41 零定时器 + `:1195`「范围只钉 daemon 生产段」⇒ 变执行者后能不能保住，先答。
- `:1211` §41.6 + `:1223-1225`「**恰好一个**模块」「「恰好一个」是断言值，不是描述」
  ⇒ **预信任（`shared/ccm:436-490`，`:454-456` 是 `jq … > tmp && mv tmp "$cj"` 就地覆盖）
  搬进 daemon 当场撞死默认层。**
- `:31` §1-A2「**动凭据的部署操作绝不经 daemon**——那会往只读组件里塞权限」
  ⇒ 「daemon 是只读组件」这个前提没了，论证结构要重写；**但结论要保住**：
  `/login` 必须走真 TTY，daemon 不该碰凭据。

### F. `doc/IPC-PROTOCOL.md`

- `:368-370` §10「**流式，非文件 IPC**……经 SSH stdout 把远端会话流式传回」⇒ 单向协议要长出反向请求面。
- `:396-411` 一次性查询表 + `:411`「**旧 daemon 兼容**：不认参数的旧版会照常发 `hello` 进流模式」
  ⇒ 执行类子命令进不进这张表、旧 daemon 遇到执行类参数会怎样（**会误入流模式并挂住 monitor**），逐条重写。
- `:413-429` §10.1 `--resolve` 三格可信度表 —— **要么做实要么删**（见 §四-4）。
- ✅ **`:431-471` §11「远端终端拉起（ccm-rbind）」整节已经在说谎**（我复核了三处）：
  - `:436`「远端 `remote-section.ts::CCM_WRAPPER_SNIPPET`（安装器写进 `~/.bashrc` 的 shell）」——
    ✅ 今天 `remote-section.ts:104` 只是 `import CCM_WRAPPER_SNIPPET from "../../shared/ccm-aliases.sh?raw"`，
    ✅ 而 `ccm-aliases.sh` 里**只有 3 个别名** `cc()` / `cch()` / `cct()`，**无任何 rbind 实现**。
  - `:440-441`「契约是 `( __ccm_rbind; exec claude ... )`」—— ✅ `__ccm_rbind` **已无任何定义**，
    但**仍有 10 处文本引用**（含 `launch-plan.ts:92` 用它给 `prelude` 字段做说明、
    `launch-dimensions.test.ts:306/316` 拿它当测试串）⇒ **一个已删的原语仍在塑造 IR 类型**
    （正是 §39「悬空设计」那条）。
  - `:446-448`「后台 watcher 每 1s……发 `\033]0;ccm-rbind-<sid>\007`」——
    2026-07-31 起 marker 改由 tmux format 常驻合成（`shared/ccm:644`）。

### G. `doc/ARCHITECTURE.md`
`:19-21` 数据流图右栏（`PowerShell session` / `(跑 cc / __ccm_bind)`）要重画 ·
`:113`（`launch.rs` 职责）· `:126-135` 远端层模块表（`tmux.rs` 那四个「monitor 自己开 ssh 动 tmux」的命令要重定归属）·
`:179`（`session-backend.ts` 「阶段①唯一后端 tmux」）。

### H. 产品文档
`README.md:263`「**只读 daemon** 模块导览」· `remote-daemon-proto/README.md:3-4`、`:7`
（「**运行期 Linux-only**」）、`:35`（「A2 多账号，**全只读**」）。

⚠ `README.md:44` / `README.en.md:42`「**daemonless 降级读取**：不装 daemon 也能纯 `tail` 轮询读远端会话」
—— **与决策 #1 的措辞冲突，见 §四-6，需用户拍板。**

`doc/远端支持方案-agent查看器与代码全景图.md:41`「**daemon 无任何索引能力**」· `:48`「与 daemon 定位冲突」
⇒ 论证支点变了，结论要复算。

### I. `doc/CONTRIBUTING.md §2.7`（`:273-286`）
「改远端会话后端命令」cookbook 的第 1 步在变更后会把人引到错的文件；第 4 步预言的正是本次变更。

### J. 工作区计划文（活文档，会被后人当依据读）
`unify-launch/MASTERPLAN.md:45`、`:347`（「daemon 零改」）·
`local-as-remote/MASTERPLAN.md:52`、`:62`、`:64`、`:85-93`（那张「Linux 本地便宜/Windows 本地不便宜」
的表，结论要重算）·
**`shared/ccm:6` 头注「本文件是……**app 与终端的唯一共同实现**」—— 这是最该第一时间改的一行。**
同文件 `:49-57`「**为什么必须是这个结构**（别"简化"掉，每条都有血）」四条**理由仍然全部有效**，
搬家时必须逐条带走。

## 二、必须保住的 20 条（正交，出处已核）

| # | 约束 | 出处 | 为什么正交 |
|---|---|---|---|
| 1 | tmux 三道门（Gate1 空 target / Gate2 identity / Gate3 `windows==1`） | `INVARIANTS:779-791` | 问的是「对哪个 target 动手」，与「谁动手」无关 |
| 2 | 绝不向用户自己的其它 tmux 会话发按键 | `:59`（A5） | 安全约束；daemon 与用户 tmux 同机后**风险反而更高**（少了 ssh 的天然隔离） |
| 3 | `-t` 恒用 `=<名>:`，尾冒号不能省 | `:659-698` | tmux 自己的解析行为（3.6 实测），跟谁发无关 |
| 4 | `TMUX_LS_FMT` 六列 + 三个双写点 | `tmux.rs:22` · `:593-595` · `:449` | 跨语言 wire 格式 |
| 5 | `RETIRE_MISS_THRESHOLD >= 2`（编译期断言） | `tmux_reconcile.rs:28-33` | 守的是 `/branch` 漂移的 ~1s 竞态，与谁起会话无关 |
| 6 | `remote_active`/`REMOTE_IDLE` 唯一写者是 emitter | `:404-424` · `:426-442` | daemon 变执行者会**新增**第五个信号源 ⇒ 更要守死「零新写点」 |
| 7 | tab 身份钉在会话身份；命中 >1 按后果可逆性分级 | `:597-619` | `:616-617`「代价不可逆」一字不变 |
| 8 | 不许拿 tmux 名/主机名/路径当持久主键 | `:513-555` | daemon 若开始记「我起过哪些会话」，从未来约束变现行 |
| 9 | `readonly_guard` 两层形状；「恰好一个」是断言值 | `:1218-1232` | 放宽必须按 G2「**收窄不删**」先例，**绝不整条删** |
| 10 | 绝不为让守卫变绿而删唯一信号源；daemon 散文不许逐字引用禁用模式 | `:1186-1205`（含 `fork_write.rs` 头注那次真实翻车） | 本次要动的正是这两个守卫 —— 最容易顺手改红线 |
| 11 | `BUILD_ID` 必须 bump | `:1240-1243` · `:458`（P5 漏做那次自陈） | **历史上漏过且全绿**，写进 DoD |
| 12 | **能力 ≠ 身份**（`v`/`build_id`/`capabilities` 三轴正交，绝不用身份代理能力） | `:498`（含 2026-07-09 事故根因） | 新增执行能力时最自然的冲动就是「按 build_id 判断这台能不能执行」= 事故的形状 |
| 13 | 保守缺省族（`kind` 缺=交互 · `status` 缺=未知 · caps 缺=空集 · `cause` 缺=gone · `attachable` 缺=true） | `:500-511` · IPC `:335`/`:390` | additive 兼容基本盘 |
| 14 | `kind` 是**排他式**契约 | IPC `:309-330` | 2026-07-31 刚写进文档 |
| 15 | seq 单调 + 前端 binary insert | `:119-141` · `:177-183` | 纯数据通道 |
| 16 | 行事件 at-least-once ⇒ 按 uuid 累积必须幂等 | `:462-474` | 同上 |
| 17 | 不写 `settings.json`/`.bashrc`/PS profile/`.tmux.conf`（除既有 opt-in 围栏块） | `:17` · `:103-115` | 用户数据安全底线 |
| 18 | UTF-8 无 BOM 双向防御；data dir 恒在 `~/.claude/claudecode-frontend/` | `:93-98` · `:63-89` | 新增 data dir 文件要在 `data_paths.rs` 声明类别 |
| 19 | daemon stdout 只跑 wire，日志走 stderr | `REMOTE-PHASE0-DEPLOY.md:144` | **exec 子进程后这条更脆**，必须显式保住 |
| 20 | §33 精神：**诚实放弃，不得近似** | `:717-718` | 「daemon 应该能做吧」的近似比「ccm 应该能做吧」**更容易蒙混** |

## 三、unify-launch 那一区的结论（本次要接的班）

**核心思想**（`unify-launch/MASTERPLAN.md:14-16` 逐字）：
> 把「起一个会话」从一堆写死的命令，变成「一个动作 + 若干正交修饰」；并让这个模型在 IR、CLI、UI
> 三处是同一个——app 的菜单项、终端的命令行参数、代码里的 IR 字段，是同一模型的三种投影。

**它明确没有统一的**（逐条带理由，本次要重新判）：
本地两条路（「本地无账号维度是**平台事实**」）· attach 不带账号（**是设计不是缺口**）·
SFTP 开终端 · 账号 step（「账号是主语不是修饰」）· 6 个 executor 未合并（与 F03 硬约束互斥）·
`container`/`agent` 不进维度注册表（§38 R12「accepted with documented rationale」，**不是关闭**）·
**daemon 侧零改**（明写范围外）· `cc-spawn` 本体（不在本仓）。

**★ R07 那次订正必须原样带进主计划**（`features/R07-plan-local-honest-name.md:90-98` 逐字）：
> 否决"真接上"引用了**错误的论据**，而且写进了规范性文档。`Get-Command` 论证……排除的是
> "TS 全量渲染好字符串、Rust 只管 exec"这一形态，**并不排除**"TS 构造 IR、Rust 只补 `Get-Command`
> 那一步"。即：**"不接"是因为接了也拿不到新东西，不是因为技术上不可能**。

订正后的真实理由（`INVARIANTS:864-871`）：`plan.action`/`plan.cwd` 恒等于输入、`plan.launcher` 恒 `""`
⇒ **没有信息增量**。⇒ 本次要翻的不是 §36 铁律本身，而是它的**前提**。

## 四、上面没问到但主计划该知道的 7 件事

### 1. ★ `shared/ccm` 那个每秒 poller 是「薄客户端化」最硬的墙

`shared/ccm:623-660`：`while kill -0 "$ccm_pid"; do … printf '\033]0;ccm-rbind-%s\007' …;
tmux set-option @ccm_sid …; sleep 1; done &`

两个不能搬进 daemon 的性质：
1. **1 秒定时循环** ⇒ 正面违反 §41 + `no_timer_guard`（`:1191-1194` 明写判据是「周期性唤醒」，
   且「用 `from_millis(8000)` 偷渡节拍也会红」）。
2. **`printf '\033]0;…'` 必须由一个 stdout 就是用户终端的进程发出。** daemon 的 stdout 是 wire
   （`REMOTE-PHASE0-DEPLOY.md:144` 定为硬约束），它**物理上够不着**用户那个窗口的标题。

⇒ **薄客户端不能薄到「发个 RPC 就退出」**：至少要留一个在用户终端里活着的壳，
且它必须 `exec` 成 claude（`shared/ccm:54`「不 exec 则 PID 对不上」）。**主计划要先答：那个壳还剩多少。**

好消息：这堵墙同时是 **E76 的解**（「贴便签的和看便签的是两个进程」这个前提会消失）。
⚠ 但 E76 里 2026-08-01 的实测结论仍然有效、**别再试**：同名 `rename-session` 不触发 hook。

### 2. ★ 预信任会把 daemon 变成「改动用户既有文件」的进程
撞 `:1211` + `:1223`（白名单「恰好一个模块」+「必须含 `.create_new(true)`」）
+ `:29` 那条已有收窄（`--account-trust` 只回三个布尔、「**绝不回传 `.claude.json` 内容**
（内含 `mcpServers` 的环境变量，可能有 API key）」——daemon 今天连**读**这个文件都被限制到一个布尔）。
**三条出路必须选一条并写下理由**：留客户端 / 走 monitor 的 SFTP 原子写层 / 给护栏开第二个洞（按 G2「收窄不删」）。

### 3. daemon 今天 Linux-only，而决策 #2 要它上 Windows
§41 四路事件全是 Linux 原语。Windows 无 inotify（有 `ReadDirectoryChangesW`）、无 pidfd
（有 `WaitForSingleObject`）、无 `/proc`、**无 tmux**。
`INVARIANTS:635-637` 恰好写着容器分裂的后果：「桌面用 abduco 起的会话、手机只会 tmux → **接不上**，
会话池当场劈成两半」。⇒ **「Windows 同一条路」到底同到哪一层（IR 层？daemon RPC 层？容器层？），
是主计划第一个要划的线。**

### 4. `--resolve` 已经是一条半成品的「daemon 参与起会话」路
IPC `:413-429` 逐字：`sessionName`「**纯从 sid 派生**……**没读过任何 pidfile、没查过 tmux** ——
据它去 attach 一个**并不存在**的 tmux 会话是现实风险」；`capabilities`「硬编码的常见组合」；
`:426` `claude_dir` 标着 `#[allow(dead_code)] // MVP 未用` —— **手上有 claude_dir 却没用**。
而 `INVARIANTS:964-967`：「Codex 的 resume 走 `--resolve` RPC，**完全不经过 `LaunchPlan`/`ccm` 管线**」。
⇒ **`--resolve` 与本次变更是同一件事的两个半成品，不能两条并存**（那正好复现「15 套实现」的病）。
`daemon-codex/` 工作区也在这条线上，要一并对账。

### 5. ✅ 现成的机器化门禁可直接当 DoD
`parity_ledger.rs:555` `assert_eq!(LEDGER.len(), 123, "命令总数变了")`（我复核 = 123）+
`INVARIANTS:1130-1134`「枚举全部 Tauri 命令，每条要么两侧都有、要么在白名单表里且带理由；
新增命令不登记就红」。
⚠ 顺带：`local-as-remote/MASTERPLAN.md:119` 写的是「120 条」—— **文档已落后代码一格**，一起校。

### 6. ★「daemonless」在仓里有**两个意思**，别混 —— 需用户拍板
- **读面**：`README.md:44` + `ssh_source.rs:95-100`/`:2607-2870` 的纯 `tail` 轮询降级 —— **今天真实存在**。
- **起会话面**：`BACKLOG E49②`/`E53` 的「daemon 没装的机器上 `cc` 必须照样能用」。

用户 2026-08-01 那句砍掉的是**后者**。**前者没有被明确砍掉，而它的存在恰好证伪了「没有 daemon 啥都读不了」这个前提。**
⇒ 主计划单列一格请用户表态：留（README 不动，但那句话的适用面在文档里收窄成「起会话」）
还是删（又一块要拆的代码 + 两处 README）。

### 7. ★ 一处会被顺手踩坏的既有守卫
`INVARIANTS:685-687`：`shared/ccm` 的 tmux 目标形态由 `sftp.rs::ccm_cli_has_required_elements`
**结构性扫描**守着，且明写「**不是固定 needle**——固定 needle 版本实测空转，把 `=名:` 全改回裸目标
三门禁仍全绿」。
⇒ **`shared/ccm` 被掏空之后这个守卫会「扫不到东西 = 没有违规」地空转变绿。**
这正是本仓反复吃亏的形状（`local-as-remote/MASTERPLAN.md:251` 记着「栽过三次」）。
**守卫要跟着搬到新的命令构造点，且要有计数自检（处数 == 登记数），不能只是「不红」。**
