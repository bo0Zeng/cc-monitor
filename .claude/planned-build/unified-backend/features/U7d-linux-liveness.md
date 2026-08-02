# U7d · 本机判活：Linux 实现（功能那一半）

- 工作区：unified-backend · 第五梯队 · 任务 #95
- 风险档：**高**（动的是「哪些本机会话算活着」这个总闸）

## Phase B：**本轮提示给我的前提，实测是错的**

提示里写（也是我上一轮自己写进计划的）：

> monitor 侧的双重校验是 PID + `procStart`（.NET DateTime.Ticks），
> Linux 的 `/proc/<pid>/stat` 第 22 字段是 starttime（时钟滴答），**两者量纲不同 —— 别硬套**。
> ⇒ 准备降级成「只查存在性 + 标注置信度」。

**实测推翻**：拿本机 `~/.claude/sessions/*.json` 逐个比对 ——

```
pid=2037626  pidfile.procStart=3169940   /proc/stat[22]=3169940   ★ 完全相等
pid=2844224  pidfile.procStart=12892607  /proc/stat[22]=12892607  ★ 完全相等
…（共 6 个，6/6 完全相等）
```

那些值的量级（~10^6）也一眼不是 .NET Ticks（~6.4e17）。
⇒ **`procStart` 是平台原生的**：Windows 上是 FILETIME 系，Linux 上是 jiffies 系，
各自与本平台的查询口径同源。**PID 复用防御在 Linux 上是满精度的，不需要任何启发式降级。**

这是纪律 8「动某一格之前先重测那一格」的第二次兑现 —— 上一格（tmux）也是同样的形状。

## 实现要点：`/proc/<pid>/stat` 不能用朴素 `split_whitespace()`

第 2 字段 `comm` 是括号包起来的可执行名，**允许含空格与括号**。
扫本机 400 个进程，**有一个踩中**：

```
✗ 朴素解法错在这个进程：comm='tmux: server'
   稳健(field22)=1042   朴素(split[21])=0
```

**而 tmux server 正是本仓的核心依赖。** 稳健解法：找**最后一个** `)`
（comm 内部的括号不会是最后一个），其后是第 3 字段起 ⇒ starttime = 其后第 19 项（0 基）。

## macOS：**不做，如实写明**

没有 `/proc`，要走 `sysctl KERN_PROC` 的 FFI；本仓没有 macOS CI，我也无法实测 ——
**按纪律不写没验过的实现**。分支保留 `false`（不是 `unimplemented!()`：
daemon 那边是 CLI，panic 是「没人能忽略的信号」；这边是 GUI 常驻进程，panic 会崩窗口。
`false` 是 fail-safe —— 少显示，而不是显示永不消失的僵尸会话），并写进文档。

## DoD 验收

| # | 项 | 结果 |
|---|---|---|
| ① | Linux 判活 = 存在性 + `procStart` 精确比对 | 5 条测试，**跑在真进程上**（自己 / 刚退出的真子进程），不是只喂夹具 |
| ② | **PID 复用防御真的生效** | `pid` 对、`procStart` 不对 ⇒ 判死 |
| ③ | `comm` 含空格 / 含右括号不许错位 | 各一条；含空格那条还**反向断言朴素切法会错**（否则测试没有区分力） |
| ④ | **端到端：本机真实会话** | 一次性探针扫本机 6 个真实 pidfile：**判活 6/6，与 `/proc` 存在性逐个一致**。改动前这 6 个全会被判死。探针跑完即删（环境相关，不该进 CI） |
| ⑤ | 文档订正 | 模块头注那句「`procStart` 是 .NET Ticks」在 Linux 上是**假话** ⇒ 改成平台对照表；`ARCHITECTURE.md` 与双语 README 从「只在 Windows 工作」改成「Windows + Linux，macOS 未实现」 |
| ⑥ | 全量门禁 | daemon 224 · monitor 677/3 ignored · npm 1154 · tsc 0 · clippy 回到基线 64 |

## 实现期的两处自我订正

1. 英文 README 里手滑写进一个俄语词 `двух` —— 已改回 `two`。
2. 两份 README 顶部写着「Windows 桌面应用」，而下一行的平台栏早就是「Windows · Linux」——
   本就自相矛盾，Linux 成为一等本机平台后更误导 ⇒ 一并改成「桌面应用（Windows / Linux）」。

## 签收

- [x] 过代码审计（D）—— 5 条真进程测试 + 端到端探针
- [x] 过工程审计（E）—— 见主计划
- [x] 主计划已更新（F）
