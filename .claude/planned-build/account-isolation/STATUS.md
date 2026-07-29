# STATUS — account-isolation

> 恢复入口。多账号「隔离又同步」通用管线 + cc-monitor 按会话切账号。归并 #68/#69。

## 当前阶段:**✅ A0–A6 + A5+ 全部完成 · Phase G(A0–A5) 已过 · A6/A5+ 各过 D/E/F · 交回用户**

> **A6 + A5+ 追加完成(2026-07-24,用户「继续做 A6 和换号重启优雅退出」)**:两功能各走 B→C→D→E→F。
> · **A5+ 优雅退出**：换号重启 ④ = `Escape`→`/exit`+Enter→有界等 10s(`awaitExitFor`/`claudeExited` 轮询前台不再是 claude)→`kill` 兜底(必跑;失败仍中止防双进程)。Rust 提纯 `build_send_keys_remote_cmd` + `tmux_send_keys` 加 `enter?`(默认 true 向后兼容,`/compact` 不受影响,补 R1 命令测)。
> · **A6 部署向导**：**纯前端零新增 daemon 命令**。只读用既有 `list_remote_accounts`;dry-run/verify/--apply/sync/login 全经既有 `launch_remote_terminal` 弹终端。纯 `acct-deploy.ts`(校验+单引号,与 launch.rs 双层防线)。
> · **D 两视角并行(各功能)**：**零阻塞**。A6 1 重要 I-1(login=`cc-acct-iso run <名>` 偏 DoD→裁定为改进[工具唯一登录入口+注入面更小],回写 DoD);hardening 采纳 A5+ S1(清挂起轮询 timer)/A6 S2(预览复制按钮)/S3(拒前导-)。两 agent 均独立重跑构建测试、grep 核实,**谎报未复现**。
> · **Phase F 文档**：INVARIANTS **§1** 补 `enter?` 注（原文误写 §37）;DESIGN §6 dry-run/verify 走终端裁定回写(daemon 只读零妥协⇒A6 不触发发版)、§5④ 优雅退出落地、§1 V2/V3 标已解;MASTERPLAN 变更记录 + 账本(tmux_send_keys `enter?` / A6 纯前端)。
> · **实测(每步 Read 回盘 + 重定向文件核实,不信内联)**：tsc 0 / npm test **453**(37 文件,+acct-deploy 12 +graceful 3 +claudeExited 5 +前导-1) / cargo test --lib **352**(+send_keys 构造测) / build ✓ / **真机零改动**(A6 自身不落盘,落盘全在用户看着的终端里由用户确认;A5+ 只 send-keys/kill/resume 远端 ssh)。

> **Phase G 收官(2026-07-24)**:1 个聚焦集成审计 agent 独立复核（自己重跑构建测试 + grep 核实"声称做了"的项，不信文档自述）——**零阻塞零重要**。关键：历史「日志谎报」事故**未复现**，features/05 声称项（删 peekSelectableAccounts / `live.sid===sid` 守卫 / tmux_send_keys 白名单 / compact 真检测器）逐条 grep/读码为真。五维度全过:① account store 六消费方一致无漂移(三站点走 withAccount + restartWithAccount 刻意分离);② 共享面账本落最终形态(全族仅 1 条计划内 TODO=优雅退出 V3);③ 文档-代码一致 + §7 四分支齐;④ daemon 只读边界全族守住;⑤ 无回归。**实测（agent 独立重跑 + 我端到端）**:tsc 0 / npm test 433 / cargo test --lib 351 / daemon cargo 124 / build ✓ / 真机零改动(生产代码只写 monitor data dir,不碰用户 ~/.claude)。
> **交回用户 · 遗留项（均需用户单独决策）**:~~(1) **发版**~~ **已完成**：v3.2.0 已发布（commit `b889808` + git tag `v3.2.0`，CI 交叉编译并内嵌两 arch musl daemon）;(2) **真机迁移**=用户空闲自跑 cc-acct-iso 管线（现也可用 A6 向导在设置里点着跑）+ 删 ~/.bashrc 的 cc-account-block;(3) **A7 本地 Windows 切号**=future，需单独审批;(4) 计划内裁剪/建议(非缺陷,已记账):resolveLiveTmux DRY、DEFAULT_COMPACT_WAIT_MS 90s 准死常量、非 cc-* 会话 compact-skip toast 文案、建议-4 daemon 结构化 reason code(发版前)、A6 未做 rollback 向导化(高危,故意不做);~~(5) **尚未 commit**~~ **已完成**：随 `b889808` 一并提交并发版。
> **真正剩余的用户待办只有两条**：**(2) 真机迁移**（用户空闲自跑 cc-acct-iso 管线，或用 A6 向导在设置里点着跑 + 删 ~/.bashrc 的 cc-account-block）与 **(3) A7 本地 Windows 切号**（future，需单独审批）；(4) 是计划内裁剪/建议、非缺陷。
> **过期修订(2026-07-25，account-ux Phase G 文档-代码交叉对比)**：本段原写着"发版待办"与"改动尚未 commit"，而写下它的那个 commit 本身就是 v3.2.0 —— 恢复入口自相矛盾，隔周回来的人会以为还没发版。已划掉。**已消项**：~~A6 部署向导~~(已完成)、~~优雅退出 V3~~(已完成)、~~tmux_send_keys 命令测~~(A5+ 补上)。

