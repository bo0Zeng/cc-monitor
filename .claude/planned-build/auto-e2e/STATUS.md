# 状态 / STATUS — auto-e2e 工作区（每次先读这里）

> 独立 planned-build 工作区。分支 `account-ux`。**给真机功能补 e2e 埋点 + 全自动测试 harness**——
> 让 F03.2 灰灯/resume/attach/换号/孤儿/account 等**端到端 GUI+SSH+tmux 流**可自动验证（当前只有
> 纯逻辑单测 + F40 渲染 e2e，真机流零 e2e 埋点）。用户 2026-07-26 定：装 Windows VM，要「给真机功能补 e2e 埋点(全自动)」。

## 当前
- **阶段**：**Phase A 主规划（深度可行性评估中）** —— 未动任何代码。
- **评估方式**：2 并行 agent（① Windows Tauri GUI 驱动可行性：tauri-driver/wdio vs OS-input+log-probe，逐层功能可驱动性 ② 逐功能 probe+fixture 埋点方案 + 在 aya 上造确定性远端 fixture）。收齐 → 综合 MASTERPLAN。
- **门禁**：Phase A 主计划**须过用户确认**（planned-build 铁律 #7）才进 B/C。

## 摸底结论（待 agent 深化）
- **无任何 Windows e2e 工具**（package.json 零 webdriver/tauri-driver/playwright）——要从零搭。
- 现有 e2e = **仅 Linux + 仅渲染**：`e2e-probe.ts`（DEV-gated，emit `[e2e]` 日志）+ `f40-suite.sh`（Xvfb + xdotool；XTEST 进不了 webview 故用中键）。**本轮真机功能零 e2e 埋点**。
- `tabs.debugSnapshot()` 只 dump 活跃 tab 的**渲染态**（scroll/pending/timeline/fold/err），**不含** status/tmuxIdle/account/origin → 要扩字段。
- **fixture 可在 aya 造**：tmux 3.6 + daemon 二进制已 build（`remote-daemon-proto/target/debug/cc-monitor-remote`）→ 可造 `@ccm_sid` tmux 会话、fake claude 起退、真跑 daemon 造确定性 idle/archived 状态。
- **预判难点**：Windows 合成输入能否进 WebView2（Linux XTEST 进不了 WebKitGTK 是前车）；↗ 拉前=HWND OS 级断言、真终端依赖，可能留手动。

## 硬约束
- daemon 零行为改动（只读；探针只加前端 DEV-gated + 测试 fixture，不改 daemon 逻辑）· 不 push/发版/bump · 不要用 emoji · 埋点只在 DEV/测试构建、生产零包含（同 e2e-probe.ts `import.meta.env.DEV` 门控）。
- 行为等价：加埋点不许改任何生产功能行为。

## 门禁纪律
结果重定向到文件 + Read/grep 核实 + pipefail；埋点改动跑 tsc + vitest 绿。
