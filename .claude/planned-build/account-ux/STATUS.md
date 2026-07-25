# STATUS — account-ux(账号切换 UX/UI 完善)

> 恢复入口。承接 account-isolation A0–A6(v3.2.0)。本轮纯前端 UX/UI 完善,不触发发版。
> 每轮开头先读本文件 + 当前 feature 文件,从记录阶段接着干。

## 当前阶段:**✅ U1–U8 全部完成 → 进 Phase G 最终验收**

> **进度(分支 `account-ux`,不 push)**:
> - U1 ✅ `72c3b1e`:纯函数地基(resolveFollowAccount/currentWorkingAccount/detectAccountMismatch/accountColorSlot/sessionBadge source)。门禁 tsc0 / vitest 475 / remote-launch 回归绿。
> - U2 ✅ `59012b5`:withAccount follow 第三态(opt-in)。门禁 tsc0 / vitest 481(+6)/ A4 老 5 用例不改保持绿 / remote-launch 契约绿。尚无调用点。
> - U3 ✅ `4d9140b`(接线)+ 审计修复(本 commit)。**Phase D 对抗审计签收**:无硬阻塞,门禁独立复跑全绿(attach 焊死/daemon 零改/降级落基座/显式零回归/逐字节契约/跨 config-dir 因 projects 共享 symlink 安全,全部证实)。**揪出 重要-1 sticky clobber**(history 默认 resume 用 `follow:{}` 不读该行 pin→落 current **且改写** lastAccount→污染 tab 路径,违反你 #1 决策"粘性优先")→ **已修**:①`withAccount` 不-clobber 记账(既有 pin 存在且解析≠pin 时不记,保住 pin;no-owner 才 become sticky)②`history.runResume` 用 `list_last_accounts` 读该行 pin 传 follow(粘性读在 history 入口也成立)。门禁 tsc0 / vitest **482** / remote-launch / history node 全绿。**遗留 fast-follow(建议级,审计确认核心已覆盖)**:4 处接线的"真注入正路"在调用点层无护栏测(tabs.vitest 加注入测有 mockImplementation/accountsCache 跨测污染风险,暂不塞)。
> - U4 ✅(本 commit):当前工作账号语义面 + chip 升级。styles.css 加 8 色 --acct-cN/inkN token + .acct-avatar 圆角方块头像(+ghost 态);account-color.ts 加 accountAvatarEl 视图 helper;account-chip.ts chip 显账号彩色头像 + 术语"默认"→"当前工作账号" + 切号 toast 三句式(变/不变);accounts-section.ts + remote-section.ts 术语改名 + 预选谓词 effectiveDefault→currentWorkingAccount。门禁 tsc0 / vitest 485(+3 头像测)。低-中风险主线程复核:改名值不变、头像 additive、CSS 不覆盖既有,零回归。设置表头像/IA 留 U7。
> - U5 ✅(本 commit):tab 徽章"信息才显"。updateAccountBadge 重写——用 detectAccountMismatch(U1):会话账号==当前工作账号/未知当前/账号未知→不挂徽章;≠当前 live→实心 .acct-avatar / lastAccount→幽灵头像。setSessionAccounts 加 currentByOrigin 形参(additive);main.ts refreshSessionAccounts 传每 origin 的 currentWorkingAccount;.tab-acct-badge 退化为容器(旧实色药丸/`—`未知态废)。门禁 tsc0 / vitest 490(+5 徽章行为测)。纯显示层主线程复核零回归。
> - U6 ✅(本 commit):**不一致检测 + 一键对齐**(破坏性,中高风险)。详见 `features/06-u6-mismatch-align.md`。
>   实现:tab **hover** `⇄`(JS 打 `.is-eligible`/CSS 管露面)+ chip `⚠k`(**推**模型)+ 批量两步确认;
>   单一谓词 `alignableCurrent` 统管 ⇄/⚠k/批量/U8-CtrlK;U8-ready public API 一次定型。
>   **Phase D 三 agent 并行对抗审计 —— 揪出 1 阻塞 + 6 重要 + 1 架构,全部已修**:
>   * **阻塞**:`idle` 判据误用 `!== "running"`(那是 **subagent** 状态串;会话枚举是
>     `busy/idle/shell/waiting/null`)⇒ busy 桶恒空 ⇒ 用户决策④「回合中二步确认」**生产中是死代码**,
>     且把跑着回合的会话文案成「空闲…几乎无感」。三个 agent 独立命中。已改白名单(未知即保守)。
>     **元教训**:门禁 490 全绿没挡住——我的测试照着实现抄了同一个错枚举。**绿的可能是错的契约。**
>   * 重要:current 不可选仍显 ⚠k/⇄(死按钮、批量全败还报成功)→ 新纯函数 `alignableCurrentAccount`;
>     汇总按发起数**谎报成功** → `restartWithAccount` 加布尔返回值报真实成败;批量/单会话**无重入防护**
>     (第二批可杀掉第一批刚 resume 的新进程,且批量已关逐个确认 ⇒ 全程无拦截)→ 两级守卫;
>     **切号后 ≤10s 反向窗口**(chip 显新号、对齐却把会话打回刚切走的旧号)→ `onDefaultChanged` 即时重算;
>     批量循环无 per-iteration catch → 补;确认文案不实(实际每会话**新开终端窗口**、旧窗口不自动关、
>     进程内状态会丢)→ 照实重写。
>   * 架构:`AccountChipDeps` 反拉 TabManager + 前向引用(靠「中间恰好没 await」的隐式不变量避 TDZ)
>     → 改**推**模型 + chip 构造移到 tabs 之后,与 `onActiveUsageChanged` 惯例一致。
>   **测试重写(D3 变异测试证明原 9 用例是「CSS 显隐外壳」)**:删掉 ⇄ 整个点击监听 / 偷偷取消破坏性
>   二次确认 / 对齐到 `WRONG-ACCT`,三个突变原本**全绿**;重写后分别被 2/1/1 个用例杀死,退回本次阻塞
>   bug 也被 4 个用例杀死。门禁:tsc0 / **npm test 522**(490→+32)/ build ✓。
> - U7 ✅(本 commit):**设置账号组 IA 重排 + 补表格 CSS**。详见 `features/07-u7-settings-ia.md`。
>   实现:「当前工作账号」横幅(不可用时 ghost 头像 + 如实说不可用)、账号表网格化、维护区收进
>   `<details>`(默认展开态**按状态给**:只有 1 个账号时展开——刚部署完唯一的正路就是"加第二个账号")、
>   补齐账号表这一片的 CSS(18/20,两个纯容器 class 不需要规则,如实记账)。
>   **Phase D 两 agent 并行审计 —— 1 阻塞 + 6 重要,全部已修**:
>   * **阻塞**:`display:grid` 打在**行**上而表是 flex ⇒ 每行各自独立 grid,`auto`/`fr` 按各自那行算宽,
>     **列跨行对不齐**——而当前账号那行少一颗「设为当前账号」按钮(in-place 行少「登录终端」),
>     行宽本就差 ~85px ⇒ 最显眼的 `.current` 行截断点与别行参差。**"列对齐"正是本功能的头号 DoD,
>     写了却不生效。** 已改 subgrid(列定义放表上)+ `@supports` 退回 flex 兜不支持的引擎。
>   * 重要:横幅与 hint **逐字重复**同一句(IA 重排新增第三份拷贝却没消重);维护区折叠在**首次使用**
>     挡路(刚部署完只有 1 个账号,唯一正路被折起来);`reload()` 重建 DOM 让展开态与半截输入被吞;
>     头像色槽用例**弱绿**(fixture `z`/`b` 恰好同槽 5,实现改成"永远同一个名"照样过);`.none` 语义
>     与 class 名/注释/取值三处矛盾;无断言锚住「行子元素数 == grid 列数」。
>   * 建议已办:`.current` 底色与 hover 同值(当前行 hover 零反馈)、路径列权重压过邮箱(倒挂)、
>     「已登录」涂绿违反决策③「信息才显」、向导框里套框、`list-style: revert` 是 no-op 且全仓唯一原生三角、
>     死 CSS `.accounts-maint-title`、窄窗把最右按钮直接裁掉且点不到。
>   **自查出的真 bug**:CSS 注释里 `-*` 紧跟斜杠**提前闭合注释**,构建 `css-syntax-error` 且注释文字漏进
>   样式表 —— **tsc 和 vitest 都看不见 CSS,只有 build 抓得到**(所以 build 必须进门禁且警告要清零)。
>   变异验证 4 个全部被杀。门禁:tsc0 / **npm test 536**(522→+14)/ build ✓ 0 警告。
> - U8 ✅(本 commit):**可发现性 + 快捷键 + 降级休眠固化**。详见 `features/08-u8-discoverability.md`。
>   实现:`selectableAccounts`/`accountColorsActive` 纯函数;chip 身份头像休眠;`Acct` 快捷键分组
>   (两条 action **默认不绑**——align 是破坏性的);Ctrl+K 术语统一 + 两条对齐命令(抽成
>   `account-commands.ts` 纯函数);新会话下拉文案说清后果。
>   **Phase D 审计 —— 1 阻塞 + 8 重要,全部已修**:
>   * **阻塞**:休眠**装错了地方**。我加在 tab 徽章上,而 `hide()` 连 U6 的 ⇄ 一起关;可 ⚠k /
>     Ctrl+K / 快捷键都不睡 ⇒ **鬼影可达**(≥2 账号但只 1 个可选、且有活会话跑在不可选账号上时:
>     chip 报 ⚠1、Ctrl+K 有对齐命令,而所有 tab 上一个徽章一个 ⇄ 都没有)。更根本的是**理由就不成立**:
>     tab 徽章自 U5 起「信息才显」,只在确知不一致时才渲染,从来不是颜色噪音,而是唯一的
>     per-session 不一致信号。→ 休眠**只留给 chip 的常显身份头像**,tab 那侧整个撤掉(连带撤回
>     `setSessionAccounts` 第 6 形参,**反而少碰一个共享面**)。**规则:颜色可以睡,信息和操作不能睡。**
>   * 重要:U7 的 `<2` 阈值仍数**总**账号数(正是计划禁止的"两处各数各的",1 isolated + 1 in-place 时
>     两个组件对"你有几个账号"给出相反判断)→ 接同一纯函数;Ctrl+K 可用性判定**不可测**(长在 main.ts
>     闭包里,改成恒 true 也不会红)→ 抽 `account-commands.ts` + 11 条测;chord 冲突守卫**留了个洞**
>     (跳过 available:false,而 dispatcher 不跳 ⇒ 一条未上线 action 能把同键的已上线快捷键彻底打哑)
>     → 不再跳过;chip 头像休眠零覆盖,且我**补测时第一版在测试里重抄了渲染分支**(=测自己)→ 改成走
>     真实 refresh();`element.click()` 在 chip 隐藏时会开出飘到视口外的菜单 → 加 `openMenu()`;
>     快捷键静默拒绝 → 给一句话;`activeSid` 被闭包冻住 → run 里重取;两处术语/语义残留 → 改。
>   **顺手堵的静默坑**:`editor.ts` 里 category 顺序是手抄数组,TS 不报错,漏加会让整组快捷键在编辑器
>   里**静默消失** → 提成 `CATEGORY_ORDER` 单一真相源 + 覆盖测。
>   门禁:tsc0 / **npm test 564**(536→+28)/ build ✓ 0 警告。变异验证 4 个全杀。
> - **当前 → Phase G 最终验收**(/full-audit + 主计划终账 + 端到端 + 收尾汇报)。

- **Phase A 产物**:`MASTERPLAN.md`(目标/架构/★共享面账本/U1–U9 拆分)。三视角设计 agent 已交叉收敛。
- **用户已拍板的 4 决策(全选推荐项,已锁进语义)**:
  1. **优先级 = 粘性优先**:显式选号 > 会话 lastAccount > 当前工作账号 > 基座。
  2. **无主会话 resume = 跟随当前账号**(无 lastAccount 的陌生来源会话也跟随,实现"拨号即生效")。
  3. **徽章 = 信息才显**(==当前账号不挂徽章;≠当前 live=实心头像 / last=幽灵 / 未知=无退 tooltip)。
  4. **批量对齐 = 要,分两步确认**(空闲默认纳入、回合中二步确认;逐会话独立继承 restart §5.2 失败语义)。
- **loop 授权**:用户「全自动 loop」= 连续跑 U1→U9 + Phase G;共享面最终形态已在账本预定 ⇒ 功能计划朝最终形态实现、不停每功能门禁;仅阻塞/计划≠现实/≥2 次失败/需新决策/全完成时停。

## 功能清单(见 MASTERPLAN §Features)
- [x] U1 地基纯函数(解析器 + 账号色 + 徽章 source) — ✅ 72c3b1e
- [x] U2 withAccount follow 模式 — ✅ 59012b5
- [x] U3 接线 resume/新会话跟随 — ✅ 4d9140b + 审计修(D 签收,clobber 防护 + history 读 pin)
- [x] U4 当前工作账号语义面 + chip 升级 — ✅(色 token + 头像 + 术语改名 + toast 三句式)
- [x] U5 tab 徽章信息才显 — ✅(detectAccountMismatch 判定 + live 实心/last 幽灵头像)
- [x] U6 不一致检测 + 一键对齐 — ✅(hover ⇄ + ⚠k + 批量两步确认;D 审计 1 阻塞 6 重要全修)
- [x] U7 设置账号组 IA 重排 + 补 CSS — ✅(横幅 + 网格化表 + 维护区折叠;D 审计 1 阻塞 6 重要全修)
- [x] U8 可发现性 + 快捷键 + 降级休眠固化 — ✅(休眠只作用于 chip 头像;D 审计 1 阻塞 8 重要全修)
- [ ] U9(可选)解钉跟随当前账号 — 风险低

## 自动模式 / 本轮 loop 目标 / 停止条件
- **自动度**:用户要「全自动 loop」= 连续跑。批准主计划 + 4 决策后,loop 连续 B→F 逐功能推进(U1→U9),共享面最终形态已在账本预定 ⇒ 功能计划朝最终形态实现、不停每功能门禁;全部完成再 Phase G。
- **每轮 = 一个功能走 C→F**(实现→代码审计 D→工程审计 E→回看 F),停在干净检查点(STATUS 更新 + 本地 commit 检查点,不加 Co-Authored-By,不 push)。
- **停止条件(任一即停,省略 ScheduleWakeup 交回用户)**:阻塞 / 计划≠现实需决策 / 同一步 ≥2 次失败 / 需新决策(如冒出账本外新共享面)/ 全部完成(先跑 Phase G 再停)。
- **兜底延迟**:用户 2026-07-25 指定 **60s 短间隔**(「不要间隔这么长」)。审计 agent 完成会自动唤醒,
  等报告期间不重复审计 agent 已在做的事、不提前开下一个功能。
- **流程欠账(D2 指出)**:U3–U6 未逐个建 feature 文件(偏离「每功能 DoD 硬门」)。U6 已补
  `features/06-u6-mismatch-align.md`;U3–U5 的结论留在本文件进度行,不回溯补建(收益低于噪音)。
  **U7 起恢复"先建 feature 文件再动手"。**
- **教训(写给后续轮次)**:U6 的阻塞 bug 是"照着实现抄测试"——判据用了个生产不存在的枚举值,
  测试跟着用同一个值,490 个用例全绿而分支从未被覆盖。**断言要锚在契约的真值上**(枚举去
  `bridge.rs`/`session-status.ts` 对),**并对关键安全属性做一次变异验证**(故意改坏,看测试会不会红)。
  U7 又添两条同型教训:① **fixture 撞值也会造成弱绿**——`z`/`b` 恰好落同一个色槽,于是"头像颜色跟随该行
  账号"的断言等于没写;选 fixture 时先验证它们在**被测维度**上确实不同,并把这个前提写成一条断言。
  ② **build 必须进门禁**:CSS 的错(注释提前闭合、规则写了不生效)tsc 与 vitest 一个都看不见;
  且要看 **WARNING 清零**,不能只看 exit code。

## 关键红线(沿用 account-isolation)
- daemon 只读铁律:不新增任何 daemon/Rust 写命令(本轮判定纯前端零 daemon 面 ⇒ 不 bump BUILD_ID、不发版)。
- 不碰用户 `~/.claude`;不改 `cc-<sid8>` 会话名;远端优先(本地 A7 不做)。
- 不 push / 不发版除非用户拍板;commit 仅本地检查点、不加 Co-Authored-By。
- **防终端污染纪律**:每步 Read 回盘;测试重定向文件再 Read + grep 计数;绝不信内联绿、绝不 watch;门禁 pipefail。

## 关联
- 底座:`../account-isolation/`(MASTERPLAN 共享面账本 + DESIGN-account-switching.md 交互基线)。
- 三视角设计提案原文:本轮 Phase A 由 UX/UI/架构三 agent 产出(结论已并入 MASTERPLAN)。
