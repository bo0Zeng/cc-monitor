# L1 — POSIX 本地 = 不走 ssh 的远端（**传输层已交付；前端接线登记未做**）

> 主计划：`../MASTERPLAN.md` §1 L1（**P0**）· §0.2 · §2「关键判断」· §3 账本第 1/4 行 · §4 顺序表第 3 位
> 前置：L0 构建那半（`4ecd93c`）· L5 对账表（`5e85959`）

## 1. 开工复测：四条断言，**两条不成立、一条是上一个功能漏的**

| 计划/记档写的 | 实测 | 处置 |
|---|---|---|
| `transport:{kind:"local"}` **无生产生产者** | ✅ **成立**：只有 `launch-requests.vitest.ts` / `launch-render-cli.test.ts` 在产 | 照此做 |
| **E40**：本地 daemon 可能与 tmux 同锅 ⇒ pidfd 探针会被一起端 | ⚠ **前提不成立**（§2） | 订正，并写死复活条件 |
| 账本第 6 行：`#[cfg(windows)]` 分布清单 **是 L0 的产出** | ❌ **L0 漏做了** | 本轮补（§3） |
| **E31**：5 个模块**只为** `shell_quote` 依赖 4847 行的 `ssh_source.rs` | ❌ **中心断言今天不成立**（§4） | 订正 E31，**不做搬家** |

## 2. E40 重测：**同锅是事实，但它警告的失效模式在 L1 不适用**

E40 要求「本地必须重测，不许继承 `zero-poll-liveness` 的结论」。重测了，分两层：

**第一层 —— cgroup 事实（实测，私有 socket）**：

```
当前 shell    /user.slice/…/app.slice/tmux-spawn-<uuid>.scope
tmux server   /user.slice/…/app.slice/tmux-spawn-<uuid>.scope   ← 同一个
同级子进程    /user.slice/…/app.slice/tmux-spawn-<uuid>.scope   ← 同一个
```

⇒ **本地起的 tmux server 不会拿到自己的 scope，它继承调用者的。**
与 SSH 那条路（每次登录得一个新 `session-<N>.scope`）**确实不同** —— E40 的担心在
cgroup 这一层**是成立的事实**，不是臆测。

**第二层 —— 但那个失效模式需要「本地有个 daemon」，而本地没有**：

`lib.rs:429` 写着「本地 jsonl-watcher：**始终** spawn（与 SSH-remote 引入前完全一致）」——
本地判活是 **app 进程内**的 `watcher::spawn_watcher` 直接读 jsonl，**根本不起 daemon**
（这也正是 §40 把 `daemon.deploy` 列为天然不对称的理由）。
探针与被探对象同锅之所以危险，是因为「锅被端时探针死了没人报」；
而本地探针就在 app 里 —— **app 死了本来就没有「报」这回事**。

⇒ **E40 在 L1 不适用。但复活条件写死**：只要有一天本地也起 daemon（或把 daemon 挪进
独立进程），E40 **立刻复活**，而且本轮已经把「同锅」从假设变成了实测事实。

### 2.1 顺带测掉一个方向相反的担心

同锅还引出一个 E40 没覆盖的问题：**app 退出会不会把本地 tmux 会话一起带走？**
实测：父 shell 退出后 tmux server **存活**（重父到 init），会话还在。
tmux 自己 daemonize ⇒ **不成立**。

## 3. 补 L0 漏的：`#[cfg(windows)]` 分布清单

账本第 6 行要的是「每一处要么有 POSIX 对应实现，要么有一条**写下来的**『这台机器上做不到』」。

**实测：52 处平台 cfg，散在 12 个模块**（计划写「8+ 个模块」）。
`cfg(windows)` 27 处 / `cfg(not(windows))`+`cfg(unix)` 23 处。

按**同名配对**筛，windows-only 7 处 —— 但逐个看下来，**6 处是测试函数或平台内部 helper**
（`from_win32` / `to_net_local_ticks` / `default_ssh_agent_pipe` / `ssh_client_available` + 2 个 `#[test]`），
**只有 1 处是真正的平台缺口**，而它就是 L1 要修的那处：

```rust
#[cfg(not(windows))]
pub fn launch_powershell_window(...) -> Result<(), String> {
    Err("拉起终端窗口仅支持 Windows（v1）".into())   // ← 唯一一处真缺口
}
```

**这一条卡死了 Linux 宿主上的全部拉起** —— 连**远端**拉起都走它（`launch_remote_terminal`
构造完 ssh 命令后交给它）。所以「Linux 上把本地当远端」的第一步，不是先做本地，
而是**先给 POSIX 一条真的送法**。

