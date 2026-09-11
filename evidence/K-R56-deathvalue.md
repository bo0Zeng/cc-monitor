# K-R56 死值验 · 变异表（`send-keys` 那条：先让两条路等价）

**量于**：分支 `track/k-r56`，工作树 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r56`，
基线 `59fc7784e3d8558f1c321a795610ba82fb244e71`（= 09-11 主干尖）。
**量法**：全部在**沙箱**里跑（`K31`：开发测试不许直接在本机跑）。镜像 `ccmon-devbox:latest`，
挂载与 `.claude/devbox/gate` 逐条相同（只把最后那条命令从 `scripts/gate.sh` 换成一条定向
`cargo test`），`CARGO_TARGET_DIR=.claude/pm-targets/k-r56`，`--network none`。
量具住址：`/tmp/claude-1000/-home-zbl----claudecode-frontend/f80a7aea-0ccf-4854-9b7a-12961a9c55aa/scratchpad/kr56/`
（`sbtest.sh` 跑沙箱 · `cut.py` 切刀 · `mutate.sh` 驱动）——
⚠ **那是会话级临时目录，下一轮读不到**；表里每一行的读数与锚点都逐字抄在下面，复刀不需要那几个文件。

**本轮口径禁真 tmux**（`K-R56#§0d`）⇒ 下面所有行为读数都来自
`tmux::tests::run_door_with_info`：它把 **builder 吐出来的生产命令串**交给真 `/bin/sh`，
`tmux` 由一个 **shell 函数**顶掉（`command -v` 认函数）⇒ **零真 tmux、零 socket、零临时文件、
不动这台机器任何状态**。⇒ 本表买到的是「探了 / 没探」「动作跑没跑」，
**买不到**「探回来的答案对不对」。

---

## 一 · 跑的那条命令（分母）

```
cargo test --manifest-path src-tauri/Cargo.toml -p monitor --lib -- \
  tmux:: local_origin_registry:: tmux_daemon_gate_guard::
```

**这个过滤面上的分母**：改前 45 条、改后 47 条（+2 = 本件新增的两条判据），
另 `1 ignored`（`tmux::tests::emit_guarded_commands_for_e2e`，`#[ignore]` 的 e2e 输入源）。
全量门禁的读数在件文件 `§8`，不在这里。

---

## 二 · 先红后绿（判据先在没修的代码上跑过）

| 趟 | 盘上是什么 | 读数 | 红的是哪几条 |
|---|---|---|---|
| **红①** | 判据已写、**生产一个字没改** | `43 passed; 3 failed; 1 ignored` | `the_local_send_keys_never_falls_back_to_ssh` · `the_ssh_fallback_always_probes_before_it_acts` · `gate2_non_prefixed_safe_name_builds_remote_check_not_instant_reject` |
| **中②** | 生产两刀都落了，判据还没跟 | `44 passed; 2 failed; 1 ignored` | `gate3_only_applies_to_kill_not_send_keys`（**代理失效**，见 §四） · `every_remote_config_lookup_deals_with_the_local_origin_first`（**存量表要跟着改**，见 §五） |
| **绿③** | 全落 | `46 passed; 0 failed; 1 ignored` | —— |

**红①里两条新判据的判定行逐字**（这两句就是「它真的走了生产那条路」的证据）：

```
thread 'tmux::tests::the_local_send_keys_never_falls_back_to_ssh' panicked at src/tmux.rs:
本机 send-keys 掉进了 SSH 回落 —— 它报的是「未找到远端配置」，
而真实原因是本机 daemon 通道不在。**错的诊断比没有诊断更贵。**
实得：未找到远端配置: "<local>"
```

```
thread 'tmux::tests::the_ssh_fallback_always_probes_before_it_acts' panicked at src/tmux.rs:
回落那条路上「恒先探会话」这条性质有 3 格不成立：
  send-keys/名字命中：命令串里**一次探测都没有**（没有 `display-message`）—— 它会对一个可能不存在的会话直接动手，而 daemon 的 `admit` 恒先 probe。
实得：if command -v tmux >/dev/null 2>&1; then tmux send-keys -t '=cc-e2e-owned:' 'CCMPROBE' Enter 2>&1; else printf 'NO_TMUX\n'; fi
  send-keys/名字命中：探不到时没报 `CCM_NO_SESSION`。实得 "ACTION_RAN:send-keys"
  send-keys/名字命中：🔴 **探不到却把动作跑了** —— 这正是本件要关掉的那条退化分支。实得 "ACTION_RAN:send-keys"
```

