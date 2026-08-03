# U8c-2c-1 — CLI 渲染器（`ccm …` 调用行）进 `launch-core` + 跨语言黄金串对拍

- 工作区：unified-backend · 第六梯队 · 任务 #96
- 前置：U8c-1（`launch-core` + 入库夹具机制）· U8c-2b-0（quote 已收口）
- 本件性质：**抽内核 + 跨语言对拍**（照搬 U8c-1 已验证的形态）。**不切生产、不删任何 TS。**

## 摸底：U8c-2c 是什么、怎么拆

上一轮判定「`container:"none"` 那一刀属于 U8c-2c」，理由是**装了 ccm 的机器走的是 CLI 形态**。
本轮把 U8c-2c 本身量清楚：

| 面 | 规模 | 说明 |
|---|---|---|
| `launch-dimensions.ts` | 173 行 / **5 个维度** | 每个维度三个钩子：`applies` · `apply`（产 `EnvOp`）· `cliFlags`（产 ccm flag）+ 可选 `requiredCaps` |
| `launch-render-cli.ts::tryRenderCli` | **~50 行逻辑** | 能力闸 → #76 防线 → attach 早返回 → 动作/容器 token → 维度循环 → cwd/launcher/args |
| `ccm-probe.ts` / `ccm_probe.rs` | 探测面 | Rust 侧**已有**解析器；TS 侧是它的消费者 |

### 关键观察：`apply` 那一半**已经搬完了**

维度的 `apply` 产出的是 `EnvOp[]`，而 `EnvOp → 载荷` U8c-1 已经在 Rust 里了
（`launch_core::render_payload`）。**没搬的是 `cliFlags` 那一半** —— 也就是 ccm 调用行。
⇒ U8c-2c 的真正内容是「**ctx → `ccm …` argv**」，不是「整个注册表」。

### 拆分

| 件 | 内容 | 状态 |
|---|---|---|
| **U8c-2c-1（本件）** | `tryRenderCli` 的 Rust 实现进 `launch-core` + 跨语言逐字节对拍。**不切生产** | 本轮 |
| **U8c-2c-2** | 生产切换：`remote-launch-run.ts` 改调 Rust（需新 tauri 命令 + IR 上线形状） | 待做 |
| **U8c-3** | 删 TS 两个渲染器 + IR + `session-behavior`；收敛 §33/§35/§37/§38；重指 S19 第五处 | 待做 |

**为什么先做内核后切生产**：U8c-1 已经验证过这条路 —— 反过来做等于**在没有逐字节判据的
情况下动生产渲染路径**，而这个仓为此付过价（`launch-render-fallback.ts` 头注记着
「早期实现曾把 cd 错放到最前面，逐字节对拍时抓到」）。

## DoD

1. `launch-core` 新增 `render_ccm_invocation(spec, caps) -> Result<String, Refusal>`，
   与 TS `tryRenderCli` **同构**：
   - **`Refusal` 是一等返回值不是 `Option`** —— TS 那边 `{ok:false, reason}` 的 `reason`
     是生产侧唯一的降级线索（`console.debug`），丢掉它等于把 §33 的「诚实降级」降级成「静默降级」；
   - **`cliFlags` 返回 `null` ⇒ 整条放弃**（§35 的 `null` 安全网）—— 在 Rust 里用
     `Option<Vec<String>>` 表达，`None` 直接短路。
2. **跨语言逐字节对拍**：扩展 U8c-1 的入库夹具机制（TS 生成 → 入库 → 两侧各自与它比），
   覆盖 **ok 与 refusal 两类**（只比 ok 的话，「该降级却渲染出来了」这一类抓不到）。
3. **不切生产**：`remote-launch-run.ts` 一个字节不动。
4. **§33/§35/§37/§38 逐条判定**本件动没动、后两件会怎么动 —— 写进 `doc/INVARIANTS.md` §33b 的表。

### 不做什么

- **不切生产**（U8c-2c-2）· **不删任何 TS**（U8c-3）· **不动 `session-backend.ts`**。
- **不搬 `apply`/`EnvOp` 那一半**（U8c-1 已经做完）。
- **不动 ccm 探测**（Rust 侧已有解析器，本件只消费「能力集合」这个入参）。

## 逐条步骤

