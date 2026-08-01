# U-1 · 护栏当下缺陷修复（第零梯队）

- 工作区：unified-backend · 主计划 §3 第零梯队 · 任务 #103
- 风险档：**高**（动的是护栏本身 —— 改坏了整个工作区都在假绿里跑）
- 由来：Phase A 四视角计划自审逮出三个**当下就坏着**的东西。它们不是本区制造的，
  但**本区第一步 U2/U3 就会引爆前两个**，所以必须先修。

## DoD

| # | 项 | 验收 |
|---|---|---|
| ① | `no_timer_guard::daemon_sources` **递归** | 拆出子目录后仍扫得到；地板从魔数 `5` 换成 **「扫到的 .rs 数 == 目录树里的 .rs 数」**；变异：把某个子目录里的文件加一处 `Duration::from_secs` 必须红 |
| ② | 三个守卫的**测试段 marker** 抽共享 helper | `main.rs`（`mod stream_flag_tests`）的测试段**真的被剥掉**；自检必须**能检出「没剥掉」**（现有的 `prod.len() < raw.len()` 光靠剥注释就满足）；变异：把 marker 改回旧值必须红 |
| ③ | `parity_ledger::command_signatures` **递归** | `src-tauri/src/adapter/` 子目录里的 `#[tauri::command]` 能被数到；变异：在子目录放一个命令必须让计数变化 |
| ④ | `DAEMON_BUILD_ID != "unknown"` 断言 | 照 `ssh_source.rs:882-889 embedded_capabilities_single_source_wired` 的形状；变异：把 `build.rs` 的路径改错必须红 |
| ⑤ | 半 bump 修掉 | 源码 `BUILD_ID` 与 `embedded-daemons/*.build_id` 一致，**或** `build.rs` 从 `cargo:warning` 改成 fail |

**不做**：不改任何护栏的**判据强度**（只改扫描面与剥法）；不动 `readonly_guard` 的白名单层；
不重命名任何东西；不碰 `~/.cc-monitor/bin/` 那个在跑的 daemon。

## 与主计划对接（共享面）

- **S1 护栏的扫描面** —— 本功能做①③，`readonly_guard` 改钉 `observe/` 留给 U1b。
- **S2 守卫的测试段 marker** —— 本功能做②，是最终形态。
- **S8 `BUILD_ID` 单源链条** —— 本功能做④⑤，U13 重命名时依赖④。
- **S13 `parity_ledger`** —— 本功能做③。

## 逐条实现步骤

1. **先记基线**：跑 `cargo test` 拿到 daemon 与 monitor 两侧当前的绿/红状态与测试数。
   *验证*：有一手输出，不靠推测。
2. **②先做**（①③依赖它剥得对）：抽 `production_code(src)` 到一处，marker 改成
   「`#[cfg(test)]` 后面跟 `mod <任意标识符>`」的形态；加一条**能检出没剥掉**的自检
   （如：剥完的文本里不许再出现 `#[test]`）。
   *验证*：`main.rs` 剥完字节数明显下降；变异改回旧 marker → 红。
3. **①**：`daemon_sources` 换成 `readonly_guard::scan` 同款的目录栈；地板换成计数相等。
   *验证*：今天扫到 14 个文件；`mkdir src/tmp_probe && echo 'use std::time::Duration; fn x(){let _=Duration::from_secs(1);}' > …` 后必须红，删掉恢复绿。**变异后 `cp -a` 还原必须 `touch`。**
4. **③**：`command_signatures` 递归。
   *验证*：`LEDGER.len() == 123` 仍绿；在 `adapter/` 放一个临时命令必须让签名集变化。
5. **④**：加断言。
   *验证*：把 `build.rs:162` 的路径改错 → 红。
6. **⑤**：先看 `build.rs:262-266` 的 staleness 检查，改成 fail 或重新生成清单。
   **不 re-zigbuild**（要装 zig，撞「不装包」红线）⇒ **走「改成 fail + 在文档里写清本地手工打包前必须先 re-zigbuild」**。
