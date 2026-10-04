//! 各条源码扫描型守卫共用的「只留生产段」剥法。
//!
//! # 为什么是一个共享 crate（U8a-2a，2026-08-02）
//!
//! 这份剥法原先住在后端的 `guard_support`（`cfg(test)` 模块）。monitor 侧够不着它，
//! 于是 monitor 的守卫各自写了**便宜近似**，最常见的一种是：
//!
//! ```text
//! let prod = src.split("\n#[cfg(test)]").next().unwrap_or(src);
//! ```
//!
//! 它只在「文件里第一个测试模块之后再没有生产代码」时才是对的。`stream_source/` 的
//! 第一个测试模块在 804 行，而 `parse_frame` 在 1771 行 —— 拿那个近似去扫它，
//! **扫描面直接归零到前 803 行**，守卫静默变瞎。
//!
//! 「抽取面画小了」这一族在 unified-backend 工作区里已经出现过四次。本 crate 是它的收口：
//! 两侧用**同一份**剥法，backend 的 `guard_support` 改为再导出。
//!
//! # ⚠ 与三个兄弟 crate 的一处**不同**，别照抄错家族不变量
//!
//! `branch-core` / `usage-core` / `acct-core` 的头注都写着「无 IO、无平台依赖」。
//! 本 crate 零依赖、平台无关，但 [`assert_tree_strips_clean`] **做真的 `read_dir` 遍历**，
//! 而且以 panic 为错误模型 —— 因为它只供 `cfg(test)` 里的守卫调用（两侧都是
//! `[dev-dependencies]`，不进任何发布二进制）。新增函数请守住这条边界：
//! **纯文本处理放这里，需要 IO 的只能是守卫断言型（panic 语义、只在测试里跑）。**
//!
//! 🔴 **而 IO 里的「写」这一半，本 crate 一处都不许有 —— 连 `#[cfg(test)]` 里也不许。**
//! 看着它的是后端侧的
//! `readonly_guard::g6_dependency_signoff::the_clean_verdict_is_re_measured_on_the_tree_every_run`：
//! 签字表把本 crate 判成「已量 · 未见写面」，而它**按原文行**重扫（`src.lines()`，
//! **不走 `production_code`**）⇒ 测试夹具里一句 `std` 的写盘调用就会让后端那一格当场红。
//! ⚠ **连这段散文自己都要小心**：那把尺子是**纯字面**的，`///` 里逐字写出那几个写盘 API 的名字
//! **一样命中**（09-12 现打过一次：本段第一版把其中一个写进了句子里，backend 那格逐字点名的正是这一行）
//! ⇒ 本段刻意只说「写盘调用」，一个 API 名都不写。要看今天有哪几个，读那两张模式表本身。
//! 〔`K-R75` 09-12 现打：一条需要真目录的树遍历判据写在这里，门禁 `backend` 那格 `685/1`；
//!  处置是把那条判据**搬去 monitor 侧的 `structural_scan.rs`**（就挨着
//!  monitor 那条树遍历；它的名字与死值验写在它自己的头注里 —— 这里刻意不复述那个名字：
//!  那份文件把自己从死名判据的语料里摘掉了，在这儿点它的名字会被读成一个不存在的符号），
//!  **不是**去动那把尺子。〕
//!
//! # 为什么剥法长这样（backend 侧的两次实测教训，原样保留）
//!
//! 原先**八处**各写一份：
//!
//! ```text
//! let marker = "\n#[cfg(test)]\nmod tests";
//! let prod = match src.find(marker) { Some(i) => &src[..i], None => src };
//! ```
//!
//! 它有**两个**独立的坑，而且互相掩盖：
//!
//! 1. **锚点写死了模块名 `tests`。** backend `main.rs` 的测试模块叫 `mod stream_flag_tests`
//!    ⇒ 匹配不上 ⇒ `None` 分支 ⇒ **整个文件（含测试段）被当成生产段扫**。
//!
//! 2. **「第一个锚点之后全砍」这个形状本身就是错的。** 它假定测试模块是文件里最后一样
//!    东西。`main.rs` 不是：`mod stream_flag_tests` 在 182–247 行，而**真正的子命令
//!    dispatch 在 275–291 行，在它后面**。
//!
//! ⇒ 只把坑 1 修掉（放宽锚点）会**当场引爆坑 2**。所以本模块的剥法是
//! 「**逐个剥掉每个 `#[cfg(test)]` 模块**」，不是「第一个之后全砍」。
//!
//! # 🔴「剥法认不出的写法」这一族 —— 已经三形，而**头注不是守门人**
//!
//! 上一节记着第一形（无花括号体）。09-12 又逮到第二形：**可见性前缀**
//! （`pub(crate) mod tests {` 穿过 `starts_with("mod ")`）。两形的形状不同，
//! **病是同一个**：判定按「字面前缀」认那一行 ⇒ 下一形照样穿得过去。
//!
//! ⇒ 本轮做了两件事，**第二件才是这一族的人数**：
//!
//! 1. 判定改成认**形状**（[`strip_visibility`]：`pub` ＋ 可选的一对配平括号，
//!    那是 Rust `Visibility` 文法定死的闭集）—— 不是再补一张前缀表。
//! 2. 反向自检补上第二半（[`assert_no_unstripped_test_module`]）：
//!    生产段里残留「测试期 `cfg` ＋ 带花括号体的 `mod`」就**当场红并点名那份文件**，
//!    而它的匹配单位（完整的词 `mod`）**与剥法的匹配单位不同** ⇒ 剥法的盲区不是它的盲区。
//!
//! ⚠ 第一形当年也是「写在头注里」的，写完之后第二形照样发生了 ——
//! **写下来的边界不是判据**（`production_code` 头注那句「三次同族」说的就是这件事）。
//!
//! # 自指陷阱（本仓连踩五次，别再踩）
//!
//! 锚点里的换行**必须**用转义写法（源码里是反斜杠 + `n` 两个字符），这样它与真正的换行
//! 不相等 ⇒ 扫源码时不会匹配到本行自己。同理，判「剥干净没」用的那个属性名要**运行时拼**。
//!
//! # ⚠ 它服务哪条要求：**没有，它是量具不是判据**
//!
//! 本 crate 守的不是任何产品性质，是**别的判据不许变瞎**（上面那段的读数：便宜近似把
//! `stream_source/` 的扫描面砍到前 803 行）。⇒ **给它点一条业务要求会是一句假话。**
//!
//! 它的方法学住址是（地板挡不住静默缩水）那一族。
//! 而与它配套的**负对照**形状值得推广（＋ `P28`）：
//! 每条源码扫描型守卫都该有一条「剥法没把我要扫的那一段剥掉」的判据 ——
//! `stream_source::write_half_guard` 的第一条就是那个样子。
//! 🔴 **那一形已经抽成原语了**：[`assert_stripper_keeps`]。
//! 它比活样本多买一件（锚点必须被便宜近似丢掉 ⇒ 对照不许被填成恒真的），
//! 少买一件（活样本那个字节数倍数刻意没抽上来 —— 那是一份文件的数）。
//! 人群怎么枚举、今天补到了哪几条、哪几条还欠着，逐字住 `P28` 的交付记录。

/// 一条 `#[cfg(…)]` 属性是不是**测试期专属**。
///
/// # 为什么不能只认 `#[cfg(test)]` 这一种写法（U8a-2a 实测，坑 1 的变种）
///
/// monitor 的 `session_map.rs` 里有
/// `#[cfg(all(test, target_os = "linux"))] mod linux_liveness { … }`（U7d 加的）。
/// 只认逐字 `#[cfg(test)]` 的锚点匹配不上它 ⇒ 那 5 个 `#[test]` 会留在「生产段」里 ——
/// 又是一次「扫描面画错」。新加的 monitor 全树自检第一次跑就把它逮出来了。
///
/// 判据 = 属性里出现 `test` 这个**独立标识符**（前后都不是标识符字符或 `-`）。
/// 于是 `cfg(test)` / `cfg(all(test, …))` / `cfg(any(test, …))` 都算，
/// 而 `cfg(all(unix, not(target_os = "linux")))` 不算、`cfg(feature = "test-utils")` 也不算。
fn cfg_is_test_only(attr: &str) -> bool {
    let b = attr.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'-';
    attr.match_indices("test").any(|(k, _)| {
        let before_ok = k == 0 || !ident(b[k - 1]);
        let after = k + 4;
        let after_ok = after >= b.len() || !ident(b[after]);
        before_ok && after_ok
    })
}

/// 剥掉源码里**每一个带花括号体的** `#[cfg(test)] mod X { … }` 块，其余原样保留。
///
/// 收尾判据 = **列 0 的右大括号**（换行紧跟一个右大括号）。测试模块是顶层 item，rustfmt 保证它的收尾大括号
/// 在列 0，而块内任何嵌套大括号都是缩进的。**刻意仍然不做完整的大括号配对** —— 变的只是
/// 「在哪份文本上找那个列 0 的 `}`」：[`test_module_ranges`] 09-13 起在
/// [`mask_all_literals`] 的**词法掩码**上找它（串 / 原始串 / 注释 / 字符字面量一律抹掉），
/// 切片仍然切原文。
///
/// # 🔴 上一版这里写着的那句话**已被证伪，原样留在这里当账**〔`K-R110`，09-13〕
///
/// 逐字是：「列 0 判据在两侧的文件上**实测干净**（backend 侧 `every_backend_file_strips_clean`、
/// monitor 侧 `every_monitor_file_strips_clean`）」。**两半都不成立**：
/// `src/backend/plugin/mod.rs` 的测试段里有一段 `r#"…"#`，内容里逐字有一行
/// `}"#;` ⇒ 区间在**第 657 行**收了尾，而真收尾在 688 行 ⇒ **31 行**测试代码漏进生产段；
/// 而那两条被点名的判据**一条都没红** —— 漏出去的那 31 行里测试属性恰好 0 个、`mod` 行 0 行。
/// ⚠ 同族第几次不必再数，形状是同一个：**「写下来的边界」不是判据**。
/// ⇒ 本轮的处置是两件事：① 锚点搬到词法掩码上找；
/// ② 补 [`assert_test_module_ranges_are_brace_balanced`]（匹配单位换成**计数**，
/// 与剥法的「第一处命中」不同 ⇒ 剥法的盲区不是它的盲区）。
///
/// # ★ 必须先确认那一行以左大括号收尾（本函数第一版栽在这里，Phase D 审计当场逮出）
///
/// `#[cfg(test)]` 底下也可能是一条**无花括号体的模块声明**：
///
/// ```text
/// #[cfg(test)]
/// mod guard_support;      ← 就是这份剥法自己在 backend main.rs 里的声明
/// ```
///
/// 锚点照样匹配，但这里没有块可剥 —— 于是「列 0 的右大括号」会一路找到**下一个顶层 item 的
/// 收尾大括号**，把中间**全部生产代码**当测试段吞掉。实测：加上这条声明后，
/// backend `main.rs:26–179` 整段消失（`const BUILD_ID` / `const CAPABILITIES` / `const EMITS` /
/// `fn split_stream_flags` 全没了），而 `no_timer_guard` 在那一段上**静默变瞎** ——
/// 把 `thread::sleep` 放进被吞区间**全绿**，放到测试模块之后才红。
///
/// 讽刺的是这正是本模块要消灭的那类 bug，且它**由本模块的引入本身**制造。
/// ⇒ 匹配到锚点后必须看那一行：以左大括号收尾才是模块体、才剥；否则原样保留、跳过继续找。
pub fn production_source(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    for (start, end) in test_module_ranges(src) {
        out.push_str(&src[i..start]);
        i = end;
    }
    out.push_str(&src[i..]);
    out
}

/// 剥掉一行开头的**可见性修饰**，返回其后的部分；没有修饰就原样返回。
///
/// # 🔴 它认的是**形状**，不是一张前缀表〔`K-R75`，09-12〕
///
/// [`test_module_ranges`] 的前一版判定逐字是 `mod_line.starts_with("mod ")` ——
/// 于是 `pub(crate) mod tests {` 穿了过去，那份文件**整段测试代码留在生产段里**。
/// `K-R74` 开发中间实打：`stream_source/` 的测试模块加一个 `pub(crate)` 前缀，
/// monitor 判定行 `1403 passed; 0 failed` 当场变成 `1399 passed; 10 failed`。
///
/// **修法不是再列一张前缀表** —— 那是把同一个病换个写法再犯一次
/// （同族第一形「无花括号体」就写在 [`test_module_ranges`] 的分支注释里）。
/// Rust 的 `Visibility` 文法本身就是一个**语言定死的闭集**，逐字只有两种形状：
///
/// 1. `pub`
/// 2. `pub` ＋ **一对配平的括号**（`(crate)` · `(self)` · `(super)` · `(in <路径>)`）
///
/// ⇒ 本函数认的就是那个形状。`pub(in crate::a::b::c…)` 的路径**任意长**，
/// 前缀表穷举不了，而形状认得出 —— 判据 `a_test_module_is_recognised_whatever_its_visibility`
/// 正是拿这一档把「换成前缀表」这条退路钉死的。
///
/// ⚠ **不认的一律原样退回**（宁可不剥，不许乱切）：`pub` 后面紧跟标识符字符
/// （`pubsub`）、括号不配平（`pub(crate mod x {`）都当作「这里没有可见性修饰」。
///
/// # 为什么是 `pub`（＝这一族**只许有一个权威源**，本仓 `E3`）
///
/// 「一行 item 声明前面那截可见性修饰」在本仓不止一个消费者：除了本 crate 的剥法，
/// monitor 侧还有判据靠「哪一行是测试模块的开头」来划自己的人群
/// （`structural_scan.rs` 里那条「每个测试 `fn` 都得真的带属性」）。
/// 那一处原先也写着 `starts_with("mod ")` —— **同一个病的第二个住址**，
/// 而它的失效方向更阴：认不出 ⇒ 那份文件**整份掉出人群**，判据照样绿。
/// ⇒ 09-12 一并接到这一份上来，别再各写一份近似的。
pub fn strip_visibility(line: &str) -> &str {
    let Some(rest) = line.strip_prefix("pub") else {
        return line;
    };
    let after = rest.trim_start();
    // `pub` 之后必须是空白或 `(`。两样都不是 ⇒ `pub` 只是某个标识符的开头。
    if after.len() == rest.len() && !rest.starts_with('(') {
        return line;
    }
    let Some(inner) = after.strip_prefix('(') else {
        return after;
    };
    let mut depth = 1usize;
    for (k, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return inner[k + c.len_utf8()..].trim_start();
                }
            }
            _ => {}
        }
    }
    line
}

