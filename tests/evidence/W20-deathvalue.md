# 步 20（`设计/70` 设置四刀 ＋ 三个 section）死值验 —— 12 刀

日期 2026-09-19 · 分支 `w20/settings-four` · 基点 `9b1cfd9e`

## 怎么做的

变异**只发生在 `/tmp/w20-dv`** 那份副本上（`src/` `tests/` 与几份配置文件的真拷贝，
`node_modules` 走软链）。**仓里一个字节都没有被改坏过** —— 落地后 `diff -r src/settings`
与 `diff -r tests/settings` 两向对拍，两棵树逐字节相同。

每一刀：改坏一处 → 跑那条判据 → 确认 `rc != 0` **且**失败输出里出现该判据的名字 →
把文件写回原内容 → 用 `sha256` 对拍**逐字节还原**。脚本 `/tmp/w20-dv/dv.py`（不入仓）。

## 12 刀逐条

| # | 刀 | 改坏的是什么 | 该红的判据 | 结果 |
|---|---|---|---|---|
| A1 | `data-section.ts` 构造期加回 `void this.load()` | 步 2「非落地页零 I/O」 | `panel-deferred-io.vitest.ts` | 红 ✓ 还原 ✓ |
| A2 | `panel.ts::flushPage` 什么都不放行（门焊死） | 步 2 的**反向**那一半：点进「应用」要真放行 | 同上 | 红 ✓ 还原 ✓ |
| A3 | `open()` 不再 `pagesLoaded.clear()` | 重开设置要能拿到**新读数** | 同上 | 红 ✓ 还原 ✓ |
| B1 | `holdSkeletonHeight` 改成字面量 `100px` | 步 1「骨架与容器同一个数」 | `settings-skeleton.vitest.ts` | 红 ✓ 还原 ✓ |
| B2 | `makeSkeleton` 不再挂 `data-skeleton` | 步 1「登记表 == 屏幕上那一批」（少一侧） | 同上 | 红 ✓ 还原 ✓ |
| B3 | `SKELETON_PX` 多登记一条没人渲染的 `ghost` | 步 1 同上（多一侧）—— 两向都要红才算相等 | 同上 | 红 ✓ 还原 ✓ |
| C1 | 把 `**别填 cct …**` 那对星号放回 tooltip | `§2.4` 纪律 · `§8` #5「界面零 markdown」 | `ui-copy-discipline.vitest.ts` | 红 ✓ 还原 ✓ |
| C2 | 把内部标识符 `tracing` 放回 ⓘ 文案 | 同上「内部标识符」那一条 | 同上 | 红 ✓ 还原 ✓ |
| C3 | 把 `SHAPES` 形状表清空 | **量具自检**：空尺子不许零命中地绿 | 同上（正控那一格） | 红 ✓ 还原 ✓ |
| D1 | `perMachineSlot` 默认就 `hidden = false` | 步 3「加载中不许把兜底态摆出来」 | `panel-groups.vitest.ts` | 红 ✓ 还原 ✓ |
| D2 | `onMachinePageRegistered()` 变 no-op | 步 3 的**另一半**：机器页来了要撤骨架 | 同上 | 红 ✓ 还原 ✓ |
| E1 | `revealPerMachineFallback()` 变 no-op | 步 3「真失败时兜底态要亮出来」 | `panel-block-isolation.vitest.ts` | 红 ✓ 还原 ✓ |

**12 / 12 全红、12 / 12 逐字节还原。**

## 为什么 D1/D2/E1 要三刀而不是一刀

步 3 是一个**判别式**，不是一个开关：「加载中」与「真失败」在屏幕上必须是两种样子。
只钉其中一边的话 ——
- 只有 D1（不许提前露脸）⇒ 「**永远藏着**」照样绿，而那时真失败的用户会看到五块凭空消失；
- 只有 E1（失败要露）⇒ 「**永远露着**」照样绿，而那就是今天那个「每次打开前 3 秒的默认视图」。

⇒ 三刀合起来才等于一条判别式。

## 这次死值验**没有**买到什么（如实写）

- **没有量真实排版**。`70 §8` 判据 #1「加载期高度变化 = 0」要真机挂 `ResizeObserver`，
  jsdom 没有排版引擎 ⇒ 上面 B1–B3 买的是「两处用同一个数」，不是「像素级不位移」。
- **没有碰后端那一侧**。`§2.4` 纪律真正的主语是「后端返回的字符串」，而
  `backend_policy.rs::death_copy` / `ledger_line` 今天仍在产 markdown 与设计论证
  （`70 §7` 第二刀 步 7，住 `src/bridge/`，本轮写区之外）。C1/C2 两刀买的是
  **前端这一侧**不再自己犯同一条；那三笔后端欠账登记在
  `tests/settings/ui-copy-discipline.vitest.ts` 的 `BACKEND_SIDE_DEBT` 里，
  并由一格判据钉着「登记不许被悄悄清空」。
- **没有覆盖那几块 per-machine 分节**（账号 / MCP / 工具 …）的构造期 I/O ——
  它们不在 `70 §10.4` 第一刀那三块的射程里，仍是一笔债。
