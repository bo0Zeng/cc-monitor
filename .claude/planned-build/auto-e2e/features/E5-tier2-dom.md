# F-E5 — Tier2 Windows DOM 冒烟套件（WebdriverIO，经 session-1 hop）

> 复用已验通的 session-1 hop（`schtasks /it`→session1 跑 wdio 驱真 WebView2）。分支 account-ux。
> 红线：daemon 零改·不改 TMUX_LS_FMT·不碰 ~/.bashrc·不 push/发版/bump·埋点只 DEV·不用 emoji。

## 关键 recon 结论（agent 摸底，file:line 已核）
- **WebDriver 直驱 DOM**：不像 XTEST 进不了 webview（f40 靠中键绕），WebDriver 直接驱 DOM → `sendKeys`(物理码)+ 选择器点击都可用。断言可 `browser.$`/`browser.execute` 直读 DOM（比 grep 日志干净），或复用 `[e2e]` 探针(Ctrl+Alt+F9/F10 / 中键)。
- **键位用物理码**：`KeybindingDispatcher` 按 `KeyboardEvent.code` 归一（`registry.ts:60-72`）→ wdio 发 `BracketRight`/`Digit1`/`KeyH` 等，别发布局相关 `key`。可编辑焦点里单键被抑制（`registry.ts:241`）。
- **裸壳可断言（无 fixture）**：`#app`/`#tab-bar`/`#message-stream`/`#status-bar`(index.html:11-14)；`.status-msg`("等待活跃…")、`.status-count`("活跃 0")、`.empty-state`("暂无活跃会话")(main.ts:136-179)；6 个顶栏钮 `.settings-trigger/.history-trigger/.panorama-trigger/.usage-trigger/.grid-monitor-trigger/.sftp-trigger`(main.ts:411-510)、`.status-cmdk`、`#tab-bar-resizer`；overlay 切换快捷键 H(历史)/G(全景)/Ctrl+K(命令栏,唯一带修饰的默认)/T(Tasks)/F11/M/Esc 可开可关。
- **需 fixture（会话/tab 存在）**：`button.tab`（**无 id/data-sid**，只能 `nth-of-type` 或 `.tab-title` 文本定位）、右键菜单 `div.tab-context-menu`>`button.tab-context-menu-item`(tabs.ts:2863/3063)、resume（仅 archived tab 的菜单项 tabs.ts:2884/2888/2895）、`.status-account` 账号 chip（配了远端多账号才显 account-chip.ts:94）。
- **confirm 阻塞**：仅 `account-restart.ts:84` 有可注入 seam(`opts.confirm`)；其余 `killRemoteTmux`(tabs.ts:2554)/`cleanupOrphanTmux`(tabs.ts:2594)/settings/sftp/history 全裸 `window.confirm/prompt`。Tier2 里**避开破坏性动作**，或 spec 内 `browser.execute(()=>{window.confirm=()=>true})` 桩掉。
- **探针出口**：`debugSnapshot()`(活跃 tab 渲染态 tabs.ts:704)、`debugSessionsSnapshot()`(全会话 status/tmuxIdle/origin/account/mismatch tabs.ts:1136)，DEV 门控、`[e2e]` 日志。

## DoD（分层，可验证）
- [ ] **E5a 核心——裸壳 DOM 冒烟（无 fixture，最高 ROI）**：wdio spec 经 session-1 hop 驱真 WebView2 断言：
  - 壳元素存在：`#app`/`#tab-bar`/`#status-bar`/`#message-stream`。
  - 状态栏文案：`.status-msg` 含"等待活跃"、`.status-count` 含"活跃 0"、`.empty-state` 可见含"暂无活跃会话"。
  - 6 顶栏钮 + `.status-cmdk` 存在且可点。
  - overlay 快捷键：发 `KeyH` → 历史 overlay 出现；`KeyG` → 全景；`Ctrl+KeyK` → 命令栏；发 `Escape` → 关闭。（真 WebDriver 键盘驱动力证 = 比裸壳静态断言更强的活证。）
  - **VM 上 N passing / 0 failing**（回盘读结果）。
- [ ] **E5b 会话相关（争取，够不着则如实降级+留档路径）**：VM app 配 aya 当远端 + aya 跑 F-E1 的 fake-claude fixture → 一个 tab 出现 → 断言 `button.tab`+`.tab-title`+`.live-dot` 类；右键 tab → `.tab-context-menu` 出现含预期项；archived tab 有 resume 项。**若 VM app 远端配置+时序太脆本轮够不着，交付 E5a + 文档写清 E5b 复用「E(b) 远端 + aya fixture」的落地路径，不假装做了。**
- [ ] **基建正式化**：wdio devDeps 写进 `package.json`+`package-lock`（spike 已证可行，现在正式收）；`wdio.conf.mjs`+specs 落 repo（`e2e/tier2/`，与 f40-suite 同级）；session-1-hop runner（hop-driver.ps1/run2.ps1）清理后落 repo + `e2e/tier2/README.md` 记跑法。**埋点/wdio 只 devDep，不进生产包（vite 已证剥离）。**
- [ ] 门禁：tsc 0 + vitest 595 不回归（加 devDep 不破前端构建）；f40-suite 既有断言不破。
- [ ] 不做：↗/真终端/SFTP/系统通知（Tier3 手动，见 F-Vwin C + MASTERPLAN）；真 claude 真出内容（hard-to-fixture）。

## 与主计划对接（共享面）
- **session-1 hop**（F-Vwin 也用）：`schtasks /create /it` + `/run`，最终形态=一个 repo 内 runner 脚本 + README；两 feature 共用，先在 E5 落成正式形态。
- **aya fixture**（E5b 复用 F-E1 的 fake-claude/gen-idle-tmux/daemon-wrapper）：不重造。
- **可注入 confirm**：E5 只需在 spec 桩 window.confirm（不动生产码）；真正的 `opts.confirm` 生产 seam 归 F-E4 孤儿（Linux）那轮，不在此提前改。

## 步骤
1. wdio devDeps 进 package.json（@wdio/cli/local-runner/mocha-framework/spec-reporter/webdriverio，锁版本）→ 本地 `npm install` 更新 lock → tsc/vitest 不回归。
2. spike 文件清理成 repo 形态：`e2e/tier2/{wdio.conf.mjs, test/shell-smoke.spec.mjs, run-in-session1.ps1, README.md}`。
3. 增量传 VM（scp 更新文件 or 新 bundle）→ VM `npm install`（补 wdio）→ 经 hop 跑 E5a → 回盘读 N passing。
4. E5b：VM app 配 aya 远端（用 E(b) key）+ aya 起 fake-claude fixture → 跑会话相关 spec；够不着则降级留档。
5. D 审计（行为等价/无生产码改动/埋点不漏）→ E→F→（可选）commit。

## 测试策略
断言优先 `browser.$(sel).isExisting()/getText()` 直读 DOM；快捷键发物理 code。confirm 用 `browser.execute` 桩。结果回盘读（别信内联）。E5a 是"WebView2 真渲染 + WebDriver 可驱"的活证；E5b 是会话流的 happy-path 薄验。

## 审计结果
（待 D 阶段填）

## 签收
- [ ] 代码审计（D）
- [ ] 工程审计（E）
- [ ] 主计划已更新（F）
