# K-P6b 读数 —— `KP6bD1` 三格的原始读数

> 量于工作树 `.claude/worktrees/k-p6b`，分支 `track/k-p6b`，**被测提交 `b4530d8`**（分支基点，本轮无生产改动）。
> 量具住址：`evidence/K-P6b-ruler-diff.py`（同树同分支）· 输出落 `evidence/K-P6b-ruler-diff.out`。
> ⚠ 本文件里每个数都带「量于哪一刻 · 用什么量的」；**引用前重打**，别当常量。

---

## ① `§0c` 那两把尺子 —— **`K-P6 §0-订正` 那一组对，`§0c` 那一组错**

### 先排掉两个「不是原因」的原因

| 猜想 | 实测 | 结论 |
|---|---|---|
| 人群不同（`src-tauri/src` vs `src-tauri`） | 两把尺子的人群**都是** `src-tauri/src/**.rs` 跟踪文件 = **106 份**。`K-P6-dial-census.py` 传的 `scope="src-tauri"` 被 `tracked_rs_files` 里那句 `if scope == "src-tauri" and not rel.startswith("src-tauri/src/")` 折回 `src-tauri/src/` | **不是原因** |
| 树动过（`K-W4b` · `K-R28` 两次合并） | 同一把尺子在 `55c7fde`（`K-P6` 量的那棵）与 `b4530d8`（今天）上，三个符号**六个数一个不差** | **不是原因** |

命令（两趟，`--rev` 只走 `git show`，不切工作树）：

```
python3 evidence/K-P6b-ruler-diff.py --selftest
python3 evidence/K-P6b-ruler-diff.py --rev 55c7fde151b54e1c59b23fc478f9b493c94f6fc7
```

### 差额在三根轴上，不是一根

一把「数某符号有几处」的尺子由三根轴定死；`§0c` 与 `K-P6` 各自只写了两根，**没写的那根就是差额的家**：

- **轴 A 测试段怎么切**：`none`（不切）/ `cheap`（按**第一个** `#[cfg(test)]` 一刀切到文件尾）/ `guard`（`guard_core::production_code` 的逐块剥法）
- **轴 B 注释剥不剥**：`keep` / `strip`（只认 Rust 词法 `code` 态）
- **轴 C 什么算一处**：`call` / `call+def` / `paren`（`grep -o '<名>('`）/ `any`

3 × 2 × 4 = 24 格全打，**要求三个符号同时对上**才算认出那把尺子（只对上一个不算 —— `§0c` 犯的正是这一条）：

```
【认尺子】
  『K-P6 §0-订正』 生产三格全对的组合 2 个：
        none/strip/call
        guard/strip/call
  『K-P6b §0c 生产』 生产三格全对的组合 2 个：
        cheap/keep/call+def
        cheap/keep/paren  ＋测试格也全对
```

⇒ **`§0c` 那把尺子被唯一认出来了**：`cheap` 切 ＋ 不剥注释 ＋ `grep -o '<名>('`。
它把 `§0c` 公布的**六个数**（生产三格 ＋ 测试三格）**一个不差**全打出来。

### 🔴 `§0c` 那把尺子错在哪 —— 仓里已经判过它一次

`cheap` 这一档就是 `ssh_source.rs` 自己的 `write_half_guard` 头注点名的那个近似。
本树 `src-tauri/src/ssh_source.rs:3189`，那一行逐字是：

```
    /// 这不是洁癖：本文件第一个 `#[cfg(test)]` 模块在 800 行附近，而本护栏要扫的
```

紧接着两行逐字：「monitor 侧此前流行的那个近似（`split("\n#[cfg(test)]").next()`）会把扫描面
**砍掉三分之二**」。旁边还挂着一条**活的**测试
`the_shared_stripper_keeps_the_part_this_guard_must_scan` 天天在跑这个差。

本量具在 Python 侧把那条测试**独立复打**一遍，同样三个锚点、同样的结论：

```
  自检 2 `guard` 与 `cheap` 的差
     `fn parse_frame          ` guard 里 在  · cheap 里 不在   ✓
     `async fn stream_loop    ` guard 里 在  · cheap 里 不在   ✓
     `async fn probe_daemon   ` guard 里 在  · cheap 里 不在   ✓
     生产段体量：整份 318615 B · guard 留下 172703 B (54%) · cheap 留下 33987 B (10%)
```

⇒ `ssh_source.rs` 有 **26** 个 `#[cfg(test)]`，第一个在 **915** 行，而文件 **8336** 行。
`cheap` 把 915 行之后**全部**判成测试段 ⇒ **生产段只剩 10%**。

