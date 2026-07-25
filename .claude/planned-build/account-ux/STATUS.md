# STATUS — account-ux(账号切换 UX/UI 完善)

> 恢复入口。承接 account-isolation A0–A6(v3.2.0)。本轮纯前端 UX/UI 完善,不触发发版。
> 每轮开头先读本文件 + 当前 feature 文件,从记录阶段接着干。

## 当前阶段:**✅ 主计划 + 4 决策已批准 · 全自动 loop 运行中 · U1 完成 → 当前 U2**

> **进度(分支 `account-ux`,不 push)**:
> - U1 ✅ `72c3b1e`:纯函数地基(resolveFollowAccount/currentWorkingAccount/detectAccountMismatch/accountColorSlot/sessionBadge source)。门禁 tsc0 / vitest 475 / remote-launch 回归绿。
> - U2 ✅ `59012b5`:withAccount follow 第三态(opt-in)。门禁 tsc0 / vitest 481(+6)/ A4 老 5 用例不改保持绿 / remote-launch 契约绿。尚无调用点。
> - U3 ✅ `4d9140b`(接线)+ 审计修复(本 commit)。**Phase D 对抗审计签收**:无硬阻塞,门禁独立复跑全绿(attach 焊死/daemon 零改/降级落基座/显式零回归/逐字节契约/跨 config-dir 因 projects 共享 symlink 安全,全部证实)。**揪出 重要-1 sticky clobber**(history 默认 resume 用 `follow:{}` 不读该行 pin→落 current **且改写** lastAccount→污染 tab 路径,违反你 #1 决策"粘性优先")→ **已修**:①`withAccount` 不-clobber 记账(既有 pin 存在且解析≠pin 时不记,保住 pin;no-owner 才 become sticky)②`history.runResume` 用 `list_last_accounts` 读该行 pin 传 follow(粘性读在 history 入口也成立)。门禁 tsc0 / vitest **482** / remote-launch / history node 全绿。**遗留 fast-follow(建议级,审计确认核心已覆盖)**:4 处接线的"真注入正路"在调用点层无护栏测(tabs.vitest 加注入测有 mockImplementation/accountsCache 跨测污染风险,暂不塞)。
> - U4 ✅(本 commit):当前工作账号语义面 + chip 升级。styles.css 加 8 色 --acct-cN/inkN token + .acct-avatar 圆角方块头像(+ghost 态);account-color.ts 加 accountAvatarEl 视图 helper;account-chip.ts chip 显账号彩色头像 + 术语"默认"→"当前工作账号" + 切号 toast 三句式(变/不变);accounts-section.ts + remote-section.ts 术语改名 + 预选谓词 effectiveDefault→currentWorkingAccount。门禁 tsc0 / vitest 485(+3 头像测)。低-中风险主线程复核:改名值不变、头像 additive、CSS 不覆盖既有,零回归。设置表头像/IA 留 U7。
> - **当前 → U5(tab 徽章升级:信息才显 + live/last 层次 + 头像)**。

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
- [ ] U3 接线 resume/新会话跟随 — 风险中
- [ ] U4 当前工作账号语义面 + chip 升级 — 风险低-中
- [ ] U5 tab 徽章升级(信息才显) — 风险中
- [ ] U6 不一致检测 + 一键对齐 — 风险中高(破坏性)
- [ ] U7 设置账号组 IA 重排 + 补 CSS — 风险中
- [ ] U8 可发现性 + 快捷键 + 降级润色 — 风险低
- [ ] U9(可选)解钉跟随当前账号 — 风险低

## 自动模式 / 本轮 loop 目标 / 停止条件
- **自动度**:用户要「全自动 loop」= 连续跑。批准主计划 + 4 决策后,loop 连续 B→F 逐功能推进(U1→U9),共享面最终形态已在账本预定 ⇒ 功能计划朝最终形态实现、不停每功能门禁;全部完成再 Phase G。
- **每轮 = 一个功能走 C→F**(实现→代码审计 D→工程审计 E→回看 F),停在干净检查点(STATUS 更新 + 本地 commit 检查点,不加 Co-Authored-By,不 push)。
- **停止条件(任一即停,省略 ScheduleWakeup 交回用户)**:阻塞 / 计划≠现实需决策 / 同一步 ≥2 次失败 / 需新决策(如冒出账本外新共享面)/ 全部完成(先跑 Phase G 再停)。
- **兜底延迟**:每轮重(审计并发多 agent)、由真实实现耗时驱动,兜底 ≥1200s,不短间隔空转。

## 关键红线(沿用 account-isolation)
- daemon 只读铁律:不新增任何 daemon/Rust 写命令(本轮判定纯前端零 daemon 面 ⇒ 不 bump BUILD_ID、不发版)。
- 不碰用户 `~/.claude`;不改 `cc-<sid8>` 会话名;远端优先(本地 A7 不做)。
- 不 push / 不发版除非用户拍板;commit 仅本地检查点、不加 Co-Authored-By。
- **防终端污染纪律**:每步 Read 回盘;测试重定向文件再 Read + grep 计数;绝不信内联绿、绝不 watch;门禁 pipefail。

## 关联
- 底座:`../account-isolation/`(MASTERPLAN 共享面账本 + DESIGN-account-switching.md 交互基线)。
- 三视角设计提案原文:本轮 Phase A 由 UX/UI/架构三 agent 产出(结论已并入 MASTERPLAN)。
