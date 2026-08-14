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

## ★★ tmux 隔离：一律走 `tmux-shim.sh`（`C7i` 红线）

**任何会碰 tmux 的套件，隔离只有一种做法**：

```sh
TMUX_SHIM_SOCK=e2eYourSuite
# shellcheck source=e2e/tmux-shim.sh
. "$(cd "$(dirname "$0")" && pwd)/tmux-shim.sh"
trap 'tmux_shim_cleanup' EXIT
```

它把一个 `$BIN/tmux` shim 放进 PATH 最前，`exec` 真 tmux 并**强插 `-L <私有名>`**。
调用点一个字都不用改，连套件 shell out 出去的东西（`ccm` / `cc-spawn` 内部也裸调 tmux）
也一并覆盖。

**绝不用 `TMUX_TMPDIR` 做隔离** —— `$TMUX` 一有值就压过它。2026-08-11 一条探针就是这么把
用户**9 个真实 tmux 会话**打没的；`C7i` 因此逐字禁掉那条路。机检在
`src-tauri/src/e2e_gate_registry.rs`（零容忍、零例外）。

⚠ 要把隔离**传给被你拉起的子进程**（比如被监护的 daemon 自己会跑 `tmux ls`）时，
传的是 **shim 目录**、让它进子进程的 `PATH`，**不是**传 `TMUX_TMPDIR`
（`local-backend-supervise.sh` 是现成的样板：`CCM_E2E_TMUX_SHIM_BIN`）。

### 改完隔离之后怎么验（`P0d`/`P0e` 的协议，照着做）

1. **跑前**记用户真实 server 的快照 —— `tmux -L default ls`
   （**这条读命令自己也带选择器**：`C7i` 说的是「一律」）；
2. 跑套件；
3. **跑后**再记一次，`diff` 两份 —— **一个字都不该变**；
4. 确认自己那台收干净了：`tmux -L <私有名> ls` 应回 `no server running`。

08-12 实测两套：`restart-suite` **24 过/0 败**、`resume-suite` **17 过/0 败**，
真实 server 跑前跑后**均为 9 个会话、逐字未变**，私有 server 均已收。

## 一次全量真跑的台账（08-13）

本机（Linux + tmux 3.6）把**所有不需要 GUI/Windows 的套件**跑了一遍，每套都对照用户真实
tmux server（跑前跑后 `tmux -L default ls` 逐字对比，**9 个会话，每次都没变**）：

| 套件 | 读数 | 备注 |
|---|---|---|
| `restart-suite` | 24 过 / 0 败 | |
| `resume-suite` | 17 过 / 0 败 | |
| `inbound-daemon-frames` | 32 过 / 0 败 | |
| `resume-daemon-frames` | 7 过 / 0 败 | |
| `graylight-daemon-frames` | 12 过 / 0 败 | |
| `daemon-gate2-acceptance` | 35 过 / 0 败 | ★ 修了它自己开的方子（登记豁免），此前每跑必 RC=1 |
| `local-backend-supervise` | 7 过 / 0 败 | ★ 修前 7/1 —— 那条 `#[ignore]` 首跑就红，见 `P3 §0h-2` |
| `tmux-guarded-acceptance` | 14 过 / 0 败 | |
| `usage-probe-acceptance` | 11 过 / 0 败 | |
| `ccm-acceptance` | 19 过 / 0 败 | |
| `ccm-pretrust-acceptance` | 13 过 / 0 败 | |
| `ccm-print-parity` | 12 过 / 0 败 | |
| `ccm-contract-parity` | 61 过 / 0 败 | |
| `tmux-target-acceptance` | 26 过 / 0 败 | |
| `daemon-fork-session` | 10 过 / 0 败 | |
| `p3t-local-tmux` | 10 过 / 0 败 | |
| `cc-spawn-uplift` | **60 过 / 0 败**（08-13 更新） | ★ 修前 19/2 —— 它还在测 `P4b` 删掉的行为；`C15` 收编后 +10 条；08-13 再 +7（地址簿不许被抹 · 敲门不许打进别人屏幕） |
| `exec-bit-guard` | RC=0 | ⚠ 打了非阻断警告：`shared/cc-bus` 与 `~/.claude/skills/cc-bus` **已漂移** |
| `daemon-sessions-rewatch` | 4 过 / 0 败 | ★ 08-13 新增（`P0b-Y2`）：`sessions/` 被换 inode / 起初不存在 / 重建后立刻写 |
| `daemon-tmux-late-server` | 2 过 / 0 败 | ★ 08-13 新增（`P0b-Y2`）：**daemon 起得比 tmux server 早**（`#60` 现象 1 的根因） |
| `cc-bus-queue-drain` | 38 过 / 0 败 | ★ 08-13 新增：`cc-send` **消息没到时必须有人说话**——滞留队列没人管 + 收件人根本不存在。**本套不用 tmux** |
| `daemon-cc-bus` | 25 过 / 0 败 | ★ 08-13 新增（`P4f`）：daemon 的 `--bus-list` / `--bus-send` 真跑。**不用 tmux**；`CLAUDE_CONFIG_DIR` 与 `CC_BUS_HOME` 双沙箱 |
| **`graylight-suite`** | **3 过 / 0 败**（08-13） | ★★ 它**不再是「跑不了」的** —— 跑法见下方 `§ 全链套件怎么跑` |

