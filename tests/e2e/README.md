# E2E 套件

无 devtools/eval 通道(生产与 `CCM_NO_DEVTOOLS=1` 下 webview 不可注入)——断言数据
全部走 **DEV 探针 → 后端日志**:

- `src/frontend/ui/e2e-probe.ts`(仅 dev 构建,`import.meta.env.DEV` 门控):
  - 启动重放抖动探针:batch 窗口内逐 rAF 采样定点卡片 `getBoundingClientRect().top`
    的方向反转(INVARIANTS §21:scrollTop 单调,只测它发现不了抖动),批末落盘
    `[e2e] jitter frames=… reversals=… retargets=…`。
  - 状态快照:`Ctrl+Alt+F9` **或中键点状态栏**(headless 用——xdotool 的 XTEST
    合成键盘进不了 WebKitGTK webview,鼠标事件畅通)→ `[e2e] snapshot
    {sid,scrollTop,distBottom,pending,midBuffer,timeline,foldWraps,sentinel,err}`。
- 日志:`~/.cc-monitor/logs/monitor/monitor.<日期>.log`,grep `fe_perf`。
- 抖动指标 = **密度绊线**(反转/帧):守卫 snap 的整数 scrollTop 对分数行高布局有
  ±亚像素合法舍入摆动,幅度与 §21 病态同级、密度差一个量级——健康 ≈0.12-0.16,
  病态 ≈1.0,断言 ≤0.4(标定 2026-07-08,详 src/frontend/ui/e2e-probe.ts 头注释)。

## 网络：门禁里每一套跑在无网沙箱里

本机门禁把每一套关进一个新网络命名空间（`bwrap --unshare-net`，见 `tests/scripts/gate.sh` 头上那段）：
开发机回环上用户真在跑的服务（中转口 8788、常驻后端的监听口）一个都连不到。套件自己要的口一律
`bind 0` 现找、经环境交给被测进程，不写死、不靠产品的默认口 —— 靠默认口的断言在本机连上的是用户的服务，
只在 CI 上才红。单跑（`npm run test:<套件>`）不包。

## ★★ tmux 隔离：一律走 `tmux-shim.sh`（`C7i` 红线）

**任何会碰 tmux 的套件，隔离只有一种做法**：

```sh
# shellcheck source=tests/e2e/tmux-shim.sh
. "$(cd "$(dirname "$0")" && pwd)/tmux-shim.sh" e2eYourSuite
trap 'tmux_shim_cleanup' EXIT
```

它把一个 `$BIN/tmux` shim 放进 PATH 最前，`exec` 真 tmux 并**强插 `-L <私有名>`**。
私有名是**这一趟的**：`<前缀>-<工作树路径短哈希>-<本趟 pid>`（`e2e_run_name`，在 `$TMUX_SHIM_SOCK` 里）——
两棵工作树同时跑门禁、同一棵树里同时起两趟，谁收尾的 `kill-server` 都打不到别人。
自带 shim 的套件与 docker 台架（网络名、容器名）用 `. tmux-shim.sh --names-only` 只取名字，
套件里只写前缀、不写名字本身；写死的私有名字由 `e2e_gate_registry` 的扫描判据拦（`-L default` 那一处
canary 例外：它问的正是用户那台默认 server）。
调用点一个字都不用改，连套件 shell out 出去的东西（`ccm` / `cc-spawn` 内部也裸调 tmux）
也一并覆盖。

**绝不用 `TMUX_TMPDIR` 做隔离** —— `$TMUX` 一有值就压过它。2026-08-11 一条探针就是这么把
用户**9 个真实 tmux 会话**打没的；`C7i` 因此逐字禁掉那条路。机检在
`src/frontend/shell/src/e2e_gate_registry.rs`（零容忍、零例外）。

⚠ 要把隔离**传给被你拉起的子进程**（比如被监护的后端自己会跑 `tmux ls`）时，
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

## 全链套件的台架

GUI 那几套（`graylight-suite` · `f40-suite`）的台架用脚本搭，不手搓（手搓容易让后端用真 `~/.claude` 起来）：

