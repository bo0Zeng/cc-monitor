# 状态 / STATUS — auto-e2e 工作区（每次先读这里）

> 独立 planned-build 工作区。分支 `account-ux`。**给真机功能补 e2e 埋点 + 全自动测试 harness**——
> 让 F03.2 灰灯/resume/attach/换号/孤儿/account 等**端到端 GUI+SSH+tmux 流**可自动验证（当前只有
> 纯逻辑单测 + F40 渲染 e2e，真机流零 e2e 埋点）。用户 2026-07-26 定：装 Windows VM，要「给真机功能补 e2e 埋点(全自动)」。

## 当前
- **阶段**：**Phase C 进行中——aya-core（F-E0+F-E1）委托独立 agent 实现+实跑**（用户 2026-07-26：先开能在 aya 跑的 + 让第三方 agent 去测试）。独立 agent 在 worktree 里建探针+fixture 并**实跑 gray-light 生命周期、如实回报真结果**（不由主线程自报绿；对齐仓库反伪造纪律）。
- **aya GUI 可行性**：webkit2gtk-4.1 在 aya 存在 → 全链 GUI e2e（Xvfb+tauri dev+loopback remote）可能可跑；agent 先试全链，不行退 daemon-frame 级（直跑 daemon 二进制断言 wire 帧）。
- **Windows SSH 那套（Tier2/3）**：待用户 VM 的 OpenSSH+工具链就位 → 先跑 WebDriver-over-SSH spike 再铺。SSH-驱动架构 + Tier-C 交互会话 recipe 已在 MASTERPLAN。
- **原 Phase A 摘要**（下方）保留供恢复参考。
- **2 agent 评估结论**：驱动=WebdriverIO(@wdio/tauri-service，DOM 层，windows CI)+ 少量 OS-input(↗，交互式 VM);断言=`[e2e]` 日志;fixture=loopback SSH 到本机 + daemon(CLAUDE_CONFIG_DIR 隔离)+fake-claude shim。**★关键：gray-light/resume/换号/孤儿 是后端+SSH+tmux 驱动、可在 Linux(aya/ubuntu CI 自环)全自动跑，不需 Windows VM**；Windows VM 只剩 Win32 层(↗)手动。
- **三层架构**：Tier1 会话生命周期 e2e(Linux 自环,最高 ROI)/ Tier2 Windows DOM 冒烟(WebDriver CI,薄)/ Tier3 Win32/native(手动清单)。
- **待批**：MASTERPLAN 的功能清单(F-E0 基建→F-E1 gray-light→F-E2 resume→F-E3 换号→F-E4 孤儿→F-E5 DOM 冒烟→F-E6 搭车) + 唯一动生产码处(可注入 confirm) + 「↗/真终端/SFTP 留手动」的范围裁定。**批准后按 F-E0 起步。**

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
