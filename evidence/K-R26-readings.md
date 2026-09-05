# K-R26 读数簿 —— 插件继承整份环境能回连宿主：先关暗路

> 实现方，09-05。件文件住计划仓 `backend-consolidation/features/K-R26-插件继承整份环境能回连宿主.md`。
> **本文件只装读数**；每个数带「量于哪一刻 · 用什么量的 · 分母怎么切的」。

## 0 · 量于哪儿（住址与量具，一次写清）

- 工作树 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r26` · 分支 `track/k-r26` ·
  基点 **`9acbdd8`**（本轮全部读数的「改前」都量于这个提交的内容）。
- 门禁九格的**唯一**跑法：`PB_WS=backend-consolidation .claude/devbox/gate <树> k-r26`。
- 刀的读数与那两样门禁不管的东西：`evidence/K-R26-devbox.py`（`ccbus` / `daemon` 两个模式，
  同一个沙箱镜像，默认断网）。改动面：`evidence/K-R26-fn-md5.py`。
  ⚠ 两把量具都会先核一次「这棵树的分支是不是 `track/k-r26`」，对不上就 `exit 3` ——
  同名量具被覆盖成「指向另一棵树」那一族病（`brief` `5k`）。
- 🔴 **宿主上一条测试都没跑**：上面每一条都在 `ccmon-devbox:latest` 里。

## 1 · 门禁九格（入场 / 交回）

| 格 | 入场（`9acbdd8`，工作树无改动） | 交回（本轮全部改动） | 差 |
|---|---|---|---|
| cargo | 1428 passed | 1428 passed | 0 |
| generated | ok | ok | — |
| **daemon** | **583 passed** | **585 passed** | **+2** |
| npm | 1542 passed | 1542 passed | 0 |
| ccm-print-parity | PASS=12 | PASS=12 | 0 |
| ccm-rbind-title | PASS=8 | PASS=8 | 0 |
| ccm-cli | PASS=264 | PASS=264 | 0 |
| ccm-contract-parity | PASS=72 | PASS=72 | 0 |
| pb check | FAIL=0 BROKEN=0 | FAIL=0 BROKEN=0 | 0 |
| **合计** | **GATE: OK** | **GATE: OK** | |

**`+2` 的分母怎么数的**：本轮往 daemon 那一包加了 **3** 条判据 ——
`a_plugin_started_here_never_sees_the_listen_port_or_token`（外层，算 passed）·
`…_inner`（内层入口，标了 `#[ignore]` ⇒ 算 **ignored**，不进 passed）·
`plugin::layer_guard::the_child_environment_allowlist_has_exactly_one_home`（算 passed）。
⇒ passed **+2**、ignored **1 → 2**。总条数 584 → 587。

⚠ **PM 派工基线与 k-r26 树首趟日志有一格不等，如实登记**：
`scratchpad/baseline-k-r26.log`（PM 03:58 落）第 9 格是
`[J3 陈账] INDEX.md 比源文件旧` ⇒ `pb check: FAIL=1` ⇒ `GATE: FAIL`，
而 PM 补充里写的是 `pb check 0 ⇒ GATE: OK`。
**我自己重打的入场读数是 `FAIL=0`** —— 计划仓 `INDEX.md` 的 mtime 是 `03:58:57`，
比那份日志晚，也比当时最新的 `.md` 都晚 ⇒ 那一格是 PM **在落完那份日志之后**
自己跑 `pb index` 补掉的。**不是我的账，也不需要我做什么**，登记在这里只为对齐口径。

## 2 · `KR26D1` —— 子进程今天继承了什么（改前 / 改后两个读数）

量法：往假插件（`plugin_walk_fixture.rs`）加一个子命令 `roster`，它把
`/proc/self/environ` 逐键打回来；判据经 `plugin::invoke::run` 起它，把键名收成集合。

### 分母怎么切的（`KR26D1` 逐字要求写清）

