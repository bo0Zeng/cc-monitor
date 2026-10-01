// 生成物 —— 不许手改。由 `tests/panorama-engine/cli_tests.rs::the_frontend_types_are_generated_from_upstream_and_this_program` 写出。
// 要求：「线上契约由上游给、本仓不手抄」。
// ① 上游 code-picture-core 的线上类型（vendored `src/wire.ts` 原样；ts-rs 从上游的 serde 属性写出，可选性随之过来）
// ② 全景小程序自己的应答（`src/panorama-engine/main.rs` 的 DTO，ts-rs）

// ── ① 上游（vendored code-picture-core `src/wire.ts`）──

// 生成物 —— code-picture-core 的线上类型(serde 序列化出来的样子),由 `src/wire_schema.rs` 从
// `#[cfg_attr(test, derive(ts_rs::TS))]` 写出(`cargo test -p code-picture-core wire_schema`)。不许手改。

/**
 * 符号 id 的**结构化拆分** —— 拆法只住这里(`file#段[@行号]`;没有 `#` 的是文件级 id,如 `Imports` 边的端点)。
 *
 * 该带文件的地方直接带上它(`Overview.entry_points` · `symbols_touching`),消费方读字段,
 * 不照着 id 的格式自己拆 —— 格式哪天变了,拆的人会一起静默坏。
 */
export type SymbolRef = { id: string, 
/**
 * 仓库相对路径(`#` 之前)。
 */
file: string, 
/**
 * `#` 之后、去掉 `@行号` 消歧的那一段(`Type::method` / `f`);文件级 id ⇒ `None`。
 * 批注与文档关联按它挂(同名多符号共用一段 —— 与 `annotations_for` / `docs_for` 的比法一致)。
 */
symbol: string | null, };

export type SymKind = "Function" | "Method" | "Class" | "Module";

export type Lang = "Rust" | "Python" | "JavaScript" | "TypeScript" | "Java" | "Kotlin" | "C" | "Cpp" | "CSharp";

export type Symbol = { id: string, name: string, file: string, kind: SymKind, lang: Lang, start_line: number, end_line: number, 
/**
 * F68：符号签名文本（如 `fn foo(a:u32)->String`）——函数定义节点从起始到 body 前的
 * 文本，折单行。拿不到（body 字段非标准 / 非可调用符号）= None。传前端做详情面板展示。
 */
signature?: string | null, 
/**
 * 返回类型的**类型名**(`-> Graph` → `Graph`;`-> Result<Graph, E>` → 取最外层名)。
 * 从 AST 的 `return_type` 字段抽,不是从 `signature` 字符串里解 —— 后者九门形态各异,脆。
 *
 * 用途:`let g = Graph::build(..)` 之后,`g.communities()` 的接收者类型就知道了 ——
 * **这仍是语法可及的,不需要类型推导引擎**(见 `graph::collect_local_types`)。
 */
return_type?: string | null, 
/**
 * 哪几个**形参位**的值会流到返回值,逗号分隔(`"0,2"`)。`None` = 没有 / 算不出。
 *
 * `fn h(a, b) -> T { a }` ⇒ `"0"`。于是 `let y = h(x, z)` 之后 `y` 的来源只有 `x`;
 * 没有这条的话 `z` 也会被算成「可能流到」—— 那是假阳。
 *
 * ⚠ 与 `arg_flow` 同一种口径:**只认语法可及的传递**,被变换过的照样算(宽),
 * 存进结构体再取出的追不到(窄)。
 */
return_flow?: string | null, 
/**
 * **函数摘要**:这个函数把哪个形参的值写进了哪个形参**所指对象**的哪条字段。
 * 逗号分隔的 `i~j:path`(`store(dst, v){ dst.f = v }` ⇒ `"1~0:f"`)。
 *
 * 用途是过程间传播:调用点 `store(&mut s, x)` 见到这条摘要,就能在**那一行**
 * 记下一个定义 `s.f ← x`,于是后面的 `sink(s.f)` 追得到 `x`。
 * 没有它的话,值一旦经由「写进参数」离开函数就断了 —— 而那是最常见的一种传递。
 *
 * 🔴 一律是 `~`(**可能**):写可能在分支里,摘要不记条件。
 * ⚠ 只认**字段写**(`dst.f = v`)。`dst = v`(重赋形参本身)不记 —— 值语义下传不出去;
 * `dst.push(v)`(方法可能改)也不记 —— 那要知道 `push` 干了什么,是猜。
 */
param_flow?: string | null, };