> **⚠️ 恢复记录(2026-07-24)**:上一段 session 的 "A4 步骤 4-5 done / cargo Finished / 62·54 green" 等日志**是终端污染(vitest --watch)伪造的,磁盘上根本没有那些改动**。核实真相:① `remote-launch.ts` 调 `buildEnvPrefix` 但**没定义**、② `accounts.ts` 用 `SESSION_ACCOUNTS_TTL_MS` 但**没定义** → 整棵树曾 **tsc 4 error 编译不过**;`history.rs last_account` / `remote-launch-run.ts configDir 透传` / `sessionBadge lastAccountByS` / `remote-launch-env.vitest.ts` **全部不在盘上**;`panel-groups.vitest.ts` 2 红(远端→账号改名漏改)。
> **已修复到真绿基线**:补 `buildEnvPrefix`/`isValidConfigDir` 定义 + `SESSION_ACCOUNTS_TTL_MS`(账号 30s / 会话 8s)+ 修 `panel-groups.vitest.ts`。**实测 `tsc --noEmit` exit 0 / `CI=true npx vitest run` = 34 文件 393 全绿**(此前"405"是伪造数)。
> **纪律(强制)**:每步 Read 回盘核实 + 真跑 tsc/vitest **重定向到文件再 Read**,绝不信内联"绿"、绝不跑任何 watch。
> **A4 已真做完并验证(2026-07-24)**:
> · step3 余:`remote-launch-run.ts` 4 个 runner(resume/resumeTmux/newSession/launcher)透传 `configDir?`(attach 不带,只重连);
> · step4:`history.rs` `EntryMetadata.last_account: Option<String>` + `MetadataPatch.last_account: Option<Option<String>>` + update 分支(照 customTitle:`rename="lastAccount" alias="last_account"` + `filter(非空白)`;**注:真实 codebase 用 plain serde default 非 double_option,null 折叠为不改、清空靠空串**)+ serde 契约测(向后兼容/camelCase/三态/apply);
> · step5:`accounts.ts` sessionBadge 加 `lastAccountByS?`(源①live→源②lastAccount 标"上次用本工具起"→源③ —)+ 5 测;
> · env 测:`remote-launch.test.ts` +6(buildEnvPrefix 空=""/合法/非法 throw、isValidConfigDir、三 builder 带 configDir 前缀在 unset 前、无 configDir 逐字节回归);
> · A3 基线修:`SESSION_ACCOUNTS_TTL_MS`(30s/8s)、`panel-groups.vitest.ts`(远端→账号组 + mock AccountsSection)。
> **完整实测**:`tsc` 0 err / `npm test` = 34 vitest 文件 **398 测** + 全 tsx 绿 / `npm run build` ✓ / `cargo test --lib` **349 passed 0 failed**。**真绿基线达成。**
> **A4 step6b 核心 + 6d 已做完验证(2026-07-24)**:
> · `accounts.ts` 加纯函数 `accountConfigDir(state,name)`(账号名→configDir,仅可选账号,否则 null)+ 4 测;
> · `history.ts`:`RowActionCtx` 加 `account?`;`runResume` 带账号时 `fetchAccounts`→`accountConfigDir` 解析注入 `runRemoteResume(...,configDir)`,不可选则 toast + 退化默认(不猜);起成功调 `recordLastAccount`→`invoke(update_history_metadata,{lastAccount})` 写源②;`showEntryMenu` 远端会话**异步追加**「用账号 X resume」项(每可选账号一条,≥2 才出,§7 降级安静不加,校验菜单仍开);`EntryMetadata` 接口加 `lastAccount?`;
> · `styles.css` 加 `.history-context-sep`。
> · 实测:tsc 0 / npm test 34 vitest **402 测** + 全 tsx / npm run build ✓。
> **A4 step6c + 6a + §7 门控 已做完验证(2026-07-24 续)**:
> · **6c 新会话带账号**:`remote-section.ts`「开新 Claude」对话框加账号下拉(异步 fetchAccounts 填 isSelectable,预选 effectiveDefault,账号库不可用则整行不显=不注入);起时 `accountConfigDir` 解析 → `runRemoteLauncher(...,configDir)`。`styles.css` 加 `.launcher-field select`。(决定:history.ts runNewSession 不加 account——新会话带账号统一走此对话框,避免菜单臃肿+死代码。)
> · **6a lastAccount 数据管道**:Rust 加只读命令 `list_last_accounts`(sid→lastAccount)+ 纯 helper `last_accounts_of` + 测,注册进 lib.rs;`main.ts` refreshSessionAccounts 拉一次 → `tabs.setSessionAccounts` 加第 3 参 `lastAccountByS` → sessionBadge 源②真正生效。
> · **§7 徽章门控(修 summary 伪造的缺口)**:`accounts.ts` 加纯函数 `shouldShowAccountBadge(origin,readyOrigins)`(本地/daemonless/未迁移不显徽章,避免满屏 —)+ 3 测;`tabs.ts` setSessionAccounts 加第 4 参 `readyOrigins` + updateAccountBadge 先门控;`main.ts` 收集 available 的 origin 传入。
> · 实测:tsc 0 / npm test 34 vitest **405 测**(+accountConfigDir 4 +源② +shouldShow 3) + 全 tsx / cargo test --lib **350** / build ✓。
> · 用上了 **code-picture**(reindex 2283 符号/175 文件 + search 定位;注:LSP 对 TS 方法跨文件引用不全,精确引用仍 grep 兜底)。
> **A4 step6 全部收官(2026-07-24)**:(6b 余)抽出共用 helper `peekSelectableAccounts`(同步读缓存,main.ts 10s 暖)+ `recordLastAccount`(history/tabs 共用写源②);`tabs.ts` resumeTab 加 account(注入 configDir + 记 lastAccount)+ 归档远端 tab 右键加「用账号 X resume」;`history.ts` 搜索卡片 header 加右键 → showEntryMenu(复用账号项);history.ts 私有 recordLastAccount 换共用。修 tabs.vitest 2 处旧签名断言 + 加账号-resume 正路测。实测:tsc 0 / npm test **412** / build ✓。
> **A4 全部完成 + D/E/F 已签收(2026-07-24)**:
> · **D 三视角并行审计**:正确性/安全**零阻塞零重要**(注入面闭合、与 daemon 逐码点对齐、回归门有测);计划符合**零阻塞**(五项磁盘核到真身、无谎报);架构/耦合**零阻塞**。建议/重要项**全修**:
>   - C1 控制符 `-` 对齐 daemon(remote-launch.ts + fromCharCode 测);
>   - history.rs null 折叠注释澄清(JSON null→不改,清空靠空串);
>   - `main.ts` 手抄 `!isSelectable` → 复用 isSelectable;
>   - **E 收敛重构**:抽 `withAccount(origin,name,run,opts)` 统一 history/tabs/新会话三站点的 resolve configDir + 降级 + 记 lastAccount(消除三份漂移 + 守卫不一致),+5 单测。**A5 换号重启是它的超集。**
> · 补建缺失的 `features/04-a4-inject-and-select.md`(含 DoD/落点/审计/签收)。
> · **最终实测**:tsc 0 / npm test 34 vitest **417** + 全 tsx / cargo test --lib **350** / build ✓。
> · 遗留技术债(低优先,已记账不阻塞):tabs 同步 peek vs history 异步 fetch 冷缓存可用性分裂 → A5 加"用账号 X 重启"时改异步追加(复用 tabs `resolveAttachMenuItem` 模式)一并收敛。
> **A5 进行中**(换号破坏性重启 + compact):
> · **Phase B done**:写 `features/05-a5-restart-compact.md`。可行性核实:④kill=`kill_remote_tmux` 已有 / ⑤带账号 resume=`buildResumeTmuxCmd+configDir`(走 withAccount) 已有 / compact 检测=`isCompactSummary` 已有 / ③send `/compact` = **需新增 headless `tmux_send_keys`**(照 kill_remote_tmux,走一次性 ssh,daemon 不参与)。承载=**两条菜单项**(重启/重启先压缩,避开无 checkbox 的 confirm)+ toast 进度。compact **默认关**。
> · **Phase C 步骤1 done**:Rust `tmux_send_keys(origin,target,keys)`(headless send-keys,target 限本工具 `cc-*` 名 `is_ccm_tmux_name` 防误发,keys shell_quote)+ 注册 lib.rs + 白名单测。cargo test --lib **351**。
> · **Phase C 步骤 2/3/4 done**(2026-07-24):
>   - **步骤2 tabs 菜单异步追加重构**:加 `appendTabContextMenuItem` + `appendAccountMenuItems`(远端 tab 菜单开后异步 fetchAccounts,复用 F51 `tabMenuGeneration` 代次守卫;归档→「用账号 X resume」/ 活→「用账号 X 重启…」+「(先压缩上下文)」danger);删同步 peek 循环 → **A4 冷缓存分裂债已收敛**。TabMenuItem 加 `title` tooltip。
>   - **步骤3 编排器 `src/account-restart.ts` `restartWithAccount`**:①预检(accountConfigDir 校验 + checkTrust 只警告)→②window.confirm(可注入)→③[可选] tmux_send_keys `/compact` + awaitCompact(可注入)→④kill_remote_tmux(**失败即 return 中止不续⑤**)→⑤runRemoteResumeTmux(...configDir)→⑥recordLastAccount + toast。**7 单测**覆盖 §5.2 全部失败语义(不可选不动手/取消/compact 失败不阻断/超时不阻断/kill 失败中止/happy 记账)。
>   - **步骤4 tabs `restartTabWithAccount`**:活 tab 右键接编排——先 list_remote_tmux + findClaudeTmux 解析当前 tmux 名(send-keys/kill 目标),不在 tmux→提示无法重启;传给编排器。
>   - 实测:tsc 0 / npm test **424**(35 文件,+7 编排器) / build ✓。
> · **Phase C 步骤 5-6 done → A5 实现全完**(2026-07-24):
>   - **步骤5 compact 真检测器**:`cards/index.ts` 加导出 `isCompactRecord(message)`(role:user + `extractText`→`stripInternalNoise`→`isCompactSummary`,与卡片渲染同判定)+6 单测;`tabs.ts` onLine(gated on `compactWaiters.size`,常态零开销)见该 sid compact 摘要行即 resolve;`awaitCompactFor(sid,5min)`=waiter race timeout,两路清理防泄漏;restartTabWithAccount 注入之,替代 90s 盲等。+4 waiter 单测(tabs.vitest mock ./cards 补 isCompactRecord)。
>   - **步骤6 终验**:tsc 0 / npm test **434**(36 文件) / cargo test --lib **351** / build ✓。真机零改动:A5 写路径仅 recordLastAccount(本机 metadata)+ tmux send-keys/kill/resume(远端 ssh),**不碰用户 ~/.claude**。
> **A5 D/E/F 已签收(2026-07-24)**:
> · D 三视角:正确性报 **1 阻塞**(restartTabWithAccount 缺 `live.sid===sid` 守卫→降级远端 cwd 回退可 kill 错会话/双进程)**已修**(精确守卫 + 3 锁定测);计划符合零阻塞零谎报(亲验 tsc0/vitest/cargo);架构裁定 **withAccount 与 restartWithAccount 应分离**(语义不兼容,已加注释)。
> · 清理:删死代码 `peekSelectableAccounts`+4 测、删 `_cwd` 死参、confirm 带 tmux 名、④ 加 S2 优雅退出 TODO、INVARIANTS 补 tmux 名跨语言契约条目。
> · 遗留(记账,不阻塞):抽 `resolveLiveTmux` DRY 三处 tmux 解析(纯 DRY,correctness 已由守卫覆盖,可选后续);R1 tmux_send_keys 命令构造测(内联难测,同 kill 惯例)。
> · 实测:tsc 0 / npm test **433** / cargo test --lib **351** / build ✓。features/05 三签收打勾。
> **A2–A5 预批全部完成签收** → **进 Phase G**:对整个 account-isolation 功能族(A0–A5)整体验收(/full-audit 或聚焦集成审计 + 文档-代码交叉 + 端到端全量构建测)+ 收尾汇报交回用户。**A6 部署向导 / A7 本地 仍需用户单独批。真机迁移仍待用户自跑管线。**
> A6 部署向导仍需单独审批。