```bash
bash tests/e2e/tier2-rig.sh setup     # 沙箱 ＋ config.json ＋ Xvfb（会自证 loopback ssh / 后端二进制 / build_id）
bash tests/e2e/tier2-rig.sh dev &     # dev 实例（HOME 指沙箱）
bash tests/e2e/tier2-rig.sh run       # graylight-suite ＋ f40-suite
bash tests/e2e/tier2-rig.sh teardown  # 按 pid 收 dev ＋ vite ＋ Xvfb
```

没有窗口管理器时 `xdotool getmouselocation` 报 `window:0`（指针与窗口关联不上），装 `openbox` 再跑。

## 全链套件（`graylight-suite`）手跑

```
Xvfb :80 -screen 0 1400x900x24 &
DISPLAY=:80 HOME=<沙箱> CLAUDE_CONFIG_DIR=<沙箱>/.claude \
  RUSTUP_HOME=$HOME/.rustup CARGO_HOME=$HOME/.cargo npx tauri dev &
E2E_DISPLAY=:80 HOME=<沙箱> CLAUDE_CONFIG_DIR=<沙箱>/.claude bash tests/e2e/graylight-suite.sh
```

四条**踩过才知道**的前提：

1. **`RUSTUP_HOME`/`CARGO_HOME` 要显式指回真路径**：沙箱 `HOME` 会把 rustup 的家一起换掉，
   `npx tauri dev` 报 `rustup could not find toolchain`。（那两个不是账号数据，与沙箱意图不冲突。）
2. **后端的 `.build_id` 标记文件名逐字是 `.build_id`**（同目录隐藏文件），
   不是 `<二进制名>.build_id` —— 写错的话 app 判「远端无版本标记」，
   **把 wrapper 覆盖成内嵌二进制**，后端就用**真** `~/.claude` 起来了。
3. **gate 丁 会因为盘上多余的后端直接 ABORT**：先 `bash tests/e2e/reap-orphan-backends.sh` 看清楚，
   自己上一轮留下的按 pid 精确收，认不出的用 `E2E_ACK_BACKENDS=<pid>` 点名放行（要举证）。
4. **收尾要连 vite 一起收**：只收 `tauri dev` 那个 node 的话，vite 还占着 devUrl 端口，
   下一次起会报 `Port 24174 is already in use`。⇒ `ss -ltnp | grep :<端口>` 按 pid 收。

★★ **这一轮真跑逮到 3 处真问题**，全都是「平时没人跑」养出来的：
① `local_backend` 那条 `#[ignore]` 两天里被判成「验不出来」，其实是**测试与后端不在同一台
   tmux server**（修法：给后端一条挂着 shim 的 PATH，强插同一个 `-S`）；
② `backend-gate2` **每跑必 RC=1**，因为它自己写的「造不出的名字应当…从表里说明」没人执行；
③ `cc-spawn-uplift` 还在断言 `cc-spawn` **已被删掉的复用行为**（`P4b` 的 E 阶段横扫漏了它）。

⇒ **「改了语义要跟改测试」这条纪律，在一套没人跑的测试上是失效的** —— 它不会红给你看。

## 跑法

```bash
# 前置:Xvfb + dev 实例(探针随 debug 构建自动就绪)
Xvfb :80 -screen 0 1920x1080x24 &
DISPLAY=:80 CCM_NO_DEVTOOLS=1 npx tauri dev &   # 等编译完、窗口出现

./tests/e2e/f40-suite.sh          # 环境变量:E2E_DISPLAY / E2E_LOG / E2E_DRAIN_MAX_MS
```

### 哪些进门禁、哪些不进

真机套件每一套在门禁 `tests/scripts/gate.sh` 里一行 `run_e2e <套件>`，经 `tests/e2e/assert-pass-floor.sh <套件>` 跑：
退出码 0、收尾 `合计 PASS=<n> FAIL=0`、`n > 0` 才算过（抓不到那行也判红，见该脚本头注）。
CI 的 e2e job 就是调门禁（`GATE_ONLY=e2e`），不另记一份名单。

> ★ **套数与地板值一律不抄在这里** —— 套件名单的唯一住址是门禁里那些 `run_e2e` 行；
> 每套断言几条只住在套件自己的输出里，门禁与 CI 都不钉这个数（几路同时加断言时不再撞数）。