- **不是** `cargo test` 那个进程。判据**再起一层进程**（`Command::new(current_exe())` +
  `--exact --ignored --nocapture --test-threads=1`，形状照 `relay::server::tests` 那台
  真子进程中转），把 `listen::ENV_PORT` / `listen::ENV_TOKEN` 用 `.env(…)` 交给**那个新进程**。
  ⇒ 「扮演 daemon 的进程」= 那个新进程；它的环境键数就是**分母①**。
- **它凭什么代表生产**：生产里 daemon 拿到那两个键的方式**一模一样** ——
  `listen.rs` 头注逐字「token 只能由宿主生成、当 env 传进来」，daemon 自己造不出它。
  同一条投喂路，只是投喂的人换成了判据。
- **为什么不用 `std::env::set_var`**：本 crate 里**四处**头注逐字禁掉了它
  （`plugin_walk_fixture.rs` · `control/gate.rs` · `relay/server.rs` · `relay/creds.rs`）——
  它与并行跑的别的判据是竞态。
- **为什么读 `/proc/self/environ` 而不是跑 `env`**：`env` 印的是**壳自己那一刻**的环境，
  壳会往里加自己的东西（POSIX 要求导出 `PWD`）⇒ 「子进程看到的键」里会混进不是继承来的几个，
  下面那条 ⊆ 会红在与本题无关的原因上。`/proc/self/environ` 是内核在 `execve` 那一刻放下的拷贝，
  **逐字节**就是 `Command` 交出去的那一份。
  ⚠ 拿不到 `/proc` 的机器上它会是空的 ⇒ 判据先断言「键集非空且含 `PATH`」（否则下面是空真）。

### 两个读数（逐字照抄判据打出来的那一行）

**改前**（工作树 = `9acbdd8` + 本轮夹具，`invoke.rs` 一个字没改；
`evidence/…/cut-A-before-envclear.log`）：

```
PWF-ENV-READING 父进程键数=38 · 子进程键数=39 · 口在子进程里=true · 令牌在子进程里=true
子进程键集=[CARGO, CARGO_HOME, CARGO_MANIFEST_DIR, CARGO_MANIFEST_PATH, CARGO_PKG_*(15),
CARGO_TARGET_DIR, CCM_LISTEN_PORT, CCM_LISTEN_TOKEN, DEBIAN_FRONTEND, HEDRON_REALM, HOME,
HOSTNAME, LD_LIBRARY_PATH, OLDPWD, PATH, PWD, PWF_ENV_INHERIT_CHILD, PWF_FILTER, RUSTUP_HOME,
RUSTUP_TOOLCHAIN, RUSTUP_TOOLCHAIN_SOURCE, RUST_RECURSION_COUNT, SHLVL, SSL_CERT_DIR,
SSL_CERT_FILE, _]
```
⇒ **39 = 38（父进程整份）+ 1（这次调用显式交办的 `HEDRON_REALM`）**，一个不少。
那两个键**都在**。

**改后**（同一棵树，`invoke.rs` 加了 `env_clear()` + 白名单；
`evidence/…/cut-B-after-envclear.log` 与 `tool-selfcheck.log` 两趟同值）：

```
PWF-ENV-READING 父进程键数=38 · 子进程键数=3 · 口在子进程里=false · 令牌在子进程里=false
子进程键集=["HEDRON_REALM", "HOME", "PATH"]
```
⇒ **3 = 白名单 7 键里这台机器上真有的 2 个（`PATH` `HOME`）+ 1 个显式交办的**。
⚠ 那 **3** 不是「白名单有几条」——白名单 7 条里 `TMPDIR` / `TMUX_TMPDIR` / `LC_ALL` /
`LC_CTYPE` / `LANG` 在这台沙箱上**本来就没设** ⇒ 判据只喂宿主真有的那几个
（`""` 与「没有」不是同一件事）。

## 3 · `KR26D2` —— 白名单逐键的论据

家：`plugin/invoke.rs` 的 `INHERITED_ENV_KEYS`（**一个具名常量、一个家**，
成员与条数只住那一处，本文件不复述成员表 —— `brief` 13b）。

