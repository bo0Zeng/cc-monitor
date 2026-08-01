# E2E 套件(Batch13-F40 起)

无 devtools/eval 通道(生产与 `CCM_NO_DEVTOOLS=1` 下 webview 不可注入)——断言数据
全部走 **DEV 探针 → 后端日志**:

- `src/e2e-probe.ts`(仅 dev 构建,`import.meta.env.DEV` 门控):
  - 启动重放抖动探针:batch 窗口内逐 rAF 采样定点卡片 `getBoundingClientRect().top`
    的方向反转(INVARIANTS §21:scrollTop 单调,只测它发现不了抖动),批末落盘
    `[e2e] jitter frames=… reversals=… retargets=…`。
  - 状态快照:`Ctrl+Alt+F9` **或中键点状态栏**(headless 用——xdotool 的 XTEST
    合成键盘进不了 WebKitGTK webview,鼠标事件畅通)→ `[e2e] snapshot
    {sid,scrollTop,distBottom,pending,midBuffer,timeline,foldWraps,sentinel,err}`。
- 日志:`~/.claude/claudecode-frontend/logs/monitor.<日期>.log`,grep `fe_perf`。
- 抖动指标 = **密度绊线**(反转/帧):守卫 snap 的整数 scrollTop 对分数行高布局有
  ±亚像素合法舍入摆动,幅度与 §21 病态同级、密度差一个量级——健康 ≈0.12-0.16,
  病态 ≈1.0,断言 ≤0.4(标定 2026-07-08,详 src/e2e-probe.ts 头注释)。

## 跑法

```bash
# 前置:Xvfb + dev 实例(探针随 debug 构建自动就绪)
Xvfb :80 -screen 0 1920x1080x24 &
DISPLAY=:80 CCM_NO_DEVTOOLS=1 npx tauri dev &   # 等编译完、窗口出现

./e2e/f40-suite.sh          # 环境变量:E2E_DISPLAY / E2E_LOG / E2E_DRAIN_MAX_MS
```

### 哪些进 CI、哪些不进（G-A/G-C，2026-07-30）

**13 套带断言数地板进了 CI**，每步都经 `e2e/assert-pass-floor.sh <套件> <地板>` 跑
（地板值写在 `ci.yml` 的调用行上；抓不到 `合计 PASS=<n>` 那行也判红，见该脚本头注）：

| job | 套件（地板） |
|---|---|
| `e2e-tmux` | tmux-target 26 · ccm-cli 44 · ccm-print-parity 12 · ccm-acceptance **19** · ccm-pretrust 13 · cc-spawn-uplift 21 · restart 24 · resume 17 · **ccm-rbind-title 8** |
| `e2e-tmux-rust` | tmux-guarded 14 · usage-probe **9** · graylight-frames **12** · restart-frames 5 · resume-frames 7 · **daemon-fork 10** |

> **E82（2026-08-01）订正**：上表原写 `ccm-acceptance 15` / `usage-probe 7` / `graylight-frames 5`，
> 与 `ci.yml` 里真正的调用行（19 / 9 / 12）对不上，且漏了 G2 新增的两套。
> **地板值的单一事实源是 `ci.yml` 的调用行**（那里有逐个 grep 的反向自检）；本表是给人读的副本，
> 改地板时**两处都要动** —— 副本漂了不会让任何东西变红，所以只能靠这条提醒。

**这 13 套刻意都不进本地 `npm test`**（`gate-integrity` 开放问题 1 的决定）：
`npm test` 要保持「不需要 tmux / 不需要 daemon 就能跑」，否则每个开发动作都变重。

> **代价，如实写在这里**：**本地改了 `shared/ccm`（或 `src/account-restart.ts` /
> `src/remote-launch.ts` 这类被上面套件驱动的真源）时，`npm test` 不会有任何反应。**
> 要拿到信号得手跑，例如 `npm run test:restart` / `npm run test:ccm-cli`；
> 想连地板一起验就 `bash e2e/assert-pass-floor.sh restart 24`。
> 不手跑的话，**第一次发现是在 CI 上**。

**`graylight-suite`（全链级）不在这 13 套里**：它断言的是**正在跑的 dev app** 写的
`monitor.*.log`，需要 GUI runner + 起整个 app —— 与本文件开头「跑法」那段要 Xvfb 的
原因相同（`ci.yml` 也已就 DOM e2e 论证过「大投入低 ROI」）。它**仍然可以本地跑**。