7. **门禁全跑**：`cargo test`（两侧）+ `cargo fmt --check` + `npm test` + `tsc`。

## 实现期与计划的偏离（铁律 4：记下来，不默默改）

### 偏离①：第②步在实现中发现计划的修法**本身是错的**

计划写「marker 改成『`#[cfg(test)]` 后面跟 `mod <任意标识符>`』」。照做**会当场制造一个更严重的 bug**：

`main.rs` 的 `mod stream_flag_tests` 在 **182–247 行，是文件中段**，而真正的子命令 dispatch 在
**275–291 行，在它后面**。「砍掉第一个锚点之后的全部」这个**形状**本身就是错的 ——
放宽锚点只会让 `main.rs` 的生产段被砍到只剩前 182 行，`build_id_guard` 的指纹变成**空集**。

实测确认（python 预演）：
- 旧锚（不匹配 ⇒ 全文）指纹 = 9 个子命令，**但它们来自 `stream_flag_tests` 里那份副本**
- 只放宽锚点 ⇒ 指纹 = **空集**
- 逐个剥每个 `#[cfg(test)]` 模块 ⇒ 指纹 = 同样 9 个，**来自真 dispatch**

⇒ 实际实现的是**逐个剥**，不是放宽锚点。**两个 bug 一直在互相掩盖**：不剥（坑1）恰好让真
dispatch 留在「生产段」里，于是护栏一直绿 —— 绿得毫无道理。

### 偏离②：第⑥步 —— 不用装 zig 也能 re-build，所以做了计划说「不做」的那件事

计划写「**不 re-zigbuild**（要装 zig，撞不装包红线）⇒ 只改成 fail + 写文档」。
实现时发现 **`rust-lld` 是 rustc 自带的，零安装**：

```
cargo build --release --offline --target x86_64-unknown-linux-musl                       → RC=0
cargo build --release --offline --target aarch64-unknown-linux-musl \
      --config 'target.aarch64-unknown-linux-musl.linker="rust-lld"'                     → RC=0
```
（不加 `--config` 时 aarch64 会挂在系统 `ld.bfd` 上 —— 那正是仓里用 zigbuild 的原因。）

⇒ 两个 arch 都从 **p1v 源码现编**并装进 `embedded-daemons/`，`.build_id` 同步为 `p1v-attachable`。
**半 bump 是真修掉了，不是绕过去。**

⚠ **登记一个观察，不作为建议**：这说明 daemon 的 musl 构建**可能不需要 zig**。
但我只验证了「本机能链出可执行的 ELF」，**没有**验证它与 CI 的 zigbuild 产物等价
（glibc/musl 版本、strip、可复现性都没比）。CI 与发版仍走 zigbuild。要不要简化那条链，
是另一件事，登记待议。

⚠ x86_64 那份**无法用 `strings` 核 build_id**（0 命中）—— 正是 `build.rs:244-247`
注释里记的「编译器把 BUILD_ID 优化成立即数、字符串在字节里不连续」，`.build_id` 清单就是
为这个而设。aarch64 那份能 `strings` 到 `p1v-attachable`（1 命中，且 `p1u` 0 命中）。

### 偏离③：build.rs 的检查做成 **panic** 而不是 warning

mtime 警告漏掉了这次真事故（二进制**更新**但内容是旧版）。改成比 **build_id 字符串本身**，
且**必须 panic**：内嵌 p1u / 期望 p1v ⇒ 装上去被 monitor 永远判 `StaleBuild` ⇒ **无限重装**，
这不是「慢一点」，是坏的。顺带补了「缺二进制 / 缺 `.build_id` 清单」两条 warning
（原先**连 warn 都没有**，正是 v2.19–v2.22 那批安装包的事故形状）。

## 变异验证（全部退出码判定，`cp -a` 还原后均 `touch`）

