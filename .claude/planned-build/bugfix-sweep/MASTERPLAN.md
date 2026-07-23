# MASTERPLAN — bugfix-sweep

> 目标:修完所有 [bug] open issue,再开新功能。诊断阶段(4 read-only agent + 2 我自诊)已完成,对着 v3.1.1 代码。**发版对外、用户拍板。**

## 诊断总表(8 bug,按可自动化程度分类)

### 类 1 — 可在 /loop 里自主修的代码(根因清晰、可单测、低-中风险)
| # | 根因(file:line) | 修法 | 面 |
|---|---|---|---|
| **#71** | `marked ^18` `gfm:true`(`render.ts:9`)按 GFM 规范吃**单波浪号** strikethrough;代码保护(`:166-167`)只护 code | 覆盖 `del` tokenizer **只认 `~~`**(或关删除线) | `render.ts` ★ |
| **#42** | `render.ts:172` 的全局非贪婪 `$$` 配对器 `/\$\$([^$][\s\S]*?)\$\$/g` 遇**奇数/游离 `$$`**(漏闭合、散文里讲 `$$`)错位配对→吞掉后一个真公式的开 `$$`→**把散文当公式渲染、丢真公式**;`[^$]` 守卫误伤 `$$$` | `$$` 配对**锚到行边界**(游离 `$$` 不能开/闭块)+ 去 `[^$]` 守卫 + (可选)裸 `\begin{}` 包裹 + 回归测 | `render.ts` ★ |
| **#67** | `usage-pivot.ts:97-101` 单一写死比较器(equivalentInputTokens 降序)四分组共用,无 `dim==="day"` 按日期分支;表头 `usage-view.ts:176-198` 无点击 | 比较器 dim-aware / 参数化 `sortKey`+`dir`(按天默认按日期)+ 表头可点排序 `▼/▲` | `usage-pivot.ts`+`usage-view.ts` |
| **#63①** | 活 tab 层只按 `sessionId` keyed(`tabs.ts:259,657,947`),`forkedFrom` 仅用于历史树(`history.ts:1851`)、**从不进活 tab/标题** → fork 显示成同名独立 tab | 把 `forkedFrom` 传到活 `Tab` + `computeTitleFor`/tab 按钮加 `↳` 血缘徽标+tooltip(仿历史树 orphan 标记) | `tabs.ts` ★ |
| **#41(残)** | `lib.rs:1504` verify-fail 重绑路是**单发** `try_bind()`;F75 只给兄弟 cache-miss 路(`:1489`)加了 `try_bind_with_retry`——issue 声称的"两处"只做了一处 | `:1504` 改 `try_bind_with_retry(…, ON_DEMAND_BIND_ATTEMPTS, ON_DEMAND_BIND_STEP_MS)` 镜像兄弟路 | `lib.rs`(Rust,非 daemon) |

### 类 2 — 已被前作修好,需**真机 E2E / 重装 helper 才能验/关**(loop 自主关不了,可补测/补文档)
| # | 状态 | 待办 |
|---|---|---|
| **#60** | B2(不变灰)+ F74(attach 不撞错)**已覆盖主情形** | 真机 E2E 确认(装 v3.1.1 + p1p daemon + ccm helper);补文档记残留(未插桩/daemonless/空 backend 边角) |
| **#63②** | F74 `@ccm_sid` 精确匹配已修 | **需每机重装 ccm helper**才生效;真机验后可关该症状 |
| **#63③** | 现源码**无复现路径**(尾渲染/fork 拷贝/折叠都不丢尾) | 疑旧版本;≥v3.1.1 重装后仍复现才查;可补"resumed-and-continued fork"尾完整性测(覆盖缺口) |
| **#46(字面)** | F76 30s 内存缓存已满足"短缓存"(热重开秒开) | 残留仅"每次启动首开只本地"——可选 localStorage 持久化(见类1可搬,或接受现状关) |

### 类 3 — 需**用户拍范围**(偏大 / 部分上游,不是快修)
| # | 情况 |
|---|---|
| **#43** | 根因**大半在上游 Claude Code**(bg-spare + compaction fork + daemon fleet-respawn 打败 Ctrl-X),cc-monitor 治不了上游。4 症状:①分裂两 tab=F74b 已拒 bg-spare child(部分修)、无 merge/supersede(开);②Ctrl-X 清不掉=cc-monitor 对**活的交互会话无 kill**、`closeTab` 只允许 archived(`tabs.ts:1348`)(开);③父假绿=daemon 判活只 pid+procStart、**缺心跳新鲜度维度**(`watcher.rs:210-224`),挂住但活的父恒绿(开);④拉不起=Resume 项只对 archived 出(`tabs.ts:2052`),假绿永不 archive→无 resume(开,是③的下游)。**核心开关=给 daemon 判活加心跳新鲜度**——大工程,且与**已暂停的 Codex 判活 F1b/DG2 同族**。 |