### tmux 隔离（E41 已解，2026-07-30）

`graylight-*` / `restart-*` / `resume-*` 六套此前**裸调 tmux**，会直接操作开发者默认
socket 上的真实会话（BACKLOG E41）。现在每套开头都钉住自己的 server：

```bash
unset TMUX TMUX_PANE
TMUX_TMPDIR="$(mktemp -d /tmp/e2e-sock.XXXXXX)"; export TMUX_TMPDIR
```

**两件事缺一不可**（实测）：
- **`unset TMUX`** —— 从一个 tmux 会话里跑套件时，`$TMUX` 会让客户端连**外层那台
  server** 并**完全忽略 `TMUX_TMPDIR`**。这才是 E41 的实质，不是「没写 `-L`」。
- **`TMUX_TMPDIR` 必须是短路径** —— unix socket 路径上限 108 字节，长目录会报
  `File name too long`。

收尾只用 `-S <私有 socket> kill-server` 收自己那台；**绝不裸 `kill-server`**
（万一隔离没生效，裸的那个会打到开发者的 server 上）。

**单实例串行**:fixture 目录/cwd 固定名(`-tmp-e2e-fork`)且 `touch src/main.ts` 会触发
全窗口 reload——并发跑两个套件会互删 fixture、互触发重放,结果不可信。

套件场景:①启动门控(rendered≪deferred)+ drain 阈值 + 抖动密度绊线;②贴底快照;
③上翻补批(active + 厚账 tab 两处,pending 下降断言);④逐 tab 点击切换贴底;
⑤合成 fork 会话折叠段断言——**fixture 必须伴生活进程 pidfile 且 pidfile 先落**
(watcher 只 emit 活跃会话,Batch5-F20;jsonl 先落会被 process_file 抢跑跳过,实测);
⑥trap 清理(pidfile/宿主进程/项目目录)。
无 WM 注意:主窗必须先 `xdotool windowraise`(tear-off 浮窗会按 z 序吃掉指针事件)。

## auto-e2e:gray-light 会话生命周期(F-E0 基建 + F-E1)

跨进程整链(daemon→帧→emitter→前端灯)的 `[e2e] tab-state` 断言,单测碰不到。**红线:daemon
零行为改动**——只加下列 `e2e/` fixture(外部 wrapper/shim)+ 前端 DEV 探针(`import.meta.env.DEV`
门控,生产零包含)。探针出口:`tabs.emitTabStateProbe`(markTmuxIdle/archiveTab/reviveTab/ensureTab
清灰四真值点)+ `tabs.debugSessionsSnapshot()`(Ctrl+Alt+F10 / 中键账号 chip → `[e2e] sessions`)。

fixtures:
- `fake-claude`——确定性 claude shim:记 argv+env → `$CLAUDE_CONFIG_DIR/argv.log`;写自身
  `sessions/<PID>.json` pidfile(procStart 喂 daemon 判活)+ 一条 `projects/.../<sid>.jsonl`;前台
  `sleep` 常驻(kill 本 PID → daemon 判 claude 死)。**默认落 /tmp/e2e-remote-claude,绝不写真 ~/.claude**。
- `gen-idle-tmux.sh <sid>`——`tmux new-session -d -s cc-<sid8> "…fake-claude…; exec sh"` + `set-option
  @ccm_sid <sid>`。`exec sh` 让 kill fake-claude 后 pane 落回 shell(tmux 会话+@ccm_sid 仍在=灰灯态)。
  **CLAUDE_CONFIG_DIR 必须内联进 tmux 命令串**(new-session 不继承本 shell env,老坑)。
- `daemon-wrapper.sh`——`exec env CLAUDE_CONFIG_DIR=/tmp/e2e-remote-claude <daemon> "$@"`,隔离远端
  读的目录(防本地会话双 tab)。

两级跑法(先建 daemon,或全链跑 app):

1. **daemon-frame 级(无 GUI,最稳,后端半场)**:`bash e2e/graylight-daemon-frames.sh`
   (需仓内 debug daemon;缺则 `CCM_E2E_DAEMON=<某个 p1p+ 的 cc-monitor-remote>`)。断言 daemon stdout 帧:
   `session_added` → (kill fake-claude) `session_removed` **且** `tmux_sessions.raw` 仍含 `@ccm_sid`
   (=灰,Idle 非 Archive) → (kill-session) `tmux_sessions` 不再含 sid(=归档触发边沿)。