**进了的那几个，一句话为什么**（详细论据住那个常量的头注，这里只列住址与要点）：
`PATH`（子进程还要找别的命令；`e2e/daemon-cc-bus.sh` 的 `[10]` 逐字「`PATH` 清空是不行的」）·
`HOME`（插件按 `~` 定位自己；同套 e2e 的 `[6]` 靠换 `HOME` 造「没装」那台机器）·
`TMPDIR` 与 `TMUX_TMPDIR`（这一族键的作用是**把子进程的写面圈小**，清掉写面**反而变大** ——
`TMUX_TMPDIR` 清掉会让子进程里的裸 `tmux` 回落到**用户自己那台**服务端）·
`LC_ALL`/`LC_CTYPE`/`LANG`（三个一起进，因为它们决定的是**同一档事**：优先级
`LC_ALL` > `LC_CTYPE` > `LANG`，只带一部分会给子进程一个与宿主**不同**的 locale）。

**考虑过而没进的**（「给不出论据的不进」，逐条写下来免得下一个人以为是漏了）：
`TMUX`/`TMUX_PANE`（`e2e/daemon-cc-bus.sh` 的 `[13]` 逐字要把它们**摘掉**才测得出正确行为）·
`TZ`（说得出的用途只有「时间戳同一个时区」，而今天经这一处口起出来的东西**没有一条**
把时间戳交回宿主 ⇒ 论据是想出来的不是量出来的）·`USER`/`LOGNAME`/`SHELL`/`TERM`
（现打：今天的被调方一处都没读它们；`stdin` 本来就是关掉的）。

## 4 · 🔴 对照臂 —— **它变红了，而且不是台架的账**

`e2e/daemon-cc-bus.sh`（CI 地板 50，`gate.sh` 里**没挂**）：

| 臂 | 代码状态 | 读数 | 落点 |
|---|---|---|---|
| 改前 | 退掉 `env_clear` + 白名单（在**最终树**上切一刀复现） | **PASS=50 FAIL=0** | `ccbus-BEFORE-cut4.log` |
| 改后 | 本轮交回的状态 | **PASS=31 FAIL=19** | `ccbus-AFTER-final.log` |
| 改后 + 候选适配层补丁 | 见 `§5`，在**副本**里试的 | **PASS=50 FAIL=0** | `ccbus-LAB-with-adapter-patch.log` |

**19 红是真缺陷，不是台架写法的问题。** 机制：那一族插件今天靠**继承**拿自己的配置 ——
`CC_BUS_HOME` · `CCBUS_POLICY_MODE` · 不带 `from` 那一趟的 `CC_BUS_ID`。
⚠ **`CC_BUS_HOME` 在那些脚本里有几处，两把尺子给两个数，分母都写出来**（09-05 现打，
面 = `shared/cc-bus/scripts/`）：按 **`$` 引用形**（`grep -rhoE '\$\{?CC_BUS_HOME'`）**13**；
按**整词命中**（`grep -rho 'CC_BUS_HOME'`，含赋值与 `export`）**16**。
两个数都不承重 —— 承重的是「它今天只从**继承**这条路过去」。
`control/cc_bus.rs::run_as` 今天只在 `as_id.is_some()` 时显式交办 `CC_BUS_ID`，其余**一个都不交办**。
⇒ `env_clear()` 一上，它们就没了。

🔴 **19 红里最要紧的一条不是「测试红了」，是一次真的破坏性动作**：

```
FAIL ★★ 名字被别人占：**不是 killed**，而是 stale_only: 期望[[false,true]] 实得[[true,false]]
FAIL ★★ 无辜会话还在: 期望[在] 实得[没了]
FAIL ★★ 无辜进程还在: 期望[在] 实得[被杀了]
```
读法：`CC_BUS_HOME` 没了 ⇒ 被起的那条命令回落到**默认**的台账目录 ⇒ 它照着一份**空的**台账
判「这个名字还是原来那个人吗」，判成「是」，于是**把一个同名的无辜会话连进程一起杀了**。
⇒ 这不是「关严了一点」，是**把一条破坏性命令的安全判定喂瞎了**。

## 5 · 候选修法（**不在本轮写区，请 PM 落**）——已在副本里验过

**住址**：`remote-daemon-proto/src/control/cc_bus.rs` 的 `run_as`。**唯一一处**，其余零改动。