export type LineRange = { start: number, end: number, };

/**
 * 人挂在代码上的锚点。Phase 1 只用文件/符号级;块级字段留占位(F09 才实现解析)。
 */
export type Anchor = { file: string, symbol: string, node_path: string | null, quote: string | null, prefix: string | null, suffix: string | null, };

export type AnchorState = "Resolved" | "MovedFileRenamed" | "MovedToOtherFile" | "Ambiguous" | "Orphaned";

export type Location = { file: string, line: number, };

export type Resolution = { state: AnchorState, location: Location | null, content_changed: boolean | null, candidates: Array<Location>, };

export type IndexStats = { files: number, symbols: number, 
/**
 * 识别为调用点但被调不在符号表(外部/stdlib/漏抓)→ 未建成边的调用点数。
 */
unresolved_calls: number, 
/**
 * **歧义**调用点数:仓内有同名候选但分不出是哪个(含全部方法调用)→ 未建成 `Calls` 边。
 * 与 `unresolved_calls`(仓内一个同名都没有)是两回事:那是「看不见」,这是「看得见但分不清」。
 * 🔴 这个数**大**是常态 —— 静态层对方法调用的接收者类型无从判定,那是编译器的活。
 */
ambiguous_calls: number, 
/**
 * 识别为 import 但目标不在仓内文件表(外部包/stdlib/本门语言解析不到文件)
 * → 未建成边的 import 条数。与 `unresolved_calls` **分开计** —— 两者成因不同,
 * 合成一个数会让「这张图漏了多少」说不清是漏在调用还是漏在依赖。
 */
unresolved_imports: number, 
/**
 * 解析报错**且未产出任何符号**(硬失败)的源文件数——只计真失败,滤掉 grammar 底噪。
 */
parse_errors: number, };

/**
 * 建索引走到哪了(`Engine::index_with_progress` 的回调参数)。
 *
 * 每一阶段先报一次 `done = 0`(说出这一阶段一共几份),之后每做完一份报一次;阶段按 [`IndexPhase`] 的顺序走。
 * 🔴 份数是**这一阶段**的,不是全程的 —— 各阶段每份的耗时差很多(解析远重于建边),合成一个百分比会骗人。
 */
export type IndexProgress = { phase: IndexPhase, 
/**
 * 这一阶段做完了几份(阶段内单调增)。
 */
done: number, 
/**
 * 这一阶段一共几份(阶段内不变)。
 */
total: number, };

/**
 * 建索引的阶段(按这个顺序走)。
 */
export type IndexPhase = "Parse" | "Link" | "Relink" | "Docs";

export type IndexDelta = { updated_files: number, added: number, removed: number, };

export type EdgeKind = "Calls" | "Imports" | "AmbiguousCall";

/**
 * 边的可信度(F10):静态调用图对动态语言不可判定,一律标注而非假装 sound。
 */
export type Confidence = "Exact" | "Dispatch" | "Heuristic" | "DynamicGuess";

export type Edge = { from: string, to: string, kind: EdgeKind, call_site_line: number | null, confidence: Confidence, 
/**
 * `AmbiguousCall` 专用:这个调用点在仓内有几个同名候选。
 * `None` = 不适用(`Calls` / `Imports`)。
 * **消费方要把它印出来** —— 「N 选 1」这件事不写在脸上,这条边就又变回一句假话。
 */
candidates?: number | null, 
/**
 * **参数流**:这条调用边上,调用方的第 i 个形参被原样传给了被调的第 j 个形参。
 * 形如 `"0>1,2>0"`(调用方参数位 `>` 被调参数位),按 i 升序。`None` = 没有这种传递。
 *
 * 只记**实参是个裸标识符、且它正好是调用方的形参**这一种 —— 那是纯语法能确定的。
 * `f(a.b)` / `f(g(x))` / 字面量一律不记。
 *
 * ⚠ 同一对 (调用方, 被调) 若有多个调用点,这里是**并集** —— 边只有一条,
 * 分不出是哪个调用点传的。要精确到调用点得按调用点建边,那是另一种图。
 */
arg_flow?: string | null, };

export type RankedFile = { file: string, score: number, symbols: number, };