/// 每个「带花括号体的 `#[cfg(test)] mod X { … }`」在 `src` 里的字节区间。
///
/// [`production_source`] 与 [`test_source`] **共用这一份判定** —— 它们是同一个事实的
/// 两半，各写一份迟早漂（本仓 E3：一个事实恰好一个权威源）。
/// 判定规则与它们各自的头注一致，改这里之前先读那两段。
///
/// ⚠ 可见性修饰由 [`strip_visibility`] **按形状**剥掉（`K-R75`）——
/// 别把那一步改回 `starts_with("pub(crate) mod ")` 这类前缀比对。
///
/// ⚠ 两个锚点都在 [`mask_all_literals`] 的**词法掩码**上找（`K-R110`，09-13）——
/// 别把它改回在裸 `src` 上找：那正是「区间在原始字符串中间收尾」那个活体的成因。
/// 守着这一条的是 [`assert_test_module_ranges_are_brace_balanced`]。
fn test_module_ranges(src: &str) -> Vec<(usize, usize)> {
    // 转义写法 ⇒ 与真正的换行不相等 ⇒ 不会匹配到本行自己。
    let open = "\n#[cfg(";
    let close = "\n}";
    // 🔴 两个锚点都在**词法掩码**上找，切片仍然切**原文**〔`K-R110`，09-13〕。
    //    掩码等长 ⇒ 偏移一一对应；掩码抹掉的都是**不是代码**的字节
    //    ⇒ 候选只会变少，收尾只会落在同一处或更靠后（偏向写在 `mask_all_literals` 头注里）。
    //    兜底（词法崩了）⇒ 退回裸文本，行为退回 09-13 之前那一档，而不是当场乱切。
    let masked = mask_all_literals(src);
    let hay: &str = masked.as_deref().unwrap_or(src);
    let mut out = Vec::new();
    let mut i = 0usize;
    loop {
        let Some(rel) = hay[i..].find(open) else {
            return out;
        };
        let j = i + rel;
        let line_end = |from: usize| {
            src[from..]
                .find('\n')
                .map(|k| from + k)
                .unwrap_or(src.len())
        };
        // 属性那一行（不含前导换行）。
        let attr_start = j + 1;
        let attr_end = line_end(attr_start);
        // 紧接着的那一行必须是 `[可见性] mod X {` 才是「带花括号体的测试模块」。
        let mod_start = (attr_end + 1).min(src.len());
        let mod_end = line_end(mod_start);
        let mod_line = src[mod_start..mod_end].trim();
        let is_test_mod = cfg_is_test_only(&src[attr_start..attr_end])
            && strip_visibility(mod_line).starts_with("mod ")
            && mod_line.ends_with('{');
        if !is_test_mod {
            // 不是测试模块（非 test 的 cfg / 无花括号体的 `mod x;` 声明 / cfg 挂在别的 item 上）
            // ⇒ 不成区间，从属性行之后继续找。
            //
            // ★「无花括号体」那一条是 Phase D 审计逮出来的：`#[cfg(test)] mod guard_support;`
            //   若被当成模块体，「列 0 的右大括号」会一路吞到下一个顶层 item 的收尾，
            //   把中间**全部生产代码**当测试段丢掉（backend main.rs 曾整段 26–179 行消失）。
            //
            // ★★ 第二形（`K-R75`，09-12）：**带可见性前缀的模块声明**。上一版这里逐字写着
            //   `mod_line.starts_with("mod ")` ⇒ `pub(crate) mod tests {` 从这个 `continue`
            //   走掉，整段测试代码留在生产段里。今天由 `strip_visibility` **按形状**接住。
            //   ⇒ 这一族的守门人是 `assert_no_unstripped_test_module`（下一形出现时它会点名那份文件）。
            i = attr_end;
            continue;
        }
        match hay[j..].find(close) {
            // 没收尾 ⇒ 文件结束前都算测试段。
            None => {
                out.push((j, src.len()));
                return out;
            }
            Some(rel_end) => {
                let end = j + rel_end + close.len();
                out.push((j, end));
                i = end;
            }
        }
    }
}

/// [`production_source`] 的**补集**：只留测试段。
///
/// 谁需要它：以「判据本身」为对象的元判据（F23 的裸遍历棘轮、F24 的匹配单位棘轮）——
/// 它们要扫的恰恰是别人的 `#[cfg(test)]` 里写了什么。
///
/// ⚠ 用它之前先想清楚**为什么不是整份文件**：拿整份文件扫，生产代码里的同形写法会混进来，
/// 而生产代码里那些多半是正常的（`watcher.rs` 的 `read_dir` 是它的本职）。
pub fn test_source(src: &str) -> String {
    let mut out = String::new();
    for (start, end) in test_module_ranges(src) {
        out.push_str(&src[start..end]);
    }
    out
}

/// 把一行里的**字符字面量**（`'x'` / `'\n'` / `'\u{2028}'`）换成等长的 `_`。
///
/// 只服务于 [`strip_trailing_comments`] 的状态机：一个 `'"'` 会把「现在在不在字符串里」
/// 这件事整段带偏（此后所有 `//` 都被当成字符串内容 ⇒ 剥法静默失效）。
/// 等长替换 ⇒ 下标与原行一一对应，切的时候切**原行**。
///
/// ⚠ 生命周期（`&'a str` / `'static`）刻意不匹配：它们没有收尾引号。
fn mask_char_literals(line: &str) -> String {
    let b = line.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'\'' {
            // `'\X'` / `'\u{…}'`：转义符在 i+2，收尾引号从 i+3 起找。
            if i + 3 < b.len() && b[i + 1] == b'\\' {
                if let Some(k) = b[i + 3..].iter().position(|&c| c == b'\'') {
                    let end = i + 3 + k;
                    out[i..=end].fill(b'_');
                    i = end + 1;
                    continue;
                }
            }
            // `'c'`（c 可能是多字节 —— 整个字符一起替，UTF-8 才不会被切碎）。
            if let Some(c) = line[i + 1..].chars().next() {
                let cl = c.len_utf8();
                if c != '\'' && c != '\\' && b.get(i + 1 + cl) == Some(&b'\'') {
                    out[i..=i + 1 + cl].fill(b'_');
                    i = i + 2 + cl;
                    continue;
                }
            }
        }
        i += 1;
    }
    String::from_utf8(out).expect("只把整字符替成 `_`，不会切碎多字节 ⇒ 仍是合法 UTF-8")
}

/// **剥掉行尾注释**（`//` 到行末），字符串字面量安全。
///
/// # 它治的洞〔`K-H2b` 第九轮 `R9M7` 实测 · `K-R3` 09-01 复现在退出臂上〕
///
/// [`production_code`] 原先只剔**整行**注释（那一行 `trim_start` 之后以 `//` 打头）。
/// 于是**任何行尾注释都能把任意文本带进「生产文本」**：
///
/// ```text
/// let _ = h.current_pid(); // h.stop();
/// ```
///
/// 生产上**再也不会**收掉那条后端，而所有 `contains(".stop()")` 型判据照样绿。
/// 09-01 在本仓退出臂上现打：这一刀装上去，**门禁七格一个数不动、`GATE: OK`**。
///
/// # 为什么不是「按第一个 `//` 截断」
///
/// [`strip_comment_lines`] 的头注逐字写着它**刻意不剥行尾注释**，理由是
/// 「按第一个 `//` 截断会砍坏字符串字面量里的 `//`（`"http://host"`）」。
/// **那个理由只否决了『按第一个 `//` 截断』这一种实现，不否决这件事本身。**
/// 本函数带一个最小状态机，只在**不在字符串里**的位置切 ⇒ `"http://host"` 原样保留。
///
/// # 🔴 保守边界（宁可留洞，不许造假红）
///
/// 下面三种情形**整行不动**，它们上面的洞仍然开着 —— 写出来，别当作已经关干净：
///
/// 1. 行里出现 raw / byte string 的开头（`r"` · `r#` · `b"`）；
/// 2. 上一行的字符串没收口（跨行字符串**里面**的行）；
/// 3. 引号在本行内不配平时，`//` 落在「字符串里」那一侧 ⇒ 不切。
///
/// 行数**不变**（[`pin_line`] 的行号语义不许动），只是行可能变短。
pub fn strip_trailing_comments(src: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_str = false;
    for raw in src.split('\n') {
        let masked = mask_char_literals(raw);
        if masked.contains("r\"") || masked.contains("r#") || masked.contains("b\"") {
            // raw / byte string：这份剥法不解析它，且状态不可信 ⇒ 原样留下、状态归零。
            out.push(raw.to_string());
            in_str = false;
            continue;
        }
        let mb = masked.as_bytes();
        if in_str {
            // 本行在字符串里 ⇒ 一个字都不切，只把「收没收口」扫出来。
            let mut i = 0usize;
            while i < mb.len() {
                if mb[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if mb[i] == b'"' {
                    in_str = false;
                    break;
                }
                i += 1;
            }
            out.push(raw.to_string());
            continue;
        }
        let mut i = 0usize;
        let mut cut: Option<usize> = None;
        while i < mb.len() {
            if in_str {
                if mb[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if mb[i] == b'"' {
                    in_str = false;
                }
                i += 1;
                continue;
            }
            if mb[i] == b'"' {
                in_str = true;
                i += 1;
                continue;
            }
            if mb[i] == b'/' && mb.get(i + 1) == Some(&b'/') {
                cut = Some(i);
                break;
            }
            i += 1;
        }
        match cut {
            // `cut` 指向 `/`，那一定是字符边界 ⇒ 切原行安全。
            Some(c) => out.push(raw[..c].to_string()),
            None => out.push(raw.to_string()),
        }
    }
    out.join("\n")
}

/// 一处 `r"…"` / `r#"…"#` / `b"…"` / `br#"…"#` 的**开头**在不在 `sb[i]`。
///
/// 返回 `Some((井号个数, 开头一共几个字节))`；`None` = 这里不是原始/字节串的开头。
///
/// ⚠ **`r#type` 那种原始标识符不算**：本函数要求井号之后**紧跟一个引号**，
/// 而 `r#type` 后面跟的是字母 ⇒ 返回 `None`。少了这一条，任何一个 `r#fn` / `r#match`
/// 都会被当成一个永不收口的字符串，把它后面**整份文件**变成「字符串里面」。
///
/// ⚠ 前一个字符是标识符字符时也不算（`var"` 里那个 `r` 是变量名的一部分，不是前缀）。
fn raw_string_open(sb: &[u8], i: usize) -> Option<(usize, usize)> {
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    if i > 0 && ident(sb[i - 1]) {
        return None;
    }
    // `b"…"` 是字节串，没有井号，行为与普通串一致；`br…` 与 `r…` 一样往下走。
    let mut k = i;
    if sb.get(k) == Some(&b'b') {
        k += 1;
        if sb.get(k) == Some(&b'"') {
            return Some((0, k + 1 - i));
        }
    }
    if sb.get(k) != Some(&b'r') {
        return None;
    }
    k += 1;
    let hash_start = k;
    while sb.get(k) == Some(&b'#') {
        k += 1;
    }
    if sb.get(k) != Some(&b'"') {
        return None;
    }
    Some((k - hash_start, k + 1 - i))
}

/// **把块注释 `/* … */` 的内容抹成空格**（等长替换，行数与字节数都不变）。
///
/// 剥不动时返回 `None` —— 见下面「兜底」那一节。这是 [`strip_block_comments`] 与
/// [`block_comment_model_holds`] 共用的那一份实现（E3：一个事实一个权威源）。
///
/// # 它治的洞〔`K-R9`，09-04〕
///
/// [`production_code`] 此前只剥**整行 `//`**（`starts_with("//")`）与**行尾 `//`**
/// （`K-R3` 09-01 补的），**块注释一个字都不剥**。于是把一段真代码包进
/// **多行块注释**、再换一个桩值，所有源码形态判据照样绿：
///
/// ```text
/// /*
/// if g.is_some() {
///     return StartOutcome::AlreadyRunning;
/// }
/// */
/// let _shim = g.is_some();
/// ```
///
/// 09-04 在本仓 `local_backend_host.rs` 的幂等门上现打过这一刀：monitor 侧
/// **1275 条测试全绿、0 失败**，而生产上「点两下起出第二个后端」那个阻塞级缺陷回来了 ——
/// 连**专门为它写的**那条判据（`starting_twice_does_not_spawn_a_second_local_backend_host`）
/// 都在数块注释里的那份文本。
///
/// ⚠⚠ **而多行块注释正是编辑器「注释掉这几行」的默认产物** —— 这不是一个刁钻的绕法。
///
/// # 判据（逐条对着「剥狠了会误伤」那三种情形写）
///
/// 单趟从左往右扫，状态 = 码 / 普通串 / 原始串 / 行注释 / 块注释（带深度）：
///
/// 1. **字符串字面量里的 `/*` 不是注释**。`"a/*b"` 里那两个字节在**串**状态下读到
///    ⇒ 不开块。`'"'` 这种字符字面量先过 [`mask_char_literals`]（与
///    [`strip_trailing_comments`] 同一把），否则一个 `'"'` 会把此后的串状态整段带偏 ——
///    实测：`shared_crate_registry_tests.rs::shared_crate_names` 里的 `.trim_end_matches('"')`
///    正是这一形，不掩码的话该文件扫到末尾深度会停在 **5**。
/// 2. **嵌套块注释**（Rust 允许）：`/* a /* b */ c */` 靠 `depth` 加减吃干净，
///    **不是**遇到第一个 `*/` 就收口。只认第一个 `*/` 会把 ` c */` 当代码吐回来。
/// 3. **`/**` 与 `/*!` 文档注释**：它们就是「`/*` + 内容」，同一条规则吃掉。
///    剥它们是**对的**且与既有行为一致 —— `///` / `//!` 早就被剥了，
///    块形的文档注释同样是散文，留着它就是留着喂饱判据的那口食。
///    （`/**/` 与 `/***/` 这两个退化形也按同一条规则收口，见测试。）
///
/// 另外两条是「别把不是注释的东西当注释」：
///
/// 4. **行注释里的 `/*` 不开块**：`// 见 /* 这里`。扫到 `//` 就**跳到行尾**，
///    那之后的 `/*` 根本不进眼。少了这一条，一句解释性散文就能把它后面整份文件吃掉。
/// 5. **原始串 / 字节串按真语法跟踪**（`r"…"` · `r#"…"#` · `b"…"` · `br#"…"#`，
///    井号个数配平、可跨行）。⚠ 这一条是**现打出来的**，不是防御性编程：
///    `src/comms/outward/server.rs` 的 `STUB_LAUNCHER` 是一段 shell，
///    里面逐字有 `hostport=${rest%%/*}` 与 `path=/${rest#*/}` —— 一个 `/*` 一个 `*/`。
///    按 [`strip_trailing_comments`] 那种「含 `r#` 的**那一行**整行不动」的便宜办法，
///    跨行原始串**内部**的行照样被当代码扫 ⇒ 那 18 个字节会被当块注释抹掉，
///    而它们是**真的字符串内容**。那就是「拿假阳换真阳」，铁律 18 禁的正是这个。
///
/// # 🔴 兜底：模型崩了就**一个字都不剥**（宁可留洞，不许造假红）
///
/// 扫完整份之后 `depth` 不为 0、或还停在某个字符串里 ⇒ 说明上面这套词法与这份文本
/// **对不上**（能编译的 Rust 一定两边都收口）⇒ 返回 `None`，调用方原样把 `src` 交回去。
///
/// ⚠ **这条兜底会静默地把洞重新打开**，所以它不许只活在这段散文里：
/// [`assert_block_comment_model_holds`] 把「今天有几处走了兜底」变成一条判据。
/// **别把这条边界读成「兜底永远不会触发」。**
///
/// # 🔴 兜底触发不触发，**由交进来的那段文本是什么单位决定**〔`K-R25`，09-04〕
///
/// 「能编译的 Rust 一定两边都收口」这句话的分母是**整份文件**。
/// 把整份文件**切小**再交进来（按 `#[test]` 切块 · 函数体窗口 · 任意切片），
/// 那一小段就**不一定**配平了：`/*` 留在这一段里、`*/` 落在段外
/// ⇒ 扫完 `depth != 0` ⇒ 兜底 ⇒ 这一段上的洞开着，而**整份文件那一档照旧配平**。
/// ⇒ 上一版看门判据只按整份文件喂 ⇒ 那一形它一个字都看不见
/// （`K-R9` 落定拍实打：看门判据绿 · 被喂的那条守卫判「合规」· 全量 monitor 全绿）。
///
/// ⇒ **两条纪律**（`K-R25`）：
/// ① 调用方**先剥整份、再切块**，别反（次序的理由写在 [`test_attr_chunks`] 头注里）；
/// ② 看门判据把「块」也当一个单位去量（现在它两个单位各判一遍）。
/// ⚠ 它量的是**那两个单位**，不是「所有单位」—— 别的单位逐处登记在
/// `tests/evidence/K-R25-D2-unit-alignment.md`。
fn try_strip_block_comments(src: &str) -> Option<String> {
    scan_and_blank(src, false)
}

/// 把**块注释 ＋ 行注释 ＋ 字符串/原始串/字节串的内容 ＋ 字符字面量**一律抹成**等长空格**。
/// 剥不动时 `None`（与 [`try_strip_block_comments`] **同一条兜底、同一台状态机**）。
///
/// # 它为什么存在〔`K-R110`，09-13〕
///
/// [`test_module_ranges`] 的收尾针是「列 0 的右大括号」，而它原先在**裸文本**上找 ——
/// 于是测试模块里一段 `r#"…"#` 的**内容**里出现列 0 的 `}`，那一段就在字符串中间收了尾。
/// 09-13 现打：全仓 git 跟踪的 `.rs` **235** 份里**恰好 1 份**踩上
/// （`src/backend/plugin/mod.rs`，`}"#;` 那一行）——
/// **31 行测试代码漏进生产段、同样这 31 行从测试段里少掉**，而两条反向自检都看不见它
/// （那 31 行里测试属性恰好 0 个、`mod` 行 0 行）。
///
/// # 🔴 新针的偏向：它只会让收尾**往后挪**，不会往前
///
/// 掩码只做一件事：把**不是代码**的那些字节抹掉。⇒ 候选的列 0 `}` 只会**变少**，
/// 收尾只会**落在同一处或更靠后**。
/// - 往后挪到**真正的**收尾 ⇒ 这正是本轮要修的。
/// - 万一词法认错、把真代码当成串/注释 ⇒ 收尾会挪过头，**测试段吞掉生产代码**
///   （这是 `production_source` 头注里记的那个老病灶的方向）。
///   看着这一档的是两侧既有的「生产段里必须还找得到某锚点」判据
///   （backend `guard_support::main_production_section_keeps_its_load_bearing_items`），
///   以及本模块的 [`assert_test_module_ranges_are_brace_balanced`]。
/// - 词法**自己**崩了（收不了口）⇒ 走兜底、退回裸文本，形状退回 09-13 之前那一档，
///   而 [`assert_block_comment_model_holds`] 与本模块那条新判据都会出声。
///
/// ⚠ **它不是「括号配平」**：收尾判据仍然逐字是「列 0 的右大括号」
/// （`production_source` 头注里那条已登记的边界一个字没改），
/// 变的只是「在哪份文本上找它」。
fn mask_all_literals(src: &str) -> Option<String> {
    scan_and_blank(src, true)
}

/// [`try_strip_block_comments`] 与 [`mask_all_literals`] **共用的那一台状态机**
/// （E3：一个事实恰好一个权威源 —— 两份近似的词法器是这一族最贵的病）。
///
/// - `blank_all == false` ⇒ 只抹块注释（09-13 之前的行为，**逐字未变**）。
/// - `blank_all == true`  ⇒ 连行注释、串内容（含定界符）、字符字面量一起抹。
///
/// 两档都**等长**（逐字节换成空格），行数与字节偏移一律不动。
fn scan_and_blank(src: &str, blank_all: bool) -> Option<String> {
    let mut out: Vec<u8> = Vec::with_capacity(src.len());
    let mut depth = 0usize;
    let mut in_str = false;
    let mut raw_hashes: Option<usize> = None;

    for (li, raw) in src.split('\n').enumerate() {
        if li > 0 {
            out.push(b'\n');
        }
        // 掩码只在「码」这个状态下做：串里 / 注释里的 `'` 是普通字符，
        // 掩了反而可能把一个真的收尾引号盖住。
        let masked;
        let scan: &str = if depth == 0 && !in_str && raw_hashes.is_none() {
            masked = mask_char_literals(raw);
            &masked
        } else {
            raw
        };
        let sb = scan.as_bytes();
        // 抹在**原行**上（掩码是等长的 ⇒ 下标一一对应）。
        let mut line: Vec<u8> = raw.as_bytes().to_vec();
        // `blank_all`：字符字面量也抹掉 —— `'{'` / `'}'` 会污染任何**按花括号计数**的下游
        // （本仓真有：`.trim_start_matches('{')`）。`mask_char_literals` 已经在 `scan` 上
        // 把它们换成了 `_`，照着位置抹到 `line` 上就行，不用第二台词法器。
        if blank_all {
            for (k, (&m, &r)) in sb.iter().zip(raw.as_bytes()).enumerate() {
                if m == b'_' && r != b'_' {
                    line[k] = b' ';
                }
            }
        }
        let mut i = 0usize;
        while i < sb.len() {
            if depth > 0 {
                if sb[i] == b'/' && sb.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    line[i] = b' ';
                    line[i + 1] = b' ';
                    i += 2;
                    continue;
                }
                if sb[i] == b'*' && sb.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    line[i] = b' ';
                    line[i + 1] = b' ';
                    i += 2;
                    continue;
                }
                // 逐**字节**抹成空格：多字节字符整段变空格，仍是合法 UTF-8，
                // 而且长度不变 ⇒ `find_pinned` 的字节偏移、`pin_line` 的行号都不动。
                line[i] = b' ';
                i += 1;
                continue;
            }
            if let Some(h) = raw_hashes {
                if sb[i] == b'"'
                    && sb.len() >= i + 1 + h
                    && sb[i + 1..i + 1 + h].iter().all(|&c| c == b'#')
                {
                    raw_hashes = None;
                    if blank_all {
                        line[i..i + 1 + h].fill(b' ');
                    }
                    i += 1 + h;
                    continue;
                }
                if blank_all {
                    line[i] = b' ';
                }
                i += 1;
                continue;
            }
            if in_str {
                if sb[i] == b'\\' {
                    if blank_all {
                        line[i] = b' ';
                        if i + 1 < line.len() {
                            line[i + 1] = b' ';
                        }
                    }
                    i += 2;
                    continue;
                }
                if sb[i] == b'"' {
                    in_str = false;
                }
                if blank_all {
                    line[i] = b' ';
                }
                i += 1;
                continue;
            }
            // ── 以下是「码」状态 ──
            if let Some((h, consumed)) = raw_string_open(sb, i) {
                // `b"…"`（无井号且不是 `br`）走普通串那条路：它认反斜杠转义。
                if h == 0 && sb[i] == b'b' {
                    in_str = true;
                } else {
                    raw_hashes = Some(h);
                }
                if blank_all {
                    let stop = (i + consumed).min(line.len());
                    line[i..stop].fill(b' ');
                }
                i += consumed;
                continue;
            }
            if sb[i] == b'"' {
                in_str = true;
                if blank_all {
                    line[i] = b' ';
                }
                i += 1;
                continue;
            }
            if sb[i] == b'/' && sb.get(i + 1) == Some(&b'/') {
                // 行注释：本行剩下的一律不看（`//` 那一半归 `strip_trailing_comments`）。
                if blank_all {
                    line[i..].fill(b' ');
                }
                break;
            }
            if sb[i] == b'/' && sb.get(i + 1) == Some(&b'*') {
                depth += 1;
                line[i] = b' ';
                line[i + 1] = b' ';
                i += 2;
                continue;
            }
            i += 1;
        }
        out.extend_from_slice(&line);
    }
    if depth != 0 || in_str || raw_hashes.is_some() {
        return None;
    }
    String::from_utf8(out).ok()
}

