# `K-R132` 死值验 —— 变异表（逐行真实输出）

## 〇 · 量具与分母，先写清楚

| | |
|---|---|
| **量具住址** | `/tmp/claude-1000/<本会话 id>/scratchpad/kr132/kr132-rig.sh`（跑）＋ `cut.py`（切/复原）。⚠ **会话级临时目录，会随会话消失** ⇒ 下面每一刀的**锚点与替换文本逐字抄在表里**，照着重打即可 |
| **被测对象指向哪棵树** | `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r132`，`CARGO_TARGET_DIR=.claude/pm-targets/k-r132` —— **与门禁那一格同源** |
| **在哪儿跑** | 沙箱（`ccmon-devbox:latest`，`--network none`）。挂载与环境**与 `.claude/devbox/gate` 那条 `docker run` 逐字同源**，只把末尾那条命令从 `bash scripts/gate.sh`（21 格，约 8 分钟）换成 `cargo test -p monitor --lib` —— 否则七刀跑不完。宿主上一条测试都没跑（`K31`） |
| **分母** | `cargo test -p monitor --lib` 的 `test result:` 那一行。⚠ 它**只是门禁 `cargo` 那一格的一部分**：门禁那个数是 9 个包合计（入场 1616 / 交回 1621），这里只有 `monitor` 这一个包的 lib 档 |
| **入场 / 交回**（monitor lib） | 入场 **1480 passed; 0 failed; 8 ignored**（＝ 刀② 那一栏「同一份 skip 名单、不切刀」现打）· 交回 **1485 passed; 0 failed; 8 ignored** ⇒ **新增 5 格** |
| **CRASH** | **0 次**。七刀全部编译得过、判定行都印出来了；没有一刀是「台子炸了」 |

**新增的 5 格**（本件全部新判据，逐条）：

1. `profile_installer::tests::the_powershell_block_puts_our_ccm_bin_dir_on_the_session_path`
2. `profile_installer::tests::the_path_line_points_at_the_directory_we_really_install_ccm_into`
3. `profile_installer::tests::the_generated_path_command_edits_only_the_user_scope_and_never_via_setx`
4. `profile_installer::tests::the_powershell_profile_lands_with_a_bom_and_the_posix_rc_never_does`
5. `profile_installer::tests::the_powershell_cc_still_bypasses_the_backend_and_that_is_registered_not_forgotten`（**反向锚点**）

---

## 一 · 变异表

| 刀 | 切在哪个函数 / 哪一处 | 锚点命中 | 真实输出（`test result:` 逐字） | 点名 | 最小面? |
|---|---|---|---|---|---|
| **③ 假红方向** 第 1 趟 | 不切 | —— | `ok. 1485 passed; 0 failed; 8 ignored; 0 measured; 0 filtered out` | —— | —— |
| **③** 第 2 趟 | 不切 | —— | `ok. 1485 passed; 0 failed; 8 ignored; 0 measured; 0 filtered out` | —— | —— |
| **③** 第 3 趟 | 不切 | —— | `ok. 1485 passed; 0 failed; 8 ignored; 0 measured; 0 filtered out` | —— | —— |
| **① 退掉那一步** | `profile_installer::render_path_block` —— 函数体开头插 `return String::new();` | **1 次** | `FAILED. 1484 passed; 1 failed; 8 ignored; 0 measured; 0 filtered out` | `the_powershell_block_puts_our_ccm_bin_dir_on_the_session_path` | ✅ **1 条** |
| **② 阴性对照** | 刀① ＋ 用 `--skip` 把本件那 5 条新判据全摘掉 | 1 次 | `ok. 1480 passed; 0 failed; 8 ignored; 0 measured; **5 filtered out**` | —— | 🔴 **不红** |
| **④ 落点分家** | `tool_registry::TOOLS` 里 `ccm` 那条**本机载体**的 `destination`：`".cc-monitor/bin/ccm*"` → `".cc-monitor/bin-elsewhere/ccm*"` | **1 次** | `FAILED. 1483 passed; 2 failed; 8 ignored; 0 measured; 0 filtered out` | `the_path_line_points_at_the_directory_we_really_install_ccm_into` ＋ **既有的** `tool_registry::installable_tools_declare_where_they_land` | ❌ 2 条，见 §2.1 |
| **⑤ 踩地雷** | `profile_installer::render_user_path_setup_command`：`$p = [Environment]::GetEnvironmentVariable('Path', 'User')` → `$p = $env:PATH` | **1 次** | `FAILED. 1484 passed; 1 failed; 8 ignored; 0 measured; 0 filtered out` | `the_generated_path_command_edits_only_the_user_scope_and_never_via_setx` | ✅ **1 条** |
| **⑥ 撤掉 BOM** | `profile_installer::encode_for_disk` 的 PowerShell 那一支 → `content.to_string()` | **1 次** | `FAILED. 1484 passed; 1 failed; 8 ignored; 0 measured; 0 filtered out` | `the_powershell_profile_lands_with_a_bom_and_the_posix_rc_never_does` | ✅ **1 条** |
| **⑦ 把反向锚点翻正** | `profile_installer::render_cc_code` 的 `cc_block`：`& claude $RemainingArgs` → `& ccm $RemainingArgs` | **1 次** | `FAILED. 1483 passed; 2 failed; 8 ignored; 0 measured; 0 filtered out` | `the_powershell_cc_still_bypasses_the_backend_and_that_is_registered_not_forgotten` ＋ **既有的** `launcher_identity_registry::the_terminal_launchers_are_still_where_the_registry_says` | ❌ 2 条，**而这 2 条正是它该有的形状**，见 §2.2 |