export type Subsystem = { label: string, files: Array<string>, size: number, 
/**
 * 成员集合的指纹(成员 id 升序后 FNV-1a,十六进制)。
 *
 * 🔴 **这不是身份,是「变没变」** —— 任何一个成员进出都会让它变。
 * 拿它当对账的配对键,会把「这团动了一下」读成「旧团消失 + 新团出现」。
 * 要配对请用 `anchors`。
 */
member_hash: string, 
/**
 * 按 PageRank 降序取的代表符号(最多 3 个)。两个用途:
 * ① **给人看** —— 一眼知道这团在干嘛,好给它命名;
 * ② **给对账当配对锚** —— 它比 `label` 稳(文件一改名 label 就换人),
 *    也比 `member_hash` 稳(那个按定义就不稳)。
 *
 * ⚠ 诚实边界:社区真分裂时两半会各自认领不同的锚,配对会认成两个新团 ——
 * 那不是配对失灵,那就是真的分裂了。
 */
anchors: Array<string>, 
/**
 * 团**内部**的调用边数(内聚的原始计数)。
 */
internal_edges: number, 
/**
 * 恰有一端在本团的调用边数(耦合的原始计数;同一条跨团边在两个团各记一次)。
 *
 * ⚠ 与 `internal_edges` 一样**只是计数,不是分数** —— 见 `rank::Graph::community_cut`。
 * 另:两个数都只算 `Calls` 边(社区本来就是从调用图聚出来的),不含 `Imports`。
 */
external_edges: number, };

export type Overview = { spine_files: Array<RankedFile>, subsystems: Array<Subsystem>, 
/**
 * 入口点(零入边、按 PageRank 排)。带 `file`:消费方按文件画的时候读字段,不拆 id。
 */
entry_points: Array<SymbolRef>, total_symbols: number, total_files: number, unresolved_calls: number, ambiguous_calls: number, unresolved_imports: number, parse_errors: number, 
/**
 * **读库出错的记录**(去重)。非空 = 这份全景**不完整**,不是「这仓就这样」。
 *
 * 🔴 为什么要有这条:查询接口返回 `Vec` 而不是 `Result`(调用方要的就是一张表),
 * 出错时退成空表 —— 而「出错」与「确实没有」长得一模一样。
 * 真实事故:`all_edges` 少查一列 ⇒ 全景**变空但不报错**。
 */
db_errors?: Array<string>, };

export type AffectedSymbol = { id: string, depth: number, };

export type ImpactSet = { root: string, affected: Array<AffectedSymbol>, };

/**
 * 一个基本块:一段**一旦进入就会从头跑到尾**的语句序列。
 */
export type CfgBlock = { id: number, 
/**
 * 块内第一/最后一条语句的行号。空块(纯汇合点)两者相等。
 */
start_line: number, end_line: number, 
/**
 * 这一块里的调用点(被调裸名, 行号)。裸名 —— 解析到哪个符号是调用图那边的事。
 */
calls: Array<[string, number]>, 
/**
 * 这一块里的**局部变量定义点**,按行号升序。给到达定义分析用。
 */
defs: Array<CfgDef>, };

/**
 * 一个局部变量的定义点:`let y = h(x) + 1;` ⇒ `{ var: y, line, sources: [h, x] }`。
 *
 * `sources` 是**这一次赋值**提到的标识符 —— 与流不敏感那张表的区别正在于此:
 * 那张表把一个变量的**所有**赋值混成一堆,这里一次赋值一条,由到达定义决定哪条算数。
 */
export type CfgDef = { var: string, line: number, 
/**
 * 源文件里的**字节偏移** —— 定序用。
 *
 * 🔴 行号排不了同一行内的先后:`let s = mk(x); sink(s.f);` 写在一行时,
 * 「定义在使用之前」这件事按行号判是 `4 < 4` = 假 ⇒ 整条链断掉。
 * 字节偏移没这个问题。`line` 留着给人看。
 */
at: number, sources: Array<string>, };

/**
 * 一个函数体的控制流图。
 *
 * 🔴 **`unsupported` 非空时整张图作废,不许用。** 这是本模块最要紧的一条:
 * 一张漏了某条边的 CFG,会把「这两个调用互斥」这种结论变成**假事实** ——
 * 而假事实比没有答案坏得多。遇到不建模的控制结构就整份弃掉,如实说是哪个。
 */
export type Cfg = { function: string, blocks: Array<CfgBlock>, 
/**
 * 有向边 (from_block, to_block)
 */
edges: Array<[number, number]>, 
/**
 * 入口块 id;出口块恒为最后一个
 */
entry: number, exit: number, 
/**
 * 🔴 非空 = 这张图**不可用**,值是踩到的那个构造名。
 */
unsupported: string | null, };

