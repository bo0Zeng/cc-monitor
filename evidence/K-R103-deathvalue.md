# K-R103 死值验原文

三条 dod ＋ `7u` 的刀与完整原文读数住这里；件文件 `§3` 只放摘要。
**全部在沙箱里跑**：`PB_WS=backend-consolidation .claude/devbox/gate <本树绝对路径> k-r103`。
**一趟一刀** · 逐刀 `git status --porcelain` 自证 · 逐刀记**锚点命中数 ＋ 落地后那份文件的 md5 前 10 位**。

---

## 〇 量具

### ① 沙箱门禁（唯一许可的跑法）

```
PB_WS=backend-consolidation \
  /home/zbl/文档/claudecode-frontend/.claude/devbox/gate \
  /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r103 k-r103
```

⚠ **等门禁跑完的判据是「日志里出现 `^GATE` 那一行」，不是 `pgrep`。**
本轮实测踩到两次：① `pgrep -f '[d]evbox/gate'` 写在一条**同时含有那条真路径**的命令里时会
**匹配到自己那个 shell**（假阳）；② 反过来也漏过一次（假阴，提前退出去读了一份没跑完的日志）。
**两次都用 `docker ps` 与日志行数复核过，没有出现两趟门禁同时压同一棵树。**

### ② 普查用的 Python 复刻剥法（只读，不进判据）

住 `/tmp/claude-1000/-home-zbl----claudecode-frontend/f80a7aea-.../scratchpad/kr103/`：
`prod.py`（复刻 `guard_core::production_code`）· `census1c.py` / `census1e.py`（尺子①）·
`census2.py`（尺子②）· `census3.py`（尺子③）。被测对象一律指
`.claude/worktrees/k-r103/remote-daemon-proto/src`。
🔴 **那是本会话专属的临时目录，下一轮取不到** —— 谁要复跑，照下面写的口径重写一份，别照住址找。
⚠ 它与 Rust 侧不是同一份实现 ⇒ **个位数出入是预期的**（本轮实测：210 vs 212）。

---

## 一 `M0` 基线（现打，量于 `674c2e8` ＋ 一份空的本文件）

```
GATE: OK —— 13 格全绿
hooks 11 · fmt 1 · fmt-daemon 1 · winchk 1 · cargo 1599（9 个包合计）· generated ok
daemon 745 · npm 1722 · ccm e2e 12 / 8 / 46 / 45 · pb check FAIL=0 BROKEN=0
分母 cargo：本树未铺 src-tauri/embedded-daemons/ ⇒ 少了「本地后端真的能起来吗」那一族 4 条
```

**实现之后**：`daemon 745 → 755`（+10 条新断言），其余各格不变。

---

## 二 两把尺子普查（PM 给的名单之外，自己量的）

### 尺子① `no_timer_guard::f09` 的人群，三种口径同一趟量

| 口径 | 人群 | 判红 | 假红 |
|---|---|---|---|
| 上一版（同一行里有引号 **且** 有三根针之一） | **176 行** | 0 | — |
| 放宽成「任何字符串字面量」 | **1704 个串** | 5 | **4** |
| 本轮（针不动，匹配单位改成**表达式**） | **224 条表达式 / 210 个串**（Python）· **212 个串**（Rust 沙箱现打） | **1** | **0** |

放宽那一档 5 处逐处：
- `control/ccm/plan.rs` —— **真**（`C14` 那条预信任等信任框）
- `observe/watcher.rs` ×2 —— 假（日志文案 `watch failed for {}`）
- `platform/pidwatch/mod.rs` —— 假（文案里的 `watch_pid_until_exit`）
- `plugin/mod.rs` —— 假，而且**它自己就是一条存量真缺陷**（见第五节）

### 尺子② `cfgless_guard` 的 `posix-shell-*` 针

| 口径 | 命中 | 假红 |
|---|---|---|
| 上一版 `Command::new("sh")` / `Command::new("bash")` | **2**（`control/ccm/mod.rs` 已签字 · `platform/shell.rs` 在门后） | 0 |
| 本轮「整条字面量恰好是 `sh` / `bash`」 | **3**（多出 `control/oneshot_session.rs` 的 `const POSIX_SHELL`） | 0 |
| 「凡出现 `sh` 就红」 | 见 `M6` —— 一份文件里就 **29 处**假红，还打断了 `CLOSED_FOR_GOOD` 那条棘轮 |

