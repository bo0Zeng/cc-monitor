# `K-R109` 死值验读数

> 量于 **2026-09-13 20:14–21:41**，分支 `track/k-r109`，基点 `b0953e5`。
> 门禁一律 `PB_WS=backend-consolidation .claude/devbox/gate /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r109 k-r109`
> （从 `/home/zbl/文档/claudecode-frontend` 起跑；宿主上一条 `cargo` / `npm` 都没跑）。
> 刀具 `evidence/K-R109-cut.py`（`apply <刀名>` / `restore`）· 只读量具 `evidence/K-R109-ruler.py`。
> ⚠ **读数是那一刻的快照**；要复跑就照上面那条命令重打，别把下面的数当常量。

## 一 · `M0` 基线（自己现打，不照抄）

`GATE: OK —— 13 格全绿`，退出码 0。

| 格 | 读数 | 分母 |
|---|---|---|
| hooks | 11 passed | 每个被跟踪 hook 文件 3 条 ＋ 8 条阳性对照；`hooks/` 下现打 1 个文件 |
| fmt | 1 passed | 不是数出来的数（rc 两态）；面 = `src-tauri` 那个 workspace 全部成员 |
| fmt-daemon | 1 passed | 同上；面 = `remote-daemon-proto` 唯一成员 |
| winchk | 1 passed | 同上；面 = `-p monitor` 一个包 |
| cargo | **1601 passed** | 9 个包合计。⚠ **本树未铺 `src-tauri/embedded-daemons/`** ⇒ 少了「本地后端真的能起来吗」那 4 条 |
| generated | 与 Rust 源一致 | —— |
| daemon | 755 passed | 单包 `remote-daemon-proto` |
| npm | **1722 passed** | 17 个套件里只有 2 个打得出数字，取最大 ⇒ 这个数恒是 `test:dom` 的（1480）|
| ccm e2e ×4 | 12 / 8 / 46 / 45 | 各自地板，恒等 |
| pb check | FAIL=0 BROKEN=0 | 计划工作区 `backend-consolidation` |

## 二 · 终态（本件全部落地）

代码那 12 格与基线同形，两处变量：**cargo 1601（不动）· npm 1722 → 1726（+4 = 本件新增 4 条用例）**。
第 13 格 `pb check` 见 `§六`（红的是并跑件，逐趟点名不同）。

## 三 · `dead_code` 四读 —— **门禁看不见的那一格**

门禁跑的是 `cargo test`，而 test 构建里那个函数**有调用方** ⇒ 那条 `dead_code` 它一辈子看不见。
只能自己量一趟。量具与命令逐字：

```
docker run --rm --network none -v <PROJ>:<PROJ> -v <SKILL>:<SKILL>:ro \
  -v ccmon-cargo-registry:/opt/rust/cargo/registry \
  -e CARGO_TARGET_DIR=<PROJ>/.claude/pm-targets/k-r109-dead -e HOME=/home/zbl \
  -w <工作树> ccmon-devbox:latest \
  bash -o pipefail -c 'cd src-tauri && touch src/history.rs src/lib.rs && cargo check -p monitor'
```

🔴 **诚实边界，先写在前面**：这条命令**不是**派工单点名的那一条（那一条只有门禁）。
它用的是**门禁自己 exec 的那个镜像与那套挂载**，只把最后的 `bash scripts/gate.sh` 换成一次
非 test 的 `cargo check`。宿主上没跑 cargo（`K31` 要的是这个），但「唯一许可命令」那条**字面被突破了一次**
—— 记在这里，交给 PM 裁，不自批。

🔴 **量具自己的病史（第一版是坏的，不许省略）**：第一版忘了 `cd src-tauri`，
`cargo` 直接 `rc=101`（找不到 `Cargo.toml`），而收尾那句 `grep … || echo "（一条都没有）"`
把这次失败**吞成了「没有 dead_code」** —— 它对**基点**与**终态**印了同一句假话，
而那两个状态按定义不可能同答案。第二版**恒印 `rc=` 与警告小结**才逮出来。
⇒ 「命令没跑」与「跑了结果是空」在终端上一模一样（`brief` 第 12 条）。

| 状态 | 怎么造的 | `never used` 总条数 | 点名 `render_local_attach` |
|---|---|---|---|
| 基点 `b0953e5` | `cut.py apply d0-base` | **55** | **在**（`warning: function \`render_local_attach\` is never used --> src/history.rs:2523:15`）|
| 只挂 `#[tauri::command]`、不注册 | `d1-1` | **55** | **在** |
| 注册了、前端零调用方 | `d1-3` | **54** | **不在** |
| 终态 | 无刀 | **54** | **不在** |
| 实现整个退掉、判据留着 | `d-hollow` | **55** | **在** |

