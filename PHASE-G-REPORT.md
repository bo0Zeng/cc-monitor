# Phase G 最终验收报告 —— 四区连续执行（2026-07-30）

> 范围：`.claude/planned-build/README.md`「四区连续执行顺序」表 #1-#20，共 36 个 commit
> （`4e7b100..f2f537a`，即 zero-poll P0 之后的全部）。分支 `account-ux`，**未 merge、未发版**。
> 依据：`planned-build` skill 的 Phase G 四项（整体审计 / 主计划终账 / 端到端验证 / 收尾汇报）。

---

## 0. 一句话

**四区功能全部走完**（`rust-ts-boundary` 8 · `account-zero` 能做的全部 · `gate-integrity` 3 ·
`zero-poll-liveness` 8 · `local-as-remote` 6）。全门禁绿、红线全部守住。
**但本报告最该被读的不是交付清单，是 §3 的「订正」与 §5 的「交了一半」两节。**

---

## 1. 端到端验证（CI 原样命令，本轮全部复跑）

| 门禁 | 结果 |
|---|---|
| monitor `cargo test --all` | **638 passed**（3 ignored） |
| monitor `cargo clippy --lib` | **36 warnings**（既有基线，零新增） |
| monitor `cargo fmt --check` | clean |
| daemon `cargo test` | **173 passed** |
| daemon `cargo clippy --all-targets -- -D warnings` | **rc=0** |
| daemon `cargo fmt --check` | clean |
| `npm test` | **872 passed / 58 files** |
| `tsc --noEmit` | **0** |
| `npm run check:types` | rc=0，生成物 **67** |
| `npm run build` | rc=0 |
| `eslint` | 7 项既有基线（未新增） |
| shellcheck | **37 文件**（地板 37），`--severity=error` rc=0 |
| vendored `run-tests.sh` | **294/294** |
| exec-bit / py_compile | rc=0 / rc=0 |
| 13 套真机 e2e 断言地板合计 | **213** |
| `.vendor_id` | `e24bfd164014351a` |

**三处命令计数钉死数互相自洽**：`parity_ledger` 121/50/20（7 天然·11 欠账·2 未裁定）+
`checked 67` · C04a 121 · 包装层 110。

---

## 2. 交付了什么（按区）

| 区 | 交付 |
|---|---|
| **rust-ts-boundary** | C01-C05 全 8 项：Rust↔TS 边界改成生成物 + 门禁（生成物必须最新） |
| **account-zero** | Z01/Z04/Z05/Z06/Z07/Z08 交付，Z02/Z03 **部分**。核心：**账号 0 = 「`configDir` 键缺席」这个状态本身**，判据是结构性的、不认名字；「空值 ≠ 未设」在 bash/Rust/TS/命令行四处一致 |
| **gate-integrity** | G-A/G-B/G-C：断言数地板（`assert-pass-floor.sh` fail-closed 三路）· vendored 1348 行 bash 进 shellcheck + 它自己 424 行测试进 CI · 6 套 e2e 加 socket 隔离并进 CI，**E41 已销** |
| **zero-poll-liveness** | P0-P7 全区收官：**A/B 两条轮询都删掉，生产段零定时器**。判活改四路内核事件；「多个会话里杀掉一个」**16s → 126ms**（对照组：拆掉 hook 5042ms）。`pidfd` 让 **PID 复用在机制上不存在**（唯一一条正确性改进）。`INVARIANTS §41` 落档，**BACKLOG E34 结案** |
| **local-as-remote** | L5 平价对账表（121 命令 → 50 能力，**20 项不对称逐条带理由**）· L0 可构建半 · L1 两半 · L2（**原方案否决**，改做跨语言漂移点守卫）· L3a 枚举半 · L4 两个 Linux CI/release job |

---

## 3. ★ 本次跑动最有价值的产出：**订正了 10 组计划断言**

「开工前复测计划里的断言」**连续 23 轮每轮都抓到不符**。这些不是执行走样，多数是**计划写下时就错**
或**被别的功能解决了而计划没同步**：