同一趟补量到的：
- `"setsid"` 整条字面量 **1** 处 ⇒ 本轮**新加一根针 + 一行签字**（那条 POSIX-only 路是
  `setsid` ＋ `sh` ＋ `sleep` 三件，上一版**只有 `sh` 那一件有针**）
- `"bash"` / `"zsh"` / `"cmd"` / `"powershell"` / `"pwsh"` 各 **0** 处
- **壳名走变量 2 处**（`Command::new(launcher)` · `Command::new(prog)`）—— **任何针都够不着，无解**
- `"tmux"` **10** 处 —— **刻意不加针**：`C12`〔用 08-11〕已把整个命令面裁成 POSIX-only

### 尺子③ 那四份 `emit_err`

| 口径 | 数 |
|---|---|
| 按名字（`fn emit_err`） | **4** ← PM 给的那个数 |
| 按行为（生产段往 stderr 打 `{code,message}`） | **12 处 / 7 份文件** |
| 其中有具名函数包着的 | **5**（4 份 `emit_err` ＋ `control/fork_write.rs` 的 `fail`） |
| 规范化函数体后**逐字同形**的 | **2**（`capture_pane` / `oneshot_session`） |

12 处逐处：`control/capture_pane.rs` · `control/cli_control.rs` · `control/fork_write.rs` ·
`control/oneshot_session.rs` · `control/resolve_query.rs`（转义那一形）·
`observe/accounts_query.rs` ×4 · `observe/history_query.rs` ×3。

git 历史（`git log -L :emit_err:<文件>`，量于 `674c2e8`，逐份都确认 `-L` 真解析到了函数）：
四份**各只有 1 个 commit**，全是引入它的那一个 ⇒ **零不同步事故**。
`0279299`（cli_control）· `a82878a`（capture_pane）· `7b7651c`（resolve_query）· `384a561`（oneshot_session）。
`control/fork_write.rs::fail` 同样 **1** 个。

---

## 三 逐刀原文

### `M0b` 地板够得着吗（`KR103D1`）

- 变异：`no_timer_guard.rs` 两条地板 `>= 120` / `>= 25` → `>= 9_999`（锚点各命中 1 次）
- 实得：`754 passed; 1 failed` —— 红 `the_shell_string_scan_finds_candidates`：
  逐字「**只抽到 212 个可能的 shell 串** —— 抽取器坏了，下面那几条会空转变绿」
- ⚠ **第二条地板（份数）没跑到** —— 第一条先红 ⇒ 那个数只有 Python 复刻量的 40，如实登记
- 同一趟顺带确认：`fmt-daemon` 已由绿转绿（上一趟它红过一次，是我写的一行超宽）

### `M1` 改人群不改头注（`KR103D1`）

- 变异：`const NEEDLES` 多一根 `"zzz-no-such-needle"`（1 次，md5 `984cb302c7`）—— **人群不变**
- 实得：`754 passed; 1 failed`，**只红一条** `the_head_note_lists_exactly_the_needles_in_use`
  `left: ["format!", "run-shell", "sh -c"]` / `right: ["format!", "run-shell", "sh -c", "zzz-no-such-needle"]`
- 还原后 md5 回到 `d2b831f80b`

### `M2` 改头注不改人群（`KR103D1`，反方向）

- 变异：头注 `针（逐字）：` 那一行删掉 `· \`format!\``（1 次，md5 `b0f4e9ef5e`）
- 实得：`754 passed; 1 failed`，**同一条**：`left: ["run-shell", "sh -c"]` / `right: ["format!", "run-shell", "sh -c"]`
- 还原后 md5 回到 `d2b831f80b`

### `M3` 匹配单位退回「行」（`KR103D1`）

- 变异：`expr_end` 的 `b'\n' if !opened => return i,` → `b'\n' => return i,`（1 次，md5 `0cbd74c750`）
- 实得：`752 passed; 3 failed`，逐条点名：
  - `the_matching_unit_is_an_expression_not_a_line` —— 「针在上一行、串在续行 ⇒ 收不进 —— 匹配单位又退回「行」了」
  - `the_registered_beat_is_actually_inside_the_population` —— 「`control/ccm/plan.rs` 上那条 `C14`
    登记的外部节拍（锚点 `Yes, I trust this folder`）**不在人群里**」
  - `every_external_beat_the_backend_produces_is_registered` 的 **stale** 那格