- **A3 进度**(步骤 1-3 done):
  · `src/accounts.ts` store(三命令包装 + 8s TTL 缓存 + config.json defaultName + 纯函数 deriveUi/effectiveDefault/sessionBadge/isSelectable/badgeText)+ `accounts.vitest.ts` **32 绿**。
  · `src/account-chip.ts` 状态栏「当前账号」chip(绑第一台可用远端 pickPrimaryOrigin;浮层选单照 SFTP host-picker;选账号=只改 config.json defaultName + info toast「已有会话不受影响」;降级安静隐藏)+ CSS + 挂进 main.ts(初始 refresh + SETTINGS_APPLIED 时刷新)+ `account-chip.vitest.ts` **11 绿**(共 43)。
- **A3 进度**(步骤 4-5 也 done):
  · `src/settings/accounts-section.ts` 设置「账号」组(占原「远端」空占位组,id 沿用;远端选择器 + 账号表[名/邮箱/mode/登录态/configDir/默认] + 设为默认/复制路径/刷新;未启用时给 manifest 路径 + 手动 init 提示[A6 部署向导占位];改默认 emit SETTINGS_APPLIED 让主窗 chip 同步)。挂进 `panel.ts`(组名「远端」→「账号」)。
  · Ctrl+K 命令:`账号:切默认为 X`(只读,读 accountChip.snapshotReady() 同步缓存)+ `账号:管理…`。AccountChip 加 `snapshotReady()`/`applyDefaultByName()`。
  · tsc 无 account/panel/main 错误;account vitest 仍 43 绿。