| # | 计划/记档说 | 实测 |
|---|---|---|
| 1 | E41 的病因是「6 套 e2e 一处 `-L` 都没有」 | **病因是继承 `$TMUX`** —— 有 `$TMUX` 时 tmux 客户端**完全忽略** `TMUX_TMPDIR`。只加 `-L` 治不好（另：socket 路径有 108 字节上限） |
| 2 | Z07 的 D1b：「隔离项会被自动 symlink 给每个账号 = 静默串号」 | **假的** —— `share_items()` 两个分支都跳过隔离项。改判为 **root=cfg/home** 的正确判据（4 份文档已订正） |
| 3 | Z01 记「launch-plan 今天只会 export」 | 假的，unset 注入**早就有两条路** |
| 4 | P5 的账：`BUILD_ID` bump 排在 P5 | **P5 漏做**，P7 复测才抓出。**这条常量改不改都全绿**（173+638+872 个测试无一会动），漏掉的后果是整轮工作在已部署远端**休眠** |
| 5 | P6 载体「两条路都比直接并入贵」 | **过时** —— G-C 已把隔离做掉了 |
| 6 | （P6 并轨时）—— | **撞出一个 P5 留下的真回归**：`graylight-daemon-frames` 第 2 段靠 8s ticker 重发快照，ticker 删了它就红。**对照组确认非本轮引入**。教训：**删周期性信号时还要跑依赖那个节拍的 e2e**（那 6 套是 CI-only） |
| 7 | E40：本地 daemon 可能与 tmux 同锅 ⇒ pidfd 探针被一起端 | **同锅是实测事实**（本地 tmux 继承调用者 cgroup，不像 SSH 每次得独立 scope），**但前提不成立** —— 本地判活是 app 进程内的 jsonl-watcher，**根本不起 daemon**。复活条件已写死 |
| 8 | E31：5 个模块**只为** `shell_quote` 依赖 4847 行的 `ssh_source.rs` | **中心断言不成立** —— 实测 7 个模块（`ssh_source.rs` 现 4960 行），**没有一个是「只为它」**。搬家**一条依赖边都断不掉** ⇒ **订正记录、不做** |
| 9 | L2 = `planLocal` 复活 + PS 渲染器 honour `plan.env` | **三条全否**：撞 `INVARIANTS §36` **铁律**（逐字禁止）· 撞 R07 已审计的决定 · **收益「嵌套 env 清理首次生效」是事实错误**（`lib.rs:124` 的清洗早于 `:161` 的 Builder，保护一直都在）。⇒ 改做它**真正的意图**：钉住真实存在的跨语言漂移点 |
| 10 | Tauri 命令数 119 | 120 → 121（且天真 `grep -c` 会数出 127：5 处散文 + 1 处 doc 注释造的假阳性 + 1 个平台 cfg 变体） |

另有 L0 漏做账本第 6 行的 `cfg(windows)` 清单（L1 补做）· L3a 计划行里混了「注入/per-account」越界内容（只做了枚举那半）。

---

## 4. 主计划终账（Phase G 第 2 项）—— **又抓到 8 处过时**

对照四区 `MASTERPLAN.md` 的功能清单逐行核，发现**已交付却仍标「未开工/待规划」**：

| 区 | 行 | 原状态 | 实际 |
|---|---|---|---|
| zero-poll-liveness | P4 / P5 / P6 / P7 | 未开工 | 全部已签收 |
| gate-integrity | G-A / G-C | 待规划 | 均已签收 |
| account-zero | Z01 | 待规划 | 已签收 |

**原因**：我在宣布 zero-poll「收官」时只改了 MASTERPLAN 顶部状态行 + STATUS，**漏了 MASTERPLAN 自己的功能表**。
Phase G 第 2 项存在的意义正是接住这个。**全部已修**（`local-as-remote / L3b` 保持「待规划」—— 它是真没做）。

顺带修掉两处结构损伤：
- **`local-as-remote` 的 L2 行被我改成了 5 列**（表头 6 列）—— L2 那轮我只核了 feature 文档的列数、没核 MASTERPLAN。已补回。
- `zero-poll-liveness:404` 的「L1 尚未开工」已过时。

修完全量复扫 **148 个文档，表格列数异常 0**。

---

## 5. ★ 交了一半的，逐条标清（**别当成做完了**）

| 项 | 做了什么 | **没做什么** |
|---|---|---|
| **L0** | Linux 上 `npm run build` + `cargo build`（完整 app 二进制）都 rc=0；三个 WebKitGTK 依赖本来就在 ⇒ 计划最担心的「碎片化很痛」**没发生** | **「起 app」待授权**。理由不是谨慎：`BUILD_ID` 已 bump 成 `p1r-event-liveness`，本分支构建的 app 一连上用户已配置的远端就会把 daemon 判 stale ⇒ **自动重装**（对外、改用户真实机器）。另：那条「不痛」的结论**只覆盖一台机器，不外推** |
| **L2** | 跨语言漂移点守卫（6 条） | **Windows 真机 8 套 152 条断言无法验证**（本机是 Linux）。缓解：L2 **没改任何 Rust 生产代码**，Windows 行为面零变化 —— 但**没验就是没验** |
| **L3a** | 本机账号枚举（只读），输出类型与远端逐字段相同；对账表 `accounts.list` **真的结清一条** | **账号注入 / per-account model / UI 入口全没有** —— 后端与 `fetchLocalAccounts` 在了，但**没有面板去调它，用户暂时看不到** |
| **L4** | `ci.yml::linux-app-build`（**会真跑**） | `release.yml::build-linux` **从未真跑过**（只在 tag 触发）· AppImage 待后续 · **deb 装上去能不能跑没验** |
| **zero-poll** | 全区收官 | **真机重部署待用户** —— 不重部署，这轮改动在已部署远端不生效 |
| **account-zero** | Z01/Z04-Z08 | Z02 三态化 / Z03(b) / Z01-follow **全卡 `tabs.ts`**（红线不碰）；Z08 第二半 `share <item>`；Z06 第二半 `mcp.rs` 三候选双写点；`verify` 硬失败范围未收紧 |