**为什么家只能在那儿、不能在白名单里**：通用调用口那张白名单**按定框不许认识任何一个
具体插件** —— `plugin::layer_guard::the_generic_port_names_no_concrete_plugin` 的针里
**逐字就有这个前缀**（`concrete_plugin_words()` 那张表里那一条注着「某个插件的环境变量前缀」）。
把它加进白名单 = 让通用层认识一个具体插件 = 本件在守的那条线自己先破。
⇒ **谁的插件，谁交办。**

逐字（副本里跑过，`PASS=50 FAIL=0`）：

```rust
    // ★★〔`K-R26` 09-05〕**这一族键从此由本模块显式交办，不再靠继承**。
    //
    // 通用调用口现在先 `env_clear()`、再按它自己那张白名单喂
    //（`plugin::invoke::INHERITED_ENV_KEYS`），而那张白名单**按定框不许认识任何一个具体
    // 插件**（`plugin::layer_guard::the_generic_port_names_no_concrete_plugin` 的针里
    // 逐字就有这个前缀）⇒ 这一族键的家只能在这里：**谁的插件，谁交办**。
    //
    // 按**前缀**而不是逐个列名：这是 cc-bus 自己的命名空间（`CCBUS_RATE_*` /
    // `CCBUS_DEDUP_WINDOW` / `CCBUS_TTL` … 是一族会长的东西），逐个列名会漏。
    let owned: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| k.starts_with("CC_BUS_") || k.starts_with("CCBUS_"))
        .collect();
    let mut env: Vec<(&str, &str)> = owned
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    // `CC_BUS_ID` 是 cc-bus 自己的契约 ⇒ 由本模块拼，通用口只负责把 env 传下去。
    if let Some(id) = as_id {
        env.retain(|(k, _)| *k != "CC_BUS_ID");
        env.push(("CC_BUS_ID", id));
    }
```
（替换掉今天那个 `let env: Vec<(&str, &str)> = match as_id { … };`。）

**副本怎么做的**（`brief` 12c：拷 git worktree 做实验先把 `.git` 删掉）：
`tar --exclude=.git --exclude=node_modules --exclude=target` 拷到 scratchpad 下的 `lab-ccbus/`，
在副本里改 `cc_bus.rs`，用副本自己的 target 目录构建、在同一个沙箱镜像里跑那套 e2e。
⇒ **本轮工作树里 `control/cc_bus.rs` 一个字节没动**（`git status` 可核）。

⚠ **诚实边界**：这份补丁我只验了 `e2e/daemon-cc-bus.sh` 这一套（50 格）。
副本里**没有**跑 `cargo test`（`cc_bus_boundary_guard` 那 5 根针 · `layering_guard` ·
`readonly_guard` 都可能对新写的这几行有话说）⇒ **PM 落它的时候要重跑整套门禁**，
别把我这个 50/0 读成「整套都验过了」。

## 6 · 死值验（`§3` 那几刀 + `7u`）—— 逐行真实输出

分母口径统一：**「判定行」= daemon 那一包 `cargo test` 打出的那一行 `test result:`**
（本轮全部在 `evidence/K-R26-devbox.py daemon` 下跑，量于工作树内容 = `9acbdd8` + 本轮改动）。
基线判定行：`test result: ok. 585 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out`。
**本轮零 CRASH**：五刀的判定行都在，没有一刀是编译不过或异常退出。