/// **剥掉块注释**（内容抹成等长空格，行数不变）。剥不动时**原样返回**。
///
/// 判据、三种误伤怎么处理、兜底为什么是「不剥」——**全部逐条写在
/// [`try_strip_block_comments`] 头注里**，改这里之前先读那一段。
pub fn strip_block_comments(src: &str) -> String {
    try_strip_block_comments(src).unwrap_or_else(|| src.to_string())
}

/// 这份文本上，块注释的词法模型**站不站得住**（`false` = 走了兜底、一个字没剥）。
///
/// 它存在的唯一理由是**别让兜底静默**：兜底是「宁可留洞」，而留下的洞正是
/// [`try_strip_block_comments`] 要治的那一个。⇒ 用 [`assert_block_comment_model_holds`]
/// 把它钉成判据，别只在散文里写一句「一般不会触发」。
pub fn block_comment_model_holds(src: &str) -> bool {
    try_strip_block_comments(src).is_some()
}

/// 把一份 Rust 源码按「一个 `#[test]` 到下一个 `#[test]`」切成块（**边界那几行本身不进块**）。
///
/// # 为什么它住在这儿（`K-R25`，09-04）
///
/// 本仓有两条守卫（monitor 的 `every_test_that_starts_the_real_backend_demands_a_private_tmux`
/// 与它的姊妹 `every_real_backend_e2e_demands_a_private_tmux_dir`）**先按 `#[test]` 切块、
/// 再逐块判**，而它们各写了一份**私有副本**的切法。切法是一个事实
/// ⇒ 恰好一个权威源（E3）。本函数就是那一份。
///
/// # ⚠ 它与「剥法」的先后是**承重的**，别调
///
/// 交进剥法的那段文本是什么**单位**，决定了看门判据
/// （[`assert_block_comment_model_holds`]）看不看得见它的兜底。
/// **先切块再剥** ⇒ 单位是块，而看门判据量的是文件 ⇒ 两把尺子对不上
/// （`K-R9` 落定拍现打过：`/*` 落 A 块、`*/` 落 B 块，整份配平、单块不配平，
/// rustc 眼里就是条普通跨行块注释 ⇒ A 块掉进兜底、洞开着，而看门判据**绿的**）。
/// ⇒ **正确的次序是「先剥整份、再切块」**：块注释的开合状态在整份文件上是配平的
/// （能编译的 Rust 一定配平），剥完之后每一块都干净。
///
/// ⚠ 剥完再切，落在块注释**里面**的那种 `#[test]` 行会被抹成等长空格
/// ⇒ 它不再是边界，前后两块合成一块。**那是对的**：注释里的 `#[test]` 不是一条测试。
/// 而剥法**不改行数、不改字节数**（[`try_strip_block_comments`] 等长抹空格；
/// [`strip_comment_lines`] 把整行注释换成空行），所以行位与原文仍然一一对应。
///
/// 自指：锚点**运行时拼**（否则本函数的源码自己就是一个边界）。
pub fn test_attr_chunks(src: &str) -> Vec<String> {
    // ⚠ **不在语料串上做裸 `split`**（`needle_anchor_registry` 判它「匹配单位比事实小」）：
    //   按行扫，边界是「整行 trim 之后逐字等于那条属性」，比子串确定。
    let anchor = concat!("#[te", "st]");
    let mut out: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in src.lines() {
        if line.trim() == anchor {
            if !cur.is_empty() {
                out.push(cur.join("\n"));
                cur.clear();
            }
            continue;
        }
        cur.push(line);
    }
    out.push(cur.join("\n"));
    out
}

/// 遍历一棵源码树，断言**没有一份文件、也没有一个 `#[test]` 块走块注释剥法的兜底**。
///
/// # 它看着的是什么
///
/// [`try_strip_block_comments`] 的兜底（`depth` 不为 0 / 停在串里 ⇒ 一个字都不剥）
/// 是**静默**的：那一段文本的块注释洞当场重新打开，而门禁上一个数都不动。
/// 本条把「今天有几处走兜底」变成读数。
///
/// # 🔴 两个单位，而这正是本条 `K-R25`（09-04）改的那一格
///
/// 上一版**只按整份文件**喂。而剥法真正被喂的单位由调用点决定：有调用点
/// **先按 `#[test]` 切块、再逐块剥**（[`test_attr_chunks`] 头注写着是哪两条）。
/// ⇒ 「整份配平、单块不配平」那一形，上一版**一个字都看不见**
/// （`K-R9` 落定拍实打：本条绿 · 那条守卫判「合规」· 全量 monitor `1278 passed; 0 failed`）。
/// ⇒ 现在**两个单位各判一遍**：整份文件 ＋ [`test_attr_chunks`] 切出的每一块。
///
/// ⚠ **射程，写清（别读成全称）**：它看着的是**这两个单位**。
/// 别的单位 —— 函数体窗口（`body_of(..)` / `brace_block(..)`）· 任意切片 `&src[a..b]` ·
/// `.lines().take(n)` 行窗口 —— **它一个都看不见**。今天全仓这样的调用点逐处点了名，
/// 数与住址在 `tests/evidence/K-R25-D2-unit-alignment.md`（量具 `tests/evidence/K-R25-D1-strip-input-unit-census.py`）。
/// **别把「两个单位」读成「所有单位」。**
///
/// `min_files` / `min_blocks` 与 [`assert_tree_strips_clean`] 的 `min_files` 同职：
/// 扫到的数低于它说明**遍历（或切法）坏了**，不是代码变干净了。
/// ⚠ **块数不是文件数**，两个地板各自量、各自报 —— 这是 `K-R25` `KR25D2` 那格前置问题
/// 「块级看门的分母怎么报」的答案：不换尺子就没法读，所以两个数一起印。
///
/// # Panics
///
/// 目录 / 文件读不了、有文件或有块走了兜底、或扫到的文件数 `< min_files`
/// / 块数 `< min_blocks` 时 panic（守卫语义，只在测试里调）。
/// # 🔴 `roots` 是**一串**树，不是一棵〔搬树 2026-09-18 ·  纪律 3〕
///
/// 仓库重组之后，「一个半边的源码」**不再是一棵树**：生产段住 `src/<半边>`、
/// 测试段住 `tests/<半边>`。而本条量的两个单位里，**块那一个单位整个住在测试段**
/// ⇒ 只喂 `src/` 那一棵，`min_blocks` 那条地板会从 1 200 掉到 412，
/// 而掉下去之后「没有文件走兜底」这个结论是**在四分之一的语料上**得出的。
/// ⇒ 调用方**把两棵一起交进来**，地板一格都不用动。
///
/// ⚠ **`roots` 里两棵树不许互相包含**（纪律 1）：包含的话同一份文件
/// 会被数两遍，两条地板一起虚高 —— 本函数当场拒绝这种参数。
pub fn assert_block_comment_model_holds(
    roots: &[&std::path::Path],
    min_files: usize,
    min_blocks: usize,
) {
    assert!(min_files > 0, "min_files 不得为 0 —— 那等于关掉计数自检");
    assert!(
        min_blocks > 0,
        "min_blocks 不得为 0 —— 那等于关掉块级计数自检"
    );
    assert!(!roots.is_empty(), "roots 为空 —— 那等于把整条判据关掉");
    // ⚠ 先**规范化**再比包含：调用方给的住址里常带 `..`（后端那个 `tests_root()` 逐字是
    //   `src/backend/../../tests/backend`），而 `Path::starts_with` 是**按分量**比的，
    //   `..` 对它只是一个普通分量 ⇒ 不规范化的话那两棵树会被误判成互相包含。
    let norm: Vec<std::path::PathBuf> = roots
        .iter()
        .map(|r| std::fs::canonicalize(r).unwrap_or_else(|_| r.to_path_buf()))
        .collect();
    for (i, a) in norm.iter().enumerate() {
        for (j, b) in norm.iter().enumerate() {
            assert!(
                i == j || !a.starts_with(b),
                "语料根 {a:?} 是 {b:?} 的子目录 —— 同一份文件会被数两遍，\
                 两条地板一起虚高（纪律 1）"
            );
        }
    }
    let mut n = 0usize;
    let mut blocks = 0usize;
    let mut bad: Vec<String> = Vec::new();
    let mut bad_blocks: Vec<String> = Vec::new();
    let mut stack: Vec<std::path::PathBuf> = roots.iter().map(|r| r.to_path_buf()).collect();
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap_or_else(|e| panic!("读目录 {d:?} 失败: {e}"))
        {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let src =
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读 {path:?} 失败: {e}"));
            if !block_comment_model_holds(&src) {
                bad.push(path.to_string_lossy().to_string());
            }
            n += 1;
            // 🔴 第二个单位：按 `#[test]` 切出的块。**整份配平不等于单块配平** ——
            //    那正是 `K-R9` 落定拍逮到的那一形（`/*` 落 A 块、`*/` 落 B 块）。
            for (i, c) in test_attr_chunks(&src).into_iter().enumerate() {
                blocks += 1;
                if !block_comment_model_holds(&c) {
                    bad_blocks.push(format!("{}#块{i}", path.to_string_lossy()));
                }
            }
        }
    }
    assert!(
        n >= min_files,
        "只扫到 {n} 个 .rs 文件（期望至少 {min_files}）—— **遍历坏了**，本条此刻是空转的"
    );
    // ⚠ 属性名**运行时拼**（本 crate 头注「自指陷阱」那一条）：报文是**生产段**，
    //   直接写字面量会被 `assert_no_test_code` 数成「剥完仍残留测试属性」。
    //   〔09-04 现打：第一版就是这么红的 —— `this_crate_strips_clean` 报 `left: 2`。〕
    let attr = concat!("#[te", "st]");
    assert!(
        blocks >= min_blocks,
        "只切出 {blocks} 个 `{attr}` 块（期望至少 {min_blocks}，扫了 {n} 份文件）—— \
         **切法坏了**，本条的块级那一半此刻是空转的"
    );
    assert!(
        bad.is_empty(),
        "{} 份文件走了块注释剥法的**兜底**（一个字都没剥）⇒ 这几份上「块注释喂饱判据」\
         那个洞此刻是**开着**的：{bad:?}\n\
         ★ 兜底的判据是「扫完 depth 不为 0 / 还停在字符串里」——\
         多半是新出现了一种本剥法不认的字面量形态。先读 `try_strip_block_comments` 头注，\
         **别把这条判据删掉了事**。",
        bad.len()
    );
    assert!(
        bad_blocks.is_empty(),
        "{} 个 `{attr}` **块**走了块注释剥法的兜底（整份文件是配平的，单块不配平）\
         ⇒ 凡是**先切块再剥**的判据，在这几块上「块注释喂饱判据」那个洞此刻是**开着**的：\
         {bad_blocks:?}\n\
         ★ 典型形状：`/*` 落在一块里、`*/` 落在下一块里 —— 对 rustc 它就是一条合法的\
         跨行块注释，**编得过、整份配平**，所以文件级那一半看不见它（`K-R9` 落定拍实打）。\n\
         ★ 出路**不是**把本条这一半删掉：把那条判据的次序改成\
         **先剥整份、再切块**（`guard_core::test_attr_chunks` 头注写着为什么），\
         那样它交进剥法的单位就是整份文件、与本条量的单位对上了。\n\
         （分母：本趟扫了 {n} 份文件 · 切出 {blocks} 块 —— 块数不是文件数，两个数各自读。）",
        bad_blocks.len()
    );
}