2. **全链级(GUI + loopback SSH)**:前置同 f40(Xvfb + dev 实例)+ config.json 配一个 loopback 远端,
   `daemonPath` 指向 `daemon-wrapper.sh`。然后 `E2E_DISPLAY=:80 bash e2e/graylight-suite.sh`。断言 monitor
   日志:`[e2e] tab-state … status=live tmuxIdle=1`(灰,该行 status=live 同时证明变灰前是 live)→
   `… status=archived`。**★ app 会自动部署 daemon**:daemonPath 同目录须放一个 `.build_id`(内容=app
   **内嵌** daemon 的 build_id,见 `sftp.rs::deploy_decision`——不是 `EXPECTED_DAEMON_BUILD_ID`),否则
   app 会用内嵌二进制覆盖写 daemonPath(把 wrapper 冲掉)。杀 fake-claude **前须等 > 一个 8s 发帧周期**,
   让 app 先收到含 @ccm_sid 的 `TmuxSessions` 帧,否则 removed 到达时 tmux 账本无此 sid → 判 Archive 丢灰。

## auto-e2e:resume idle 就地复用(F-E2,#75/#76)

跨进程验 resume:远端 archived/idle-tmux 会话 resume 时**复用原会话名 `cc-<sid8>`、不产 `cc-<sid8>-N`
孤儿**(治 #76),且账号注入正确的 `CLAUDE_CONFIG_DIR`(治 #75)。复用 F-E0 的 fake-claude/gen-idle-tmux。

**★ 诚实分层(硬结构限)**:Linux headless 的 GUI resume **结构性不可执行**——一键拉起走
`launch.rs::launch_powershell_window`,该函数 `#[cfg(not(windows))]` 直接 `Err("拉起终端窗口仅支持
Windows")`,故 app 里点 resume 在 Linux 必回退剪贴板、**绝不真执行**命令。因此 argv/孤儿断言的诚实天花板
= **命令级**:直接驱**真源** builder(`remote-launch.ts` 的 `buildResumeIntoExistingTmuxCmd` 等,经
`resume-cmd-driver.ts` import,不重写)拿到 app **真正会跑**的命令串,再把该串真跑到真 tmux + fake-claude,
断言 argv.log(`--resume <sid>` + `CLAUDE_CONFIG_DIR`)与 `tmux ls` 孤儿数。复活(灰→live)的**检测**由
daemon 判活边沿断言(后端半场)。本地 resume(`resume_history_session`)同为 Windows-only,Linux 不可执行。

fixtures / 驱动:
- `resume-cmd-driver.ts`——tsx 驱动器,import 真实 `remote-launch.ts`/`accounts.ts`,打印 app 真会跑的
  resume 命令串 / 账号解析结果(#75/#76 的修复活在这些函数里,套件据其 stdout 断言并真跑到 tmux)。
- `fake-claude` 必须可执行(`chmod +x`;直接被 `gen-idle-tmux` 内联 exec)——F-E0 提交时误落 100644,已修 100755。

两级跑法(都无需 GUI,全自动):

1. **命令级整合(最全,主套件)**:`bash e2e/resume-suite.sh`。逐边界:①`resume-cmd-driver.ts` 取真源命令串,
   ②断言命令形状(复用名/无 new-session/无 -N/`CLAUDE_CONFIG_DIR` 前缀),③真 send-keys 进 idle pane 的 sh,
   ④断言 argv.log(sid 命中行的 `CLAUDE_CONFIG_DIR` + `--resume`)与 `tmux list-sessions` 孤儿计数。覆盖:idle
   就地复用无孤儿 / 无 tmux 新建注账号 / 带 pin 落 X 目录(两隔离账号) / 不带 pin 走基座 + `resolveFollowAccount`
   落当前工作账号 / 重复 resume 幂等(create-gate 短路) / tmux 消失回退 / 会话仍 live 守卫不误动。
2. **daemon-frame 复活清灰(后端半场)**:`bash e2e/resume-daemon-frames.sh`(需仓内 debug daemon;缺则
   `CCM_E2E_DAEMON=<某 p1p+ 的 cc-monitor-remote>`)。序列 `SessionAdded`(live)→(kill fake-claude)
   `SessionRemoved` + tmux 帧仍含 @ccm_sid(灰)→(真源就地 resume 命令复用原名)`SessionAdded` **再现**
   = 后端灰→live 复活边沿;全程 tmux 单会话无 `-N` 孤儿。

## auto-e2e:换号重启编排(F-E3,#68/#69)

命令级 + daemon-frame 验优雅换号:`compact→exit→kill→resume(新账号)` 序列、resume 落新账号 `CLAUDE_CONFIG_DIR`、失败中止语义(kill 失败不续 resume / resume 未起不记账)、批量对齐 idle/busy 分流。诚实分层同 F-E2(GUI 结构性不可执行 → 命令级天花板)。
- `restart-cmd-driver.ts` + `restart-shims/`(ESM loader 只重定向 Tauri IPC 边界到真 tmux+fake-claude,其余全真源;含 kill/resume 失败注入)。
- 跑:`bash e2e/restart-suite.sh`(命令级 24/0) + `bash e2e/restart-daemon-frames.sh`(5/0:旧号 `SessionRemoved`→新号 `SessionAdded` 迁移、无孤儿)。批量对齐 idle/busy 另由 `tabs.vitest.ts`「account-ux U6」覆盖。

## auto-e2e:Tier2 Windows DOM 冒烟(F-E5)

真 WebView2 DOM 冒烟(WebDriver + session-1 hop),独立文档见 `e2e/tier2/README.md`。E5a 裸壳 6/6(壳元素/状态文案/6 顶栏钮可点/H·G·Ctrl+K overlay 开+Escape 关);E5b 会话相关未做、路径已记档。

## 人工场景(未脚本化,原因与流程)

**F47+F48 SFTP 文件面板(Windows 真机)**:传输/拖入/打开终端是平台交互,Linux e2e 无法覆盖。
1. 设置卡某台远端点「文件」→ 面板列出该 host 文件(面包屑可点、目录在前);
2. 下载(文件行「下载」→ 选本地落点,进度条走完)/上传(头「上传」选本地文件,覆盖前确认);
3. 从资源管理器拖文件进面板 → 上传到当前目录(dragover 虚线);
4. 新建目录/改名/删除(删除二次确认回显真名);目录 Pin 书签点跳;
5. 「在此打开终端」→ wt.exe 起 ssh 落到当前目录;非 UTF-8 名文件行写按钮灰置;
6. 拒写验证:导航到 ~/.claude/projects 试删/传 jsonl → 应被守卫拒(提示用历史浏览器)。

**F42 turn-end 系统通知(真机观感)**:窗口切到后台,让某会话跑完一轮 → 应出系统通知
「Claude 完成一轮 — <tab 标题>」;窗口在前台时不应有通知;启动 monitor(历史重放)不应放礼花。

**F41 远端一键 resume(Windows 真机)**:触发面 = wt.exe/PowerShell 拉起 + Windows OpenSSH,
本仓 e2e 跑在 Linux 无法覆盖。人工验证流程(装含 F41 的版本后):
1. 远端某会话结束(tab 变灰)→ tab 右键「Resume」→ 应弹出新终端窗口自动 ssh 并 resume,
   cwd 正确、cc-monitor 阅读器随之点亮同一 tab;历史浏览器远端条目 ↺ 同理;
2. 变体 a(回退路径):临时把该主机配置改错(如 label 拼错的 origin 不存在)→ 右键 Resume
   应 toast「拉起失败,已复制 resume 命令」且剪贴板有裸命令;
3. 变体 b(双引号 launcher):设置「远端 resume 命令」为 `cc --allowedTools "Bash(*)"` →
   一键应主动回退复制(校验拒绝),改成单引号写法后应正常拉起。

**chunked 大增量批(R-1 缓冲)**:触发面 = 远端 SSH 重连 chunked 重放(末块先发)+
离线期 >600 行增量——本地 watcher 追加是升序到达,按构造不产生中部插入,无法本地
合成;脚本化需可控地断开/重连 daemon 且不污染真实会话镜像。已由单测钉住路由与
批末排序挂载(`tabs.vitest.ts`「R-1」用例);人工验证流程:
1. 远端机器上对某会话 tmux 挂起 monitor 连接(断网/杀 daemon 进程);
2. 该会话继续产出 >600 行;
3. 恢复连接 → 观察该 tab:内容一次性补齐、贴底不逐帧抖、无 NotFoundError。

**WebView2(生产)复核**:WebKitGTK 无 overflow-anchor,补批补偿路径两端语义不同;
发版前在 Windows 真机把 ①③④ 手动过一遍。