/**
 * 值流到的一个落点:某函数的某个形参位。
 */
export type FlowStep = { id: string, 
/**
 * 形参位(0 基;**不含**接收者形参)
 */
param: number, 
/**
 * 从起点算起的跳数
 */
depth: number, 
/**
 * 🔴 `false` = **确定**原样传下去(实参就是那个形参,或它的借用);
 * `true` = **可能**流到 —— 值被变换过或经中间变量,由**流不敏感**的过程内分析得出。
 *
 * 流不敏感意味着**过度近似**:`y = a; g(y); y = b; g(y)` 会算成 a 和 b 都流到两处。
 * 链上只要有一跳是 `true`,整条结论就只能当「可能」读。
 */
derived: boolean, };

/**
 * 「这个参数流到哪去了」的结果。
 *
 * 🔴 **它只追「原样传下去」这一种流动** —— 实参必须是个裸标识符且正好是当前形参。
 * 被变换过(`g(x + 1)` / `g(x.field)` / `g(h(x))`)、被存进结构体再取出来、
 * 经闭包捕获的,**一律追不到**。那些要函数体内的定义-使用链(第三档)。
 */
export type FlowSet = { root: string, param: number, steps: Array<FlowStep>, 
/**
 * 触到深度上限。**截断了必须说**,否则「没再往下」会被读成「到头了」。
 */
truncated: boolean, };

/**
 * 一条调用路径:逐跳的边。`steps[0].from` = 起点,`steps.last().to` = 终点。
 *
 * 🔴 **这是「调用关系上存在一条链」,不是「执行轨迹」。** 图里没有分支、循环、
 * 提前返回、条件,也没有先后。同一函数体内多个调用点按行号排只是**书写顺序**,
 * 不是运行顺序 —— 别把它读成 trace。
 */
export type CallPath = { steps: Array<Edge>, };

export type PathSet = { from: string, to: string, paths: Array<CallPath>, 
/**
 * 因条数或深度上限被截断。**截断了必须说** ——
 * 不说,读者会把「我只找了这么多」读成「一共就这么多」。
 */
truncated: boolean, 
/**
 * 搜索过程中遇到并切断的环数(递归 / 互递归)。0 = 没遇到。
 */
cycles_cut: number, };

export type LinkSource = "Colocation" | "Frontmatter" | "Inline";

export type DocLink = { doc_path: string, target_file: string, target_symbol: string | null, source: LinkSource, };

export type DriftItem = { doc_path: string, target_file: string, target_symbol: string | null, reason: string, };

export type AnnotationStatus = "Active" | "Proposed";

export type Annotation = { id: string, file: string, symbol: string | null, body: string, author: string, status: AnnotationStatus, 
/**
 * **谁写的**(2026-09-24)—— 与 `status`(审没审)是两件事,批准只改 `status` 不改它。
 *
 * 🔴 为什么要有:批准之后「人写的」与「agent 提议、人点了批准的」都是 `Active`,
 * 此前只剩 `author` 一个自由文本能分 —— 那是约定,不是数据。而人那一侧的价值是
 * 「人指出哪里不对」:agent 读到**自己**提的话、却当成人的指示,是这一族最贵的错。
 * 旧侧车文件没有这个字段 ⇒ `Unrecorded`(不猜)。
 */
origin: AnnotationOrigin, };

/**
 * 批注的来源。
 */
export type AnnotationOrigin = "Human" | "Agent" | "Unrecorded";

/**
 * 单个符号的完整视图:符号本体 + 直接调用者/被调 + 关联文档 + 批注。
 * 由 `Engine::node` 组装(此前散在 MCP 层现拼,cc-monitor 得自己组合;F15 收口)。
 */
export type NodeView = { symbol: Symbol, callers: Array<Edge>, callees: Array<Edge>, docs: Array<DocLink>, annotations: Array<Annotation>, };

/**
 * 以某符号为心的邻域调用子图(双向、可控深度):节点集 + 边集。
 */
export type SubGraph = { symbols: Array<Symbol>, edges: Array<Edge>, };

/**
 * 以某符号为心的邻域,**每个够得着的符号带距根几跳**(`Engine::neighborhood`)。
 *
 * 与 [`SubGraph`] 同一个邻域(同一个 depth 口径),只是把「第几跳」这个量直接给出来 ——
 * 消费方要按跳数分层画图时不必自己再走一遍图(那等于在消费方那一侧算图)。
 */
