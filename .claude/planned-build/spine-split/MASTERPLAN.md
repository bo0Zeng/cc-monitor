# MASTERPLAN — spine-split（评估 + 决定：**不拆，维持现状**）

> 结论先行（用户 2026-07-26 拍板）：**两个「god file」都不拆，维持现状。** 本文件是「为什么决定不拆」的
> 留档证据——将来有人再提「拆了吧」时先读这里。评估用了 3 个并行 agent（tabs 可拆清单 / 拆后安全 /
> ssh_source 可行性），报告要点摘录在下。**未动任何代码。**

## 判据（本轮最重要的收获）
**拆分由「具体架构病」证成——错向依赖 / 逻辑没法测 / 真实耦合痛点——不由行数证成。**
「文件大 / god file 听起来就该拆」是直觉、不是证明。评估后：两个文件都没有能压过「拆分**引入**的新风险」的具体收益。

## ssh_source.rs（4756 行）——决定不拆
拆分通常宣称的收益，逐条对不上：
- **编译时间**：Rust 按 **crate** 编译；同 crate 拆模块增量编译**零收益**。
- **封装**：`stream_loop` 触碰几乎所有簇 → 拆开**逼** private 放宽成 `pub(super)`/`pub(crate)` = **削弱**封装（拆分头号理由在此为负）。
- **可测性**：纯逻辑簇（frames/reconcile/batcher/…）**已各有独立测试模块**，零增益。
- **导航**：逻辑清晰 + 注释完善 + 内聚已高，收益边际。
- **合并冲突**：单人项目，N/A。
拆分要**引入**的新风险：可见性放宽 · 测试重新分区（`tier1_tests` 横跨 import+conn）· **`REMOTE_IDLE` 等 static 复制误伤 §24 单写者**（真实正确性风险）。→ 风险/收益倒挂，**维持现状**。
（agent 曾建议「叶子簇 import/frames/reconcile/types 可现在拆」——即便那几个，收益也只是导航性 reorg，不足以抵 static 归属决策 + 测试分区成本，一并不做。）

## tabs.ts（3178 行）——决定不拆（size 维度）
- 唯一命中「具体架构病」的是 **F12（remote-config）= 分层倒挂**（配置数据层住 UI 文件、被 8 个非 UI 模块 import、逼测试整体 mock UI）——**已在 audit-fixes 修完**，收益明确。
- 剩下的（tmux-match 纯函数、右键菜单、标题计算…）内聚已好，抽出=**纯导航性 reorg**，同 ssh_source 的弱收益/引入风险画像。账号子系统更是经 `alignableCurrent` 三域纠缠（早先 F13 摸底已定「别硬拆」）。→ 为「大」而拆不划算，**维持现状**。

## 3 agent 评估报告摘要（留档）
- **tabs 可拆清单**：15 单元、3 波（安全→中→纠缠）。最安全 cut=tmux-match 纯函数（verbatim move、全测）；最纠缠=account-alignment(~550 行，`alignableCurrent`+`compactWaiters↔onLine`+`updateAccountBadge`↔tab-bar 三向结）。零测盲区：snapshotSessions/trackUsage/cleanupOrphanTmux/拖拽。
- **拆后安全**：tsc strict+noUnusedLocals 兜机械错，**兜不住**运行期 DOM 接线 / 模块单例语义 / **import 环（无 no-cycle 门禁）**。account 子系统有 29 测可作金标准。
- **ssh_source 可行性**：build 基线绿；6 个 static 是耦合骨干；stream_loop 是蛛网；§24 纯 reorg 不受威胁但**拆分手误复制 static 会静默破坏单账本**。裁决：叶子簇可拆但收益薄、core 区延后——综合权衡后**整体不拆**。

## 状态
**关闭（决定不拆）。** 无后续功能。若未来出现**具体**架构病（非行数），再按判据单独立项。
