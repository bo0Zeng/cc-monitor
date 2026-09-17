# `K-R135` 死值验 —— 逐刀真实输出

> 量于 **09-15**，被测对象 = 工作树 `.claude/worktrees/k-r135`，切刀基线提交 **`f578827`**。
> 量具：`docker run … ccmon-devbox:latest … cargo test -p monitor --lib`
> —— **同镜像 `ccmon-devbox:latest`、同挂载、同 `--network none`、只换最后那条命令**
> （`.claude/devbox/gate` 末行写死 `bash scripts/gate.sh`，跑不了单把量具；缺口已立 `K-R130`）。
> 驱动脚本 `$SCRATCH/kr135/runall.sh` ＋ 逐刀 patch `$SCRATCH/kr135/cuts/c<N>.py`：
> **每刀先断言锚点命中数再改**，打印「变异已落地」；跑完 `git checkout --` 还原，收工 `git status` 干净。
>
> 🔴 **分母（三处都写明，别相减）**
> · 下面每条判定行都是 `cargo test -p monitor --lib` **这一个档**（monitor 一个包的 lib 测试），
>   **阴性对照 = `ok. 1488 passed; 0 failed; 8 ignored`**；
> · 门禁 `cargo` 那一格是**9 个包合计取最大值**，本件入场 **1621** → 交回 **1624**（+3）；
> · 两把尺子作用域不同。`monitor --lib` 这一档的**入场值我没单独打过**，别拿 1488 减出一个入场数。
> · 「+3」= **新立 3 条**判据（状态相等口径 · 别名 snippet 行为 · 线上字段对拍）；
>   另有 **2 条是翻正**既有判据（名字换了、条数不变）。

## 〇 · 阴性对照（不切刀，原样跑一趟）

```text
test result: ok. 1488 passed; 0 failed; 8 ignored; 0 measured; 0 filtered out; finished in 16.72s
```

⇒ **台子本身不是恒红**，下面每一条红都是那一刀切出来的。

## 一 · 变异表（逐行真实输出）