**跑不了的（本机缺条件，不是没跑）**：`f40-suite` / `restart-daemon-frames` /
`ccm-cli.test`（要 Xvfb + 跑着的 dev app）· `ccm-rbind-title`（要 Windows 的 `wt.exe`）。
⚠ `graylight-suite` **08-13 起不在这一行里了** —— 它跑通了（3 过 / 0 败），跑法见下。

## 全链套件（`graylight-suite`）怎么跑〔08-13 实测记录〕

```
Xvfb :80 -screen 0 1400x900x24 &
DISPLAY=:80 HOME=<沙箱> CLAUDE_CONFIG_DIR=<沙箱>/.claude \
  RUSTUP_HOME=$HOME/.rustup CARGO_HOME=$HOME/.cargo npx tauri dev &
E2E_DISPLAY=:80 HOME=<沙箱> CLAUDE_CONFIG_DIR=<沙箱>/.claude bash e2e/graylight-suite.sh
```

四条**踩过才知道**的前提：

1. **`RUSTUP_HOME`/`CARGO_HOME` 要显式指回真路径**：沙箱 `HOME` 会把 rustup 的家一起换掉，
   `npx tauri dev` 报 `rustup could not find toolchain`。（那两个不是账号数据，与沙箱意图不冲突。）
2. **daemon 的 `.build_id` 标记文件名逐字是 `.build_id`**（同目录隐藏文件），
   不是 `<二进制名>.build_id` —— 写错的话 app 判「远端无版本标记」，
   **把 wrapper 覆盖成内嵌二进制**，daemon 就用**真** `~/.claude` 起来了。
3. **gate 丁 会因为盘上多余的 daemon 直接 ABORT**：先 `bash e2e/reap-orphan-daemons.sh` 看清楚，
   自己上一轮留下的按 pid 精确收，认不出的用 `E2E_ACK_DAEMONS=<pid>` 点名放行（要举证）。
4. **收尾要连 vite 一起收**：只收 `tauri dev` 那个 node 的话，vite 还占着 devUrl 端口，
   下一次起会报 `Port 24174 is already in use`。⇒ `ss -ltnp | grep :<端口>` 按 pid 收。

★★ **这一轮真跑逮到 3 处真问题**，全都是「平时没人跑」养出来的：
① `local_backend` 那条 `#[ignore]` 两天里被判成「验不出来」，其实是**测试与 daemon 不在同一台
   tmux server**（修法：给 daemon 一条挂着 shim 的 PATH，强插同一个 `-S`）；
② `daemon-gate2` **每跑必 RC=1**，因为它自己写的「造不出的名字应当…从表里说明」没人执行；
③ `cc-spawn-uplift` 还在断言 `cc-spawn` **已被删掉的复用行为**（`P4b` 的 E 阶段横扫漏了它）。

⇒ **「改了语义要跟改测试」这条纪律，在一套没人跑的测试上是失效的** —— 它不会红给你看。

## 跑法

```bash
# 前置:Xvfb + dev 实例(探针随 debug 构建自动就绪)
Xvfb :80 -screen 0 1920x1080x24 &
DISPLAY=:80 CCM_NO_DEVTOOLS=1 npx tauri dev &   # 等编译完、窗口出现

./e2e/f40-suite.sh          # 环境变量:E2E_DISPLAY / E2E_LOG / E2E_DRAIN_MAX_MS
```

### 哪些进 CI、哪些不进（G-A/G-C，2026-07-30）

真机套件**每一套都带断言数地板进 CI**，都经 `e2e/assert-pass-floor.sh <套件> <地板>` 跑
（抓不到 `合计 PASS=<n>` 那行也判红，见该脚本头注）。

