# P 段就绪评估（2026-07-28，R 段与 B 段全部完成后写）

## 一、前置 G01-G03 已由 R 段满足，T01 解锁

| 前置 | MASTERPLAN §4 的要求 | 现状（实测） |
|---|---|---|
| G01 | `cargo fmt` 28 处红（CI 唯一阻断性 Rust 门） | **绿**（`cargo fmt --check` rc=0） |
| G02 | 分支领先 main 15 commit **从未跑过 CI**；7 套真机 e2e 不在 CI | **已进 CI**：`e2e-tmux` + `e2e-tmux-rust` 共 **8 套**（R00 加 7 套 + B02 加 `cc-spawn-uplift`）；draft PR 每次 push 都跑 |
| G03 | `INVENTORY.md` 冻结在 F01 → 成功标准① 从未验收 | **R06 已重写**（符号名 + 可跑 grep 锚点，不再用行号） |

→ 「在信号不可信的基座上开集成工程 = 蒙眼开刀」这条顾虑已解除。

## 二、T06 的一条历史顾虑已消解（实测）

我的长期记忆里记着「code-picture 语料迁移 → cc-monitor 有 9 文件/11 处跨仓引用待改」。
**实测现在只剩 2 处，且都是文档、无代码引用**：
`integrate-toolchain/STATUS.md`（计划自身的叙述）与 `vendor/code-picture-core/VENDOR.md`
（那正是 D1 要改写的文件）。→ **T06 不再被跨仓引用挡着**，D1 照原计划即可。

## 三、T01/T03 的「≥2 真实消费者」前提——现在才真正成立

这是本工作区自己定的纪律（STATUS：「注册表的正当性来自有 ≥2 个真实工具要求同一套机制。
提前建等于为假想需求设计（unify-launch R12 踩过的同型错误）」）。逐条核对：

**T01（注册表内核）**：MASTERPLAN §2 那张表列了五套工具，每套只实现了
「源 / 落点 / 探测 / 装升卸 / 配置面」五个正交关注点中的三四个，且写法各不相同
（`.vendor_id` 指纹 vs `build.rs` freshness 是**两套**vendor 写法；`--ccm-probe` vs
daemon `hello` 帧是**两套**能力自报）。→ **前提成立**，且不是"将来会有第二个"，是**现在就有五个**。

**T03（生成待贴文本统一组件）**：**B 段刚刚把它的第二个消费者做出来了**。
现在仓里有两个真实的「生成 + 复制 + 说明」生成器：
- F08 的 shell 别名生成器（`src/launcher-diagnostics.ts:142-161`）
- **B04 的钩子片段生成器**（`src/settings/cc-bus-hooks-section.ts`，本轮新增）

两者形状高度重合（生成文本 → 只读 textarea → 复制按钮 → 成功/失败 toast + 粘贴后如何生效
的说明），且**都刻意不代写用户文件**。→ T03 的抽象**从今天起被真实需求证成**；
在 B04 之前建它就是提前抽象。**这条要写进 T03 的功能计划当作它的正当性依据。**

## 四、对 MASTERPLAN 的两处订正（B 段已改变事实）

1. **账本里「`~/.claude/settings.json` 的 cc-bus 钩子」那一行（T03,T05）已由 B04 达成**：
   「cc-monitor 只读+诊断+生成」已落地，且比账本写的更细——四态而不是三态
   （见 `unify-launch/features/B04-hook-states-from-real-disk.md`：用户实际写法与规范片段
   功能等价但字符串不等，等值比较会把正确安装误报成第三态）。
   → T05 在本工作区的剩余部分只有**部署**（把仓内 `shared/cc-bus/` 装到 skills + symlink），
   诊断部分不必重做。
2. **T05/T09 已整体移出本工作区**（STATUS 已记），B 段以 B01-B04 承接完毕。
   本工作区的 T05 条目应改标为「只剩部署，诊断已由 B04 完成」。

## 五、成功标准② 的最后一次机会在这里

`unify-launch/features/B03-B04-cockpit-and-hooks.md §12` 已实证：**B 段全程没有验收场地**
（`tryRenderCli` 全仓只有一个真实调用点，图形化 spawn 是远端 exec 不经 IR）。
P 段若真需要给远端工具传新的 TS 修饰（如受管工具部署时要选账号/选 agent），那才是第二次
验收的场地。**判据**：加一个 `LaunchDimension` 时，是否零改 builder/renderer/调用点。
若 P 段同样不需要——那说明这条标准假定了「新维度会持续出现」，而实际演进方向是
**shell 侧长能力**（`--detach`/`--tmux-size` 都是 shell-only），TS 的 IR 反而稳定了。
**IR 稳定是好事，不该记成"验收失败"**；届时按 §12 的措辞改写标准，**不伪造第二次验收**。

## 六、下一步（Phase B）

按 §4 的依赖图，第一个功能是 **T01 受管工具注册表内核**。它的 Phase B 计划要回答：
- `ToolSpec` 到底声明什么（五个正交关注点各自的最小契约）；
- **先用五个已知行为的工具验证抽象**（§4 第 3 点：T04 收编先于 T05/T06 搬迁——
  反过来做等于用未验证的抽象去接未集成的工具，两个变量一起动）；
- 复用而非新发明：`profile_installer.rs` 的「备份→写→读回比对→回滚」、
  `sftp.rs::ccm_cli_has_required_elements` 的**结构性扫描**（已实证固定 needle 是空转的）。
