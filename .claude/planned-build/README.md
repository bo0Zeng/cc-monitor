# planned-build 工作区索引

`planned-build` skill 的持久产物按**工作区**分子目录，每区一套 `{MASTERPLAN,STATUS}.md` + `features/`。
恢复某区先读其 `STATUS.md`。本索引由 audit-fixes F11 建，列各区一句话状态（可能滞后，以各区 STATUS 为准）。

| 工作区 | 主题 | 状态（约） |
|---|---|---|
| **audit-fixes/** | full-audit + open issue bug 全修 + 测试/门禁/文档/重构（`account-ux` 分支可干净合入） | F01-F12 完成；F13 脊柱拆分**单独拆出 → spine-split/**；剩 Phase G |
| **spine-split/** | 脊柱拆分评估：tabs.ts(3178)/ssh_source.rs(4756) 是否分解为小模块 | **关闭——评估后决定不拆**（判据=具体架构病非行数；两文件拆分负收益+引入 §24/可见性风险；唯一真架构病 F12 已在 audit-fixes 修）。未动代码 |
| **auto-e2e/** | 给真机功能（灰灯/resume/attach/换号/账号）补 e2e 埋点 + Windows 全自动测试 harness | Phase A 深度可行性评估中（2 并行 agent：Windows GUI 驱动 + probe/fixture 方案；待用户批主计划） |
| **account-ux/** | 多账号 UX（切号菜单 / 按会话选号 / 换号优雅重启 / app 内部署向导，#68/#69） | 已交付（v3.2.0） |
| **account-isolation/** | 多账号隔离又同步内核（`cc-acct-iso`：各 `CLAUDE_CONFIG_DIR` + symlink 共享） | 已交付（v3.2.0） |
| **bugfix-sweep/** | 会话/生命周期一批 bug 清扫 | 归档（成果已并入主线） |
| **daemon-codex/** · **codex-phase2/** | daemon 侧适配 codex（非 CC agent）分阶段 | 计划/协作待推进 |
| **tmux-daemon-reconcile/** | tmux 存活对账（带外杀 tmux → 变灰，#60-A / F74c） | 已交付（reconcile_step + 收帧收割器，见 INVARIANTS §24/§24bis） |

> 注：状态摘要仅导航用；权威状态恒以各区 `STATUS.md` 为准。新开工作区时在此追加一行。