**它因此丢掉的 4 处真生产调用点**（`guard/strip/call+def` 下逐处列出，行号带逐字校验位）：

| 住址 | 那一行逐字 |
|---|---|
| `src-tauri/src/ssh_source.rs:2083` | `let (session, _fp) = connect_session(cfg, None, None).await?;` |
| `src-tauri/src/ssh_source.rs:2257` | `let (session, _fp) = connect_session(cfg, None, None).await?;` |
| `src-tauri/src/ssh_source.rs:4748` | `let (session, _fp) = connect_session(cfg, None, None).await?;` |
| `src-tauri/src/ssh_source.rs:5755` | `match connect_session(&cfg, Some(Duration::from_secs(30)), Some(on_stage)).await {` |

另外 `paren` 那一档把**定义行**也算一处（`ssh_source.rs:701` 的 `pub(crate) async fn connect_session(`），
这就是 `connect_sftp` 那格 `15` 与 `14` 差的那 1 处（`sftp.rs:47` 的 `pub async fn connect_sftp(cfg: &RemoteConfig) -> Result<SftpConn, String> {`）。

### 🔴 `§0c` 从「中间那格逐字相同」推出的结论也是错的

`§0c` 逐字写着：「`connect_and_exec_cmd` **逐字相同**（18/11）⇒ 那一组是**生产段口径**，这一点确定了」。

**不确定**。那个 18 是**两条不同的路各自到达的**，读数逐格：

| 组合 | 生产 处/份 | 测试 处/份 |
|---|---|---|
| `none/keep/call` | 30 / 13 | 0 / 0 |
| `none/strip/call` | **18 / 11** | 0 / 0 |
| `cheap/keep/call` | **18 / 11** | 12 / 6 |
| `cheap/strip/call` | 16 / 10 | 2 / 1 |
| `guard/strip/call` | **18 / 11** | 0 / 0 |

- `keep`（不剥注释）比 `strip` **多 12 处**（30 − 18），多出来的全是注释里提到的；
- `cheap` 又把 **12 处**切到测试侧（30 − 18）—— 而这 12 处里 **10 处是注释、2 处是真代码**
  （由 `cheap/strip/call` 的 16 与 `none/strip/call` 的 18 之差定出那 2 处）。

⇒ **+12 与 −12 恰好抵消，而两个 12 装的不是同一批东西。** 这正是「两处差额相互抵消」那一形，
不是「同口径」。**一个符号撞对了不能认尺子** —— 本量具的【认尺子】表因此要求三个符号同时对上。

### ⇒ 哪把尺子对（答死）

**`K-P6 §0-订正` 那一组对**：`connect_session` **7 处 / 3 份** · `connect_and_exec_cmd` **18 处 / 11 份** ·
`connect_sftp` **14 处 / 4 份**（单位：**调用点**，不含定义行与 `use` 提及）。

而且这一组**同时也是生产段的数** —— 这不是我替它辩护，是量出来的：
`guard/strip/call` 与 `none/strip/call` 在这三个符号上**给出同一组数**，因为

> **这三个符号的调用点，在测试段里一处都没有**（`guard/strip/call` 三行的测试格全是 `0 / 0`）。

⚠ **诚实边界，这句「一处都没有」的分母**：分母 = `src-tauri/src/**.rs` 跟踪的 106 份里、
被 `guard` 剥法判为测试段的那部分文本，词法按标识符边界、只认 `code` 态。
**它数不到**：`use` 别名（`use ssh_source::connect_session as f;`）与函数指针（`let f = connect_session; f(..)`）——
这个洞与 `K-P6-dial-census.py` 头注第 42 行那条**是同一个洞**，本量具不声称堵住它。

⚠ **`guard` 这一档自己也有一处已知的向上偏**：`guard_core::test_module_ranges` 按契约**只剥
带花括号体的 `#[cfg(test)] mod X { … }`**，挂在别的 item 上的 `#[cfg(test)]` 留在「生产段」里。
本树活体：`ssh_source.rs` 的 `#[cfg(test)] const KNOWN_FRAME_KINDS`（自检 1 现打：剥完仍剩 **1** 个
`#[cfg(test)]`，而 `#[test]` 是 **0** —— 后者才是 `assert_no_test_code` 断的那个）。
本轮三个符号**都不落在那一块里**，所以这一偏对本轮读数无影响；但**别把 `guard` 当完美尺子**。

---

## ② `§0b` 那四条，逐条自己重打（`K-P7` 是 09-06 早上量的，中间主干动过两次）

