# U1a · 守卫强度对拍基线

- 工作区：unified-backend · 主计划 §3 第一梯队 · 任务 #89
- 风险档：**中**（不改判据强度，只把强度变成可测量、可对拍的数据）
- 由来：账本 **S11** —— `sftp.rs::ccm_cli_has_required_elements` 在 U9 要迁到
  `control/` 的命令构造点。**迁移是强度悄悄下降的经典时机**：断言被「顺手重写」一遍，
  少一条 needle、`require` 的阈值改小一点，全绿，没人知道。
  U1a 在迁移**之前**把强度做成可测量的东西。

## 这条不是「加计数自检」—— 它已经有了

计划自审 §0.5-4 已订正：`ccm_cli_has_required_elements` 现有四路判据，
把 `shared/ccm` 掏空会**四路一起红**，不是空转变绿。所以 U1a 要做的不是补强度，
是**把现有强度从「一段散在的断言代码」变成「一份可被下一个实现对拍的数据」**。

## DoD

| # | 项 | 验收 |
|---|---|---|
| ① | 强度可**测量** | 有一个纯函数吃脚本文本、吐一个结构化的强度读数（needle 命中数 / 通道 A 字面量命中数 / `-t` 目标扫描的 `checked` / `$t` 定义是否被钉死） |
| ② | 强度有**基线**且机器钉住 | 一条测试断言当前读数**逐字段不低于**记录的基线。变异：从 needle 表里删一条 ⇒ 红；把 `require` 的阈值改小 ⇒ **不影响**（阈值不是强度，读数才是）；从 CLI 里删掉一处 `-t` 用法 ⇒ 红 |
| ③ | 原判据**一条不少地保留** | `ccm_cli_has_required_elements` 的四路判据全部还在，行为逐字不变；变异：把 CLI 里的 `=名:` 全改回裸目标 ⇒ 仍然红（这是 F01 修过的生产事故形状） |
| ④ | U9 能直接复用 | 强度模块与 `sftp.rs` 解耦：迁移时改的是「喂给它哪份脚本文本」，不是重写断言 |
| ⑤ | 全量门禁绿 | 两侧 `cargo test` + `fmt` + `tsc` + `npm test` |

**不做**：不改任何判据的强度（不加 needle、不提阈值、不改扫描器）；不动 `shared/ccm`；
不做 U1b（护栏按新边界重钉 —— 那要等 `observe/`/`control/` 存在）；不碰 `structural_scan` 内核。

## 与主计划对接

- **S11**（`ccm_cli_has_required_elements`）：本功能交付它的**前半**——基线。
  后半（迁移后逐条对拍）在 U9。账本里 S11 的最终形态因此细化为：
  「强度读数由 `ccm_cli_contract::measure()` 单一产出；迁移前后**同一个函数**跑两份脚本文本，
  逐字段 `>=`」。
- 不碰其他账本项。

## 逐条实现步骤

1. 新建 `src-tauri/src/ccm_cli_contract.rs`（`#[cfg(test)]`-only）：
   - `REQUIRED_NEEDLES` / `CHANNEL_A_LITERALS` / `EXACT_T_DEF` 三张表从 `sftp.rs` 原样搬出（**逐字不改**）。
   - `pub(crate) struct Strength { needles: usize, channel_a: usize, t_targets_checked: usize, t_def_pinned: bool }`
   - `pub(crate) fn measure(script: &str) -> Strength`
   - `pub(crate) fn assert_at_least(got: &Strength, floor: &Strength, who: &str)`
   - `BASELINE: Strength`（**实测填，不手打** —— 先写个明显偏高的值跑一次，从失败信息里读真值）
   *验证*：`cargo test` 编得过。
2. `sftp.rs::ccm_cli_has_required_elements` 改成调用共享表 + 保留原四路断言，**逐条对照原文核对没丢**。
   *验证*：`git diff` 逐行看；变异③跑一次。
3. 加基线测试：`measure(CCM_CLI_SCRIPT)` 逐字段 ≥ `BASELINE`。
   *验证*：变异①②各跑一次。
4. 全量门禁。

## 测试策略

变异一律退出码判定；`cp -a` 还原后 `touch`。**改完先 grep 计数确认变异真的落地**。
Rust 侧还要核「红得对不对」——rc=101 也可能是编译失败（假红）。

## 实现期与计划的偏离

### 偏离①：**DoD ② 里有一条判据是错的**，实现期用变异证伪后改了

计划原文：「把 `require` 的阈值改小 ⇒ **不影响**（阈值不是强度，读数才是）」。
**这条站不住。** 阈值改小**本身就是降强度** —— U9 迁移时写个 `require(1)`，`-t` 那条判据
当场退化成近乎无效，而读数基线完全管不着它。