| # | 做什么 | 怎么验证 |
|---|---|---|
| 1 | `launch-core`：`CliSpec` / `Refusal` / `render_ccm_invocation` | 单测覆盖 attach 早返回 · #76 防线 · 能力闸 · `null` 安全网 |
| 2 | 五个维度的 `cliFlags`/`requiredCaps`/`applies` 在 Rust 里实现 | 单测逐维度 |
| 3 | 夹具扩展：TS 生成 ok/refusal 两类用例 | `npm run gen:payload-golden` 幂等 |
| 4 | Rust 侧读同一夹具对拍 + 计数自检 | 变异：改任一侧 ⇒ 红在正确的一侧 |
| 5 | §33b 表更新 | 人读 |
| 6 | 全量门禁 + 17 套 e2e | 逐套对数 |

## 代码审计结果（D）

D 由**我自己逐条变异复验**完成（上一轮的教训：我写的判据初版两条都不合格，
所以这轮每条都先问「它能不能红」再往下走）。

### ★ 上一轮写的守卫，这一轮就咬到我自己

`quote_singleton_guard`（U8c-2b-0 建的「POSIX quote 只许一个实现」）在本轮
`cargo test` 里**当场变红** —— 我新写的 `cli.rs::argv` 又 `format!` 了一份逃逸，
成了**第六份**副本。改成调 `crate::posix_quote` 后转绿。

⇒ 这是那条守卫第一次在**真新代码**（不是人造变异）上生效。上一轮写它的理由
「下一个要 quote 的人复制一份出来同样不会红」—— **下一个就是我，隔了一轮**。

### ★ 我自己知道的一处偏离，写代码时就修了（没等夹具抓）

初版把「能力检查」整体提到维度循环**外面**，而 TS 是**逐维度交错**的
（`for (const dim …) { 查它要的 cap; 问它的 flags }`）。两者在「缺能力」与「说不出」
**同时成立**时会给出**不同的 reason** —— 而 reason 是生产侧唯一的降级线索
（`remote-launch-run.ts` 用 `console.debug` 打它），换一个就是换一条诊断。
已改成与 TS 同构的维度表遍历，并把这句写进代码注释。

### 变异复验总表

| 变异 | 结果 |
|---|---|
| M1 §35 安全网拆掉（`cliFlags` 说不出也照渲染） | **红**（1/16 不一致） |
| M2 #76 防线拆掉（`send-into` 也渲染） | **红** |
| M3 attach 分支不再早返回（开始读修饰） | **红** |
| M4 改 TS 渲染器**不重生成**夹具 | **TS 红 / Rust 绿** —— 正是「静默分家」该被抓在的那一侧 |
| （非人造）`cli.rs::argv` 自己写了份 quote | **红** —— 上一轮的守卫抓的 |

### 首跑就全对，这件事本身要说清楚

16 条（9 ok + 7 refusal）**首跑零不一致**，包括六种降级理由的**逐字文案**。
这不是运气：Rust 实现是照着 TS 逐行写的。**夹具的价值不在今天对上，在以后不许分家** ——
M4 证明了这一点（改一侧不重生成，红在正确的那一侧）。

### 诚实边界

- **`args` 那一格夹具没覆盖**：`plan.args` 生产零producer，`CliSpec.args` 恒空。
  U8c-2c-2 若让它有内容，要同步加用例（`arg_is_join_safe` 的非 ASCII 过严也在那时才撞上）。
- **`ccm_path` 恒 `"ccm"`**：TS 那边是默认参数，生产没有别的取值。
- **探测面没搬**：本件只消费「能力集合」这个入参，`ccm-probe` 两侧照旧。

## 工程审计结果（E）

- **摸底改写了 U8c-2c 的定义**：它不是「整个维度注册表」—— 维度的 `apply`（产 `EnvOp`）
  那一半 U8c-1 已经搬完了，**没搬的只有 `cliFlags`**。所以本件的范围是「ctx → ccm argv」，
  比计划里写的小得多。
- **§35 / §37 两条不变量已在 Rust 侧兑现**，`doc/INVARIANTS.md` §33b 的判定表逐行更新
  （连同「能力检查必须逐维度交错」的理由）。§33/§38 仍未动，留 U8c-3。
- **不切生产是刻意的**：U8c-1 已经验证过「先建判据、后切路径」这条路；反过来做等于
  在没有逐字节判据的情况下动生产渲染路径，而这个仓为此付过价。
- 账本：新增行留给 U8c-2c-2（切换时才产生新的共享面）；本件没有引入新的双写点 ——
  `argv` 那次差点引入，被守卫挡下。

## 签收

- [x] 过代码审计（D）—— 四条变异逐个转红；**上一轮的 quote 守卫在真新代码上咬到我自己**
- [x] 过工程审计（E）—— 摸底把 U8c-2c 的范围缩小到「`cliFlags` 那一半」；§35/§37 已在 Rust 兑现
- [x] 主计划已更新（F）