- **A3 进度**(步骤 6-7 done,全 7 步完成):
  · tab 账号徽章:`tabs.ts` 加 `acctBadge` span + `setSessionAccounts()`/`updateAccountBadge()` + `snapshotSessions.account` + import sessionBadge;`session-status.ts` GridSessionSnapshot += account;CSS `.tab-acct-badge`(未知 .unknown 弱化)。
  · 数据管道:`main.ts` 定期(10s)遍历远端 fetchSessionAccounts + fetchAccounts 聚合喂 `tabs.setSessionAccounts`(放在 tabs 构造之后,避免 TDZ)。
  · **全量验证**:`npm run build`(tsc+vite)通过无 error;`npm test` **405 vitest 全绿**(新增 accounts 32 + account-chip 11 + panel 测适配 + grid-monitor snap 补 account)。
- **A3 Phase D**:3 视角审计中(正确性+集成 / 计划+架构 / ?)。改了 tabs.ts 核心文件是审计重点。
- **实现决定(已定)**:多台远端 chip 绑"第一台非 daemonless 远端"(pickPrimaryOrigin);多台完整分组切台留作 A3 增强或后续。
- 用户拍板(2026-07-23):**M1=独立 skill `cc-acct-iso`** / **`.claude.json` 隔离** / **A3=一次到位全套 #68** / **迁移=全迁,模型 = V2 纯分离** / **A0=我(在 aya 本机=账号所在的"远端")只读探测**。
- 拓扑注:本 CC 实例跑在 **aya**(= 账号所在的 Linux/cc-monitor 眼里的"远端"),故 A0 探测即本机跑、无需经 daemon。
- **V2 结构**:`~/.claude` = 纯共享库;每个号 = `~/.claude-accts/<name>/`(真 `.credentials.json`+`.claude.json`+`backups/` + 共享项 symlink);全部靠 `CLAUDE_CONFIG_DIR` 起,A3 无特例。
- **A1 产出**:`~/.claude/skills/cc-acct-iso/`(SKILL.md + scripts/{cc-acct-iso,lib.sh,cc-acct-iso-install.sh,test/run-tests.sh} + examples/config)。166 条沙盒断言全绿、shellcheck 干净、**真机 `~/.claude` 零改动**(迁移由用户空闲自跑)。
- **设计基线**:`DESIGN-account-switching.md`(cc-monitor 侧全部交互与底层细节的单一来源;15 条已验证事实、三种"切账号"语义、换号重启完整编排、降级矩阵、安全边界)。
- **用户 2026-07-23 拍板(三条,已生效)**:
  1. **批准 A2→A5 连续跑**(不必每个 feature 停下来等批)。A6 部署向导仍需单独审批。
  2. **换号重启默认不 compact**,做成可勾选。
  3. **A2–A5 全做完再一起发一版**(中间用"功能不可用"降级路径开发,本地手工部署 dev 版 daemon 验证)。
