# BACKLOG — 跨功能族的遗留项集中登记

> 建于 2026-07-25(account-ux Phase G)。**动因**:文档审计发现遗留项散落在至少 12 个文件、5 种不同
> 标题下(「遗留 fast-follow」「已知取舍」「不做(防蔓延)」「流程欠账」「后续功能提案」…),
> 没有单一入口 ⇒ 出现过**承诺断链**:U6 明确写「⚠k 汇总浮层顺延 U8」,而 U8 收官时它在任何文件里
> 都没再出现过——需求无声蒸发。
>
> **规矩**:凡在 feature/STATUS/MASTERPLAN 里写下"顺延到 X"「记进待办」「future」的,**同时**在这里登一行。
> 每条注明来源,便于回溯。此文件只登记,不做决策——是否做由用户拍板。

## A. 已承诺但尚未兑现(**优先**:这些是断链风险最高的)

| 项 | 来源 | 状态 |
|---|---|---|
| **⚠k 汇总浮层**:点 chip 的 ⚠k 开浮层列出不一致会话,每行「→对齐」,底部「全部对齐(k)」 | `account-ux/features/06-u6-mismatch-align.md`「DoD 表 · 汇总浮层 ⏭ 顺延 U8」 | **未做,且在 U8 断链**。U8 用两步 `window.confirm` 顶替(已逐行列出"会话→目标账号"),功能上可用;浮层的增量价值 = 逐行取舍 + 非模态可读(k 大时 confirm 很难看)。**待用户定:做 / 正式取消** |
| chip 菜单没走 `pushOverlay` ⇒ **Esc 会同时关掉 chip 菜单和背后的 overlay**(历史/设置) | `account-ux/features/08-u8-discoverability.md`「已知取舍」;A3 起既有缺陷 | 未修。修它要动 A3 的菜单生命周期(实现 `OverlayHandle` + push/popOverlay),超出 U8 范围 |
| **产品文档缺口**:README 停在 v3.0.0(实际 3.2.0)、**零账号功能覆盖**;仓内无账号用户文档 | Phase G 文档审计 B1/I1 | **未做**。详见下方 §D |

## A2. Phase G 四视角审计新增(2026-07-25)

**已修(本轮)**
- ~~`restartWithAccount` 把"走到第⑤步"当成"已 resume"~~ **已修**:`runRemoteResumeTmux` 返回 void 且
  自吞两条失败路径 ⇒ 会话被 kill、没起来,却照样记 pin、报成功、被批量计成成功。失败是**确定性**的
  (F34 launcher 含双引号被 `launch.rs` 拒 / tmux 名不合白名单 / 缺 OpenSSH)。→ 改返回 boolean,
  未拉起时不记账 + 明确 error toast + return false。**加了返回值契约测 + 变异验证。**
- ~~`mcp-section.ts` 含裸 NUL 字节~~ **已修**:该 643 行模块(含 4 个**写**命令调用点)被 `file` 判为
  `data`,**grep 静默跳过**——而本仓的跨切面安全网就是 grep(多处文档明写"改前先 grep")。改 `\u0000` 转义。

**未修(报给用户,按优先级)**
| 项 | 来源 | 说明 |
|---|---|---|
| **daemon 只读无自动化门禁** | G1-B1 | daemon 当前**确实**只读(三重证据:无写调用、流模式不读 stdin、一次性查询是封闭 match),但 `watcher.rs` 里有 `sh -c` 跑 `tmux ls`,**把它改成写只需改一个字符串字面量,CI 全绿**。而本仓自己已确立"硬约束⟹测试强制"的范式(§26 护栏有 `every_capability_token_is_strippable`)。**最重要的不变量是唯一一条纯靠人审的** |
| **CSS 零门禁 + 37 个孤儿类名** | G3-1 | `styles.css` 5964 行、本轮 +386,无 stylelint/无类名对账。TS 引用了 37 个 CSS 里不存在的类(如 `.launcher-acct-select`——账号新会话下拉,A0–A6 引入,一直没人发现)。tsc 看不见字符串类名、vitest 断言 classList 而非样式、build 只做语法解析。**我这轮亲手栽在 CSS 上一次(注释提前闭合)** |
| **`main.ts` 1054 行零测试覆盖** | G3-2 / G1-I2 | 全仓仅 3 个源文件从未被任何测试加载,`main.ts` 是最大的一个,且是所有接线根 |
| tabs.ts 的 follow-resume 从**懒填充内存表**读 pin | G4-2 | `accountLastByS` 在首轮轮询前/`list_last_accounts` 抛错/远端关闭时是空的 ⇒ `withAccount` 的不-clobber 判据失效 ⇒ 可能用 current 覆盖磁盘上真实的 pin。history.ts 入口是**现读**的(U3 审计修过),tabs.ts 两处没跟上 |
| 显式选号解析不到时**静默落基座** | G4-3 | 3 个调用点里 2 个不传 `onUnselectable` 完全静默;history 那条的文案描述了一条**代码里不存在**的回退 |
| 破坏性重启的 tmux 目标不保证 `cc-*` | G4-4 | 用户在自己的 `tmux new -s work` 里跑 `cc` 也会带 `@ccm_sid` ⇒ compact/优雅退出被后端白名单拒(降级为直接强杀),且 `kill-session` 会端掉该 session **所有** window |
| `refreshSessionAccounts` 无重入/顺序保护 | G4-5 / G1-I3 | 慢的旧快照可覆盖新快照,把 U6 审计刚关上的"切号反向窗口"从并发侧重新打开 |
| `defaultName` 全局但缓存按 origin,失效只清一台 | G4-6 | 多远端下非主远端最长 30s 用旧账号判定,且 follow 会按旧账号**持久化** pin |
| 重启成功后 live 探测最长 ~18s 不刷新 | G4-7 | ⇄ 一放开又"够格",用户以为没生效再点一次 → 把刚起来的进程再杀一遍 |
| `TabManager` 上帝类(2427 行/74 方法) | G1-I1/P1 | 每族新功能都改它,本轮 +274 行。建议按缝拆:账号切面 → `AccountAlignmentService`(顺带解决 `setSessionAccounts` 形参膨胀) |
| `settings/remote-section.ts` 分层倒挂 + 1 个真循环依赖 | G1-I4 / G3-7 | 数据层函数住在设置 UI 里,被 7 个非设置模块依赖;与 `views/port-forward` 构成值依赖环 |
| `account-commands.ts` 的 `alignSession(sid)` 签名仍锁"冻住的 sid" | G4-9 | U8 只修在接线处,纯函数签名和它的 11 条测仍锁着旧契约,第二个接线者会踩回去 |
| vendor `code-picture-core` 的 25 个测试 CI 从不执行 | G3-8 | 无 `[workspace]`,`cargo test --all` 跑不到它 |
| `npm test` 把 564 测排在 179 测之后 | G3-3 | `&&` 链短路 ⇒ 最便宜的测试守着最贵的测试的门 |
| TS 侧零 lint / 零 formatter | G3-9 | 全仓唯一强制的风格门禁是 Rust 的 `cargo fmt --check` |

## B. 用户侧待办(需要你本人在真机上做,我不代劳)

| 项 | 来源 | 说明 |
|---|---|---|
| **真机迁移** | `account-isolation/STATUS.md` 遗留项 (2) | 空闲时自跑 cc-acct-iso 管线(或用 A6 设置向导点着跑)+ 删 `~/.bashrc` 的 cc-account-block。**红线:我不碰你的 `~/.claude`** |
| **A7 本地 Windows 切号** | 同上 (3) | future,需单独审批。现在账号功能**只支持远端** |

## C. 计划内裁剪 / 已知取舍(非缺陷,记账备查)

- `resolveLiveTmux` DRY;`DEFAULT_COMPACT_WAIT_MS` 90s 准死常量;非 `cc-*` 会话 compact-skip toast 文案;
  daemon 结构化 reason code(发版前);A6 未做 rollback 向导化(高危,**故意**不做)
  — 来源 `account-isolation/STATUS.md` (4)
- **per-origin 当前工作账号**(现在是全局单值):`account-ux/MASTERPLAN.md` 账本记为 future
- **U9(可选)解钉「让此会话跟随当前账号」**:清 sid 的 lastAccount。`account-ux` 唯一未做的功能。
  **开工前需先定义**:解钉后若 live 进程仍在旧号上跑,是否还算"不一致"(U6 已提出,未决)
- **对齐的「先压缩上下文」变体**(复用 `compactFirst`):U6 的新入口硬编码 false,右键菜单仍有该变体
- U3/U4/U5 **没有 feature 文件**(`account-ux/STATUS.md` 记为"流程欠账",决定不回溯补建)。
  文档审计建议**例外补 U3**——它引入的「粘性优先 + sticky clobber 防护」是全轮语义最微妙的改动,
  现在只能从 STATUS 的一段速记里考古
- **issue #61**(tab 移动顺序 / 归类 / 锁定灰 tab):用户提过,**本轮完全没做**,是独立功能

## D. 文档债(Phase G 文档审计,按优先级)

**P0(与 account-ux 是否合并无关——都是**已发版** v3.2.0 的内容)**
1. **README 补账号功能 + 版本对齐**:`README.md:5` 与 `:259` 的 `v3.0.0` → 实际 `3.2.0`;
   删掉 `:9` 对不存在的「CHANGELOG [未发布] 段」的悬空引用;「功能」一节加「多账号」小节
2. ~~`account-isolation/STATUS.md` 遗留项过期~~ **已修(2026-07-25)**
3. **仓内补账号用户文档**(建议 `doc/ACCOUNTS.md`):cc-acct-iso 是什么 / 从哪装(它**不在本仓**,
   在 `~/.claude/skills/cc-acct-iso/`)/ 迁移五步 / 设置向导怎么用 / 降级矩阵
4. **`doc/RELEASING.md` checklist 加两条**:README 版本号两处 + README 功能列表 —— 不修这条,
   下次发版 README 还会漏(这是连续三次漏改的**机制性根因**)

**P1(account-ux 合并/发版时一起做)**
5. `CHANGELOG.md` 加 `## [未发布]` 段(本仓既有惯例);**注意本轮纯前端、daemon 零改,不 bump 版本**
6. `doc/ARCHITECTURE.md` 模块树补账号子系统 + 加一段「账号解析优先级」
   (`显式 > lastAccount 粘性 > 当前工作账号 > 基座`——全族最容易被后人改错的语义)