/// `production_source` + 剥掉行注释（**整行的与行尾的都剥**）。
///
/// 剥注释是必需的：两侧的注释**大量**在解释「为什么这里没有定时器了 / 哪些写模式被
/// 禁了 / 有哪些子命令」，逐字提到那些字面量。不剥的话守卫会被**解释它自己的那段散文**
/// 喂饱（backend 的 `no_timer_guard` P4 实测被打红过；`build_id_guard` 的指纹会被注释里的
/// 子命令名污染）。
///
/// # ★ 09-01（`K-R3`）：行尾注释那一半是**后补的**，补之前它是这一族的活体洞
///
/// 原先只剔整行注释 ⇒ **行尾注释整行保留**（那一行不以 `//` 开头）
/// ⇒ 把一处真调用换成 `别的调用(); // 原调用()`，所有存在型文本判据照样绿。
/// 剥法见 [`strip_trailing_comments`]（连同它**没有**关掉的三种情形一起写在那儿）。
///
/// ⚠ 同一个函数、同一族病、**第二次**：上一次（本模块头注）治的是「自检太弱」，
/// 这次露的是「**剥法太窄**」。⇒ 加新原语时先问一句「它剥不掉的那部分，谁在看着」。
///
/// # ★★ 09-04（`K-R9`）：**第三次** —— 块注释
///
/// 09-01 补上行尾那一半之后，头注（与 `strip_comment_lines` 那一段）逐字留了这句边界：
/// 「**别把这条边界读成「注释都剥干净了」**」。那句话写下来了，**但没有人守着它**：
/// 剥法仍然只认 `//`，`/* … */` 一个字都不剥。⇒ 把真代码包进**多行块注释**再换个桩值，
/// 判据全绿（09-04 在 `local_backend_host.rs` 的幂等门上现打：monitor **1275 条全绿 0 失败**）。
///
/// ⇒ 本轮的处置**不是**再写一句边界，而是[`strip_block_comments`] 把它剥掉，
/// 并用 [`assert_block_comment_model_holds`] 看着那条兜底。
/// ★ 三次同族，教训一句话：**「写下来的边界」不是判据，只有判据是判据。**
///
/// # ★★★ 09-04（`K-R25`）：**第四次 —— 同一族病，这次长在「作用域」上**
///
/// 上一条（`K-R9`）把块注释关掉了，而**它声称守的面（文件）比它的作用域（交进来的那段文本）大**：
/// 剥法在哪个单位上跑由调用方决定，看门判据却只按整份文件量
/// ⇒ 「整份配平、单块不配平」那一形**原样复现了一次全绿**。
/// ⇒ 本拍的处置是**两条一起**：调用方先剥整份再切块（[`test_attr_chunks`]），
/// 看门判据把块也当一个单位量（[`assert_block_comment_model_holds`]）。
/// ★ 四次同族，教训再加一句：**审一条修法，要问的不是「它修好了吗」，
/// 是「它的作用域与它声称守的面是不是同一个」。**
pub fn production_code(src: &str) -> String {
    // 🔴 顺序：块注释**先**剥。整行 `//` 那道 filter 会**删行**，
    //    先删就可能把 `/*` 开头那一行删掉、只留下半截块注释。
    //    而块注释剥法自己认得 `//`（行注释里的 `/*` 不开块），先后不会互相打架。
    let no_block = strip_block_comments(&production_source(src));
    let kept = no_block
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    strip_trailing_comments(&kept)
}

/// 反向自检：剥完的文本里**不许再出现测试属性**。
///
/// 这条是本模块存在的第二个理由。原先各处的自检是 `prod.len() < raw.len()` ——
/// 它**光靠剥注释就满足**，与测试段有没有剥掉毫无关系，所以三条守卫扫了几个月的测试代码
/// 都没人发现。判据字符串运行时拼，避免命中本文件自己。
pub fn assert_no_test_code(who: &str, prod: &str) {
    let attr = format!("#[{}]", "test");
    let n = prod.matches(attr.as_str()).count();
    assert_eq!(
        n, 0,
        "{who}：剥完仍残留 {n} 个测试属性 —— 剥法坏了，此刻这条守卫在扫测试代码"
    );
}

/// 反向自检的**第二半**〔`K-R75` `KR75D2`，09-12〕：
/// 剥完的文本里不许再出现「**测试期专属 `cfg` ＋ 一个带花括号体的 `mod`**」。
///
/// # 它和 [`assert_no_test_code`] 分工在哪，为什么两条都要
///
/// [`assert_no_test_code`] 数的是残留的**测试属性**（`#[test]`）。它接得住「测试模块没剥掉
/// 而里面有测试函数」，**接不住「测试模块没剥掉而里面一个 `#[test]` 都没有」** ——
/// 而那不是假想：09-12 现打，monitor 树上就有 **3 处** `#[cfg(test)] pub(crate) mod X {`
/// （`backend_kill.rs` 的 `creation_detect` · `shared_crate_registry.rs` 的 `ci_yaml` ·
/// `write_site_registry.rs` 的 `writers`），它们全是**只给测试用的量具模块、里面没有 `#[test]`**
/// ⇒ 剥法认不出它们、它们整段待在生产段里被各条判据扫，**而反向自检一声不吭**。
///
/// # 🔴 为什么它不会和剥法一起瞎掉：**匹配单位不同**
///
/// 剥法认的是「行首（剥掉可见性之后）是 `mod `」。本条认的是「这一行里有 `mod` 这个**完整的词**、
/// 且这一行以 `{` 收尾」（[`contains_word`]）。⇒ 剥法的盲区（前缀多了点什么）**恰好不是**本条的盲区。
/// 一条把「剥法认不出的写法」这一族数出来的判据，如果和剥法共用同一种匹配单位，
/// 它就是一个**不会响的闹钟**。
///
/// # 它的射程边界，两侧都写出来
///
/// - **接得住**：`pub(crate) mod` · `pub(in …) mod` · 同一行的 `#[cfg(test)] mod x {` ·
///   属性与 `mod` 之间夹了文档注释/别的属性的写法（往下跳过空行、`//`、`#[…]` 再看）。
/// - **接不住**：① **过剥**（剥多了 —— 那一族由两侧的「生产段里必须还找得到某锚点」判据守，
///   见 backend `guard_support` 那条）；② `#[cfg(test)]` 挂在**不是 `mod` 的**花括号体上
///   （`fn` / `impl` / `enum` / `thread_local!`，09-12 现打两棵树共 **31 处**）——
///   那不是「剥法漏了」，剥法从来只认 `mod` 块；把它们也判红等于给这条守卫加一族假红。
/// - **它不数「有没有 `#[test]`」** ⇒ 与 [`assert_no_test_code`] 是并集关系，不是替代。
///
/// # Panics
///
/// 有残留时 panic，并**逐处点名**（`who` ＋ 行号 ＋ 那一行的逐字内容）。
pub fn assert_no_unstripped_test_module(who: &str, prod: &str) {
    // 自指陷阱：needle 运行时拼 ⇒ 本文件自己的生产段里不存在这个字面量。
    let cfg_open = format!("#[{}(", "cfg");
    let lines: Vec<&str> = prod.split('\n').collect();
    let mut offenders: Vec<String> = Vec::new();
    for (i, raw) in lines.iter().enumerate() {
        let t = raw.trim();
        if !t.starts_with(cfg_open.as_str()) || !cfg_is_test_only(t) {
            continue;
        }
        // 同一行就带着 item 体（`#[cfg(test)] mod x {`），或者往下找第一条**实**行。
        let (at, cand) = if t.ends_with('{') {
            (i, t)
        } else {
            let mut j = i + 1;
            while j < lines.len() {
                let s = lines[j].trim();
                if s.is_empty() || s.starts_with("//") || s.starts_with('#') {
                    j += 1;
                    continue;
                }
                break;
            }
            match lines.get(j) {
                Some(s) => (j, s.trim()),
                None => continue,
            }
        };
        if cand.ends_with('{') && contains_word(cand, "mod") {
            // ⚠ 这个序号是**生产段里的行号，不是原文行号**（调用方多半已经剥过注释 ⇒ 行会少）。
            //   真正的校验位是后面那段**逐字内容**，拿它去原文里搜。
            offenders.push(format!("{who} · 生产段第 {} 行 · 逐字 `{cand}`", at + 1));
        }
    }
    assert!(
        offenders.is_empty(),
        "{who}：生产段里残留了 {} 处**剥法没认出来的测试模块** —— 此刻这些判据在扫测试代码：\n{}\n\
         ★ 这一族已经犯过三形：① 无花括号体的 `mod x;`（backend main.rs 曾整段 26–179 行消失）\n\
         ② 可见性前缀 `pub(crate) mod`（`K-R74` 实打：判定行 1403/0 → 1399/10）③ 就是你现在看到的这一处。\n\
         ⇒ **改的是 `guard_core::test_module_ranges` 的剥法，不是这份文件的写法**；\n\
         而且**不许再列一张前缀表** —— 要认形状（`strip_visibility` 那一档是范例）。",
        offenders.len(),
        offenders.join("\n")
    );
}

// ── KR110D1 新判据：切出来的每一段，花括号必须净配平 ──
/// 反向自检的**第三半**〔`K-R110`，09-13〕：[`test_module_ranges`] 切出的**每一段**，
/// 按**词法**数的 `{` 与 `}` 必须一样多。
///
/// # 它和另外两半分工在哪，为什么非得有第三条
///
/// [`assert_no_test_code`] 数残留的**测试属性**；[`assert_no_unstripped_test_module`]
/// 数残留的「测试期 `cfg` ＋ 带花括号体的 `mod`」。**两条都接不住「区间在字符串中间收了尾」**：
/// 09-13 现打的活体（`plugin/mod.rs`）里，漏出去的那 31 行**测试属性恰好 0 个、`mod` 行 0 行**
/// ⇒ 两条都一声不吭，而那份文件上每一条按生产段/测试段扫的判据人群都是错的。
///
/// # 🔴 为什么它不会和剥法一起瞎掉：**匹配单位不同**
///
/// 剥法认的是「**第一处**列 0 的 `}`」（一次命中就收尾）。本条数的是**整段里的两个计数**。
/// ⇒ 收尾挪早了（落进字符串/注释里）**必然**留下 `{` 多于 `}`；
/// 收尾挪晚了吞掉一个顶层 item（那个 item 自身配平）本条**看不见** —— 那一档由
/// 两侧「生产段里必须还找得到某锚点」的语义钉守，**这条边界写在这里，别读成本条全都接得住**。
///
/// # 它的射程边界，两侧都写出来
///
/// - **接得住**：区间收在字符串字面量 / 原始串 / 块注释里的那一形（本轮那个活体）；
///   收在**无花括号体** `mod x;` 之后那一形（历史第一形 —— 那时区间会多吞一个 `}`）。
/// - **接不住**：① 收尾挪晚、吞掉的恰好是配平的整块（上一段已写）；
///   ② 它与剥法**共用同一台词法状态机**（[`scan_and_blank`]）——
///   词法本身认错，两边一起错。看着词法那一档的是 [`assert_block_comment_model_holds`]
///   与本条的兜底分支（下面那句 panic）。
/// - **判不了就明说**：掩码走兜底（词法收不了口）⇒ 本条**当场 panic 说「判不了」**，
///   不许静默当成通过。09-13 现打：全仓 235 份 `.rs` 走兜底的**0 份**。
///
/// **给一条源码扫描型守卫立负对照**：剥完之后，它要扫的那几段**还在**。
///
/// # 它判的不是产品性质，是「**这把尺子够得着我说它够得着的那一段**」
///
/// 本 crate 的反向自检只有**欠剥**那一半（[`assert_no_test_code`] 数残留的测试属性、
/// [`assert_no_unstripped_test_module`] 数残留的测试模块）。**过剥那一半一直没有原语** ——
/// 本函数头注上游那句「那一族由两侧的『生产段里必须还找得到某锚点』判据守」
/// 说的就是它，而那句话当时只在 backend 一侧有**一处**落点
/// （`guard_support_tests::main_production_section_keeps_its_load_bearing_items`）。
///
/// 失效形状是现打过的：便宜近似 `src.split("\n#[cfg(test)]").next()` 只在
/// 「第一个测试模块之后再没有生产代码」时才对。`stream_source/` 的第一个测试模块在
/// 804 行而 `parse_frame` 在 1771 行 ⇒ 扫描面**归零到前 803 行**，守卫静默变瞎。
/// ⇒ 每条这样的守卫都该有一条这个形状的判据；活样本是
/// `stream_source::write_half_guard::the_shared_stripper_keeps_the_part_this_guard_must_scan`，
/// 本函数是把它**抽成一份**（一条形状出现 N 次就抽一个住址；
/// 而这一族原先「八处各写一份」的账，本 crate 头注第一段就记着）。
///
/// # 三件事一起买，缺一件这条对照就退化
///
/// 1. **欠剥**：剥完不许有测试属性残留（转调 [`assert_no_test_code`]）。
/// 2. **过剥**：`anchors` 里每一个都必须**还在**生产段里。
/// 3. 🔴 **尺子活着**：`anchors` 里每一个都必须被**便宜近似丢掉**。
///    没有第 3 件，第 2 件就退化成一句普通的存在性断言 ——
///    对一个「针本来就在第一个测试模块之前」的文件，换成便宜近似**照样绿**，
///    于是这条对照读起来像覆盖、实际一格都不守。
///    ⇒ **`anchors` 只许填住在第一个测试模块之后的那些针。**
///
/// # ⚠ 它**买不到**什么（两侧都写出来）
///
/// - **不买「针还是那个针」**：`anchors` 是调用方交进来的字面量。守卫真正拿去判事的针
///   如果和这里填的不是同一批，本条照样绿 —— 那是「登记表与执行链脱钩」那一族
///   （`judge-not-in-exec-chain`），本条**不治**。⇒ 填的时候从那条守卫的针表里抄，
///   别另起一批好过的。
/// - **不买字节数地板**：活样本里那句 `good.len() > cheap.len() * 2` **刻意没抽上来** ——
///   那个倍数是 `stream_source/` 一份文件的数（19% 存活），换一份文件就是假的；
///   而地板在「静默缩水」这个方向上本来就是瞎的。
///   第 3 件（逐针零命中）比它严，也不需要维护一个会漂的数。
/// - **不买「这份文件里的针够全」**：它只判交进来的那几个。
///
/// # Panics
///
/// 三件里任何一件不成立时 panic，并点名 `who` ＋ 那个具体的锚点。
pub fn assert_stripper_keeps(who: &str, raw: &str, anchors: &[&str]) {
    assert!(
        !anchors.is_empty(),
        "{who}：锚点表是空的 —— 空表会让这条对照**恒真**，那比没有它更坏（静默覆盖）"
    );
    let good = production_code(raw);
    assert_no_test_code(who, &good);
    // 自指陷阱（本 crate 头注「连踩五次」那一条）：锚点里的换行**必须**是转义写法，
    // 这样它与真正的换行不相等 ⇒ 扫源码的判据读到本行时不会匹配到本行自己。
    let cheap = raw.split("\n#[cfg(test)]").next().unwrap_or(raw);
    for a in anchors {
        assert!(
            good.contains(a),
            "{who}：共享剥法把 `{a}` 剥掉了 —— **剥过头了**，此刻这条守卫扫不到它"
        );
        assert!(
            !cheap.contains(a),
            "{who}：便宜近似居然也留住了 `{a}` —— 这一格不是通过，是**这条对照失去意义**：\n\
             该锚点住在第一个测试模块**之前**，换成便宜近似照样绿。\n\
             ⇒ 换一个住在第一个测试模块之后的针（那才是会静默缩水的那一段），\n\
             或者先确认这份被扫文件的结构是不是变了。"
        );
    }
}