---

## 6. ★ Phase D 的账：**全区欠账，不是强度裁剪**

`planned-build` 铁律 8 要求 **Phase D 并行多 agent 审计**；Phase G 第 1 项要求调 **`/full-audit`**
（其 SKILL 明写「多 agent 并行做」）。

**本会话有常驻指令「除非用户要求不开 agent」** ⇒ 两者都**没做**。代替方案：

- **主线程变异验收**（每个功能 3-6 条，含「护栏自身失效」这类反向自检）
- **全门禁 + 真机实跑**（私有 socket、对照组、逐条 rc 读数）
- **主线程跨区审计**（本报告 §4 就是它的产出 —— 而且真抓到了 8 处）

**这是欠账，不是强度裁剪。** 每个 feature 文档里都逐字写明了这一点。
若要补，`/full-audit` 是现成入口，用户说一句即可。

---

## 7. 红线核对（整会话 `4e7b100..HEAD`）

| 红线 | 核实方式 | 结果 |
|---|---|---|
| 不改 `TMUX_LS_FMT` | 两处 `const` 定义行与会话前**逐字比对** | ✅ 未变（守卫 `tmux_ls_fmt_double_write_point_stays_in_sync` 绿） |
| 不改 `RETIRE_MISS_THRESHOLD >= 2` | 同上 | ✅ 未变 |
| 不改 `shared/ccm` 本体 | `git diff --stat` | ✅ 一字未动 |
| 不碰 `tabs.ts` | 同上 | ✅ 一字未动 |
| daemon 只读铁律 I7 | `readonly_guard` | ✅ 绿 |
| 生产段零定时器 | `no_timer_guard`（3 条） | ✅ 绿 |
| 不改 workflow 触发条件 | 两个 workflow 的 `on:` 会话前后**逐字比对** | ✅ 均未变 |
| 不写 `~/.claude/settings.json` / `~/.bashrc` | mtime 07-29 / 07-27，**均早于本会话** | ✅ 未写 |
| 不动 `~/.claude-accts/` z/b 真实文件 | `accounts.json` mtime **07-26 17:41:02** size **413**；z/b 仍是两个指向 `~/.claude/settings.json` 的软链 | ✅ 未动 |
| 不碰实况 daemon | `~/.cc-monitor/bin/cc-monitor-remote` mtime **07-28 16:20** | ✅ 未碰 |
| tmux 纪律 | 全程私有 socket（`unset TMUX` + 短 `TMUX_TMPDIR`）；默认 socket **hook 数 57**（基线 57）、无孤儿 server、`/tmp` 零残留 | ✅ |
| 不 merge / 不发版 / 不起 app | — | ✅ |

> 说明：默认 socket 现有 4 个会话（基线 3 + `cc-AuthorKit27`，15:19 创建、命名符合 `cc-spawn` 约定）。
> **不是本会话产物** —— 本会话所有 tmux 操作都带 `-S <私有 socket>`。

---

## 8. 建议的下一步（按「值不值得先做」排）

1. **重部署 daemon**（`p1r-event-liveness`）—— 否则 zero-poll 整轮工作在远端不生效。**收益最大、成本最低**。
2. **裁定 L5 交出的 2 项未裁定**：panorama 远端代码图谱（21 条命令，最大一块）· 远端 ccm 安装向导。这两项本表**刻意没替产品做主**。
3. **L3a 的 UI 入口** —— 后端已就绪，接一个面板就能让用户看到本机账号。
4. **L5 实测发现的 3 项欠账**：`cc-bus.cockpit`（驾驶舱管不了本机 agent）· `session.tasks`（远端 tab 拿不到任务）· `subagent.load`（远端 subagent 展不开）。三条都是**实测**出来的真缺口。
5. **发一次版**验证 `release.yml::build-linux`（它从未跑过；失败不会伤及 Windows 产物）。
6. **补 Phase D / `/full-audit`** —— 若希望把 §6 那笔账结掉。
7. `tabs.ts` 解禁后：Z02 三态化 + Z03(b) + Z01-follow（七步顺序见 `account-zero/features/Z02-PARTIAL.md` §6）。
8. 明确**不打算做**的：`graylight-suite` 进 CI（要 GUI runner）· E31 搬家（收益为零）· E39 补定时器（**绝不** —— 那等于零轮询造假）。
