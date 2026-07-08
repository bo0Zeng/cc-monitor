# 开发环境与调试

本机起 cc-monitor dev 模式 + 调试技巧。

生产构建 → [BUILDING.md](BUILDING.md)。发版 → [RELEASING.md](RELEASING.md)。

---

## 前置依赖

| 工具 | 版本 | 检查 |
|---|---|---|
| **Node.js** | LTS (≥ 18) | `node -v` |
| **Rust** | stable (≥ 1.75) | `rustc --version` |
| **MSVC Build Tools 2022** | 含 VCTools workload | `where link.exe` 应找到 MSVC 的 link |
| **WebView2 Runtime** | Win11 自带 / Win10 [手装](https://developer.microsoft.com/microsoft-edge/webview2/) | — |

`scripts/run.ps1` 走 `vswhere.exe` 自动找 MSVC（非默认路径也行），无需手动 vcvars。

---

## 起 dev server

```powershell
cd D:\path\to\cc-monitor

npm install                                              # 一次性
powershell -NoProfile -File scripts\run.ps1 dev          # 弹 1100x800 窗口
```

直接 `npx tauri dev` 也行，**前提是当前 PowerShell 已注入 vcvars**（否则 link.exe 找的是 Git Bash 的 GNU coreutils 假冒，编译挂）。

### dev 模式的特殊行为

- **自动打开 DevTools**（`lib.rs::setup()` 内 `#[cfg(debug_assertions)] window.open_devtools()`），可看前端 console
- **HMR**：保存 `src/` 下 TS / CSS 会触发 vite HMR 自动刷新前端；保存 `src-tauri/` 下 Rust 会触发增量 cargo build + 重启 monitor
- **`main.ts` 强制 full reload**：HMR 检测到任何 TS 改动直接 `location.reload()`，不做部分热替换。避免长跑监控时旧 listener 与新代码并存导致消息重复 / event_replay 状态不一致

### 其它常用命令

```powershell
powershell -NoProfile -File scripts\run.ps1 check    # cargo check
powershell -NoProfile -File scripts\run.ps1 clean    # cargo clean
powershell -NoProfile -File scripts\run.ps1 build    # 生产构建（详 BUILDING.md）
```

`scripts/run.ps1` 子命令清单 → [`../scripts/README.md`](../scripts/README.md)。

---

## 端口冲突

### 默认 dev 端口 24174

如果启动报（端口号是当时的 dev 端口）：
```
Error: listen EACCES: permission denied ::1:24174
```

**原因**：Windows 的 Hyper-V / WSL2 / WinNAT 把一段段端口加进**动态保留范围**，应用层无法 bind——netstat 看不到占用进程，但 listen syscall 失败。保留段在重启后会重新分配，所以「昨天能跑今天不行」很正常。

**为什么默认选 24174**：实测保留段集中在较低区间（用 `netsh int ipv4 show excludedportrange protocol=tcp` 可看，通常 ~1000–12500），而系统 ephemeral 段从 49152 起。24174 落在「保留段之上、ephemeral 之下」的冷门高位，最不容易被占。历史上用过的 1420（Tauri 默认，落 1366-1465）、5174（落 5110-5209）都因撞进保留段被坑过。

**确认 + 看保留段**：

```powershell
function Test-Port($addr, $port) {
    try {
        $l = New-Object System.Net.Sockets.TcpListener($addr, $port)
        $l.Start(); $l.Stop(); "OK"
    } catch { "FAIL: $($_.Exception.Message)" }
}
"IPv4 24174 : " + (Test-Port ([System.Net.IPAddress]::Loopback) 24174)
# 看当前所有 TCP 保留段（你的 dev 端口若落在某段内 = 被保留）：
netsh int ipv4 show excludedportrange protocol=tcp
```

万一 24174 也被占，临时换端口跑（不必改任何提交文件）：

```powershell
$env:VITE_PORT = "24500"
# 写个临时覆盖文件，省得动 tauri.conf.json：
'{ "build": { "devUrl": "http://localhost:24500" } }' | Set-Content -Encoding utf8 "$env:TEMP\ccm-dev.json"
powershell -NoProfile -File scripts\run.ps1 dev --config "$env:TEMP\ccm-dev.json"
```

**真正的根因解决**：
- 重启电脑（动态保留段会重排，通常就不再压到默认端口）
- 或 `net stop winnat; net start winnat`（管理员 PS）释放并重导保留段，但影响 Docker / WSL 网络，慎用

vite.config.ts 已支持 `VITE_PORT` env override，HMR 端口自动设为 `VITE_PORT + 1`。

---

## DevTools 用法

### 看前端 console

dev mode 自动开 DevTools。生产 build 没 DevTools（`tauri.conf.json` 默认禁用）。

调试前端 bug 必须先起 dev mode。

### 查 IPC 调用

DevTools Network tab 看不到 Tauri IPC（不走 HTTP）。要看 IPC：
- 在 ts 端 `invoke()` 前后加 `console.log`
- Rust 端 `tracing::info!`（dev 模式 stdout 可见；生产 build 看不到，issue #4 会加 log 文件）

### 查 capability 报错

报 `Permission xxx not allowed`：

1. 看 `src-tauri/gen/schemas/acl-manifests.json` 确认 plugin 的 permission set 实际内容
2. 看 `src-tauri/capabilities/default.json` 当前 grant 了哪些
3. 通常需要加 inline scoped permission，详 [CONTRIBUTING § 2.6](CONTRIBUTING.md#26-添加新-tauri-capability-permission)

---

## 跑测试

```powershell
cd src-tauri
cargo test --lib          # 全部单元测试
cargo test --lib profile_installer    # 单个模块
cargo test --lib -- --nocapture       # 看 println! 输出
```

前端**当前没有自动化测试套件**（v1.7 范围内）；改前端靠 dev mode 手测 + DevTools。

例外：三个零依赖断言脚本（用 `node` 原生跑 `.ts`，不引 vitest/tsx；**手动 pre-push 门禁、未接 CI**；`tsc --noEmit` 仍会自动类型检查它们）：

| 脚本 | 覆盖 | 动了哪些文件要跑 |
|---|---|---|
| `npm run test:diff` | `src/cards/diff.ts` 纯 diff 逻辑（#14） | `cards/diff.ts` |
| `npm run test:branching` | `computeMainBranch` 主线/折叠算法（#8/#22/#25 幂等） | `branching.ts` / `branch-fold.ts` |
| `npm run test:api-error` | API 报错卡纯函数（#21） | `cards/api-error.ts` |

---

## 后端日志（dev mode）

`scripts/run.ps1 dev` 启动后，dev shell 的 stdout 会显示：

- vite dev server log
- cargo build 进度
- monitor.exe 自己的 `tracing::*!` 输出（默认 INFO 级）

调级别：

```powershell
$env:RUST_LOG = "debug"; powershell -NoProfile -File scripts\run.ps1 dev
# 或更细：
$env:RUST_LOG = "monitor=debug,tauri=warn"; ...
```

生产 build 没 stdout（`windows_subsystem = "windows"`）→ 看不到 tracing 输出。**已在 v2.0.0+ 实现**：tracing 输出到 `<monitor_data_dir>/logs/monitor.YYYY-MM-DD.log` 文件 + 设置面板 → 诊断区可调日志级别 + ERROR 级 toast 反馈。详 `src-tauri/src/logging.rs` + 设置面板。

---

## 调试技巧 / 常见问题

### Tab 不出现
- 跑 `claude` 后 `~/.claude/sessions/` 应该有新 `<PID>.json` 文件
- 没有：claude 启动有问题
- 有但 Tab 不出现：dev console 看 `jsonl-line` / `jsonl-batch` 事件是否到前端

### cc 集成握手不成功
- `~/.claude/claudecode-frontend/ps-await/<PID>.json` 写了又被删 → monitor 收到了
- 写了**没被删** → monitor 没 watch 到 / parse 失败
- 看 dev stdout 是否有 `bind: parse ... failed`

### profile 写入失败
- 设置面板 [安装] 后看 alert 报错
- 检查 `~/.claude/claudecode-frontend/...` 下不存在（v1.7.10 的"防御性 abort"会触发）
- 检查目标 profile 路径是否真存在 + 权限

### tooltip 不显示 / 显示在错位置
- DevTools Elements tab 看 `.settings-info-tooltip` 是否真挂到 `<body>` 末尾
- 看 inline style 的 `left/top` 值是否合理（不应大于 `window.innerWidth` 或负数）
- 详见 [ARCHITECTURE.md § 5 CSS portal](ARCHITECTURE.md#5-关键设计选择--理由) 的设计理由

### Web 字体 / 图片 / KaTeX 加载导致消息错位 / 贴底跟随失灵
- `stream.ts` 的 `MessageStream` 用 `ResizeObserver` + 守卫式 `snap()` 应对：内容后载长高时若仍贴底则自动跟随到底部
- 如果贴底失灵，DevTools 看是否触发了 ResizeObserver 回调，以及 `snap()` 的守卫条件（落后底部 >1px 才贴）是否被满足

### 从 claude 内嵌 shell 启动 dev → resume 出的会话不注册、不落盘、无 Tab（已修 issue #24，回归排查）

**症状**：monitor 历史里点 ↺ resume，claude 窗口正常打开、能对话、能跑工具，但 monitor
永远不出 Tab；且该会话**写的任何东西都不落盘**（关窗即丢）。同一条 `claude --resume`
从用户自己开的终端跑则一切正常。

**原理（环境变量继承链 + Claude 的嵌套检测）**：
1. dev monitor 若是从 Claude Code 会话内的 shell 启动（如让 Claude 跑 `run.ps1 dev`），
   该 shell 带着 claude 注入的 `CLAUDECODE=1` / `CLAUDE_CODE_CHILD_SESSION=1` /
   `CLAUDE_CODE_SESSION_ID=<父sid>` / `CLAUDE_CODE_ENTRYPOINT=cli`；
2. Windows 子进程默认继承全部环境 → monitor → wt.exe → powershell → `claude --resume`
   原封拿到这些变量；
3. claude 据此判定自己是**嵌套子会话**（防嵌套实例与父会话互踩的合理设计）→ 不注册
   `sessions/<PID>.json`、不写会话 jsonl（实测仅启动 +2s 追加过一次 ai-title/mode/
   permission-mode 元数据，之后零字节；全 `~/.claude` 树 mtime 扫描验证）；
4. monitor 是纯只读渲染器，active 判定靠 `sessions/<PID>.json`——没注册 = watcher 过滤
   掉它全部行 = 无 Tab。**monitor 行为正确，无米下锅。**

**判别法**：`sessions/` 里没有该 claude 的 `<PID>.json` + jsonl mtime 冻结 + 用户自己
终端 resume 正常 → 即此问题。**注意 resume 窗口里写的内容不会被保存。**

**已修**（issue #24）：`lib.rs::run()` 最前（任何线程 spawn 之前）
`scrub_env_vars(&NESTED_CLAUDE_ENV_KEYS)` 清掉上述四个嵌套标记（保留
`CLAUDE_CONFIG_DIR`——monitor 自己消费它），之后 spawn 的一切子进程都干净。
清洗发生时日志留痕 `scrubbed inherited claude nested-session env markers: ...`。
**边界**：scrub 只管 monitor 自己 spawn 的进程链；若 Windows Terminal 配置为
"attach 到已存在窗口"，新 tab 的 shell 继承的是已有 WT server 进程的环境——
那个 server 若本身从 claude 会话里启动，resume 出的 claude 仍带毒（monitor
管不到别人的进程树）。

### 会话内容在 timeline 底部整段重复（已修 issue #26，回归排查）
- 根因：watcher 截断重读换新 seq 重投整个文件（at-least-once，INVARIANTS § 25），seq 去重放行 → 每条记录以更大 seq 在末尾再渲染一遍
- 检查：`tabs.ts onLine` 的 `processedUuids` 按 uuid 整体拒重还在（ensureTab/seq 去重之后、trackAgents/渲染之前）
- 复现：对一个被 watch 的活跃 jsonl 手动截短再追加回去 → 后端出 `jsonl truncated ... full re-read` warn → Tab 内容应**不**翻倍

### 大段消息被误折成「已被 ESC 回退」（已修 issue #25，回归排查）
- 根因：行投递是 at-least-once（INVARIANTS § 25），重复 uuid 毒化 `computeMainBranch` 的 Kahn 拓扑——`remaining` 计数被多扣、重复点全部祖先 leftover、折叠信号（latestDescTs/hasAssistant）全错 → fork 赢家/多 root 分类误判。重复常是 attachment 等**不渲染**的记录，肉眼看不出输入有重复；实测 1 条重复 attachment 即折 1541/4331 条
- 检查四道防线：(1) `computeMainBranch` 入口 uuid 去重还在；(2) `BranchFolder.seenUuids` 拒重还在；(3) DevTools console 有无 `[branching] Kahn leftover` warn（出现 = 异常输入新形态，warn 里带嫌疑 uuid）；(4) 后端 log 有无 `jsonl truncated ... full re-read` warn（出现 = 发生过截断重投；频繁出现要查谁在改写 jsonl）
- 复现/回归：`npm run test:branching`（#25 三用例：root 级毒化 / fork 级毒化 / 全文件 doubled 幂等）。

### 启动重放期最新消息整行高频上下微抖（已修，回归排查）
- 根因：旧内容逐帧插到贴底视口上方 → 浏览器逐帧重排 + 重做 scroll anchoring，HiDPI/高刷屏分数像素下舍入误差每帧不同 → 整块 ±0.5px 抖（详 INVARIANTS § 21）
- 检查三道防线是否被破坏：(1) `snap()` 是否还守卫（没被改成无脑 `scrollTop=scrollHeight`）；(2) `.stream` 是否被加了 `overflow-anchor: none`；(3) F40a 尾部优先门控是否仍在——`TabManager.onLine` 对 `seq < floor` 的旧记录收纳进 `TailWindow` 不建卡（INVARIANTS § 21.3；deferMode/flushDeferred 已于 F40a 退役）
- **关键**：`scrollTop` 本身不震荡（单调增长），只测 scrollTop 发现不了——要测可见元素 `getBoundingClientRect().top` 的逐帧方向反转。