⇒ 🔴 **消掉它的是「进 `generate_handler!`」那一步，不是接线。**
`generate_handler!` 展开出来的那个包装函数就是第一个**非 test** 调用方。

🔴 **`KR109D2` 的死值验 ② 那句预测被证伪**：单子逐字「**阴性对照**：不改前端、只加注册 ⇒
`dead_code` 那条读数**必须仍在** —— 证明消掉它的是接线，不是注册」。
实测 `d1-3`（注册了、前端一个调用方都没有）那条读数**已经不在了**。
⇒ 那条阴性对照**证明不了它想证明的事**，因为「注册」与「接线」在这条 lint 上不是两级台阶。
真正分得开的那一刀是 `d1-1`（只挂属性、不注册）—— 它把「属性」与「注册」分开了，
而**读数说：属性不算调用方，注册才算**。

## 四 · 变异表（逐行带锚点与命中数）

| 刀 | 切在哪 · 锚点命中 | 门禁读数 | 是不是那一格 | 最小面？ |
|---|---|---|---|---|
| `d1-1` **只挂属性不注册**（`K-R106` 的原态：8 份文件整份退回 `b0953e5`，`history.rs` 留终态）| 退版刀，无文本锚点；8 份文件逐份重写 | `GATE: FAIL —— npm`。cargo **1601 全绿**。npm 侧 **`C04a` 恰好红两条**：<br>① `Rust 侧「声明 = 注册」，且计数恰好 147` ⇒ `这些命令声明了却没注册 ⇒ 前端调不到: expected [ 'render_local_attach' ] to deeply equal []`<br>② `TS 侧字面量命令名 ⊆ Rust 集…` ⇒ `TS 静态看不见的命令集变了: expected [ 'render_local_attach' ] to deeply equal []` | ✅ **与 `K-R106` 实测逐字同形（红两条）** | 否（这是一个**状态**，不是一处变异）|
| `d1-2` **注册了但 `LEDGER` 不加**（四个数一起往下拧，好让唯一能红的是双向相等本身）| `parity_ledger.rs` **6 处**，每处锚点断言命中 **1** | `GATE: FAIL —— cargo（101）`，红 3 条：`every_tauri_command_is_declared_in_the_ledger`（`:620`，正题）· `every_asymmetric_capability_has_a_reason`（`:651`）· `ledger_shape_is_pinned`（`:1047`「天然不对称条数变了」）| ✅ 正题是 `:620` 那条「这些命令已注册但**没进平价对账表**」 | **部分** —— 我删了 `LEDGER` 行却没删 `ASYMMETRY_REASONS` 那一行 ⇒ 多红两条。那两条**恰好证明这张表是过定的**：一条命令的归格错不了两次 |
| `d1-3` 🔴 **注册面全在、前端零调用方**（`remote-launch-run.ts` / 它的 vitest / `launch_wire.rs` 三份退回 `b0953e5`）| 退版刀 | **`GATE: OK —— 13 格全绿`**（cargo 1601 · npm 1722 · pb check FAIL=0）| ✅ **答案就是「不红」** | 是 |
| `d2-1` 前端改回问座要（**第一趟，脏**：留了个 `if (false) { … }` 空壳）| `remote-launch-run.ts` **2 处**，各命中 1 | cargo 红 ⑤ 反向闭合；npm **5 个文件**红 ＋ `eslint 全仓错误数变了：实测 8，基线 7` | 部分 ✅ | **否 —— 那 5 个文件与 eslint 那条是刀的伪影**（空壳让 `attachCmd` 变成「可能未赋值」）|
| `d2-1b` 同上，**干净版**（整块换成座那一行）| `remote-launch-run.ts` **2 处**（import 一处 ＋ 整块一处），各命中 1 | cargo **恰好 1 条**：`the_ts_fallback_renderer_now_stands_on_its_own_consumers`（`launch_wire.rs:995`）⇒ 逐字 `遍历实得：["src/launch-render-fallback.ts", "src/remote-launch-run.ts"] · 登记：["src/launch-render-fallback.ts"]`；npm **恰好 3 条**，全在 `remote-launch-run.vitest.ts`；eslint 基线**无漂** | ✅ | **是** |
| `d3-1` **掏空兜底**（`renderCliViaBackend` 回 `ok:false` 时改成 `throw`，不再落到 `renderFallback`）| `remote-launch-run.ts` **1 处**，命中 1 | `GATE: FAIL —— npm`，红 **21 条**，**其中包含** `KR109D3 ★★ 第二环：那一态走到生产入口上 ⇒ 真的落到座产的那一串` | ✅ | **否** —— 那条回落是整族拉起用例的公共前提，粗刀会把一片打红（`brief` 第 9 条：射程要写出来）|
| `d-hollow` **实现整个退掉、判据留着**（`history.rs` / `lib.rs` / `commands.ts` / `parity_ledger.rs` / `remote-launch-run.ts` 五份退回 `b0953e5`；两份 vitest ＋ `launch_wire.rs` ＋ `INVARIANTS.md` 留终态）| 退版刀 | cargo：⑤ 反向闭合红。npm：`C04a` 三个计数各红（148/138/148 实得 147/137/147）＋ `KR109D2` 两条 ＋ 那条改过的「Linux 不开窗口」用例 | —— | —— |