> tmux 三道门的真机覆盖在 `backend-gate2-acceptance.sh`（真后端二进制 ＋ 真 tmux server，用例逐行来自
> `gate2-golden.tsv`）。

**这些套件刻意都不进本地 `npm test`**（`gate-integrity` 开放问题 1 的决定）：
`npm test` 要保持「不需要 tmux / 不需要后端就能跑」，否则每个开发动作都变重。

> **代价**：本地改了后端的 `ccm`（或别的被上面套件驱动的真源）时，`npm test` 不会有任何反应。
> 要拿到信号得手跑，例如 `npm run test:restart` / `npm run test:ccm-cli`；
> 想连门禁那套判法一起验就 `bash tests/e2e/assert-pass-floor.sh restart`。
> ~~不手跑的话，**第一次发现是在 CI 上**。~~
>
> ⚠ **08-06 订正：那句已经不成立。**〔用 08-05〕裁定**不再 push**，而 `ci.yml` 只在
> `push` / `pull_request` 上触发 ⇒ **CI 至今没跑过**。今天不手跑的后果不是「CI 上才发现」，
> 是**没有任何一次发现**。这个前提由
> `shared_crate_registry_tests.rs::the_premise_behind_three_honesty_boundaries_still_holds` 盯着
> （谁加了 `workflow_dispatch`/`schedule`，这句话与另外三条诚实边界都要一起重判）。
>
> ★ 而且这些套件**本机都跑得动**（隔离的 tmux socket 或根本不碰 tmux）：本机门禁
> `tests/scripts/gate.sh` 的 `run_e2e` 每一套都跑（清单以那里为准，**此处不抄**）。

**`graylight-suite`（全链级）不在门禁里**：它断言的是**正在跑的 dev app** 写的
`monitor.*.log`，需要 GUI runner + 起整个 app —— 与本文件开头「跑法」那段要 Xvfb 的
原因相同（`ci.yml` 也已就 DOM e2e 论证过「大投入低 ROI」）。它**仍然可以本地跑**。

**`f40-suite`（渲染/滚动管线级）同样不在门禁里，理由同规格**（U0 2026-08-01 补写）：
它要 Xvfb + 一个**正在跑的 `tauri dev`**（见本文件开头「跑法」），断言的是整机渲染行为
（启动门控 / 贴底 / 上翻补批 / fork 折叠 / 抖动密度绊线）。GUI runner 的投入
与 `graylight-suite` 是同一笔账。
>
> **它也喂不进 `assert-pass-floor.sh`**：该脚本抓的是 `合计 PASS=<n>` 那行，而 f40 不打印这行
> （`grep -n '合计 PASS' tests/e2e/f40-suite.sh` 无命中）。它的断言数还随环境分支变（多组 `ok`/`bad`
> 互斥），**所以这里刻意不写一个具体条数**。

> 手跑：`npm run test:f40`（手动套件，不进 CI；`src/doc/RELEASING.md` 发版清单里「动过滚动 / 渲染管线就跑一遍」指的就是它）。

**单实例串行**:fixture 目录/cwd 固定名(`-tmp-e2e-fork`)且 `touch src/frontend/ui/main.ts` 会触发
全窗口 reload——并发跑两个套件会互删 fixture、互触发重放,结果不可信。

套件场景:①启动门控(rendered≪deferred)+ drain 阈值 + 抖动密度绊线;②贴底快照;
③上翻补批(active + 厚账 tab 两处,pending 下降断言);④逐 tab 点击切换贴底;
⑤合成 fork 会话折叠段断言——**fixture 必须伴生活进程 pidfile 且 pidfile 先落**
(watcher 只 emit 活跃会话,Batch5-F20;jsonl 先落会被 process_file 抢跑跳过,实测);
⑥trap 清理(pidfile/宿主进程/项目目录)。
无 WM 注意:主窗必须先 `xdotool windowraise`(tear-off 浮窗会按 z 序吃掉指针事件)。

## auto-e2e:gray-light 会话生命周期(F-E0 基建 + F-E1)

