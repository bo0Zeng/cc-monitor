# 功能计划 — T07 设置面板：**不拆**，治真正的病

## 0. 前置判断：数清耦合面之后，`panel.ts` 826 行**不该拆**

按 memory 那条纪律「拆分看缺陷不看行数——别默认 god file 该拆，拆由具体架构病证成」，
先数了四个耦合面（实测，不是印象）：

| 耦合面 | 实测 | 判读 |
|---|---|---|
| panel 持有的 section 引用 | **2 个**（`dataSection` / `remoteSection`，只为 `refresh()`） | 极小，不是 god object |
| section 之间的直接 import | **0**（8 个 section 互不 import） | 已经是解耦的 |
| `SETTINGS_APPLIED_EVENT` 订阅点 | 4 处（panel / accounts-section / main / keybindings-editor） | 靠事件解耦，不是直接调用 |
| **构造期做 I/O 的文件** | **9 个文件 / 24 处 `void this.load\|check\|refresh\|fetch`** | ← **这才是真正的病** |

**结论：`panel.ts` 不是 god file，它是 8 个互不依赖的 section 的装配点。**
拆它治不了任何已知缺陷——**没有架构病可以由"拆"来治**。
本会话「先数清再抽」已第十一次改变计划（T04 第二步是"不该建"，这里是"不该拆"）。

## 1. 而数出来的那个病，比"拆"重要得多

**9 个文件、24 处在构造期就发起 I/O**（`new XxxSection()` 里 `void this.loadX()`），
而 T03 交接的第 ② 条指出整条构造链**没有一个 try/catch**：

```
paste-block.ts:80 throw
  → panel.ts:404 buildBody()
  → panel.ts:215 构造器 this.el = this.build()
  → main.ts:859 new SettingsPanel(...)
  → main.ts:122 await bootstrapSettings()
  → main.ts:102 DOMContentLoaded handler   ← 也没有
```

两者相乘才是真爆炸半径：**任何一个 section 在构造期抛（包括它那 24 处 I/O 里任一处
同步抛、或 `buildPasteBlock` 的三句话必填 `throw`）→ 整个设置窗白屏、零提示。**
今天三个 `buildPasteBlock` 调用点都是字面量所以不触发，但 24 处构造期 I/O 是活的。

**治法是分区块隔离，不是拆文件。**

## 2. DoD

- [ ] `panel.ts` 的每个区块用**分区块 try/catch** 包起来：某块构造失败 → 就地渲染
      「此区块加载失败：<err>」+ 可复制的错误文本，**其余区块照常出**
- [ ] 有测试证明：让某个 section 的构造抛 → 面板**仍然渲染**，且那一块显示错误、别的块在
      （**反向自检**：去掉 try/catch → 该测试必须红）
- [ ] ① `remote-section.ts` 的 smoke 测试：真 `new RemoteSection()` 走一遍（它此前**零执行**）
- [ ] ③ 补 `cc-bus-*` / `hooks-*` 的 CSS
- [ ] ④ `profile_installer` 做可注入 fs（`read/write/copy/exists` 四闭包）+ 真文件级损坏围栏测试
      ——**红线是"绝不真写盘"，所以先有注入层才能测**。顺带让 T01-P6 与「ccm CLI 写坏不回滚」变得可做

**不做**：拆 `panel.ts`（见 §0）· 把 24 处构造期 I/O 改成懒加载（那是另一个功能，
且会改变所有 section 的可见行为，风险 > 收益，**登记**）

## 3. 风险

- 分区块 try/catch 若包得太粗（整个 `buildBody` 一个 catch）= 一块坏还是全没
  → 必须**每块一个**，且测试要断言"别的块还在"
- ④ 的注入层会动 `install_to_profile` 签名（它是 `#[tauri::command]` 链上的）
  → 保持外层签名不变，只在内部分出可注入的核心

## 4. 代码审计结果（Phase D）
（待填）

## 5. 工程审计结果（Phase E）
（待填）

## 6. 签收
- [ ] 通过代码审计（无阻塞项）
- [ ] 通过工程审计
- [ ] 主计划已据此更新（含变更记录）