| # | `§0b` 的话 | 我怎么打的 | 读数 | 今天成不成立 |
|---|---|---|---|---|
| 1 | **origin 仍由「哪条连接」决定 ⇒ 协议面零改动** | 读 `src-tauri/src/inbound_client.rs` 的登记面 | `register(origin: &str, client: Arc<InboundClient>)`（`:649`）· `unregister`（`:660`）· `client_for(origin)`（`:700`）· `pub const LOCAL_ORIGIN: &str = "<local>";`（`:684`）—— **origin 是登记表的键，一条连接一个客户端**；`remote-daemon-proto/src/wire.rs` 的 `Frame` 上**没有** origin 字段 | **成立** |
| 2 | **六根针一根不碰** | 数 `single_stream_guard.rs` 的 `PINS` 五元组条数 | **6 条**，逐条：`main.rs/writer_task(` 2·2 · `main.rs/inbound::spawn(` 2·2 · `main.rs/busy.swap(true` 1·1 · `observe/watcher.rs/mpsc::channel::<Frame>(` 1·3 · `listen.rs/Admit::Stream` 1·2 · `main.rs/REPLY_BURST` 2·2 | **成立**（现打就是 6） |
| 3 | **additive** | —— | 见下方「诚实边界」 | **判不了**（见下） |
| 4 | **`Overflow.lost` 不动** | 读 `wire.rs` 的 `Overflow` 变体 | `wire.rs:387` `lost: Vec<LostFrame>,` · `wire.rs:391` `lost_truncated: bool,` —— 两个字段都在，本轮一个字节没动 | **成立** |

### 第 3 条为什么我写「判不了」而不是「成立」

「additive」是一句**关于将来那一版**的话（新加的东西只增不改）。本轮**没有落任何生产改动**
⇒ 盘上不存在可以拿来判 additive 的 diff。我查过的路：① `git diff b4530d8` 生产面为空；
② 件文件 `§1` 没有给 additive 定判据；③ `K-P7` 那句是**对候选形状的预判**，不是对某份 diff 的读数。
⇒ 三条路都答不了，按第 14 条**明写「判不了」**，不写成「复核通过」。

---

## ③ 🔴 `KP6bD1③` —— E 与 B 正面比（**给读数，不给感觉**）

### 先答 PM 那个前提：**它是对的**

PM 的话逐字：「B（给协议加 `origin` 维）在这个场景下少一个进程 —— **本机后端本来就是界面起的子进程**」。

**Windows 客户端上，本机后端确实是界面起的子进程。** 证据链三环，逐环给住址：

1. **Windows 发版真的编一份原生 Windows daemon**：`.github/workflows/release.yml` 的
   `Build local backend sidecar (native)` 步（`working-directory: remote-daemon-proto` · `cargo build --release`），
   下一步 `Stage sidecar for externalBin` 把 `cc-monitor-remote.exe` 拷成
   `src-tauri/binaries/cc-monitor-remote-$triple.exe`。该步头注逐字：
   「**必须原生编：daemon 在 Windows 上真编得过（2026-08-04 真机实测 exit=0、release 2.6 MB，且跑起来发完整 hello）**」。
2. **它真的被打进安装包**：`tauri build` 那步带 `--config src-tauri/tauri.sidecar.conf.json`，
   而那份配置的全文就是 `"externalBin": ["binaries/cc-monitor-remote"]` ⇒ 装完落在 `monitor.exe` 旁边。
3. **消费侧在 Windows 上找得到它**：`local_daemon.rs:1113` 的 `resolve_daemon_bin` **先**走
   `local_backend::resolve_beside_this_exe(env!("CCM_TARGET_TRIPLE"))`（`local_backend.rs:799`，
   用 `std::env::consts::EXE_SUFFIX`，**跨平台**），**拿不到才**回落到内嵌字节；
   而那道 `cfg!(target_os = "linux")` 闸（`local_daemon.rs:1383`）**只管内嵌那一路**。

### 🔴 顺手逮到一条：`local_daemon.rs` 头注里有一句对 Windows 说过头了

`local_daemon.rs:1475–1477`（`K-H2b` 的「判不了的」那一段）逐字：

> - **Windows 上这条路的运行时行为**：内嵌的那两份 sidecar 是 musl Linux 二进制，
>   `start_local_backend` 里那条 `cfg!(target_os = "linux")` 闸对本函数**同样适用**
>   —— 非 Linux 宿主上 `resolve_daemon_bin` 拿不到东西，本函数就不会被调到。