- ★ **这一刀就是上一版那个洞的死值验**：人群够不着 ⇒ 判据零命中地绿

### `M4` 没签字的外部节拍（`KR103D1`）

- 变异：`control/oneshot_session.rs` 生产段加一条合成违规
  `format!("{}", " && (for _i in 1 2; do true; done)")`（1 次，md5 `21e51bd75b`）
- 实得：`754 passed; 1 failed`，**只红** `every_external_beat_the_backend_produces_is_registered`
  的 **unsigned** 那格：「后端产出的 shell 串里出现了**没签字**的循环关键字」
- 还原后合成函数 `grep -c` = 0；那份文件 diff 只剩 18+/7-（头注那一段）

### `M5` argv 形不响 ⇒ 红（`KR103D2`）

- 变异：`posix-shell-sh` 的 needle `"sh"` → `Command::new("sh")`（1 次，md5 `7892bc39d1`）
- 实得：`753 passed; 2 failed`：
  - `an_argv_shaped_shell_start_is_caught_and_a_mere_mention_of_sh_is_not` ——「argv 形起 shell 没被逮到」
  - `platform_assumptions_outside_a_gate_are_each_signed_for` —— 本轮那行新签字变成**过期条目**
- ⚠ 第一版的还原脚本**拒跑**（锚点命中 2 次：`REGISTERED` 里 `control/ccm/mod.rs` 那行的锚点逐字相同）
  ⇒ 换成带上下文的多行锚点。**fail-closed 起作用了，没有误改。**

### `M6` 别把针扩成「凡出现 `sh` 就红」（`KR103D2` 的红线）

- 变异：同一根 needle `"sh"` → 裸 `sh`（1 次，md5 `4374a88994`）
- 实得：`752 passed; 3 failed`：
  - `an_argv_shaped_shell_start_is_caught_and_a_mere_mention_of_sh_is_not` ——
    「把渲在长串里的 `sh` 也判红了 —— 那就是「凡出现 sh 就红」，净变宽」
  - `platform_assumptions_outside_a_gate_are_each_signed_for`
  - `the_files_that_were_moved_into_the_adapter_layer_stay_clean` —— **`CLOSED_FOR_GOOD` 那条棘轮断了**
- ★ 假红长这样（`observe/watcher.rs` 一份文件里 **29 处**，逐字节选）：
  `use std::collections::{HashMap, HashSet};` · `snapshot.publish(...)` · `pub fn shutdown(&self)` ·
  `out.push(ReadLine {` · `active_sids: HashSet::new(),`
  —— 全是 `HashSet` / `push` / `snapshot` / `shutdown` 里的那两个字母。
  ⇒ **这条红线不是风格偏好，是一刀实打出来的。**

### `M7` 信封没签字 ⇒ 红（`KR103D3`）

- 变异：`readonly_guard.rs` 的 `SIGNED` 删掉 `"bad_parent"` 那一行（1 次，md5 `922677c3d3`）
- 实得：`754 passed; 1 failed`，**只红** `every_error_envelope_site_is_signed_for`（unsigned 那格）
- 还原后 `grep -c bad_parent` = 1，md5 `80b8e0ad56`

### `M8` 键集分叉 ⇒ 红（`KR103D3`）

- 变异：`control/capture_pane.rs` 的 `"message"` → `"msg"`（1 次，md5 `5fbe0675ac`）
- 实得：`754 passed; 1 failed`，**只红** `every_envelope_carries_both_keys`：
  「这几处信封只有 `code` 没有 `message` —— 键集分叉了」
- 还原后 `grep -c '"msg": message'` = 0

### `M9` 过期条目 ⇒ 红（`KR103D3`）

- 变异：`SIGNED` 加一条指向 `control/kr103-no-such-file.rs` 的假行（1 次，md5 `29639ccb08`）
- 实得：`754 passed; 1 failed`，**只红** `every_error_envelope_site_is_signed_for` 的 **stale** 那格：
  「`SIGNED` 里这几条今天一处都匹配不上」
- ⇒ `M7` 与 `M9` 合起来证明那一条**两个方向都有牙**