跨进程整链(后端→帧→emitter→前端灯)的 `[e2e] tab-state` 断言,单测碰不到。**红线:后端
零行为改动**——只加下列 `tests/e2e/` fixture(外部 wrapper/shim)+ 前端 DEV 探针(`import.meta.env.DEV`
门控,生产零包含)。探针出口:`tabs.emitTabStateProbe`(markTmuxIdle/archiveTab/reviveTab/ensureTab
清灰四真值点)+ `tabs.debugSessionsSnapshot()`(Ctrl+Alt+F10 / 中键账号 chip → `[e2e] sessions`)。

fixtures:
- `fake-claude`——确定性 claude shim:记 argv+env → `$CLAUDE_CONFIG_DIR/argv.log`;写自身
  `sessions/<PID>.json` pidfile(procStart 喂后端判活)+ 一条 `projects/.../<sid>.jsonl`;前台
  `sleep` 常驻(kill 本 PID → 后端判 claude 死)。**默认落 /tmp/e2e-remote-claude,绝不写真 ~/.claude**。
- `gen-idle-tmux.sh <sid>`——`tmux new-session -d -s cc-<sid8> "…fake-claude…; exec sh"` + `set-option
  @ccm_sid <sid>`。`exec sh` 让 kill fake-claude 后 pane 落回 shell(tmux 会话+@ccm_sid 仍在=灰灯态)。
  **CLAUDE_CONFIG_DIR 必须内联进 tmux 命令串**(new-session 不继承本 shell env,老坑)。
- `backend-wrapper.sh`——`exec env CLAUDE_CONFIG_DIR=/tmp/e2e-remote-claude <backend> "$@"`,隔离远端
  读的目录(防本地会话双 tab)。

两级跑法(先建后端,或全链跑 app):

1. **backend-frame 级(无 GUI,最稳,后端半场)**:`bash tests/e2e/graylight-backend-frames.sh`
   (需仓内 debug 后端;缺则 `CCM_E2E_BACKEND=<某个 p1p+ 的 cc-monitor-backend>`)。断言后端 stdout 帧:
   `session_added` → (kill fake-claude) `session_removed` **且** `session_state` = `reconnectable`
   (=灰) → (kill-session) `session_state` = `ended`(=归档边沿；原看 `tmux_sessions` 快照帧，那一帧删了)。

2. **全链级(GUI + loopback SSH)**:前置同 f40(Xvfb + dev 实例)+ config.json 配一个 loopback 远端,
   `backendPath` 指向 `backend-wrapper.sh`。然后 `E2E_DISPLAY=:80 bash tests/e2e/graylight-suite.sh`。断言 monitor
   日志:`[e2e] tab-state … liveness=dead recoverability=attachable`(可重连;原先是 `status=live tmuxIdle=1`)→
   `… liveness=dead recoverability=resumable`(已结束)。**★ app 会自动部署后端**:backendPath 同目录须放一个 `.build_id`(内容=app
   **内嵌** 后端的 build_id;那条按旁挂标记判的路已删,今天后端读字节自报的身份戳,见后端 `control/deploy_plan.rs` 的 `identity_decision`——不是 monitor 的「我这一版」),否则
   app 会用内嵌二进制覆盖写 backendPath(把 wrapper 冲掉)。杀 fake-claude **前须等 > 一个 8s 发帧周期**,
   让 app 先收到含 @ccm_sid 的 `TmuxSessions` 帧,否则 removed 到达时 tmux 账本无此 sid → 判 Archive 丢灰。

## auto-e2e:resume idle 就地复用(F-E2,#75/#76)

