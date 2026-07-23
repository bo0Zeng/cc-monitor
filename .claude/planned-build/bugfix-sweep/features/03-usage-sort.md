# F-usage-sort(#67 用量视图排序混乱)

> 类1 feature 3。症状:所有分组都按「等效∑」降序、表头不可点、选了「按天」日期仍乱跳。

## DoD / 验收
- 「按天」默认**按日期降序**(最近在上);切维度时排序重置为该维度默认(其余维度=等效∑降序,保 F88d-fix 立意)。
- **表头全部可点**:点列名换排序列、再点切升/降序;**▼/▲ 只挂当前排序列**(原来死写在「等效∑」上)。
- 排序抽成纯函数 `sortPivotRows` / `defaultSortForDim`(node 可断言);`pivotUsage` 不动(保其 9 个既有测)。
- 验证:tsc + 全套 `npm test`(tsx 套件 + vitest)。

## Phase D 审计(2 视角)+ 处置
两审均**无阻塞**,且都在 jsdom 实证三个症状确已修复、列数 8/8/8 无错位。收敛出的重要项**全部已修**:
1. **首列排的是隐藏 `key`、显示的却是 `label`**(project 维=全路径、session 维=uuid)→ 点「按项目」得到按路径+大小写敏感的名字序,**重现 #67 同类观感**;且字符串比较用 UTF-16 码元序(大写全排小写前、中文垫底)。→ `sortValue` 的 key 分支改返回 **`label`** + 字符串比较改 **`localeCompare`**(ISO 日期串仍是日历序)。**补了视图层区分性测**(按项目升序须得 `Alpha, Zebra`,按 key 则会得 `Zebra, Alpha`)。
2. **视图层零测试**——3 个纯函数测钉不住"视图有没有真接上"(删掉 addEventListener / 把 ▼ 写死,仍全绿)。→ `usage-view.vitest.ts` **补 6 条 DOM 测**:默认按天日期降序 + 指示符只挂一列、点「合计」▼ 迁移、同列再点 ▲ 且行序反转、切维度重置默认、点已激活维度**不**抹排序、按项目首列按 label 排。
3. **流式重渲弹回顶部**:`renderList` 每帧 `replaceChildren`,表头可点后用户更可能在扫描窗口操作本表。→ 存/还原 `listEl.scrollTop`(照 session-viewer 既有范式)。
4. feature 计划文件缺失 → 即本文件。

已采纳的建议:表头 handler **实时读 `this.sort`**(不捕获渲染期 `active`,消除未来"点了没反应"的脆弱性);点**已激活**维度按钮**早退**(不静默抹掉用户排序);`sort` 初值由 `defaultSortForDim(this.dim)` 推导(去重复);**平手回退跟随 sign**(否则主键全平手时 asc/desc 渲染完全相同、箭头却翻了)——**注意此处我第一版把符号写反了**(写成 `sign*(equiv_b-equiv_a)`,desc 反而把小的排上),已改为与主键同构的 `sign*(equiv_a-equiv_b)` 并**加测锁死**(asc 必须是 desc 的逆序 + desc 平手时等效∑大的在上);hover 底色 `var(--bg-3,…)` 是 no-op(`--bg-3` 全仓未定义、回退值等于表头自身底色)→ 改仓内既有 `var(--overlay-hover)`;表头**键盘可达**(role=button + tabindex + Enter/Space + aria-sort,照 agents-panel 既有做法);合计行首格补 `usage-key` 类(修既存右对齐瑕疵);比较器加 `Number.isFinite` NaN 防御。

未采纳(记录):NaN 深层加固(pre-existing、不可达)、排序偏好持久化(计划未要求)、▼/▲ 进 textContent 导致的列宽微跳(可用定宽指示符,低优先)。

## Phase E 工程审计(主线程对账)
- **共享面账本更新**:本 feature 把 **`src/styles.css`** 纳入改动面(原账本写「#67 独立无重叠」)。下一个 feature **F-fork-badge(#63①)** 大概率也要改 `styles.css`(tab 血缘徽标)→ 已在 MASTERPLAN 账本补记,届时按最终形态协调、勿各写各的。
- 唯一消费者是 `usage-view.ts`(全仓 grep `pivotUsage|sortPivotRows|defaultSortForDim` 仅命中 pivot/view/test 三处);HUD(`usage-hud.ts`)只用 `pricing`,不碰 pivot → 无外溢。
- 双重排序(pivotUsage 底座 + 视图再排)经论证安全:Array#sort 稳定,底座实际充当确定性三级键;已在注释记明,勿"优化"掉底座排序。

## 签收:代码审计[x](重要项全修+复验) 工程审计[x] 主计划已更新[x]