> ★ **套数与地板值一律不抄在这里** —— 单一事实源是 `ci.yml` 里那些
> `run: bash e2e/assert-pass-floor.sh <套件> <地板>` 调用行。
> **本表只列套件名**，而「这份名单与 `ci.yml` 一致」由 `doc_copy_registry` 的
> `the_e2e_readme_suite_list_matches_ci` 机检（多一个、少一个、改名都红）。
>
> ⚠ 为什么不再留数字：本节此前逐字写着「副本漂了不会让任何东西变红，**所以只能靠这条提醒**」
> —— 然后它就又漂了三次（套数 15→19 · `ccm-cli` 44→53 · `usage-probe` 9→11），
> 而且上一轮 E82 订正时**也是这么写的**。**散文纪律等于没有纪律**（定框 E12）。

| job | 套件 |
|---|---|
| `e2e-tmux` | tmux-target · ccm-cli · ccm-print-parity · ccm-acceptance · ccm-pretrust · ccm-contract-parity · cc-spawn-uplift · cc-bus-queue-drain · restart · resume · ccm-rbind-title |
| `e2e-tmux-rust` | tmux-guarded · usage-probe · inbound-frames · daemon-gate2 · local-backend · graylight-frames · restart-frames · resume-frames · daemon-fork · daemon-sessions-rewatch · daemon-tmux-late-server · daemon-cc-bus |

**这些套件刻意都不进本地 `npm test`**（`gate-integrity` 开放问题 1 的决定）：
`npm test` 要保持「不需要 tmux / 不需要 daemon 就能跑」，否则每个开发动作都变重。

> **代价，如实写在这里**：**本地改了 `shared/ccm`（或 `src/account-restart.ts` /
> `src/remote-launch.ts` 这类被上面套件驱动的真源）时，`npm test` 不会有任何反应。**
> 要拿到信号得手跑，例如 `npm run test:restart` / `npm run test:ccm-cli`；
> 想连地板一起验就 `bash e2e/assert-pass-floor.sh restart 24`。
> ~~不手跑的话，**第一次发现是在 CI 上**。~~
>
> ⚠ **08-06 订正：那句已经不成立。**〔用 08-05〕裁定**不再 push**，而 `ci.yml` 只在
> `push` / `pull_request` 上触发 ⇒ **CI 至今没跑过**。今天不手跑的后果不是「CI 上才发现」，
> 是**没有任何一次发现**。这个前提由
> `shared_crate_registry.rs::the_premise_behind_three_honesty_boundaries_still_holds` 盯着
> （谁加了 `workflow_dispatch`/`schedule`，这句话与另外三条诚实边界都要一起重判）。
>
> ★ 而且**这些套件并非都要真 tmux**：实测有几套零依赖跑得通（清单与跑法以判据里的
> `LOCALLY_RUNNABLE` 为准，**此处不抄**）。所以「手跑」的成本比这段话当初以为的低。

**`graylight-suite`（全链级）不在上表那些套件里**：它断言的是**正在跑的 dev app** 写的
`monitor.*.log`，需要 GUI runner + 起整个 app —— 与本文件开头「跑法」那段要 Xvfb 的
原因相同（`ci.yml` 也已就 DOM e2e 论证过「大投入低 ROI」）。它**仍然可以本地跑**。

**`f40-suite`（渲染/滚动管线级）同样不在上表里，理由同规格**（U0 2026-08-01 补写）：
它要 Xvfb + 一个**正在跑的 `tauri dev`**（见本文件开头「跑法」），断言的是整机渲染行为
（启动门控 / 贴底 / 上翻补批 / fork 折叠 / 抖动密度绊线）。GUI runner 的投入
与 `graylight-suite` 是同一笔账。
>
> **它也喂不进 `assert-pass-floor.sh`**：该脚本抓的是 `合计 PASS=<n>` 那行，而 f40 不打印这行
> （`grep -n '合计 PASS' e2e/f40-suite.sh` 无命中）。它的断言数还随环境分支变（多组 `ok`/`bad`
> 互斥），**所以这里刻意不写一个具体条数** —— 本文件正文刚因为「抄来的数字过期」被订正过两次。

> **它此前是 `e2e/*.sh` 里唯一一个连 npm 脚本都没有的套件** —— 只能 `bash e2e/f40-suite.sh` 裸跑，
> 于是 `doc/RELEASING.md:21`「动过滚动/渲染管线就跑一遍」那条 checklist 在肌肉记忆上比别的都难执行。
> U0 补了 `npm run test:f40`。**补脚本 ≠ 进 CI**：它仍然是手动套件，前置照旧。

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
4b. **新建文件(P6a)**:头「新建文件」→ 输入新名字 → 目录里出现一个 0 字节文件;
    **再点一次、输入同一个名字 → 必须被拒(toast「同名已存在」),那份文件的内容不许变** ——
    机检只钉到「写命令没发出去」,「远端真的没被改」只有真机看得见;
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