---

## 四 `7u`：把实现整个退掉，还有多少条新断言仍绿

**退掉的三处（逐处点名，不是 `git checkout`）**：
① `expr_end` 的匹配单位退回「行」· ② `posix-shell-sh` / `posix-shell-bash` 两根针退回
`Command::new(...)` · ③ 删掉本轮新加的 `posix-setsid` 信号。
**保留**：全部新判据、两张登记表、两处头注。
⚠ `KR103D3` **没有行为实现可退** —— 它的交付本身就是判据 ＋ 登记表，这一栏如实写明。

实得：`750 passed; 5 failed`。**红的 5 条**：
`every_external_beat_the_backend_produces_is_registered` · `the_matching_unit_is_an_expression_not_a_line` ·
`the_registered_beat_is_actually_inside_the_population` · `an_argv_shaped_shell_start_is_caught_and_a_mere_mention_of_sh_is_not` ·
`platform_assumptions_outside_a_gate_are_each_signed_for`。

**仍绿的新断言（6 条），逐条给理由**：

| 仍绿的 | 为什么它该绿 | 它的牙在哪一刀上 |
|---|---|---|
| `the_lexer_tells_char_literals_from_lifetimes` | 它钉的是**词法**（`'"'` 不许让扫描器失步 · `'a` 不许被当成字符字面量），`7u` 退的是**窗口**，两件事 | 🔴 **没单独切过刀 —— 按「自检 ≠ 变异验过」记为未验** |
| `the_head_note_lists_exactly_the_needles_in_use` | 它钉的是「头注那一行 == `NEEDLES`」，`7u` 两样都没动 ⇒ 绿是对的 | `M1` / `M2`，**两个方向都验过** |
| `the_envelope_scan_is_not_silently_empty` | `7u` 的射程不含 `KR103D3` | 🔴 **没单独切过刀 —— 未验** |
| `every_error_envelope_site_is_signed_for` | 同上 | `M7`（unsigned）＋ `M9`（stale），**两向都验过** |
| `every_envelope_carries_both_keys` | 同上 | `M8` 验过 |
| `every_signed_row_says_why_it_is_its_own_copy` | 同上；它是表自身的形状自检 | 🔴 **没单独切过刀 —— 未验** |
| `the_needle_bites_on_both_writings_and_not_on_the_bare_word` | 同上；它是 `code_key_hits` 的纯函数自检（三刀内建） | 三刀内建，但**没从外面切过刀验它会红** |

🔴 **仍绿不等于仪式，但也不等于验过** —— 上表里点了 4 条「未验」，别读成「7u 证明了它们没牙」。

---

## 五 普查顺带逮到的一条**存量真缺陷**（写区外，一个字没动）

**`plugin/mod.rs` 的原始字符串让 `production_code` 提前收尾，约 30 行测试代码漏进「生产段」。**

- 病灶：`remote-daemon-proto/src/plugin/mod.rs:648` 的 `let lifted = r#"` ——
  那段样本是一张「擦掉了名字的真码表」，**内容里含列 0 的 `}`**。
- 机理：`src-tauri/crates/guard-core/src/lib.rs` 的 `test_module_ranges` 用
  `close = "\n}"`（列 0 右大括号）当测试模块的收尾判据 ⇒ 在那个 `}` 上提前收尾
  ⇒ 从 `"#;` 那一行到文件末尾**全部留在生产段里**。
- 后果：本 crate **每一条**按生产段扫的判据，在那份文件上的人群都是错的。
  本轮尺子①「放宽成任何字符串字面量」那一档的 4 处假红里，有 1 处就是它。
- 🔴 **它今天不红**：`assert_tree_strips_clean` 只查两样 —— 残留的 `#[test]` 属性、未剥的 `mod` 行；
  那段漏出来的尾巴**两样都没有**。
- ⚠ 而 `guard_support` 的头注**逐字预言过这一形**：
  「哪天有人在测试模块里写了一段列 0 含右大括号的原始字符串，**这里会红**，那时再上真正的大括号配对」
  —— **那句话今天是假的。**〔「写下来的边界不是判据」，同一族第五次。〕
- 🔴 **两处都在写区外**，而 `guard-core` 还住 `src-tauri/`（本件明令不许碰）⇒ **交 PM 立件。**