| # | 变异 | 预期 | 实测 |
|---|---|---|---|
| 1 | `src/tmp_probe/probe.rs` 放一处 `Duration::from_secs` | 递归生效 ⇒ 红 | **RC=101**，**两条**一起红：`daemon_production_code_has_no_periodic_wakeups`（逐字报 `tmp_probe/probe.rs`，相对路径命名对）+ `every_duration_use_is_registered_as_non_timer`（`[("tmp_probe/probe.rs",1),("watcher.rs",1)]`，2≠1）。<br>〔**2026-08-01 补跑订正**：本行原写「诊断报出 `tmux_probe/probe.rs`」，与左列的 `tmp_probe` 对不上 —— 那是我转录时打错的，不是两个目录。名字不靠回忆定，直接重跑一次取实测；顺带发现原记录漏了第二条也会红。〕 |
| 2 | 把 `daemon_sources` 改回**非递归**（子目录仍在） | 子目录自检 ⇒ 红 | **RC=101**，报「遍历退化成非递归了」 |
| 3 | `guard_support` 的锚点退回 `mod tests` | 三条真守卫 + 四条回归钉 ⇒ 红 | **RC=101，7 条一起红**：`accounts_query::main_dispatches_every_subcommand_we_handle` · `build_id_guard::adding_a_subcommand_forces_a_build_id_bump` · `build_id_guard::the_comparison_actually_bites` + 4 条 `guard_support` 自测 |
| 4 | `build.rs` 的 daemon 源码路径改错 | `DAEMON_BUILD_ID` 退化成 `"unknown"` ⇒ 红 | **RC=101**，**只有** `embedded_build_id_single_source_wired` 红（capabilities 那条读同一文件但我没改它那处，故不受影响 —— 精确命中） |
| 5 | 半 bump 本身（不用造，当下就是） | build.rs panic | **RC=101**，逐字报出 `p1u-fork-session` vs `p1v-attachable` |
| 6 | **（Phase D 新增）**把 `wire.rs` 从扫描面里剔掉 | 数量相等判据 ⇒ 红 | **RC=101**：`扫到 14 个 .rs + 跳过自身 1 个 ≠ 树上的 16 个` |
| 7 | **（Phase D 新增）**藏掉 x86_64 的 `.build_id` 清单 | 缺清单硬门 ⇒ 红 | **RC=101**，`build.rs:302` panic，诊断里带可直接粘的 `printf` 补救命令 |
| 8 | **（Phase D 修复验证）**在 `main.rs` 被吞区间（26–179）放 `thread::sleep` | 修好后 ⇒ 红 | **RC=101**。修复前**同一段代码全绿**，放到测试模块之后才红 —— 这一对红绿就是「过剥」bug 的判定证据 |
| 9 | **（Phase E 修复验证）**在 `readonly_guard` 原先看不见的 `main.rs:23–40` 区间放 `std::fs::write` 探针 | 修好后 ⇒ 红 | **RC=101**（`daemon_write_capability_is_confined_to_one_module`）。**同一段代码在旧剥法下生产段里查无此串 = 假绿**（不改代码、用逐字复刻的模拟证的）。红绿相反 ⇒ 判定成立 |

## 门禁结果

| 项 | 基线 | 现在 |
|---|---|---|
| daemon `cargo test` | 186 passed | **194 passed**（+8 = `guard_support` 自测 4 条 + Phase D 补的 3 条回归钉 + Phase E 补的 `no_test_code_leaks_into_any_production_section`），RC=0 |
| monitor `cargo test --lib` | 663 编译出 | **661 passed / 3 ignored**（+1 = 新的 build_id 断言），RC=0 |
| `cargo fmt --check`（两侧） | — | OK |
| `tsc --noEmit` | — | RC=0 |
| `npm test` | — | 79 文件 / 1148 测，RC=0 |

## 代码审计结果（Phase D，高风险档 ⇒ 全视角并行 + 两轮）

### 阻塞（1 项，已修）

**B1 · 我自己在这一轮制造的护栏盲区 —— 两份审计各自独立逮到。**

第②步为了共用剥法，我往 `main.rs` 加了

```rust
#[cfg(test)]
mod guard_support;
```