7. `src/README.md` / `src-tauri/README.md` 补账号模块行
8. README 设置面板一节按 F82b **4 组终态**重写 + 加「账号」组;快捷键表加 `Acct` 分组;
   删掉写死的「26 个可用 action」(实际 30,且三处数字互不相同)

**P2(过程文档卫生)**
9. `.claude/planned-build/` 加索引 README:哪些族活着 / 收官 / 冻结(现在得逐个打开才知道)
10. 把 `account-ux/MASTERPLAN.md` 里的**仓库级**事实上移 `doc/INVARIANTS.md`:
    本仓**没有浅色主题**(`color-scheme: dark`,无 `prefers-color-scheme`)、`theme.ts` 的 TOKENS
    只覆盖 11 个 token(`--accent`/`--border-*`/`--overlay-hover`/`--text-faint` 不可换肤)
    ⇒ **以后功能的 DoD 别再写「明暗主题各扫一眼」**,改写「零硬编码颜色 + var 全部有定义」
11. MASTERPLAN 变更记录只记「账本形态变了什么 + 为什么」,实现叙事留在 `features/`
    (现在 account-ux 的 MASTERPLAN 有 32% 是变更记录,与 feature 文件重复)

---

## E. unify-launch + integrate-toolchain 两工作区遗留（2026-07-29 Phase G 登记）

> **这一节本该在写下「顺延」那一刻就存在。** Phase G 文档视角审阅把它的缺失点为阻塞 B2：
> 本轮在 feature/STATUS/PHASE-G-DRAFT 里写了十几条「未收」「登记」「不做」，
> **BACKLOG 里一条都没有**——`grep -n "R15\|R16\|B03" BACKLOG.md` 零命中。
> 也就是本文件头注亲自定义的那个失效模式（U6→U8 断链）在本轮完整复现了一次。

| ID | 项 | 来源 | 状态 / 说明 |
|---|---|---|---|
| E1 | **R16：`views/history.ts` 账号 resume 菜单缺基座逃生口** | `unify-launch/STATUS.md`「R15/R16」 | **未做，疑为活 bug**（= issue #75 场景）。`appendAccountResumeItems` 没有「基座」项，用户无法从菜单显式起一个不注入账号的会话。原文写「计划文里查无决策记录，疑为遗漏」 |
| E2 | R15：`LaunchContext.passThrough` 纯透传子集 | 同上 | 登记，暂不做（收益是类型收紧，无行为变化） |
| E3 | **T01-P6 + ccm CLI 写坏不回滚** | `T01` §12 · `sftp.rs:843-847` 注释 | 两条都等同一个前置：`ProfileFs` 六闭包注入层（`exists`/`read_to_string`/`metadata`/`create_dir_all`/`copy`/`atomic_write_string`）。ccm CLI 读回不符**不回滚**，损坏的 CLI 留在远端，而下次 probe 可能仍报 installed |
| E4 | `remote-section.ts` 的 `refresh()` 全函数零 try/catch | T07 §9 | 今天不炸只因 `readRemoteConfig` 自带兜底「永不抛」——**靠被调方的性质，不是靠自己的结构** |
| E5 | `accounts-section.ts` 四处（102/130/415/519）整条链在 try 外 | T07 §9 | 同 E4 一类 |
| E6 | B03 的 M1-M6 | `unify-launch` B03/B04 计划 §12 | 其中 M6 = `CcBusAgent.pane` 解析了没有任何读者 |
| E7 | 族 B 5 处「复制诊断」重复实现 | T03 §9 | `paste-block.ts` 已收编 3 处，这 5 处是另一族（诊断文本复制），未收 |
| E8 | 卸载缺「强制清理 cc-monitor 块」入口 | T04 §13 | 悬空 BEGIN 时 `find_pair` 一律 `Err` 中止（这是对的），但用户没有任何出口把坏块清掉 |
| E9 | 24 处构造期 I/O 改懒加载 | T07 §5 | **明写不做**（实测只有约 11 处真在构造器里；改造面大、收益是启动延迟，不是正确性） |
| E10 | `(Remote, LocalGlob)` 组合不可达但有意保留 | T02 §8 | 保留理由是 `HostScope` 与 `PathResolution` 正交，不为省一个不可达组合去耦合两个维度 |
| E11 | **八套 CI 真机套件没有「最小断言数」地板** | Phase G 代码工程视角审阅（阻塞 2） | **未做**。每套收尾只有 `[ "$FAIL" -eq 0 ]`，加上 3 处静默 SKIP 分支，任何环境退化都是「少跑若干条 + 绿」。本仓 `structural_scan.rs::require(min_checked)` 已经把这道自检做成调用方想忘都忘不掉，**Rust 侧做到了，价值更高的 shell 侧没做**。做法：每套收尾加 `[ "$PASS" -ge <实测数> ]`，数字取一次真机运行的实测值（静态 `ck`/`chk` 调用数 ≠ 运行期断言数，两者差得很远）。**2026-07-29 Phase G 已实测，直接用这组数即可**：`tmux-target` 26 · `ccm-cli` 44 · `ccm-print-parity` 12 · `ccm-acceptance` 15 · `ccm-pretrust` 13 · `cc-spawn-uplift` 21 · `tmux-guarded` 14 · `usage-probe` 7（合计 152）。前 7 套尾部已打印 `PASS=<n>`，`cc-spawn-uplift` 没打印、要先加 |
| ~~E12~~ | ~~`ci.yml` 断言条数标签与实际不符~~ | 同上（阻塞 3） | **已修**（2026-07-29）：标签按实测改为 ccm-cli 44、cc-spawn-uplift 21，表头改为「8 套 / 152 条」并逐套列出 |
| E13 | `vendor/cc-acct-iso/scripts/` 1348 行 bash 在 shellcheck 门禁之外 | 同上（重要 4） | 它被 `include_bytes!` 进二进制并部署到远端执行，**与当初把 `shared/ccm` 纳入门禁的论证一字不差**。实测今天 `shellcheck --severity=error` 零告警 ⇒ 扩进来是零成本 |
| E14 | 6 套已自动化的 e2e 套件不在 `package.json` 也不在 `ci.yml` | 同上（重要 5） | `graylight-suite` / `graylight-daemon-frames` / `restart-suite` / `restart-daemon-frames` / `resume-suite` / `resume-daemon-frames`。只需 tmux + 一个 debug daemon，而 CI 的 `daemon` job 已经在编译 `remote-daemon-proto` |
| E15 | **两个渲染器对「未选账号（base）」不等价** | Phase G 整体设计视角审阅（阻塞 1，本席复现后降为「重要」） | CLI 渲染器吐 `--base` → ccm `unset CLAUDE_CONFIG_DIR`（强制基座）；兜底渲染器 base 态**零 env op** → `bash -lic` 继承登录 shell 里的 `CLAUDE_CONFIG_DIR`（继承语义）。**降级理由**：审阅把它归为 R11 同型，但 `shared/ccm:320` 显示「落 manifest 默认账号」只在**无继承值**时触发，而兜底路径没有 ccm、没有 manifest，R11 那个病灶在那条路上不存在。剩下的暴露面是「用户自己在远端 profile 里 export 的值被继承」——**那算 bug 还是用户本意，取决于「未选账号」是「不注入」还是「强制基座」，是产品语义决定，不在验收轮里单方面改远端启动语义**。待用户拍板 |
| E16 | 首次安装失败时不删新建的文件（本机 + 远端各一处） | Phase G 实现细节视角审阅（重要） | `backup_path = None` ⇒ rollback 闭包是空操作，而文案说「已尝试回滚」。**文案已在 Phase G 改成如实**（`sftp::rollback_note`），但「删掉新建的半坏文件」是行为新增（远端要 `remove`），未做 |
| E17 | 远端 hooks 诊断读死 `$HOME/.claude/settings.json`，不认 `CLAUDE_CONFIG_DIR` | 同上（重要） | 本机侧刚在 B04 修成尊重它，远端侧没跟上，而 `source` 字段照样报一个看着很确定的来源。改 `REMOTE_HOOKS_CMD` 里那一处定值即可，注入面不变 |
| E18 | 远端 `classify_command` 用 basename 匹配回答精确路径问题 | 同上（重要） | 远端 `exists` 闭包只比 basename ⇒ `PathMissing` 这一态对任何「装在别处」的程序**永远不可达**。远端已经有 `X\t<path>` 精确行可用 |
| E19 | `shared/ccm` 相对 `--cwd` 被应用两次 + 会产孤儿会话 | 同上（重要） | `tmux new-session -c` 的相对路径由 tmux **server** 解析，内层 ccm 再 `cd` 一次；且 `pane_current_path`（绝对、已解符号链接）与相对 `$cwd` 恒不相等 ⇒ 每次调用都退让到新的 `cc-foo-N`。Phase D 为预信任造过 `cwd_abs`，没推广到容器路径。**含符号链接的绝对路径同型** |
| E20 | `upload_atomic` 会把符号链接形态的 `.bashrc` 换成普通文件 | 同上（重要） | `try_exists` 跟随链接 → rename 搬的是链接本身 → 落一个普通文件 → 链接被删。dotfiles/stow 用户的 `.bashrc → ~/dotfiles/bashrc` 就此断开，而备份只存内容、链接不可恢复 |
| E21 | `config.json` 的丢失更新 + 固定 tmp 名 | 同上（重要） | 7 处写者全是 `load → 改整棵树 → save 整棵树`，Rust 侧无版本/无合并；`tmp = path.with_extension("json.tmp")` 是固定名，而同仓 `profile_installer::atomic_write_string` 特意加了 PID+时间戳「避免并行写碰撞」 |
| E22 | 「Claude 配置目录在哪」有 4 份独立实现 | Phase G 整体设计视角审阅（重要 9） | `hooks_diag.rs:415` / `paths.rs:33` / `mcp.rs:31` / `config_surface.rs:600`。`hooks_diag` 自己写着「若它自己再写一遍判定，两处就会各自漂移」——`config_surface` 遵守了（调 `hooks_diag::claude_config_dir`），另两处没有 |
| E23 | IR 之外的第 6 个手拼 builder | 同上（重要 2） | `src/remote-launch.ts:70-76` 的 `buildUsageProbePayload` 手拼 `export CLAUDE_CONFIG_DIR=…; unset <nested>; claude`，即 `account` + `nested-env-reset` 两个维度的手工复刻，由 F10（晚于 F03）新增 |
| E24 | `remote-launch.ts` 五个导出**生产零调用点**，却塑造了 IR 的类型设计 | 同上（重要 3） | `LaunchAccount.name` 的可选性 + `cliFlags` 返回 `null` 强制降级这条分支，唯一理由是服务这条只有测试在用的兼容面。按本轮那把 ≥2 尺子，这是一个**没有生产消费者的兼容面换来一条永久降级分支** |
| E25 | 五份 T 文档章节号重复（T02/T03/T04/T07 各有两套 §5-§8） | Phase G 文档视角审阅（阻塞 B3 后半） | 第一套一律「（待填）」+ 未勾选，第二套才是真内容。T01 是唯一做对的（续编成 §11/§12）。今天**没有**实际引用踩上去（现有跨文档引用都指向续编号的 §9/§12/§13），但二义性是latent。修法：照 T01 把第二套续编下去 + 每份最前加一节 ≤10 行「当前事实」 |
| E26 | MASTERPLAN 功能表状态列全面过期 | 同上（重要 I1） | `integrate-toolchain/MASTERPLAN.md` §1 里 T01-T09 全标「待做」而 5 个已交付；`unify-launch/MASTERPLAN.md` §1 表**完全没有 R00-R09 / B01-B04 的行**——三分之二的在跑工作只存在于 STATUS 的两张表里。它自称「单一事实来源」 |
| E27 | 文档里 4 处把 `doc/INVARIANTS.md` 的**行号 37** 写成了**节号 §37** | 同上（重要 I3） | `account-isolation/MASTERPLAN.md:73`/`:111`、`STATUS.md:11`、`features/07-a5plus-graceful-exit.md:60`。内容实际在 §1（位于第 37 行）；而今天真的存在一个 §37，讲的是维度 `applies`——**跟着引用走会落到完全无关的一节，比 404 更坏，因为读者不会察觉** |
| E28 | 已删的 `shared/ccm-wrapper.sh` 仍被 4 处当现存文件引用 | 同上 | 含 `account-onboarding/UNIFIED-PLAN.md:71` 对一个已删文件下达未来指令（「须 lockstep」） |
| E29 | `planned-build/README.md` 索引缺 integrate-toolchain + 七行失准 | 同上（重要 I5） | 缺的正是今天唯一在动的工作区；另有一行「tmux-daemon-reconcile 已交付」而该目录无 STATUS/MASTERPLAN、`PLAN.md` 从不声明完成——「已交付」只存在于索引里，不可回溯 |
| E30 | README 产品文档漂移复发 | 同上（重要 I2） | **版本号与两个测试数已修**（2026-07-29：三处 v3.2.0→v3.3.0、cargo 365→536、DOM 595→814，均为实测）。**仍未做**：README 那个约 1400 字的单段落「项目状态」把整部版本史塞进去、与 CHANGELOG 重复，且**零账号功能覆盖**；以及 §A 那条登记行自己写着「README 停在 v3.0.0」也该更新——**「修了一半、登记项没跟着更新」正是这条复发的机制** |
| ~~E31~~ | `shell_quote` 放错模块 —— **⚠ 2026-07-30 `local-as-remote` L1 复测：中心断言不成立，暂不做**。实测用它的是 **7** 个模块（不是 5，多了 `pubkey`/`tool_registry`）、`ssh_source.rs` 现 **4960** 行（不是 4847），而**没有一个模块是「只为它」依赖 `ssh_source`**：`accounts` 还要 `RemoteConfig`，`account_usage`/`cc_bus`/`remote_history`/`tmux` 还要 `connect_and_exec_cmd`（`remote_history` 另加 `connect_session`、`tmux` 另加 `stream_loop`）⇒ **搬去 `utils.rs` 一条依赖边都断不掉**，本条承诺的收益不存在。**将来成立的条件**：等出现第一个与 ssh 无关的调用者（L1 的本地路径今天走 argv、不需要引号处理）。原文如下 | Phase G 代码工程视角审阅（重要 6） | `ssh_source.rs` fan-in 14 里，5 个模块（`accounts`/`account_usage`/`cc_bus`/`remote_history`/`tmux`）**只为一个与 SSH 无关的纯字符串工具**依赖这个 4847 行模块。搬去 `utils.rs` 即可，一处改动。（顺带：该审阅逐个查过 14 个依赖方，只用到 6 个符号 ⇒ **这是内聚的 SSH facade，不是上帝对象，不该按行数拆**） |
| E32 | Rust 侧零覆盖率门禁 | 同上（重要 7） | 31570 行 / 536 测试，无覆盖率地板。TS 侧地板只量 `*.vitest.ts` 而分母含 `*.test.ts` 覆盖的文件 ⇒ 只能设到 40/34/36/41，且新增只有 `*.test.ts` 覆盖的模块会**压低**全局数字、可能误红 |

