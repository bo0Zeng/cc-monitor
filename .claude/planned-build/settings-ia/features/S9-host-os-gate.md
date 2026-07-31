# S9 — `CcIntegrationSection` 加 OS 门

> **它是怎么冒出来的**：Phase G 核账时对照 MASTERPLAN §1 逐行读，发现 S9 仍写着「未开工」——
> 而 loop 的前提写的是「S0-S10 全绿」。**前提是错的**。S9 在用户已批准的主计划里，
> 跳过它等于静默缩范围，所以先把它做完再收官。

## §0 复核：这条计划的前提这次是**真的**（与 S10 相反）

S10 那轮的教训是「计划说要撤的入口根本不存在」，所以这次先证实缺陷再动手：

- `src/settings/cc_integration.ts` 全篇 PowerShell（`$PROFILE` / `profile.ps1` / `Microsoft.PowerShell_profile.ps1`），
  UI 标题逐字是「PowerShell 集成」。
- 它挂在**本机页**的「工具」栏（`panel.ts:653-658`，`appliesTo: "local"`）。
- **全仓前端零 OS 分支**：`grep -rn "Windows NT|X11|Macintosh|win32|darwin" src/` = 0 命中。

⇒ Linux 上打开设置 → 本机页 → 工具，看到的是一个装 PowerShell profile 的安装器。
v3.4.0 已经发了 `.deb`，所以这不是假想用户。

### §0.1 比计划写的严重一档：它**构造即发 IPC**

```ts
constructor() {
  this.root = this.build();
  void this.refresh();          // → cc_integration_status
  void this.refreshAutoLaunch();// → cc_get_auto_launch
}
```

`safeBlock("终端集成", () => new CcIntegrationSection().element)` 是 `buildBody()` 里
**eager** 构造的。⇒ 非 Windows 上每次建设置面板都白跑两次 Windows 专用后端调用。

**这决定了门开在哪**：不能只 `hidden = true`（那两次 IPC 照发），**必须不构造**。

## §1 OS 从哪来：`navigator.userAgent`，不加包也不加命令

三条路，选第三条：

| 路 | 否决理由 |
|---|---|
| `@tauri-apps/plugin-os` | **要装包**（红线） |
| 新开一个 Tauri 命令读 `std::env::consts::OS` | 要动 §6 那三处钉死的 `120` + `parity_ledger` 的 121/50/20。**更要命的是它会新增一项「本地独有能力」**——而 §2.2 整段论证的立足点正是「本地独有正在清空」。为一个 UI 显隐给自己的架构论证挖个坑，不划算 |
| **`navigator.userAgent`** | 零依赖、零 IPC、纯函数可测。Tauri 的 webview 在三个平台上分别是 WebView2 / WKWebView / WebKitGTK，UA 里 `Windows NT` / `Macintosh` / `X11; Linux` 都是稳定标识 |

**判定顺序有讲究**：先 Windows 再 macOS 最后 Linux。反过来会被
`Mozilla/5.0 (X11; Linux ...)` 之外的串咬到（Android UA 也含 `Linux`；这里虽然遇不上，
但把顺序写对不花钱）。

### §1.1 测不出时**往显示的方向倒**

`unknown` ⇒ **照常显示**。

理由是比较两种错的代价：
- 错判成非 Windows 而藏起来 ⇒ **Windows 用户找不到安装入口**，且界面上没有任何线索说它去哪了。
- 错判成 Windows 而显示 ⇒ 就是**今天的行为**，不构成回归。

所以 `unknown` 走「显示」这条，是把不确定性压在**无回归**的一侧。

## §2 藏起来之后那一栏**说一句话**，不是凭空少一块

本机页「工具」栏在 Linux 上会只剩 MCP + cc-bus 钩子。§2.4 那张表里「本机 · 启动器」
一格写的是「PowerShell 集成（未来 POSIX ccm）」—— 也就是说 Linux 上这一格**确实还空着**。

沿用 S5 `readiness.ts` 立的那条区分：**不适用 ≠ 缺**。一行静态说明（不是 ⚠，S7 判据里
静态说明本就不该做成警告），讲清「这台机是 Linux，PowerShell 集成不适用；POSIX 那边的
`ccm` 目前只对远端机器提供安装入口」。

