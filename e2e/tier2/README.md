# F-E5 Tier2 —— Windows DOM 冒烟套件（WebdriverIO）

在**真 Windows + 真 WebView2**里，用 WebDriver 驱 cc-monitor 的 DOM，断言裸壳形态与
overlay 快捷键。跟 `e2e/f40-suite.sh` 同级——f40 靠 X 中键绕进不了 webview 的洞，Tier2
用 WebDriver 直驱 DOM（`sendKeys` 物理码 + 选择器点击都可用）。

- **spec**：`test/shell-smoke.spec.mjs`（E5a 裸壳，无 fixture）
- **wdio 配置**：`wdio.conf.mjs`（经典 tauri-driver 路径：spawn `tauri-driver --native-driver <msedgedriver>`，非 `@wdio/tauri-service`）
- **session-1 payload**：`run-in-session1.ps1`（被 schtasks /it 触发那一段）

## 为什么要 session-1 hop

session-0（普通 SSH 非交互会话）**起不来 WebView2**（无桌面 / 无渲染）。已验通的绕法：
建一个 `schtasks /it`（interactive）计划任务，`schtasks /run` 让它落到**已登录的交互会话
（session 1）**里跑 wdio。aya 侧只需轮询 `wdio.log` 的完成标记再回盘读结果。

## 前置（VM 一次性）

- `tauri-driver.exe` = `%USERPROFILE%\.cargo\bin\tauri-driver.exe`（`cargo install tauri-driver`）
- `msedgedriver.exe` —— 版本匹配 VM 的 WebView2 Runtime；放 `%USERPROFILE%\.cargo\bin\` 或填 `MSEDGEDRIVER` env
- app exe：`C:/Users/vm260726/cc-monitor/src-tauri/target/debug/monitor.exe`（KVM_cc build 出，可用 `APP_EXE` 覆盖）
- wdio devDeps：repo 根 `npm install` 后 `node_modules` 会被 Node 从 `e2e/tier2/` 向上解析到；
  或复用 VM 上已缓存 node_modules 的目录（把本目录内容拷进去即可）。

## 跑法（从 aya 驱动，三步）

```bash
# 1. 把本目录同步到 VM（示例落 C:\Users\vm260726\e2e-tier2；或拷进已装 node_modules 的目录复用缓存）
scp -r e2e/tier2/* win11:e2e-tier2/

# 2. 装 wdio 依赖（若该目录还没 node_modules）
ssh win11 "powershell -NoProfile -Command \"Set-Location \$env:USERPROFILE\e2e-tier2; npm install --no-fund --no-audit\""

# 3. session-0 → session-1 hop：建 /it 任务跑 run-in-session1.ps1 → 轮询 wdio.log 完成标记 → 回盘读
ssh win11 powershell -NoProfile -ExecutionPolicy Bypass -Command @'
  $dir = "$env:USERPROFILE\e2e-tier2"
  $log = "$dir\wdio.log"
  $tr  = "powershell -NoProfile -ExecutionPolicy Bypass -File $dir\run-in-session1.ps1"
  schtasks /create /tn cc-tier2 /tr $tr /sc ONCE /st 23:59 /it /f | Out-Host
  Remove-Item $log -ErrorAction SilentlyContinue
  schtasks /run /tn cc-tier2 | Out-Host
  $deadline = (Get-Date).AddSeconds(240)
  while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 4
    if ((Test-Path $log) -and (Select-String -Path $log -Pattern "TIER2 END" -Quiet)) { break }
  }
'@
# 回盘读结果（log 若是 UTF-16 用 iconv -f UTF-16 转）
ssh win11 "powershell -NoProfile -Command \"Get-Content \$env:USERPROFILE\e2e-tier2\wdio.log\""
```

## 判读

- **PASS**：log 出 `N passing` / `0 failing` + `WDIO_EXIT=0`。这是「WebView2 真渲染 +
  WebDriver 可驱 DOM + 物理码快捷键真起效」的活证。
- **FAIL 且错在会话/前台**（app 起不来、无 display、session-0 拒渲染）：说明 hop 没落到
  session 1，检查是否已登录交互会话 + `/it` 生效。
- 只信回盘读的 log，别信 SSH 内联回显（终端污染可伪造进度）。

## 覆盖 / 不覆盖

- **E5a（本 spec，已落地）**：壳元素 · 状态栏文案 · 6 顶栏钮 + cmdk 可点 · H/G/Ctrl+K
  overlay 开关（物理码 `browser.keys`）。无 fixture、无破坏性动作。
- **E5b（会话相关，未落地——需 fixture）**：`button.tab` / `.tab-title` / `.live-dot`、
  右键 `.tab-context-menu`、archived tab 的 resume 项。落地路径：VM app 配 aya 当远端
  （E(b) key）+ aya 跑 F-E1 的 fake-claude 造 idle tmux → 一个 tab 出现后再断言。见
  `.claude/planned-build/auto-e2e/features/E5-tier2-dom.md` DoD E5b。
- **不做（Tier3 手动 / hard-to-fixture）**：↗ 弹外链 · 真终端 · SFTP · 系统通知 · 真
  claude 真出内容。