export type Neighborhood = { root: string, 
/**
 * 根不在里面。depth 升序、同层 id 升序。
 */
reached: Array<Reached>, };

/**
 * 邻域里的一个符号与它距根几跳(1 = 直接调它 / 它直接调)。
 */
export type Reached = { id: string, depth: number, };

/**
 * 一处要落盘的改动。**只描述,不执行**。
 */
export type FileEdit = { 
/**
 * 仓相对路径,`/` 分隔。
 */
rel: string, 
/**
 * 算的时候盘上是什么(`None` = 不存在)。写者拿它当 CAS 期望:盘上已经不是它 ⇒ 别写、重算。
 */
before: string | null, 
/**
 * 要变成什么(`None` = 这份文件该不存在 = 删)。
 */
after: string | null, 
/**
 * 写的时候要不要建父目录(批注目录首写才建 —— 开面板 / 只读查询绝不建它)。
 */
parents: boolean, };

/**
 * 一次计划:原方法的返回值 + 要落盘的那一处(`None` = 盘上已经是想要的样子,什么都不用写)。
 *
 * 🔴 `edit` 为 `Some` 时 `before != after` 恒成立(构造处归一),写者不用自己再比一遍。
 */
export type Planned<T> = { value: T, edit: FileEdit | null, };

export type ArchNode = { 
/**
 * Mermaid 里的 id。**从标签派生、人能读**(`lang_kotlin`)——
 * `s0`/`s1` 那种不透明 id 逼着读图的人在边与节点定义之间来回翻。
 */
id: string, 
/**
 * 代表文件(成员最多的那个)。
 */
label: string, size: number, files: number, 
/**
 * 代表符号(给人一眼看懂这团在干嘛)。
 */
anchors: Array<string>, 
/**
 * **成员文件**(仓库相对,升序)。给消费方**下钻**用:点一个团/模块 → 列它有哪些文件。
 * ⚠ 与 `files`(个数)同源,`member_files.len() == files`。
 */
member_files: Array<string>, };

export type ArchLink = { from: string, to: string, 
/**
 * 确定的边条数。
 */
exact: number, 
/**
 * 动态派发(候选集完整,运行时才知道哪个)。
 */
dispatch: number, 
/**
 * 按名字凑的候选 —— `Heuristic` + `DynamicGuess`。
 */
guess: number, };

/**
 * 图种。**加一种图 = 在这里加一个变体 + `ALL` + `info()`**,别处不用写它的名字。
 */
export type DiagramKind = string;

/**
 * 一张图画出来的**形状**。消费方按它选渲染器 —— 不按图种。
 */
export type DiagramShape = "clusters" | "call_graph" | "type_graph";

/**
 * 一张图认的**输入**。id 与 [`DiagramRequest`] 的字段名**逐字相同**(有测试钉两向相等)。
 */
export type DiagramParam = "symbol" | "depth" | "max_nodes" | "certain_only" | "exclude_tests";

/**
 * 一种图的元数据 —— [`kinds`] 原样吐给第三方。
 */
export type DiagramKindInfo = { 
/**
 * 线上是 id(`"module"`)。
 */
id: DiagramKind, 
/**
 * 人读名。
 */
title: string, summary: string, 
/**
 * 这张图**认**哪些输入。不在表里的输入会被忽略(不报错),所以消费方只该给人拧这几个。
 */
params: Array<string>, shape: string, };

/**
 * 一次画图请求。**一个类型喂所有图**;全部字段可缺,缺了取默认。
 * 字段名与 [`DiagramParam::id`] 逐字相同。
 */
export type DiagramRequest = { symbol?: string | null, depth?: number | null, max_nodes?: number | null, certain_only?: boolean | null, exclude_tests?: boolean | null, };

/**
 * 画图失败。🔴 每一种都是**明说**的失败,没有一种会回落成「画一张别的图」。
 */
export type DiagramError = { "error": "unknown_kind", given: string, known: Array<string>, } | { "error": "needs_symbol", kind: DiagramKind, } | { "error": "symbol_not_found", symbol: string, };

/**
 * 一张画好的图。
 */
export type Diagram = { kind: DiagramKind, honesty: Honesty, body: DiagramBody, };

