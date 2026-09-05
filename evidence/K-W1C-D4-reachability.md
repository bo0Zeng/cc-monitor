# K-W1C · D4 可达性摸底 —— 甲问「判不了」，乙问「做」

## 甲 · 「窗口还在，但里面已经换人了」今天可达吗

### 结论：**判不了。** 我在这台机器上做不到这次实测。

理由是**结构性的、可核的**（现打于 `.claude/worktrees/k-w1c` 分支尖，命令在下面）：
拉前整族是 Windows-only —— `bind.rs` 里 `#[cfg(windows)]` **11 处** / `#[cfg(not(windows))]` **10 处**，
非 Windows 侧 `verify_binding` 与 `activate` 逐字返 `only supported on Windows`、`try_bind` 返 `false`。

```
$ command grep -c '#\[cfg(windows)\]' src-tauri/src/bind.rs        # 11
$ command grep -c '#\[cfg(not(windows))\]' src-tauri/src/bind.rs   # 10
```

⚠ **这不等于「结构上不可能发生」** —— 件计划 `§4` 第 3 条逐字禁止拿后者代替一次实测。
下面是**在 Windows 真机上可照做的步骤**；甲问由那台机器答，不由我推。

### 一个把甲问变便宜的转写（★ 本节最值钱的一格）

原题面要求造出「点 A 的 ↗ 拉到了 B 的窗口」。**那一幕的必要条件可以先落成一个文件级读数**，
不必先点那一下：

> **判据形状**：`sid-hwnd-cache.json` 里出现**两条 `hwnd` 值相同、sid 不同**的记录，
> **且**这两个 sid **同时**出现在会话列表里（都没被归档）。

两条理由：
1. `record` 与 `try_bind` 都是 `by_sid.insert(sid, binding)` —— **只按 sid 去重，从不按 hwnd 去重**
   ⇒ 「N 个 sid 认领同一个 hwnd」在结构上允许。这个状态一旦出现，
   `verify_binding`（`IsWindow` + 属主 PID + 属主 procStart）对**两条都恒绿**。
2. 反过来，若这个状态**造不出来**，那「拉到别人窗口」也就到不了 ——
   于是甲问的答案落在**同一个读数**上，而这个读数是一个 `cat`，不是一次肉眼观察。

⇒ **在 Windows 真机上先跑「造这个状态」，再补那一下点击。**

### 真机步骤（三条候选路，逐条给「要记什么读数」）

前置（三样，缺一条读数就不算）：
- monitor 跑在**已登录的交互桌面**（session 1）。session 0 里窗口 `IsWindowVisible` 恒假，
  `find_window_by_marker_substr` 会过滤掉不可见窗口 —— 本仓已有先例把这条写在
  `remote_bind_finds_real_ccm_rbind_window` 的头注里（跑法：`schtasks /it` 把测试投进 session 1）。
- 两份盘上文件的路径先记下来（`%USERPROFILE%\.claude\claudecode-frontend\`）：
  `sid-hwnd-cache.json` · `ps-registry\*.json`。
- **每一步之前和之后各 `cat` 一次那份 json，两份都留档** —— 「命令没跑」与「跑了没变化」
  在终端上一模一样。

**路① `/branch`（预期会被 `Superseded` 拦住 —— 这一路是用来确认那道闸真的在的）**
1. 一个 PowerShell 窗口里跑 `cc`（走 PS 握手，`ps-registry\<PS_PID>.json` 落地）。
2. 等 monitor 日志出现 `sid-hwnd: bound sid=<A> → hwnd=0x…`。记下那个 hwnd。
3. 在同一个会话里执行 `/branch`（或 `/clear`）⇒ 同一个 pidfile 原地换 sid。
4. **要记的读数**：`sid-hwnd-cache.json` 里 A 那条**在不在**？monitor 日志里有没有
   `sid-hwnd: forgot sid=<A>`？
   - 若 A 那条没了 ⇒ **这一路被 `Superseded` 那条边拦住了**，如实写「这一步拦住了」，
     并把日志那两行逐字贴出来。
   - 若 A 那条还在、且 hwnd 与 B 相同 ⇒ **甲问答「可达」**，直接跳到「那一下点击」。

**路② 同一个 PS 窗口里先后两个 claude（绕开 `Superseded`，因为它不是「同一个 pidfile 换 sid」）**
1. 同上跑 `cc` → 会话 A，记 hwnd。
2. 让 A 的 claude 进程**非正常**结束（关掉 claude 但**留下** `sessions\<PID>.json`，
   例：任务管理器结束 claude 进程树；⚠ 不许动 monitor 自己）。
3. **不重跑 `cc`**，在同一个 PowerShell 窗口里直接 `claude` ⇒ 会话 B。
   `ps-registry\<PS_PID>.json` 仍在（心跳只清**死** PS，这个 PS 没死）
   ⇒ B 的 `record` 会查到**同一个 hwnd**。
4. **要记的读数**：`sid-hwnd-cache.json` 里 A、B 两条**同时在**且 `hwnd` 相等吗？
   会话列表里 A 还在吗（还是已经被心跳按 `Gone` 摘掉了）？
   - 两条同时在且 hwnd 相等 ⇒ **必要条件造出来了**。
   - A 被摘掉了 ⇒ 记下「是心跳按 `Gone` 摘的」，并记它花了多久
     （那就是这一格的**窗口期**有多宽，是个真读数）。

**路③ 远端 `/resume` 切 sid（`RemoteHwndCache` 那一侧，纯内存、没有文件可 `cat`）**
1. 远端起 ccm wrapper 的会话 A（窗口标题带 `ccm-rbind-<A>`），本地 ↗ 确认能拉前。
2. 在同一个 ssh + tmux 窗口里 `/resume` 切到会话 B ⇒ wrapper 把标题重刷成 `ccm-rbind-<B>`。
3. **要记的读数**：A 的 Tab 还在不在（live 还是 idle 灰灯）？此时点 A 的 ↗：
   - 那个 ssh 窗口被拉到前台 ⇒ **甲问在远端这一侧答「可达」**
     （那个窗口现在跑的是 B，不是 A）。把 toast 原文 / 无 toast 一起记下来。
   - 报「未绑定窗口」或「窗口已不存在」 ⇒ 记下逐字原文，那是「拦住了」。
   ⚠ 这一侧**没有落盘文件**（`RemoteHwndCache` 是进程内存里的 `HashMap`）
   ⇒ 只能靠「点一下 + toast 原文」，读数天生比路①② 弱。**如实写，别补一个自己没量的。**

**那一下点击（三条路里任一条造出必要条件之后）**
- 点 A 那个 Tab 的 ↗，记三样：① 哪个窗口被拉到前台（截图或窗口标题逐字）·
  ② 有没有 toast、原文逐字 · ③ 同一刻 `sid-hwnd-cache.json` 的内容。
- 🔴 **不许把「窗口没了」和「窗口还在但换人了」压回同一个读数** —— 那是又造一个
  「一个值装两件事」。两者的分辨点：`IsWindow` 真 / 假。

### 判不了这一格还需要什么

一台**已登录**的 Windows 桌面 + 那台机器上跑着的 monitor + 上面三条路各跑一趟的
`sid-hwnd-cache.json` 前后快照。⚠ 本仓的 win11 VM 归用户管（`K-P4` 那件的 parked 说明里
同一条卡点逐字写着「要真机跑 Windows，而 win11 VM 归用户」）⇒ **这一问归排期，不归本波。**

---

## 乙 · 不可达也要钉住吗

### 判：**要**，钉的是**负向**那一句，落点 `bind.rs`（本波已落）

判据名 `verify_binding_cannot_tell_that_the_window_changed_hands`。它断言
`verify_binding` 的 `#[cfg(windows)]` 那一支：**确实**在看 `IsWindow` / `owner_pid` /
`owner_proc_start` 三样（抽取器自检，防零命中地绿），而**不含** `title_at_bind`、
也不含 `GetWindowText`（防「读当前标题」那另一半写法）。