| **E42** | **★ `/usage` 解析从未真机验证过 —— 「用量读不到」很可能不是版本漂移，是它一直没对过** | 用户 2026-07-31 报「抓到了屏幕但认不出格式」 | **未修**。`src/account-usage-parse.ts` 的文件头注**自己写着**：正则「基于训练知识对 `/usage` 公开呈现形态的**回忆重建**，**没有经过任何真机验证**」，且 `unify-launch/features/F10-remaining-account-ux.md` §7 留了一份**上线前必须做的真机验证清单**——**那份清单从未执行**。⇒ 用户看到的那句「可能是 Claude Code 版本更新导致 /usage 输出变了」是**降级文案里的猜测**，不是诊断结论；更可能的解释是 `LABEL_PATTERNS`/`PERCENT_RE`/`RESET_RE` 三组模式**从一开始就对不上真实输出**。**修法唯一前置 = 拿到一份真实 `/usage` 抓屏文本**（本仓红线禁止启动真实已认证的 claude 去自测）。拿到后重写那三组模式即可，Rust 侧编排与调用方都不用动（该文件头注已声明这是唯一需要重写的文件） |
| **E43** | **账号选择器移出设置，与「设置 / 历史」并排放右上角** | 用户 2026-07-31 | **未做（UX）**。要一并想清**哪些该留在设置里、哪些该移出来**——判据建议落在「**这是常用切换动作，还是一次性配置**」上，而不是按「和账号有关」归堆 |
| **E44** | **全量梳理 UX 交互；两处「部署」是否该统一呈现** | 用户 2026-07-31 | **未做（UX）**。**实测澄清一件事：那两处部署的不是同一个东西** —— 设置「连接」里是 `deploy_remote_daemon`（部署 `cc-monitor-remote` daemon，`remote-section.ts:949`），设置「账号」里是 `deploy_remote_acct_iso`（部署 `cc-acct-iso` 这个 bash 工具，`accounts-section.ts:236`）。⇒ **后端不该合**（T04 第二步已审计过「统一部署器不该建」：7 个入口逐个列步骤序列后确认，三个范式里两个早就共享了、第四个只有一个使用者）；要讨论的是**呈现层**要不要让用户在一个地方看到「这台机器上我装了哪些东西、各自什么状态」 |
| **E45** | **设置页面大改**：「集成」改名「部署」· 该折叠的折叠 · 警告信息收成图标点开才展开 · 按「远端部署 / 本地部署」分栏 | 用户 2026-07-31 | **未做（UX）**。现状树见 `PHASE-G-REPORT.md` 之后的讨论记录 / 本条下方。**注意既有决定**：T07 实测四个耦合面后判定**不拆 `panel.ts`**（风险>收益），真病是「任一 section 构造期抛 = 整页白屏」，已用 `safeBlock` 分区块隔离治掉——**本条是信息架构改动，别顺手把那个结论推翻** |
| E33 | **远端 tab 带外杀 tmux 后「变灰」延迟长到被用户当成 bug** | 用户 2026-07-29 真机观测 + `tmux_reconcile.rs` 头注自陈 | **⚠ 延迟那半已解，诊断那半未做**（2026-07-30，`zero-poll-liveness` P0-P7）。**① 标定两个常量 ⇒ 已被更好的东西取代**：`TMUX_EMIT_INTERVAL` **整个删掉了**（判活全部事件驱动，见 `doc/INVARIANTS.md` §41）——「多个中杀一个」实测 **126ms**（对照组 5042ms）、「杀到 server 没了」**27ms**。`RETIRE_MISS_THRESHOLD >= 2` **一字未动**，它现在只在**兜底路**上生效（死亡帧绕过它）⇒ 标定的必要性大幅下降。**② 给 tab 加可见诊断（三格一眼可分）＝ 仍未做**，且三格的内容要跟着改写：①「在等那 ~16 秒」现在≈不可感知；②「daemon 没在发帧」要多列一种成因——**hook 没装上**（server 重启后重装失败 / daemon 是旧版）；③ `never_bound` 按设计永不判，不变。**这是 UI 改动，不在 `zero-poll-liveness` 范围内，留在本条。** 以下为原始记录：**未做**。用户报「一直有个 tab 不灰」，随后自行变灰 ⇒ 机制没坏，是**延迟**。**延迟可以算出来**：`TMUX_EMIT_INTERVAL`(**8s**，`remote-daemon-proto/src/watcher.rs:65`) × `RETIRE_MISS_THRESHOLD`(**2**) ≈ **16 秒**。（`tmux_reconcile.rs` 头注说这两个是「占位常量、真机未标定」——**那句只对 threshold 准**，`TMUX_EMIT_INTERVAL` 是有明确值的；用户追问「不是都没有轮询了」时才查清的。另：**轮询没消失，是搬走了**——删掉的是 monitor 侧每 8s 新建 SSH 跑 tmux ls，换成 daemon 在它自己那台机器上周期跑、经 `TmuxSessions` 帧上报；「不新增轮询」红线守的是 monitor 侧那条。`RETIRE_MISS_THRESHOLD` **不能降到 1**——`tmux_reconcile.rs:31` 有编译期断言钉死 `>= 2`，因为 `/branch` 漂移有 ~1s 竞态窗），且「带外杀端到端变灰」本身就在它的**真机累积项**里、**从没验过**。⇒ 这是那条累积项的**第一个真机观测点**。做法：① 标定两个常量；② 给 tab 加一条可见的诊断（数据来源 / `ever_bound` / `miss` 计数），让**三格**一眼可分——它们从 UI 上完全看不出区别，全表现为「不灰」：① **在等那 ~16 秒**；② **daemon 没在发 `TmuxSessions` 帧**（没跑 / 版本旧 / `raw` 是 `NO_TMUX` / backend 集为空被观测无效门保守跳过）⇒ **对账一轮都没执行，不是延迟长**；③ **`never_bound` 按设计永不判**（`never_bound` 的 sid 按设计**永不变灰**，见 `reconcile_step` 末支注释——bg / 无 wrapper / 直起 claude 免疫误 retire）。**注意本地会话完全不经这条路**：`reconcile_step` 全仓只在 `ssh_source.rs:2383` 的远端收帧循环里被调，独立 poller 已删（`lib.rs:705`） |