当场用变异证了：把 `MIN_CHECKED_T_TARGETS` 从 10 改成 3，**全套依旧 4 passed**。

⇒ 读数与阈值是**两个都能被单独放水的旋钮**（读数 = 脚本里有多少处 `-t`；阈值 = 护栏肯为
多少处负责），两个都得钉。加了 `const _: () = assert!(MIN_CHECKED_T_TARGETS >= 10);`。

### 偏离②：基线的 `t_targets_checked` 是 **10 不是 11** —— 而且这暴露了一个陈旧断言

计划没预设这个数。实测跑出 10，而 `sftp.rs` 的注释逐字写着「真实脚本 checked=11 ……
往下留 1 的余量以免正常增删命令时误红」。

**没有直接把基线改成 10 了事**（方法纪律：守卫钉死的计数不要为了让自己过关而改 —— 先问
那个数该不该变）。逐 commit 追：

| commit | `-t` 数 |
|---|---|
| `a91a3dd`（写下「checked=11」那次） | 11 |
| `ca56c5d` / `65940b5` | 11 |
| **`666cc14`**（无名 `--tmux` 改为无条件新建会话） | **10** |
| `cdeeeec` / `d08660f` / HEAD | 10 |

逐行 diff 确认：`666cc14` 删掉两处 `tmux display-message -p -t "=$tmux_name…"`、
加回一处 `has-session -t "=$tmux_name:"`，净 −1。**是真实行为变更的正当结果，
不是护栏被悄悄丢了。**

⇒ 基线记 10。但顺带得出一个结论：**那句「留 1 的余量」早就不成立了，余量是 0。**
不下调阈值 —— 下调等于把「少一处 tmux 命令」重新变成无声的。

### 偏离③：五条钉子从运行期 `assert!` 改成**编译期** `const _: () = assert!(…)`

初版写在 `#[test]` 里，`cargo clippy` 当场指出「this assertion has a constant value」×3 ——
**它说得对**：两边都是 `const`，判定在编译期就能做完。改成编译期断言严格更强：
改坏了**编不过**（`error[E0080]`）、测试过滤器绕不开、顺带消掉 clippy 噪音
（噪音本身会让人对告警脱敏）。本仓已有先例：daemon 的 `RETIRE_MISS_THRESHOLD >= 2`。

实测：把阈值改成 3 ⇒ `error[E0080]: evaluation panicked: assertion failed: MIN_CHECKED_T_TARGETS >= 10`，
`could not compile`，RC=101。

### 我在这一轮踩的一个坑（记下来）

跑完「从 needle 表删一条」的变异后，我用 `git checkout -- src/ccm_cli_contract.rs` 还原 ——
**那是个未跟踪的新文件，`git checkout` 对它是空操作**，变异就那么留在文件里了。
是下一条命令的 system-reminder 贴出文件内容才看见 `@ccm_agent` 不见了。
⇒ **新文件的变异只能用 `cp -a` 备份还原**，`git checkout` 只对已跟踪文件有效。

## 变异验证（退出码判定）

| # | 变异 | 判据 | 实测 |
|---|---|---|---|
| 1 | `$t` 定义从 `=名:` 改成裸值 | `pin_definition` | **RC=101**，逐字报「定义必须逐字是 …… 改了它就能绕过整个结构性扫描」；基线那条同时红 |
| 2 | `attach -t "=$attach_name:"` → `attach -t "$attach_name"` | `=名:` 谓词 | **RC=101**：`第 367 行：tmux 目标必须是 =名: 精确形态`（共检查 10 处）—— 这是 F01 修过的生产事故形状 |
| 3 | `MIN_CHECKED_T_TARGETS` 10 → 3 | 编译期钉子 | 加钉子**前**：4 passed（**证伪了计划的判据**）；加钉子**后**：`error[E0080]` **编不过** |
| 4 | 从 `REQUIRED_NEEDLES` 删一条 `@ccm_agent` | 编译期钉子 + 读数 | **RC=101**：`表长与基线必须一致` + `关键要素命中 10 < 基线 11` |
| 5 | 构造读数低于基线（needles−1 / checked−1 / 不再 pin） | `assert_at_least` 自检 | 三条各自 `catch_unwind` 必 err；反向（读数上涨）不许误判 |

## 代码审计结果（D，一个综合视角 agent）

### 阻塞（0 项）

审计逐条字节级复核了「判据一条不少」：11 条 needle `diff -u` 无输出 · 2 条通道 A 字面量
`cat -A` 核过转义位置 · `EXACT_T_DEF` 逐字相同 · `scan_after_marker` 五个参数逐个等价
（**窗口 48 一字未改**、放行谓词语义一致）· `require` 阈值等价且更强。
5 条编译期钉子**逐条隔离变异**，全部 `error[E0080]` + `could not compile`，非恒真。

