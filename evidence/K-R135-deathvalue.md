# `K-R135` 死值验 —— 逐刀真实输出

> 量于 **09-15**，被测对象 = 工作树 `.claude/worktrees/k-r135`，切刀基线提交 **`0edd88a`**。
> 量具：`docker run … ccmon-devbox:latest … cargo test -p monitor --lib`
> —— **同镜像 `ccmon-devbox:latest`、同挂载、同 `--network none`、只换最后那条命令**
> （`.claude/devbox/gate` 最后一行写死 `bash scripts/gate.sh`，跑不了单把量具；缺口已立 `K-R130`）。
> 驱动脚本：`$SCRATCH/kr135/runall.sh` ＋ `$SCRATCH/kr135/cuts/c<N>.py`（每刀先断言锚点命中数再改，
> 并打印「变异已落地」；每刀跑完 `git checkout --` 还原那 4 份文件）。
>
> 🔴 **分母**：下面每一行的判定行都是 **`cargo test -p monitor --lib` 这一个档**
> （monitor 这一个包的 lib 测试）。**它不是门禁 `cargo` 那一格的 1623** ——
> 那个数是**9 个包合计取最大值**，两把尺子作用域不同，**别相减**。

## 〇 · 阴性对照（不切刀，原样跑一趟）

```text
test result: ok. 1487 passed; 0 failed; 8 ignored; 0 measured; 0 filtered out; finished in 15.54s
```

⇒ **台子本身不是恒红**，下面每一条红都是那一刀切出来的。

⚠ **「本件净加 2 条」这个数是从门禁那把尺子上量的，不是从这一档**：门禁 `cargo` 那一格
入场 **1621 passed**、交回 **1623 passed**（同一把尺子两次读数，差 **+2**）。
本件**新立 2 条**判据、**翻正 2 条**既有判据（名字换了、条数不变）⇒ 与 +2 对得上。
**`cargo test -p monitor --lib` 这一档的入场值我没单独打过** —— 别拿 1487 去减出一个入场数。

## 一 · 变异表（逐行真实输出）

| 刀 | dod | 切在哪（锚点） | 锚点命中 | 判定行（逐字） | 红的是哪几条 | 打中哪条断言 | 最小面？ |
|---|---|---|---|---|---|---|---|
| **①** | `KR135D1b` | `profile_installer.rs::render_cc_code` 的拼装点 —— 把一段会话级 `$env:PATH = "$ccmBinDir;$env:PATH"` **加回去** | 1 | `FAILED. 1486 passed; 1 failed; 8 ignored` | `the_powershell_block_never_touches_the_session_path_again` | `:2390`「这一块又在动会话级 `$env:PATH` 了（include_cc_function=true）」 | ✅ 1 条 |
| **②** | `KR135D1` | `render_user_path_setup_command`：`$p = [Environment]::GetEnvironmentVariable('Path','User')` → `$p = $env:PATH`（**地雷 2，加那一侧**） | 1 | `FAILED. 1486 passed; 1 failed; 8 ignored` | `the_generated_path_command_edits_only_the_user_scope_and_never_via_setx` | `:2508`「**「加」**的新值不是从**用户级**那一档读出来的」 | ✅ 1 条 |
| **③** 🔴 | `KR135D1` | `render_user_path_removal_command`：`Where-Object { $_ -ne $d }` → `{ $_ -notlike "*$d*" }`（**撤那一侧独有的第三条**） | 1 | `FAILED. 1486 passed; 1 failed; 8 ignored` | `the_generated_path_command_edits_only_the_user_scope_and_never_via_setx` | `:2529`「**「撤」**不是按**整格**比的 —— 子串口径会把 `<我们那段>-old` 之类一起删掉」 | ✅ 1 条 |
| **④** | `KR135D1` | `user_path_has_our_bin`：`split(';').any(整格相等)` → `to_ascii_lowercase().contains(...)` | 1 | `FAILED. 1486 passed; 1 failed; 8 ignored` | `the_user_path_status_uses_the_same_equality_as_the_generated_commands` | `:2604`「子串比法：`<我们那段>-old` 被读成「已经在了」」 | ✅ 1 条 |
| **⑤** | `KR135D2` | `render_cc_code` 的 `cc_block`：`& {word} $RemainingArgs` → `& claude $RemainingArgs` | 1 | `FAILED. 1485 passed; **2** failed; 8 ignored` | `the_powershell_cc_goes_through_ccm_exactly_like_the_posix_one` ＋ `launcher_identity_registry::the_terminal_launchers_are_still_where_the_registry_says` | 前者：调用行 `assert_eq!` 不等；后者 `launcher_identity_registry.rs:282` 「`T2` 的锚点 `& {word} $RemainingArgs` … left: 0」 | ✅ **刻意 2 条** |
| **⑥** | `KR135D3` | `shared/ccm-aliases.sh`：`for __ccm_d in "$HOME/.local/bin" "$HOME/.cc-monitor/bin"` → 只留 `.local/bin` | 1 | `FAILED. 1486 passed; 1 failed; 8 ignored` | `the_shared_alias_snippet_really_puts_both_ccm_dirs_on_path_local_first` | `:2712`「本机那个 `ccm` 落点不在 PATH 上」，实得 3 段（只有 `.local/bin`） | ✅ 1 条 |
| **⑦** | `KR135D3` | 同一行，**两个目录都在，只把顺序对调** | 1 | `FAILED. 1486 passed; 1 failed; 8 ignored` | 同上 | `:2729`「`…/.cc-monitor/bin`（我们放下去的那一份）排在 `…/.local/bin`（你那份旧的住处）后面」 | ✅ 1 条 |
| **⑧** | `KR135D1` | **全退**：三处一起退 —— `render_user_path_setup_command` → `None` · `render_user_path_removal_command` → `None` · `user_path_has_our_bin` → `false` | 3 处各 1 | `FAILED. 1485 passed; **2** failed; 8 ignored` | `the_generated_path_command_edits_only_the_user_scope_and_never_via_setx` ＋ `the_user_path_status_uses_the_same_equality_as_the_generated_commands` | 见下节 | — |

