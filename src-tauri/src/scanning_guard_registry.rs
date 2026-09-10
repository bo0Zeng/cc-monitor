//! **扫描型判据的自匹配元判据**〔audit-0805 F23，F+#3 开〕。
//!
//! # 它治的不是一个 bug，是一个**族**
//!
//! 症状永远一样：**判据在自己的登记表 / 注释 / 常量里找到了自己要找的东西 ⇒ 恒绿**。
//! audit-0805 实测五次：
//!
//! | 何处 | 判据在自己的什么东西里找到了自己 |
//! |---|---|
//! | F12 | 跨语言对拍匹配到自己的**注释** |
//! | F13 | 原子替换登记表匹配到自己的 `RULE` **常量**（写着两个符号的调用示例） |
//! | F18 | 文档副本判据匹配到自己的**登记表**（`HAS_A_GUARD` 里写着那些判据名） |
//! | F05 | 函数体抽取的**锚点**命中了自己 `PHASES` 表里的字符串 |
//! | F05 | 棘轮 `matches()` **数到自己**：6 vs 真实 4 |
//!
//! ★ **五次没有一次是被「判据变红」发现的** —— 四次靠变异、一次靠 clippy。
//! 这一类缺陷的**默认结局是恒绿**，所以「以后小心点」不是修法。
//!
//! # 修法：让它写不出来，而不是再检测一遍
//!
//! `guard_core::scan_tree!` 按构造摘除调用者自己那一份（用 `file!()`，调用方改不错）。
//! 本模块要求：**测试段里不许再出现裸的目录遍历** —— 要么走 `scan_tree!`，
//! 要么在下面这张存量清单里，而清单**只许变短**。
//!
//! # ★ 存量已**逐条判过真伪**（08-06 第二刀）
//!
//! 判准是**「它靠什么读不到自己」**，四类穷尽（下面 `PENDING` 每一行都属其一）：
//!
//! | 类 | 数 | 它凭什么安全 |
//! |---|---|---|
//! | **生产 / 夹具 IO** | 5 | 遍历的根本不是源码树（`.claude/projects` 的 jsonl · 标注池 · tempdir · `.ssh` · PowerShell profile）。**自匹配这个概念对它们不成立** |
//! | **剥生产段（构造性摘除）** | 21 | 只扫 `production_code`/`production_source` 的产物；判据自己住在 `#[cfg(test)]` 里 ⇒ **按构造读不到自己** |
//! | **显式摘除自身** | 2 | `SELF` 常量 / `replace(&own, "")`（`atomic_replace_registry` · `doc_copy_registry`） |
//! | **扫的树不含自己** | 2 | `frame_cadence_guard` 只扫 `.md`（自己是 `.rs`）· `shared_crate_registry` 扫 `crates/` 与 `Cargo.toml`（自己在 `src-tauri/src`） |
//!
//! ⇒ **30 个存量里没有一个是「会自匹配却没防住」的**。它们不是 30 个待修的 bug，
//! 是 30 个**已分类的、各有安全理由的**遍历。棘轮继续挡**新增**，而不再暗示这里有一堆债。
//!
//! ⚠ 第二刀唯一动过的一个是 `parity_ledger.rs`：它原来靠**写死文件名**跳过自己
//! （`== Some("parity_ledger.rs")`）—— 那种摘除**改名即静默失效**，而失效后看起来和没失效
//! 一模一样（`scan_tree!` 头注逐字警告过这个形态）。已换成 `scan_tree!`（按 `file!()` 摘除）。
//! ★ **但要如实说**：把那个摘除关掉，**没有任何判据变红** —— 真正挡住它自匹配的是
//! 另一招（`attr` 运行时拼 `format!("#[tauri::{}]", "command")`，于是字面量不在自己源码里）。
//! ⇒ 这一改**去掉的是一个改名即失效的形态，不是修了一个活缺陷**。别把它读成后者。
//!
//! 本条的契约仍是两句：**新增的不许出现；存量只许降。**
//!
//! # ★ 登记一个**第五形**（`K-R31` `D2`，09-06）—— 只登记，不加判据
//!
//! 上面那张四类表说的是「这份遍历**凭什么读不到自己**」。`K-R31` 新立的那条判据
//! （`local_backend.rs::nothing_in_the_production_path_runs_code_between_fork_and_exec`）
//! 走的是一个上面没有的形状：**`scan_tree!` 摘掉自己那份之后，
//! 又用 `include_str` 那个宏把自己那一份显式加回来**。
//!
//! 为什么它非这么写不可：那条判据扫的是「生产段里有没有 `pre_exec`」，
//! 而**最可能长出那种写法的恰恰是它自己所在的文件**（起进程那一跳就在那儿）——
//! 按构造摘掉自己 = 在最该看的那一份上瞎掉。
//! 它挡住自匹配靠的是**第二类**（剥生产段）：形态表住 `#[cfg(test)]`、被守的那句话住 `///`，
//! 逐份过 `guard_core::production_code` 之后**按构造都不在扫描面里**，
//! 并配了一条**阴性对照**（同一段包进注释 ⇒ 必须不命中）把「剥法真的在跑」钉住。
//!
//! ⚠ **这一段只是登记，不是判据**，而且**这一形有几处我没数出来** —— 说清查了哪条路：
//! 09-06 现打的是一把**文件级**的尺子（`scan_tree!` 与「取自己那一份的 `include_str`」
//! 同现于一份文件 ⇒ **16 份**），而事实的单位是「**同一条判据里**摘掉自己又加回来」——
//! 尺子比事实粗，那 16 份里多半是「两条不同的判据各用各的」。⇒ **那个数没人量过。**
//! 而这一形一旦被复制而**没带剥法**，本模块**一个字都看不见**。
//! **别把这句读成「已经守住了」，也别读成「全仓就这一处」。**
//!
//! # 🔴 `K-R33`（09-06）—— 上面那一段登记完的**第二天**就发现：它根本没被判到
//!
//! `every_registry_guard_keeps_its_reverse_half` 靠 `TABLE_DECLS` **按名字**认表，
//! 而 `K-R31` 那条判据的表叫 `FORMS` —— 名字不在那个闭集里 ⇒ 它**恒真地过**。
//!
//! 🔴 **要紧的不是漏了一条，是「过了」与「它没扫到你」在输出上一模一样**（都是静默的绿）。
//! ★ 这正是本模块头注治的那一族的**镜像**：那一族是「判据在自己的登记表里找到了自己」，
//! 这一格是「**登记表根本没去找它**」。两边的默认结局都是恒绿。
//!
//! ## 走的是「闭集 ＋ 点名钉住」，不是「按形状认」—— 为什么
//!
//! 两条路，实测之后选了前者：
//!
//! - **按形状认**（口径改成「测试段里既有 `const … : &[…]`、又有一处树遍历」）：
//!   09-06 现打，人群从 8 涨到 33，而其中**两份当场红** ——
//!   `exec_site_registry.rs` 真的有反向那半，只是措辞是「已经不存在」而 `REVERSE` 只认另两句
//!   （⇒ 要落地它，还得**再放宽一次** `REVERSE`）；
//!   `tmux_daemon_gate_guard.rs` 压根没有登记表，它那几张全是 needle 表（⇒ 那是**误采**）。
//!   ⇒ 这条路要么连着放宽两处，要么配一张豁免清单，两样都是在拆这条守卫。
//! - **闭集 ＋ 点名钉住**（今天这一条）：`TABLE_DECLS` 里加一个名字，人群 8 → 9，
//!   新进来的**恰好**是被漏掉的那一份（`const FORMS:` 全树现打**只有一处**）。
//!
//! ⚠ **代价是真的，写在这儿别让下一个人重新发现**：闭集按名字认 ⇒
//! **改名即静默失效**，而本模块前面那一段（`parity_ledger`）逐字警告过这个形态。
//! ⇒ 对价是 `every_registry_guard_keeps_its_reverse_half` 里那条 `MUST_BE_RECOGNISED`：
//! 它拿**真实住址**把这一形钉住，于是「表改了名 ⇒ 掉出人群」从**静默**变成**当场红并点名**。
//! ★ 但它只钉住**被点名的那几份**：别的判据改表名，本模块仍然看不见。
//!
//! ## 📌 纪律（这一条是本节存在的理由，不是附注）
//!
//! **新写一条「扫描面 ＋ 常量表」型的判据，那张表要起成 `TABLE_DECLS` 里已有的名字之一。**
//! 起了别的名字 ⇒ 这条元判据看不见你，而你会以为它在守着 —— 那就是 `K-R31` 那一轮的实况。
//! 非要用新名字的话：往 `TABLE_DECLS` 里加，**并且同拍往 `MUST_BE_RECOGNISED` 里加一行**
//! （只加前者的话，下次谁把名字改回去就又是静默的绿）。
//!
//! ## ⚠ 它**没有**买到什么（`K-R33` 只治一形）
//!
//! - **粒度是「文件」，不是「那条判据」**：一份文件只要测试段里有任意一处 `REVERSE` 形态就算过。
//!   `local_backend.rs` 的测试段里另有 **27 处** `assert_eq!(`
//!（09-06 现打：全文 28 处、全部落在测试段内，其中只有 1 处是 `K-R31` 的阳性对照）
//!   ⇒ **把 `K-R31` 那条判据的阳性对照整个删掉，本条照样绿**（09-06 变异实打，读数在件文件里）。
//!   本条买到的是「它进了人群、数得着」，**不是**「它的反向那半被逐条守着」。
//! - **射程只有 `src-tauri/src` 一棵树**（`scan_tree!` 的实参就写在那儿）：
//!   `remote-daemon-proto/src` 里的登记表这条元判据一份也没看。
//!   🔴 **这一条 `K-R37`（09-06）已经治了** —— 今天的实参是**两棵树**，
//!   逐条读数与「还差哪几棵」见下面 `K-R37` 那一节。**这一行留着是账，不是现状。**
//! - **`D1②` 现打的真数**（09-06，分母 = 三棵树 `*_registry.rs`/`*_guard.rs` 共 47 份、
//!   剔掉本文件自己）：那 138 条 `const X: &[` 里**逐条读过**，真的是「一条判据自带的登记表」
//!   的有 63 条，而今天认得出的只有 8 条 —— 其中 20 条住在 daemon 树（射程之外，另一族病）。
//!   **剩下的这一族本件刻意不治**，读数与逐条判词落在 `K-R33` 件文件里。
//!
//! # 🔴 `K-R36`（09-06）—— 上面那三条里的**第一条**，治掉了一半
//!
//! 上一节「⚠ 它没有买到什么」的第一条（粒度是「文件」不是「那条判据」）**已经不再是全称**。
//! 本件把粒度收到判据级，**而只收了一档** —— 两档必须分开读，别压成一句：
//!
//! - ✅ **表声明在判据体内**（`const … :` 落在某个 `#[test] fn` 的花括号里）：
//!   现在**逐条判**。删掉这条判据自己那半反向 ⇒ **它红、并点到它的名字**，
//!   同一份文件里别的判据有多少 `assert_eq!(` 都接不住它。
//!   `K-R31` 那条（表叫 `FORMS`、住 `local_backend.rs`）正是这一档 ——
//!   `§0` 那句「另外 27 处随便哪一处都够它过」到此为止。
//! - ❌ **表声明在模块级前言里**（`mod tests {` 里、所有 `#[test]` 之外）：
//!   它今天**仍然只被文件级那条断言守着**。为什么不顺手做了，见下一小节。
//!
//! ⇒ **本条的名字对得上的是第一档。** 第二档的判据级射程**今天不存在**。
//! 别把「本条逐条判」读成「每一张登记表都被逐条判了」。
//! 两档各有几条、盖住哪几份文件，**每趟现算并印在 `--nocapture` 里**（不写死在这儿：`brief` 13b）。
//!
//! ## 为什么模块级那一档没顺手做 —— 是**实测**挡掉的，不是没想到
//!
//! 试过一版「模块级的表按**使用者**归属：哪条判据的体里出现那个表名，就算它带表」。
//! 09-06 现打（量具 `evidence/K-R36-per-guard-census.py`，读数与逐条判词在
//! `K-R36` 件文件 `§8`）：人群 2 → 22 条、红 1 → 7 条，而那 7 条**逐条读过**：
//!
//! - **3 条是误采** —— 表名只出现在**字符串字面量或注释**里：一处是
//!   `contains_word(&body, "REGISTERED")`（指的是**另一份文件**的表名）、
//!   一处是 panic 文案里反引号包着的表名、一处是一行 `//` 注释；
//! - **3 条是「反向那半住在同一张表的兄弟判据里」** —— 那几条是纯粹的**表内容体检**
//!   （每行字段填没填 · 理由够不够长），它们**根本没有扫描面**，
//!   「登记了却已经不在」这一向对它们**不成立**；
//! - **只有 1 条是真缺**（进了下面那张豁免表）。
//!
//! ⇒ 按名字归属**要么连着误采三条，要么再配一层遮罩**（剥掉字符串与注释再认名字）。
//! 而遮罩在本仓的语料上**不可靠**：判据的测试段里满是**合成 Rust 源码串**，
//! `structural_scan.rs` / `local_backend.rs` 还有 raw string，一次失步就整段跟着错，
//! **而错的方向是静默的绿**。⇒ **宁可射程小而准**：多出来的三条假红，
//! 换来的是「这条守卫谁都不信了、于是被关掉」——`D2③` 逐字警告的正是这一步。
//!
//! ## 落地走的是**乙（逐条豁免表）**，不是甲（棘轮）
//!
//! 两条路的代价件里都写着，这里只记**为什么选乙**：
//! 甲（今天的 N 记成上限、只许降）**不点名** —— 今天那一条被补好、同时另一条烂掉，
//! 数仍然是 N ⇒ **静默地绿**。那正是本模块从头到尾在治的形状。
//! 乙按**住址**认，换一条就当场红。
//!
//! ⚠ **乙的代价是真的**：豁免表自己会腐（登记的那条补好了 / 改名了 / 删了，
//! 而豁免仍留着，下一条同名判据一进来就自动带着一张免检章）。
//! ⇒ 对价是它自己那半**反向**（幽灵检查）：豁免对不上一条**今天确实还缺**的判据 ⇒ 红。
//!
//! # 🔴 `K-R37`（09-06）—— 那三条里的**第二条**：射程从一棵树扩到两棵
//!
//! 上面「⚠ 它没有买到什么」的**第二条**（射程只有 `src-tauri/src`）**已经不再成立**：
//! `every_registry_guard_keeps_its_reverse_half` 今天走**逐子树循环**，实参两棵
//!（形状照本模块 [`raw_walkers`]，它从一开始就是这么写的）。
//!
//! ## 🔴 它买到的红是 **0** —— 这一句必须写在最前面
//!
//! `D1` 现打（量具 `evidence/K-R37-daemon-tree-census.py`，读数与**逐条判词**在
//! `K-R37` 件文件 `§8`）：`remote-daemon-proto/src` 那 77 份 `.rs` 里，
//! `TABLE_DECLS` 那四个名字**一处都没有出现过**（分母 = 整份文件文本，比测试段还宽）
//! ⇒ 扩射程之后，文件级人群 +0、判据级 +0、新红 **+0**。
//!
//! ⇒ **本件买到的不是「今天多逮了几条」，是「那棵树从此在视野里」。**
//! 明天有人在那边新写一条「扫描面 ＋ 常量表」型判据、把表**起成闭集里的名字**
//!（那正是本模块那条纪律要求的写法）而忘了留反向那半 —— 从今天起它会当场红并被点名；
//! 在此之前，本条对整棵树**恒真地绿**，而那种绿与「那边全都合规」长得一模一样。
//!
//! ## ⚠ 别把「新红 0」读成「那边都合规」—— 两个数量的是两件事
//!
//! 同一趟 `D1` 还逐条读了那 79 条真声明（`grep` 口径 80，差的那一条是注释里举的例子），
//! 判词按 `K-R33` 那一套分：**登记表 26 · skip 表 15 · 形态表 19 · needle 表 8 ·
//! 非判据表 11**。那 26 张登记表**逐张核过反向那半**：25 张有（双向对拍或逐条幽灵检查）、
//! 1 张（`build_id_guard::SUBCOMMAND_HISTORY`）是只追加的历史表、「登记了却已经不在」
//! 这一向对它不成立 ⇒ **真缺的 0 张**。
//! ⇒ 「新红 0」在这棵树上**恰好**与事实同向，但那是**两个独立的 0**：
//! 一个是「闭集按名字认，这棵树一个名字都不占」，另一个是「那边今天真的都补着」。
//! **别把前者当成后者的证据** —— 换一棵树它们立刻分开。
//!
//! ⚠ **上一节里另有一句今天成了陈账，逐字指住**：`K-R33` 那条 `D1②` 读数写着
//! 「其中 20 条住在 daemon 树（**射程之外**，另一族病）」——「射程之外」四个字从今天起不对了
//!（那句话本身的数没错，它量的是 `K-R33` 那一趟的人群）。**这里只点账，不改 `K-R33` 的读数**：
//! 改一个别的件落下的读数，等于把它的分母换掉而不说，那是本区最贵的那条病。
//!
//! ## 射程今天到哪儿为止（逐字写明还差哪几棵）
//!
//! 覆盖：`src-tauri/src` · `remote-daemon-proto/src`。
//! **仍然没看的（09-06 现打，本仓 `.rs` 的其余落点）**：
//! `src-tauri/crates`（9 份）· `src-tauri/vendor`（23 份）· `src-tauri/build.rs`（1 份）。
//! ⚠ 那三处闭集命中同样是 0（现打）⇒ 今天补进来也是 +0 红；
//! **但「今天 +0」不是「不用补」** —— 这一行说的是「没看」，不是「没有」。
//!
//! ## 采集量地板：为什么是**逐子树**一个，不是一个总数
//!
//! `D2` 的 acceptor 逐字点名过一种糊弄法：「把新树加进实参而**实际上采不到东西**」。
//! 而这棵树对人群的贡献是 0 ⇒ 把它的实参改坏，**下面所有断言一条都不会红**。
//! 只看总数的地板同样接不住（`src-tauri/src` 那一百多份自己就顶过去了）。
//! ⇒ 地板逐子树各一个（抽取器自检⑤）。
//! ⚠ 它**不**管「路径根本不存在」那一形 —— 那一形 `scan_tree_excluding_self` 自己 panic。
//!
//! ## 射程钉死：**反过来那一刀**（把树删掉）同样是静默的
//!
//! 地板只管「声明了的那几棵各自真采到了东西」，**管不着「少声明了一棵」** ——
//! 而 daemon 那棵今天对人群的贡献是 0 ⇒ 把它从实参里删掉，本条**每一条断言都照旧绿**，
//! 输出与今天逐字相同。⇒ 抽取器自检⑥拿一份**真实住址**当见证把射程钉住
//!（形状照 `MUST_BE_RECOGNISED`：**不是**把上面那份子树清单再抄一遍 ——
//! 抄一份的话「清单少一棵」与「钉子少一条」会被同一次编辑一起改掉，那是恒真）。
//! ⚠ 它钉住的只有**被点名那一棵**：将来再加第三棵而没同拍加见证，删掉它仍然是静默的。
//!
//! # 🔴 `K-R38`（09-06）—— 那个「只许降」的棘轮，**机器上原先一颗牙都没有**
//!
//! 上面那张存量清单 `PENDING` 的头注从 08-06 起就写着「**只许变短**」，
//! 而守它的 [`the_pending_inventory_only_shrinks`] 断的是 `n <= PENDING_CEILING` ——
//! **`PENDING_CEILING` 就是同一份文件里的一个常量** ⇒ **抬上限只会让它更容易过**。
//! （`K-R33` 的 `R33M3` 实打：把上限抬 1，**不红**。）
//! ⇒ 在本件之前，「只许变短」**只住在一行注释里的纪律上**。
//!
//! ## 🔴 它买的是「把一个**已经成立**的习惯钉住」，不是「止住一条正在漏的口子」
//!
//! 这一句必须写在最前面，因为两种说法对下一个读它的人意思完全不同。
//! `D1②` 现打（量具 [`ratchet_history`] 与 `evidence/K-R38-ratchet-history-census.py`，
//! 分母 = 触碰过本文件的**全部** 9 个提交）：这个上限从立起来到今天
//! **降过 2 次、抬过 0 次**（`25ce345` 31 → `b4c3d1c` 30 → `3609470` 29），
//! 而且**9 个提交上 `PENDING` 都恰好等于 `CEILING`** ⇒ **余量从 08-06 起就一直是 0**，
//! 这一个月里没有任何人去抬它。**它没有在漏。**
//!
//! ⇒ 那本件凭什么还要做：因为**纪律这一档在本仓被证伪过**。铁律 21（只许点名文件
//! `git add`）那条风险行 `7v` 逐字记着「**写下来这一档对 PM 无效**」——
//! 那条规矩写下来之后，写它的人在**同一批提交**里自己又破了两次。
//! ⇒ 一条只靠注释守着的性质，在这个仓里等于没守。**买的是留痕与阻力，不是止血。**
//!
//! ## 走的是**乙（对着 git 历史面比）**，不是甲（钉一个 sha）
//!
//! - **甲 · 把上限钉在一个 sha 上**：得有人维护那个 sha，**而它自己就是一条会腐的登记**
//!   —— 本区 09-06 刚栽过「基线写死一个提交号，而主干在往前走」（`K-R34` 的 `D4`）。
//!   ⚠ 更要命的是**它买不到棘轮**：钉住 `3609470` 上的 29 之后，清单降到 25 再涨回 29
//!   **仍然过** —— 甲买的是「不超过那一天」，不是「只许降」。
//! - **乙 · 对着历史面比**（今天这一条）：**没有第二张要维护的表**，也没有第二个会腐的数。
//!   上限正当地降下去、提交之后，历史最低档**自动跟着降** ⇒ 棘轮往前咬一格；
//!   而且**咬完不会松**：把它抬回去，即使提交了照样红（历史里那个更低的档还在）。
//!
//! 🔴 **乙的代价件里写着：它要一个历史面，而本仓的历史面判据吃过 packfile 的亏**
//! （风险 `5i`：`pb accept` 曾在本机结构性判不了，551 条 BROKEN）。
//! ⇒ **开工前先证明这条路在这台机器上跑得动**，09-06 两侧现打：宿主、以及门禁那个
//! **断网**沙箱（`--network none`，容器内同 uid）里 `git log` 都给 **9** 个提交、
//! `git show <sha>:<本文件>` 逐份取得出上限，两侧逐字同值。
//! ★ 关键在于走的是**真 `git` 二进制**，不是自己去解 object ——
//! `5i` 那次栽的正是「自己解 object，解不动打包过的」。packfile 那条路归 git 自己。
//! 本仓已有先例在门禁里跑 git：`skill_host::git_common_dir`（`rev-parse`）·
//! `doc_claim_registry`（`ls-files`）。
//!
//! ⚠ 反过来那条也记着：`ssh_source` 头注逐字写过「**为什么不在测试里跑 `git show`**
//! —— 那会让判据依赖『测试跑在一棵有 `.git` 的树里』」。**那条在它那儿是对的**：
//! 它要的是一段**已经被删掉的**历史文本，冻结下来就不会腐。
//! 而本条要的恰恰**不能冻结** —— 冻下来的数就又是一个「同一份文件里的常量」，
//! 也就是本件正在治的那个东西。⇒ 两处的取舍不同，不是谁推翻谁。
//!
//! ## ⚠ 它**没有**买到什么（别把名字读大）
//!
//! - **它守的是「这两个数不许比历史上出现过的最低档还高」，不是「清单真的在变短」。**
//!   一年不动、`n` 一条没少，本条**照样绿**。这两句不许压成一句。
//! - **它挡不住「把本条整个删掉」** —— 没有任何判据挡得住这个。它买的是**留痕**：
//!   抬那个数字，从「改一个字符、零阻力零留痕」变成「还得动一条判据」，
//!   而后者在 diff 里是看得见的。
//! - **「删条目腾余量」不归本条** —— 接住它的是
//!   [`no_new_guard_walks_the_tree_without_excluding_itself`]：删掉一行而那份文件
//!   还在裸遍历 ⇒ 它当场以 `newcomers` 红（09-06 变异实打，读数在件文件 `§8`）。
//!   本条只管那两个**数**。
//! - 🔴 **它不管那张豁免表**（`REGISTERED`，`K-R36` 落的）。
//!   盘上现在是**两张会腐的登记表，腐法不同、接住它们的东西也不同**：
//!   那一张按**住址**认、由它自己那条幽灵检查接着；这一张是两个**数**、由历史面接着。
//!   **别并成一件事，也别以为那条幽灵检查能顺带守住 `PENDING`。**

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// 裸遍历的形态。
    const RAW_WALKS: &[&str] = &["read_dir(", "WalkDir", "collect_rs(", "collect_ts("];

    /// **存量**：测试段里仍在裸遍历的文件（08-06 实测 31 个）。
    ///
    /// ⚠ **只许变短。** 迁一个就从这里删一行并把 `PENDING_CEILING` 调下来。
    /// 不许往里加 —— 新写的扫描型判据必须走 `guard_core::scan_tree!`。
    ///
    /// # 🔴 「只许变短」今天**谁在守它**（`K-R38` 09-06，`D3①`）
    ///
    /// [`the_pending_ratchet_never_turns_backwards`] —— 它拿**git 历史**当权威，
    /// 断「今天这个数不许比它在历史上出现过的**最低档**还高」。
    /// ⇒ **在此之前这句话只是一行注释里的纪律**：守它的
    /// [`the_pending_inventory_only_shrinks`] 断的是 `n <= PENDING_CEILING`，
    /// 而那个上限就在下面几行 ⇒ 抬一下就过（`K-R33` 的 `R33M3` 实打不红）。
    ///
    /// ## ⚠ 它买到的比这句话的名字**小**，逐字写明没买到什么
    ///
    /// - 守的是**这个数不许涨回去**，**不是**「清单真的在变短」——
    ///   一年一条没迁，本条照样绿。
    /// - **删一行来腾余量**不归它管：接住那一形的是
    ///   [`no_new_guard_walks_the_tree_without_excluding_itself`]
    ///   （删掉的那份文件还在裸遍历 ⇒ 当场以 `newcomers` 红）。
    /// - 它**挡不住把那条判据本身删掉** —— 买的是**留痕**，不是不可能。
    const PENDING: &[&str] = &[
        "src-tauri/src/account_usage.rs",
        "src-tauri/src/atomic_replace_registry.rs",
        "src-tauri/src/backend/control/daemon_kill.rs",
        "src-tauri/src/backend/control/launch_wire.rs",
        "src-tauri/src/backend/control/local_query.rs",
        "src-tauri/src/backend/mod.rs",
        "src-tauri/src/cross_half_edge_registry.rs",
        "src-tauri/src/doc_claim_registry.rs",
        "src-tauri/src/doc_copy_registry.rs",
        "src-tauri/src/frame_cadence_guard.rs",
        "src-tauri/src/gate_singleton_guard.rs",
        "src-tauri/src/local_read_surface_registry.rs",
        "src-tauri/src/panorama.rs",
        "src-tauri/src/parser.rs",
        "src-tauri/src/polling_registry.rs",
        "src-tauri/src/profile_installer.rs",
        "src-tauri/src/quote_singleton_guard.rs",
        "src-tauri/src/rust_timer_registry.rs",
        "src-tauri/src/session_name_registry.rs",
        "src-tauri/src/shared_crate_registry.rs",
        "src-tauri/src/ssh_source.rs",
        "src-tauri/src/tmux_daemon_gate_guard.rs",
        "src-tauri/src/utils.rs",
        "remote-daemon-proto/src/layering_guard.rs",
        "remote-daemon-proto/src/no_timer_guard.rs",
        "remote-daemon-proto/src/observe/watcher.rs",
        "remote-daemon-proto/src/platform/fallback_guard.rs",
        "remote-daemon-proto/src/protocol_doc_guard.rs",
        "remote-daemon-proto/src/readonly_guard.rs",
    ];

    /// 存量上限（**递减棘轮**）。
    ///
    /// 🔴 **只许往下调。** 守这句话的是 [`the_pending_ratchet_never_turns_backwards`]
    /// （`K-R38` 09-06）：它对着 **git 历史**比，把这个数抬上去**当场红，而且提交了也不会绿**
    /// —— 历史里那个更低的档还在。⇒ 别在这里试「先抬一格让今天好过」，那正是它挡的动作。
    /// ⚠ 它守的是这个**数**；「删一行腾余量」那一形归
    /// [`no_new_guard_walks_the_tree_without_excluding_itself`]。
    // 08-08：`daemon_route.rs` 的裸遍历迁到了 `guard_core::scan_tree!`（那一轮把它的
    // 发现面从一个目录扩到整棵树，顺带就该换掉手写遍历）⇒ 清单少一行，上限一起降。
    const PENDING_CEILING: usize = 29;

    /// 判定「这是一个带登记表的判据文件」的声明形态。**闭集，按名字认。**
    ///
    /// 🔴 **这是一个闭集，不是一族形状** —— 往里加一个名字就是在**放宽**一条守卫，
    /// 加之前先读模块头注那一节（`K-R33`）：它写着为什么这里走「闭集 ＋ 点名钉住」
    /// 而不是「按形状认」，以及那条纪律要求新写的扫描型判据把表**起成这里的名字之一**。
    const TABLE_DECLS: &[&str] = &[
        "const REGISTERED:",
        "const SITES:",
        "const SCHEDULING_SITES:",
        // `K-R33` 09-06：`K-R31` 那条判据的形态表（`local_backend.rs`）。
        // 全树现打：三棵树的 `.rs` 里 `const FORMS:` **恰好一处**，就是它 ⇒ 这一条不引入误采。
        "const FORMS:",
    ];

    /// 识别器：这份**测试段**里有没有一张登记表。
    ///
    /// 单独成函数（`K-R33`），是为了让下面那条反向自检能拿**合成文本**正反各喂一遍。
    /// 直接在真树上判的话，「采到了它、而它过了」与「压根没扫到它」在输出上一模一样 ——
    /// 那正是 `K-R33` 立件的那一格。
    fn declares_a_guard_table(regs: &str) -> bool {
        TABLE_DECLS.iter().any(|d| regs.contains(d))
    }

    /// 反向那半的合法形态。**闭集。**
    ///
    /// 🔴 **不许为了让新红变绿往里加成员**（`K-R36` `D2①` 明禁）：第一形 `assert_eq!(`
    /// 几乎每一份测试段里都有 —— 闭集再宽一点，下面那两层判定就双双恒空，
    /// 而输出与今天一模一样（绿）。真有第四种正当写法 ⇒ **单独论证 + 给读数**
    /// （哪几条在用、为什么它也算「反向那半」），别顺手加。
    ///
    /// 〔`K-R36` 09-06 把它从 [`every_registry_guard_keeps_its_reverse_half`] 的函数体里
    /// 提到模块级，**成员一个没加、一个没减**。提出来的唯一理由：新加的那条反向自检
    /// 要拿**同一份**闭集去判合成文本，而一个闭集只许有一个住址（`brief` 13b）。
    /// 顺带删掉了原先那句「反向那半的**两种**合法形态」—— 那个基数写死在散文里，
    /// 而成员早已是三个：13b 治的正是这一形，而它就长在这条判据自己头上。〕
    const REVERSE: &[&str] = &["assert_eq!(", "已经不在了", "已经没有"];

    /// 一个 `#[test]` 块里**那个 fn 项本身** —— 从 `fn` 那一行到**同缩进**的收尾 `}`。
    ///
    /// # 为什么不能整块拿去判〔`K-R36` 09-06 现打的活体〕
    ///
    /// [`guard_core::test_attr_chunks`] 切出来的块是「这条 `#[test]` 到下一条 `#[test]`」，
    /// 它**还带着这条判据之后、下一条判据之前的模块级代码**（辅助函数 · 常量 ·
    /// 下一条判据的文档注释）。拿整块判「表声明在谁体内」会把一张**模块级**的表
    /// 算成「它上面那条判据自带的」：`polling_registry.rs` 的 `SCHEDULING_SITES`
    /// 声明在 `every_data_poll_names_its_event_source_and_owner` **之后**、
    /// 真正用它的 `every_scheduling_call_site_is_classified` **之前**
    /// ⇒ 按块判会把它记到前者头上，而前者没有反向那半 ⇒ **一条假红**。
    ///
    /// # 它认什么、认不出什么（认不出的是**漏判**，不是假绿）
    ///
    /// 认：跳过块首的空行 / 属性行 / 注释行之后，**紧接着就是 `fn <名字>`**。
    /// 这一条顺带把 [`guard_core::test_attr_chunks`] 的**模块级前言**那一块挡在外面
    /// （前言跳过属性之后是 `mod … {`，不是 `fn`），而且**不按块的下标跳** ——
    /// 文本以 `#[test]` 打头时前言那一块根本不存在，按下标跳会跳掉第一条真判据。
    ///
    /// 认不出（整条跳过，不进人群）：`#[test]` 与 `fn` 之间夹着**折行**的属性
    /// （`tmux.rs` 那个折行的 `#[cfg_attr(` 是这一形），以及 `pub fn` / `async fn`。
    /// ⚠ **单行**的 `#[ignore = "…"]` / `#[cfg(…)]` **不是**漏判面 —— 块首的属性行会被跳过。
    /// 09-06 现打：人群那几份文件里，本函数认不出的块 **0 块**
    /// ⇒ 今天一条都没漏。分母与逐处住址在 `evidence/K-R36-per-guard-census.py` 的输出里
    /// （分母只取人群那几份就够：块里真有登记表声明 ⇒ 那份文件必定在人群里）。
    ///
    /// 收尾靠**缩进配对**（`rustfmt` 的产物上成立），**不是花括号配平**：配平要解析字符串与
    /// 字符字面量，而本仓的判据语料里满是**合成 Rust 源码串**（还有 raw string），
    /// 一次失步就整段跟着错，而错的方向是**静默的绿**。宁可用一把粗而稳的尺子。
    fn guard_fn_item(chunk: &str) -> Option<(String, String)> {
        let lines: Vec<&str> = chunk.lines().collect();
        let head = lines.iter().position(|l| {
            let t = l.trim_start();
            !(t.is_empty() || t.starts_with('#') || t.starts_with("//"))
        })?;
        let first = lines[head];
        let indent = &first[..first.len() - first.trim_start().len()];
        let name: String = first
            .trim_start()
            .strip_prefix("fn ")?
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            return None;
        }
        let close = format!("{indent}}}");
        let end = lines[head..]
            .iter()
            .position(|l| *l == close)
            .map_or(lines.len(), |k| head + k + 1);
        Some((name, lines[head..end].join("\n")))
    }

    /// 一段**测试段**文本里，每一条**把登记表声明在自己体内**的判据 —— `(判据名, 判据体)`。
    ///
    /// 🔴 **单独成函数，与 [`declares_a_guard_table`] 同一个理由**：直接在真树上判的话，
    /// 「每条判据各判各的」与「口径其实把整份文件的文本喂给了每一条」
    /// 在输出上**一模一样**（都是绿）。
    /// [`the_per_guard_split_does_not_hand_every_guard_the_whole_file`] 拿合成文本把这一格钉住。
    fn guards_declaring_a_table(test_src: &str) -> Vec<(String, String)> {
        guard_core::test_attr_chunks(test_src)
            .iter()
            .filter_map(|c| guard_fn_item(c))
            .filter(|(_, item)| declares_a_guard_table(item))
            .collect()
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf()
    }

    /// 抠出所有 `#[cfg(test)]` 段（到下一个顶层 `}` 为止）。
    fn test_regions(src: &str) -> String {
        let mut out = String::new();
        let mut i = 0usize;
        while let Some(j) = src[i..].find("#[cfg(test)]") {
            let at = i + j;
            let end = src[at..].find("\n}\n").map_or(src.len(), |e| at + e);
            out.push_str(&src[at..end]);
            out.push('\n');
            i = at + "#[cfg(test)]".len();
        }
        out
    }

    /// # 〔audit-0805 08-06〕横扫结论：「多条判据一起瞎」这一族**全仓已清**
    ///
    /// 起因是两次实测：`no_timer_guard` 的两道针被同一种改写一次穿两层；
    /// `inbound` 的两条判据开头是**同一句** `if spec.fields.is_empty() { continue; }`
    /// —— 一个声明就能同时关掉两条。由此命名了这个形态并横扫全仓，两轮口径：
    ///
    /// ① **判据之间共用同一个提前返回**（文本相同的 `if … { continue/return }`）：
    ///    全仓 4 种，其中三种是各自独立的文件过滤 / 自排除（`rel == SOLE_HOME` 在两个
    ///    单例守卫里各指各的常量），**真正的共用开关只有 `spec.fields.is_empty()` 那一处**，
    ///    已由 `inbound::declaring_zero_fields_needs_a_reason` 补上。
    /// ② **多条判据共用同一个采集器**（它一坏就集体失明）：逐个核过
    ///    `rust_files` / `collect_ts` / `doc_files` / `daemon_sources` / `layer_sources` /
    ///    `scan_files` / `platform_cfgs` / `backend_files` / `daemon_control_production` …
    ///    —— **每一个都有自检**（地板、或与登记表的数量相等对拍）。**零发现。**
    ///
    /// ⚠ 为什么不把这条横扫做成判据：我写的两版探测器**都不可靠** ——
    /// 第一版按单行匹配 `if … { continue; }`，而 rustfmt 把它拆成两行 ⇒ 全仓零命中；
    /// 第二版按「调用点后 400 字符内有 assert 地板」判自检，把 `collect_ts` / `layer_sources` /
    /// `backend_files` 三个**有自检**的误报成没有（它们的自检写在变量上、或是等数对拍）。
    /// ⇒ 依它建判据 = 把一个测不准的量具钉进门禁。**登记为已核事实，不做成机检。**
    ///
    /// 「扫描面 + 登记表」型判据的**反向那半**必须在（〔audit-0805 08-06〕裁决件产出）。
    ///
    /// # 它钉的是一个被实测证明**今天成立**的前提，不是一个缺陷
    ///
    /// 本轮怀疑过两件事，量下去**两件都不成立**：
    ///
    /// 1. **「自检重建了扫描面副本」是不是缺陷** —— 不是。`polling_registry` 与
    ///    `session_name_registry` 的自检确实各自又走了一遍遍历器，但
    ///    ① 把 `scan()` 的根整个打瞎 ⇒ 棘轮的**反向那半**当场红
    ///    （逐字「登记表里的 `src/session-accounts-poll.ts` 已经没有周期唤醒了」）；
    ///    ② 局部缩水（静默跳过一个子目录）⇒ 自检的地板红（`118 < 170`），
    ///    因为自检与 `scan()` **共用同一个 walker 函数**，函数体坏了两边一起坏。
    /// 2. **是不是有登记表只做单向对拍** —— 没有。六个登记表逐个变异验过，
    ///    模拟「某个调用点退役了」都会红（`atomic_replace_registry` 逐字
    ///    「登记表里的 `config.rs [MoveFileExW]` 已经不在了 —— 删掉这条」）。
    ///
    /// ⇒ 于是**第 1 条的安全性整个压在第 2 条上**：反向那半一旦被削成单向，
    /// 「打瞎扫描面」就再没有人接住，而自检那份副本**照样绿**。
    /// 这正是本区反复记的形状：**一条纪律的成立依赖另一条，而那条依赖没人盯。**
    ///
    /// # 它查得动什么、查不动什么
    ///
    /// 查的是**存在性**：每个带登记表的判据文件里，必须至少有一种反向那半的形态
    /// （`assert_eq!` 双向对拍，或显式的「登记了但实测没有」诊断）。
    /// ⚠ **查不动「反向那半是否真的在被执行」** —— 有人可以留着字样却把逻辑绕开。
    /// 这是**前提触发器**，不是证明：它挡的是「顺手删掉反向那半」，
    /// 那是实际会发生的动作（改判据时嫌它啰嗦），而不是蓄意伪装。
    #[test]
    fn every_registry_guard_keeps_its_reverse_half() {
        let root = repo_root();
        let mut population: Vec<String> = Vec::new();
        let mut missing: Vec<String> = Vec::new();
        // 🔴 `K-R36`：**判据级**那一层。与文件级那层**各扫各的** —— 它刻意不挂在
        // `declares_a_guard_table(&regs)` 那个 `continue` 后面：挂上去的话，
        // 文件级采不到就把判据级一起带瞎，而两层同时瞎与两层都过**输出完全相同**。
        let mut per_guard: Vec<String> = Vec::new();
        let mut missing_guards: Vec<String> = Vec::new();
        // 🔴 `K-R37` 09-06：**逐子树循环**。上一版这里的实参逐字只有
        // `&root.join("src-tauri/src")` 一棵树 ⇒ `remote-daemon-proto/src` 那 77 份 `.rs`
        // 本条**一份也没打开过**，而它的失败文案**只点它采到的文件、没采到的一个字都不提**
        // ⇒ 「没红」与「没看」在输出上一模一样（本模块从头到尾在治的正是这个形状）。
        // ★ 形状不是发明的：本模块 [`raw_walkers`] 从一开始就是这么写的（逐字同一份清单）。
        let mut scanned: Vec<(&str, usize)> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for sub in ["src-tauri/src", "remote-daemon-proto/src"] {
            let files = guard_core::scan_tree!(&root.join(sub), &["rs"]);
            scanned.push((sub, files.len()));
            for (f, src) in files {
                let rel = f
                    .strip_prefix(&root)
                    .unwrap_or(&f)
                    .to_string_lossy()
                    .replace('\\', "/");
                seen.push(rel.clone());
                for (name, item) in guards_declaring_a_table(&guard_core::test_source(&src)) {
                    per_guard.push(format!("{rel}::{name}"));
                    if !REVERSE.iter().any(|m| item.contains(m)) {
                        missing_guards.push(format!("{rel}::{name}"));
                    }
                }
                let regs = test_regions(&src);
                if !declares_a_guard_table(&regs) {
                    continue;
                }
                population.push(rel.clone());
                if !REVERSE.iter().any(|m| regs.contains(m)) {
                    missing.push(rel);
                }
            }
        }
        // 抽取器自检⑤（`K-R37`）：**采集量地板，逐子树一个** —— 不是只看总数。
        //
        // 🔴 它买的是 `D2` 的 acceptor 逐字点名的那一格：「把新树加进实参而**实际上采不到
        // 东西**」（路径拼错 / 后缀过滤掉了）。为什么必须**逐子树**：daemon 那棵今天对
        // `population` 的贡献是 **0**（闭集里那几个名字在那棵树上一处都没有，`D1` 现打），
        // ⇒ 把它的实参改坏，下面那些断言**一条都不会红**，而总数地板也顶得过去
        //（`src-tauri/src` 那 100 多份自己就够）。**一个只看总数的地板在这里等于没有。**
        //
        // ⚠ 诚实边界：路径**不存在**那一形其实不靠本格 —— `scan_tree_excluding_self`
        // 自己会 `panic!("读目录 … 失败")`。本格接的是**存在、但采不到东西**那一形
        //（后缀写错 · 指到一个几乎空的子目录）。两形各有各的接手人，别把本格读大。
        const SUBTREE_FLOOR: usize = 40;
        let starved: Vec<String> = scanned
            .iter()
            .filter(|(_, n)| *n < SUBTREE_FLOOR)
            .map(|(sub, n)| format!("  {sub} —— 只采到 {n} 份"))
            .collect();
        assert!(
            starved.is_empty(),
            "这几棵子树的采集量低于地板 {SUBTREE_FLOOR}：\n{}\n\
             ⇒ 那个实参此刻**几乎什么都没采到**，而本条对它「全绿」——\n\
             那正是「没红」与「没看」在输出上一模一样的那一格。\n\
             ⇒ 先核实参（路径拼对了吗 · 后缀过滤对吗），别调地板让今天好过。\n\
             （本趟逐子树的采集量：{scanned:?}）",
            starved.join("\n")
        );
        // 抽取器自检⑥（`K-R37`，**点名**）：**射程本身**也要被钉住。
        //
        // 没有它，把实参改回一棵树是**静默**的：daemon 那棵今天对人群的贡献是 0
        // ⇒ 删掉它，上面那个地板（只看还剩的那几棵）与下面所有断言**全部照旧绿**，
        // 而输出与今天一模一样。那正是本件立件的那一格，只是方向反过来。
        //
        // 🔴 **为什么钉的是一个真实住址，而不是把上面那份子树清单再抄一遍**：
        // 抄一份的话，「清单少一棵」与「钉子少一条」会被同一次编辑一起改掉 ⇒ 恒真。
        // 形状照上面的 [`MUST_BE_RECOGNISED`]：拿**盘上真有的那一份**当见证。
        const MUST_BE_IN_REACH: &[(&str, &str)] = &[(
            "remote-daemon-proto/src/wire.rs",
            "daemon 那棵树的见证 —— `K-R37` 之前本条的实参逐字只有 `src-tauri/src`，\
             那棵树的 `.rs` 一份也没被打开过",
        )];
        let out_of_reach: Vec<String> = MUST_BE_IN_REACH
            .iter()
            .filter(|(p, _)| !seen.iter().any(|q| q == p))
            .map(|(p, why)| format!("  {p} —— {why}"))
            .collect();
        assert!(
            out_of_reach.is_empty(),
            "这几份**应当**在本条的射程里，而本趟一份都没扫到：\n{}\n\n\
             本趟逐子树的采集量：{scanned:?}\n\n\
             🔴 别把这条读成「那份文件没了」—— 它红的多半是**射程被改窄了**：\n\
             上面那个 `for sub in [...]` 少了一棵树，或者那棵树的实参指到了别处。\n\
             ★ 射程改窄在本条上是**静默**的：被删掉那棵树对人群的贡献可以是 0，\n\
             于是下面每一条断言都照旧绿，输出与改窄之前逐字相同 ——\n\
             「没红」与「没看」在输出上一模一样，那正是 `K-R37` 立件的那一格。\n\
             ⇒ 处置：把那棵树加回实参；真要缩射程，先回答「那边从此谁看」。",
            out_of_reach.join("\n")
        );
        // 🔴 `K-R33`：**把采到了谁印出来。**
        //
        // 在此之前，本条对一份「它压根没扫到」的文件与一份「它采到了、而且过了」的文件
        // **输出完全相同**（都是静默的绿）。`K-R31` 新加的那条判据整整一轮落在前一格里
        // 而没有任何人看得出来 —— 那不是漏了一条，是这条判据**说不出自己看了谁**。
        // 只在 `--nocapture` 下可见；判红时那几条断言的文案里另有一份。
        // ⚠ 标签刻意**不写判据的函数名** —— 抄一份名字进字符串，改名那天它就是一句假话
        //（本模块头注里的 `parity_ledger` 就是这个形态）。判据名 cargo 自己会打在上一行。
        // 🔴 `K-R37`：**射程也是一个读数**。人群数说不出「我看了哪几棵树、各几份」——
        // 而本件治的正好是那一格：一棵树整个没被打开，人群数与「那边全合规」同值。
        // ⇒ 每趟**现算**并印出来（`brief` 13b：别把清单/基数写死在散文里）。
        eprintln!(
            "〔登记表型判据 · 本趟的射程〕{} 棵子树 · 逐棵采集量 {scanned:?}",
            scanned.len()
        );
        eprintln!(
            "〔登记表型判据 · 本趟采到的人群〕{} 个：\n  {}",
            population.len(),
            population.join("\n  ")
        );
        // 抽取器自检①：人群不能空 —— 空了下面那条会零命中地绿。
        assert!(
            population.len() >= 5,
            "只认出 {} 个带登记表的判据文件（08-06 实测 6 · 09-06 实测 9）—— 抽取器坏了，本条此刻是空转的：{population:?}",
            population.len()
        );
        // 抽取器自检③（`K-R33`，**点名**）：闭集是**按名字**认的，而名字是会被改的。
        // 改一个名字 ⇒ 那一形当场退回「没被扫到」，而「没被扫到」与「过了」在上面那条
        // 断言上**输出完全相同**（都不红）。⇒ 拿真实住址把至少一形钉住，让它改名即红。
        const MUST_BE_RECOGNISED: &[(&str, &str)] = &[(
            "src/backend/control/local_backend.rs",
            "`K-R31` 的 `nothing_in_the_production_path_runs_code_between_fork_and_exec`，\
             它的表叫 `FORMS`",
        )];
        let unseen: Vec<String> = MUST_BE_RECOGNISED
            .iter()
            .filter(|(p, _)| !population.iter().any(|q| q.ends_with(p)))
            .map(|(p, why)| format!("  {p} —— {why}"))
            .collect();
        assert!(
            unseen.is_empty(),
            "这几份**应当**被采进人群，而本趟一个候选都没采到它们：\n{}\n\n\
             本趟采到的是这 {} 个：\n  {}\n\n\
             🔴 别把这条读成「那份文件坏了」—— 它红的是**本条自己瞎了**：\n\
             上面那个 `TABLE_DECLS` 是**按名字**认表的闭集，被点名的那份文件把表改了个名字\n\
             （或换了写法）⇒ 它从此掉出人群，而掉出去之后本条对它**恒真地绿**。\n\
             ★ `K-R33` 立件的正是这一格：`K-R31` 那条判据的表叫 `FORMS`，闭集里没有这个名字，\n\
             于是它整整一轮**没被判到**，而输出与「判到了并且过了」一模一样。\n\
             ⇒ 处置：把新表名加进 `TABLE_DECLS`（那是**放宽**，先读模块头注 `K-R33` 那一节），\n\
                或者把那张表改回闭集里的名字。",
            unseen.join("\n"),
            population.len(),
            population.join("\n  ")
        );
        // 抽取器自检②（负向）：**不带登记表的文件不许进人群**，
        // 否则「人群够大」这个自检可以靠把整棵树算进来而恒真。
        assert!(
            !population.iter().any(|p| p.ends_with("src/tmux.rs")),
            "`tmux.rs` 没有登记表却被算进人群 —— 判别式太松，人群数就不再说明任何事"
        );
        assert!(
            missing.is_empty(),
            "这些登记表型判据**没有反向那半**（登记了但实测已经没有 ⇒ 也该红）：\n  {}\n\
             ★ 单向对拍只挡「多一处」，挡不住「登记表腐烂」——\n\
             而本仓另有一条纪律**整个压在它上面**：扫描面被打瞎时，\n\
             接住的正是反向那半（自检那份副本照样绿，实测过）。\n\
             ⇒ 删它之前先想清楚谁来接「扫描面悄悄不扫了」这件事。",
            missing.join("\n  ")
        );

        // ── 🔴 `K-R36`：判据级那一层（上面那几条判的单位是**文件**）───────────────
        //
        // 只在 `--nocapture` 下可见。标签刻意**不写判据的函数名**，理由同上面那一处。
        eprintln!(
            "〔登记表判据 · 判据级射程〕{} 条（**表声明在判据体内**的那一档；\
             模块级前言里的表仍在文件级那一档，见模块头注 `K-R36` 那一节）：\n  {}",
            per_guard.len(),
            per_guard.join("\n  ")
        );
        // 抽取器自检④（`K-R36`，**点名**）：切法一坏，这一层**采到 0 条**，
        // 而 0 条时下面那两条断言**恒真地绿** —— 与 `K-R33` 那一格同形：
        // 「没扫到你」与「判过你了」在输出上一模一样。⇒ 拿真实住址把**本件的题眼**钉住。
        const MUST_BE_JUDGED_PER_GUARD: &[(&str, &str)] = &[(
            "src/backend/control/local_backend.rs::nothing_in_the_production_path_runs_code_between_fork_and_exec",
            "`K-R36` 的题眼：它的表叫 `FORMS`、声明在它自己体内，\
             而同一份文件的测试段里另有几十处 `assert_eq!(` ——\
             文件级那一层对它恒真，判据级这一层才判得到它",
        )];
        let unjudged: Vec<String> = MUST_BE_JUDGED_PER_GUARD
            .iter()
            .filter(|(p, _)| !per_guard.iter().any(|q| q.ends_with(p)))
            .map(|(p, why)| format!("  {p} —— {why}"))
            .collect();
        assert!(
            unjudged.is_empty(),
            "这几条**应当**被判据级那一层判到，而本趟一条都没采到：\n{}\n\n\
             本趟判据级采到的是这 {} 条：\n  {}\n\n\
             🔴 别把这条读成「那条判据坏了」—— 它红的是**判据级那一层自己瞎了**：\n\
             切法（`guard_fn_item` ＋ `guard_core::test_attr_chunks`）一坏，这一层采到 0 条，\n\
             而 0 条时下面那两条断言恒真地绿。\n\
             ⇒ 先看被点名那条判据是不是把表**挪出了函数体** —— 挪出去就掉回文件级那一档，\n\
                模块头注 `K-R36` 那一节逐字写着这两档的分界。",
            unjudged.join("\n"),
            per_guard.len(),
            per_guard.join("\n  ")
        );

        // `K-R36` `D2③` 在**甲（棘轮）**与**乙（逐条豁免表）**里选了**乙**，
        // 两条路各自的代价与选它的理由写在模块头注 `K-R36` 那一节，这里不写第二遍。
        /// 今天**确实缺**反向那半、而本件**不去补**的那几条（`D2②`：补是别人的活）。
        ///
        /// 三列：**住址**（`路径::判据名`）· 为什么今天不补 · **解锁条件**。
        /// 起名 `REGISTERED` 是照本模块头注那条纪律（新写的「扫描面 ＋ 常量表」型判据，
        /// 表要起成 `TABLE_DECLS` 里已有的名字之一）。
        /// ⚠ **诚实边界**：本文件被 `scan_tree!` 按构造摘除 ⇒ 这条元判据**看不见自己这张表**，
        /// 起对名字在这里买到的只是**纪律的一致性**，不是「它真被判到了」。
        /// 真正接住这张表腐烂的是紧跟着的那条**幽灵检查**。
        const REGISTERED: &[(&str, &str, &str)] = &[(
            "src-tauri/src/structural_scan.rs::comment_stripping_has_exactly_one_shared_implementation",
            "它自带扫描面（`scan_tree!`）与登记表，但只断了「扫到的里有没有没登记的」这一向；\
             「登记了却已经不在」那一向没人接 —— 而那正是本条要治的族。\
             本件按 `D2②` 只让它红出来，不代补",
            "给它补上双向对拍（`assert_eq!(found, want)`），\
             或一条「登记表里的 X 已经不在了」的诊断；补完把这一行删掉 —— \
             不删的话下面那条幽灵检查会逼你删",
        )];

        // 这张豁免表自己那半**反向**：登记了却**已经不在了**的豁免，必须红。
        // 没有它，豁免表就是一张只会长草的免检名单（`D2③乙` 逐字写着的那个代价）。
        let stale: Vec<String> = REGISTERED
            .iter()
            .filter(|(addr, ..)| !missing_guards.iter().any(|m| m.as_str() == *addr))
            .map(|(addr, ..)| format!("  {addr}"))
            .collect();
        assert!(
            stale.is_empty(),
            "豁免表里这几条**已经不在了** —— 它们要么补上了反向那半，要么改名/被删了：\n{}\n\
             ⇒ 把这几行从上面那张 `REGISTERED` 里删掉。留着就是把这条守卫的余量白送出去：\n\
             下一条同名的判据一进来，就自动带着一张谁也没签过的免检章。\n\
             （本趟判据级实缺的是这 {} 条：{missing_guards:?}）",
            stale.join("\n"),
            missing_guards.len()
        );

        let unexcused: Vec<String> = missing_guards
            .iter()
            .filter(|m| !REGISTERED.iter().any(|(addr, ..)| *addr == m.as_str()))
            .map(|m| format!("  {m}"))
            .collect();
        assert!(
            unexcused.is_empty(),
            "这几条判据**自带登记表、却没有自己那半反向**：\n{}\n\
             ★ 单位是**这一条判据**，不是这份文件 —— 同一份文件里别的判据有多少\n\
             `assert_eq!(` 都接不住它。`K-R36` 立件的正是这一格：在一份 28 处断言的文件上，\n\
             「每条判据」与「这份文件」相差 27 条判据。\n\
             ⇒ 处置**二选一**：\n\
               ① 给它补上反向那半 —— 双向对拍（`assert_eq!`），\n\
                  或一条「登记表里的 X 已经不在了」式的诊断；\n\
               ② 真有理由今天不补 ⇒ 写进上面那张 `REGISTERED`，**三列都要填**\n\
                  （住址 · 为什么不补 · 解锁条件），幽灵检查会盯着它别长草。\n\
             🔴 **没有第三条路**：往 `REVERSE` 里加一个成员把它变绿 ——\n\
                那是把一条本来就近乎空真的守卫弄得更空（`K-R36` `D2①` 明禁）。",
            unexcused.join("\n")
        );
    }

    /// ★ `K-R33` 的**反向那半**：识别器不许「什么表都算」。
    ///
    /// # 没有它，把上面那条判据关掉只需要一次「放宽」
    ///
    /// [`declares_a_guard_table`] 的口径一旦宽到「测试段里有个 `const … : &[…]` 就算」，
    /// 人群就会**恒真地**吃下几乎每一份判据文件；而 `REVERSE` 的第一形是 `assert_eq!(`，
    /// 几乎每一份测试段里都有 ⇒ `missing` 恒空 ⇒ 上面那条判据变成一场仪式，
    /// **而它的输出与今天一模一样（绿）**。
    /// ⇒ 放宽口径必须同时买一条「**这些不许被采**」，否则买到的只是一个更大的空转。
    ///
    /// ⚠ **诚实边界（别把这条读大）**：今天的口径是**按名字的闭集**，
    /// 按构造就不会过采 ⇒ 下面那几格**此刻是廉价的**，它们不是在证明今天的口径准。
    /// 它们承的是**将来**那一拍：谁把闭集换成一族形状，这几格当场红。
    #[test]
    fn the_registry_table_recogniser_does_not_say_yes_to_every_const_slice() {
        // 正：闭集里的每一个名字都要真的被认出来 ——
        // 识别器与闭集脱钩（比如把名字写死进函数体）时，这一格逮它。
        for d in TABLE_DECLS {
            let synthetic = format!("    {d} &[&str] = &[\"x\"];");
            assert!(
                declares_a_guard_table(&synthetic),
                "闭集里写着 `{d}`，识别器却认不出这一形 —— 识别器与 `TABLE_DECLS` 脱钩了：{synthetic}"
            );
        }
        // 负：needle 表 / skip 表 / 扫描面表**都不是登记表**，不许被采进人群。
        // 三种都是本仓真实存在的形态 —— `K-R33` `D1②` 09-06 逐条判过（分母与逐条判词住件文件，
        // 量具 `evidence/K-R33-table-decl-census.py`）：那 138 条里三者合计比登记表还多。
        const NOT_A_REGISTRY_TABLE: &[(&str, &str)] = &[
            (
                "    const NEEDLES: &[&str] = &[\"read_dir(\", \"WalkDir\"];",
                "needle 表 —— 拿去在文本里搜的串，它没有「登记了却已经不在了」这一半",
            ),
            (
                "    const EXEMPT: &[(&str, &str)] = &[(\"a.rs\", \"为什么豁免\")];",
                "skip 表 —— 豁免清单，它腐的方式与登记表不同（该由各自的幽灵检查治）",
            ),
            (
                "    const EXTS: &[&str] = &[\"rs\", \"ts\"];",
                "扫描面表 —— 说的是「去哪儿找」，不是「找到了谁」",
            ),
            (
                "    let sites = collect_sites();",
                "压根不是常量声明（口径一旦按「出现 sites 字样」认，这一行就会被采）",
            ),
        ];
        for (synthetic, what) in NOT_A_REGISTRY_TABLE {
            assert!(
                !declares_a_guard_table(synthetic),
                "{what}\n  —— 它被当成登记表采进来了。\n\
                 🔴 口径宽到「什么表都算」= **关掉**上面那条判据，而不是扩大它的覆盖：\n\
                 人群恒真地满，而 `REVERSE` 里的 `assert_eq!(` 几乎人人都有 ⇒ `missing` 恒空。\n\
                 逐字：{synthetic}"
            );
        }
    }

    /// ★ `K-R36` 的**反向那半**：判据级那一层不许**恒真地**采到反向那半。
    ///
    /// # 没有它，把「按判据判」悄悄写回「按文件判」看不出来
    ///
    /// [`guards_declaring_a_table`] 只要有一处把**整份文件**（或整块）的文本交给每一条判据，
    /// 「逐条判」当场退回「按文件判」，而**真树上的输出与今天一模一样（绿）** ——
    /// 那正是 `K-R36` 立件的那一格，也是 `K-R33` 那一格的同形：
    /// **「判过了」与「压根没判到」在输出上不可区分。**
    /// ⇒ 拿一份合成文本正反各喂一遍：前一条自带反向那半、后一条**确实没有**，
    /// 断言**红且只红后一条**（`D2` 的 acceptor 逐字要的就是这一格）。
    ///
    /// ⚠ **夹具名取中性名，且断言不取自夹具的名字**（`brief` 12 的 `6g`）：
    /// 两个判据名在这里是**变量** —— 喂进去的与断出来的是同一个值，
    /// 改夹具名不会让这一格恒真，也不会让它假红。
    #[test]
    fn the_per_guard_split_does_not_hand_every_guard_the_whole_file() {
        // 🔴 锚点**运行时拼**，而且夹具每一行都缩进 —— 两条都是承重的：
        //   ① 写成字面量的话，这份夹具会变成**本文件源码里一个真的 `#[test]` 边界**
        //      （`guard_core::test_attr_chunks` 按「整行 trim 之后逐字等于那条属性」认）；
        //   ② 顶格的 `}` 会被 `guard_core` 的 `test_module_ranges` 当成本文件测试段的收尾，
        //      把本文件自己的测试段**提前截断**，于是剥法把后半段当生产代码。
        // ★ 本模块头注治的正是「判据在自己的源码里找到了自己」这一族 —— 这里不许复发。
        let attr = concat!("#[te", "st]");
        let (kept, lost) = ("keeps_its_reverse_half", "lost_its_reverse_half");
        let synthetic = [
            "    mod fixture {".to_string(),
            "        const SITES: &[&str] = &[\"住在前言里，不算任何一条判据自带\"];".to_string(),
            format!("        {attr}"),
            format!("        fn {kept}() {{"),
            "            const REGISTERED: &[&str] = &[\"甲\"];".to_string(),
            "            assert_eq!(REGISTERED.len(), 1, \"双向对拍\");".to_string(),
            "        }".to_string(),
            format!("        {attr}"),
            format!("        fn {lost}() {{"),
            "            const REGISTERED: &[&str] = &[\"乙\"];".to_string(),
            // ⚠ 阴性那一条**刻意一个断言都不写**〔`R36M3` 自查逮到，09-06〕：
            // 第一版写的是 `assert!(!REGISTERED.is_empty(), …)`，于是**往 `REVERSE` 里加
            // `assert!(` 这个成员会把本格弄红** —— 那等于本条顺手把一个不归它管的闭集钉死了
            // （`R36M3` 逐字：那是 `KRF2` 那一族的形状，不是本件射程）。
            // 阴性对照要的只是「这条判据没有反向那半」，写成**不含任何断言**最不易被牵连。
            "            let _only_the_forward_half = REGISTERED.len();".to_string(),
            "        }".to_string(),
            "    }".to_string(),
        ]
        .join("\n");

        let judged = guards_declaring_a_table(&synthetic);
        let names: Vec<&str> = judged.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec![kept, lost],
            "判据级的切法采错了人群 —— 期望**恰好这两条**：\n\
             前言里那张 `SITES` 不属于任何一条判据（它是模块级的，归文件级那一档），\n\
             把它算进来就等于又退回「按文件判」。\n\
             合成夹具逐字：\n{synthetic}"
        );

        let no_reverse: Vec<&str> = judged
            .iter()
            .filter(|(_, item)| !REVERSE.iter().any(|m| item.contains(m)))
            .map(|(n, _)| n.as_str())
            .collect();
        assert_eq!(
            no_reverse,
            vec![lost],
            "**红且只红它**这一格没买到。两个方向各说明一次：\n\
             · **一条都没点**（后一条也算「有反向那半」）⇒ 口径把**整份文本喂给了每一条**：\n\
               前一条的 `assert_eq!(` 漏进了后一条 ⇒ 「逐条判」退回「按文件判」，\n\
               而那时真树上的输出与今天**一模一样（绿）**。这是本条存在的全部理由。\n\
             · **多点了名**（把前一条也算缺）⇒ 切块把某一条自己那半反向切丢了。\n\
             合成夹具逐字：\n{synthetic}"
        );
    }

    /// 今天仍在裸遍历的文件（相对仓根）。
    fn raw_walkers() -> Vec<String> {
        let root = repo_root();
        let mut out = Vec::new();
        for sub in ["src-tauri/src", "remote-daemon-proto/src"] {
            // ★ 本模块自己也走 `scan_tree!` —— 它就是那条规矩的第一个遵守者。
            //
            // ⚠ **摘除在这里今天不是承重的**（变异实测）：把 `scan_tree!` 换成一个匹配不上的
            // 摘除名，本条**照样绿** —— 因为真正让本文件不被标记的是下面那个
            // `!regs.contains("scan_tree!")`：本模块的测试段里就写着 `scan_tree!`。
            // 留着摘除是**纵深防御**：哪天本模块多写一个不走 `scan_tree!` 的扫描助手，
            // 没有摘除就会拿 `RAW_WALKS` 里那四个字面量把自己算进去。
            // ★ 「哪一行在真正干活」这种断言**必须变异验过再写** —— 本区第三次
            //（F14 第六刀 `[ -r ]` 不能省 · F12 `uiStrings` 两道都不能省 · 本条）。
            for (f, src) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
                let regs = test_regions(&src);
                // ★ F23 第二刀：**去掉了 `&& !regs.contains("scan_tree!")` 那半**。
                //
                // 它是**整份文件级的豁免**：只要测试段里出现过一次 `scan_tree!`，
                // 这个文件里**再多裸遍历也不会被标记**。豁免的粒度是「文件」，
                // 而事实的粒度是「那一处遍历」—— 又一次**匹配单位与事实不同级**
                // （F24 那一族的反面：这次是单位比事实**大**）。
                //
                // 变异实测：给 `byte_cap_registry`（它用 `scan_tree!`）的测试段加一处裸
                // `read_dir`，**本条照样绿**。去掉那半之后当场红。
                // ⚠ 先证明它恒绿再删（E11）：去掉后**一个文件都没被新标记** ——
                // 说明今天没有「既用 `scan_tree!` 又裸遍历」的文件，那半是纯死重。
                // 而 `scan_tree!` 的调用文本里本来就不含 `RAW_WALKS` 的四个字面量，
                // 所以只用 `scan_tree!` 的文件本来也不会被标记 —— 那半从来没起过作用。
                if RAW_WALKS.iter().any(|w| regs.contains(w)) {
                    out.push(
                        f.strip_prefix(&root)
                            .unwrap_or(&f)
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        out.sort();
        out
    }

    /// ★ 正题：**测试段里不许新增裸遍历**。
    #[test]
    fn no_new_guard_walks_the_tree_without_excluding_itself() {
        let found = raw_walkers();
        // 抽取器自检：扫不到时下面的对拍会两边都空、静默变绿。
        assert!(
            found.len() >= 20,
            "只扫到 {} 个裸遍历文件（08-06 实测 31）—— 抽取器坏了",
            found.len()
        );
        let newcomers: Vec<&String> = found
            .iter()
            .filter(|f| !PENDING.contains(&f.as_str()))
            .collect();
        assert!(
            newcomers.is_empty(),
            "有扫描型判据在测试段里**裸遍历目录**，且不在存量清单里：\n{}\n\n\
             ⇒ 改走 `guard_core::scan_tree!(&root, &[\"rs\"])` —— 它按构造摘除调用者自己那份。\n\
             ★ 为什么非要这条：判据在自己的登记表/注释/常量里找到自己 ⇒ **恒绿**，\n\
             audit-0805 实测五次，**五次都不是被判据变红发现的**（四次靠变异、一次靠 clippy）。\n\
             「以后小心点」对这一族无效，所以修法是**让它写不出来**。",
            newcomers
                .iter()
                .map(|s| format!("  {s}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// ★ **递减棘轮**：存量只许降。
    #[test]
    fn the_pending_inventory_only_shrinks() {
        let n = PENDING.iter().filter(|s| !s.is_empty()).count();
        assert!(
            n <= PENDING_CEILING,
            "存量清单涨到 {n}（上限 {PENDING_CEILING}）—— **只许降**。\
             迁一个就删一行并把上限调下来；**不许把上限调上去让今天好过**。"
        );
        // 清单不许长草：登记的文件必须真的还在裸遍历。
        let found = raw_walkers();
        let stale: Vec<&&str> = PENDING
            .iter()
            .filter(|p| !found.iter().any(|f| f == *p))
            .collect();
        assert!(
            stale.is_empty(),
            "存量清单里这些已经不裸遍历了（迁完了或文件没了）：{stale:?}\n\
             ⇒ 删掉它们并把 `PENDING_CEILING` 一起调下来 —— 留着就是把棘轮的余量白送出去。"
        );
    }

    // ── 🔴 `K-R38` 09-06：给那个「只许降」的棘轮装闸 ─────────────────────────
    //
    // 上面那条断的是 `n <= PENDING_CEILING`，而 `PENDING_CEILING` 就住在这份文件里
    // ⇒ **抬上限只会让它更容易过**。选路（乙 · 对着 git 历史面比）、它的代价、
    // 以及它**没有**买到什么，全写在模块头注 `K-R38` 那一节，这里不写第二遍。

    /// 本文件在仓里的相对住址 —— 下面要拿它去问 git 历史。
    const SELF_REL: &str = "src-tauri/src/scanning_guard_registry.rs";

    /// 找 `PENDING_CEILING` 那行声明的针 —— 🔴 **运行时拼，别写成字面量**。
    ///
    /// 承重，理由是本模块头注治的那一族：下面两个解析器要跑在**本文件自己的历史版本**上，
    /// 而针一旦写成字面量，每一份历史 blob 里它就有**两处**（真声明 ＋ 这行字面量），
    /// 于是「解析到的是哪一处」由两者在文件里的先后决定 —— 一次挪动就能让它悄悄解析错，
    /// **而错的方向是静默的绿**。★「判据在自己的常量里找到了自己」在这里不许复发。
    /// ★ 拼法照本文件已有的那一处（`concat!("#[te", "st]")`）—— 同一个理由，别改回字面量。
    fn ceiling_needle() -> &'static str {
        concat!("const PENDING_", "CEILING", ": usize = ")
    }

    /// 找 `PENDING` 那张表表头的针 —— 同上，**拼出来的，不写字面量**。
    /// ⚠ 必须带冒号：`const PENDING_CEILING` 也以 `const PENDING` 打头。
    fn pending_needle() -> &'static str {
        concat!("const ", "PENDING", ": &[&str] = &[")
    }

    /// 一份**本文件源码文本**里的 `PENDING_CEILING` 值。找不到 ⇒ `None`（不许默默当 0）。
    fn ceiling_in(src: &str) -> Option<usize> {
        let needle = ceiling_needle();
        let at = src.find(needle)? + needle.len();
        src[at..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .ok()
    }

    /// 一份**本文件源码文本**里 `PENDING` 的条数。找不到那张表 ⇒ `None`。
    ///
    /// 口径与判据自己数的那个对齐（`PENDING.iter().filter(|s| !s.is_empty()).count()`）：
    /// 从表头那一行起、到同层 `];` 为止，数**以引号打头**的行。
    fn pending_count_in(src: &str) -> Option<usize> {
        let at = src.find(pending_needle())?;
        let mut n = 0usize;
        for line in src[at..].lines().skip(1) {
            let t = line.trim_start();
            if t.starts_with("];") {
                return Some(n);
            }
            if t.starts_with('"') {
                n += 1;
            }
        }
        None
    }

    /// 跑一条**只读**的 git，回它的 stdout。
    ///
    /// 🔴 **两种「问不到」分开报**，它们在类型上不是一回事（照 `skill_host::git_common_dir`
    /// 那条逐字记着的实测）：机器上没有 `git` 时 [`std::process::Command`] 给的是
    /// `io::Error(NotFound)`，**不是**一个非零退出码。
    ///
    /// ⚠ 两支都 **fail-closed（panic）**，这是刻意的：读不到历史时必须红，不许静默地绿 ——
    /// 「历史面是空的」与「棘轮没被倒着转」在输出上一模一样，那正是本条要治的形状。
    fn git_read(root: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .unwrap_or_else(|e| {
                panic!(
                    "起不来 `git`（{e}）—— 本条拿 git 历史当权威，问不到就**不许猜一个出来**。\n\
                     ⚠ 这一支不是「git 说不知道」，是**进程都没起来**（PATH 里没有它）。\n\
                     ⇒ 本条刻意 fail-closed：读不到历史时红，而不是绿。"
                )
            });
        assert!(
            out.status.success(),
            "`git {}` 在 {} 上退出码 {:?} —— 这一支是「git 起来了、但它说不行」。\n\
             git 自己说：{}\n\
             ⇒ 常见来路：这棵树不在版本控制里 · 浅克隆（`--depth`）把历史截掉了。\n\
                两种都要修环境，**不许把本条改成读不到就跳过**（那等于把闸拆了）。",
            args.join(" "),
            root.display(),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// 本文件在 git 历史上每一个版本的读数 —— `(短 sha, 上限, 条数)`，外加**没解析出来**的份数。
    ///
    /// ⚠ 解析不出来的**不静默丢掉**：份数一起回，由调用方连读数印出来。
    /// （合法的一形：某个提交早于这两个常量存在。今天 9 份全解析得出，实测。）
    fn ratchet_history(root: &Path) -> (Vec<(String, usize, usize)>, usize) {
        let mut rows = Vec::new();
        let mut unparsed = 0usize;
        for sha in git_read(root, &["log", "--format=%h", "--", SELF_REL]).split_whitespace() {
            let spec = format!("{sha}:{SELF_REL}");
            let blob = git_read(root, &["show", &spec]);
            match (ceiling_in(&blob), pending_count_in(&blob)) {
                (Some(c), Some(p)) => rows.push((sha.to_string(), c, p)),
                _ => unparsed += 1,
            }
        }
        (rows, unparsed)
    }

    /// 纯算子：今天的读数 `today` 对着历史面 `hist`，棘轮有没有**被倒着转**。
    ///
    /// 回**历史上最低的那一档**当见证；没被倒转 ⇒ `None`。
    ///
    /// 🔴 **单独成函数**，与 [`declares_a_guard_table`] 同一个理由：真树上今天 `today`
    /// 恰好**等于**历史最低档 ⇒ 把 `>` 写成 `<`、或者把 `hist` 传成空的，
    /// **输出与判对了一模一样（绿）**。
    /// [`the_ratchet_reader_can_tell_a_raise_from_a_drop`] 拿合成读数把这一格钉住。
    ///
    /// ⚠ **诚实边界**：`hist` 为空时它回 `None`（= 绿）。**空历史那一格不归它**，
    /// 归 [`the_pending_ratchet_never_turns_backwards`] 里那条**地板**。
    /// 两格刻意分开：并成一格的话，「历史读不到」与「棘轮没被倒转」又会同形。
    fn ratchet_backslide(today: usize, hist: &[(String, usize)]) -> Option<(String, usize)> {
        let low = hist.iter().min_by_key(|(_, v)| *v)?;
        if today > low.1 {
            Some(low.clone())
        } else {
            None
        }
    }

    /// ★ `K-R38` 的正题：**那两个数不许比它们在历史上出现过的最低档还高。**
    ///
    /// 选路理由（乙，不是甲）· packfile 那条风险怎么证掉的 · 它**没有**买到什么，
    /// 全在模块头注 `K-R38` 那一节，**这里不复述**（复述就会漂）。
    #[test]
    fn the_pending_ratchet_never_turns_backwards() {
        let root = repo_root();
        let n = PENDING.iter().filter(|s| !s.is_empty()).count();
        let (hist, unparsed) = ratchet_history(&root);

        // 抽取器自检①：**历史面不许是空的 / 短的**。
        //
        // 🔴 这一格是本条的地基：`ratchet_backslide` 拿到空历史时回 `None`（绿），
        // 于是「git 读不到历史」与「棘轮没被倒着转」**输出完全相同** ——
        // 那正是本模块从头到尾在治的形状，只是这次长在本条自己头上。
        // 会把历史面弄空的真实来路：浅克隆（`--depth 1`）· 两个常量被改了名
        //（针是按名字认的）· 本文件被挪了地方（`SELF_REL` 就馊了）。
        const HISTORY_FLOOR: usize = 5;
        assert!(
            hist.len() >= HISTORY_FLOOR,
            "只从 git 历史里读出 {} 份本文件的旧版本（地板 {HISTORY_FLOOR}，09-06 实测 9 份，\
             另有 {unparsed} 份解析不出来）——\n\
             ⇒ **本条此刻是空转的**：历史面一空，下面那两格恒真地绿。\n\
             常见来路：① 浅克隆把历史截掉了（要 `fetch-depth: 0`）；\n\
                       ② `PENDING` / `PENDING_CEILING` 被改了名（针是按名字认的）；\n\
                       ③ 本文件挪了位置 ⇒ `SELF_REL`（`{SELF_REL}`）馊了。\n\
             🔴 **不许靠调低地板让今天好过** —— 那是把闸拆了，而拆完输出还是绿的。",
            hist.len()
        );

        // 抽取器自检②：**解析器与真常量对拍。**
        //
        // 上面那两个针是拿文本认的，而下面比的是**真常量**（`PENDING_CEILING` / `n`）。
        // 解析器要是系统性偏了（比如总是多数一行、或总回一个大数），历史最低档跟着偏，
        // 而**真树上照样绿**。⇒ 拿本文件此刻的源码喂一遍解析器，逼它复现那两个真值。
        let me = include_str!("scanning_guard_registry.rs");
        assert_eq!(
            (ceiling_in(me), pending_count_in(me)),
            (Some(PENDING_CEILING), Some(n)),
            "解析器在**本文件此刻的源码**上复现不出那两个真常量 —— 它偏了。\n\
             ⇒ 历史面上的读数跟着一起偏，而真树上本条**照样绿**（两边同向偏）。\n\
             这一格就是为了不让那种偏法静默通过。"
        );

        // 只在 `--nocapture` 下可见 —— 射程与历史面本身也是读数（`brief` 13b：现算，别写死）。
        eprintln!(
            "〔存量棘轮 · 本趟的历史面〕{} 份旧版本（解析不出 {unparsed} 份）· \
             今天 上限={PENDING_CEILING} 条数={n}\n  {}",
            hist.len(),
            hist.iter()
                .map(|(s, c, p)| format!("{s} 上限={c} 条数={p}"))
                .collect::<Vec<_>>()
                .join("\n  ")
        );

        let ceilings: Vec<(String, usize)> = hist.iter().map(|(s, c, _)| (s.clone(), *c)).collect();
        let counts: Vec<(String, usize)> = hist.iter().map(|(s, _, p)| (s.clone(), *p)).collect();

        if let Some((sha, was)) = ratchet_backslide(PENDING_CEILING, &ceilings) {
            panic!(
                "🔴 **棘轮被倒着转了**：`PENDING_CEILING` 今天是 {PENDING_CEILING}，\
                 而它在 `{sha}` 上是 {was}。\n\
                 上面那行头注写着「只许变短」——**这一条从今天起是机器在守，不再是纪律**。\n\
                 ⇒ 处置：把上限调回 {was} 或更低。\n\
                 ★ 想「先抬一格让今天好过」的话，本条正是来挡这个动作的：\n\
                   在它之前，抬这个数是**改一个字符、零阻力、零留痕、零人知道**\n\
                  （`n <= PENDING_CEILING` 里那个上限就在同一份文件里 ⇒ 抬它只会更容易过）。\n\
                 ⚠ 提交了也不会变绿：本条比的是**历史上出现过的最低档**，那个更低的档还在。\n\
                 ⚠ 真有一条新的非进 `PENDING` 不可 ⇒ 先答「为什么它不能走 `scan_tree!`」，\
                   那是一次要被人看见的讨论，不是一个字符。"
            );
        }
        if let Some((sha, was)) = ratchet_backslide(n, &counts) {
            panic!(
                "🔴 **存量清单涨回去了**：`PENDING` 今天 {n} 条，而它在 `{sha}` 上是 {was} 条。\n\
                 头注逐字写着「**只许变短**」——今天守它的是本条。\n\
                 ⇒ 处置：把新加的那几行拿掉，改走 `guard_core::scan_tree!`。\n\
                 ⚠ 别去抬 `PENDING_CEILING` —— 上面那一格会当场逮住它。"
            );
        }
    }

    /// ★ `K-R38` 的**反向那半**：那把比较尺子，得真的分得开「抬上去」与「降下来」。
    ///
    /// # 没有它，本条是一场仪式
    ///
    /// 真树上今天 `PENDING_CEILING` 与 `n` **恰好等于**历史最低档（9 个提交上余量都是 0）。
    /// ⇒ 把 [`ratchet_backslide`] 里的 `>` 写成 `<`、把 `min_by_key` 写成 `max_by_key`、
    /// 或者让历史面传成空的 —— **真树上的输出与判对了一模一样（绿）**。
    /// 那正是本模块从头到尾在治的形状：**「判过了」与「压根没判」不可区分。**
    ///
    /// ⚠ **夹具里的 sha 与数字都取中性值**，断言比的是**喂进去的那个值本身**
    /// （`brief` 12 的 `6g`：别让断言取自夹具的名字）。
    #[test]
    fn the_ratchet_reader_can_tell_a_raise_from_a_drop() {
        let hist: Vec<(String, usize)> = [("aaa", 31), ("bbb", 30), ("ccc", 29)]
            .iter()
            .map(|(s, v)| ((*s).to_string(), *v))
            .collect();
        let low = ("ccc".to_string(), 29);

        // 正：抬上去 ⇒ 必须逮到，而且点的是**历史最低**那一档（不是最近那一档）。
        // 🔴 「点最低那一档」是承重的：点最近那一档的话，抬上去之后只要**提交一次**，
        // 最近那一档就变成抬过的值 ⇒ 下一趟当场变绿，棘轮咬完就松。
        assert_eq!(
            ratchet_backslide(30, &hist),
            Some(low.clone()),
            "比 30 高于历史最低档 29 —— 这一格没逮到，说明比较写反了或者点错了档"
        );
        assert_eq!(
            ratchet_backslide(99, &hist),
            Some(low),
            "点的必须是**历史最低**那一档，不是最近的那一档"
        );

        // 平 / 降：棘轮正着转，一格都不许红。
        assert_eq!(
            ratchet_backslide(29, &hist),
            None,
            "与历史最低档持平，不许红"
        );
        assert_eq!(
            ratchet_backslide(28, &hist),
            None,
            "降下去正是要买的动作，不许红"
        );
        assert_eq!(ratchet_backslide(0, &hist), None, "降到底，仍然不许红");

        // 🔴 **空历史 ⇒ 它回 `None`（绿）**，这一格是**故意钉住的诚实边界**，不是缺陷：
        // 接住「历史面读不到」的是 `the_pending_ratchet_never_turns_backwards` 里那条**地板**。
        // 钉在这里，是为了不让谁把这一支改成 panic 之后顺手把那条地板删掉 ——
        // 那样一来两格并成一格，而并完之后**没有任何输出会变**。
        assert_eq!(
            ratchet_backslide(usize::MAX, &[]),
            None,
            "空历史这一支归**地板**管，不归这把尺子管；两格刻意分开，别并"
        );

        // 解析器那一半：针是运行时拼的，拿它自己拼出来的文本正反各喂一遍。
        let synthetic = format!("    {}{};\n", ceiling_needle(), 7);
        assert_eq!(
            ceiling_in(&synthetic),
            Some(7),
            "解析器认不出自己那根针拼出来的声明"
        );
        assert_eq!(
            ceiling_in("没有这根针的一段文本"),
            None,
            "认不出就要回 None，不许默默当 0"
        );

        let table = format!(
            "    {}\n        \"a.rs\",\n        \"b.rs\",\n    ];\n",
            pending_needle()
        );
        assert_eq!(pending_count_in(&table), Some(2), "表里两行，数不出 2");
        assert_eq!(
            pending_count_in(&format!("    {}\n    ];\n", pending_needle())),
            Some(0),
            "空表要回 Some(0)，与「找不到那张表」（None）**不是一回事**"
        );
        assert_eq!(
            pending_count_in("没有那张表的一段文本"),
            None,
            "找不到表就回 None"
        );
    }
}