跨进程验 resume:远端 archived/idle-tmux 会话 resume 时**复用原会话名 `cc-<sid8>`、不产 `cc-<sid8>-N`
孤儿**(治 #76),且账号注入正确的 `CLAUDE_CONFIG_DIR`(治 #75)。复用 F-E0 的 fake-claude/gen-idle-tmux。

**★ 诚实分层**:GUI resume 一键拉起要真开一个终端窗口(`platform/terminal.rs::open_local`),无头门禁里不开窗。因此 argv/孤儿断言的诚实天花板
= **命令级**:直接驱**生产渲染链**(生产 `launch-requests.ts::plan*` → 生产 `buildLaunchRenderRequest` →
生产 Rust `render_launch_payload`,经 `resume-cmd-driver.ts` → `launch-render-driver.ts`,不重写)拿到 app **真正会跑**的命令串,再把该串真跑到真 tmux + fake-claude,
断言 argv.log(`--resume <sid>` + `CLAUDE_CONFIG_DIR`)与 `tmux ls` 孤儿数。复活(灰→live)的**检测**由
后端判活边沿断言(后端半场)。本地 resume(`resume_history_session`)同为 Windows-only,Linux 不可执行。

fixtures / 驱动:
- `resume-cmd-driver.ts`——tsx 驱动器,命令串经 `launch-render-driver.ts` 走生产渲染链(生产 TS 请求 → 生产 Rust
  `render_launch_payload`,后者经 `cargo test --lib emit_launch_render_for_e2e -- --ignored` 那个数据出口),
  账号解析 import 真实 `accounts.ts`,打印 app 真会跑的 resume 命令串 / 账号解析结果(套件据其 stdout 断言并真跑到 tmux)。
  ⚠ 第一次跑要编 monitor 的 lib 测试(几分钟);之后走缓存。
- `fake-claude` 必须可执行(`chmod +x`;直接被 `gen-idle-tmux` 内联 exec)——F-E0 提交时误落 100644,已修 100755。

两级跑法(都无需 GUI,全自动):

1. **命令级整合(最全,主套件)**:`bash tests/e2e/resume-suite.sh`。逐边界:①`resume-cmd-driver.ts` 取真源命令串,
   ②断言命令形状(复用名/无 new-session/无 -N/`CLAUDE_CONFIG_DIR` 前缀),③真 send-keys 进 idle pane 的 sh,
   ④断言 argv.log(sid 命中行的 `CLAUDE_CONFIG_DIR` + `--resume`)与 `tmux list-sessions` 孤儿计数。覆盖:idle
   就地复用无孤儿 / 无 tmux 新建注账号 / 带 pin 落 X 目录(两隔离账号) / 不带 pin 走基座
   落当前工作账号 / 重复 resume 幂等(create-gate 短路) / tmux 消失回退 / 会话仍 live 守卫不误动。
2. **backend-frame 复活清灰(后端半场)**:`bash tests/e2e/resume-backend-frames.sh`(需仓内 debug 后端;缺则
   `CCM_E2E_BACKEND=<某 p1p+ 的 cc-monitor-backend>`)。序列 `SessionAdded`(live)→(kill fake-claude)
   `SessionRemoved` + tmux 帧仍含 @ccm_sid(灰)→(真源就地 resume 命令复用原名)`SessionAdded` **再现**
   = 后端灰→live 复活边沿;全程 tmux 单会话无 `-N` 孤儿。

## auto-e2e:换号重启(帧命令 `session-restart`)

整条在后端：真后端二进制经流的入方向收 `session-restart`，私有 tmux server ＋ 假 claude（`CCM_FAKE_COMPACT`：收到 `/compact` 写一条压缩摘要）。
验：先压缩等到摘要 ⇒ 同名用新号起、新进程报出（新号目录、经 ccm、带中转地址）· 压缩超时照常重启（本机那一形）· 停不了 ⇒ 不起新的 ·
号选不了 ⇒ 什么都不动 · 等压缩时撤单 ⇒ 不停不起。跑：`bash tests/e2e/restart-suite.sh`。

## auto-e2e:Tier2 Windows DOM 冒烟(F-E5)

真 WebView2 DOM 冒烟(WebDriver + session-1 hop),独立文档见 `tests/e2e/tier2/README.md`。E5a 裸壳 6/6(壳元素/状态文案/6 顶栏钮可点/H·G·Ctrl+K overlay 开+Escape 关);E5b 会话相关未做、路径已记档。

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
合成;脚本化需可控地断开/重连后端且不污染真实会话镜像。已由单测钉住路由与
批末排序挂载(`tabs.vitest.ts`「R-1」用例);人工验证流程:
1. 远端机器上对某会话 tmux 挂起 monitor 连接(断网/杀后端进程);
2. 该会话继续产出 >600 行;
3. 恢复连接 → 观察该 tab:内容一次性补齐、贴底不逐帧抖、无 NotFoundError。

**WebView2(生产)复核**:WebKitGTK 无 overflow-anchor,补批补偿路径两端语义不同;
发版前在 Windows 真机把 ①③④ 手动过一遍。
