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