⚠ 第二条那个「**3 格**」的分母是 **4 种守护形态 × 3 项断言**：
`send-keys/名字命中` 一格全红（3 项），另外三格（`send-keys/名字未命中` · `kill/名字命中` ·
`kill/名字未命中`）**全绿** ⇒ 这是一刀**单断**，不是目录级塌陷。

---

## 三 · 变异表（每一刀：先断锚点命中数，再切，再跑）

一趟只切一刀，切完立刻还原（叠刀的读数不作数 —— 唯一刻意叠刀的是 §六 的 `7u`）。
每一行的「锚点命中」= `cut.py` 在切之前断言的那个数，命中数不对就**不切**、读数不作数。

| 刀 | 切在哪个函数 / 哪一处 | 锚点命中 | 打的是哪条性质 | 读数 | 红的逐条 |
|---|---|---|---|---|---|
| **M1** | `tmux.rs::tmux_send_keys` 的 `Routed::NoChannel` 臂 —— **整块删掉** `<local>` 早退 | **1** | `KR56D1` | `44 passed; 2 failed; 1 ignored` | `tmux::the_local_send_keys_never_falls_back_to_ssh` · `local_origin_registry::every_remote_config_lookup_deals_with_the_local_origin_first` |
| **M2** | `tmux.rs::build_guarded_tmux_cmd` 的退化分支 —— **整块换回** K-R56 之前那条「零探测一行」 | **1** | `KR56D2` | `44 passed; 2 failed; 1 ignored` | `tmux::the_ssh_fallback_always_probes_before_it_acts` · `tmux::gate2_non_prefixed_safe_name_builds_remote_check_not_instant_reject` |
| **M2b** | 同上，但**只打该盖的最小面**：探测**留着**，只把 `if [ -z "$info" ]; then …CCM_NO_SESSION…; else {action}; fi` 换成裸 `{action}` | **1** | `KR56D2` 的「探不到就不动手」那一半 | `45 passed; 1 failed; 1 ignored` | **只有** `tmux::the_ssh_fallback_always_probes_before_it_acts` |
| **M2c** | 同一处，**反向格**：探测留着，但把那一整句换成恒 `printf 'CCM_NO_SESSION'`（动作永远不跑） | **1** | 验判据③「探得到就照样动手」不是仪式 | `44 passed; 2 failed; 1 ignored` | `tmux::the_ssh_fallback_always_probes_before_it_acts` · `tmux::send_keys_cmd_construction` |
| **M3** | `tmux.rs::tmux_send_keys` 的 SSH 回落尾段（`connect_and_exec_cmd` 那一段）—— 删掉**回落那份实现** | **1** | `KR56D3` | `44 passed; 2 failed; 1 ignored` | `tmux_daemon_gate_guard::send_keys_now_routes_through_the_daemon` · `tmux_daemon_gate_guard::the_two_command_bodies_are_actually_extracted` |

**CRASH：0**。五刀都是正常的 `test result: FAILED` + 退出码 101，判定行数逐趟都在
（`46 = passed + failed + ignored` 恒成立），没有一趟是编译不过或运行期炸。

★ **M2b 是本表最值钱的一刀**：它证明 `KR56D2` 那条判据的牙**长在「不动手」那一步上**，
不是长在「串里有没有 `display-message` 这几个字」上 —— 后者用 M2 那种粗刀验不出来
（粗刀会把两半一起打红，读起来像有牙）。

★ **M2c 的第二条红是真的，不是误伤**：动作永远不跑 ⇒ 生成的串里没有
`tmux send-keys -t …` 那一句 ⇒ `send_keys_cmd_construction` 的 `contains` 断言落空。

---

## 四 · 中途逮到的：一条**代理判据**当场失效（本轮自查抓的）

`tmux::tests::gate3_only_applies_to_kill_not_send_keys` 原版逐字是：

```rust
assert!(!sk.contains("windows"), "send-keys 不删东西，不该有 Gate 3: {sk}");
```

