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