每一刀切之前都断言过「锚点恰好命中 1 次」再改，harness 逐刀印「变异已落地：<刀> @ <文件>（锚点命中 1 次）」；
复原之后 `git diff` 与切刀前那份 patch **逐字节相同**（现打对比过，0 行差异）。

---

## 二 · 两条不是最小面的，理由写死

### 2.1 刀④ 为什么是 2 条（不是判据粗）

第二条 `installable_tools_declare_where_they_land` 是**既有**判据：它要求「声明了可装的工具，
必须申报落点在哪」，而它同时钉着那条落点的形状。改落点它跟着响是**正确的**，
不是我的判据把面打宽了 —— 我那一条在 `profile_installer` 里，它在 `tool_registry` 里，
两条咬的是**同一个改动的两面**（「PATH 那一侧跟不跟得上」与「这条申报本身自不自洽」）。

🔴 **这一刀真正要买的东西是「分工对不对」，而它买到了**：刀① 把 `render_path_block` 掏空，
`the_path_line_points_at_…` **不红**（它比的是两处申报之间一不一致，掏空不影响一致性）；
刀④ 把落点改掉，`the_powershell_block_puts_…` **不红**（它是自洽的，两边一起变）。
⇒ **两条判据各守一半，谁也替不了谁。**

### 2.2 刀⑦ 为什么是 2 条（而且那正是它该有的形状）

刀⑦ 是**反向锚点的死值验**：把 `cc` 翻正成走 `ccm`。两条同时红 ——
我这一条（提醒「你在翻正一处已登记的不一致」）＋ 既有的 `launcher_identity_registry`
那条 `T2` 锚点（报文逐字：「**少了** = 这条终端启动器搬家或退役了 ⇒ 来本表改登记」）。

⇒ **「翻正要同拍改两处」这件事，机器上真的拦得住** —— 这就是把它做成反向锚点、
而不是写一句注释的全部理由。

⚠ **这一刀不是在证「本件把 `cc` 改好了」** —— 本件**没有**改它（三条理由住
`render_cc_code` 的头注与 `evidence/K-R132-摸底.md` §3）。它证的是：
**那处不一致今天被钉住了，翻正它的人一定会被机器叫住。**

---

## 三 · 🔴 判据盘不盘得住这一形 —— `KR132D2` 逐字要的那个答案

**诚实结论：本地判据只盘得到「片段生成」那一半；「真装完敲不敲得到」仍然只有真机答得了。**

| 这一形的哪一段 | 本地判据盘不盘得住 | 靠什么 |
|---|---|---|
| 我们要写进用户 profile 的那几行里**有没有** PATH 这回事 | ✅ 盘得住 | 刀① 现打：红 1 条，最小面 |
| PATH 那一行指的目录**与真落点是不是同一个** | ✅ 盘得住 | 刀④ 现打 |
| 给用户那条命令**有没有踩两个经典地雷** | ✅ 盘得住 | 刀⑤ 现打 |
| 落盘的字节**编码对不对**（BOM） | ✅ 盘得住（盘的是**字节形状**） | 刀⑥ 现打 |
| **PS 5.1 真的把它解对了吗** | ❌ **盘不住** | 只有真机。本轮实测读数住 `evidence/K-R132-摸底.md` §4.2 |
| **真跑一趟 NSIS `/S` / MSI `/qn`，再开一个新终端敲一次** | ❌ **盘不住** | 只有真机。⚠ 本轮**也没敲到这一格**（要把 v3.8.0 装到构建机上，`§0d` 明令别动）——`K-R129` 敲过装机那一半，本轮敲的是「装了终端集成之后」那一半 |

**为什么在构造上盘不住**：这一形是 Windows-only ＋ 真机装机，而本门禁的 npm / tsc / e2e / cargo
**全跑在 Linux 沙箱里**。`scripts/gate.sh` 的 `GATE_BLIND` 里 `windows-runner` 那一条早就逐字写着这件事
（「本门禁的 npm / tsc / e2e 全跑在 Linux 上……本机在构造上红不了」）。
⇒ **判据装在 `cargo` 那一格**（`profile_installer.rs` 的 `#[test]`），它够得到的就是上表打勾的那四行。

🔴 **刀② 是这段话最硬的那个证据**：把本件这 5 条摘掉之后，退掉 PATH 那一步 ⇒
**1480 格里一格都不红**。也就是说 `K-R129` 那条缺陷在本件之前**本地一格都看不见**，
而本件**只**把「片段生成」那一半接上了闸 —— 剩下那两行仍然是真机的活，我没有把它说成盘住了。