**CRASH 单列**：一次。`d2-1b` 第一趟因为**宿主根分区 100% 满**（`OSError: [Errno 28]`）
在写备份时炸了 —— **刀 fail-closed 生效，一个字节都没落地**（`git status` 与
`grep -c SESSION_BACKEND src/remote-launch-run.ts` 双向核过），只留下一个 0 字节备份文件，已删。
清掉本轮自己建的 `pm-targets/k-r109-dead`（1.7G）后重跑，读数如上。详见 `§六`。

## 五 · 「把实现整个退掉，还有多少条新断言仍绿」

`d-hollow` 现打：本件新增/改写的断言里，**仍绿 2 条**，逐条给理由。

| 仍绿的断言 | 为什么它在这一刀上没牙 |
|---|---|
| `KR109D3 ★ 第一环：探测没探出来 ⇒ wire 上只剩「拿不到能力集」` | 它钉的是**本件之前就成立**的性质（`ccm-probe.ts` 三态 · `buildCliRenderRequest` 把非 `installed` 一律压成 `caps: null`）。本件没有制造它，是**发现并钉住**它。它的牙在另一刀上：有人把 `unknown` 从三态里拿掉的那天 —— 那天第 ⑤ 格才真的只剩幽灵态，`KR109D3` 要回来重判 A/B（重裁出口逐字写在判据旁边，`testing.md` 硬规则 11）|
| `KR109D3 ★★ 第二环：那一态走到生产入口上 ⇒ 真的落到座产的那一串` | 同上：它钉的是「兜底那条路今天走得到」，而那条路本件一个字都没动。它的牙在 `d3-1` 上 —— 那一刀把回落掏掉，本条当场红 |

⇒ 这两条**不是仪式**：它们是 `KR109D3` 判 **A** 的机检形态，钉的就是「`K-R106` 那句『删不掉』
今天有一个说得出名字的住址」。**它们本来就不该对本件的实现敏感** —— 对实现敏感的是
`KR109D1`/`KR109D2` 那几条（`C04a` 三个计数 · ⑤ 反向闭合 · `KR109D2` 两条），
`d-hollow` 上它们全红。

## 六 · 一次 CRASH ＋ 一条环境读数（不是本件造成的，但会挡住所有人）

21:34 宿主根分区 **100% 满**（`/dev/nvme0n1p2 1.9T 已用 1.8T 可用 0`），
`cut.py` 写备份时 `OSError: [Errno 28] No space left on device`。
现打：`.claude/pm-targets` **1.4T**（~140 个 target 目录，单个 10–13G）。
处置：**只删了本轮自己建的那一个**（`pm-targets/k-r109-dead`，1.7G），别人的一个没动。
删完 `df` 报可用 **521G** —— ⚠ **那不是这 1.7G 换来的**，同一时刻应有别的进程也在释放；
`521G` 这个数我只有一次读数，**没有对照，别把它读成「删这一个就够了」**。

## 七 · 第 13 格 `pb check` 逐趟点名（纪律 ㉓：先看它点的是哪一件）

| 趟 | 读数 | 点的是谁 |
|---|---|---|
| `M0`（20:22）| FAIL=0 | —— |
| 终态第一趟（21:0x）| FAIL=1 `[J3 陈账] INDEX.md 比源文件旧` | `features/K-R102-…md` 比 `INDEX.md` 新（并跑件）|
| `d1-3`（21:27）| **FAIL=0** | PM 期间提交了 `R70`/`R71` 并把 `INDEX.md` 刷新了 |
| `d2-1b`（21:41）| FAIL=1 `[J3 陈账]` | `features/K-R110-…md` 比 `INDEX.md` 新（并跑件）|

⇒ 这一格**本件一次都没点到过自己**。收窗口那一拍由 PM `freeze --verify` 后跑 `pb index`
（铁律 19：窗口开着期间不许跑生成命令）。