| ~~**E34**~~ | ~~**★ 把 tmux 存活的「轮询」换成事件：带外杀 tmux 后近乎零延迟变灰**~~ **✅ 已解（2026-07-30，独立工作区 `zero-poll-liveness` P0-P7 全部交付）** | 用户 2026-07-29 明确要求「我要把轮询杀掉」 | **daemon 里 A/B 两条轮询都已删除，生产段零定时器**（`no_timer_guard.rs` 钉住）。四路事件 + 实测延迟 + 三盲区分类见 **`doc/INVARIANTS.md` §41**。**★ 对本条原措辞的三处订正**（不是执行走样，是登记时就写错了）：① 原文承诺「**daemon 零改**」——**不成立**，用户 2026-07-30 当日松了该红线（原话「daemon 是能改的，我的要求就是性能最佳且不要轮询」）；零改做不到正向死亡帧，16s 只能降到「新间隔×2」，因为 `RETIRE_MISS_THRESHOLD >= 2` 是编译期断言、判据本身就是轮询式的。② 原文只盯了 tmux 那条 8s 轮询，**范围写小了**——daemon 里是 **A/B 两条**（还有 2s 判活 tick）。③ 原文「把轮询换成事件」的**字面版会留 2 个盲区**：「会话活着但卡死」与「整台机器挂掉」。如实分类后：前者**今天的轮询也没在做**（卡死的 CC 在 `tmux ls` 里照样在）⇒ 删轮询在这格零损失；后者机器内部无解，靠 monitor 断连自愈。**另**：原文列的「需改 `shared/ccm` 本体」也不成立——hook 由 **daemon** 装（只有它有「server 重启」这个时机），ccm 一字未改。下方旧小节保留作历史，顶部订正块已列出别照着做的四条 |
| **E39** | **`notify-debouncer-mini` 静默吞掉 inotify 队列溢出 ⇒ 溢出时事件永久丢失** | `zero-poll-liveness` P0-⑤ 读源码实测（2026-07-30） | **既有盲区，非新引入**。`notify 6.1.1/src/inotify.rs:208` 把 `Q_OVERFLOW` 报成 `EventKind::Other + Flag::Rescan`，但 `notify-debouncer-mini 0.4.1/src/lib.rs:319` 的 `add_event` **只读 `event.paths`**，而溢出事件 `paths` 为空 ⇒ 循环体一次不执行 ⇒ 完全不可见。**两端都中招**（daemon 与 `src-tauri/src/watcher.rs` 用同一套 debouncer）。今天没有周期兜底：目录发现（`SessionAdded`）与 jsonl 行流只靠 notify 事件。**pidfd 对此免疫** ⇒ `zero-poll-liveness` P2 会把判活这一路摘出 inotify 依赖。处置选项：① 换 raw `notify` + 自写去抖（要保 `DEBOUNCE_MS=100` 双写点语义）② 加一个只为拿 `need_rescan` 的 raw-notify 哨兵实例 ③ 接受并登记。**绝不补定时器**（那等于零轮询造假）。**结论来自读源码，未真触发过溢出** |
| ~~**E41**~~ | ~~6 套非 CI 的 e2e 套件不做 tmux socket 隔离 ⇒ 在开发机上会动开发者自己的 tmux server~~ **✅ 已解（2026-07-30，`gate-integrity` G-C）**：6 套统一加前导 `unset TMUX` + 短 `TMUX_TMPDIR`（零调用点改动，84 处裸 `tmux` 一个没改）。**★ 归因订正**：本条原文把病因记成「一处 `-L` 都没有」——**那只是表面特征**。实测的实质是**从 tmux 会话里跑时继承了 `$TMUX`**，客户端会连外层那台 server 并**完全忽略 `TMUX_TMPDIR`**（我第一次就是只设 `TMUX_TMPDIR` 没 `unset TMUX`，会话照样落在默认 socket 上）。另：socket 路径有 108 字节上限，`TMUX_TMPDIR` 必须短路径。5 套已进 CI 并自带地板（24/17/5/5/7）；`graylight-suite` 拿到隔离但不进 CI（全链级，要跑起 GUI app，与 ci.yml 既有论证同源）。详见 `gate-integrity/features/G-C-e2e-suites-into-ci.md` | `zero-poll-liveness` P1 工程审计实测（2026-07-30） | ~~**未修，已改 P6 计划**~~。`graylight-suite` · `graylight-daemon-frames` · `restart-suite` · `restart-daemon-frames` · `resume-suite` · `resume-daemon-frames` + helper `gen-idle-tmux.sh` —— **一处 `-L` 都没有**，直接在**默认 socket** 上 `new-session` / `kill-session`（如 `graylight-daemon-frames.sh:56`）。在 CI 的干净容器里无害，但这 6 套正是 **E14** 记的「不在 CI」那 6 套 ⇒ 实际只会被人在开发机上手跑。对比：CI 那 8 套安全（6 套带 `-L`：`ccm-acceptance`/`ccm-pretrust`/`cc-spawn-uplift`/`tmux-guarded`/`tmux-target`/`usage-probe`；另 2 套 `ccm-cli.test.sh`/`ccm-print-parity.sh` **根本不调 tmux**——grep 命中的 "tmux" 全在注释与 `--print` 断言里，`ccm-cli.test.sh:4` 明写「不真起 agent、不碰 tmux」）。⇒ `zero-poll-liveness` **P6 的延迟 e2e 原计划挂在 graylight 一族，实测行不通**：要么先给这 6 套加 `-L` 隔离（顺带闭合本项 + 让 E14 进 CI 变得安全），要么改用已隔离的载体。**旁证**：`graylight-daemon-frames.sh:30` 留了个 keepalive 会话，注释自陈是为了绕开 §24bis 空 backend 守卫——而那正是 P1 修掉的 bug ⇒ 该 bug 当年是被**测试侧绕过**而不是被发现 |
| **E40** | **cgroup 隔离结论只对「daemon 经 SSH 起」成立 ⇒ `local-as-remote` L1 必须重判** | 同上，P0-③ | **提醒项，不是 bug**。实测：tmux server 在 `session-12881.scope`、每个新 SSH 登录得新 `session-<N>.scope` ⇒ 必然不同锅 ⇒ daemon 自持的 pidfd 探针扛得住「tmux 整锅 SIGKILL」。**但 L1（本地=不走 ssh）里 daemon 可能与 tmux 同锅**，届时探针会被一起端。L1 落地时必须重测这一格，**不许继承 `zero-poll-liveness` 的结论** |

---

## E34 详述：事件驱动的 tmux 存活信号（用户点名要做，需先调研）

> ### ⚠ 2026-07-30 订正：本小节以下内容有四条已被实测推翻，**以 `zero-poll-liveness` 工作区为准**
>
> 本项已升格为独立工作区 `.claude/planned-build/zero-poll-liveness/`（主计划用户已批、P0 已交付签收）。
> 以下旧文保留作历史记录，但**这四条别照着做**：
>
> 1. **「daemon 零改」不成立、且用户已松该红线**（2026-07-30 原话「daemon 是能改的，
>    我的要求就是性能最佳且不要轮询」）。零改做不到正向死亡帧 ⇒ 16s 只能降到「新间隔×2」，
>    因为 `RETIRE_MISS_THRESHOLD >= 2` 是编译期断言、判据本身是轮询式的
> 2. **范围写小了**：daemon 里是 **A/B 两条**轮询（2s 判活 tick + 8s `tmux ls`），本小节只盯了后者
> 3. **「需改 `shared/ccm` 本体」不成立**：hook 由 **daemon** 装——只有 daemon 有
>    「server 重启」这个时机（socket 目录 inotify），ccm 只在建会话时被调一次
> 4. **「把 sid 字面量烤进 per-session hook，最干净」这条路不存在**：P0 实测
>    `session-closed` **专门不支持 per-session**（对照实验：`session-renamed -t A` 会触发）。
>    而且 `#{@ccm_sid}` 在 `session-closed` 里**解析到别的会话** —— 照直觉写会把
>    还活着的会话变灰。必须用全局 `[50]` + `#{hook_session_name}` + monitor 侧反查
>
> **另**：「§24 单写者」不再是开放问题——所有新信号都汇进既有
> `SessionChange{removed}` → emitter，零新写点。

**用户原话**：「为什么延迟这么高? 不应该一关就杀吗? 不是事件驱动吗 / 怎么做成零延迟」→「我要把轮询杀掉」

### 现状（2026-07-29 实测，不是推测）

两条信号路性质完全不同：

| 信号 | 机制 | 延迟 |
|---|---|---|
| claude 进程退出 | pidfile 消失 → daemon 的 `notify` inotify（`DEBOUNCE_MS = 100`，`remote-daemon-proto/src/watcher.rs:61`） | **~0.1 秒，本来就是事件驱动** |
| tmux 会话被带外杀 | **没有任何东西推**，只能 daemon 周期跑 `tmux ls`（`TMUX_EMIT_INTERVAL = 8s`，`watcher.rs:65`）× `RETIRE_MISS_THRESHOLD`(2) | **~16 秒** |

daemon 用 `notify` 只 watch 两处：`projects`（递归）+ `sessions`（非递归）。
**「一关就杀」在正常情况下确实是瞬时的**——那 16 秒只出现在
`tmux_reconcile.rs` 头注点名的那个场景：**claude 被守护托管而不随 tmux 死**
（`ccm --detach` 那类 disown 的会话）⇒ pidfile 还在 ⇒ 事件路无信号 ⇒ 只剩轮询能发现。

**一处订正**：轮询**没有被消灭，是搬走了**——删掉的是 monitor 侧每 8s 新建 SSH 跑 `tmux ls`
（`lib.rs:705`：`run_tmux_reconcile_poller` 已删），换成 daemon 在它自己那台机器上周期跑。
「不新增轮询」那条红线守的是 monitor 侧那条。

### 可行性：tmux 其实有事件，只是本仓没用

- 本机 **tmux 3.6**；`session-closed` hook 自 2.4 起就有
- 而 **`shared/ccm` 与 `shared/cc-bus/scripts/*` 现在一个 tmux hook 都没设**（grep 零命中）

