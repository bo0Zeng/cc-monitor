# UB-复盘1 — 生产层补判据（三视角复盘的 P0）+ §1.4b 裁决落档

- 工作区：unified-backend · 复盘产物（不在 U 序列里，是给 U 序列补网）
- 触发：用户 2026-08-03「开多 agent 复盘一下之前的实现」

## 一、三视角复盘的结论（三个 agent 独立跑）

| 视角 | 最要紧的一条 |
|---|---|
| 架构漂移 | **方向在偏移**：主计划要「控制从 monitor 移到 daemon」，九个 commit 完成的是「渲染从 TS 移到 monitor 的 Rust」。中间产物几乎一样、终点不同；**今天后者全绿、前者仍是零** |
| 微架构整洁度 | 零活 bug，但 **5 处注释与代码不符**（其中 `quote_singleton_guard` 的失败文案把它自己头注专门订正过的错数字写了回去）· wire 类型 5–7 份副本 · TS 侧 4 份 posix quote **零守卫**（而 TS 才是生成黄金字节的那侧） |
| 判据体系 | **46 个变异存活 30 个。** 被钉死的是纯函数+夹具那层；**唯一真正改变生产行为的那层判据是零**，那层 12 个变异全部存活 |

## 二、§1.4b 裁决（用户选 A）

架构审计的诊断：`launch-core` 之所以必须存在，是因为 **monitor 侧根本没有 `backend/` 边界**
（54 个平铺 `.rs`）—— 「因为没有第二个地方，才造了第三个地方」。用户选 **A**：

1. monitor 划 `src-tauri/src/backend/{control,observe}/`，渲染与 wire 适配层迁进去；
2. `launch-core` 缩回真两侧共用的三个原语（`posix_quote` / `config_dir_command_safe` /
   `UNSET_CONFIG_DIR_PREFIX`），**不再持有决策**；
3. ⚠ 这一层与 §1.2 **不许互相顶账**（抽 crate ≠ 一份代码两种生命周期；同理抽
   `usage-core`/`acct-core` ≠ S9 的读面合流）。

已写进 `MASTERPLAN.md` §1.4b。**排期：在本件（P0 补网）之后做** —— 它要搬的正是那批零判据的代码。

## 三、本件做的：P0 —— 一个机制杀掉 12 个存活变异

**病**：`render_ccm_launch` / `render_launch_payload` 这两个 tauri 命令**本体零调用零判据**。
旧对拍是「Rust 侧自己重搭一个 `CliSpec`/`PayloadSpec`」，所以 wire 映射那 45 行从没被验过。
审计实测存活的包括：`send_into` 恒 false（#76 防线抹掉）· 具名账号降成 `Base`
（`--account z` 悄悄变 `--base`，正是 F05 那个形态）· 丢 `cwd` · 丢 `model` · 清空 `nested_env` ·
**TS 把嵌套字段 `send_into` 写成 `sendInto` ⇒ 每次 tmux 拉起静默回退 TS 兜底**。

**药**：
1. 把请求构造从 `renderCliViaBackend` 抽成导出的 `buildCliRenderRequest` /
   `buildPayloadRenderRequest` —— **夹具与生产用同一份代码产 `req`**；
2. 两份夹具的每条用例带上那个 `req`（入库）；
3. Rust 侧改成**用生产 wire 类型反序列化 + 调生产命令**，与 TS 渲染器的产物逐字节比。

⇒ 一次覆盖四件事：字段名对不对（反序列化会失败）· `deny_unknown_fields` 在不在 ·
映射臂对不对 · **请求构造漏没漏字段**。而且是**行为对拍不是文本对拍**。

### ★ 新判据第一次跑就抓到一个真的：`wrap` 被生产 wire 静默丢掉

两个审计各自独立点名过这条（「内核为未来功能建好了，wire 却把它挡在门外 ——
将来接上时不会有任何东西红」）。`launch_cli_cmd.rs` 硬写 `wrap: &[]`，而 §39 明写
`WrapSpec` 是给 F04 rbind 留的槽。**旧对拍看不见它**（它自己重搭 spec 时把 wrap 填了）；
新对拍第一跑就红：夹具那条 wrap 折叠用例 TS 产物带包裹、Rust 产物没有。

⇒ 按审计建议**在 wire 上补齐**（三侧各两个字段），不是让它继续静默。

### 顺带删掉的重复（微架构审计点名）

`launch_cli_parity.rs` 的 `Ctx` / `FxAction` / `FxContainer` / `FxAccount` 四个手写镜像
（`FxAction` 与 `WireAction` 逐字相同、连映射 match 都是复制的）—— 改用生产类型后全成死代码，删。
净效果：**少四个类型、少一份 match，对拍从「我重搭一个 spec」升级成「跑生产命令」**。

## 四、变异复验

| 变异（审计实测**存活**） | 现在 |
|---|---|
| M7a `send_into` 恒 false | **红** |
| M7b 具名账号降成 `Base` | **红** |
| M7c 丢 `cwd` | **红** |
| M7e 清空 `nested_env` | **红** |
| （非人造）`wrap` 静默丢 | **红** —— 第一跑就抓到，已修 |

⚠ **过程中我自己造了一次假绿**：`cargo test --quiet "a\|b"` 的过滤器不是正则，
输出「0 passed / 741 filtered out」被我一眼读成绿。**「0 passed」是「一个都没跑」。**
换成 `_parity` 后基线才显出 9 passed，四个变异才真的各红一条。
—— 这与上一轮 `grep | head` 造假证据是同一类：**读数之前先确认那个数在数什么。**

## 五、还没做的（如实登记，按复盘的优先级）

- **P1 `launch-core/src/cli.rs` 补自己的单测**（今天 0 条）。审计最担心的一条：
  今天挡「两侧一起错」的是 TS 的 `launch-render-cli.test.ts`，而它**排期在 U8c-3 删除** ——
  那天一到，`cli-golden.json` 变成没有生成者的冻结文件，对拍退化成「Rust 没变」的快照。
  R1–R7 那 7 个存活变异就是这条测试该杀的清单。**这是 U8c-3 的硬前置。**
- **P2 五处注释与代码不符** + 九处头注过度声称（含我那句「所以不存在两边同时错」——
  因果不成立，审计用 M9/P1/P2 证伪）。
- **P3** 删 4 条仪式性判据 · 棘紧 3 条扫描地板（40 vs 实测 99 / 4 vs 5 / 30 vs 39）·
  `shared_crate_registry` 的 `contains` 认注释 · `account_usage` 接线守卫的抽取窗口漏进后面实参。
- **P4** §1.4b 的搬家（monitor `backend/`）。
- **然后**才是 U8a-2c。

## 签收

- [x] 过代码审计（D）—— 四个此前存活的生产层变异逐条转红；新判据第一跑抓到 `wrap` 静默丢
- [x] 过工程审计（E）—— §1.4b 裁决落档；P1–P4 逐条登记，含「U8c-3 硬前置」
- [x] 主计划已更新（F）