**最后那半句在发版的 Windows 包上不成立**：`resolve_daemon_bin` 的**第一条**路是 `resolve_beside_this_exe`，
而发版包里那份 sidecar 就在 `monitor.exe` 旁边 ⇒ 它**拿得到**，`start_local_relay(bin)`（`:1403`）
**会被调到**，中转在 Windows 上**会起**。
那句话只在「Windows 开发构建、exe 旁没有 sidecar」时才对。
⚠ **不在本轮写区**（`local_daemon.rs` 我一个字没动），**登记上报，交 PM 定去向**。

### 正面比：两条路各要付什么

⚠ 单位说明：「新增进程」数的分母 = **一台 Windows 客户端上，为了让拨号离开界面进程而多出来的常驻进程数**。
今天那台机器上已有：`monitor.exe`（界面）＋ `cc-monitor-remote.exe`（本机后端，被监护）
＋ `cc-monitor-remote.exe --relay`（中转，被监护，`local_daemon.rs:1403` 起）。

| 轴 | **B**（协议加 `origin` 维） | **E**（字节代理，照 `--relay` 形状） |
|---|---|---|
| Windows 客户端**新增**常驻进程 | **0**（复用已在跑的本机后端） | **+1** |
| 协议面（`wire.rs` / `inbound.rs`）改动 | **要动** | **0** |
| 与本件 `DoD` 的关系 | 🔴 **直接冲突 `KP6bD5①`**（逐字「协议面零改动（`wire.rs` 与 `inbound.rs` 的 diff 为空）」） | 相容 |
| 与本轮**红线**的关系 | 🔴 **撞红线**（派工单逐字「**不许改** `remote-daemon-proto/src/inbound.rs`」） | 不撞 |
| 六根针 | 碰第 4 根（`observe/watcher.rs` 的 `mpsc::channel::<Frame>(`，`crate_want=3`） | **0**（`K-P7` 给的条件：不复用 `listen::Admit`） |
| `russh` 进 daemon crate | **要**（今天 `remote-daemon-proto/Cargo.toml` 零 `russh` 依赖；`src/` 下 `russh` 命中 **1 处**，在 `inbound.rs` 且是注释） | **同样要** —— `--relay` 住 `remote-daemon-proto/src/main.rs` 的分派臂，代理照它的形状就落在同一个 crate 里 |
| `no_timer_guard` 要登记 | **要** | **同样要** |

**最后两行是本格最要紧的读数** —— 它们是 PM「E 更贵 / B 更省」两边都**没算进去**的部分：

- `no_timer_guard` 的人群逐字（`no_timer_guard.rs` 头注）：「**本 crate `src/` 递归全部 `.rs`
  （`SKIPPED_BY_NAME` 跳过自身）剥掉测试段之后的源码文本**」⇒ **E 的代理进程只要住在
  `remote-daemon-proto/src/` 下，就和 B 一样落在这道闸里面。**
- 而拨号**头一句就带定时器**：`ssh_source.rs:712–714` 逐字
  `let keepalive_interval = inactivity_timeout.is_none().then(|| Duration::from_secs(30));`
  —— `Duration::from_secs` 正在 `no_timer_guard` 的禁用形状表里。
- **已有先例证明这条路走得通、也证明它要付钱**：`REGISTERED_DURATION_USES` 里已经有中转那一条
  （`server.rs` · `Duration::from_millis(30_000)` · `relay/server.rs::DOWNSTREAM_DEADLINE`），
  说法栏写着为什么它不是定时器。⇒ **每一处超时都要这样逐条登记并论证**，两条路一样。

### ⇒ 结论（答死那一问）

**B 更省的只有一根轴：Windows 客户端上少一个常驻进程（−1）。PM 那个前提是对的。**

**但它不是「最省的那条」**，因为它省下的那一个进程要用三样东西换：
**动协议面** ＋ **碰第 4 根针** ＋ **动 `inbound.rs`**。而后两样在本件里不是「贵一点」，是**规则冲突**：
`KP6bD5①` 逐字要求协议面 diff 为空，本轮红线逐字禁止改 `inbound.rs`。
⇒ **选 B 不是「改题面 `§1`」，是同时作废 `KP6bD5①` 与一条红线。**

而 `§0` 自己写下的取舍标准逐字是：「**选 E 的理由不是它最好，是它最容易反悔**」。
把场景收窄到「Windows 客户端 + Linux 服务端 + Windows 后端不保活」之后，
**这条理由一个字都没变**（收窄改的是「保活买不买得到」，不是「哪条路好反悔」）。