- **停止条件(仍然有效)**:阻塞 / 计划≠现实 / 同一步 ≥2 次失败 / 需要新决策 / A2–A5 全完成 → 停下交回用户。A6 部署向导按约定仍需单独审批。
- **loop 节奏**:用户要 60s 固定间隔全量续跑。每轮推进一个干净检查点(一个 feature 走 C→F,或大 feature 拆成几个可测子步)。
- **用户待办(不属于我)**:① 空闲时跑迁移管线(`init <名>` dry-run → `--apply` → `verify` → `shellinit` 贴 rc → `add <第二号> --from-credentials ~/.claude/accounts/<号>.json --apply`);② 迁移后删掉 `~/.bashrc` 里的 `cc-account-block`。

## 真机现状(只读核实 2026-07-23,A1/A3 设计输入)
- 旧机制 = `~/.bashrc` 的 `cc-account-block`(swap `~/.claude/.credentials.json` + `zcc/bcc/cc/cct` 函数),快照库 `~/.claude/accounts/{z,b}.json`,`.last=b`,live == `b` ⇒ **现默认号=b**。
- **`z` 的凭据仅存在于快照文件** ⇒ `add --from-credentials <file>` 是必需功能(否则 z 要重登)。
- `~/.claude.json` 在 **$HOME** 下(非 config dir 内)、其 `oauthAccount` 与 live 凭据不同步(swap 不动它)。设 `CLAUDE_CONFIG_DIR` 后 `.claude.json` 落 `<cfg>/`、**不回退读 $HOME** ⇒ 这正是弃用 V1(会分裂成两份)的原因。
- 迁移完成后应退休 `cc-account-block`(工具 `shellinit` 打印替代片段,**不擅自改用户 rc**)。