⇒ 把「轮询获得的信号」换成「tmux 推的事件」是可行的：
`session-closed` hook 在会话关闭那一刻触发 → 碰一下 daemon **已经在 inotify** 的目录
→ 复用整条既有事件链 → **daemon 零改**（红线保住），延迟 16s → **~100ms**。

**附带收益**：`RETIRE_MISS_THRESHOLD >= 2` 那条编译期断言的存在理由
（防轮询抖动 + `/branch` 漂移竞态误判）**在事件路径上不成立**——
hook 明确说「**这个具体会话**关了」，没有抖动可言。所以事件路可以不要 debounce。

### 三个必须先调研/实测的问题（**不许凭推测动手**）

1. **`session-closed` 支不支持 per-session 作用域？** tmux 文档在这点上有歧义
   （会话对象正在销毁）。若支持，`ccm` 建会话时可以把 sid **字面量烤进** per-session hook，
   最干净；若只支持全局，就得用 `#{hook_session_name}` + 一张名字→sid 映射。**必须实测。**
2. **谁写、写什么 —— 会不会破 §24「单写者」？** 最直接的是 hook 删掉该会话的 pidfile 让既有
   `SessionRemoved` 路照跑，但那让 liveness 目录**多一个写者**。要么换成
   「写一个独立的『tmux 已关』标记，由既有唯一写者读」，要么**显式论证**这个第二写者可接受
   （断连 flush 已经是第二生产者的先例）。
3. **hook 里跑什么才安全？** `run-shell` 在 tmux server 上下文里跑，
   要确认不会因为 hook 失败卡住 server、以及远端机器上路径/权限如何取。

### 两条需要用户表态的红线

- **必须改 `shared/ccm` 本体**（加 hook 只能在建会话那一步做）——而「不改 `shared/ccm` 本体」目前在册
- **§24 单写者**倾向哪种解法（见上方问题 2）

### 附带的诊断项（E33，与本条同源）

三种「不灰」从 UI 上完全看不出区别：① 在等那 ~16 秒 ② daemon 没在发 `TmuxSessions` 帧
（没跑 / 版本旧 / `raw == NO_TMUX` / backend 集空被观测无效门保守跳过 ⇒ **一轮都没执行**）
③ `never_bound` 按设计永不判。**做 E34 时顺手把这三格显示出来**，否则下次还是只能靠猜。

| **E35** | **★ 真 bug：历史会话「留空恢复默认」清不掉自定义标题** | C04d 批 6c 读边界时发现，**已实测确认机理** | **未修**（修它是行为改动，不在 C04d 范围）。见下方独立小节 |
| **E36** | **多账号加第三方 API key** | 用户 2026-07-30 问 | **未做（零代码）。用户已选路线乙**（`apiKeyHelper` 写进每账号自己的 `settings.json`）⇒ **前置 = `account-zero` Z08**（`isolate` 迁移能力）。见下方独立小节顶部的订正块 |
| ~~**E37**~~ | ~~`CLAUDE_CONFIG_DIR` 零官方文档，而整套多账号隔离压在它上面~~ | 2026-07-30 查官方文档均无此项 | **✅ 已销（2026-07-30，`account-zero` Z07）**。从「失效时无人知晓」变成「有版本钉 + 四条检测」：**D1b 致命**（secret 泄漏进共享库；零误报。⚠ 当时写的理由「会被自动 symlink 给每个账号 = 静默串号」**是错的**——隔离项从不被 symlink 出去；**Z01 已订正并改判**：`root=cfg` 的 `.credentials.json` = 账号 0 已登录（正常），`root=home` 的 `.claude.json` 仍致命）· D2 提示（共享库出现声明外的 mode 600 文件）· D3 提示（版本与钉的不同 ⇒ 要求复核声明）· D4 提示（声明项缺席——**刻意只提示**：`policy-limits.json` 这类是懒创建的，「还没创建」与「改了位置」不可判定）。版本探测**只读、绝不执行 claude**。**边界如实**：换位置这类能报能迁；**换机制（keyring）按目录切身份整体失效、救不了**，只承诺当场可见 |

---

## E35 详述：「留空恢复默认」清不掉自定义标题（真 bug，机理已确认）

**发现方式**：C04d 批 6c 要给 `update_history_metadata` 写包装层签名，被迫精确写下 `patch`
的类型，于是去读了 Rust 侧的 `MetadataPatch`。

**机理**（读了 struct + `update_history_metadata` 函数体，不是只看注释）：

```rust
pub struct MetadataPatch {
    #[serde(default, rename = "customTitle", alias = "custom_title")]
    pub custom_title: Option<Option<String>>,   // ← 双层 Option
}
// update_history_metadata:
if let Some(t) = patch.custom_title { entry.custom_title = t.filter(|s| !s.trim().is_empty()); }
```

`#[serde(default)]`（**非** double_option）下：键缺失 → 外层 `None`；键存在但值是 JSON `null`
→ **也是外层 `None`**（`Option<T>` from null 恒为 None）。⇒ `if let Some(t)` **不触发**
⇒ **`null` 的语义是「不改」**。Rust struct 注释也明写：「清空走空/空白串 → `Some(Some(""))`
→ update 里 filter 掉，**不靠 null**」。

**而前端传的正是 null**（`src/views/history.ts`，「自定义标题（留空恢复默认）」那个 prompt）：

```ts
patch: { customTitle: next.trim() === "" ? null : next.trim() }
```

⇒ 用户按提示「留空」提交 → 前端发 `null` → 后端**什么都不做** → **标题清不掉**。
UI 文案承诺的「留空恢复默认」不成立。

**修法（一行）**：前端把 `null` 改成 `""`（走 `Some(Some(""))` → filter → None = 清空）。
**为什么本轮不修**：C04d 每个 commit 的硬判据是**行为逐字节不变**，这是行为改动。
需要一条复现测试 + 一个独立 commit。

**包装层已经把这个陷阱写在签名旁边**（`src/ipc/commands.ts` 的 `update_history_metadata`
条目），并刻意**不为 `MetadataPatch` 生成类型**——生成 `customTitle?: string | null`
会让人以为 `null` 是清空，那是**说谎的类型**。

---

## E36 详述：多账号加第三方 API key

> ### ⚠ 2026-07-30 订正：用户选了**乙**，本小节以下正文写的是**甲**
>
> 以下正文论证的是**甲：加两个窄 `EnvOp` 变体**（`ANTHROPIC_API_KEY`/`ANTHROPIC_AUTH_TOKEN`
> 走环境变量注入）——那条路确实**零红线、零迁移、零 `ISOLATE_SET` 改动**，论证仍然有效。
>
> **但用户 2026-07-30 明确选的是乙**：`apiKeyHelper` 写进**每账号自己的** `settings.json`
> （理由：那才是官方推荐的形态，且 `settings.json` 里还有一堆本就该按账号分的键——
> `hooks`（用户的 cc-bus 加入点）/ `model`（不同账号可能不同 plan）/ `permissions.defaultMode`
> / `effortLevel` / `env` / `theme`）。用户原话：「我要用乙」+「cc-bus 本来就应该这样。
> 不装的账号不应该被自动启动消耗额度」——**后半句把 per-account 加入变成了功能需求，不是代价**。
>
> **乙 的代价与甲完全不同**：
>
> | | 甲（EnvOp） | **乙（已选）** |
> |---|---|---|
> | 改 `ISOLATE_SET` | 不用 | **要**（`settings.json` 今天是共享的：实测 z/b 都软链到 `~/.claude/settings.json`） |
> | 迁移已有 z/b | 不用 | **要**，而且 **cc-acct-iso 根本没有这个能力** |
>
> ⇒ **乙 的前置 = `account-zero` **Z08** —— 2026-07-30 已交付签收**（`isolate` 能力 + `sync` 改成私有化）。
> 那个能力**同时**是「Claude Code 换登录位置后迁移」（Z06/Z07）的前置——一个能力两个需求都要。
>
> **授权状态**：用户已授权动 `~/.claude/skills/cc-acct-iso/` **且**授权改 `z`/`b` 真实账号目录
> （走「备份 → 改 → verify 复核 → 不符回滚」），共享库那份 `settings.json` 留作新账号模板。
> 但随后说「先不要改，我现在在用 claude code」⇒ **等用户发话再动**。
> 已论证：**隔离那一步本身对在用的会话不可观测**（同目录 `mktemp` + `mv` 原子替换、内容逐字节
> 相同、全机无进程长开 `settings.json` 的 fd）；**会出事的是之后往里写 key 那一步**。

### 以下是甲路线的原始论证（保留：若将来改主意选甲，这些结论直接可用）


**用户问**：「现在的多账号可以有的是第三方 apikey 吗?」→ **今天不支持**（下文①②③），
但**怎么做**这件事，我第一版的结论是错的，已订正。

### ★ 对我自己初判的订正（重要）

我最初写「要动三层，第一层要改 `~/.claude/skills/cc-acct-iso/` 的 `ISOLATE_SET`（红线），
且会变更现有 z/b 两账号的共享结构、需迁移」。**这条是错的**——我漏看了本仓自己已经建好的机制：

**cc-monitor 就是构造启动命令的那一方**，`src/launch-plan.ts` 的 IR 里已经有 `EnvOp`：

```ts
export type EnvOp =
  | { kind: "export-config-dir"; value: string }
  | { kind: "export-model"; value: string }   // F07：每账号默认模型（ANTHROPIC_MODEL）
  | { kind: "unset-config-dir" }
  | { kind: "unset-nested-env" };
```

`export-model` **已经在做「每账号一个 `ANTHROPIC_MODEL`」**。加 `ANTHROPIC_BASE_URL` /
`ANTHROPIC_AUTH_TOKEN` 就是在同一处再加两个**窄变体**，与 `settings.json` 完全无关
⇒ **零红线、零迁移、零 ISOLATE_SET 改动。**

**官方文档给的决定性依据**（`code.claude.com/docs/en/env-vars`）：
> `ANTHROPIC_API_KEY`: "…**When set, this key is used instead of your Claude Pro, Max, Team,
> or Enterprise subscription even if you are logged in.**"

启动时的 env 就够，且**优先于订阅登录** ⇒ per-account 天然按启动区分。

### 今天为什么不支持（三条实测）

1. **全仓零处理**：`ANTHROPIC_API_KEY`/`ANTHROPIC_BASE_URL`/`apiKeyHelper` 在
   `src/` + `src-tauri/src/` + `shared/` **grep 零命中**；`RemoteAccount` 没有一格能装它们。
