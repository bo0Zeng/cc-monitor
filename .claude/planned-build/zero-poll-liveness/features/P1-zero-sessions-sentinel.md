# P1 — `ZeroSessions` 观测分类（销掉 `INVARIANTS:408` 那条预登记的真 bug）

> 主计划：`../MASTERPLAN.md` §1 P1 · 前置：P0（本功能的形态由 P0 实测定死）· 后继：P3 会收紧判据
>
> **本功能修的是一个 2026-07-25 就写进 `doc/INVARIANTS.md` 的已知 bug**，
> 当时明文记着「干净修法要动 daemon（红线：daemon 零行为改动），留 daemon 版本批次」。
> 用户 2026-07-30 松了那条红线 ⇒ 解锁。

## 1. DoD

- [x] **先写复现的失败测试再改**（回归纪律）——见 §2 的 red→green 记录
- [x] daemon `run_tmux_ls()` 返回四值枚举而非裸 `String`（账本第 2 行最终形态）
- [x] wire `TmuxSessions` 加 additive `observation` 字段，**`raw` 载荷逐字节不变**、不 bump `PROTO_VERSION`
- [x] monitor 那条内联 if 提成**纯函数**（原来住在需要真远端连接的 `async fn` 里、单测碰不到）
- [x] 新旧混搭两个方向都不回归（旧 daemon×新 monitor / 新 daemon×旧 monitor）
- [x] **销掉** `doc/INVARIANTS.md:408`（不是搬走、不是改措辞）
- [x] 留 P3 的升级路径注释，且说清该升级**不改帧契约**
- [x] 变异验收双向成立（§3）
- [x] 全门禁绿且数字不降；8 套真机 152 条逐个不变

**明确不做**：不改 `TMUX_LS_FMT`（红线）· 不改 `RETIRE_MISS_THRESHOLD >= 2` ·
不 bump `BUILD_ID`（**刻意推到 P5**，理由见 §5）· 不碰前端（本功能零 TS 改动）

## 2. red → green 记录（回归纪律）

类型化语言里"先写失败测试"的正确形态：**先把纯函数落成旧语义**，写上新语义的断言 →
红 → 再改语义 → 绿。这样"旧行为不许回归"的那几条**从头到尾都是绿的**，
真正翻转的只有那一条。

```
（旧语义 + 新断言）
test tmux::tests::zero_sessions_is_a_valid_observation_not_a_skip ... FAILED
  left: Skip          ← 旧代码：空 backend 一律保守跳过
 right: Backend({})   ← 期望：确证零会话是**有效观测（空集）**
其余 22 条全绿（含 old_daemon_empty_raw_still_skips 等四条"不许回归"）

（改语义后）
611 passed; 0 failed   ← monitor lib（基线 603 + 8 条新）
128 passed; 0 failed   ← daemon（基线 125 + 3 条新）
```

## 3. 变异验收（Phase D）

**强度：中风险**（跨两个 crate 的 wire 变更 + 改判活语义）⇒ 主线程变异 + 全门禁。
每次都走判色三步：① `git diff` 确认落位 ② 确认**编译过**（不是编译失败造成的红）
③ 确认红的是本次变异该红的那条。

| 变异 | 做了什么 | 结果 |
|---|---|---|
| **A** | 把 `exec tmux …` 改回 `tmux … \|\| true`（撤销 P1 的核心改动） | **成立**。编译过（`Finished test profile`），`probe_script_propagates_rc_with_fake_tmux` 红，且红在**预期那一格**：rc=2 被 `\|\| true` 折成 `ZeroSessions`（`left: ZeroSessions / right: Unobservable`），报的正是我为它写的那句「观测失败被折成零会话会批量误灰」 |
| **B** | 只改 daemon 侧 `OBS_ZERO_SESSIONS` 的取值（模拟忘了同步 monitor） | **成立**。编译过，`observation_tokens_double_write_point_stays_in_sync` 红，诊断准确指出 daemon 源里缺哪个定义 |
| **C** | （即 §2 的 red）monitor 侧不认 `zero_sessions` | **成立**，见 §2 |

### 为什么 A 必须用「真跑脚本 + 假 tmux」而不是字符串断言

`|| true` 与 `exec` 的差别**在字符串断言里看不出来**——两者都是"一段包含 tmux ls 的
shell"。只有真执行才知道 rc 有没有传出来。所以那条测试建一个临时目录、写一个可执行的
假 `tmux`（按需 `exit 0/1/2`）、把 `PATH` 指过去，真跑 `tmux_probe_script()`。
这是 `tmux.rs::emit_guarded_commands_for_e2e` 头注那条教训的直接应用：
**门禁只锁字符串形状不锁行为是不够的。**

### 这条测试自己抓到的一个问题（如实记录）