## ★共享面账本(最终形态)
- **`src/render.ts`** ← #71 + #42。最终形态:marked 配置里(a)`del` tokenizer 覆盖只认 `~~`;(b)`preprocessMath` 的 `$$` 配对行锚定 + 去 `[^$]` 守卫。两者互不踩(一个改 tokenizer、一个改预处理),**合成一个 feature(F-render)一次做完**,别两趟。
- **`src/tabs.ts`** ← #63①(fork 血缘:Tab 加 `forkedFrom`、`computeTitleFor` 加徽标)。若 #43 被拍板做"kill 活会话/supersede",也改 `tabs.ts`(closeTab 门、菜单)→ **那时与 #63① 的 Tab 结构改动协调**;#43 不做则 #63① 隔离。
- 独立无重叠:`usage-*.ts`(#67)、`lib.rs` 重绑(#41)、`history-cache`(#46)。

### 类1 追加(2026-07-23 用户真机命中,live)
| **#72** | cc-monitor 自己 Resume(tmux) 建 `cc-<sid8>` 但**不设 `@ccm_sid`**(只有远端 wrapper `ccm-wrapper.sh:24` 设);`@ccm_sid` 才是 cc-monitor attach/kill 的匹配键(`findClaudeTmux` `tabs.ts:209-231`),故自建会话自己匹配不上→cwd 回退警告/菜单移除 | resume 编排 `tmux new-session` 后加 `tmux set-option @ccm_sid <sid>`(sid 已知);落点 `session-backend.ts` createRunAttach / `remote-launch.ts` buildResumeTmuxCmd+buildLauncherCmd | `session-backend.ts`+`remote-launch.ts` |

> **§3 契约补充(与 aterm 同步中)**:「resume 编排者负责在建的 `cc-<sid8>` 上 `set-option @ccm_sid <sid>`」。aterm 同侧同 bug(其 app-resume 会话也缺 @ccm_sid → 在 cc-monitor 里 attach/kill 不了),两边各修自己那侧、不阻塞。名 `cc-<sid8>` 不动(§3 锁)。

## 实现顺序(类1,低耦合可快跑)——**用户真机命中的身份族提到最前**
1. **F-remote-pull-identity**(#41 + #72)——用户 live 正被它烦。#41=`lib.rs:1504` 加重试(镜像兄弟路)+ 重试窗口调长 +(加分)前端 ↗ 首次扫不到自动轮询几秒;#72=resume 编排设 `@ccm_sid`。同"远端拉/接身份"族,合成一个 feature。中风险(碰 Rust + 前端 + tmux 载荷;真机窗口时长要你确认)。
2. **F-render**(#71+#42)——隔离、高值、有测兜底。低风险。
3. **F-usage-sort**(#67)——隔离前端。低风险。
4. **F-fork-badge**(#63①)——隔离前端(除非 #43 同做 tabs.ts)。低-中。
5. **(可选)F-history-persist**(#46 localStorage)——隔离前端。低。

## 关键约束(影响能否"自动全做")
- **类1 的 5 个 feature 可在 /loop 里自主实现 + 单测 + gate 绿**(纯 code + vitest/cargo test)。
- **类2 关不了靠 loop**:#60/#63②③ 要真机 E2E / 重装 helper——loop 只能补测/补文档,**关 issue 是用户真机验后的动作**。
- **类3(#43)要用户先拍范围**才能进 loop:defer 全部 / 只做自主切片(给活会话加 kill,让假绿至少可清)/ 全做(解冻心跳判活,大工程)。

## 门禁 / 待用户定
1. **#43 范围**:defer(推荐,上游+暂停族)/ 只做 kill-活会话 UI(缓解②④)/ 全做心跳判活。
2. **#46**:做 localStorage 持久化(首开也暖)/ 按字面已满足直接关。
3. **自动度**:批准后 loop 连续跑类1 五个 feature(C→F 各自代码审计+工程审计),还是每个 feature 停给你看?
4. **类2 验证/关闭**:loop 只补测+补文档,真机验+关 issue 你来——确认这样处理。

## 变更记录
- 2026-07-23 建 masterplan:8 bug 诊断完,分 3 类;发现 #41/#46/#60/#63②③ 多为"已修待验"、#42/#67/#71/#63① 为真残留、#43 偏上游+暂停族。
- 2026-07-23 用户拍板:#43 defer、#46 做 localStorage 持久化、loop 连续跑类1、类2 loop 补测补文档+用户真机验关。
- 2026-07-23 用户真机命中 #41(↗ 拉前延迟弹"未绑定窗口")+ 新开 **#72**(cc-monitor 自建 resume 会话不设 @ccm_sid→attach/kill 不了/警告)。aterm cc-bus 同问收敛到同根(其 app-resume 会话同缺 @ccm_sid)。→ 把身份族(#41+#72)合成 **F-remote-pull-identity** 提到实现顺序**第 1**;§3 补"resume 编排设 @ccm_sid"契约(与 aterm 同步、各修各侧)。
- 2026-07-23 **F-remote-pull-identity 完成**(feature 1):#72 createRunAttach 加 `ccmSid?` → create 分支 `(set-option -t <name> @ccm_sid <full-sid> 2>/dev/null || true) &&`(建议-1:非阻断);#41 `lib.rs:1504` 加重试 + `ON_DEMAND_BIND_ATTEMPTS 15→40`(1.5s→4s)+ `tabs.ts` ↗ 超时 5s→8s。Phase D 2 视角无阻塞、建议-1 已修。gate:monitor 343 测/clippy 37/tsc 干净/前端 node 测过。**ledger 最终形态**:`createRunAttach` create 序列 = new-session → `(set-option @ccm_sid || true)` → send-keys → attach(`ccmSid` 省则不插,向后兼容)。剩类1:F-render→F-usage-sort→F-fork-badge→F-history-persist。