2. **`settings.json` 是共享的**：真机验证 `~/.claude-accts/z/settings.json` 与 `b/settings.json`
   **都指向** `~/.claude/settings.json` ⇒ 走 `settings.json` 的 `env`/`apiKeyHelper`
   会被**所有账号共用一个 key**。（用户的 `apiswitch/settings1.json` 参考就是这个形态
   ——**单账号正确，多账号不适用**。）另：认证优先级里 `apiKeyHelper` **排在 OAuth 之上**
   ⇒ 它一存在，已登录的订阅账号也会被顶掉走网关。
3. **`logged_in` 的判据不认它**：它是 stat `.credentials.json` **存在性**得来的
   （代码注释自承「不代表凭据有效」）⇒ 纯 key 账号不产生该文件、UI 显示「未登录」。

### 建议形态（等用户拍）

- `EnvOp` 加两个**窄变体**（不开自由 `{key,value}` 后门——本仓刻意拒绝过，
  注释写着「防止绕开校验往命令里塞任意变量名」）：
  `{ kind: "export-base-url"; value }` · `{ kind: "export-auth-token"; value }`
- `RemoteAccount` 加 `auth_kind`（`subscription` | `api-key`），`logged_in` 按 kind 分别判
- **key 存哪里要用户定**：`accounts.json` 明文 / 系统 keyring / 沿用 helper 脚本

### 调研中发现的两处「官方 vs 本仓」冲突（独立风险，值得单独排期）

| 项 | 官方文档 | 本仓 / 现实 |
|---|---|---|
| 用户级 `settings.local.json` | **明确写不存在**（`.local` 只在项目级） | `config_surface.rs` 把 `<config_dir>/settings.local.json` 当用户级作用域建模，还有测试断言那儿的 hook 算已装 |
| `CLAUDE_CONFIG_DIR` | settings 页与 env-vars 页**都没提** | **整套多账号隔离全靠它**；`mcp.rs::claude_json_candidates()` 也在读它。它确实有效，但**无文档保证** |

第一条 ⇒ 我一度想用 `settings.local.json` 做 per-account，**那个前提被官方否认，已放弃**。
第二条是**稳定性风险**：隔离依赖一个未文档化的环境变量，Claude Code 若改其行为，
多账号会整体失效且我们没有文档依据。**建议单独登记为 E37 并做一次真机验证 + 版本钉**。

| **E38** | **panorama 一族 10 个类型的生成被 SS-10 铁律 + code-picture 仓红线双重阻塞** | C04d 批 7 实测 | **结构性阻塞，如实登记**。见下方小节 |

---

## E38 详述：vendored 类型的生成为什么做不了（不是忘了）

C04d 批 7 迁 `src/panorama/api.ts` 的 21 个调用点时发现：它们的返回类型有 **10 个**
（`Overview` · `NodeView` · `SubGraph` · `Edge` · `ImpactSet` · `Symbol` · `DocLink` ·
`Annotation` · `DriftItem` · `IndexStats`）住在
**`src-tauri/vendor/code-picture-core/src/model.rs`** —— vendored 代码。

**两重阻塞，任一条单独成立就做不了**：

1. **`VENDOR.md` 的 SS-10 铁律**（原文）：
   > **副本是上游的镜子，不是分身**（SS-10 铁律）：**只照上游改，绝不在副本里改出自己的版本**。
   > 要加字段/改行为先改上游再 re-vendor。

   给它们加 `#[cfg_attr(test, derive(ts_rs::TS))]` 就是「在副本里改出自己的版本」。
2. **「先改上游」要动 `code-picture` 仓**（`/home/zbl/文档/project/self项目/code-picture/code-picture`）
   —— 本会话在册的红线，需另外授权。

**批 7 的处置**：按主计划 §5「**名字钉死是普遍的，类型生成是按需的**」——
本批只做**名字钉死 + 实参把关**（21 条包装层条目），返回类型指向 `src/panorama/types.ts`
的现有手写类型。`PanoramaStatus` 例外（它在 `panorama.rs`、是本仓自己的）⇒ 已生成。

**代价是具体的**：这 10 个手写镜像**仍无守卫**，是全仓剩下最大的手写跨边界面。
它们会不会漂？**会**——上游 F68 就给 `Symbol` 加过 `signature: Option<String>` 字段
（`VENDOR.md` 沿革里记着），那种改动如果 TS 侧没跟上就是静默不一致。

**要做的话有三条路，都需要用户决定**：
- **甲**：授权动 `code-picture` 上游 → 加派生 → re-vendor。**最正**，但跨仓。
- **乙**：在 cc-monitor 侧写一个**薄适配层**（`panorama.rs` 里定义自己的 DTO + `From<model::X>`），
  给 DTO 加派生。**不碰 vendor**，代价是 10 个转换函数 + 一层拷贝。
- **丙**：维持现状，改为**给这 10 个手写类型加一条结构性守卫**
  （对拍 `vendor/.../model.rs` 的字段名集合 vs `src/panorama/types.ts`）。
  **不生成、但能抓漂移**；成本最低，且不违反任何铁律。**我倾向丙**——
  它把「防漂」这个真实目的达成了，而生成物本身在这里只是手段。