**无 CRASH**：十刀每一趟都印出了判定行，且 `1487 = 1485 passed + 2 failed` / `1486 + 1` 两种形状都对得上总数，
没有一趟是「判定行掉了」或「异常退出」。

## 二 · 🔴 ②③ 是**同一条判据的两条不同断言** —— 这正是「扩到撤那一侧」买到的东西

②③ 的「红的是哪几条」字面**一模一样**（判据名没变，`KR135D1` 是往它里面加人群，不是新起一条）。
**分得开它们的是断言行号与那句话**：`:2508` 说的是「加」、`:2529` 说的是「撤」。
⇒ 件文件逐字要求的「**照它扩到撤那一侧**」是**真的有牙**的：
把撤那一条的整格比换成子串比，在上一轮那版判据下**一条都不会红**（上一轮的人群里根本没有「撤」）。

⚠ **按格认，不按标签字面认**（`brief 12b`）：只看判据名会把 ②③ 读成「同一刀跑了两遍」。

## 三 · 把实现整个退掉，还有多少条新断言仍绿（刀⑧）

退掉的是 `KR135D1` 的**三处生产实现**（逐处见上表刀⑧）。结果：**2 红 3 绿**。

| 本轮的判据 | 刀⑧ 之后 | 为什么仍绿 —— 逐条给理由 |
|---|---|---|
| `the_generated_path_command_edits_only_the_user_scope_and_never_via_setx` | 🔴 红 | 两条命令都变 `None`，`.expect("生成的「加」命令")` 当场炸 —— 地板断言起作用了 |
| `the_user_path_status_uses_the_same_equality_as_the_generated_commands` | 🔴 红 | 恒 `false` 被**第一条正向地板**逮到（「整格命中都认不出来 —— 判据够不着被测对象了」） |
| `the_powershell_block_never_touches_the_session_path_again`（`D1b`） | ✅ 仍绿 | **应该绿**：它判的是**生成进 profile 的那一块里有没有 `$env:PATH`**，与这三个函数在不在没有关系。它的牙在刀①上（现打 1 红）。 |
| `the_powershell_cc_goes_through_ccm_exactly_like_the_posix_one`（`D2`） | ✅ 仍绿 | **应该绿**：它判的是 `cc` 那一行走不走 `ccm`，另一个实现面。它的牙在刀⑤上（现打 2 红）。 |
| `the_shared_alias_snippet_really_puts_both_ccm_dirs_on_path_local_first`（`D3`） | ✅ 仍绿 | **应该绿**：它判的是 `shared/ccm-aliases.sh` 那一行，连 Rust 都不读（真 `source` 一趟量 `$PATH`）。它的牙在刀⑥⑦上（各 1 红）。 |

⇒ **三条「仍绿」都不是仪式**：它们各自的牙在别的刀上，逐刀有账。

## 四 · 假红方向（切**不该红**的地方，确认它们不红）

| 刀 | 切了什么 | 判定行 | 红名单 | 买到什么 |
|---|---|---|---|---|
| **⑨** | `profile_installer.rs` 里**一句注释**（`// 🔴 KR135D2：这一行就是翻正的落点。` → 换一句话），**逻辑一个字节没动** | `ok. 1487 passed; 0 failed` | 空 | 这几条判据**不是按文件 md5 / 不是按注释文本**判的 |
| **⑩** | `shared/ccm-aliases.sh` 那一行的**行尾注释**（两个目录与顺序都没动） | `ok. 1487 passed; 0 failed` | 空 | `D3` 那条判据判的是 **`source` 之后的 `$PATH`**，不是那一行长什么样 ⇒ 改措辞不会假红 |

⇒ **十刀合起来**：8 刀该红的红了、红在正格上；2 刀不该红的没红。

## 五 · 诚实边界（这张表买不到什么）

- 🔴 **它一个字都不证明「真机上能用」。** 全部十刀跑在 **Linux 沙箱**里
  （`GATE_BLIND` 的 `windows-runner` 逐字写着这件事）。`cc` 翻正之后敲下去起不起得来 ·
  那两条 PowerShell 命令在真 Windows 上改 PATH 的真实结果 —— **本件一趟真机都没上**。
- 🔴 **`KR135D1` 的界面那一格没有进这张表，因为它没落地**（IPC 那一跳要动写区外的
  `lib.rs` 与 `parity_ledger.rs`，逐条住 `features/K-R135-…md` 的 `§8`）。
  ⇒ 这张表证的是「**后端那半的机制有牙**」，**不是**「用户点得着」。
- 刀⑤ 的「2 条」是**刻意**的，不是粗刀：`K-R132` 上一轮就现打验过「动那一行 ⇒ 两条同时红」，
  本件两处一起改，那条性质原样保留。