| 刀 | 锚点（切在哪个函数 / 哪一处） | 锚点命中 | 变异 | 判定行 | 红了哪几条 |
|---|---|---|---|---|---|
| **1** 退掉 `env_clear` | `invoke.rs::run` 里 `cmd.env_clear();` | **1** | 整行注释掉（`production_code` 剥注释 ⇒ 生产段里它不存在） | `582 passed; 3 failed; 2 ignored` | ① 行为那条（报文逐字点名 `CCM_LISTEN_PORT`）② `the_child_environment_allowlist_has_exactly_one_home`（`left [(".env(",2)]` vs `right [(".env(",2),(".env_clear(",1)]`）③ `nothing_here_hands…`（`env_clear` 计数 `left 0 / right 1`） |
| **2** 白名单多塞令牌键 | `invoke.rs` 的 `INHERITED_ENV_KEYS` 里 `    ("LANG",` 那一行 | **1** | 其后插一条 `("CCM_LISTEN_TOKEN", …)`，白名单 7 → 8 条，`env_clear` **不动** | `584 passed; 1 failed; 2 ignored` | **只有**行为那条。报文：`常驻监听口的**令牌**漏进了插件进程：CCM_LISTEN_TOKEN … 子进程键集=["CCM_LISTEN_TOKEN","HEDRON_REALM","HOME","PATH"]`；读数行 `令牌在子进程里=true`。⇒ **判据看的是「键」，不是「调没调 `env_clear`」**（家唯一那条这一刀是绿的） |
| **3** 第二处白名单式写法 | `invoke.rs::run` 里 `for (k, v) in env {` 那个循环之后 | **1** | 补一行 `cmd.env("TZ", "UTC");`（`.env(` 2 → 3） | `583 passed; 2 failed; 2 ignored` | ① 家唯一（`left [(".env(",3),…]` vs `right [(".env(",2),…]`）② 行为那条 —— 而且红在 **⊆** 那一句：`这些键既不在 INHERITED_ENV_KEYS 里、也不是这次调用显式交办的：["TZ"]` ⇒ ⊆ 那一句**有牙** |
| **4 = `7u`** 整个退掉 | 同刀 1 的那一行 **＋** 它下面那个白名单 `for` 循环 | 1 + 1 | 两段一起注释掉（常量留着，不然编译不过） | `582 passed; 3 failed; 2 ignored` | 同刀 1 的三条。内层停在 `!child.contains(port_key)` 那一句 ⇒ 它**前面**那三句是绿着走过去的 |
| **5** 白名单清空 | `INHERITED_ENV_KEYS` 整张表 | 7 条 | 七条全注释掉（表变空） | `582 passed; 3 failed; 2 ignored` | ① 行为那条，红在**反空真**那一句：`子进程报回的键集里连 PATH 都没有 … 报回来的原文："HEDRON_REALM=alpha\n"` ② `nothing_here_hands…`（`child_path == host_path`）③ `the_walk_really_started_a_process_not_a_hand_built_done` |

**对照臂的两刀**（同刀 4 的代码状态，换量具跑 `ccbus`）：见 `§4` 那张表。

### `7u`：把实现整个退掉，新断言里**还有几条仍绿**

新增/改动的断言共 **11** 条（分母 = 我这一轮写下的每一句 `assert*`，逐句数的）。
刀 4（`env_clear` + 白名单全退）之后：

| # | 断言 | 7u 之后 | 仍绿的话，它的牙在哪一刀 |
|---|---|---|---|
| 1 | 外层：内层进程打出了读数行（反空真） | **绿** | 未切（要切「把内层的全名写错」那一刀）⇒ **判不了** |
| 2 | 外层：内层退出码为 0 | 红 | — |
| 3 | 内层：父进程环境里真有那两个键（反空真） | **绿** | 未切（要切「外层那两句 `.env(…)` 掉一句」）⇒ **判不了** |
| 4 | 内层：子进程键集含 `PATH`（反空真） | **绿** | **刀 5**（当场红，报文逐字端出 `"HEDRON_REALM=alpha\n"`） |
| 5 | 内层：显式交办那一项到了子进程 | **绿** | 未切（要切「把调用方 `env` 那个循环删掉」）⇒ **判不了** |
| 6 | 内层：`!child.contains(ENV_PORT)` | 红 | — |
| 7 | 内层：`!child.contains(ENV_TOKEN)` | 未跑到（6 先 panic） | **刀 2**（单独红过，报文点名） |
| 8 | 内层：⊆（键集 ⊆ 白名单 ∪ 显式交办） | 未跑到 | **刀 3**（单独红过，逮到 `TZ`） |
| 9 | 内层：白名单非空 | **绿** | 🔴 **够不着自己的反例**：把白名单清空的话第 4 条先红（刀 5 实测）⇒ 它是一句**冗余**的反空真，如实登记，不算数 |
| 10 | 内层：白名单每条都写了「为什么」 | **绿** | 未切（要切「把某一条的 why 改成空串」）⇒ **判不了** |
| 11 | 家唯一（三半：家在 · 别处零处 · 调用面对账） | 红 | — |