| 刀 | dod | 切在哪（锚点） | 命中 | 判定行 | 红的是哪几条 | 打中哪条断言 | 最小面？ |
|---|---|---|---|---|---|---|---|
| **①** | `KR135D1b` | `render_cc_code` 拼装点 —— 把会话级 `$env:PATH = "$ccmBinDir;$env:PATH"` **加回去** | 1 | `FAILED. 1487 passed; 1 failed` | `the_powershell_block_never_touches_the_session_path_again` | `:2537`「这一块又在动会话级 `$env:PATH` 了」 | ✅ 1 条 |
| **②** | `KR135D1` | **「加」**那条：`GetEnvironmentVariable('Path','User')` → `$env:PATH`（**地雷 2**） | 1 | `FAILED. 1487 passed; 1 failed` | 地雷判据 | `:2662`「**「加」**的新值不是从用户级那一档读出来的」 | ✅ 1 条 |
| **③** 🔴 | `KR135D1` | **「撤」**那条：`-ne $d`（整格）→ `-notlike "*$d*"`（子串） | 1 | `FAILED. 1487 passed; 1 failed` | 地雷判据 | `:2683`「**「撤」**不是按整格比的 —— 子串口径会把 `<我们那段>-old` 之类一起删掉」 | ✅ 1 条 |
| **④** | `KR135D1` | `user_path_has_our_bin`：整格相等 → `contains` 子串 | 1 | `FAILED. 1487 passed; 1 failed` | `the_user_path_status_uses_the_same_equality_as_the_generated_commands` | `:2846`「子串比法：`-old` 被读成「已经在了」」 | ✅ 1 条 |
| **⑤** | `KR135D2` | `cc_block`：`& {word} $RemainingArgs` → `& claude $RemainingArgs` | 1 | `FAILED. 1486 passed; **2** failed` | `the_powershell_cc_goes_through_ccm_exactly_like_the_posix_one` ＋ `launcher_identity_registry::the_terminal_launchers_are_still_where_the_registry_says` | 前者调用行 `assert_eq!` 不等；后者 `launcher_identity_registry.rs:282`，`left: 0` | ✅ **刻意 2 条** |
| **⑥** | `KR135D3` | `shared/ccm-aliases.sh` 那一行 → 只留 `$HOME/.local/bin`（**回到病灶那一版**） | 1 | `FAILED. 1487 passed; 1 failed` | `the_shared_alias_snippet_really_puts_both_ccm_dirs_on_path_local_first` | `:2712` 那一族「本机那个 `ccm` 落点不在 PATH 上」 | ✅ 1 条 |
| **⑦** | `KR135D3` | 同一行，两个目录都在、**只把顺序对调** | 1 | `FAILED. 1487 passed; 1 failed` | 同上 | 顺序那一断言「`…/.cc-monitor/bin` 排在 `…/.local/bin` 后面」 | ✅ 1 条 |
| **⑧** | `KR135D1` | **全退**：`render_user_path_setup_command`→`None` · `render_user_path_removal_command`→`None` · `user_path_has_our_bin`→`false` | 3 处各 1 | `FAILED. 1486 passed; **2** failed` | 地雷判据 ＋ 状态判据 | 见 §三 | — |
| **⑨** 🆕 | `KR135D1` | **「探」**那条：`GetEnvironmentVariable('Path','User')` → `$env:PATH` | 1 | `FAILED. 1487 passed; 1 failed` | 地雷判据 | `:2662`「**「探」**的新值不是从用户级那一档读出来的」 | ✅ 1 条 |
| **⑩** 🆕 | `KR135D1` | 生产段**再多起一处进程**（`user_path_add` 里加一句 `Command::new("cmd")`） | 1 | `FAILED. 1486 passed; **2** failed` | 地雷判据 ＋ **`write_site_registry::spawn_sites::every_local_spawn_is_declared`** | `:2724`「起进程的地方应当**恰好一处**…实得 2 处」 | ✅ **2 条是好事**，见下 |
| **⑪** 🆕 | `KR135D1` | 在 `user_path_remove` 里**再拼一段**写环境变量的 PowerShell（第二份实现） | 1 | `FAILED. 1487 passed; 1 failed` | 地雷判据 | `:2745`「「写环境变量」的地方应当**恰好两处**…多一处 = 同一件事有了第二份实现」 | ✅ 1 条 |
| **⑫** 🆕 | `KR135D1` | 给 `UserPathStatus` 的字段加 `#[serde(rename = "onPath")]`，**不改前端** | 1 | `FAILED. 1487 passed; 1 failed` | `the_user_path_status_wire_fields_match_the_hand_written_ts` | `:2799`「线上字段 `onPath` 在前端那份**手写**接口里找不到」 | ✅ 1 条 |

**无 CRASH**：十四刀每一趟都印出了判定行，`1487+1` / `1486+2` / `1488+0` 三种形状都对得上总数 1488。

## 二 · 三处「同一条判据、不同断言」—— 这正是「把人群扩过去」买到的牙

②③⑨ 红的**判据名字一模一样**（`KR135D1` 是往它里面加人群，不是新起一条）。
分得开它们的是**断言行号与那句话里的「加 / 撤 / 探」**：

| | 切的是哪一条命令 | 打中 | 上一轮那版判据会不会红 |
|---|---|---|---|
| ② | 「加」 | `:2662`「**加**」 | 会（上一轮就有「加」） |
| ③ | 「撤」 | `:2683`「**撤**」整格那一条 | 🔴 **不会** —— 上一轮人群里根本没有「撤」 |
| ⑨ | 「探」 | `:2662`「**探**」 | 🔴 **不会** —— 「探」是本轮才有的第三条 |

⚠ **按格认，不按标签字面认**（`brief 12b`）：只看判据名会把这三刀读成「同一刀跑了三遍」。

## 三 · 把实现整个退掉，还有多少条新断言仍绿（刀⑧）

退掉 `KR135D1` 的**三处生产实现**。结果：**2 红 4 绿**。

