# F-render(#71 单波浪号删除线 + #42 多行/奇数 $$ LaTeX)

> 类1 feature 2。同改 `src/render.ts`(共享面),两处独立:#71 = 覆盖 marked `del` tokenizer;#42 = `preprocessMath` 的 `$$` 配对。

## DoD / 验收
- **#71**:覆盖内建 `del` tokenizer 只认 `~~x~~`;单 `~` 返回 `undefined`(不能 `false`——marked `use()` 只在 `===false` 回退内建、会复活单 `~`)。`~/foo~/bar` 不再被划、`~~text~~` 仍渲染。
- **#42**:块级 `$$` 配对改「行边界规则」(开=行首或后接换行;闭=行尾或前接换行)+ 入口 CRLF→LF 归一 + 去 `[^$]` 守卫。散文里游离/未配对/元讨论 `$$` 不被当定界符 → 不再吞后一个真公式。
- 验证:tsc 干净;render vitest 22 测(14 F73 + 8 新,**区分性**);全套 333 测过;ReDoS 安全。

## Phase D 审计(2 视角)+ 处置(重要项已修 → 复验)
- **审计 1(正确性+边界)**:#71 SOLID(marked 语义确认、无 ReDoS、`~~` 保留)。**#42 首版(line-anchored「须行首/行尾」)有 3 个重要回归**:① CRLF 行尾块不渲染;② 开定界前有字(`文字：$$⏎…`)不渲染;③ 闭定界后有标点(`$$。` 中文极常见)不渲染。建议:return-undefined footgun(已在注释强标)。
- **审计 2(回归+测试质量)**:无阻塞/无回归/无 scope creep;但**重要:两个头号新测不区分修没修**——#71 例 `~/.claude … ~/.codex` 因空格 flanking stock marked 本就不划(选了非触发变体);#42 元讨论测在旧 buggy 正则下也过(count=2 且"包裹"进 annotation)。#71 注释叙述也不准。
- **处置(修 + 复验)**:
  - #42 首版正则**重写**为「行边界规则」(开=行首|后接换行;闭=行尾|前接换行)+ CRLF 归一 → 修掉 3 个回归。**离线 harness 实证全 8 例通过**(F73 块/单行/glued + 重要1/2/3 + 元讨论bug 2块 + 行内不吞)后才落码。
  - 测试改**区分性**:#71 用 `见 ~/foo~/bar`(闭 `~` 贴非空白、stock 会划)+ `a ~foo~ b`;#42 在 **preprocess 层**断言(散文 `用 $$ 包裹显示公式。` 原样保留、第三块 `$$\ne=f\ng=h\n$$` 成块)——旧正则两者皆失败(**DOMPurify 剥离 KaTeX annotation**,故不在渲染后 HTML 验区分性)+ 补 重要1/2/3 回归守卫测。
  - #71 注释更正(真触发是闭 `~` 贴非空白如 `~/foo~/bar`,`~/.claude … ~/.codex` 反而不触发)+ 强标 return-undefined footgun。
  - 建议(多波浪号 `~~~` 中行 cosmetic 偏差)→ 无害、不动。

## Phase E 工程审计(主线程对账)
- 共享面 `src/render.ts`:#71(inline tokenizer)与 #42(pre-parse 字符串变换)**独立无共享状态**,一次做完、不互踩(账本最终形态达成)。
- 全 markdown 路径统一走 `renderMarkdown`→`preprocessMath`(cards/interactive/tabs);`cards/diff.ts` 不走、不受影响。无 `~`/`$$` 的消息字节不变。全套 333 测过 → 无跨文件污染(全局 del 覆盖安全)。

## 签收:代码审计[x](重要项已修+复验) 工程审计[x] 主计划已更新[x]