它拿「串里有没有 `windows` 这个词」**代理**「有没有 Gate 3 那道门」。
K-R56 给 send-keys 补上存在性探测之后，它的格式串里出现了
`PROBE_ONLY_FMT`（`#{session_windows}`）⇒ **本条当场红，而 Gate 3 一个字节都没进来**。

⇒ 收紧成断言 **Gate 3 那条守卫表达式本身**，而且那条表达式是从生产函数 `gate_guard_expr`
**现取**的，不是在判据里手抄一份（手抄就成了第二份判定，翻掉生产那一行会一个字不响）：

```rust
let gate3 = gate_guard_expr(false, true);       // 现取，不手抄
assert!(kill.contains(gate3), …);               // 正向那格
assert!(!sk.contains(gate3), …);                // 反向那格
assert!(!gate3.is_empty() && gate3.contains("$w"), …);  // 防它退化成空串（空串两格同时失效）
```

⚠ 这一条**不是本件买的性质** —— 它是 K-R56 那一刀**暴露出来**的一条既有代理失效。
`7u` 那一趟它仍绿，理由就是这个（见 §六）。

---

## 五 · 中途逮到的：写区外一处**被机器逼着要改**的连带

`src-tauri/src/local_origin_registry.rs` 的 `TRIAGE_DEBT` 里逐字登记着
`("tmux.rs", "tmux_send_keys")` —— 那正是本件要还的这一笔。
`<local>` 早退一落地，那条守卫当场红，判定行逐字：

```
thread 'local_origin_registry::tests::every_remote_config_lookup_deals_with_the_local_origin_first'
panicked at src/local_origin_registry.rs:243:
登记表里这些今天已经先分本机了：["tmux.rs::tmux_send_keys"] —— 把它们从表里删掉，并把计数改小。
```

⇒ 那个文件**不在本件写区**，但它**自己的断言逐字在指挥这一步**，而且那张表是
「等号不是地板」（`assert_eq!(TRIAGE_DEBT.len(), TRIAGE_DEBT_TODAY)`）⇒ **不跟就红**。
处置与越界申报写在件文件 `§8`。改动共两处：删表里那一行 + `16 → 15`（另加两处来历注释）。

---

## 六 · `7u`：把实现整个退掉，还有多少条新断言仍绿

**刻意叠刀**（M1 + M2 一起落，两个锚点各命中 1 次）⇒ 生产两刀全退，判据全留：

```
test result: FAILED. 42 passed; 4 failed; 1 ignored; 0 measured; 1343 filtered out
    local_origin_registry::tests::every_remote_config_lookup_deals_with_the_local_origin_first
    tmux::tests::gate2_non_prefixed_safe_name_builds_remote_check_not_instant_reject
    tmux::tests::the_local_send_keys_never_falls_back_to_ssh
    tmux::tests::the_ssh_fallback_always_probes_before_it_acts
```

**本件动过的判据共 4 条，退掉实现之后 3 条红、1 条仍绿：**

| 判据 | 退实现之后 | 仍绿的理由（逐条给，不写「仪式」也不写「有牙」） |
|---|---|---|
| `the_local_send_keys_never_falls_back_to_ssh`（新） | **红** | —— |
| `the_ssh_fallback_always_probes_before_it_acts`（新） | **红** | —— |
| `gate2_non_prefixed_safe_name…`（翻面那一句） | **红** | —— |
| `gate3_only_applies_to_kill_not_send_keys`（收紧） | **仍绿** | 它钉的是「Gate 3 不许混进 send-keys」，与 K-R56 这一刀**正交**。它本轮变的是**尺子**（代理 → 现取守卫表达式），不是性质。它自己的牙由 M2c 之外的路验：把 `gate_guard_expr` 的 `(false,true)` 臂改成空串 ⇒ 新加的第三句反向自检红。 |

另：`KR56D3` 的落点是**盘上已有的**判据
`tmux_daemon_gate_guard::send_keys_now_routes_through_the_daemon`，它在 `7u` 里**仍绿**，
**而那是对的**：`7u` 退的是本件加的东西，一份实现都没删 ⇒ 它本来就该绿。
它的牙由 **M3** 验（删掉回落那份 ⇒ 当场红）。