| 本轮的判据 | 刀⑧ 之后 | 为什么仍绿 —— 逐条给理由 |
|---|---|---|
| 地雷判据（加/撤/探三条命令） | 🔴 红 | 两条命令变 `None`，`.expect("生成的「加」命令")` 当场炸 —— 地板断言起作用了 |
| `the_user_path_status_uses_the_same_equality_…` | 🔴 红 | 恒 `false` 被**第一条正向地板**逮到（「整格命中都认不出来」） |
| `the_powershell_block_never_touches_the_session_path_again`（`D1b`） | ✅ 仍绿 | **应该绿**：它判的是**生成进 profile 的那一块里有没有 `$env:PATH`**，与这三个函数无关。牙在刀①（1 红）。 |
| `the_powershell_cc_goes_through_ccm_exactly_like_the_posix_one`（`D2`） | ✅ 仍绿 | **应该绿**：它判 `cc` 那一行走不走 `ccm`，另一个实现面。牙在刀⑤（2 红）。 |
| `the_shared_alias_snippet_really_puts_both_ccm_dirs_on_path_local_first`（`D3`） | ✅ 仍绿 | **应该绿**：它判 `shared/ccm-aliases.sh` 那一行，连 Rust 都不读（真 `source` 一趟量 `$PATH`）。牙在刀⑥⑦（各 1 红）。 |
| `the_user_path_status_wire_fields_match_the_hand_written_ts` | ✅ 仍绿 | **应该绿**：它判的是**类型的线上字段名**，与那三个函数的**函数体**无关。牙在刀⑫（1 红）。 |

⇒ **四条「仍绿」都不是仪式**：各自的牙在别的刀上，逐刀有账。

## 四 · 🔴 刀⑩ 那「2 条」是**两道独立的闸同时响**，不是粗刀

- 一条是本件自己那句 `assert_eq!(spawns, 1)`（`K33`：起进程只许一处）；
- 另一条是仓里那张**默认拒绝**的登记表 `write_site_registry::spawn_sites::every_local_spawn_is_declared`
  —— 它逐字要求「本机每一处起进程都要申报」。

⇒ **「偷偷多起一个进程」这件事在本仓有两道彼此独立的闸**，本轮新加的那一处
（`profile_installer.rs::run_user_path_powershell`）**两边都登记了**。
★ 这也反过来证明 `R88` 放行 `SPAWNS` 那一行**不是走形式**：不加那一行，闸当场红。

## 五 · 假红方向（切**不该红**的地方，确认它们不红）

| 刀 | 切了什么 | 判定行 | 红名单 | 买到什么 |
|---|---|---|---|---|
| **⑬** | `profile_installer.rs` 里**一句注释**，逻辑一个字节没动 | `ok. 1488 passed; 0 failed` | 空 | 这几条判据**不是按文件 md5 / 注释文本**判的 |
| **⑭** | `shared/ccm-aliases.sh` 那一行的**行尾注释**（两个目录与顺序都没动） | `ok. 1488 passed; 0 failed` | 空 | `D3` 那条判的是 **`source` 之后的 `$PATH`**，改措辞不会假红 |

⇒ **十四刀合起来：12 刀该红的红了、红在正格上；2 刀不该红的没红。**

## 六 · 诚实边界（这张表买不到什么）

- 🔴 **它一个字都不证明「真机上能用」。** 十四刀全跑在 **Linux 沙箱**里
  （`GATE_BLIND` 的 `windows-runner` 逐字写着这一族盖不到）。
  那一跳 `powershell.exe` 起不起得来 · `[Environment]::SetEnvironmentVariable` 的
  **`WM_SETTINGCHANGE` 广播**真不真的生效 · 改完重开终端三种 shell 是不是都敲得到 `ccm`
  —— **本件一趟真机都没上**，全是「没验」，不是「验过是绿的」。
- 🔴 **界面那一格的行为面没有判据。** 它的两条真纪律（「探不动不许静默成未安装」·
  「两个按钮互斥禁用」）今天**没有 vitest 在数** —— 落点只能是
  `src/launcher-diagnostics.vitest.ts`，**那一份不在 `K-R135` 的写区里**。
  ⇒ 这张表证的是「**后端那半 ＋ 类型边界有牙**」，**不是**「那一格点起来是对的」。
- 刀⑤ 的「2 条」与刀⑩ 的「2 条」**性质不同**：前者是同一处事实的两个登记点（`K-R132` 早就验过），
  后者是两道**独立**的闸。两者都不是粗刀。