## §3 接缝：`perMachineBlocks` 加一个字段，不另起一套

`movePerMachineTo` 已经有一条「这块对本机/远端有没有意义」的显隐判据（`appliesTo`）。
OS 门是同一类判据，加 `onlyOnHostOs?: readonly HostOs[]` 跟着它走，
**不新开第二条显隐路径**。同时构造侧按同一个谓词决定「建还是不建」。

## §3bis 自审顺出来的第二处：「还差什么」清单也是 OS 盲的

`readiness.ts` 的 `notApplicable()` 只排掉「本机 · daemon」。而 `ccm` 是 **POSIX 的 bash
启动器** —— monitor 跑在 Windows 上时，本机那一格的对应物正是这次加门的「终端集成」。

⇒ Windows 用户会在**这张专为新用户做的清单**上读到一条
「本机 · 启动器：未测过 —— 终端里没有 cc 命令」，而那条在他机器上**无从补起**。
这跟 S5 立那条「缺 ≠ 不知道」是同一类错：在最不该误导人的地方误导人。

同轮修掉。`hostOs` 走**注入**而不是让 `computeGaps` 直接调 `hostOs()` ——
这个模块的卖点就是纯函数，`isDaemonless` 当初也是为同一个理由注入的。
省略 = 按非 Windows 处理（`ccm` 照常算数），这样默认值不会替谁下结论。

**范围**：只管本机。远端是不是 POSIX 跟 monitor 跑在哪没关系（远端一律走 ccm），
有一条反向测试钉住这点。

## §4 测试与变异（退出码判定）

`detectHostOs` 是纯函数 ⇒ 直接钉三大平台真实 UA 串 + 未知串。
面板侧钉两条：Windows 上那块在、非 Windows 上那块不在**且不构造**（用 mock 计构造次数）。

**已知会被打红的既有测试**：`panel-groups.vitest.ts` / `panel-block-isolation.vitest.ts`
在 jsdom 下跑，jsdom 的 UA 含 `linux` ⇒ 不设覆盖就会少一块。
这**正是想要的信号**（说明门真的生效了），处理办法是在这两个套件里显式置成 windows，
而不是把门放宽。

### §4.1 计划里那条变异**没做，因为它是等价变异**（如实记）

原计划写的第 ① 条是「判定顺序反过来」。动手前先算了一遍：把 `Linux` 那条提到最前，
三个平台的真实 UA 结果**一个都不变**（Windows 串不含 `Linux`/`X11`，macOS 串也不含）。
⇒ 它今天是个**等价变异**，为它写测试必然是安慰剂。

所以顺序照样从窄到宽排（给将来加平台留余地），但代码注释与测试文件里都**明说没有测试在守它**，
而不是摆一条看着像在守、其实杀不掉任何变异的断言。

### §4.2 实跑的 8 条，逐条退出码见红

| # | 变异 | 打红的 |
|---|---|---|
| M1 | `unknown` 不再放行 | 「认不出时照常显示」 |
| M2 | `hostOsAllows` 恒真 | Linux/macOS 两条 |
| M3 | 构造点的门拆掉 | 构造计数那条 |
| M4 | 认不出 macOS | macOS 不适用那条 |
| M5 | 名单里混进 `linux` | Linux 不构造那条 |
| M6 | `ccm` 不分 OS 一律不适用 | 「非 Windows 照常算数」 |
| M7 | 省略 `hostOs` 时默认按 windows | 「省略 ≠ Windows」 |
| M8 | 调用点漏传 `hostOs` | remote-section 的接线那条 |

**M8 第一次报绿，是夹具问题不是覆盖问题**：`perl -0pi` 那条多行模式没匹配上，
变异**压根没写进文件**。靠「改完先 grep 计数确认真的改了」当场识破 ——
只看退出码不看变异是否落地，就会把「没改」读成「改了也不红」。

## §5 签收

- [x] 过代码审计（自审 + 4 条变异，退出码判定）
- [x] 过工程审计
- [x] 主计划已更新