/// # Panics
///
/// 有不配平的区间时 panic 并**逐段点名**（`who` ＋ 行区间 ＋ 两个计数）；
/// 词法兜底时也 panic（诊断里逐字写着「判不了」）。
pub fn assert_test_module_ranges_are_brace_balanced(who: &str, src: &str) {
    let Some(masked) = mask_all_literals(src) else {
        panic!(
            "{who}：词法掩码对这份文本收不了口（`scan_and_blank` 走了兜底）\n\
             ⇒ **本条对这一份判不了**，不许把它读成通过。同一档还会让\n\
             `assert_block_comment_model_holds` 出声 —— 先去看那一条说了什么。"
        )
    };
    let mut offenders: Vec<String> = Vec::new();
    for (start, end) in test_module_ranges(src) {
        let seg = &masked[start..end];
        let opens = seg.matches('{').count();
        let closes = seg.matches('}').count();
        if opens != closes {
            offenders.push(format!(
                "{who} · 第 {}–{} 行这一段：`{{` {opens} 个 / `}}` {closes} 个",
                src[..start].matches('\n').count() + 1,
                src[..end].matches('\n').count() + 1
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "{who}：`test_module_ranges` 切出的 {} 段里有段**花括号不配平**：\n{}\n\
         ★ 典型成因：收尾判据（列 0 的右大括号）落进了**字符串字面量 / 原始串 / 块注释**里\n\
         ⇒ 那一段在字符串中间收了尾，后面的测试代码整段漏进生产段，\n\
         而 `assert_no_test_code` 与 `assert_no_unstripped_test_module` **两条都看不见**\n\
         （漏出去的那截里测试属性与 `mod` 行都可能恰好是 0）。\n\
         ⇒ **改的是 `guard_core` 的剥法（在哪份文本上找锚点），不是这份文件的写法**；\n\
         把那段原始字符串缩进一格只是把症状挪走，下一个 `r#\"` 还会撞上。",
        offenders.len(),
        offenders.join("\n")
    );
}
// ── /KR110D1 ──

/// 遍历一棵源码树，对每个 `.rs` 文件断言 [`assert_no_test_code`] **与**
/// [`assert_no_unstripped_test_module`]（`K-R75` 09-12 补的第二半）。
///
/// 抽出来是因为两侧各有一份一模一样的遍历（backend `every_backend_file_strips_clean`、
/// monitor `every_monitor_file_strips_clean`），而遍历本身也会坏 —— `min_files`
/// 就是那条计数自检：扫到的文件数低于它，说明**遍历坏了**，不是代码变干净了。
///
/// # ⚠ 它的**射程**就是 `root` 那棵树 —— 树外的文件今天没有任何人在看它的生产段
///
/// 09-12 现打（`KR75D3`，量法与逐份清单住 `tests/evidence/K-R75-剥法认形状与真静默读数.md`）：
/// 全仓 git 跟踪的 `.rs` **230** 份，落在本函数三个调用点的根之下的 **198** 份，
/// **32 份在射程之外**（其中 vendored 的第三方引擎 23 份（今天已删）、
/// `src/common/*-core` 八份、`src/frontend/shell/build.rs` 一份）。
/// 那 32 份里加一个剥法认不出的测试模块 —— **实测谁都不红**（死值验读数在同一份 evidence 里）。
/// 🔴 这是**读数不是现状判词**：要不要把射程铺过去归 PM，别顺手在这里改根。
///
/// # Panics
///
/// 目录读不了、文件读不了、或扫到的文件数 `< min_files` 时 panic（守卫语义，只在测试里调）。
pub fn assert_tree_strips_clean(root: &std::path::Path, min_files: usize) {
    assert!(min_files > 0, "min_files 不得为 0 —— 那等于关掉计数自检");
    let mut n = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap_or_else(|e| panic!("读目录 {d:?} 失败: {e}"))
        {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let src =
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读 {path:?} 失败: {e}"));
            // ★ 点名用**相对 `root` 的路径**，不是文件名〔`K-R75`，09-12〕：
            //   两棵树里 `mod.rs` 各有好几份（monitor `backend/mod.rs` · `backend/control/mod.rs` …），
            //   只印一个 `mod.rs` 等于没点名 —— 而「点名那份文件」正是这条反向自检的产出。
            let who = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            // 用 `production_code`（**连注释一起剥**）而不是 `production_source`：
            // 散文里逐字提到测试属性是**正常的**（本文件的头注就在解释它），
            // 那不是「剥法坏了」。U8a-2a 实测：两侧各有一个文件因此假红
            //（`guard-core/src/lib.rs` 自己 + monitor
            //  `ccm_cli_contract_tests.rs::cc_spawn_resolves_a_real_ccm_file_not_a_directory`）。
            // 🔴 〔步 7c 剖分 2026-09-19〕**这个地址从行号改成了符号。**
            // 原来写的是 `ccm_cli_contract.rs` 的第 181 行。那份文件剖分之后只剩 40 行 ⇒ 行号越界，
            // `structural_scan::line_number_addresses_stay_in_range_and_never_grow` 当场红，
            // 而它的诊断逐字写着「改法只有一条：**点符号**，别换一个今天对的行号 ——
            // 换一个今天对的行号就是把这一族再走一遍」。⇒ 照它说的办。
            let prod = production_code(&src);
            assert_no_test_code(&who, &prod);
            // 第二半：剥法**认不出**的测试模块（里面可能一个 `#[test]` 都没有 ⇒ 上一条看不见它）。
            assert_no_unstripped_test_module(&who, &prod);
            // 第三半：区间**花括号净配平**。喂的是**原文**不是 `prod` ——
            // 它要判的正是「区间切在哪儿」，而 `prod` 已经是切完的产物。
            assert_test_module_ranges_are_brace_balanced(&who, &src);
            n += 1;
        }
    }
    assert!(
        n >= min_files,
        "只扫到 {n} 个 .rs 文件（期望至少 {min_files}）—— **遍历坏了**，本条此刻是空转的"
    );
}

/// `hay` 是不是以 `needle` 结尾，**且断在路径分量的边界上**。
///
/// 两边都必须是**已经把 `\` 归一成 `/`** 的路径串 —— 归一化是调用方的事，
/// 这里只管边界。
///
/// ⚠ **不许退回裸 `ends_with`**：那是**松匹配**，`/repo/x/a-src/lib.rs` 会被认成
/// `src/lib.rs`。摘多了和摘少了对称地坏：多摘一份别人的源码 ⇒ 判据的人群悄悄小一份，
/// 而它照样绿。〔09-09 本仓在覆盖率键那边刚吃过同族一次〕
///
/// ⇒ 命中的条件是「整串相等」**或**「匹配点前面恰好是 `/`」。
/// 这一刀只会让摘除**更严**（少摘、绝不多摘）⇒ 各判据的人群只可能变大或不变。
fn path_suffix_matches(hay: &str, needle: &str) -> bool {
    if !hay.ends_with(needle) {
        return false;
    }
    let at = hay.len() - needle.len();
    // `at == 0` ⇒ 整串就是 `needle`；否则它前面那个字节必须是分隔符。
    at == 0 || hay.as_bytes()[at - 1] == b'/'
}

/// 遍历源码树，按 `caller_file` 做**后缀比**、摘除调用者自己那一份。
///
/// # 🔴 它在本仓**几乎处处不生效，而「几乎」那两个字是量出来的**〔`P4` 2026-09-21 现打〕
///
/// 本仓所有判据都由 `#[path]` 挂进生产树 ⇒ `file!()` 给的是
/// `../../tests/backend/X.rs` / `../../../tests/frontend/shell/X.rs` 这种**带 `..` 的折返路径**。
/// 而后缀比的草垛是 `root.join(相对路径)` 拼出来的**字符串** ——
/// ⇒ **它命不命中，只取决于那个 `root` 自己的字符串里有没有同一段 `..`。**
///
/// | 那一侧 | 根怎么来的 | 串里有 `..` 吗 | 自摘 |
/// |---|---|---|---|
/// | monitor | 一律过 `guard_support::repo_root()`（`.parent().parent()`，会规范化） | 没有 | **一处都命不中** |
/// | backend · 生产树 | `guard_support::src_root()` = `CARGO_MANIFEST_DIR` | 没有 | 命不中（而且调用者本来就不在这棵树里） |
/// | backend · 测试树 | `guard_support::tests_root()` = `CARGO_MANIFEST_DIR.join("../../tests/backend")` | **有** | **真的落刀** |
///
/// ⇒ 全仓现打：唯一一处**真的会把调用者摘走**的调用点是
/// `plugin_walk_fixture.rs` 扫 backend 测试树那一趟（`P4` 当轮已改成明写名单）；
/// 别处的这一刀**一份都摘不到**，而「摘不到」与「摘到了」在输出上一模一样。
///
/// ⚠ **别把上面那张表读成一条性质** —— 它是三个字符串今天长什么样的读数。
/// 有人把 `tests_root()` 改成规范化的写法，第三行当场变成「命不中」，
/// 而**不会有任何东西变红**（那正是本函数最危险的地方）。
/// 半条绊线在 `the_scan_tree_macro_no_longer_excludes_its_caller_after_the_split`：
/// 它钉的是 **guard-core 自己这棵树**上自摘不命中；它**钉不住**上面第三行。
///
/// ⇒ **别拿它当「摘掉我自己」的办法**：写新判据要摘掉自己就走
/// [`scan_tree_excluding`] 的明写名单（条 73 · 纪律 2、4），
/// 那条路**摘不到就 panic**，而这条路摘不到时**一个字都不说**。
///
/// # 它本来防的是本 crate 头注那一族的**兄弟病**
///
/// 头注治的是「**抽取面画小了**」（剥法把扫描面砍没）。这里治的是反过来那半：
/// **抽取面画大了 —— 把判据自己也扫进去了**。
///
/// 症状永远一样：判据在**自己的**登记表 / 注释 / 常量里找到了自己要找的东西 ⇒ **恒绿**。
/// audit-0805 实测**五次**：
///
/// | 何处 | 判据在自己的什么东西里找到了自己 |
/// |---|---|
/// | F12 | 跨语言对拍匹配到自己的**注释** |
/// | F13 | 原子替换登记表匹配到自己的 `RULE` **常量**（里面写着两个符号的调用示例） |
/// | F18 | 文档副本判据匹配到自己的**登记表**（`HAS_A_GUARD` 里写着那些判据名） |
/// | F05 | 函数体抽取的**锚点**命中了自己 `PHASES` 表里的字符串 |
/// | F05 | 棘轮 `matches()` **数到自己**：6 vs 真实 4 |
///
/// ★ **五次没有一次是被「判据变红」发现的** —— 四次靠变异、一次靠 clippy。
/// 也就是说这一类缺陷的**默认结局是恒绿**。⇒ 修法不是「再检测一遍」，
/// 是**让它写不出来**：调用方拿不到「包含自己」的那份语料。
///
/// # 用法
///
/// ```ignore
/// let files = guard_core::scan_tree!(&root, &["rs"]);   // 自称摘除调用者所在文件
/// // ⚠ 在本仓那一刀**不生效**（见上面那一节）——今天它等于「一份都不摘」。
/// ```
///
/// ⚠ 直接调本函数也行，但**别手写 `file!()` 以外的东西**当 `caller_file` ——
/// 那等于把摘除关掉，而关掉之后看起来和没关一模一样。
/// ⚠⚠ 而在本仓，`file!()` 本身就已经让它**不生效**了（见上面那一节）——
/// 所以这一段讲的是**这个函数的设计意图**，不是它今天在本仓的行为。
///
/// # Panics
///
/// 目录读不了 / 文件读不了时 panic（守卫语义，只在测试里调）。
pub fn scan_tree_excluding_self(
    root: &std::path::Path,
    exts: &[&str],
    caller_file: &str,
) -> Vec<(std::path::PathBuf, String)> {
    // `file!()` 给的是相对编译单元根的路径（如 `src/byte_cap_registry.rs`，
    // workspace member 则是 `crates/guard-core/src/lib.rs`）；扫到的是绝对路径
    // ⇒ 用**后缀**比对。空串会退化成「摘除一切」，直接拒绝。
    assert!(
        !caller_file.is_empty(),
        "caller_file 为空 —— 摘除会退化成把整棵树都摘掉，那比不摘更坏（静默空集）"
    );
    // ★ **归一化要做在两边，不能只做在草垛那边**〔09-09 Windows CI 现打〕。
    //
    // Windows 上 `file!()` 给的是**反斜杠**形：云端 windows-latest 的 panic 位置
    // 逐字是 `crates\guard-core\src\lib.rs`。而下面比对的草垛早就 `\` → `/` 归一过了
    // ⇒ 针和草垛不同形，后缀比**恒不命中** ⇒ 摘除在 Windows 上整个是空转的。
    //
    // ⚠ 这正是本函数头注说的「关掉之后看起来和没关一模一样」那一形，
    // 只不过关掉它的不是人、是平台：Linux 一路绿，Windows 上判据默默地
    // 把自己那一份也读进了自己的语料。
    // ⚠ 修法**不是**给 Windows 开特判：归一成同一种分隔符之后，两个平台走的是同一条路。
    let caller_norm = caller_file.replace('\\', "/");
    walk_tree(root, exts)
        .into_iter()
        .filter(|(path, _)| {
            // ← 就是这一行：调用者自己那份进不来
            !path_suffix_matches(&path.to_string_lossy().replace('\\', "/"), &caller_norm)
        })
        .collect()
}

/// 一个包的**源码人群**里、住在它自己 `src/` 之外的兄弟源码树：`(包名, 那棵 src/)`。
///
/// 由那个包的 manifest 明写：`[package.metadata.guard]` 下 `population = ["<包目录，相对本 manifest>", …]`。
/// 用处只有一个：monitor 的代码搬进只有前端链的几个包之后，「monitor 源码树」这一问的人群仍是搬家前那一群
/// （[`walk_tree`] · [`module_address`] 都按它收 / 认）。`root` 是那个包的目录或它的 `src/` 时才生效，别的根一律空。
///
/// # Panics
///
/// 明写的目录没有 `Cargo.toml` / 没有包名 / 没有 `src/` —— 声明坏了就当场红，不许静默少扫。
pub fn population_trees(root: &std::path::Path) -> Vec<(String, std::path::PathBuf)> {
    let pkg = if root.join("Cargo.toml").is_file() {
        root.to_path_buf()
    } else if root.file_name().and_then(|n| n.to_str()) == Some("src")
        && root
            .parent()
            .is_some_and(|p| p.join("Cargo.toml").is_file())
    {
        root.parent().expect("上面判过有父目录").to_path_buf()
    } else {
        return Vec::new();
    };
    let manifest = std::fs::read_to_string(pkg.join("Cargo.toml"))
        .unwrap_or_else(|e| panic!("读 {:?} 失败: {e}", pkg.join("Cargo.toml")));
    let text = strip_hash_comment_lines(&manifest);
    let Some(at) = text.find("\n[package.metadata.guard]") else {
        return Vec::new();
    };
    let section = &text[at + 1..];
    let section = section[1..]
        .find("\n[")
        .map_or(section, |end| &section[..end + 1]);
    let Some(list) = section.split_once("population").map(|(_, rest)| rest) else {
        return Vec::new();
    };
    let list = list
        .split_once('[')
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(inner, _)| inner)
        .unwrap_or_else(|| panic!("{pkg:?} 的 `[package.metadata.guard] population` 不是一个数组"));
    list.split(',')
        .map(|s| s.trim().trim_matches('"'))
        .filter(|s| !s.is_empty())
        .map(|rel| {
            // 词法归一（`shell/../filewin` ⇒ `filewin`）：住址按它拼，带 `..` 的串会让调用方的 `strip_prefix` 对不上。
            let mut dir = std::path::PathBuf::new();
            for c in pkg.join(rel).components() {
                match c {
                    std::path::Component::ParentDir => {
                        dir.pop();
                    }
                    std::path::Component::CurDir => {}
                    other => dir.push(other.as_os_str()),
                }
            }
            let raw = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_else(|e| {
                panic!("{pkg:?} 明写的人群 `{rel}` 下没有 Cargo.toml（{e}）—— 声明坏了")
            });
            let name = strip_hash_comment_lines(&raw)
                .lines()
                .map(str::trim)
                .find_map(|l| {
                    l.strip_prefix("name")?
                        .trim_start()
                        .strip_prefix('=')
                        .map(|v| v.trim().trim_matches('"').to_string())
                })
                .unwrap_or_else(|| panic!("{dir:?}/Cargo.toml 没有包名"));
            let src = dir.join("src");
            assert!(
                src.is_dir(),
                "{pkg:?} 明写的人群 `{rel}` 下没有 src/ —— 声明坏了"
            );
            (name, src)
        })
        .collect()
}

/// 两个 `scan_tree_*` 共用的那一趟遍历 —— **只管「收哪些文件」，一个都不摘**。
///
/// 抽出来的理由是（一条形状出现 N 次就抽一个住址）：
/// 摘除口径有两种（`file!()` 自摘 —— **在这一处不生效** / 明写名单），
/// 而遍历口径只许有一份 ——
/// 「不下构建产物目录」「空 `exts` = 整棵树」「非 UTF-8 跳过」这三条边界
/// 各自都是现打逮出来的，写第二份必然漂。
///
/// 〔收尾重排〕**人群按模块树认，不只按目录认**：根下源码用 `#[path]` 挂进来、住在根外的**生产**文件
/// （通信层成员住 `src/comms/` 之后由壳 / 后端的模块树挂进去，「目录只是住址」）也算这棵根的人群，
/// 挂的是 `mod.rs` 就连它那一层目录一起收。指向任何 `tests` 段的挂载不跟（那是测试段，不是生产人群）。
/// 不跟的话，搬家那一拍两边几百条扫描型判据会**静默少扫**那几份（搬家时现打普查量过）。
///
/// 同一条理由的第二形：根是一个包（或它的 `src/`）而那个包的 manifest 明写了兄弟源码树（[`population_trees`]），
/// 那几棵也算这棵根的人群 —— 代码从包里搬进只有它链的兄弟包（通道 · 宿主原语 · 文件窗口），「这个包的源码」这一问的人群不变。
fn walk_tree(root: &std::path::Path, exts: &[&str]) -> Vec<(std::path::PathBuf, String)> {
    let mut out = walk_dir(root, exts);
    for (_, tree) in population_trees(root) {
        out.extend(walk_dir(&tree, exts));
    }
    if !exts.is_empty() && !exts.contains(&"rs") {
        out.sort_by(|a, b| a.0.cmp(&b.0));
        return out;
    }
    let norm = |p: &std::path::Path| -> std::path::PathBuf {
        let mut o = std::path::PathBuf::new();
        for c in p.components() {
            match c {
                std::path::Component::ParentDir => {
                    o.pop();
                }
                std::path::Component::CurDir => {}
                other => o.push(other.as_os_str()),
            }
        }
        o
    };
    let root_n = norm(root);
    let mut seen: std::collections::BTreeSet<std::path::PathBuf> =
        out.iter().map(|(p, _)| norm(p)).collect();
    let mut i = 0;
    while i < out.len() {
        let (host, text) = (out[i].0.clone(), out[i].1.clone());
        i += 1;
        if host.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let Some(dir) = host.parent() else { continue };
        for line in text.lines() {
            let Some(rest) = line.trim_start().strip_prefix("#[path = \"") else {
                continue;
            };
            let Some(rel) = rest.split('"').next() else {
                continue;
            };
            if rel.split(['/', '\\']).any(|seg| seg == "tests") {
                continue;
            }
            let target = norm(&dir.join(rel));
            if target.starts_with(&root_n) || !target.is_file() || !seen.insert(target.clone()) {
                continue;
            }
            let found = if target.file_name().and_then(|n| n.to_str()) == Some("mod.rs") {
                walk_dir(target.parent().expect("mod.rs 有父目录"), exts)
            } else {
                walk_dir_files(&[target.clone()], exts)
            };
            for (p, src) in found {
                let pn = norm(&p);
                if pn == target || seen.insert(pn.clone()) {
                    out.push((pn, src));
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// 〔收尾重排〕一份文件在 `root` 这棵模块树里的**模块住址**（相对 `root`，正斜杠）。
///
/// 住在 `root` 下的：就是相对路径。住在根外、由根下某份文件用 `#[path]` 挂进来的（[`walk_tree`] 顺进来的那些）：
/// 按「没有 `#[path]` 时它该住的地方」给 —— `lib.rs` 里 `#[path = "../comms/outward/mod.rs"] mod relay;`
/// ⇒ `comms/outward/server.rs` 的模块住址是 `relay/server.rs`。模块树搬家不变 ⇒ 按模块住址登记的表搬家不用改。
/// 认不出来（不在根下、也没被挂）⇒ 原样的路径串。
pub fn module_address(root: &std::path::Path, path: &std::path::Path) -> String {
    let fwd = |p: &std::path::Path| p.to_string_lossy().replace('\\', "/");
    let norm = |p: &std::path::Path| -> std::path::PathBuf {
        let mut o = std::path::PathBuf::new();
        for c in p.components() {
            match c {
                std::path::Component::ParentDir => {
                    o.pop();
                }
                std::path::Component::CurDir => {}
                other => o.push(other.as_os_str()),
            }
        }
        o
    };
    let root_n = norm(root);
    let path_n = norm(path);
    if let Ok(rel) = path_n.strip_prefix(&root_n) {
        return fwd(rel);
    }
    // 人群里的兄弟源码树（[`population_trees`]）：`<包名>/<在那个包里的模块住址>`。
    let trees: Vec<(String, std::path::PathBuf)> = population_trees(root)
        .into_iter()
        .map(|(label, tree)| (label, norm(&tree)))
        .collect();
    for (label, tree) in &trees {
        if let Ok(rel) = path_n.strip_prefix(tree) {
            return format!("{label}/{}", fwd(rel));
        }
    }
    let bases = std::iter::once((root_n.clone(), String::new()))
        .chain(trees.into_iter().map(|(l, t)| (t, l)));
    for (base_root, label) in bases {
        for (host, text) in walk_dir(&base_root, &["rs"]) {
            let host_n = norm(&host);
            let Some(dir) = host_n.parent() else { continue };
            let lines: Vec<&str> = text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                let Some(rest) = line.trim_start().strip_prefix("#[path = \"") else {
                    continue;
                };
                let Some(rel) = rest.split('"').next() else {
                    continue;
                };
                let target = norm(&dir.join(rel));
                // 紧跟着的 `mod 名;`（中间只许隔着属性行 / 注释行）。
                let name = lines[i + 1..]
                    .iter()
                    .map(|l| l.trim_start())
                    .find(|l| !(l.starts_with("#[") || l.starts_with("//")))
                    .and_then(|l| {
                        let l = l
                            .strip_prefix("pub(crate) ")
                            .or_else(|| l.strip_prefix("pub "))
                            .unwrap_or(l);
                        l.strip_prefix("mod ")?
                            .split(|c: char| c == ';' || c.is_whitespace())
                            .next()
                    })
                    .map(str::to_string);
                let Some(name) = name else { continue };
                // 宿主是 `lib.rs` / `main.rs` / `mod.rs` ⇒ 子模块住它那一层；否则住 `<宿主名>/` 下。
                let host_name = host_n.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let base = if matches!(host_name, "lib.rs" | "main.rs" | "mod.rs") {
                    dir.to_path_buf()
                } else {
                    dir.join(host_n.file_stem().unwrap_or_default())
                };
                let Ok(base_rel) = base.strip_prefix(&base_root) else {
                    continue;
                };
                let base_rel = [label.as_str(), &fwd(base_rel)]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("/");
                let join = |tail: &str| {
                    if base_rel.is_empty() {
                        tail.to_string()
                    } else {
                        format!("{base_rel}/{tail}")
                    }
                };
                if target == path_n {
                    return if target.file_name().and_then(|n| n.to_str()) == Some("mod.rs") {
                        join(&format!("{name}/mod.rs"))
                    } else {
                        join(&format!("{name}.rs"))
                    };
                }
                if target.file_name().and_then(|n| n.to_str()) == Some("mod.rs") {
                    if let Some(tdir) = target.parent() {
                        if let Ok(inner) = path_n.strip_prefix(tdir) {
                            return join(&format!("{name}/{}", fwd(inner)));
                        }
                    }
                }
            }
        }
    }
    fwd(path)
}

/// 一份份指名的文件照 [`walk_dir`] 同一口径收（后缀 · 非 UTF-8 跳过）。
fn walk_dir_files(
    files: &[std::path::PathBuf],
    exts: &[&str],
) -> Vec<(std::path::PathBuf, String)> {
    let mut out = Vec::new();
    for path in files {
        let ok_ext = exts.is_empty()
            || path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| exts.contains(&e));
        if !ok_ext {
            continue;
        }
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("读 {path:?} 失败: {e}"));
        if let Ok(src) = String::from_utf8(bytes) {
            out.push((path.clone(), src));
        }
    }
    out
}

/// 一棵目录树按目录收（[`walk_tree`] 的目录那一半）。
fn walk_dir(root: &std::path::Path, exts: &[&str]) -> Vec<(std::path::PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let rd = match std::fs::read_dir(&d) {
            Ok(rd) => rd,
            Err(e) => panic!("读目录 {d:?} 失败: {e}"),
        };
        for entry in rd {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                // ★ **不下到构建产物目录里**〔09-15 现打逮到〕。
                //
                // 上面那条「空 `exts` = 整棵树」的能力有个没写出来的前提：**树里全是文本**。
                // 一旦某个 crate 目录里躺着 `target/`（谁在 crate 里直接跑过一次 cargo 就会有），
                // 遍历就会走进 `.fingerprint/dep-lib-*` 这种二进制，下面那个 `read_to_string`
                // 当场 panic：`stream did not contain valid UTF-8`。
                // 失败长得**像判据自己坏了**，而真相是「它扫到了不该扫的东西」——
                // 而且它**只在那些目录存在的机器上红**，CI 干净树上一路绿。
                //
                // ⚠ 判据用的是 `CACHEDIR.TAG` 而不是「目录名叫 target」：那是 cargo
                // 自己往每个构建目录里写的标记（`std::fs` 看得见），
                // 按名字判会误伤真叫 `target` 的源码目录，按标记判不会。
                if path.join("CACHEDIR.TAG").is_file() {
                    continue;
                }
                stack.push(path);
                continue;
            }
            // ★ **空列表 = 不按扩展名筛**〔`PS1` 08-13 补的能力〕。
            //
            // 为什么非补不可：`src/shared/cc-bus/` 那 17 个文件里，脚本（`cc-send` / `cc-spawn` / …）
            // **没有扩展名**。原来空列表会退化成「一个都不要」——于是想扫那棵树的判据
            // 只能自己 `read_dir`，而那正是 `scanning_guard_registry` 那条**递减棘轮**禁的，
            // 且它逐字「**不许把上限调上去让今天好过**」。
            // ⇒ 缺的是**原语的能力**，不是纪律的例外。补在这里，棘轮一格都不用动。
            //
            // ⚠ 语义刻意是「不筛」而不是「筛出无扩展名的」：调用方要的是**整棵树**。
            if !exts.is_empty() {
                let ok_ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| exts.contains(&e));
                if !ok_ext {
                    continue;
                }
            }
            // ★ **读不动 = panic（守卫语义照旧）；不是文本 = 跳过**〔09-15 现打〕。
            //
            // `read_to_string` 把两件事压成了一个 `Err`：**IO 真的失败**（权限 / 坏盘 ——
            // 那是判据该嚷的）与**这个文件不是 UTF-8 文本**（`.pyc`、`.png`、cargo 指纹……）。
            // 压在一起的后果：判据在有生成物的机器上 panic，消息长得像它自己坏了
            // （`stream did not contain valid UTF-8`），而 CI 干净树上一路绿 ——
            // **一个只在某些机器上红、且红得看不出原因的判据**。
            //
            // 拆开之后语义是准的：本函数收的是**文本**语料，
            // 而一个非 UTF-8 的字节流**不可能**含调用方要找的那些标识符 ⇒ 跳过它零损失。
            // ⚠ 刻意**不**按目录名拉黑单（`target` / `__pycache__` / `node_modules` …）：
            // 那种名单只会越拉越长，而且每漏一个就复现一次同样的假失败。
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => panic!("读 {path:?} 失败: {e}"),
            };
            let Ok(src) = String::from_utf8(bytes) else {
                continue; // 不是文本 ⇒ 不是语料
            };
            out.push((path, src));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// 遍历源码树，**按一张明写的名单摘除**，并且**摘不到就当场红**。
///
/// # 🔴 它是条 73 那条新纪律的落点
///
/// 纪律逐字：**「摘掉我自己」不许靠 `file!()`** —— 在本仓那条路**不生效**。
///
/// [`scan_tree_excluding_self`] 用 `file!()` 当针去做后缀比。那在「判据与被扫的树
/// 同住一棵」的年代是对的；**仓库重组把判据搬进 `tests/` 之后它就塌了**：
/// 被 `#[path]` 引进来的文件，`file!()` 给的是
/// `src/../../../tests/bridge/X_tests.rs` 这种**带 `..` 的折返路径**，
/// 而草垛是规范化过的绝对路径 ⇒ **后缀比不命中 ⇒ 摘除整个是空转**。
///
/// 两种后果，**方向相反而症状相同**：
///
/// | 方向 | 症状 |
/// |---|---|
/// | 判据手工把被测那份补回来过 | 补回来的那份**变成重复** ⇒ 「恰好 1 处」读成 2 处 ⇒ **红** |
/// | 判据没补过、指望摘除挡住自己 | 自己那份**进了人群** ⇒ 判据在自己的登记表/散文里找到自己 ⇒ **静默全绿** |
///
/// ⇒ 本函数把排除变成**调用方明写的一张名单**（纪律 2：
/// 「把靠位置的排除换成明写的排除 —— 位置会变，那一行不会」），
/// 并加一条 `file!()` 那条路**从来没有过**的自检：
///
/// ★ **名单上每一条都必须真的摘到东西**，摘到 0 份就 panic。
///   这正是上面那张表两行的共同前提 ——「摘除悄悄失效」—— 的机检形态：
///   路径写错、文件改名、被排除的那份搬走，都当场变红，而不是安静地多扫一份。
///
/// # 名单的匹配口径
///
/// 与 [`scan_tree_excluding_self`] 同一把尺（[`path_suffix_matches`]）：
/// 「整串相等」**或**「匹配点前面恰好是 `/`」，只会摘得更严、绝不多摘。
/// 名单里写**仓根相对路径**（如 `tests/frontend/shell/structural_scan_tests.rs`），
/// 别写裸基名 —— `K-R75` 逐字：两棵树里同名文件是常态。
///
/// # Panics
///
/// 目录读不了 / 文件读不了时 panic；名单里有一条**一份都没摘到**时 panic。
pub fn scan_tree_excluding(
    root: &std::path::Path,
    exts: &[&str],
    excluded: &[&str],
) -> Vec<(std::path::PathBuf, String)> {
    assert!(
        !excluded.iter().any(|e| e.is_empty()),
        "排除名单里有空串 —— 空串会命中一切，那比不排除更坏（静默空集）"
    );
    // 名单为空时退化成「不排除」是合法的：调用方就是要整棵树。
    // ⚠ 但**不许**把 `file!()` 塞进名单 —— 那是本函数存在的理由的反面。
    let all = walk_tree(root, exts);
    let mut hits = vec![0usize; excluded.len()];
    let mut out = Vec::new();
    for (path, src) in all {
        let norm = path.to_string_lossy().replace('\\', "/");
        let mut dropped = false;
        for (i, e) in excluded.iter().enumerate() {
            if path_suffix_matches(&norm, &e.replace('\\', "/")) {
                hits[i] += 1;
                dropped = true;
            }
        }
        if !dropped {
            out.push((path, src));
        }
    }
    for (i, e) in excluded.iter().enumerate() {
        assert!(
            hits[i] > 0,
            "排除名单上的 `{e}` 在 {root:?} 下一份都没摘到 —— \
             它改名了 / 搬走了 / 路径写错了。\n\
             ★ 这一格非红不可：排除悄悄失效之后，那份文件会**回到人群里**，\
             而判据在自己的登记表或散文里找到自己 ⇒ **恒绿**，\
             读起来和「真的没有违规」一模一样。"
        );
    }
    out
}

/// 剥掉 **`#` 整行注释**（YAML / shell 那一套）。
///
/// [`strip_comment_lines`] 的兄弟：那个认的是 `//` / `/*`（Rust、TS），
/// 接不住 `.yml` 与 `.sh`。⚠ **两个都要有，而且都要住在这里** ——
/// 08-08 实测同一天里有两条判据各自在自己文件里内联了一份 `#` 剥法，
/// 第一份被 `structural_scan` 的登记表当场逮住，第二份（`sftp.rs` 读 `release.yml`）
/// 是被**变异**逮住的：把 `cargo zigbuild --target aarch64-…` 那行**注释掉**，
/// 判据照样绿 —— 它读的是原文，命中的是那行注释自己。
///
/// ⚠ 只剥**整行**注释：行尾注释（`run: foo   # 说明`）保留。要剥行尾的话，
/// YAML 里 `#` 可以合法出现在引号内，那需要真解析器 —— 不在这里假装能做。
pub fn strip_hash_comment_lines(src: &str) -> String {
    src.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 列出整棵树里的 **shell 脚本**（相对 `root` 的路径，已排序）。
///
/// 判「是不是 shell 脚本」按**两种真实形态**取，不按后缀一种取：
/// `*.sh`，**或**「没有扩展名 + 首行 shebang 里带 `sh`」。后者不是边角料 ——
/// `shared/ccm`、`tests/e2e/fake-claude`、`src/shared/cc-bus/scripts/` 里那些命令都是这一形，
/// 而 CI 的 shellcheck 列表里逐个手写着它们。
///
/// ⚠ **本函数不摘除调用者**：调用者是 `.rs`，扫的树里没有它 —— 属「扫的树不含自己」
/// 那一类（`scanning_guard_registry` 头注的第四类），自匹配这个概念对它不成立。
/// （和 [`scan_tree_excluding_self`] 的差别今天只在**意图**上：那个函数自称会摘，
/// 而它那一刀在这一处不生效 —— 见它自己的头注。）
///
/// 跳过的目录是构建与依赖产物；它们里面的脚本不是本仓的产物，扫进来只会制造噪音。
///
/// # Panics
///
/// 目录读不了时 panic（守卫语义，只在测试里调）。
/// 走整棵树、按谓词收文件（相对 `root` 的路径，已排序）。
///
/// 跳过的目录是构建与依赖产物；它们里面的东西不是本仓的产物，扫进来只会制造噪音。
fn walk_repo(root: &std::path::Path, keep: &dyn Fn(&std::path::Path, &str) -> bool) -> Vec<String> {
    const SKIP: &[&str] = &[
        ".git",
        "target",
        "node_modules",
        "dist",
        "coverage",
        ".vite",
        // 施工纪律第 21 条的各路编译产物 / 临时脚本目录（仓内不提交，`info/exclude` 挡着）：
        //   放一个 `.sh` 进去，`shell_lint_registry` 就红一条「没被 lint 也没登记」。
        ".scratch",
    ];
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let rd = match std::fs::read_dir(&d) {
            Ok(rd) => rd,
            Err(e) => panic!("读目录 {d:?} 失败: {e}"),
        };
        for entry in rd {
            let path = entry.expect("dir entry").path();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if path.is_dir() {
                if !SKIP.contains(&name.as_str()) {
                    stack.push(path);
                }
                continue;
            }
            if keep(&path, &name) {
                out.push(
                    path.strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    out.sort();
    out
}

/// 整棵树里所有 `<ext>` 后缀的文件（相对 `root`，已排序）。
///
/// ⚠ 与 [`scan_tree_excluding_self`] 的区别：那个**读文件内容**、按扩展名筛、
/// 并自称摘除调用者自己（防自匹配 —— 而那一刀在这一处不生效，见它自己的头注）；
/// 这个只要路径，用于「这一类文件今天有哪些」的清点。
/// 一个目录里**住着谁**：全部文件（任何后缀），相对 `root`，只按目录走、**不顺 `#[path]`**。
/// 「这个目录里住着哪些文件」这一问用它（例：`src/comms/` 下的文件集合 == 通信层登记表）。
pub fn files_under(root: &std::path::Path) -> Vec<String> {
    walk_repo(root, &|_, _| true)
}

pub fn files_by_extension(root: &std::path::Path, ext: &str) -> Vec<String> {
    let suffix = format!(".{ext}");
    walk_repo(root, &|_, name| name.ends_with(&suffix))
}

pub fn shell_scripts(root: &std::path::Path) -> Vec<String> {
    walk_repo(root, &|path, name| {
        if name.ends_with(".sh") {
            return true;
        }
        if name.contains('.') {
            return false;
        }
        // 只读首行：二进制文件也可能没有扩展名，别整份读进来。
        std::fs::read(path)
            .map(|b| String::from_utf8_lossy(&b[..b.len().min(64)]).to_string())
            .map(|head| {
                let first = head.lines().next().unwrap_or_default().to_string();
                first.starts_with("#!") && first.contains("sh")
            })
            .unwrap_or(false)
    })
}

/// 遍历源码树。🔴 **它自称摘除调用者自己，而那一刀在本仓几乎处处不生效** ——
/// 命中与否只取决于扫描根的字符串里有没有和 `file!()` 同一段 `..`；
/// 逐侧读数、唯一那一处真会落刀的调用点、以及为什么这件事没人守着，
/// 逐字住 [`scan_tree_excluding_self`] 头注那张表。
///
/// ⇒ 今天它在绝大多数调用点等于「遍历一棵树，一份都不摘」。
/// **要摘掉自己就走 [`scan_tree_excluding`] 的明写名单**（摘不到就 panic），别指望本宏。
/// ⚠ 它的调用点散布在判据树里，而那些抬头里「自摘生效」这句话已经逐份清过一遍
/// （`P4` 2026-09-21）；`no_guard_prose_still_claims_the_scan_tree_self_exclusion_works`
/// 是那一轮留下的相等断言，新写一句就红。
///
/// ⚠ 用宏而不是让调用方自己传 `file!()`：传参那种写法**允许写错**，
/// 而写错之后判据看起来照样绿 —— 这一族的全部危险就在「错了看不出来」。
#[macro_export]
macro_rules! scan_tree {
    ($root:expr, $exts:expr) => {
        $crate::scan_tree_excluding_self($root, $exts, file!())
    };
}

/// 一个字符算不算「标识符的一部分」——匹配单位的边界由它定义。
///
/// 含**非 ASCII 字母**（`起流` 的下一个字 `程` 必须算，否则中文短语一律判成有边界）
/// 与**连字符**（`src/backend` 的下一段 `-X` 必须算，crate 名/YAML 值大量是连字符形）。
///
/// ⚠ 与 [`cfg_is_test_only`] 里那个 ASCII 版**刻意不共用**：那里判的是 Rust 属性里的标识符
/// （只可能是 ASCII），这里判的是任意源码文本里的「词」。合成一个会让其中一处变松。
fn ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// **把一个事实钉在恰好一处，且那一处不许被撑大**。
///
/// # 它治的族：匹配单位比事实小
///
/// 裸 `hay.contains(needle)` 的匹配单位是**子串**，而判据想钉的事实通常是
/// **一整行 / 一个完整签名 / 一个具体的词**。子串比事实小 ⇒ 任何把事实撑大的改动
/// 都从缝里溜过去，判据照样绿。audit-0805 实测三次：
///
/// | 何处 | needle | 撑大成 | 后果 |
/// |---|---|---|---|
/// | F05 | `起流` | `起流程` | 阶段埋点判据认错阶段 |
/// | F16 | `src/backend` | `src/backend-X` | 「跨 target check 走后端的 lock」这个前提没了却不红 |
/// | F19 | `exe_suffix: &str` | 另一个函数的**同名参数** | 断言指的不是它自称的那个函数 |
///
/// ★ **三次没有一次是被「判据变红」发现的**，全靠变异。这一族的默认结局同样是恒绿。
///
/// # 判据
///
/// 1. **恰好一处**（F19 那种「匹配到别处」当场红）；
/// 2. 那一处**两侧都有边界** —— needle 首字符是标识符字符时前一个字符不许是，
///    末字符是标识符字符时后一个字符不许是（F05/F16 那种「被撑大」当场红）。
///
/// 「被撑大的那些命中」**不计入唯一性计数**：`sleep 1` 与 `sleep 10` 同时存在时，
/// 前者仍然是唯一的干净命中。诊断里会把撑大的那些一并打出来，因为它们常常正是**下一次**的病灶。
///
/// # 返回
///
/// `Ok(字节偏移)`，或 `Err(诊断)` —— 诊断带上实际看到的上下文，照 F01 Phase D 的教训：
/// **变异要连诊断文案一起读**，只写「不匹配」的判据在变异时看不出落地没落地。
/// 从一份 `Cargo.toml` 的正文里，抠出**依赖声明**里那些 `path = "…"`。
///
/// # 为什么这件事要有一个家
///
/// 一份 manifest 里 `path = "…"` 有**两种**完全不同的意思：
/// **依赖**（`foo = { path = "../bar" }` —— 指一棵有自己 `Cargo.toml` 的树）与
/// **目标声明**（`[[bin]]` / `[[example]]` / `[[test]]` 底下顶格的 `path = "src/x.rs"`
/// —— 指一个**入口文件**）。把后者当成前者，就会拼出
/// `<入口文件>/Cargo.toml` 去问 git，得到一条**讲错成因的假红**。
///
/// 🔴 这一形不是假想：2026-09-23 往 `monitor` 包里加第二个 `[[bin]]` 时当场撞上了，
/// 而它对**任何人**加第二个 bin/example/test 都会犯，不是某一次的手滑。
///
/// # 判准是**形状**，不是白名单
///
/// 依赖声明恒是**内联表**（同一行上 `path =` 之前有 `{`）；目标声明恒是**表里的一个顶格键**。
/// ⇒ 不需要枚举「哪些段是目标段」，也就不会在上游多出一种目标类型的那天悄悄失效。
///
/// ⚠ 它**买不到**什么：多行写法（`[dependencies.foo]` 段底下单独一行 `path = "…"`）
/// 认不出 —— 那一形今天全仓零处，登记为已知欠算，**不是已守**。
pub fn inline_table_paths(toml: &str) -> Vec<String> {
    let key = "path = \"";
    let mut out = Vec::new();
    for line in toml.lines() {
        let Some(i) = line.find(key) else { continue };
        // 顶格的 `path =`（目标声明）⇒ 不是依赖。内联表那一形前面一定有 `{`。
        if !line[..i].contains('{') {
            continue;
        }
        let rest = &line[i + key.len()..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
        }
    }
    out
}

pub fn find_pinned(hay: &str, needle: &str) -> Result<usize, String> {
    if needle.is_empty() {
        return Err("needle 为空 —— 那会匹配到任何地方，等于关掉判据".into());
    }
    let first_is_ident = needle.chars().next().is_some_and(ident_char);
    let last_is_ident = needle.chars().next_back().is_some_and(ident_char);
    let mut clean: Vec<usize> = Vec::new();
    let mut stretched: Vec<String> = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = hay[from..].find(needle) {
        let at = from + rel;
        let end = at + needle.len();
        let before_ok = !first_is_ident || !hay[..at].chars().next_back().is_some_and(ident_char);
        let after_ok = !last_is_ident || !hay[end..].chars().next().is_some_and(ident_char);
        if before_ok && after_ok {
            clean.push(at);
        } else {
            let lo = hay[..at].char_indices().rev().nth(10).map_or(0, |(i, _)| i);
            let hi = hay[end..]
                .char_indices()
                .nth(12)
                .map_or(hay.len(), |(i, _)| end + i);
            stretched.push(hay[lo..hi].replace('\n', "\\n"));
        }
        // 按字符步进，别按 needle 长度 —— 重叠命中也要数到。
        from = at + hay[at..].chars().next().map_or(1, char::len_utf8);
    }
    match clean.len() {
        1 => Ok(clean[0]),
        0 if stretched.is_empty() => Err(format!(
            "`{needle}` 一处都找不到 —— 事实没了，或者抽取面画错了（本条此刻是空转的）"
        )),
        0 => Err(format!(
            "`{needle}` 只出现在**被撑大的**位置，没有一处是完整的词：{stretched:?}\n\
             ★ 这就是「匹配单位比事实小」：裸 `contains` 在这里会绿，而它指的根本不是那个事实。"
        )),
        n => Err(format!(
            "`{needle}` 命中 {n} 处（偏移 {clean:?}）—— 断言指不明是哪一处。\n\
             ★ F19 就栽在这里：`exe_suffix: &str` 同时出现在另一个函数的参数表里，\n\
             于是「已收敛到单点」那句话锚定的是别人。把 needle 扩到能唯一确定那个事实的大小。"
        )),
    }
}

/// 「hay 里有没有 needle 这个**完整的词**」—— 不要求唯一，只要求有边界。
///
/// [`find_pinned`] 要求恰好一处；有些场合事实本身就会出现多次（一个变量名在函数里用了五次），
/// 那时要的是本函数。边界判定与 [`find_pinned`] 共用 [`ident_char`]（E3：一个事实一个权威源）。
pub fn contains_word(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let first_is_ident = needle.chars().next().is_some_and(ident_char);
    let last_is_ident = needle.chars().next_back().is_some_and(ident_char);
    let mut from = 0usize;
    while let Some(rel) = hay[from..].find(needle) {
        let at = from + rel;
        let end = at + needle.len();
        let before_ok = !first_is_ident || !hay[..at].chars().next_back().is_some_and(ident_char);
        let after_ok = !last_is_ident || !hay[end..].chars().next().is_some_and(ident_char);
        if before_ok && after_ok {
            return true;
        }
        from = at + hay[at..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

/// **把整行注释抹掉**（留空行，不删行）〔audit-0805 §5 3h，08-06〕。
///
/// # 它治的是本区犯了三次的同一个病
///
/// 「**判据数到注释**」：扫源码文本的判据没剥注释，而注释里往往正好写着那个形态 ——
/// 警告（「⚠ 别再写 `thread::sleep`」）、病史（「此前是 `take(MAX)`」）、举例。
/// 三次分别是 F12 的跨语言对拍 · F24 的裸 `contains` 计数 · 1k 的属性回溯。
///
/// # 为什么留空行而不是删行
///
/// 删行会让**行号错位**，而 [`pin_line`] 返回的正是行号、诊断里也要报行号。
/// 留空行的扫描效果与删行完全一致（空行匹配不上任何 needle），却保住了行号。
/// ⚠ 迁移既有调用点时这一点**改变了返回值**：原先删行的两处，`pin_line` 的行号
/// 从「剥后序号」变成「原文序号」—— 那是修正，不是回归。
///
/// # ★★ 09-01（`K-R3`）：**行尾注释现在也剥了 —— 原来那条「刻意不剥」的理由不成立**
///
/// 〔原文留档，别当它还是现状〕「`let n = 5; // MAX_FOO = 99` 里那个 `MAX_FOO` **仍然会被扫到**。
/// 不剥的理由不是偷懒：按第一个 `//` 截断会砍坏**字符串字面量**里的 `//`
/// （`"http://host"`、`"a//b"` 这类），把好行截成半行、制造新的假阴性。」
///
/// 🔴 **那个理由只否决了『按第一个 `//` 截断』这一种实现，不否决这件事本身。**
/// [`strip_trailing_comments`] 带一个最小状态机，只在**不在字符串里**的位置切
/// ⇒ `"http://host"` 原样保留，而 `MAX_FOO` 剥得掉。**同职的两处（本函数与
/// [`production_code`]）同一拍一起改** —— 只改一处就是「只覆盖了那条病的一个动词」。
///
/// ⚠ **仍然剥不干净的三种情形**逐条写在 [`strip_trailing_comments`] 头注里
/// （raw / byte string 那一行 · 跨行字符串里面 · 引号本行不配平）。**别把这条边界读成「注释都剥干净了」。**
///
/// # ★★ 09-04（`K-R9`）：上面那句边界**曾经是一个活体洞**，现在它被剥掉了一半
///
/// 本函数原先只按**行前缀**判块注释（`*` / `/*` 打头的整行），**没有任何跨行状态**：
///
/// ```text
/// /*
/// h.stop();          ← 这一行不以 `*` 或 `/*` 打头 ⇒ 原样留在「生产文本」里
/// */
/// ```
///
/// ⇒ 编辑器「注释掉这几行」的默认产物**整段穿过去**。现在先过
/// [`strip_block_comments`]（带深度的真词法，等长抹空格 ⇒ **行数仍然不变**），
/// 行前缀那一刀留着当第二层（兜底触发时它还能接住 `*` 打头的续行）。
///
/// ⚠ **与 [`production_code`] 同一拍改** —— 本函数头注上面那段逐字写着
/// 「同职的两处同一拍一起改，只改一处就是『只覆盖了那条病的一个动词』」。
///
/// # 🔴 09-04（`K-R25`）：**交进来的是什么单位，是调用方的责任**
///
/// 上面那条块注释剥法带一条静默兜底，而**兜底会不会触发取决于交进来的那段文本自己
/// 配不配平**，不取决于它所属的文件配不配平。⇒ 把整份文件切小了再交进来
/// （按 `#[test]` 切块 · 函数体窗口 · `&src[a..b]`），那一小段就可能不配平
/// ⇒ 这一格上的洞悄悄开着，而看门判据看的是别的单位。
/// **纪律：先剥整份，再切块。** 理由与那一形的实测读数写在 [`test_attr_chunks`] 头注。
pub fn strip_comment_lines(src: &str) -> String {
    let src = &strip_block_comments(src);
    let blanked = src
        .lines()
        .map(|l| {
            let t = l.trim_start();
            // `*` 与 `/*` 是块注释的续行与开头；不含 `*/`（那行通常已经是续行形态）。
            if t.starts_with("//") || t.starts_with('*') || t.starts_with("/*") {
                ""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    strip_trailing_comments(&blanked)
}

/// **把一个事实钉成一整行**（trim 后逐字相等，且恰好一行）。
///
/// [`find_pinned`] 的边界判据挡不住「同一行被加长」中的一类：分隔符不是标识符字符时
/// （`working-directory: src/backend` 后面接 `/sub`）边界看起来是干净的。
/// 凡事实本身就是「**某个文件里有这么一行**」，用本函数，别用子串。
///
/// 返回命中的**行号（0 基）**。
pub fn pin_line(hay: &str, line: &str) -> Result<usize, String> {
    let hits: Vec<usize> = hay
        .lines()
        .enumerate()
        .filter(|(_, l)| l.trim() == line)
        .map(|(i, _)| i)
        .collect();
    match hits.len() {
        1 => Ok(hits[0]),
        0 => {
            let near: Vec<&str> = hay
                .lines()
                .filter(|l| l.contains(line))
                .take(3)
                .map(|l| l.trim())
                .collect();
            Err(format!(
                "没有任何一行 trim 之后等于 `{line}`。\n\
                 包含它但**不等于**它的行（这正是「事实被撑大了」的样子）：{near:?}"
            ))
        }
        n => Err(format!(
            "有 {n} 行都等于 `{line}` —— 断言指不明是哪一行（行号 {hits:?}）"
        )),
    }
}

// ═══ 从 monitor `structural_scan.rs` 搬来（足迹的申报表与它的判据进了后端，两侧用同一把）═══

/// 一次扫描的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanReport {
    /// 实际检查过的出现次数。
    pub checked: usize,
    /// 违反性质的出现（每条带可读描述）。
    pub violations: Vec<String>,
}

impl ScanReport {
    /// 断言这次扫描通过。**`min_checked` 不是可选的**——它就是要件 3。
    ///
    /// 扫到的数量少于 `min_checked` 时同样失败，措辞明确指向"扫描器可能失效了"
    /// 而不是"被测代码有问题"：这两种失败的排查方向完全不同，混在一起会浪费很多时间。
    pub fn require(&self, min_checked: usize, what: &str) -> Result<(), String> {
        // **`min_checked = 0` 等于把要件 3 静默关掉**（T01 审计 I3）：`checked < 0` 恒假。
        // 文档写「`min_checked` 不是可选的」，但类型上它是——所以这里把它变成硬失败。
        if min_checked == 0 {
            return Err(format!(
                "{what}：min_checked 不得为 0——那等于关掉计数自检（要件 3），\
                 而扫描器失效时正是靠它报警"
            ));
        }
        if !self.violations.is_empty() {
            return Err(format!(
                "{what}：{} 处违反（共检查 {} 处）\n  - {}",
                self.violations.len(),
                self.checked,
                self.violations.join("\n  - ")
            ));
        }
        if self.checked < min_checked {
            return Err(format!(
                "{what}：只扫到 {} 处（期望至少 {min_checked} 处）——**扫描器可能失效了**，\
                 而不是被测代码变干净了。先查扫描器的枚举逻辑，别急着调低阈值。",
                self.checked
            ));
        }
        Ok(())
    }
}

/// 钉死一个逃生口的定义（要件 4）。
///
/// 凡是 [`scan_after_marker`] 的 `allow` 放行的间接变量，它的定义必须逐字出现在文本里。
/// 不钉的话，`$t` 这类变量可以被改成裸值——**扫描照样全绿，而防线已经没了**。
pub fn pin_definition(
    text: &str,
    definition: &str,
    assign_prefix: &str,
    what: &str,
) -> Result<(), String> {
    // **只 `contains` 是不够的**（T01 审计 S3，已独立复现）：在钉死的定义之后再追加一行
    // `t="$tmux_name"`，`contains` 仍然通过、扫描仍然全绿，而 `$t` 运行期已经是裸值了。
    // 所以除了「逐字存在」，还要断言**该变量在非注释行只被赋值一次**。
    if !text.contains(definition) {
        return Err(format!(
            "{what} 的定义必须逐字是 `{definition}`——它是被放行的间接目标的唯一来源，\
             改了它就能绕过整个结构性扫描"
        ));
    }
    let assigns = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter(|l| l.trim_start().starts_with(assign_prefix))
        .count();
    if assigns != 1 {
        return Err(format!(
            "{what} 在非注释行被赋值 {assigns} 次（应为 1 次）——多次赋值时后一次生效，\
             钉死第一处等于没钉：`{assign_prefix}…` 可以被改成裸值而扫描照样全绿"
        ));
    }
    Ok(())
}

/// 一处**符号地址**：`(引用点行号, 被引文件基名, 符号名, 是不是前缀形)`。
pub type SymbolAddress = (usize, String, String, bool);

pub fn is_path_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '/' || c == '-'
}

/// 往前收一个 ASCII 路径 —— 遇到中文（多字节）自然停在字符边界上。
///
/// 抄 `doc_claim_registry.rs` 那条同族判据的收法：本仓的地址几乎都嵌在中文散文里。
pub fn path_before(line: &str, at: usize) -> &str {
    let b = line.as_bytes();
    let mut s = at;
    while s > 0 && is_path_char(b[s - 1] as char) {
        s -= 1;
    }
    &line[s..at]
}

/// 枚举 `text` 里每一处 `文件.rs::符号`。
///
/// **前缀形**（第四个返回值 `true`）：抽出来的符号名以 `_` 收尾。本仓真实出现两形，
/// 都不是腐坏，而是写法：
///   · 通配（`build_local_` 后面跟着 `*_command`）；
///   · 行折（`emit_backend_` 与它的后半截被 `///` 换行拆开）。
/// ⇒ 这一档降级成「那个文件里有**某个**以它打头的声明」，**不许**当成找不到就报红。
pub fn symbol_addresses(text: &str) -> Vec<SymbolAddress> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let b = line.as_bytes();
        let mut from = 0usize;
        while let Some(k) = line[from..].find(".rs::") {
            let at = from + k;
            let base = {
                let p = path_before(line, at);
                let full = format!("{p}.rs");
                full.rsplit('/').next().unwrap_or_default().to_string()
            };
            let mut e = at + 5;
            while e < b.len() && {
                let c = b[e] as char;
                c.is_ascii_alphanumeric() || c == '_'
            } {
                e += 1;
            }
            let sym = line[at + 5..e].to_string();
            if !sym.is_empty() && base != ".rs" {
                let prefix = sym.ends_with('_');
                out.push((i + 1, base, sym, prefix));
            }
            from = at + 5;
        }
    }
    out
}

/// 枚举 `text` 的**生产段**里所有以 `verbs` 任一动词打头的 `fn` 名（去重、有序）。
///
/// # 它服务的是哪一族判据
///
/// 一族「**声明缺口**」：某张表上写着「这一格今天盘上没有实现」（`None` / `false`），
/// 而那句话**没有任何东西核**。本仓的活体是（今天住后端的）`registry.rs::TOOLS` 的 `remote-daemon`：
/// 字段写着 `uninstallable: false`，而 `sftp.rs::uninstall_remote_backend` 是设置面板上
/// 那个「卸载后端」按钮背后的实现，**一直都在** —— 假申报活了一个月，一格没红。
///
/// ⇒ 处方：申报「没有」的那一格，**去它家里扫一眼有没有一个没人认领的同族实现**。
///
/// # 🔴 射程写死，别读大一格
///
/// 它按**名字**认，一个字的语义都不读：
///   · 动词表由调用方给，**不是穷举** —— 叫别的名字的实现它一个都看不见；
///   · 它只说「那份文件的生产段里有一个这么打头的 `fn`」，
///     **说不出**那个 `fn` 是不是真在做那件事（反过来也一样）。
/// ⇒ 它买到的是「那句『没有』有人在核」，**不是**「那句『没有』一定是真的」。
///
/// 剥法走**共享原语** `guard_core::production_code`（剥注释 + 剥测试段）——
/// 本文件那条 `every_comment_stripping_transformer_is_registered` 逐字要求
/// 「先问共享原语为什么不够」，这里够。
pub fn fn_names_starting_with(text: &str, verbs: &[&str]) -> Vec<String> {
    let prod = production_code(text);
    let mut out = Vec::new();
    for line in prod.lines() {
        let mut it = line.split_whitespace().peekable();
        while let Some(tok) = it.next() {
            if tok != "fn" {
                continue;
            }
            let Some(next) = it.peek() else { continue };
            let name: String = next
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() && verbs.iter().any(|v| name.starts_with(v)) {
                out.push(name);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
#[path = "../../../../tests/common/guard-core/lib_tests.rs"]
mod tests;

/// 测试层分级（TQ1）：五层各一张登记表 ＋ 各一条反空真自检。人群是整个仓的测试树，
/// 住在这里是因为它是「元层的元层」，而且本 crate 跟着门禁 `cargo` 那一格跑。
#[cfg(test)]
#[path = "../../../../tests/common/guard-core/test_tiers_tests.rs"]
mod test_tiers;

/// 仓里零引用仓外的开发文档：全部跟踪文本逐行过一张检测网，命中集 == ∅。
#[cfg(test)]
#[path = "../../../../tests/common/guard-core/no_outside_refs_tests.rs"]
mod no_outside_refs;
