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
| E31 | `shell_quote` 放错模块 | Phase G 代码工程视角审阅（重要 6） | `ssh_source.rs` fan-in 14 里，5 个模块（`accounts`/`account_usage`/`cc_bus`/`remote_history`/`tmux`）**只为一个与 SSH 无关的纯字符串工具**依赖这个 4847 行模块。搬去 `utils.rs` 即可，一处改动。（顺带：该审阅逐个查过 14 个依赖方，只用到 6 个符号 ⇒ **这是内聚的 SSH facade，不是上帝对象，不该按行数拆**） |
| E32 | Rust 侧零覆盖率门禁 | 同上（重要 7） | 31570 行 / 536 测试，无覆盖率地板。TS 侧地板只量 `*.vitest.ts` 而分母含 `*.test.ts` 覆盖的文件 ⇒ 只能设到 40/34/36/41，且新增只有 `*.test.ts` 覆盖的模块会**压低**全局数字、可能误红 |

| E33 | **远端 tab 带外杀 tmux 后「变灰」延迟长到被用户当成 bug** | 用户 2026-07-29 真机观测 + `tmux_reconcile.rs` 头注自陈 | **未做**。用户报「一直有个 tab 不灰」，随后自行变灰 ⇒ 机制没坏，是**延迟**。**延迟可以算出来**：`TMUX_EMIT_INTERVAL`(**8s**，`remote-daemon-proto/src/watcher.rs:65`) × `RETIRE_MISS_THRESHOLD`(**2**) ≈ **16 秒**。（`tmux_reconcile.rs` 头注说这两个是「占位常量、真机未标定」——**那句只对 threshold 准**，`TMUX_EMIT_INTERVAL` 是有明确值的；用户追问「不是都没有轮询了」时才查清的。另：**轮询没消失，是搬走了**——删掉的是 monitor 侧每 8s 新建 SSH 跑 tmux ls，换成 daemon 在它自己那台机器上周期跑、经 `TmuxSessions` 帧上报；「不新增轮询」红线守的是 monitor 侧那条。`RETIRE_MISS_THRESHOLD` **不能降到 1**——`tmux_reconcile.rs:31` 有编译期断言钉死 `>= 2`，因为 `/branch` 漂移有 ~1s 竞态窗），且「带外杀端到端变灰」本身就在它的**真机累积项**里、**从没验过**。⇒ 这是那条累积项的**第一个真机观测点**。做法：① 标定两个常量；② 给 tab 加一条可见的诊断（数据来源 / `ever_bound` / `miss` 计数），让**三格**一眼可分——它们从 UI 上完全看不出区别，全表现为「不灰」：① **在等那 ~16 秒**；② **daemon 没在发 `TmuxSessions` 帧**（没跑 / 版本旧 / `raw` 是 `NO_TMUX` / backend 集为空被观测无效门保守跳过）⇒ **对账一轮都没执行，不是延迟长**；③ **`never_bound` 按设计永不判**（`never_bound` 的 sid 按设计**永不变灰**，见 `reconcile_step` 末支注释——bg / 无 wrapper / 直起 claude 免疫误 retire）。**注意本地会话完全不经这条路**：`reconcile_step` 全仓只在 `ssh_source.rs:2383` 的远端收帧循环里被调，独立 poller 已删（`lib.rs:705`） |

| **E34** | **★ 把 tmux 存活的「轮询」换成事件：带外杀 tmux 后近乎零延迟变灰** | 用户 2026-07-29 明确要求「我要把轮询杀掉」 | **已升格为独立工作区 `zero-poll-liveness`（2026-07-30 主计划已批、P0 已交付）**。下方旧小节保留作历史，但**其中三条已被实测推翻**，见该小节顶部的订正块 |
| **E39** | **`notify-debouncer-mini` 静默吞掉 inotify 队列溢出 ⇒ 溢出时事件永久丢失** | `zero-poll-liveness` P0-⑤ 读源码实测（2026-07-30） | **既有盲区，非新引入**。`notify 6.1.1/src/inotify.rs:208` 把 `Q_OVERFLOW` 报成 `EventKind::Other + Flag::Rescan`，但 `notify-debouncer-mini 0.4.1/src/lib.rs:319` 的 `add_event` **只读 `event.paths`**，而溢出事件 `paths` 为空 ⇒ 循环体一次不执行 ⇒ 完全不可见。**两端都中招**（daemon 与 `src-tauri/src/watcher.rs` 用同一套 debouncer）。今天没有周期兜底：目录发现（`SessionAdded`）与 jsonl 行流只靠 notify 事件。**pidfd 对此免疫** ⇒ `zero-poll-liveness` P2 会把判活这一路摘出 inotify 依赖。处置选项：① 换 raw `notify` + 自写去抖（要保 `DEBOUNCE_MS=100` 双写点语义）② 加一个只为拿 `need_rescan` 的 raw-notify 哨兵实例 ③ 接受并登记。**绝不补定时器**（那等于零轮询造假）。**结论来自读源码，未真触发过溢出** |
| **E41** | **6 套非 CI 的 e2e 套件不做 tmux socket 隔离 ⇒ 在开发机上会动开发者自己的 tmux server** | `zero-poll-liveness` P1 工程审计实测（2026-07-30） | **未修，已改 P6 计划**。`graylight-suite` · `graylight-daemon-frames` · `restart-suite` · `restart-daemon-frames` · `resume-suite` · `resume-daemon-frames` + helper `gen-idle-tmux.sh` —— **一处 `-L` 都没有**，直接在**默认 socket** 上 `new-session` / `kill-session`（如 `graylight-daemon-frames.sh:56`）。在 CI 的干净容器里无害，但这 6 套正是 **E14** 记的「不在 CI」那 6 套 ⇒ 实际只会被人在开发机上手跑。对比：CI 那 8 套安全（6 套带 `-L`：`ccm-acceptance`/`ccm-pretrust`/`cc-spawn-uplift`/`tmux-guarded`/`tmux-target`/`usage-probe`；另 2 套 `ccm-cli.test.sh`/`ccm-print-parity.sh` **根本不调 tmux**——grep 命中的 "tmux" 全在注释与 `--print` 断言里，`ccm-cli.test.sh:4` 明写「不真起 agent、不碰 tmux」）。⇒ `zero-poll-liveness` **P6 的延迟 e2e 原计划挂在 graylight 一族，实测行不通**：要么先给这 6 套加 `-L` 隔离（顺带闭合本项 + 让 E14 进 CI 变得安全），要么改用已隔离的载体。**旁证**：`graylight-daemon-frames.sh:30` 留了个 keepalive 会话，注释自陈是为了绕开 §24bis 空 backend 守卫——而那正是 P1 修掉的 bug ⇒ 该 bug 当年是被**测试侧绕过**而不是被发现 |
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