⇒ **我的报数结论：E 仍然是那一条。B 在进程数上省 1，在协议面 / 针 / 红线上付 3，
且在 `russh` 与 `no_timer_guard` 两项上与 E 付得一样多。**

⚠ **这是给读数的判断，不是替用户选路** —— `§0` 写死「选路归用户，一个字就能推翻」，本文件不改那一条。

### ⚠ 本格**判不了**的（别读成「E 没问题」）

- **E 的代理进程到底落在哪个 crate**：`K-P7` 只说「照 `--relay` 那条分派臂的形状」。
  落在 `remote-daemon-proto/src/` ⇒ 吃 `no_timer_guard`（上表按这一支算的）；
  落在 monitor 侧新起一个二进制 ⇒ 躲开那道闸，但要**新增一份构建产物 + 一条 CI 步 + 一条打包路**。
  **两支的代价我只量了第一支** —— 第二支要 PM 定了落点才量得动。
- **Windows 真机是不是这样**：`§2.5` 禁真远端 / 真 daemon ⇒ 上面三环全是**盘上证据**（CI 配置 + 打包配置 + 消费侧代码），
  **不是真机实测**。K-P6 那句「Windows 上本机后端只有 stdio 监护、没有常驻监听口」说的是**监听口**，
  与本格说的「**有没有那个子进程**」是两件事，两句都成立，别压成一句。

---

## ④ `§3` 变异表 —— 逐刀真跑的读数

四刀里**两刀跑得了、两刀跑不了**。跑不了的两刀（`P6bM1` / `P6bM4`）都以 `D2` 的判据与反向自检为被测对象，
而那两样本轮**不存在**（`D2` 结构性挡住，见 `§8`）⇒ **不是「跑了没红」，是「没有可切的东西」**。

门禁一律走沙箱：`PB_WS=backend-consolidation .claude/devbox/gate <本工作树> k-p6b`。

| 刀 | 锚点 · 命中数 | 判据读数 | 门禁九格 | 最小面？ |
|---|---|---|---|---|
| **入场（无变异）** | —— | `D5①` diff = **0** B | 全绿 `GATE: OK`：cargo 1451 · generated ok · daemon 590 · npm 1590 · e2e 12/8/264/72 · pb check FAIL=0 BROKEN=0 | —— |
| **P6bM2** 给协议加一个字段 | `wire.rs` 的 `Frame::Overflow`，构造/声明/模式共 **5 处**逐处改到编得过；加的是 `origin: Option<String>` | `D5①` diff **0 → 1250 B** ⇒ **红** | 🔴 **九格全绿** `GATE: OK`（daemon 仍 590） | —— |
| **P6bM2 粗细追问** 只改 `wire.rs` 一个注释字 | 同族、**不加字段** | `D5①` diff **834 B** ⇒ **照红** | 九格全绿 | 🔴 **刀太粗** |
| **P6bM3** 改一根针的数 | `PINS` 里 `busy.swap(true` 那条的 `(1,1)`，切前断言**恰好命中 1 次**，改成 `(2,1)` | daemon **588 passed; 2 failed** ⇒ **红**，且**点名那根针** | `GATE: FAIL —— daemon（退出码 101）`，**只有 daemon 一格红**，其余八格全绿 | ✅ **最小面** |
| **P6bM3 粗细追问** 只改那根针的**说法**、不改数 | 同族、**不动数值** | daemon 回到 **590 passed** | 九格全绿 `GATE: OK` | ✅ **刀不粗** |
| **P6bM1** 把拨号退回界面进程 | —— | **跑不了** | —— | —— |
| **P6bM4** 给反向自检喂空输入 | —— | **跑不了** | —— | —— |

### 两条要端给 PM 的结论

1. 🔴 **协议面今天没有任何机检站着。** `P6bM2` 实打：往 `Frame::Overflow` 里加一个
   **序列化可跳过**的 `origin` 字段，跨 daemon 与 monitor 两侧编译通过、**门禁九格全绿**。
   ⇒ `KP6bD5①` 那条「diff 为空」**不是冗余的仪式，它是那一格唯一站着的东西**。
2. 🔴 **但它是最粗的一档。** 粗细追问实打：只改一个注释字，同一条判据**照红**（834 B）。
   ⇒ 它买到的是「**动了那两份文件会被看见**」，**不是**「协议没变过」。
   想要后者，判据得换成**结构面**的（比如钉住 `Frame` 各变体的字段集合），本轮**没做、也没被要求做**。

⚠ 三刀跑完**逐刀还原**，交回时 `git diff b4530d8 -- src-tauri/ remote-daemon-proto/ src/` **为空**（现打）。