### 重要（3 项，全部当轮修掉）

| # | 发现 | 处置 |
|---|---|---|
| **A1** | **我造成的静默退化**：新模块的 `mod` 声明插错了位置，把 `#[cfg(test)]` 与 `mod structural_scan;` 的配对拆开 ⇒ `structural_scan` 变成**无条件编译**。`lib.rs:50-53` 的注释逐字写着「加 `cfg(test)` …… 顺带消掉 5 条 dead_code 警告」，那条决策被原样撤销。审计是**数出 dead_code 正好 +5** 才发现的；CI 的 `cargo build` 没有 `-D warnings` ⇒ **不会红** | 改回正确配对；实测 dead_code **16 → 11**，与 `git stash` 出来的 HEAD 基线**逐字相等**。并在插入点写下警示注释 |
| **A2** | **`Strength` 漏了 `violations`，读数对 F01 的生产事故形状是瞎的。** 审计实测：把 4 处非 `$t` 的精确目标全改成裸目标（**不动 `t=` 定义**），`checked` 仍 10、`t_def_pinned` 仍 true ⇒ **四字段逐字段相等、`assert_at_least` 全绿**。今天挡住它的是 `sftp.rs` 那个 `require()`——**而那正是 U9 要搬走重写的那一段**。照账本 S11「逐字段 `>=`」照做，U9 可以只带走 `measure()` 而把 F01 的防线丢在原地 | 加 `t_violations` 字段（**比较方向与其他三个相反**：只许少不许多）+ 编译期钉子 `BASELINE.t_violations == 0`。复验：同一变异现在报「`-t` 目标违规 4 > 基线 0」 |
| **A3** | `pin_definition` 的四个参数**两处各写一遍** —— 与我自己给 `scan_t_targets` 写的理由（「两处各写一遍，迟早漂」）冲突 | 抽 `pin_t_def(script)`，真断言 `.expect(…)`、读数 `.is_ok()` |

### 审计的一条判断我采纳并记下

它指出 DoD ② 那条错判**与主计划账本直接冲突**：`MASTERPLAN.md` 的 S11 逐字要求
「`require` 的 `min_checked` **不低于迁移前的 checked**」。⇒ 不是我「加严」，是功能计划抄漏了账本。

### 建议（登记）

- 5 条编译期钉子只在**测试构建**里存在（整模块 `#![cfg(test)]`）。CI 跑 `cargo test --all` 故无洞，
  但只跑 `cargo build` 不会触发它们。
- `MIN_CHECKED_T_TARGETS >= 10` 挡的是「只改常量」，挡不住「连 assert 里的字面量一起改」——
  这是这类钉子的固有上限，注释已正面交代。
- `doc/INVARIANTS.md:686` 仍指 `sftp.rs::ccm_cli_has_required_elements`，今天准确，U9 迁移时要改。

## 工程审计结果（E，主线程对账）

- **账本 S11 的最终形态因本功能细化**（见主计划 §2）：强度读数由 `ccm_cli_contract::measure()`
  单一产出；迁移前后同一函数跑两份脚本文本，逐字段比较 —— **三个字段 `>=`、`t_violations` 是 `<=`**。
  A2 证明了少写最后这半句就会漏掉 F01 的整条防线，所以账本里必须写全。
- **U9 的迁移清单**（现在就写死，免得到时候只搬一半）：`measure()` 喂新构造点文本 ·
  `require()` **必须一起搬**（它看 violations，读数只是它的镜子不是替身）· `pin_t_def()` ·
  `doc/INVARIANTS.md:686` 的指向。
- 未碰其他账本项；`CCM_CLI_SCRIPT` 放宽到 `pub(crate)` 是让新模块可见的最小改动，
  全仓无守卫依赖它私有（审计 grep 核过）。

## 签收

- [x] 过代码审计（D）—— 阻塞 0 · 重要 3，全部当轮修完并各自复验
- [x] 过工程审计（E，主线程对账）—— 账本 S11 细化并写死 U9 迁移清单
- [x] 主计划已更新（F）

## 最终门禁

| 项 | 值 |
|---|---|
| monitor `cargo test --lib` | **663 passed / 3 ignored**，RC=0 |
| daemon `cargo test` | 194 passed，RC=0 |
| `cargo fmt --check` 两侧 | OK |
| `cargo clippy --all-targets` 本模块命中 | **0**（初版有 3 条「assertion has a constant value」，改编译期断言后清零） |
| `cargo build --lib` dead_code | **11 == HEAD 基线**（A1 修复前是 16） |
| `tsc --noEmit` / `npm test` | RC=0 / 80 文件 1154 例 |
| `assert-pass-floor.sh ccm-cli 44` | `PASS=44（地板 44）` |