初版的第⑤格（"PATH 里没有 tmux"）我写的是 `PATH=/usr/bin:/bin`——**而真 tmux 就在
`/usr/bin`** ⇒ 测试真跑了**默认 socket** 上的 `tmux ls`，列出了用户三个真实会话。
只读、无损，而且**断言当场红**（期望 `NoTmux`、实得 `Sessions(...)`）所以立刻发现。
已改成用一个确实为空的临时目录。

**教训**：写"某工具不存在"的测试时，`PATH` 必须**构造性地**不含它，
不能靠"我以为它不在这几个目录里"。

## 4. 实现要点

### 4.1 daemon：让 rc 透出

原先 `tmux ls -F '…' 2>/dev/null || true` 把 tmux 的 rc **吞掉**，五种观测压成
"空串 / 有内容"两种。改成：

```sh
if command -v tmux >/dev/null 2>&1; then exec tmux ls -F '<FMT>' 2>/dev/null; else exit 97; fi
```

`exec` 让 tmux 的 rc 原样成为 `sh` 的 rc；97 是"PATH 里无 tmux"的约定哨兵（tmux 只用 0/1）。
分类落在纯函数 `classify_tmux_probe(code, stdout)`：

| 观测 | 分类 |
|---|---|
| `Some(0)` + stdout 非空 | `Sessions(raw)` |
| `Some(0)` + stdout 空 | `ZeroSessions`（`exit-empty off` 那格，P0 实测） |
| `Some(1)` | `ZeroSessions`（server 不在；两种 stderr 措辞都走这里） |
| `Some(97)` | `NoTmux` |
| 其他 / `None`（被信号杀） | `Unobservable` |

**判据刻意不看 stderr**——P0 实测 stderr 有两种措辞，且拿英文消息当判据本身就是错的。

### 4.2 wire：additive，且**热路径字节不变**

`TmuxSessions { raw, observation: Option<String> }`，`skip_serializing_if`。映射：

| 分类 | `raw` | `observation` | 旧 monitor 看到什么 |
|---|---|---|---|
| `Sessions` | 原文 | **省略** | 与 P1 之前**逐字节一致** |
| `ZeroSessions` | `""` | `"zero_sessions"` | 空 raw ⇒ 保守跳过 = 今天的行为（无回归） |
| `NoTmux` | `"NO_TMUX\n"` | `"no_tmux"` | 认既有哨兵 = 今天的行为 |
| `Unobservable` | `""` | `"unobservable"` | 同 `ZeroSessions` 行 |

**有会话时省略 `observation`** 是刻意的：raw 非空本身就说明是有会话，省略让热路径
（绝大多数帧）字节不涨。新 monitor 只需要在 raw 为空时看这个字段。

**不 bump `PROTO_VERSION`**（`main.rs:111` 明写那是破坏性变更专用，会把每台旧 daemon 误判）。
**不动 `emits`**：没有新增帧 kind，`tmux_sessions` 早已登记。

### 4.3 monitor：把内联 if 提成纯函数

原来是 `ssh_source::stream_loop` 里的
`if raw.trim() != "NO_TMUX" { … if !backend.is_empty() { … } }`——**把五种语义不同的
观测压成两条路**，其中「daemon 确证零会话」被误并进「观测失败」。且它住在一个需要真
远端连接的 `async fn` 里，单测碰不到。

现在是 `tmux::classify_tmux_observation(raw, observation) -> TmuxObservation`（`Backend(set)` / `Skip`）。
放 `tmux.rs` **不放 `tmux_reconcile.rs`**：后者刻意 source-agnostic（§30「别固化成只有
tmux 能这样」、函数签名连 `tmux_*` 命名都避开），把 `parse_tmux_ls` 依赖塞进去会破坏那条设计。

**未知 `observation` 取值刻意落回 raw 判据** ⇒ 未来 daemon 加新分类时，
老 monitor 退化成今天的保守行为、不误灰（向前兼容）。

### 4.4 一处刻意保留的不对称

`observation` 说有会话、但 raw 里一个 `@ccm_sid` 都没有（老会话 / 未装 wrapper）⇒ 仍跳过。
因为对账判据是 sid 集，**"有 tmux 会话但都没绑 sid" ≠ "零会话"**。有测试钉住。

## 5. 工程审计结果（Phase E）

### 5.1 修完后的延迟：**有界化，不是即时化**（必须说清）

该场景从「永不（卡到断连 flush）」变成 `RETIRE_MISS_THRESHOLD`(2) × 推帧间隔(8s) ≈ **16s**。
降到 ~10ms 是 P3/P5 的事。**不许把 P1 描述成"修成即时"。**

### 5.2 尚未真机生效：`BUILD_ID` 刻意不在 P1 bump

`BUILD_ID` 仍是 `p1q-accounts` ⇒ 已部署的旧 daemon 不会被判 `StaleBuild` ⇒ 不会自动重装
⇒ **本修复在远端休眠**。

