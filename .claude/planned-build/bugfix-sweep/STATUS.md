# STATUS — bugfix-sweep

> 恢复入口。每次先读本文件 + 对应 feature 文件,从记录阶段接着干。

## 目标
把 cc-monitor 当前所有 **[bug] 标签 open issue** 全部修掉,**然后**再开新功能。纪律:每 bug **先详细全面诊断**(多 agent、对着代码定位根因 + 复现 + 影响面)→ masterplan 过**用户审批门禁** → `/loop` 自动逐 bug 实现→代码审计(D)→工程审计(E)→回看(F)。**发版对外、用户拍板。**

## 当前阶段:**feature 1 完成(过 D/E/F)→ 进 feature 2(F-render)**
- masterplan 已批准(#43 defer / #46 做持久化 / loop 连续跑类1 / 类2 补测补文档+用户真机验关)。
- ✅ **F-remote-pull-identity(#41+#72)完成 + 本地 commit**(未 push)。Phase D 无阻塞、建议-1 已修;gate 全绿(monitor 343 测/clippy 37/tsc/前端 node 测)。真机验关留用户(app 需重发)。
- bug 数 9。类1 剩:**F-render(#71+#42)← 下一个** → F-usage-sort(#67)→ F-fork-badge(#63①)→ F-history-persist(#46)。
- **下一步(feature 2)**:写 `features/02-render.md`(Phase B)→ 实现 #71(marked del tokenizer 只认 `~~`)+ #42(`render.ts:172` `$$` 配对行锚定 + 去 `[^$]` 守卫)→ D/E/F。
- **carry-forward**:#41 窗口 4s 真机标定;#72 真机验 attach 不再弹警告。

## Bug 清单(8)
| # | 一句话 | 诊断 | 子系统 |
|---|---|---|---|
| #41 | 拉前不及时(rbind 标题一次扫、无重试) | agent 跑中 | 远端拉前(Rust 窗口标题扫描) |
| #42 | 多行 `$$` LaTeX 不渲染(F73 后仍未闭合) | agent 跑中 | 渲染 `render.ts` |
| #43 | resume 分裂两 tab / 父假绿 / 拉不起 | agent 跑中 | 远端会话生命周期 `tabs.ts`/`ssh_source` |
| #46 | 历史来源加载延迟(刚进只本地)应缓存 | agent 跑中 | 历史来源加载 |
| #60 | 带外杀 tmux 不变灰 + attach 错会话 | agent 跑中(核 B2+F74 是否已修) | 远端会话生命周期 |
| #63 | fork tab 独立显示 + 内容不符 + 尾消息漏 | agent 跑中 | 分支/fork `branching.ts`/history |
| #67 | 用量排序全按等效∑降序/表头不可点/按天不按日期 | **已诊断** | 用量 `usage-view.ts`/`usage-pivot.ts` |
| #71 | 单 `~` 被当删除线(marked GFM 单波浪号) | **已诊断** | 渲染 `render.ts` |

### 已诊断
- **#67**:`src/views/usage-pivot.ts:97-101` 单一写死比较器(equivalentInputTokens 降序)对所有分组共用,无 `dim==="day"` 按日期分支;`day` 键是 ISO(可排却没被当排序键)。表头 `src/views/usage-view.ts:176-198` 只设 textContent、无点击。修:比较器 dim-aware/参数化 sortKey+dir + 表头可点。
- **#71**:`marked ^18` `gfm:true`(`src/render.ts:9`)按 GFM 规范吃**单波浪号** strikethrough;代码保护(`:165-166`)只护 code、不护散文 `~`。修:覆盖 `del` tokenizer 只认 `~~`(或关删除线)。

## ★共享面账本(种子,诊断后补最终形态)
- **`src/render.ts`** ← #42 + #71 同改此文件(渲染管线)。最终形态:一处协调好的 marked 配置/tokenizer + preprocess,#42 的数学预处理与 #71 的 del tokenizer 覆盖不互踩。**两个渲染 bug 应一起想、避免补丁叠补丁。**
- **`src/tabs.ts` + 远端会话生命周期(`ssh_source.rs`/`tmux_reconcile.rs`/`@ccm_sid` 身份)** ← #43 + #60(+ 可能 #63)同域。最终形态待诊断:resume/attach/变灰/身份匹配是一套一致语义,别各修各的。
- **用量 `usage-view.ts`/`usage-pivot.ts`** ← 仅 #67。
- 独立:#41(Rust 拉前扫描)、#46(历史来源加载)、#63(fork/branching)。

## 下一步
等 4 个诊断 agent 回 → 合成 MASTERPLAN.md(逐 bug:根因/复现/影响/修法/风险 + 依赖顺序 + 账本最终形态)+ features/NN-*.md → **呈用户审批(门禁)** → 用户批准后起 `/loop`。

## 自动模式 / loop 停止条件
- 用户要:masterplan 批准后 `/loop` 自动逐 bug 跑 C→F,全部完再 Phase G。
- **停 loop**:撞审批门禁 / 阻塞 / 计划≠现实 / 同步骤≥2 次失败 / 全部完成(先 Phase G)。**发版**不在 loop 内(用户拍板)。
- masterplan 未批准前**不起 loop**。