/**
 * 按**形状**分的图体。变体集合与 [`DiagramShape::ALL`] 一一对应(有测试钉)。
 */
export type DiagramBody = { "shape": "clusters", nodes: Array<ArchNode>, links: Array<ArchLink>, } | { "shape": "call_graph", center: string, depth: number, nodes: Array<CallNode>, edges: Array<CallEdge>, } | { "shape": "type_graph", types: Array<TypeNode>, relations: Array<TypeRelation>, };

/**
 * 调用子图的一个节点(符号的**画图所需**那几样,不是整个 `Symbol`)。
 */
export type CallNode = { id: string, name: string, file: string, kind: SymKind, start_line: number, };

/**
 * 调用子图的一条边。一条就是一条,不聚合 —— 所以只有一个可信度。
 */
export type CallEdge = { from: string, to: string, confidence: Confidence, 
/**
 * 歧义边:这个调用点在仓内有几个同名候选(「N 选 1」要印出来)。
 */
candidates: number | null, call_site_line: number | null, };

/**
 * **公共诚实信号** —— 每种图都带。
 *
 * 🔴 `None` 与 `Some(0)` 是两回事:`Some(0)` = 量了、没有;`None` = **这张图不量这个**
 * (类图不画调用 ⇒ 调用那两格是 `None`)。把 `None` 写成 0 会让读者以为「一处都没漏」。
 *
 * 格子怎么读:
 * * `unresolved_calls` —— **看不见**:识别为调用但连不上仓内符号(仓外 / 漏抓)。全仓读数。
 * * `ambiguous_calls` —— **分不清**:仓内有同名候选但钉不死是哪个。全仓读数。
 * * `filtered_guess_links` —— **滤掉**:全靠名字凑、一条确定的都没有而没画的连接(本图)。
 * * `excluded_test_symbols` —— **排除**:因为是测试而没画的符号(本图)。
 * * `omitted` —— **省略**:因为节点上限而没画的节点 / 符号 / 连接(本图)。
 * * `db_errors` —— **读库出错**:非空 = 这张图不完整。
 */
export type Honesty = { unresolved_calls: number | null, ambiguous_calls: number | null, filtered_guess_links: number | null, excluded_test_symbols: number | null, omitted: Omitted | null, db_errors: Array<string>, };

/**
 * 因为节点上限而没画的量。
 */
export type Omitted = { nodes: number, symbols: number, links: number, };

/**
 * 一个类型。
 */
export type TypeNode = { 
/**
 * 图里的 id(Mermaid 能吃的字符;由名字派生,人能读)。
 */
id: string, 
/**
 * 声明里的类型名。
 */
name: string, 
/**
 * 若仓内有同名的**类型符号**(`SymKind::Class`),这是它的 id —— 给下钻用。
 * `None` = 这个类型只从方法限定名 / 字段 / 实现关系里认出来,没有独立符号。
 */
symbol: string | null, 
/**
 * 全部字段(按名升序;类型在仓外的也列 —— 它是这个类的组成)。
 */
fields: Array<TypeField>, 
/**
 * 全部方法(按名、id 升序),连符号 id。
 */
methods: Array<TypeMethod>, };

export type TypeField = { name: string, ty: string, };

export type TypeMethod = { name: string, symbol: string, };

/**
 * 类型之间的一条关系。`from`/`to` 是 [`TypeNode::id`]。
 */
export type TypeRelation = { from: string, to: string, kind: TypeRelKind, 
/**
 * 组合关系的字段名;实现关系为 `None`。
 */
label: string | null, };

/**
 * 关系种类 —— 都来自声明。
 */
export type TypeRelKind = "implements" | "composes";

// ── ② 全景小程序自己的应答 ──

/**
 * 本程序**自己的**应答形状（引擎直出之外的那几样）：`status` 那三格。
 * `stale` = 源文件有改动、索引已陈旧；`indexedAt` = 上次索引的 unix 秒（`null` = 从未建完）；`symbols` = 已索引符号数。
 * 它与下面那份的 TS 声明进前端生成物 `types.ts`（ts-rs，只在测试构建里派生）。
 */
export type PanoramaStatus = { stale: boolean, indexedAt: number | null, symbols: number, };

/**
 * `diagram` 的应答：上游 `Diagram`（形状归 vendored pin）＋ 上游 Mermaid 文本（复制给 agent 与「画不出」兜底用，不许解析它）。
 */
export type PanoramaDiagram = { diagram: Diagram, mermaid: string, };

