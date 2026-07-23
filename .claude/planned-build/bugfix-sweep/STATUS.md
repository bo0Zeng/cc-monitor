# STATUS — bugfix-sweep

> 恢复入口。每次先读本文件 + 对应 feature 文件,从记录阶段接着干。

## 目标
把 cc-monitor 当前所有 **[bug] 标签 open issue** 全部修掉,**然后**再开新功能。纪律:每 bug **先详细全面诊断**(多 agent、对着代码定位根因 + 复现 + 影响面)→ masterplan 过**用户审批门禁** → `/loop` 自动逐 bug 实现→代码审计(D)→工程审计(E)→回看(F)。**发版对外、用户拍板。**

## 当前阶段:**feature 4 完成(过 D/E/F)→ 进 feature 5(F-history-persist,类1 最后一个)**
- masterplan 已批准(#43 defer / #46 做持久化 / loop 连续跑类1 / 类2 补测补文档+用户真机验关)。
- ✅ **F-remote-pull-identity(#41+#72)** — commit `0d228fd`(本地)。
- ✅ **F-render(#71+#42)** — 完成 + 本地 commit。Phase D 2 视角:#71 SOLID;**#42 首版有 3 回归(CRLF/开前有字/闭后标点)+ 测试不区分** → 已**重写 #42 正则(行边界规则,离线 harness 实证全过)+ 测试改区分性(preprocess 层)+ #71 注释更正**。gate:tsc / render 22 测 / 全套 333 测 / ReDoS 安全。
- ✅ **F-usage-sort(#67)** — 完成 + 本地 commit。Phase D 2 视角无阻塞;重要项(首列排 key 非 label / 视图层零测 / 流式重渲弹顶)全修 + 补 6 条 DOM 测;并修掉我自己写反的平手符号。gate:tsc / 全套 npm test(vitest 339)。
- ✅ **F-fork-badge(#63①)** — 完成 + 本地 commit `a0345e6`。Phase D 2 视角均无阻塞/无重要;采纳建议(applyForkedFrom 前移到 turnEndNotifier 之前)+ 补 3 条 pin 测(单 ↳ / 远端序 / tooltip,共 6 条 #63①)。**未改 styles.css**(账本预测未命中,纯文本前缀无需 CSS)。
- bug 数 9。类1 剩:**F-history-persist(#46)← 最后一个**。
- **下一步(feature 5)**:`features/05-history-persist.md` → 实现 #46(把 `remoteCache` 持久化到 localStorage + 构造时 hydrate,让每次启动**首开**也暖、不再只本地;F76 已做 30s 内存缓存,这里补跨启动持久)→ D/E/F。之后 **Phase G**(`/full-audit`)。
- **carry-forward(+#63①)**:远端 fork 徽标依赖 daemon 实时行是否透传 `forkedFrom`(`remote_history` 对远端历史已不提取 fork 关系)——真机验。
- **carry-forward**:#41 窗口 4s 真机标定;#72 真机验 attach 不再弹警告。#71 多波浪号中行 cosmetic 偏差(无害)。

## Bug 清单(9,诊断全部完成)
| # | 一句话 | 状态 |
|---|---|---|
| #41 | 拉前不及时(verify-fail 重绑单发 + 窗口太短) | ✅ 修完(feature 1) · 真机标窗口=carry-forward |
| #72 | 自建 resume 会话不设 `@ccm_sid` → attach 弹回退警告 | ✅ 修完(feature 1) · 真机验=carry-forward |
| #71 | 单 `~` 被当删除线(marked GFM 单波浪号) | ✅ 修完(feature 2) |
| #42 | 奇数/游离 `$$` 吞掉真公式 | ✅ 修完(feature 2,首版有回归已重写) |
| #67 | 用量排序:全按等效∑降序 / 表头不可点 / 按天不按日期 | ✅ 修完(feature 3) |
| #63 | fork tab 独立显示 + 内容不符 + 尾消息漏 | ① 待修(feature 4);② F74 已修(需重装 ccm 验);③ 现源码无复现路径(疑旧版) |
| #46 | 历史来源加载延迟(刚进只本地) | 字面已由 F76 满足;**持久化待做**(feature 5) |
| #60 | 带外杀 tmux 不变灰 + attach 错会话 | B2+F74 已覆盖 → **待你真机验证后关** |
| #43 | resume 分裂两 tab / 父假绿 / 清不掉 / 拉不起 | **defer**(大半上游 + 依赖已暂停的 daemon 心跳判活) |

### 已诊断
- **#67**:`src/views/usage-pivot.ts:97-101` 单一写死比较器(equivalentInputTokens 降序)对所有分组共用,无 `dim==="day"` 按日期分支;`day` 键是 ISO(可排却没被当排序键)。表头 `src/views/usage-view.ts:176-198` 只设 textContent、无点击。修:比较器 dim-aware/参数化 sortKey+dir + 表头可点。
- **#71**:`marked ^18` `gfm:true`(`src/render.ts:9`)按 GFM 规范吃**单波浪号** strikethrough;代码保护(`:165-166`)只护 code、不护散文 `~`。修:覆盖 `del` tokenizer 只认 `~~`(或关删除线)。

## ★共享面账本(种子,诊断后补最终形态)
- **`src/render.ts`** ← #42 + #71 同改此文件(渲染管线)。最终形态:一处协调好的 marked 配置/tokenizer + preprocess,#42 的数学预处理与 #71 的 del tokenizer 覆盖不互踩。**两个渲染 bug 应一起想、避免补丁叠补丁。**
- **`src/tabs.ts` + 远端会话生命周期(`ssh_source.rs`/`tmux_reconcile.rs`/`@ccm_sid` 身份)** ← #43 + #60(+ 可能 #63)同域。最终形态待诊断:resume/attach/变灰/身份匹配是一套一致语义,别各修各的。
- **用量 `usage-view.ts`/`usage-pivot.ts`** ← 仅 #67。
- 独立:#41(Rust 拉前扫描)、#46(历史来源加载)、#63(fork/branching)。

## 收尾(类1 全完成后)
1. **Phase G**:`/full-audit` 一次性全项目验收 + 主计划终账 + 端到端(全量 build/test)。
2. **类2 交付用户真机验证**:#60(B2+F74)、#63②(需每机重装 ccm 助手)、#63③(≥v3.1.1 是否仍复现)、#46 字面部分 → 验过由用户关 issue。
3. **发版**:所有修复都要发一版 app 才在用户机器生效(#41/#72 还需远端 daemon 已是 p1p)。**对外动作、用户拍板**,不在 loop 内。

## 自动模式 / loop 停止条件
- 用户要:masterplan 批准后 `/loop` 自动逐 bug 跑 C→F,全部完再 Phase G。
- **停 loop**:撞审批门禁 / 阻塞 / 计划≠现实 / 同步骤≥2 次失败 / 全部完成(先 Phase G)。**发版**不在 loop 内(用户拍板)。
- masterplan 未批准前**不起 loop**。
