# U8c-2b-0 — 账本 S5 收口：Rust POSIX quote 合一（+ 落档 container:none 那一刀为什么切不动）

> ⚠ **是五份不是四份** —— 计划里写的四份是我摸底数的、账本 S5 记的也是四份；
> 第五份 `acct_iso_deploy.rs::sq` 是守卫第一次跑当场抓到的（见 D 段）。

- 工作区：unified-backend · 第六梯队 · 任务 #96
- 前置：U8c-1（`launch-core` 已建）· U8c-2a（它已有生产消费方）
- 本件性质：**收口 + 落档**。零功能变化、零字节变化。

## 摸底：`container:"none"` 那一刀今天切不动（铁律 4，本轮第一件事）

本轮原定「远端 `container:"none"` 那条路改发结构化请求」。**它是真生产路径**（不是死路）：
`tabs.ts:2025` 的 Tab resume 与 `fork-flow.ts:195`（无 tmuxName 时）都走 `runRemoteResume`。
**但那不是能不能切的关键。**

关键在 `remote-launch-run.ts::renderLaunchCommand` 的分流：

```ts
const r = tryRenderCli(plan, ctx, probe);   // 装了 ccm ⇒ 走这条，产出 `ccm …` 调用行
if (r.ok) return r.cmd;
…
return renderFallback(plan);                // 没装 ccm 才走这条，产出裸载荷
```

⇒ **装了 ccm 的机器（本仓用户就是）走的是 CLI 形态，`renderFallback` 根本不执行。** 于是：

| 切法 | 后果 |
|---|---|
| 只把 `renderFallback` 的 none 分支搬进 Rust | **搬的是一条在目标机器上不跑的路** —— 看着像进展，实际零收益 |
| 连 CLI 形态一起搬 | 那要把 `ccm …` 调用行也在 Rust 里渲染 ⇒ **整个维度注册表**（`launch-dimensions.ts` 173 行 + `requiredCaps` + `cliFlags` + ccm 探测）= **U8c-2c 的全部内容** |

⇒ **改计划**：`container:"none"` 这一刀**不是最小刀，是 U8c-2c 的一部分**。本轮换成账本
**S5 点名、且 U8c-1 自己造出来的那笔债** —— 它恰好是 U8c-2c 的前置。

## 为什么是 S5

`MASTERPLAN` 的 S5 逐字写着（U8c-1 自己加的订正）：

> ⚠ **U8c-1 订正**：「quote 收不了」这条前提**不再成立** —— 共享 crate 家族本身就是那个载体，
> 而 U8c-1 在 `launch-core` 里加了**第三份** monitor 侧 POSIX quote…
> **要么收进 `launch-core` 并让三处调它，要么开一条对拍。今天两样都没有。**

摸底当时数出**四份**、实现逐字节相同（`format!("'{}'", s.replace('\'', r"'\''"))`）——
**守卫跑起来是五份**（多一个 `acct_iso_deploy.rs::sq`，见 D 段）：

| # | 位置 | 谁在用 |
|---|---|---|
| 1 | `src-tauri/src/launch.rs::posix_quote` | 本机 POSIX 拉起 |
| 2 | `src-tauri/src/ssh_source.rs::shell_quote`（`pub`） | 远端命令拼装，全仓多处 |
| 3 | `remote-daemon-proto/src/control/tmux_hook.rs::sq` | daemon 装 tmux hook |
| 4 | `src-tauri/crates/launch-core::posix_quote` | **U8c-1 新加的那份** |
| **5** | **`src-tauri/src/acct_iso_deploy.rs::sq`** | **摸底漏了 —— 守卫抓到的** |

**U8c-2c 会让第 4 份变成所有载荷的唯一出口** —— 在那之前把另外三份收掉，
比之后再收便宜（那时调用面更大）。

## DoD

1. 三份改成**委托**给 `launch_core::posix_quote`（保留各自的名字与可见性，调用点零改）。
   daemon 侧因此第一次依赖 `launch-core` —— 那本来就是 U8c-2 计划里的一步。
2. **一条零命中守卫**：Rust 生产段里不许再出现第二个「`'…'` 包裹 + `'\''` 逃逸」实现。
   配抽取器自检（扫不到文件 ⇒ 红，不是零命中零失败地绿）。
3. **字节零变化**：各处既有测试全绿；新增「monitor 侧三个入口对同一输入产出相同」的对拍。
4. 顺带兑现上一轮登记：`arg_is_join_safe` 的**非 ASCII 过严**边界 —— 错误文案要说实话。

### 不做什么

- **不动 TS `posixQuote` 与 `shared/ccm::sq`**（跨语言那两份由黄金串夹具对拍；ccm 那份是 S10 的
  刻意副本）。
- **不切 `container:"none"`**（见摸底 ⇒ U8c-2c）。
- **不动 `ps_quote`**（PowerShell 是另一套转义规则，不是副本）。

## 逐条步骤

