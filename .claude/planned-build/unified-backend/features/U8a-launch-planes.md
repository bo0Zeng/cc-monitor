# U8a · 起会话：Phase B 摸底与三平面分解

- 工作区：unified-backend · 第六梯队 · 任务 #96
- 本件产出：**分解与判定**（实现按分解后的平面分批做）

## 逐条重测计划的断言（纪律 8）

| 计划原话 | 实测 | 判定 |
|---|---|---|
| 「**三种容器形态**各自独立行为」 | `LaunchContainer` 是 **2 种 kind**（`none` / `tmux`）× **3 种 tmux mode**（`create-or-attach` / `send-into` / `attach-only`） | 说的是 **mode**，不是 kind。措辞订正 |
| 「`send-into` 是 `ccm` 表达不了的**唯一**形态」 | **成立**。`canRenderCli` 的 `ok:false` 共 6 条，只有 `send-into` 那条是**表达力缺口**（「无 CLI 等价语法」）；其余是「未装 ccm」/「缺能力 X」/「attach 必须是 tmux 容器」/「`cliFlags` 返回 null」—— 能力缺失与参数校验，不是形态表达不了 | ✅ 这条站得住 |
| 「#76 防线」 | `launch-render-cli.ts:12` 头注：`container.mode === "send-into"` → 强制走兜底，「**这条是防 #76 复发的关键**」，且有专测（`#76 防线应自述`） | ✅ 已在代码里，不是口头约定 |

## ★ 真正的分解：不是「三种形态」，是**三个平面**

「起会话」被当成一件事，实际是三件、**归属完全不同**：

| 平面 | 做什么 | 归谁 | 现状 |
|---|---|---|---|
| **① 计划面** | 决定「要跑什么命令」（含账号 / 容器 / resume 目标） | daemon `control/` | **已经在那儿了** —— `--resolve` 产出 `CommandPlan`，U6b-3 已把它吸收进流通道 |
| **② 远端执行面** | 在远端**真的**建 tmux / `send-into` 一个已存在的 idle 会话 | daemon `control/`（**该搬的是这个**） | 今天由 monitor 拼命令串、经 SSH 送过去 |
| **③ 本机开窗面** | 在**用户自己机器上**开一个终端窗口（再由它 ssh 过去） | **只能是 monitor** —— daemon 在远端，开不了你面前的窗 | `launch_powershell_window`，**Windows-only** |

⇒ **U8a 不是「把起会话整个搬进 daemon」**，只有平面 ② 能搬。
平面 ③ 结构上搬不了；平面 ① 已经搬完了。

这解释了为什么 `send-into` 特殊：它是平面 ② 里**唯一不能靠「拼一条命令串扔过去」完成**的
—— 它要对一个**已存在的** tmux 会话做 send-keys，而 `ccm` 的语法里没有「就地复用、不新建」。
搬进 daemon `control/` 之后，它就不再需要 CLI 等价语法（daemon 直接调 tmux），
**#76 那条防线的形态会变**：从「渲染器拒绝渲染」变成「daemon 有一条专门的命令」。

## 平面 ③ 的 Linux 现状：**已处置，不是缺口**（差点误报）

U7d 刚让 Linux 成为一等本机监听平台，于是我去查「Linux 上点 ↗ 会怎样」：
`launch_powershell_window` 在非 Windows 直接返回 `Err(...)`，全仓也**没有**任何
`gnome-terminal` / `x-terminal-emulator` 之类的实现（`xdg-open` 只用于开目录）。

**但那不是缺口** —— `remote-launch-run.ts` 头注逐字写着：

> 失败回退 = F09 旧行为：复制命令 + toast 说明（**非 Windows dev** / 配置缺失 /
> wt+PowerShell 都 spawn 失败时，用户仍拿得到可粘贴命令，**功能永不变砖**）

即 Linux 上点 ↗ = 命令进剪贴板 + toast。**刻意设计，不改。**
（真要在 Linux 上开窗是 U8b 的事，且需要选终端模拟器 —— 那是产品决定，不是清理。）

## 交付与移交

本件只交付**分解与判定**；实现按平面分批：

| 后续件 | 内容 |
|---|---|
| **U8a-2** | 平面 ② 搬进 daemon `control/`：一条入方向命令（`Disposition::spawn` 臂 + `COMMANDS` 登记，三条纪律自动生效）。⚠ 它要在远端**起进程**，与 `resolve` 那种纯计算命令不同 —— 信任边界、超时、失败语义都要单独设计，不能照抄 |
| **U8b** | 平面 ③ 的 OS 分派（Linux/macOS 开窗）+ `launch.rs:304` 那个真 bug |
| **U8c / U8d** | 原样保留 |

## 签收

- [x] 过代码审计（D）—— 本件是测量，四条断言逐条核过（三条成立、一条措辞订正）
- [x] 过工程审计（E）—— 分解写入主计划，U8a 拆成 U8a（判定）+ U8a-2（实现）
- [x] 主计划已更新（F）