这是**无花括号体的模块声明**。新剥法的锚点 `\n#[cfg(test)]\nmod ` 照样匹配，然后去找收尾的列 0 `}`
—— 一路找到 179 行 `fn split_stream_flags` 的收尾，**把 `main.rs:26–179` 整段当测试段吞掉**：
`const BUILD_ID`、`CAPABILITIES`、`EMITS`、全部 mod 声明、整个 `split_stream_flags` 全没了。
`no_timer_guard` 在那一段**静默变瞎**。

判定证据（变异 #8）：同一段 `thread::sleep` 放进被吞区间 ⇒ **全绿**；放到测试模块之后 ⇒ RC=101。

**修法**：匹配到锚点后**必须确认那一行以 `{` 收尾**，否则判定为模块声明、原样保留并从下一行续扫。
加三条回归钉：`a_bodyless_cfg_test_mod_declaration_swallows_nothing` ·
`main_production_section_keeps_its_load_bearing_items` · `strips_every_test_module_not_just_the_first`。

> **这条要单独记住**：我**修一个护栏盲区的动作，当场制造了同一类的新盲区**，
> 而且新盲区比原来的更大（原来漏 275–291 共 17 行，新的漏 26–179 共 154 行）。
> 已写进 `doc/INVARIANTS.md` §41.4 第 3 条派生纪律。

### 重要（4 项，全部已修）

| # | 发现 | 处置 |
|---|---|---|
| I1 | 地板只有「字节下限」挡不住**单文件被剥空**：最大的 `watcher.rs`（34_506 B，占 29%）整个消失后总量仍有 84_948 ≥ 80_000、照样绿 | **两条判据都上**：字节地板 + 「扫到数 + 跳过自身 1 == 独立目录遍历数」。变异 #6 验证 |
| I2 | 子目录自检可以被「空子目录」骗过 | 自检改成要求那个子目录**里面真有 `.rs`** |
| I3 | 「缺 `.build_id` 清单」原做成 warning —— 那正是本轮硬门的静默旁路，且是最危险场景（有人手工塞了陈旧二进制没写清单）恰好绕开 | 升成 panic。变异 #7 验证 |
| I4 | **本功能零文档改动，违反主计划 §4.4 硬门槛⑧** | 已补 5 处，见下「文档缺口」 |

### 我自己造的第二个坑（不是审计报的，是门禁打出来的）

B1 的回归钉，我第一版夹具用了**带真实换行的多行字符串字面量**，里面有列 0 的 `}`
⇒ 当场把 `every_daemon_file_strips_clean` 打红（漏 4 个 `#[test]`）。改成 `\n` 转义的单行形式解决。
**顺带证明了那条自检不是安慰剂** —— 它真的会咬。

### 建议（登记，不在本功能做）

- `history_query.rs:232/309` 有 `#[cfg(test)]` **自由函数**（非模块），任何剥法都不会剥、也检不出来。
- 共享剥法目前只收敛了 8 处中的 3 处。
- 三条新递归都**跟随符号链接**（本仓 `src/` 下无符号链接，但没设防）。
- aarch64 那份内嵌二进制**从未被执行过**（本机无 qemu），只做过字节级核对。
- 建议 `.build_id` 里连 sha256 一起存，身份判定就不只靠一个人写对字符串。

## 实现期与计划的偏离（续）

### 偏离④：地板判据从「数量相等」改成「字节下限」，Phase D 后**两条都上**

DoD ① 原文写的是「地板从魔数 `5` 换成**扫到的 .rs 数 == 目录树里的 .rs 数**」。
实现时我换成了**字节下限**（理由：数量判据对「拆分」不友好，而本工作区 U2/U3 接下来做的正是拆分；
字节数直接度量「扫到的是不是真代码」且对拆分免疫）—— **但当时没登记这个偏离，是漏记，铁律 4 的违反。**

Phase D 审计 I1 证明了字节地板单独用有洞（`watcher.rs` 整个消失仍绿）。
最终形态是**两条判据并存**：字节下限挡「代码搬进子目录只剩壳」，数量相等挡「单个文件被剥空/漏采」。
⇒ DoD ① 原来要的那条**也在**，只是不再是唯一那条。