| # | 做什么 | 怎么验证 |
|---|---|---|
| 1 | daemon 加 `launch-core` 依赖；`tmux_hook::sq` 改委托 | daemon 237 全绿 + 跨 target Windows check |
| 2 | `launch.rs::posix_quote` / `ssh_source::shell_quote` 改委托 | monitor 全绿 |
| 3 | 四入口对拍测试 | 变异：把某一份改回自己实现且行为不同 ⇒ 红 |
| 4 | 零命中守卫 + 抽取器自检 | 变异：新写一份 quote ⇒ 红；扫不到文件 ⇒ 红 |
| 5 | `arg_is_join_safe` 文案 | 人读 |
| 6 | 全量门禁 + e2e | 逐套对数 |

## 代码审计结果（D）

本件改动面小（五处一行委托 + 一条守卫），D 由**我自己逐条变异复验**完成 ——
而它当场抓到了**三件事**，其中两件是我自己写的判据不合格。

### ★ 守卫第一次跑就抓到「第五份」

我摸底数出四份，账本 S5 记的也是四份。守卫一跑，命中
**`src-tauri/src/acct_iso_deploy.rs::sq`** —— 逐字节相同的第五份。

⇒ **「到底有几份」这件事，人数的和机器数的不一样。** 这正是「不做成机检就等于没有」的
最直接证据：五份从来没红过，是靠巧合保持一致的。

### ★ 我自己的两条判据不合格（变异复验抓到，已修）

| 变异 | 初版结果 | 病因 | 修复后 |
|---|---|---|---|
| **M2** 把唯一的家里的实现挖空（`push_str("XX")`） | **存活** | 反向自检写了个宽松的第三备选（裸逃逸子串），而 `out.push('\'')` 那个**字符**字面量含同样的字节 ⇒ 恒真 | **红** |
| **M3** `launch.rs::posix_quote` 换成不逃逸的实现 | **存活** | 行为对拍只比了 `ssh_source::shell_quote` **一个**入口；零命中守卫也挡不住它（不含逃逸序列就不命中） | **红**（三入口逐个对拍） |

**M2 那条尤其要记**：反向自检的作用是「防止『只有一个家』退化成『一个都没有』」，
而我把它写成了恒真 —— **一条本身恒绿的自检，比没有更坏**（它让人以为那一层有人守着）。

### 变异复验总表

| 变异 | 结果 |
|---|---|
| M1 有人又复制一份 quote 出来 | **红**（逐字报出命中文件） |
| M2 唯一的家里把实现挖空 | **红**（修复后） |
| M3 某个入口换成不逃逸的实现 | **红**（修复后，报出是哪个入口 + 哪个输入） |
| 抽取器扫不到文件 | **红**（`>= 40` 自检） |

### 诚实边界（写在守卫头注里）

- 它查的是**逃逸序列的源码形态**，查不了「换个写法的等价实现」（手写 char 循环而不用
  `replace`）—— 与本仓其它约定型守卫同一档。**比没有强，别读成证明。**
- daemon 的 `tmux_hook::sq` 跨 crate 够不着行为对拍，只被零命中守卫覆盖（它扫 `remote-daemon-proto/src`）。

## 工程审计结果（E）

- **本件是 U8c-2c 的前置**：U8c-2c 会让 `launch-core` 成为所有载荷的唯一出口，
  在那之前把 quote 收掉，比之后再收便宜（那时调用面更大）。
- **账本 S5 收口**：那条「要么收进 `launch-core`、要么开一条对拍，今天两样都没有」是
  U8c-1 自己写下的债，本件还清 —— 并把「五处不是四处」写进账本（人数错了一次，别留着）。
- **daemon 第一次依赖 `launch-core`** —— 那本来就在 U8c-2 的计划里。跨 target Windows check
  与 237 条 daemon 测试都绿。
- **零功能变化、零字节变化**：五处都是一行委托，实现逐字节等价（行为对拍是判据）。
- ⚠ **本轮没做 `container:"none"` 那一刀**，摸底理由写在开头：装了 ccm 的机器走 CLI 渲染器，
  只搬 `renderFallback` 等于搬一条不跑的路；搬 CLI 形态 = 整个维度注册表 = **U8c-2c**。
- **上一轮登记的 `arg_is_join_safe` 非 ASCII 过严**：DoD #4 兑现了**其中的文案那一半** ——
  错误信息现在逐字说出放行集、并点名「非 ASCII 也一律拒，已知过严且与
  `config_dir_command_safe` 放行中文不对称」。⚠ **放行集本身没改**（今天零生产流量，
  改它要连 U8c-2c 的 args 真实内容一起定）。**那一半仍然登记在案。**

## 签收

- [x] 过代码审计（D）—— 守卫第一次跑就抓到**第五份**；我自己的两条判据被变异复验打回重写
- [x] 过工程审计（E）—— 账本 S5 收口（并订正「五处不是四处」）；daemon 首次依赖 launch-core
- [x] 主计划已更新（F）
