# 统一方案 v2（会话启动机制统一 + 终端同步 + 账号 UX）

基于枚举 agent 诊断（a6c54efe）+ 用户 REDIRECTION-v2 五条。F5(9d3a7e6)/F1(7680a43) 已 commit。本文件是主计划 v2，待用户审批。

## 一、诊断结论（restart/resume 为何经常失败）

现在有 **4 套 payload builder + 2 套账号模型 + 分叉的 @ccm_sid 设置**，不统一是所有 bug 的根：

- **根因1（最致命）**：cc-monitor「开新 Claude」(A1, remote-launch.ts:227/242-246) 跑**裸 claude、不设 @ccm_sid** → 自己起的会话 restart/resume 必撞守卫失败。
- **根因5**：终端 `cct` 起的会话名 `<dir>_cc`，被 restart 的 kill/send-keys 的 `cc-*` 白名单(tmux.rs:229)拒 → 能 attach 不能重启。
- **根因3**：A3 resume 把 @ccm_sid 焊死建时 sid、跑裸 claude 无 poller → /branch 漂移后失准。
- **根因2**：wrapper(ccm-wrapper.sh:13) 硬读 `~/.claude/sessions/`，无视 CLAUDE_CONFIG_DIR → 账号隔离下读错目录、@ccm_sid 不设。
- **根因4**：@ccm_sid 是 session 级选项、多 pane 失准。
- **账号模型大冲突**：bashrc `_cc_acct`(:251) = 凭据 swap 进单一 ~/.claude、互斥（旧方案）；cc-acct-iso = CLAUDE_CONFIG_DIR 隔离、并发（新方案）。两套并存打架。

## 二、统一架构（核心设计）

### 1. LaunchSpec + 单一 builder（R5 可扩展，别写死）
把 4 套 builder 收敛成**一个可组合的规格对象 + 一个 builder**：
```ts
interface LaunchSpec {
  mode: "new" | "resume" | "restart" | "attach";
  cwd: string;
  backend: "tmux" | "direct";       // +tmux 维度
  account: { configDir: string } | null;  // +账号维度（null=基座）
  launcher: string;                 // 默认 "claude"，用户可配（cc/cct/…）
  sid?: string;                     // resume/restart/attach
  tmuxName?: string;                // 解析/派生
  compactFirst?: boolean;
  // 以后加参数 = 加字段 + 一个修饰器，不再开新 builder
}
buildLaunch(spec): string  // = [账号前缀] + [代理前缀] + unset<nested> + launcher(+--resume sid)
```
- payload 由**有序修饰器链**组成（accountPrefix / proxyPrefix / unset / launcher / resumeFlag / …），新维度插一个修饰器即可。
- tmux backend 一律经 `SESSION_BACKEND.createRunAttach`，**统一在 create 分支 set @ccm_sid**。
- A2 直连 / A7 开终端：要么纳入（backend:"direct" 但标"不可管理"），要么明确标记不可 restart/resume（诚实）。

### 2. @ccm_sid 可靠性（修根因 1-5）
- **已知 sid（resume/restart）**：create 分支显式 `set-option @ccm_sid <sid>`（A3 已有，扩到所有 tmux create）。
- **未知 sid（new）**：launcher **统一走 ccm wrapper**（非裸 claude），由 wrapper 内 poller 回填 @ccm_sid → 修根因1。
- **wrapper 读账号感知路径**：`${CLAUDE_CONFIG_DIR:-$HOME/.claude}/sessions/$cpid.json` → 修根因2。
- **kill/send-keys 白名单**：从「名前缀 `cc-*`」改为「**@ccm_sid 命中即放行**」（restart 入口已用 @ccm_sid 精确守卫，名前缀二次设限纯误伤终端会话）→ 修根因5。
- /branch 漂移（根因3）：走 wrapper 后 poller 持续跟漂移，A3 焊死问题随之消解。

### 3. 账号模型统一
- 标准化到 **CLAUDE_CONFIG_DIR（cc-acct-iso 方向）**，全程序一套。
- bashrc 的 `_cc_acct` 凭据 swap 旧方案**该退役**——但**红线：不碰 ~/.bashrc** → cc-monitor 侧**检测到旧 swap 方案 + 打印迁移指引让用户自己删**，不代改。

### 4. 终端同步
- `cc/ccm/cct` 三个启动器纳入 `shared/ccm-wrapper.sh`（已是 monitor↔远端单一来源），由 cc-monitor 安装器统一部署；**launcher 可配置（默认 claude，用户填 cc/cct）**。
- 使「所有 tmux 起一套、所有直起一套、所有指定账号起一套」从同一份 shared 脚本 + 同一个 SESSION_BACKEND 座派生。

## 三、主计划 v2 功能顺序（重排）

已完成：F5 一键部署 ✓、F1 切号入口 ✓（R2 徽章待改）。

1. **U0 — 启动统一（最高优先，修 restart/resume）**：LaunchSpec + 单一 builder（可扩展）；A1 新起走 wrapper 设 @ccm_sid；白名单改 @ccm_sid 命中；wrapper 读 CLAUDE_CONFIG_DIR 感知路径。DoD：cc-monitor 新起/终端 cct 起的会话都能 restart/resume；换号重启对已知会话成功；tsc/vitest/cargo test 绿 + 真机验一次。
2. **U1 — 终端同步 + 账号模型统一**（原 F6 长大）：cc/ccm/cct → shared 脚本 + 可配置 launcher；检测旧 swap 方案给迁移指引；wrapper 账号感知。
3. **U2 — tab 徽章多账号启用即常显**（R2，改 F1 徽章）：多账号系统启用（≥2 账号）时每 tab 常显徽章；单账号/未启用不显。
4. **U3 — 右键菜单分级 flyout**（R4）：一级动作行「Resume ▸」「重启 ▸」+ ▸ 三角，悬停/点击展开二级选账号；context-menu 支持 submenu。
5. **F2′ — 移除对齐 UI**（简化：强制全局砍了 → 直接全清 ⇄/⚠k/alignAll/countAccountMismatches/命令面板对齐，换会话账号只留 U3 的 per-session）。
6. **F3 — 账号面板砍卡片**（manifest/configDir/verify/sync 收排障）。
7. **F4 — 加号一键化**。
8. **F7 — 账号额度用量（plan 窗口%）**。
9. Phase G 全量验收。

依赖：U0 是地基（其余 UX 建在统一启动上）；U1 依赖 U0 的统一 builder；F2′ 可与 U0 合（restart 路径本就重写）。

## 四、开放/风险
- U0 触碰 SESSION_BACKEND 座 + remote-launch 全部 builder + tmux.rs 白名单——**高危、改动面大**，严格行为等价 + 真机验证。红线「remote-launch CLAUDE_CONFIG_DIR 注入不动」需放宽为「注入语义保持、但收敛进统一 builder」（请用户确认这条红线调整）。
- wrapper 改 `shared/ccm-wrapper.sh` = monitor↔远端双写点，须 lockstep。
- 账号模型统一触及 cc-acct-iso 契约；bashrc 旧 swap 退役靠指引（不碰 rc）。
