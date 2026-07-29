# 状态 / STATUS — gate-integrity（恢复工作的入口，每次先读这里）

- **当前阶段**：**A 主规划已落盘，等用户审批**
- **当前功能**：无（G-A 待批准后开工）
- **当前步骤**：n/a
- **已完成功能**：无
- **下一个功能**：G-A（八套真机套件加断言数地板）
- **阻塞 / 待用户确认**：
  - **[待批准] 主计划**
  - **[待定] G-C 的 6 套要不要进本地 `npm test`** —— 建议**只进 CI**。
    理由：`npm test` 应保持「不需要 tmux/daemon 就能跑」。
    代价（本地改 `shared/ccm` 不会自动触发）如实写进 `e2e/README.md`
  - **[待定] `run-tests.sh` 若今天就红，修还是先登记** —— 建议先以「允许失败」进 CI 一轮看清
  - **[待定] 地板值与 `ci.yml` 标签的同源对拍要不要做** —— 建议做（那些标签已经漂过一次：
    39 vs 44、12 vs 21）
- **最近一次计划回看时间**：2026-07-29（Phase A 落盘）
- **自动模式（/loop）**：未起
- **备注**：
  - **路线图第 ③ 项。规模小（3 个功能），但它保护其余全部工作。**
  - **G-B 同时是 `account-zero` Z01 的前置**：Z01 要改 `vendor/cc-acct-iso/scripts/`，
    而那 1348 行今天在 shellcheck 门禁之外、它自己的 424 行测试从没跑过。
    **没有网不能改那个工具。** ⇒ 若 `account-zero` 先启动，G-B 插到它前面。
  - **本工作区最先改 `ci.yml`**，`rust-ts-boundary` C05 与 `local-as-remote` L0/L4 之后追加。
    **不改任何 workflow 触发条件。**
  - **验收判据**：「加上了地板」不算，**「人为删一条断言，CI 红」才算**。
  - **地板值直接用 Phase G 的实测数，不必重测**：
    `tmux-target` 26 · `ccm-cli` 44 · `ccm-print-parity` 12 · `ccm-acceptance` 15 ·
    `ccm-pretrust` 13 · `cc-spawn-uplift` 21 · `tmux-guarded` 14 · `usage-probe` 7 = **152**。
    前 7 套尾部已打印 `PASS=<n>`，**`cc-spawn-uplift` 没打印、要先加**。
  - 注意：静态 `ck`/`chk` 调用数 ≠ 运行期断言数（`ccm-cli` 静态 36 / 实测 44）。**用实测值。**