**为什么推到 P5**：① P5 要加新帧 kind，届时一次 bump + 一次重部署覆盖整个工作区，
避免让用户的每台远端主机被强制重装两次 ② additive 设计保证旧 daemon 期间行为无回归
（保守跳过 = 今天的行为），所以休眠是安全的、不是"半成品上线"。
**已记进 `STATUS.md` 阻塞/待办**，不许忘。

顺带核实：`release.yml` 每次发版都用 `cargo-zigbuild` **现场交叉编译**两个 musl daemon，
仓里 `embedded-daemons/` 只是本地兜底 ⇒ bump 不需要我在本机交叉编译。
另：build script 那条「内嵌 daemon 比源码旧」警告**本来就在响**（daemon 源码最后改于
2026-07-25 `027ae89`，内嵌二进制 2026-07-08），**不是本轮引入的**。

### 5.3 账本对账

| 账本行 | 本功能做了什么 | 是否朝最终形态 |
|---|---|---|
| 2 `run_tmux_ls()` 返回契约 | **建立四值枚举**（P0 定死的最终形态） | ✅ 到位。P3 只加调用时机与内部判据，不改契约 |
| 3 wire + EMITS | 加 additive 字段；不动 EMITS（无新 kind） | ✅ additive-only，不 bump `PROTO_VERSION` |
| 4 `ssh_source` 收帧臂 | 内联 if → 纯函数；仍只往 `remote_tx` 送 `SessionChange{removed}` | ✅ **§24 单写者不破**，零新写点 |
| 5 INVARIANTS §24bis | **销掉**残留项 + 写清修法与遗留 | ✅ |
| 1 `watch_loop` 最终形态 | **未触及**（P1 不改循环结构） | 留给 P2 建统一 channel |

### 5.4 对后续的影响（两条，都改了后面的计划）

**① P6 的载体不能用 graylight 一族——它们不做 socket 隔离。**
本轮审计了全部 e2e 套件的 socket 隔离情况：

| 套件 | 真调 tmux？ | 带 `-L` 隔离？ |
|---|---|---|
| `ccm-acceptance` · `ccm-pretrust` · `cc-spawn-uplift` · `tmux-guarded` · `tmux-target` · `usage-probe` | 是 | **是** ✅ |
| `ccm-cli.test.sh` · `ccm-print-parity.sh` | **不调**（"tmux" 只出现在注释与 `--print` 断言里；`ccm-cli.test.sh:4` 明写「不真起 agent、不碰 tmux」） | n/a ✅ |
| `graylight-suite` · `graylight-daemon-frames` · `restart-suite` · `restart-daemon-frames` · `resume-suite` · `resume-daemon-frames` · `gen-idle-tmux.sh`（helper） | **是** | **否** ❌ |

⇒ 那 6 套（正是 BACKLOG **E14** 记的"不在 CI 的 6 套"）会在**默认 socket** 上建/杀会话。
在 CI 的干净容器里无害，但在开发机上会动开发者自己的 tmux server。
**本轮据此跳过它们**（如实标注，不假装跑过）。
**主计划 §1 P6 原写「并入既有 graylight 一族」⇒ 必须先给那 6 套做 socket 隔离，
或改用别的载体。** 已登记 BACKLOG E41 + 改主计划 P6。

**一个漂亮的旁证**：`graylight-daemon-frames.sh:30` 有一行
`KEEP="cc-e2ekeep-$$"   # 无关 cc-* 会话:kill 掉 fixture 后 backend 仍非空(§24bis 空 backend 守卫)`
——**这个 keepalive 会话的存在理由就是绕开 P1 修掉的那个 bug**。P1 之后它不再必需
（留着也无害）。这说明该 bug 当时是被"测试侧绕过"而不是被发现的。

**② P3 有一处明确的收紧对象。** `rc=1` 直接判 `ZeroSessions` 是刻意的保守
（socket 权限异常这类罕见情形理论上可能误 retire）。P3 持有 server 的 pidfd 之后，
「pidfd 说 server 活着但 `tmux ls` rc=1」= 真异常 ⇒ 归 `Unobservable`。
**该升级不改帧契约**，已在两侧代码注释里留了指引。

### 5.5 一处基线订正

我此前记的「daemon cargo test 47」是**错的**——那是 `watcher.rs` 单文件里 `#[test]` 的
数量。daemon crate 实际总数是 **125**（本轮 +3 = 128）。已更正。

## 6. 签收

- [x] 通过代码审计（中风险档：三条变异双向成立 + 全门禁 + 8 套真机 152 条逐个不变）
- [x] 通过工程审计（账本 4 行朝最终形态；两条对后续计划的实质影响已回写）
- [x] 主计划已据此更新（P6 载体改写 + §7 变更记录 04）