> **方法论**：「同名配对」这个筛子噪音很大（7 个里 6 个是噪音）。
> 它只能用来**缩小人工检视的范围**，不能直接当判据 —— 又一次「判据别落在表面特征上」。

## 4. E31 订正：**搬家断不掉任何一条依赖边**

E31 说「5 个模块（`accounts`/`account_usage`/`cc_bus`/`remote_history`/`tmux`）只为一个与
SSH 无关的纯字符串工具依赖这个 4847 行模块」。实测：

- 用 `shell_quote` 的是 **7 个**模块（多了 `pubkey`、`tool_registry`），`ssh_source.rs` 现 **4960** 行
- **但没有一个是「只为它」**：

| 模块 | 它从 `ssh_source` 拿的**其它**东西 |
|---|---|
| `accounts` | `RemoteConfig` |
| `account_usage` | `connect_and_exec_cmd` |
| `cc_bus` | `connect_and_exec_cmd` · `RemoteConfig` |
| `remote_history` | `connect_and_exec_cmd` · `connect_session` |
| `tmux` | `connect_and_exec_cmd` · `stream_loop` |

⇒ 把 `shell_quote` 搬去 `utils.rs`，**一条模块→`ssh_source` 的边都断不掉**。
E31 承诺的收益不存在。

**所以本轮不做这次搬家**，只订正记录。**但给它留了一个将来会成立的理由**：
等 L1 的本地路径需要引号处理时（今天不需要——本地走 argv，见 §5），
就会出现**第一个与 ssh 无关的调用者**，那时搬家才有真理由。
**不为了「计划里写了」而做一次收益为零的改动。**

## 5. 交付：传输层的本地那条路

### 5.1 判据落在性质上：抽出「与传输无关」的那层校验

`build_remote_ssh_ps_command` 里原有四条校验。抽出前三条成 `validate_launch_cmd`
（空 / 超长 / 控制字符）—— 三条送法一律适用，**一份判据，不留会静默漂移的副本**。

**第四条刻意留在远端**：「拒绝双引号」的理由是 *PowerShell 5.1 向 native 程序传参对内嵌 `"`
有历史畸变*。那是**那条送法的**约束，不是命令本身的性质。
一并搬过去 = 把一个 Windows 怪癖套到 Linux 上，让本地无端拒绝一批合法命令。
有测试专门钉这条（`double_quote_rejection_is_powershell_only`）。

### 5.2 本地送 argv，不送命令串

`build_local_posix_argv(cmd) -> ["bash", "-lic", cmd]`。

- **不拼串**：本地没有「要穿过一层 shell」的问题，拼成串再让别人拆是白造一个注入面。
- **`bash -lic` 保留**：它与远端是同一个语义（PATH / 别名 / 函数按「用户粘贴进交互终端」解析），
  `ccm` 正是靠它才被找到。少的那一跳**只有 ssh**。

### 5.3 `launch_local_posix` 三处设计

| 决定 | 理由 |
|---|---|
| **不开 GUI 终端窗口** | POSIX 上没有「唯一的终端」；会话容器本来就是 tmux（`ccm --tmux` 自己建）。开窗口要先猜用户用哪个终端模拟器 —— 一个会在别人机器上错的决定 |
| **`process_group(0)` + stdio 全 null** | 否则子进程跟着 app 的 Ctrl-C 一起走，还占住 app 的 stdio |
| **起一条线程收尸** | `process_group` 不改父子关系 ⇒ 不 `wait` 就留僵尸。线程随子进程结束（`ccm` 建完会话就返回） |

Windows 宿主上同名函数返回 Err 并指向 L2 —— **平台差异收在这一层**，调用点不必自己写 `cfg`。

### 5.4 新命令 `launch_local_terminal`

`launch_remote_terminal` 的本地对侧：同一个 payload，直接 exec。
与远端那条同样跑在 `spawn_blocking` 里（spawn 是阻塞 OS 调用，不堵 IPC 派发线程）。

## 6. ★ 三道既有门禁替我把关，其中一道是我不知道的

加完命令跑门禁，**接连红了三次**，每次都指得很准：

| # | 谁红的 | 报什么 |
|---|---|---|
| 1 | **L5 自己的 `parity_ledger`**（上一轮刚建） | 「这些命令已注册但**没进平价对账表**：`["launch_local_terminal"]`」——**「有意的摩擦」在野外验证了一次** |
| 2 | **C04a `commands.vitest.ts`**（既有，我**事先不知道**） | 计数 120、且**每条 Rust 命令必须有 TS 包装**：「TS 静态看不见的命令集变了」 |
| 3 | 同上，另一条 | 包装层覆盖数 110 |