## A0 结果(2026-07-23,PASS)
- **实测**(本机 CC 2.1.218):`CLAUDE_CONFIG_DIR=/tmp/ccprobe.$$ claude mcp list` → 在该目录建了 `.claude.json` + `backups/`,且**没读**真 `~/.claude.json`(用户级 MCP server 消失,只剩项目级 code-picture 显 pending approval = 全新空 config 无审批记录)。⇒ CLAUDE_CONFIG_DIR **确实把 config dir 整体重定位**。
- **文档**(claude-code-guide 查证):authentication.md 明文——设了 CLAUDE_CONFIG_DIR(Linux/Windows)则 `.credentials.json` 落该目录而非 `~`(**HIGH**)。`.claude.json` 重定位文档未明说,但 "every ~/.claude path" 措辞隐含(MEDIUM-HIGH)+ 我实测已证。
- ⇒ 两个不同 CLAUDE_CONFIG_DIR = 两套 credentials + `.claude.json` = **并发隔离成立**。**A0 PASS**。
- **文档 caveat + 缓解**:symlink 跨账号共享是**未文档化**用法;`projects/`(auto memory)并发写有理论竞争。但**单账号今天已多会话并发共享同一 `~/.claude/projects/`**(cc-monitor/tmux 多开)= 该竞争**现状已存在且被容忍**,隔离凭据 + 共享 projects **不新增**风险类别。设计保留 `.claude.json` 隔离(已定),`projects/` 照用户意图共享。

