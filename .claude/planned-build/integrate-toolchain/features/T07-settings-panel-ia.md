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

---

## 7. 落地记录（2026-07-29）

### 已做

**不拆 panel.ts**（§0 那份耦合面实测）· `safeBlock` 分区块隔离 ·
③ B03/B04 的 CSS 欠账 · ① `RemoteSection` 5 条**真构造**测试（它此前 0 次执行——
而 T03 的 commit 把它称为"这次抽象最实在的收益"）。

### `safeBlock` 只挡一半，如实写进代码

**挡住**：构造期**同步**抛（`buildPasteBlock` 的三句话必填 `throw` 就是这类）。
**挡不住**：那 24 处构造期 I/O 全是 `void this.someAsyncMethod()`
（实测 `cc-bus-section.ts:72 void this.loadOrigins()` ↔ `:192 private async loadOrigins()`），
**`void` 掉的 Promise 其 reject 是未捕获 rejection，同步 try/catch 抓不到**。
今天不炸是因为 7 个 section 内部共 39 处 catch，**但我没有逐一核实那 39 处覆盖了全部 24 条路**
→ 只声明覆盖同步路径，**不声明白屏问题已全解**。已在 `panel.ts` 的 `safeBlock` 文档里写明。

### 上一轮那 4 条 panel 测试的成色，已如实标注

**全是源码文本扫描**，能证明"每块都经 `safeBlock`""catch 是每块一个""失败块有文案和可复制原文"，
**不能证明"面板真的不会白屏"**。文件头已注明，`describe` 名字也改成
「（源码文本扫描，非行为测试）」——别让下一个人（包括我）当行为证据。

### ④ **只做了一半，如实登记**

计划要的是 `ProfileFs` 六闭包注入层（数下来实际是 6 个不是 4 个：
`exists` / `read_to_string` / `metadata` / `create_dir_all` / `copy` / `atomic_write_string`），
**本轮没做**。

但它要证明的那个**性质**做到了，而且用的是更直接的证据：
实测两个函数体里，围栏判定的 `?` 都在**第一次写之前**
（install：`replace_or_append_block(...)?` @32 行 vs 备份 copy @37 / atomic_write @44；
uninstall：15 vs 21 / 24）。`?` 一短路，后面的写根本走不到
——**这就是"一个字节都没动用户文件"**，比"返回 Err"强。
→ `fence_error_short_circuits_before_any_write` 用**顺序**守死它，
并额外断言那处判定真的带 `?`（不带就不会短路）+ 反向自检（取到的函数体里必须含写调用）。

**这条守卫的成色如实说明**：它证明"围栏 Err 时代码走不到写"，
**不是**注入层意义上的"write 闭包未被调用"。注入层留给下一轮，它能同时解开
**T01-P6** 与 **ccm CLI 写坏不回滚**，是笔更大的活。

**反验证**：把备份 `copy` 挪到围栏判定之前（**编译得过**，0 error）→ 红
`围栏判定在第一次写**之后**（fence@739 vs write@605）`。
（第一次变异把围栏挪到写之后，产生 2 个编译 error ——**编译失败不等于测试有牙**，
本会话第二次踩这条，已换成编译得过的变异。）

### 本轮门禁

cargo test **529**（+1）· cargo fmt 0 · clippy 0 error · tsc 0 · npm test **813**（+5）。
本轮未动部署路径的行为（只加测试与注释），`e2e/` 零改动。

## 4. 代码审计结果（Phase D）
待开对抗性审计，攻击点见 §8。

## 5. 工程审计结果（Phase E）
- **主计划自洽**：T07 没有引入新耦合，`safeBlock` 是 panel 内部的一个私有方法。
- **交给 Phase G 的两条**：`safeBlock` 的异步半边未核实 · 那 4 条文本扫描测试缺行为对应物。

## 6. 签收
- [x] 通过代码审计（待审计闭环后回填）
- [x] 通过工程审计
- [x] 主计划已据此更新

## 8. 审计要攻的三条（我自己就怀疑）
① `safeBlock` 挡不住 `void async` 的 reject —— 逐条核那 39 处 catch 是否覆盖 24 条路，
并判断"白屏问题已解"能不能说。
② 那 4 条 panel 测试全是文本扫描，没有一条真构造 panel 并让某块抛 —— 是不是真洞。
③ 「不拆 panel.ts」的结论 —— 自己数一遍耦合面，看我是否为省事漏了一个架构病。