**⇒ 一条方法论**：建新守卫前先查**这条性质有没有人已经在守**。
L5 那轮我扫了 `generate_handler!`，却没 grep「谁在断言它」——
`parity_ledger` 的「注册集完整性」这一格与 C04a **重复**了。
（不重复的是能力平价那一格，那才是 L5 的独有价值；两者互补，但我当时不知道有重叠。）

### 6.1 一次自己造成的破坏，已回滚重做

改 C04a 的钉死数时我用了无差别 `replace("120", "121")` —— 把**扫描窗口大小 120**
（`code.slice(m.index, m.index + 120)`、以及解释那个窗口的两行注释）**一起改了**。
那与命令计数毫无关系，改了会悄悄放宽一个扫描器的容差。

`git checkout` 回滚后逐条判断：13 处 `120` 里 **3 处是窗口大小、不能动**。
**这正是我自己记着的「`rep(a,b,n=1)` 带显式处数」纪律，本轮先违反再补上。**

## 7. 变异验收（Phase D）

| 变异 | 结果 |
|---|---|
| **A** 把 PowerShell 的「拒双引号」怪癖也套到本地 | **成立且隔离**：只红 `double_quote_rejection_is_powershell_only`，报「POSIX 本地不经 PowerShell，不该拦」 |
| **B** 本地路径偷偷多加一个修饰（`exec {cmd}`） | **成立且隔离**：只红 `local_and_remote_share_the_same_payload` |
| **C** 本地跳过共享校验 | **成立且隔离**：只红 `local_argv_shares_the_transport_agnostic_validation` |

三条都精确红在**单条**断言上。恢复后先跑「恢复态应全绿」确认基线回来了
（`cp -a` 保 mtime 那个坑，L5 踩过，本轮全程 `touch`）。

**如实标注**：铁律 8 要求并行多 agent 审计；本会话常驻指令「除非用户要求不开 agent」
⇒ 主线程变异 + 全门禁 + 真机实测代替。**这是欠账，不是强度裁剪。**

## 8. 门禁

| 门禁 | 前 | 后 |
|---|---|---|
| monitor `cargo test --all` | 626 | **629**（+3 新断言） |
| monitor clippy lib | 36 | **36**（新函数一度是 +1 dead_code —— **不加 `#[allow]` 糊过去**，而是把 spawn 那半补完给它真调用点） |
| Tauri 命令数 | 120 | **121**（`parity_ledger` 121/50/21、C04a 121、包装层 111 三处同步） |
| npm / tsc / check:types | 866 · 0 · 67 | **同左** |
| daemon / shellcheck / vendored | 173 · 37 rc=0 · 294 | **同左** |

## 9. 签收（部分）

- [x] **E40 重测**：同锅是**实测事实**，但失效模式在 L1 不适用（本地不起 daemon）；**复活条件写死**
- [x] 顺带测掉反向担心：app 退出**不会**带走本地 tmux 会话
- [x] **补上 L0 漏的 `cfg(windows)` 清单**：52 处 / 12 模块，筛出**唯一一处真缺口**
- [x] **订正 E31**：搬家断不掉任何一条边 ⇒ **不做**，并写明将来成立的条件
- [x] 抽出与传输无关的校验；**PowerShell 怪癖没跟着搬**（判据落在性质上）
- [x] `build_local_posix_argv` + `launch_local_posix` + `launch_local_terminal`
- [x] **验收判据机器化**：换 transport 后剥净包装**逐字节相同**（主计划 §2 第 1 条的原话）
- [x] 三条变异全部隔离成立
- [ ] **前端接线**：`transport:{kind:"local"}` 的生产者 + 调 `launch_local_terminal` —— §10

## 10. 没做的（登记）

| # | 事项 | 为什么 |
|---|---|---|
| 1 | **前端接线**（`remote-launch.ts` 侧产生 local transport 并调新命令） | 本轮把 Rust 侧的送法建齐了；前端那半要改的是**用户可见的启动流**，且**验不了**（红线不起 app）。⇒ 单独一步 |
| 2 | **真机跑一次本地拉起** | 要么起 app（红线），要么真起 claude（红线）。命令构造那半已被逐字节钉住；**spawn 那半今天没有测试覆盖，如实记**——它薄（12 行）但确实没验 |
| 3 | E31 `shell_quote` 搬家 | §4：今天收益为零 |
| 4 | `tmux.manage` 仍是 ParityDebt | L1 没有加 tmux 管理的本地命令；那属 L2/后续。**没有顺手把对账表改绿** |