## 文档缺口（Phase D I4 的处置，已全部落地）

| 文件 | 改了什么 |
|---|---|
| `doc/RELEASING.md:22` | 原说「build.rs 有 staleness **warning** 兜底」—— 现已是三种 panic；补上「重编 + **同步 `.build_id` 清单**」与两条出路 |
| `doc/REMOTE-PHASE0-DEPLOY.md:15-40` | 手工打包**必须写 `.build_id` 清单**（缺清单 = 编译期 panic，原文完全没提清单这回事）；补 `rust-lld` 零安装路线与它**不等价于 CI 产物**的限定；三种 panic 列表 |
| `doc/CONTRIBUTING.md:82` | 「内嵌二进制一致」这条现在是**机器强制**，不再靠自觉 |
| `doc/INVARIANTS.md:1188` | 「扫**全 crate** 生产段」这句**在本次改动前是假的**（非递归 + 剥法过剥），加订正标注 |
| `doc/INVARIANTS.md §41.4` | 「两条派生纪律」→**三条**，新增「剥法必须同时防欠剥与过剥」，含 B1 的完整病历与「同一个坑 `readonly_guard` 早填过、另外三处没跟」这条元教训 |

## 工程审计结果（Phase E：主线程对账 + 1 个聚焦 agent）

### 先说被点名的那条潜在阻塞：**不成立，发版不会被打挂**

`build.rs` 升 panic 后，`release.yml` 的 `build-daemons` **确实写 `.build_id` 清单**
（`:56` 用 `grep -oP 'const BUILD_ID: &str = "\K[^"]+'` 从源码抠、`:57-58` 写两份、`:62-67` 的
upload glob 前缀匹配把清单一起带走），`build-windows` `:108-118` 还会再拷一次 + 缺清单 `exit 1` +
与源码对拍，`build-linux` `:223-234` 同款。**三条 panic 在发版链上够不着。**
CI 侧同理：`embedded-daemons/` 只由 release.yml 现场创建，全仓无第二处写入点 ⇒ 干净 checkout 走
`build.rs:327-336` 的 warning 分支，且 CI 没有把 `cargo:warning` 变红的设置。

### 阻塞（0 项）

### 重要（5 项，全部当轮修掉）

| # | 发现 | 我的核实 | 处置 |
|---|---|---|---|
| R1/R2 | **`readonly_guard::strip_cfg_test` 有两条过剥（fail-open）**，其中一条是我这轮撑大的 | **属实，且比审计说的更要紧。** 审计说「只有 `guard_support.rs` 一个文件残留 5 个 `#[test]`、其余 15 个为 0」——**这半句不成立**：HEAD 就有 4 个文件残留（`watcher.rs` 76 / `accounts_query.rs` 25 / `resolve_query.rs` 10 / `history_query.rs` 4），我的文件只是第五个、且最小。但**过剥那半是真的**：`main.rs:23` 行尾注释里逐字写着 `#[cfg(test)]`，裸 `find` 起跳后括号配平吃到 `:40` 的 `use tokio::io::{…}` ⇒ **`main.rs:23–40`（15 条 `mod` + 2 条 `use`）从来不在扫描面里**；而我加的 `#[cfg(test)] mod guard_support;` 是**无花括号体声明**，同一个 bug 又让它多吞两行 | 两条规则一起上：**锚点钉行首** + **声明（先遇 `;`）不吃后文**。扫描面 **217_853 → 221_928**（+4_075，是**扩大**不是收窄）。另加 `no_test_code_leaks_into_any_production_section` 把欠剥方向也钉成机器判据 |
| R3 | 三份文档都说「抠不到源码 `const BUILD_ID` ⇒ panic」，实际只在**有内嵌二进制**时才触发 | 属实：整段在 `build.rs:266` 的 `if src.exists()` 里。干净 clone / CI 的常态恰恰是没有那个目录 ⇒ `DAEMON_BUILD_ID` 静默变 `"unknown"` | 三份文档都补上前提，并指明兜那一档的是 `ssh_source.rs::embedded_build_id_single_source_wired` **不是** `build.rs` |
| R4 | `REGISTERED_DURATION_USES` 用裸文件名，而 `daemon_sources()` 已改成返回相对路径 ⇒ **U2/U3 一搬家就必红** | 属实（`n == *file` 全等） | 改成「相对路径全等 **或** 以 `/文件名` 结尾」。纯搬家不再触碰红线表；真删掉代码仍会红 |
| R5 | `parity_ledger::command_signatures()` 的递归**没排序**，而插入是「首个胜」 | 属实。今天只有一层平目录 + 空的 `adapter/`，影响为零；但多子目录出现同名命令时结果随文件系统顺序漂 | 加 `files.sort();` |
| — | **字节基线我抄错了** | 审计算 121_131、我注释里写 119_454。**用护栏自己的口径实测：121_131，审计对。** 连带「移除 `watcher.rs` 后 84_948」也算错（应为 86_625） | 订正，并把**复测办法**写进注释（临时把常量改大跑一次，失败信息里恒打印实时值），不再手抄 |