**论据四条**：

1. **这是一条隐式前提，而前提没人看着就会烂。** `verify_binding` 的安全性今天**借给了上游**
   （`Superseded` → removed → `forget`）。件计划自己把这件事写在散文里，
   而散文烂掉的方式本件 `§0a` 刚刚现打过一次：`U8e` ③ 那句「没有任何机制」
   在写下的那天就已经是假的，隔了两个月没人对账。**同一个仓、同一条链、同一族病。**
2. **它是「本波不许改行为」这条纪律下唯一买得到的东西。** D3 与 D4/D5 严格分开
   （件计划逐字）；给 `verify_binding` 加标题比对是**改行为**，那一步要真机正向判据撑着，
   而真机今天拿不到 ⇒ 本波能落的只有「把今天的事实钉住」。
3. **它会在正确的时刻出声。** 谁哪天加了标题比对（那是件好事），这一条红，
   红的那句话逐字要求两件事：补一条真机正向判据、把本条改写成新的事实
   —— 而不是「把这句话删掉」。⇒ 它不是拦路石，是一张**交接单**。
4. **形状有先例。** 本仓已有同形的负向源文本判据
   （`session_map.rs` 的 `the_heartbeat_branch_never_rereads_the_directory`：
   「心跳分支里不许出现 `scan_dir(`」＋「扫描分支里确实有一处」的双侧自检）。
   ⇒ 不是新造一套。

**它买不到什么（逐条写明，别读大）**：
- 买不到「这一幕今天真的到不了」（那是甲问，判不了）。
- 买不到「加了标题比对就对了」（标题会被 claude 自己改写，正向判据必须在真窗口上验）。
- 它按**源文本**判、不按行为判 ⇒ 只对 `#[cfg(windows)]` 那一支的**写法**说话；
  哪天有人把标题比对写进一个被调用的 helper 里，本条**看不见**（射程如实写在判据头注里）。

### 「若判做」那一路我**没有**走，理由

件计划给的最便宜形状是「让远端那条 verify 顺带比一次标题里的 marker」。
**本波不做**，两条理由：
1. **本机那条没有 marker 机制**（它走 PS 握手，`record` 组的 `title_at_bind` 是
   `Windows PowerShell` 这种普通标题，不是 `ccm-rbind-<sid>`）⇒ 两侧不同料，
   件计划自己写着「别硬做成一套」。只做远端那半，会造出一格**本机与远端判据不对称**的新账。
2. **它是改行为**，而「改坏了没有」今天判不出来：远端那条 verify 加了标题比对之后，
   `↗` 的失败面会变（原来 `Idle` 灰灯时那条绑定仍然拉得前，加了 marker 比对之后
   wrapper 停写标题的窗口期里会开始报错）—— 那一格要真机才判得了。
⇒ **归下一件**（真机那一拍连甲问一起做），不写成「暂不处理」。
