// P2s（定框 C8）：**每台机一份后端策略** —— 今天只有一条「monitor 退出时是否结束它」。
//
// # 〔B2 · 条 66 · `设计/01 §3.3b`〕那个值**不住这里**，也不住 monitor 进程里
//
// 它住**后端所在那台机器**上（后端自己的状态文件，一台机器一个值），**只有后端写**。
// 本文件与 `settings/backend-section.ts` 读它、改它都经两条后端命令
// （`backend_exit_policy` / `set_backend_exit_policy` → 后端 `exit-policy-read` / `exit-policy-set`）。
// ⇒ 搬家前那一套（存进 monitor 的 config.json、启动时与改动时**推**给 Rust、一条写盘串行链）
//   **整条删掉**：留着就是第二个真相源（`§3.3b ②`）。原来那条「两个写者」的担心换了解法 ——
//   不是「让 Rust 别写」，是「让两边写不同的文件」，后端那份前端从不碰（单写者）。
//
// # ⚠ 「不结束」到底等不等于「继续跑」——**今天要看它有没有真脱离**（K-P1 08-26 翻面）
//
// 翻面之前：backend 是纯 stdio 子进程，monitor 一退读端就断，它在 **153 毫秒**内自己
// broken-pipe 退出（实测 08-11，P2s §0a）。那时这条策略的真实语义是
// 「立刻结束」与「让它自己退」之差，不是「后台常驻」，而 P2s-Y5 据此立了一条**无条件**禁令。
//
// K-P1 给了后端一个监听口，并让它在 Linux 上**真脱离** ⇒ 那一支上「勾掉」**真的**是继续跑。
// ⇒ 禁令换成**按状态分档**（〔MIG-2 · ㊴〕今天那四档由后端出成品，见本头注末段）。
//
// ★★ 「无人监护」这半是用户裁定的一半，不许省（DECISIONS K14 逐字：
// 「第一档必须在 UI 上如实说『继续跑，无人监护』，这是本裁定的一半，不许只做常驻不做这句话」）。
//
// 〔CP2b · 第四波〕下面那几句话的**家搬进了文案表**（`src/shared/copy/table.json` 的 `backendPolicy.*`）。
//   这里的导出名照旧（界面与测试按名字取），值从表里取；Rust 那侧从前那份「同名同值、逐字对拍」的副本随之删掉。
//
// 〔PB1 · `设计/90 §4` 阶段 B〕崩溃读数（「健康」那一格）的三档判定与那几句**搬走了**：后端出成品
//   （`backend_policy.rs::health_face`，状态 ＋ 一句 ＋ ⓘ ＋ `[详情]`），`settings/backend-section.ts` 只排版。
//   这里原来那段（三档判定 · `[详情]` 判定 · 取文常量 · 读数接口）整段删了，判据 `judgment-single-home` 的 J21 钉它零实现。
//
// 〔MIG-2 · `99 §2.1 ㊴`〕「退出行为」那四档也搬走了：后端 `exit-policy-read` 自己出成品 `said`（它知道自己是回环常驻
//   还是被监护，`control/exit_policy.rs::said`），界面原样摆。这里原来的四句取文口 · `ExitState` · `describeExitBehavior`
//   〔散文墓碑〕随之删了；「那四句只有一个家」今天就是文案表 ＋ 后端那一处取文。

/**
 * 本机在 origin 这套命名里的名字。
 *
 * ⚠ **跨语言常量**：Rust 侧是 `inbound_client::LOCAL_ORIGIN`。两边漂了**不会报错** ——
 * 本机开关会去操作一个谁都没登记过的 origin，设了没反应且不报错。
 * 由 Rust 侧一条判据逐字对拍（`the_local_origin_is_the_same_string_on_both_sides`）。
 */
export const LOCAL_ORIGIN = "<local>";