### 建议（已采纳 2 条）

- `files.len() + 1` 里的 `1` 与「跳过自身」隔空耦合 ⇒ 抽成 `SKIPPED_BY_NAME` 表，U1b 加护栏时只改表。
- `build.rs` 的补救命令没说要在 `remote-daemon-proto/` 下跑、也没带拷贝与写清单两步 ⇒ 补全三步。
- 文档里给用户的 `cargo build` 带了 `--offline`（那是我实现期的本机约束）⇒ 去掉；
  硬编码的 `"p1v-attachable"` ⇒ 改成从 `main.rs` 抠（同 `release.yml:113` 的写法）。
- `.claude/planned-build/README.md` 的本区索引仍写着**被否决的 v1 形态**（「决策内核 crate + 三宿主」）
  和「6 个待拍板」⇒ 已改写。

### 登记待办（不在 U-1 范围）

- **遍历一处都没收敛**：daemon crate 里 5 份独立目录遍历 + monitor 侧 1 份。
  ⚠ 收敛时**必须保留 `count_rs_in_tree()` 的独立性** —— 它与 `daemon_sources()` 分开写正是那条
  「数量相等」判据的全部价值，合掉就变恒真。
- **`readonly_guard` 与 `guard_support` 的剥法仍是两套，且各有对方没有的能力**：前者能剥
  `#[cfg(test)]` **自由函数**（`history_query.rs:232/309`，实测两者对该文件差 1246 字节），
  后者对「注释里的属性」免疫。收敛需要做**并集剥法**，naive 换过去会让 `readonly_guard` 变弱。
- 老剥法还剩 5 处是「第一个锚点之后全砍」的形状，今天全对但 U2/U3 拆文件即坏：
  `tmux_hook.rs:166` · `watcher.rs:2133/2162/2238` · `local_accounts.rs:468`
  （另 `ssh_source.rs:3190` 是 monitor 侧第二份 `strip_cfg_test` 副本）。
- monitor 侧 4 处硬编码 daemon 源路径，U2/U3 搬家会断：`local_accounts.rs:451-456` ·
  `tmux.rs:608/936` · `ssh_source.rs:3153`。都是**响的**（编译错/测试红），故 U-1 只给静默的
  `build.rs` 那条加断言是选对了目标；建议 U2 开工前把这 4 处 + `build.rs` 的 3 处收进一个常量。
- `has_rs_subdir` 那条回归钉**当下处于休眠**（`src/` 还是平的），U2 建出 `platform/` 后自动上岗。

## 签收

- [x] 过代码审计（D，两轮：一轮三视角 + 一轮产物审计）——阻塞 1 项 + 重要 4 项，全部修完复验
- [x] 过工程审计（E，主线程对账 + 1 个聚焦 agent）——阻塞 0 项 + 重要 5 项，全部修完复验；
      其中 R1 的「唯一/首个」框架经我核实**不成立**，已在上表逐字订正
- [x] 主计划已更新（F）——账本 S1/S2/S8/S13 状态落账，§7 变更记录追加两条