---

## 六之二 · 🔴 自查回打：**病复发在治它的代码里**（全量门禁逮的，定向 cargo 没逮到）

这一节是**墓碑**，刻意放在 `evidence/` —— 那条机检的语料面是
`src-tauri/src` · `src-tauri/crates` · `remote-daemon-proto/src` · `src` · `doc` · `e2e` ＋
`src-tauri/build.rs`（现打 598 份），`evidence/` **不在里面**。
抄回源码注释里就又是一处「散文点名一个不存在的名字」。

**第一趟全量门禁 `GATE: FAIL —— cargo（退出码 101）`**，红 1 条
（`structural_scan` 那条「散文点名的名字必须在代码里」的机检），判定行逐字：

```
src-tauri/src/tmux.rs  `the_local_origin_literal_matches_the_frontend`  盘上 1 处，登记表写 0 处
```

⇒ **`the_local_origin_literal_matches_the_frontend` 这个判据名是我编的，盘上零处。**
我在写 `<local>` 射程那段注释时，凭对 `inbound_client.rs` 那段代码的印象写了个名字，
**没有回去核**。真名是 `the_local_origin_is_the_same_string_on_both_sides`。

**第二趟仍 `GATE: FAIL`，而且变成 2 条** —— 我按「出路①」改对了名字，
却在旁边写了一句自证、**把那个假名字逐字引了回来**，又顺手点了那条机检自己的名字：

```
src-tauri/src/tmux.rs  `every_dead_name_named_in_the_prose_is_declared_dead`  盘上 1 处，登记表写 0 处
src-tauri/src/tmux.rs  `the_local_origin_literal_matches_the_frontend`        盘上 1 处，登记表写 0 处
```

⚠ 第二个名字**确实存在**（`structural_scan.rs` 里那条判据本身），
但那条机检的语料**按构造摘掉了自己所在的文件** ⇒ 它的名字对别处的散文而言就是「零定义」。
⇒ 第三趟按 `brief` 第 15 条处置（「写在**源码注释**里的自证等于埋掉了 ⇒ 交回时抬进上报口」）：
源码注释里只留**不点名**的那句话 ＋ 指向本节，两个名字都搬到这里。

★ **这条值得单独记两点：**
1. **定向 cargo 那 46 格一个都没响** —— 我自己挑的那个过滤面
   （`tmux:: local_origin_registry:: tmux_daemon_gate_guard::`）**结构上盖不到**
   `structural_scan::`。⇒「定向跑绿了」这句话的射程就只有那个过滤面，
   **不许读成「改动没问题」**。全量门禁买到的正是这一格。
2. **它是同一族病的第三次**：本件治的是「散文说一件盘上已经不成立的事」
   （`build_guarded_tmux_cmd` 头注那句「零额外 round trip」· 账本 ③ 那半句 ·
   `send_keys_cmd_construction` 那句「逐字节相同」），而我在讲这件事的注释里犯了它。

---

## 七 · 判不了的（写在这里，不许读成绿）

1. **「探了之后拿到的答案对不对」判不了** —— 那要真 tmux，本轮红线禁（`K-R56#§0d`）。
   本表只买「探了 / 没探」「动作跑没跑」。
2. **真机那一跳没跑**：`e2e/tmux-guarded-acceptance.sh` 场景 6（`send_keys_owned` 真送达）
   要一个隔离 `-L` socket 的真 tmux ⇒ 本轮没跑。按代码推它仍会 HIT（会话存在 ⇒ 回包非空
   ⇒ 动作照跑），**但那是推的，不是量的**。⚠ 它那一行的说明文字
   「零额外 round trip 的退化形态仍工作」今天已经**馊了**，而 `e2e/` 不在本件写区。
3. **`NoChannel` 这一档的真实触发频率仍然没有运行期计数** —— 本件现打复核（见件文件 `§8`），
   ⇒ 本件**不解锁**「能不能删回落」，只把前置铺好。
4. **`<local>` 那道保护的射程**：它比的是 `origin`、不是 target 会话名，
   拦得住 / 拦不住的形状逐条写在
   `tmux::tests::the_local_send_keys_never_falls_back_to_ssh` 的头注里，别读成「加了保护就安全了」。
