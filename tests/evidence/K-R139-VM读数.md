# `K-R139` 臂 B 读数 —— **win11 VM 上把 v3.8.0 过一遍**

> 量于 09-15。真机：`win11` KVM（`DESKTOP-H18UDOV`，Win11 专业版，**系统 ANSI 代码页 `ACP=936`（GBK）**，
> `PSVersion=5.1.26100.9444`），用户 `VM260726`（本机唯一有**活跃 console 会话**的用户，session 1）。
> 被测产物：`cc-monitor_3.8.0_x64-setup.exe`，sha256
> `280400CDB6A4E97C95EAB174BD0F3A2C6E97011847FE417C4487620588D5DF09`
> （**与发版清单 `SHA256SUMS.txt` 逐字同，且上传到 VM 之后用 `Get-FileHash` 重打过一次**）。
> 量具住 `<会话临时目录>/kr139-{survey,probe,ccm,shot,shot2}.ps1`，正文见 `§6`（**全文抄在本文里** ——
> 临时目录会被回收，抄进来才复得出）。

---

## 〇 · 开工前快照（**动机器之前先量**）

`kr139-survey.ps1`，量于 09-15 09:36（装之前）：

| 项 | 开工前 |
|---|---|
| 三处 `Uninstall\*` 里 `DisplayName -like '*cc*monitor*'` | **0 条** |
| `C:\Program Files\cc-monitor` | **不存在** |
| `kr129clean\.cc-monitor\bin\` | `cc-monitor-local-p2j-bus-state.exe(7921664)`, `ccm.exe(7921664)` |
| `VM260726\.cc-monitor\bin\` | `cc-monitor-local-p2e-dial.exe(7737344)` |
| `VM260726` 的 HKCU `Environment\Path` | `…\.cargo\bin;…\AppData\Local\Microsoft\WindowsApps;…\AppData\Roaming\npm` |
| `$PROFILE` | **不存在** |
| 登录会话 | `services` = session 0（断开）· **`console` = `VM260726`，session 1，活动** |

⇒ **`kr129clean` 今天仍然不干净**（两个 exe 还在，与 `K-R132` `§0.2` 记的逐字节相同）——
**本轮一个字节都没动它**，见 `§5`。

---

## 一 · 🔴 我自己先撞上了 `K-R132` `§4` 那条 BOM 缺陷 —— 这是一条**活体旁证**

**不是推理，是我的量具当场坏了。**

第一版 `kr139-survey.ps1` 写的是**不带 BOM 的 UTF-8**、正文里含 CJK（`"0 条"`）。
在这台 `ACP=936` 的机器上跑，PowerShell 5.1 **按 GBK 解**，于是：

```
== installed cc-monitor (3 hives) ==
0 M-fM-^]? }
Write-Output
==
Program
Files\cc-monitor
==
if (Test-Path 'C:\Program Files\cc-monitor') {
```

**CJK 那一行把它下面几行吞了 / 打散了，脚本后半段被当成数据回显出来**，
而 `powershell -File` 的**退出码仍是 0** —— 与 `K-R132` `§4` 描述的形状（静默、无报错）一致。

**修法同 `K-R132`**：同一份内容、只在前面加三个字节 `EF BB BF` ⇒ 当场全对。
⇒ 本条**独立复现了那条机制在这台机器上成立**（分母：**一台**机器、ACP=936；
别的代码页我没量，`K-R132` `§4.1` 那条「英文代码页 1252 上不成立」我**没重打**，只引住址）。

---

## 二 · 装 v3.8.0（`KR139D3` ③ 的前半）

`installMode` 现打是 `perMachine`（`src-tauri/tauri.conf.json:49`，逐字 `"installMode": "perMachine"`）
⇒ 要提权。走的是**`/IT` 计划任务**那条路（本项目既有的 session-1 hop）：
`schtasks /Create /TN kr139inst /TR "…\kr139-setup.exe /S" /RU VM260726 /IT /RL HIGHEST`。

装完（`kr139-survey.ps1` 重打）：

| 项 | 装之后 |
|---|---|
| 三处 `Uninstall\*` | **`FOUND: cc-monitor v3.8.0`** |
| `C:\Program Files\cc-monitor` | `cc-monitor-remote.exe (7921664)` · `monitor.exe (48615936)` · `uninstall.exe (156665)` |
| `VM260726` HKCU `Path` | **一个字节没变**（与开工前逐字相同） |
| `$PROFILE` | **仍然不存在** |

⇒ **装机这一步本身是成功的**，而且**它没有碰 PATH** —— 与 `K-R132` `§0.1`
「全仓零个 Windows PATH 写入点」那条读数一致（那条我**没重打**，住址 `evidence/K-R132-摸底.md#0.1`）。

---

## 三 · 🔴 `KR139D3` ① —— 三种 shell × 三个名字，**装完、跑完各一遍**

量具 `kr139-probe.ps1`。**`cc` 用 `Get-Command` / `where` / `command -v` 三种口径**，
其中 `command -v` 认得出**函数与别名**（`which` 只认 PATH 上的文件）。

### 3.1 装完 `.deb`… 装完 setup、**还没起过 app**（`MARK AFTER-INSTALL-BEFORE-APP`）

| | `ccm` | `cc` | `cct` |
|---|---|---|---|
| PowerShell | NOTFOUND | NOTFOUND | NOTFOUND |
| `cmd` | NOTFOUND | NOTFOUND | NOTFOUND |
| Git Bash | NOTFOUND | NOTFOUND | NOTFOUND |

`$HOME\.cc-monitor\bin` 里只有开工前那个 `cc-monitor-local-p2e-dial.exe`；`onPATH=False`。

### 3.2 起过 app 之后（`MARK AFTER-APP-RUN`）

**app 与后端都真的起来了**：`monitor pid=412` · `cc-monitor-remote pid=1536` · `cc-monitor-remote pid=8372`。
**`ccm.exe (7921664)` 出现在 `C:\Users\VM260726\.cc-monitor\bin\`** —— 就是
`install_local_ccm_entry`（`src-tauri/src/backend/control/local_backend.rs:1657` 那个调用点）放下来的。

| | `ccm` | `cc` | `cct` |
|---|---|---|---|
| PowerShell | NOTFOUND | NOTFOUND | NOTFOUND |
| `cmd` | NOTFOUND | NOTFOUND | NOTFOUND |
| Git Bash | NOTFOUND | NOTFOUND | NOTFOUND |

`onPATH=False`（`($env:PATH -split ';') -contains "$env:USERPROFILE\.cc-monitor\bin"`）。

🔴 **⇒ 9/9 找不到，而 `ccm.exe` 就在盘上、后端就在跑。**
这正是 `K-R132` 那条已发版缺陷，**在 `v3.8.0` 上原样复现**。
⚠ **`K-R132` 的修（`dfac600`）在我这棵树的基线上，但不在 `v3.8.0` 里** ——
所以这一格**不是回归，是那条修还没发出去**。

---

## 四 · `KR139D1` 的探针那一格（Windows 侧）

`ccm --ccm-probe` 直接问 `C:\Users\VM260726\.cc-monitor\bin\ccm.exe`：

```
name=ccm
version=5
self=C:\Users\VM260726\.cc-monitor\bin\ccm.exe
capabilities=new,resume,attach,tmux,account,model,cwd,agent,launcher,ccm-sid,print,detach,tmux-size,tmux-base,bus-register,daemon-discover,account-via-daemon,base-url-across-tmux
agents=claude,codex
build=p2j-bus-state
```

**`probeLineCount=6` —— 六行，逐字对上** `control/ccm/mod.rs:160` 那句
「吐出 name= / version= / self= / capabilities= / agents= / build= 六行」。

⚠ **一条观察，量法有限制，别读大**：同一趟 `ccm --help` 的中文在我的 SSH 管道里是**乱码**
（UTF-8 输出被 GBK 控制台解）。**这是经 SSH 抓的，我没有在 VM 的真 console 窗口里复核过**
⇒ 只记作「**可能**存在的 CJK 控制台呈现问题」，**不作结论**。

---

## 五 · 🔴 `KR139D3` ④ —— **GUI 到底渲没渲出来：渲出来了**（前两轮都没敲到）

`K-R129` / `K-R132` 两轮都卡在「SSH 落 session 0 无桌面」。
本轮走 **`/IT` 计划任务**把 app 拉进 **session 1**（`VM260726` 的活动 console 会话），
再在同一个 session 里截屏。

窗口几何（`GetWindowRect`）：`hwnd=196760 title=[cc-monitor] rect=-8,-8,1160,830`
（即 1168×838 的真窗口，不是 0×0 的幽灵）。

**截图两张**（`kr139-shot.png` / `kr139-shot2.png`，第二张先把控制台窗口最小化再把 app 提到前台）：

第二张里**逐字读得出的界面元素**（这才是「渲出来了」的证据，`hwnd` 不是）：

- 标题栏 `cc-monitor` ＋ 右上角一排六个工具图标
- 画布中央：**「暂无活跃会话」**、下一行 **「打开终端跑 `claude` 后将自动出现」**
- 底部状态条：**「M2 · 等待活跃 Claude Code 会话...」** · **「活跃 0」** · **「命令 Ctrl + K」**
  · **「还差什么：2 项确认缺，1 项还没测过」**
- 左侧栏：**「已归档 0」**

⇒ **GUI 真的渲出来了，中文字形正常，不是白屏。**
⚠ **这一格买到的是「渲出来了」，不是「功能都对」** —— 我没有登录过任何账号、
没有起过真 `claude` 会话 ⇒ 列表区是空态。空态之外的界面**没测**。

---

## 六 · `KR139D3` ③ —— **装 → 卸 → 看残留**

卸载走 `C:\Program Files\cc-monitor\uninstall.exe /S`（同样 `/IT` ＋ `/RL HIGHEST` 计划任务）。
卸之前先把 app 与后端停掉（`monitor pid=412` · `cc-monitor-remote pid=1536` · `pid=8372`，
三个都 `Stop-Process -Force`）。**`uninstall exit=0`。**

| 项 | 卸之后 | 判 |
|---|---|---|
| 三处 `Uninstall\*` 登记 | **0 条** | 摘干净了 |
| `C:\Program Files\cc-monitor` | **不存在** | 摘干净了 |
| `VM260726` HKCU `Path` | 与开工前逐字相同 | 本来就没碰过 |
| 🔴 `%USERPROFILE%\.cc-monitor\bin\ccm.exe` | **还在**（7921664） | **残留** |
| 🔴 `%LOCALAPPDATA%\com.ccmonitor.app\EBWebView\` | **整个还在**（现打 20+ 个文件，含 1.3MB 的 `BrowserMetrics-spare.pma`、`component_crx_cache` 若干 MB） | **残留** |

⇒ **`K-R132` `§0.2` 那条「卸载器把自己摘干净了、`%USERPROFILE%\.cc-monitor\` 整个留着」
在 `v3.8.0` 上原样复现，而且现打多出一处：WebView2 的数据目录 `com.ccmonitor.app` 也整个留着。**

⚠ **分母**：我只查了这五处（登记三处合一 · Program Files · HKCU Path · `.cc-monitor` · `com.ccmonitor.app`）。
「有没有别的残留」**我没有穷举全盘**，那个分母我数不出来。

---

## 七 · ⚠ `KR139D3` ② —— **判不了**，拦路石写清楚

要判的是「产品**自己写出来的那份** PowerShell profile 有没有 BOM、CJK 注释下一行有没有被吞」。

**判不了。拦路石是：`v3.8.0` 里那份 profile 只有 GUI 能触发，而我敲不到那个按钮。**

我查过的路（**逐条**，第 14 条要求）：

1. **`$PROFILE` 从头到尾没被创建过** —— 装前 / 装后 / 起过 app 之后三次量，全是 `exists=False`。
2. **`ccm --help` 全文现打（45 行）里没有任何一个装 profile / 写 rc 的子命令** ——
   动作只有 `new` / `resume` / `attach`，选项里没有 `shellinit` 这一族。⇒ **没有 CLI 路。**
3. **全盘搜过 app 写出来的 `.ps1`** —— `$env:USERPROFILE` 下三小时内改过的 `.ps1` **只有我自己那七份量具**；
   `C:\Program Files\cc-monitor` 下**一个 `.ps1` 都没有**（只有三个 exe）；
   `%APPDATA%` / `%LOCALAPPDATA%\com.ccmonitor.app` 下也没有。⇒ **模板不落盘，装 profile 是运行时行为。**
4. **试过用命令面板触发** —— 状态条上写着「命令 `Ctrl + K`」。
   用 `WScript.Shell::SendKeys("^k")` 打进已提到前台的 app 窗口，**面板没开**
   （截图 `kr139-palette.png` 与前一张逐像素级别看不出差别）⇒ **`SendKeys` 进不了 WebView2**。
   再往下就要真的做坐标点击 / DOM 注入，那是另一件事的量级。

⇒ **这一格如实记「判不了」，不记「通过」，也不记「没问题」。**

### 7.1 但有一条**活体旁证**，方向是不利的

`§1` 里我自己那份不带 BOM 的 CJK `.ps1` 在这台机器上**当场被吞**了。
再加上两条现打的事实：

- **`K-R132` 的 BOM 修（提交 `dfac600`）不在 `v3.8.0` 里** ——
  `git rev-list --count v3.8.0..b3e5e22` = 15，而 `dfac600` 就在那 15 个里
  （`git log --oneline v3.8.0..b3e5e22` 现打含
  `dfac600 K-R132: ccm 不在 PATH 上 —— 补 PATH 那一行，并修真机逮到的 BOM 缺陷`）。
- 这台机器 `ACP=936`，正是那条机制成立的前提。

⇒ **合理预期是「`v3.8.0` 在这台机器上写出来的 profile 会犯那个病」，但我没有把它量出来。**
**预期不是读数** —— 按 `R80` 那条形状（「没验 ≠ 没有」），本条只记预期，归 `§8` 的「没测到」。

---

## 八 · 收工时那台机器的状态（逐项核过，与 `§0` 开工快照比）

| 项 | 开工前 | 收工时 | 一样吗 |
|---|---|---|---|
| `Uninstall\*` 登记 | 0 条 | 0 条 | ✅ |
| `C:\Program Files\cc-monitor` | 不存在 | 不存在 | ✅ |
| `VM260726\.cc-monitor\bin\` | `cc-monitor-local-p2e-dial.exe(7737344)` | `cc-monitor-local-p2e-dial.exe(7737344)` | ✅（我把本轮出现的 `ccm.exe` 删了） |
| `kr129clean\.cc-monitor\bin\` | `…p2j-bus-state.exe(7921664)`, `ccm.exe(7921664)` | 逐字相同 | ✅ **一个字节没动** |
| `VM260726` HKCU `Path` | 三项 | 逐字相同 | ✅ |
| `$PROFILE` | 不存在 | 不存在 | ✅ |
| `%LOCALAPPDATA%\com.ccmonitor.app` | 不存在 | **已删** | ✅ |
| 我放的量具 `kr139-*` ＋ `kr139r.ps1` | — | **全删**（`dir kr139*` 零命中） | ✅ |
| 我建的计划任务（6 个） | — | **全删**（`schtasks /Query | Select-String kr139` 零命中） | ✅ |
| VM | 关机状态 | **从 guest 内 `shutdown /s /t 0` 关机**，`virsh list` 现打 `关闭` | ✅ **没有用 `virsh destroy`** |

⚠ **本轮确实动了这台机器**（装了一次、起了一次 app、卸了一次），**但都还原了**。
`kr129clean` 与构建环境**一个字节没动**。