⇒ **11 条里 7u 之后仍绿 5 条**（1 · 3 · 4 · 5 · 9）。其中
**1 条另有刀证得出有牙**（第 4 条，刀 5）· **1 条明确是冗余**（第 9 条）·
**3 条本轮没切、判不了**（1 · 3 · 5，它们都是**反空真**类断言，
按设计就该在两边都绿；要证它们有牙得各切一刀，本轮没切）。
另外，被我**改了方向**的那条旧断言（`nothing_here_hands…` 的 `child_path == host_path`）
在 7u 之后也**仍绿** —— 它买的是「那一趟真的 fork/exec 了」，与清不清环境无关；
而它**有牙**：刀 5 当场红。

## 7 · 改动面（逐块 md5，量具 `evidence/K-R26-fn-md5.py`，基线 `9acbdd8`）

```
── remote-daemon-proto/src/plugin/invoke.rs
   基线 13 块 · 工作树 14 块 ⇒ 新增 1 · 删除 0 · 改了 1 · 一个字节没动 12
     新增  eecd0789a461  INHERITED_ENV_KEYS
     改了  277240315798 -> cc24605110e5  run
── remote-daemon-proto/src/plugin/mod.rs
   基线 13 块 · 工作树 17 块 ⇒ 新增 4 · 删除 0 · 改了 0 · 一个字节没动 13
     新增  62d2fa232358  ENV_CALL_SHAPES
     新增  526a8a06fcdb  ENV_CALL_SITES
     新增  b1f2919ed415  env_call_census
     新增  68b10d27130f  the_child_environment_allowlist_has_exactly_one_home
── remote-daemon-proto/src/plugin_walk_fixture.rs
   基线 49 块 · 工作树 57 块 ⇒ 新增 8 · 删除 0 · 改了 3 · 一个字节没动 46
     新增  b06d833a0876  ROSTER_SUB
     新增  531480c119c8  INHERIT_INNER_TEST
     新增  a6c2e1d808e8  INHERIT_MARK
     新增  d05bcddf2f05  INHERIT_READING
     新增  f027d710d362  FAKE_PORT_VALUE
     新增  d123856e53ac  FAKE_TOKEN_VALUE
     新增  fd6dc7a624ae  a_plugin_started_here_never_sees_the_listen_port_or_token
     新增  9557bede82d8  a_plugin_started_here_never_sees_the_listen_port_or_token_inner
     改了  c19769c4ae70 -> 31225cd11890  script_text
     改了  a3659bda6306 -> f8d5c466def6  fixture_vocabulary
     改了  db4e3d18b6b8 -> 70eb77738be0  nothing_here_hands_the_plugin_a_designed_way_back_into_the_host_commands
```
⚠ 口径：抽取器**按缩进切块**（Rust 没有现成的 `ast`），切的是块 + 紧挨其上的文档注释与属性
⇒ **只改头注也会让 md5 变**，这是刻意的（本轮改了三处头注，那要看得见）。
自检地板：切不到就报 CRASH 而不是「没变化」（本趟三个文件都过了地板）。

## 8 · 门禁自己逮到我一次（如实登记）

第一版把那两把沙箱量具写成了 `evidence/K-R26-*.sh`。交回前的门禁**第一趟当场红**，
判据 `shell_lint_registry::tests::every_shell_script_is_either_linted_or_registered_as_exempt`，
报文逐字点了这两个文件的名（`这些 shell 脚本既不在 CI 的 shellcheck 表达式里、也没登记豁免`）。
那张登记表住 `src-tauri/src/shell_lint_registry.rs`，**不在本轮写区**。
⇒ 修法不是去登记豁免，是**让这个问题消失**：两把量具改写成 `.py`（`evidence/` 下今天
一个 `.sh` 都没有，那是这个目录本来的写法），`.sh` 删掉。第二趟门禁 `GATE: OK`。