| **E47** | **「/branch 后进终端，灰点会莫名变绿」尚未解释** | 用户 2026-07-30 与灰点 bug 一并报的第二个现象 | **未查**。S0 修掉了灰点本身（`cause=superseded` ⇒ 直接归档，不再有那个灰点），所以这个现象**可能已随之消失**——但成因从未查清，不当作已修。猜测方向：attach 会触发 tmux hook（ccm 装了 `set-titles`，可能命中 `session-renamed`）⇒ 来一帧新 `TmuxSessions` ⇒ 状态被重算。**复现前不要写修法** |
| **E48** | **UI 不阻止两台机器取同一个 origin（`label \|\| host`），而 origin 是全系统的机器身份** | S1 自审时发现 | **未修**（S1 只保证「无效配置不会静默吞编辑/静默删不掉」，没有让它变合法）。origin 是 `announced_registry` / `idle_registry` / `tmux_raw_registry` / 会话来源标注共用的 key，重了会在这些地方互相覆盖 —— 持久化层只是最后一环。**该在 UI 上拦**（保存时校验重名并提示），归 S4「机器详情页」时一起做 |
| **E49** | **`cc` 的会话命名逻辑写在 shell 里，与前端 `deriveTmuxName` 构成跨语言双写点** | 用户 2026-07-31 提议「cc 不该自己在 bashrc 搞，应该去调用 daemon 生成会话」 | **未做，是个真提议**。今天 `shared/ccm::derive_tmux_name`（shell）与 `src/remote-launch.ts::deriveTmuxName`（TS）各写一份，靠 `e2e/ccm-cli.test.sh` 的真值对拍钉住。由一处统一开会话能消掉这个双写点，让终端与 monitor 真正走同一条路（今天是「规则相同、各写一份」）。**★ 更正（同日，用户当场指出）**：我最初记成「正面撞铁律 I7（daemon 只读）」，**那是错的**。`readonly_guard` 守的是 daemon **自己的源码里不许有文件系统写调用**（`fs::write`/`create_dir`），针对被观测文件系统（`~/.claude` 等）；`tmux new-session` 是起进程、不是 fs 写，护栏拦不到。真正的问题只是**语义**：daemon 今天是观察者，让它变执行者是可讨论的取舍，不是机器化的墙。**待定的真问题**：① 谁当那个「统一开会话」的角色（daemon？还是 monitor 侧已有的 SSH exec 通道？）；② daemon 没装的机器（daemonless）上 `cc` 必须照样能用 |
| **E50** | **「daemon 只读」这条被我反复引用得过宽，文档该把边界写清** | 用户 2026-07-31 反问「daemon 从来没说过只读，不然 code-picture 怎么集成进去？都要在目录生成 code-picture 文件夹了」 | **未做**。事实边界：`readonly_guard` = **daemon 进程不写被观测文件系统**。而 **cc-monitor 本来就在远端写**（装 ccm 进 `~/.bashrc`、部署 daemon 二进制、装 cc-acct-iso、写 `.mcp.json`），走的是 monitor 侧 SSH exec / SFTP，**不经过 daemon**。⇒ code-picture 要在远端生成文件夹与 I7 **不冲突**（daemon 源码里也确实没有 panorama/code-picture，grep 为空）。`doc/INVARIANTS.md` 该把「谁不许写什么」写成一句不会被误引的话 <br><br>**✅ 2026-07-31 已解（branch-anywhere G2）**：护栏按「收窄不删」改成两层 —— 默认层照旧禁那 11 条写模式；另开**恰好一个**白名单模块 `fork_write.rs`，对它加**更严**断言（必须 `.create_new(true)`；不得删除/改名/复制/硬链软链/截断/追加/覆盖写/建目录/`set_len`/`.create(true)`）。判据从「daemon 不许碰文件系统」改述为**「daemon 不许改动用户既有数据」** —— 前者只是后者在「daemon 从不写盘」年代的近似。边界写进 `doc/INVARIANTS.md` §41.6。|
| **E51** | **BACKLOG 编号靠人手递增，已经撞过一次号** | 2026-07-31 自身事故 | **未做**。本轮追加时按「上次见到的最大号 +1」拍了三个号，而文件中部**早有**同号条目（早先的 UX 条目）⇒ 重号。随后用「首次出现的整行模式」定位改写，命中的是**靠前那个**，一次性删掉了其后 262 行（已从上一次 commit 恢复，内容零丢失）。**三条教训**：① 加条目前先扫全表取真实最大号，别凭记忆；② 改长文档一律按**行号/唯一锚点**定位，绝不用会重复的模式串做切片；③ 写这条记录时又踩了一次同族的坑 —— 叙述里逐字引用那个表格行模式，把去重自检打红了（同 `no_timer_guard` 记过的「散文里写了守卫的禁用模式」）。**改措辞，别改检查。** |
| **E52** | **点击某条消息 → 从那里 fork 出一条新对话**（用户 2026-07-31 需求；**仓里从未登记过**——用户以为开过 issue，实查 33 个 OPEN 里只有 `#63`「fork 被显示成独立 tab / 尾消息漏显示」那个**显示** bug，`#12` fork 树历史早已 closed） | 用户 2026-07-31 | **未做，且第一步是查证不是设计**。已定：起点=**指定某条消息**（不是会话末尾）；入口=**渲染界面每条消息旁一个小按钮**。<br><br>**必须先查清 CC 到底怎么支持**（三条候选，别猜）：① `claude --resume <sid> --fork-session` —— 已知存在（daemon 后台任务在用，会写 `kind:"bg"` pidfile），但**没看到指定分叉点的参数**，很可能只能从 tip 分叉；② CC TUI 里的 **Esc-Esc rewind** —— 本仓 `branching.ts` 已经在建模它产出的分支（`#22/#25/#36` 都是那条路径的 bug），说明「从更早的消息分叉」在 CC 里是**存在**的，但那是 TUI 交互、不是 CLI；③ 其它未知入口。<br><br>⇒ **先做 spike**：确认「从指定 uuid 分叉」有没有非 TUI 的路。查不到就退回「从末尾 fork」并把这个限制如实告诉用户，**绝不用驱动 TUI 按键（送 Esc Esc + 猜光标位置）来假装做到了** —— 那是不可验证的脆弱路径。<br><br>读侧已就绪：历史页早就按 `forkedFromSessionId` 画 fork 树（深度缩进 + 展开状态持久化），新分支建出来立刻能在同一棵树上看见 |
| **E53** | **`cc` 改为调 daemon 开会话**（用户 2026-07-31 拍板，取代 E49 里「先只消双写点」那个保守选项） | 用户 2026-07-31 | **未做，方向已定**。目标：终端 `cc` 与 cc-monitor 走**同一条**开会话的路，一并消掉 `derive_tmux_name`(shell) ↔ `deriveTmuxName`(TS) 这个跨语言双写点。<br><br>**不是护栏问题**（这一条我先前说错过，见 E49 的更正）：`readonly_guard` 只拦 daemon 源码里的**文件系统写**，`tmux new-session` 是起进程、拦不到。真正要处理的是**语义转向**：daemon 从观察者变成执行者。<br><br>**开工前必须先答的两个**：① **daemon 没装的机器**（daemonless 模式）上 `cc` 必须照样能用 ⇒ 需要一条降级路径，而降级路径若还是「shell 自己算名字」，双写点就没真消掉；② daemon 变执行者之后，`no_timer_guard` / `readonly_guard` 这两道护栏的**边界要重新写清**（见 E50），否则下一个人还会像我一样误引。<br><br>与 `#77`（cc-bus 深度集成：通信/开 agent/CC cd 走 daemon）**是同一个方向**，应合并考虑 |
| **E54** | **「孤儿会话」这个词在仓里指两件不同的事，混用过一次** | 用户 2026-07-31 追问「为什么会有孤儿会话？我们不是本来就能识别 claude code 会话吗」 | **已澄清，文档待收口**。① 字面「多出一个你没要的会话」—— 旧 e2e 断言「只有一个 cc-proj，无 cc-proj-2 孤儿」用的是这个意思，是 **UX 抱怨不是可见性 bug**（识别 Claude Code 会话靠 pidfile，与 tmux 名无关，那个会话有 tab）。② **issue #76 的真孤儿**：tmux 会话**没有 `@ccm_sid`** ⇒ cc-monitor attach/管理不了；#76 里堆的 `cc-<8hex>` / `<project>_cc` 属于这种，**且不是 ccm 起的**（来源待排查）。<br><br>**我在解释 ccm 改动时混用了这两个意思**，用户当场追问才拆开。已加 e2e 场景 5ter 钉住「多开出来的第二个会话也带自己的 `@ccm_sid`」（关掉通道B打标即红）⇒ 新行为产的是 ①，不是 ②。<br><br>待做：`doc/INVARIANTS.md` / #76 里把这两个词分开命名（如「多余会话」vs「失管会话」），别再让人混 |
| **E55** | **`/home/<user>` 这个家目录假定**（远端 daemon 路径 / cc-acct-iso 部署路径的默认建议） | 2026-07-31 按用户「不要对这台机器特化」的要求全仓扫描时发现 | **未改，且可能不该改**。`machine-card.ts::defaultDaemonPathFor` 与 `acct-deploy.ts` 都按 `/home/<user>` 推默认路径（`root` 已特判成 `/root`）。**它们只是「默认建议」，用户可改**，且 daemon 是 **Linux-only**（`remote-daemon-proto` 刻意不在 workspace 里，就是为了别让 Linux-only 的 daemon 拖累 Windows CI）⇒ 对 Linux 远端这个默认是对的。macOS 远端（`/Users/<user>`）需要手改一次。<br><br>**真要治的做法不是改死另一个字面量**，而是**连上之后问远端要 `$HOME`**（`ssh … echo $HOME`），拿真值填。那需要一次额外往返，且得想清「还没连上时显示什么」。归 S4 之后的 onboarding 一并考虑 |
| **E56** | **「新用户能直接上手 + 依赖一站式备齐」这条横切要求还没有落点** | 用户 2026-07-31 | **待设计**，S5 起落实。已做的一小步：把三处 UI 占位符从写死 `pi` / `/home/pi/...` 改成举多例或 `<你的用户名>`（只写一个例子会让人以为非那么填不可），并清掉一处注释里作者本机的真实路径。<br><br>**扫描结论如实记：生产代码里没有功能性的本机硬编码**——命中的绝大多数在 `#[cfg(test)]` 夹具里（那是对的，夹具本就该有具体值）。所以这条要求的实质**不在「去硬编码」，而在 onboarding**：新用户装上之后，怎么一眼看清「还缺什么、点哪里补齐」。<br><br>与「改动足迹」页是一体两面：那页答「我在你机器上装过/写过什么」，这条答「还差什么」。S5 一起设计 |
| **E57** | **「改动足迹」页只有配置面审计表，缺「按机器分组 + 谁装的 / 何时装的 / 能不能撤」** | S5 开工复核 | **未做，且不是纯前端能补的**。主计划 §2.3 给这页的定位是「我在你机器上碰过哪些文件 / 做了什么 / 现状 / 能不能撤」。今天只有 T02 的 `ConfigSurfaceSection`（一张「你动过我哪些文件」的表）。缺的三样都要后端出数据：**谁装的**（是 cc-monitor 装的还是用户手装的）、**何时装的**（时间戳，现在没记）、**撤销入口**（每一项对应的卸载动作，今天只有 daemon/ccm 两个有）。<br><br>与 E56 是一体两面（那块答「还差什么」，这页答「装过什么、能不能撤」），但**不是同一件事**，别混做 |
| **E58** | **余下三处静态 ⚠ 逐条搬进 ⓘ** | S7 分类后剩下的纯文案搬运 | **未做（低风险纯文案）**。已按 S7 立的判据分好类，都属「静态说明 → ⓘ」：`machine-card.ts:424`（daemonless 能力子集）· `remote-section.ts:923`（↗ 拉前限制）· `usage-view.ts:208`（用量系数说明）。<br><br>**明确不搬**的两条（按判据本就该常驻，不是遗漏）：指纹未验证（**安全**警告，忽略它可能中间人）、profile 已有同名 function（**状态性**，取决于扫描结果）|
| **E59** | **机器详情页上还留着 origin 选择器 —— 「进去 = 选定一个实例」这条地基判据没兑现** | Phase G 整体设计审计（2026-07-31） | **未修，要先定「藏还是删」**。主计划 §3 账本第 4 行的最终形态是「4 处 origin 选择器**由页上下文取代**」；S4a/S4b 只做到「四份共用一个 store」，**选择器一个都没删/没藏**。`accounts-section.ts:115` 恰在 `hosts.length > 1`（本计划的目标场景）时显示下拉；`mcp-section` 有一整行机器按钮。<br><br>**后果不止难看**：写动作按 store 定目标（`mcp-section.ts:699` `const startOrigin = this.origin`、`accounts-section.ts:182` `launch_remote_terminal({origin: this.origin})`）⇒ **在标着 A 的页面上把东西写进 B**，而 `router.activeId` 仍是 A、界面上看不出来。<br><br>做法建议：给四个分节加 `setPageMode(inPage)`（照 `MachineCard` 已有的那个契约），页内隐藏自己的选择器 —— 页头就是选择器。**横跨 4 个文件，且「隐藏 vs 删除」是个真决定**，故不在 Phase G 顺手做 |
| **E60** | **`makeInfoIcon` 仍无 `destroy()` —— 自己写死的硬前置被跳过了** | Phase G（两个视角独立命中）；原始登记在 `settings-ia/STATUS.md`「必须做但刻意延后的」第 1 条 | **未做**。那条前置逐字写着「**必须先于任何页面化**（16 个调用点、无销毁路径，页面级构造/销毁会稳定泄漏）」，而 S4b-1/S4b-2 的页面化**已经做完了**，调用点也从 16 涨到 **24**。`info-icon.ts:47` 把 tooltip `appendChild(document.body)`，全文件无任何回收路径；`rebuildCards()` 每次重建全部 `MachineCard`，每开一次设置窗跑两遍。<br><br>**窗口关闭会兜住**，所以今天不是用户可感缺陷 —— 但判据是「页面化之前」，不是「泄漏够不够大」。**门是自己立的，越过去了且没有任何门禁会红**，这一点比泄漏本身值得记 |
| **E61** | **`SettingsRouter` 方向键走注册序、导航按 DOM 序显示 —— 机器页一出现就错位** | Phase G 整体设计审计 | **未修（a11y 回归）**。`addRoute` 用 `anchor.after(navButton)` 把子项插到父项之后（视觉序：应用/机器/aya/nano/改动足迹），而 `onNavKeydown` 遍历 `this.routeIds` = **Map 注册序**（应用/机器/改动足迹/aya/nano）—— 机器页是 `RemoteSection` 异步 `refresh()` 之后才注册的，永远排在 footprint 后面。⇒ 焦点在「机器」上按 ↓ 跳到「改动足迹」，`End` 落到最后一台机器而不是视觉最后一项。<br><br>`router.vitest.ts` 三条测试（扁平三页 / 只查 DOM 序 / 父+**一个**子项）互补但从不组合，恰好漏掉生产里唯一的真实构型（父 + **多个**子项 + 后注册）。修的时候要**同时**补那个构型的测试 |
| **E62** | **「有改动需重启」条活不过关窗口，而模块头注声称它只能靠真重启消掉** | Phase G 整体设计审计 | **未修**。`restart-notice.ts` 的 `reasons` 是**模块级内存变量**；windowMode 下 `close()` 是 `getCurrentWindow().close()`，下次 `open_settings_window` 是 `WebviewWindowBuilder::new` 建**全新窗口**。⇒ 改完远端配置 → 关设置窗（这正是改完之后的标准动作）→ 再打开 → **条没了，而 monitor 根本没重启**。<br><br>头注却写着「刻意不给『知道了』按钮…要让它消失只有一个办法：真的重启」—— 那句话现在是假的。同一批代码里 `machine-status` 已经论证过「跨重启保留是对的」并落了 localStorage，这里把一个**进程级**状态放进了**最短命的那个窗口**的内存里。<br><br>另：`panel.ts:404`（Claude 数据目录）与 `panel.ts:797`（showBgSessions）两处也需重启、也没给条子供货（诊断日志那处 Phase G 已补）|
| **E63** | **`machine-card` ↔ `remote-section` 双向值导入 —— 全仓唯一一个真运行期循环依赖** | Phase G 代码工程审计（对 175 个前端模块跑 Tarjan，3 个 SCC 里只有这个两侧都是值导入） | **未修（低风险但该断）**。`machine-card.ts:25` 从 `remote-section` 取 `describeStage`，`remote-section.ts:52` 又 import `MachineCard`。S4b-3b-3 抽 `MachineCard` 时把 `describeStage`（`ConnectStage` → 文案的纯函数）落在了旧文件里 ⇒ 两个 1100 行的文件在模块层面又粘回一块。<br><br>今天靠「顶层只有声明、没有求值」侥幸不炸，且 `eslint.config.js` 里没有 `import/no-cycle`。修法很轻：`describeStage` + `ConnectStage` 转口一起挪到第三个小模块。顺带 `machine-card.ts:26` 从 `remote-section` 转口 `type ConnectStage`，而生成物就在 `../generated/ConnectStage` |
| **E64** | **Phase G 审计报的一批工程债（本区外，逐条带证据）** | Phase G 四视角审计 | **未做，集中登记免得散失**。① **`.test.ts` 是唯一没有「覆盖面地板」的测试面** —— 16 个 `.test.ts` 靠 package.json 里 16 条手写脚本串起来，加第 17 个而忘了登记 ⇒ 它永远不跑、CI 全绿（`.vitest.ts` 靠通配符天然免疫）。② **覆盖率地板已从「余量 2-3 点」退化成 10-13 点**（阈值 40/34/36/41，实测 52.97/44.52/48.18/54.39）⇒ 约 1790 条 statement 可以变成无覆盖而不红。③ **daemon 两条护栏用非递归 `read_dir` 且反向自检地板停在 5**（crate 今天 13 个 `.rs`）⇒ 把文件搬进子目录，扫描范围会静默缩水 —— **本条属「护栏不动」红线，须用户点头才动**。④ `panorama/types.ts` 的 `PanoramaStatus` 与生成物**两份都在被消费**（`panorama/api.ts` 用手写的、`ipc/commands.ts` 用生成的）。⑤ eslint/stylelint 在 CI 里是 `|| true`，是全仓唯一没上「计数钉子」的门禁（别处都钉了：shellcheck ≥37、vendored ≥294、e2e 逐套地板）。⑥ `cc.ps1.tpl` 102 行 PowerShell 被 `include_str!` 进二进制、写进用户 profile，**零静态检查**，而同一条「是生产件就得扫」的判据已让 `shared/ccm` 进了 shellcheck |
| **E65** | **常设文档整体停在 v3.3.0，且发版 checklist 的机制性根因第 4 次未修** | Phase G 文档工程审计 | **未做**。① `README.md:5/:9/:271` 仍写 v3.3.0（v3.4.0 那个 commit 只动了 CHANGELOG/package.json/Cargo/tauri.conf），且平台仍写「Windows 10/11」——**而 v3.4.0 首次发了 `.deb`**，Linux 用户读完会得出「不支持」。② **根因未修**：BACKLOG §D P0 早已写明「`doc/RELEASING.md` checklist 加两条：README 版本号 + 功能列表 —— 不修这条下次还会漏（连续三次漏改的机制性根因）」，实查 RELEASING §1 七条里**没有一条提 README**，v3.4.0 第四次复发。③ `doc/ARCHITECTURE.md` 里 `shared/` / `cc-bus` / `generated` / `ts-rs` / `LaunchPlan` **全部 0 命中**，模块表漏 14 个 Rust + ~19 个 TS 模块；`src/README.md` 与 `src-tauri/README.md` mtime 停在 Jul 26。④ 同一事实多副本且已互相矛盾：`README.md:9` 与 `:271` 的测试数在**同一文件内**打架（536/814 vs 364/595）。⑤ `CHANGELOG.md` 没有 `[未发布]` 段，而 v3.4.0 之后已攒 30+ commit |
| **E66** | **`doc/IPC-PROTOCOL.md` §10 的 wire 帧表少了三分之一，且没有 S0 加的 `cause`** | Phase G 文档工程审计 | **Phase G 已修 `cause` 那半（S0 的欠账），三个缺帧一并补齐**。原表 6 行（hello/line/session_added/session_status/session_removed/overflow），而 `wire.rs` 的 `Frame` 是 **9 个** —— 缺 `turn_end`、`tmux_session_closed`、`tmux_sessions`；`session_removed` 行只列 `sid`，S0 加的 `cause` 全文 0 命中。`remote-daemon-proto/README.md:21` 同样写「共 6 个」且自称「字段细节以 `../doc/IPC-PROTOCOL.md` §10 为准」—— 指向一份更旧的文档。**两处都已更新** |
| ~~E46~~ | **（墓碑）编号已作废，勿引用** | `069b9cb` 补登过 E46，随后 `3aec308`「恢复被误删的 262 行 + 三条新条目重编号」把它顶掉 | **不是漏号**。留这行是因为编号账本一旦断号又无记录，外部引用（feature 文件里写「见 E46」）就会静默指向虚空 —— `S0-tmux-snapshot-staleness.md` 那处把 E47 写成 E43 正是同类事故 |
| **E69** | **`upload_atomic` 的注释与函数体直接矛盾，照注释实现会把 daemon 变砖** | aterm 侧交叉核对时发现（cc-bus DN-7，2026-07-31），本仓复核属实**且多一处** | **✅ 已修**。函数注释原写「写 tmp → **删旧** → rename」+「rename 后再 `set_metadata` 兜底」，而函数体是「写 tmp → 旧目标 **rename 成 `.bak`** → rename → 清 bak」，且代码明写**绝不** set_metadata——真机 e2e 实证 OpenSSH sftp-server 上 setstat（哪怕只设 permissions、`size=None`）会把刚 rename 好的文件**截断成 0 字节** ⇒ daemon 不可 exec ⇒ 连接 EOF ⇒ marker 变空 ⇒ 无限重部署。**模块文档 §原子写 那节也同样写着「删旧」**，一并订正（F89a 把「先删」改成「先备份」的理由是先删一旦 rename 失败就丢原件）。|
| **E70** | **daemon 的 musl 产物没挂进 Release 资产** | aterm 自部署路线的唯一前置（cc-bus DN-8，2026-07-31） | **✅ 已做，但没按请求的位置放**。挂在 `build-daemons` 会踩 release.yml 自己注释警告的坑：该 job 跑在 `build-windows` **之前** ⇒ ① 抢先创建 release（正是那段注释要避开的竞态）② **绕过** build-windows 里「四处版本号与 tag 一致」那道检查。改挂 `build-linux`（`needs: [build-daemons, build-windows]`，两条都天然继承，且它本来就下载了同一份 artifact）。资产：`cc-monitor-remote-{x86_64,aarch64}` + 各自 `.build_id`。|
| **E68** | **拉前绑定的 marker 被 claude 的状态标题冲掉（真机复现）** | 用户 2026-07-31 报「经常拉前失败」，当场查清 | **✅ 已修**。根因：`ccm` 把 tmux `set-titles-string` 设成 `#T`（窗口标题 = pane 标题），而 **claude 也在往 pane 标题写状态**（转圈 + 当前在干什么）⇒ 两个写者抢同一个位置。ccm 每 **20 秒**补一次，而点 ↗ 时的现扫窗口是 `40×100ms = **4 秒**`（`bind.rs::ON_DEMAND_BIND_ATTEMPTS`）⇒ **首次绑定约 1/5 命中**。<br><br>**实测证据**：五个会话里四个空闲的 marker 都在、唯独忙碌那个被冲成「⠐ 理解…」，点 ↗ 必弹「未绑定窗口」。<br><br>**不是「时好时坏」而是「首次绑定看运气」**：`verify_binding` 只查窗口存不存在与属主进程，**不看标题** ⇒ 一旦某次扫描撞上那 20 秒脉冲、绑定进了缓存，此后标题被冲成什么样都照常能拉（这解释了用户「为什么现在又行了」）。<br><br>**修法**：让 tmux 从 `@ccm_sid` **自己合成**窗口标题（`#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}`），与 pane 标题彻底分开 ⇒ marker **常驻**，首次绑定不再看运气。`bind.rs` 那句「4s 若仍偶发不足，据真机再抬」是在**症状层**调参（1.5s→4s），抬到 20s 才能保证，而那样点一次要等 20 秒。<br><br>e2e `ccm-rbind-title.sh`（8/8，含反向自检）；**改的是 `shared/ccm`，要重新部署 ccm 才生效**。|
| **E67** | **main 上的 CI 已经红了很久（v3.4.0 与 v3.5.0 连红）** | v3.5.0 发版时逐条看 CI 才发现 | **前端两条已修（`4891f84`），余下三处未查完**。已修：`host-os.vitest` 把 jsdom 的 UA 当成恒为 linux（Windows 上是 `win32` ⇒ unknown）· `paste-block-guard` 的 `walk()` 产出 `src\main.ts` 而白名单是`src/main.ts` ⇒ **那条守卫在主平台上从来没绿过**。<br><br>**未修**：① `config_surface::tests::settings_scopes_include_local_and_admit_project_is_unchecked` 在 Windows 上 panic（`config_surface.rs:1490`；该文件 `:368` 有 `cfg!(target_os = "windows")` 分支，怀疑同源）② E2E scripts health 里 vendored cc-acct-iso 自测报 `jq: Could not open file …/.claude-accts/accounts.json`（CI 沙箱里那几个 fixture 没建起来）③ 两个 e2e real-machine job。<br><br>**这条的价值不在这几个测试**，在于「Release 绿 + CI 红」这个组合已经持续两个版本没人处理 —— 红着的门禁等于没有门禁，而本仓为「门禁不许静默失效」建过四道地板。**修完要顺手确认 branch protection 有没有把 CI 设成必需**。