## Features
- **A0** ✅ PASS(见上)。
- **A1** ✅ 通用隔离 shell 模块 cc-acct-iso + manifest。3 视角审计 6 阻塞/11 重要全修 + 50 条回归断言。
- **A2** ✅ daemon 账号能力(`--list-accounts`/`--session-accounts`/`--account-trust`,只读+BUILD_ID bump)。3 视角审计,修 R1(PID 复用误归属)/重要-B(特殊文件 OOM)/R2(export 前缀)+ Unicode 两端对齐。daemon 124 测 / monitor 348 测全绿。
- **A3** ✅ 账号模型 + 全局切换 UI(`accounts.ts`、状态栏 chip、设置面板、命令栏、会话徽章)。
- **A4** ✅ 按账号启动/resume(`buildEnvPrefix` 注入 + 选账号 UI 四落点 + `lastAccount` 记忆 + `withAccount` 收敛)。D/E/F 三视角零阻塞签收,417 vitest/350 cargo/build 绿。
- **A5** ✅ 换号破坏性重启 + compact(旧号先 compact 再换 · `tmux_send_keys`/`restartWithAccount`/真检测器 · kill 失败中止防双进程)。D/E/F 三视角签收(1 阻塞守卫已修),433 vitest/351 cargo/build 绿。
- **A5+** ✅ 换号重启「优雅退出」(§5④,`features/07`):④ 从直接 kill 升级为 `Escape`(打断)→`/exit`+Enter→有界等 10s(`awaitExitFor`/`claudeExited` 轮询)→`kill` 兜底。Rust 提纯 `build_send_keys_remote_cmd`+`tmux_send_keys` 加 `enter?`(默认 true 向后兼容)。D 两视角零阻塞零重要(无 resume-while-alive 窗口)+ hardening S1。453 vitest/352 cargo/build 绿。
- **A6** ✅ app 内部署向导(`features/06`):**纯前端零新增 daemon 面**。只读状态用既有 `list_remote_accounts`;dry-run/verify/`--apply`/sync/`/login` 全经既有 `launch_remote_terminal` 弹终端。纯构建器 `src/settings/acct-deploy.ts`(校验+POSIX 单引号,与 launch.rs 双层防线)+ `accounts-section.ts` 启用向导/维护区/每行登录按钮。D 两视角零阻塞(I-1 login=`run <名>` 裁定为改进,回写 DoD)+ hardening S2/S3。§6 dry-run 张力已裁定回写 DESIGN。453 vitest/build 绿。
- A7(future):本地 Windows 账号隔离(A6 向导纯前端、正交,不拖累)。

## 后续功能提案（非本族计划内，待用户批）
- **账号用量 `/usage` 抓取**（2026-07-24 用户提，**issue #73** + 草案 `doc/账号用量-usage抓取方案.md`）：现有 usage(#52) 只是 jsonl token 会计，**看不到 plan 额度**(5h/周窗口%)；plan 额度**无非交互 API/命令**(claude-code-guide 查证)、唯一路 = 让 claude 渲染 `/usage` 再 `capture_remote_pane` 抓屏。机制可行(复用 `tmux_send_keys`+`capture_remote_pane`,**零新增 daemon**)、on-demand 快照(避 API 限流)、临时会话隔离(不侵入用户现场)。待批做/不做 + 承载形态(原样抓屏 overlay vs 结构化解析)。
- **远端 agent 查看器 / 远端代码全景图**（草案 `doc/远端支持方案-agent查看器与代码全景图.md`，均需动 daemon+发版）。

## A2 完成后的账本补充(A3+ 必读)
- **manifest 加了 `updatedAt`/`mode` 两字段** + 五条消费方规则(见 MASTERPLAN §契约)。A3 的 `accounts.ts` 按此消费。
- **daemon 三命令的 monitor 包装在 `src-tauri/src/accounts.rs`**:`available:false` 区分了"daemonless"与"版本过旧",A3/A5 据此分支降级。
- **`--session-accounts` 的账号归属经过 procStart 对拍**:`account:null` = 确实不知道(不猜),A3 徽章遇 null 显示 `—`。
- **~~BUILD_ID 已 bump 但内嵌 daemon 未重编~~（**已由 v3.2.0 的 CI 交叉编译解决**）：原文 BUILD_ID 已 bump 但内嵌 daemon 未重编**:`cargo check` 会警告"内嵌 daemon 旧"——这是预期,A2–A5 一起发版时解决。本地验证需手工 `cargo zigbuild` 部署 dev daemon。

## 关键红线
- 不擅自改用户 `~/.claude`(迁移由用户跑管线;我只沙盒测)。
- 不读/传凭据 token 内容。
- 不 push/发版除非用户拍板。
- 不改 `cc-<sid8>` 会话名(§3)。

## 自动模式 / 停止条件
- 门禁:Phase A masterplan(已过)+ 每个 feature 计划(B)未获用户批准前**不起 loop、不动码**。
- 批准后可 /loop 连续跑 A1→A3(A0 已 PASS)。阻塞/计划≠现实/≥2 次失败/全完成(Phase G)→ 停。
