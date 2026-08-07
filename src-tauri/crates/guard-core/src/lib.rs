//! 各条源码扫描型守卫共用的「只留生产段」剥法。
//!
//! # 为什么是一个共享 crate（U8a-2a，2026-08-02）
//!
//! 这份剥法原先住在 daemon 的 `guard_support`（`cfg(test)` 模块）。monitor 侧够不着它，
//! 于是 monitor 的守卫各自写了**便宜近似**，最常见的一种是：
//!
//! ```text
//! let prod = src.split("\n#[cfg(test)]").next().unwrap_or(src);
//! ```
//!
//! 它只在「文件里第一个测试模块之后再没有生产代码」时才是对的。`ssh_source.rs` 的
//! 第一个测试模块在 804 行，而 `parse_frame` 在 1771 行 —— 拿那个近似去扫它，
//! **扫描面直接归零到前 803 行**，守卫静默变瞎。
//!
//! 「抽取面画小了」这一族在 unified-backend 工作区里已经出现过四次。本 crate 是它的收口：
//! 两侧用**同一份**剥法，daemon 的 `guard_support` 改为再导出。
//!
//! # ⚠ 与三个兄弟 crate 的一处**不同**，别照抄错家族不变量
//!
//! `branch-core` / `usage-core` / `acct-core` 的头注都写着「无 IO、无平台依赖」。
//! 本 crate 零依赖、平台无关，但 [`assert_tree_strips_clean`] **做真的 `read_dir` 遍历**，
//! 而且以 panic 为错误模型 —— 因为它只供 `cfg(test)` 里的守卫调用（两侧都是
//! `[dev-dependencies]`，不进任何发布二进制）。新增函数请守住这条边界：
//! **纯文本处理放这里，需要 IO 的只能是守卫断言型（panic 语义、只在测试里跑）。**
//!
//! # 为什么剥法长这样（daemon 侧的两次实测教训，原样保留）
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
//! 1. **锚点写死了模块名 `tests`。** daemon `main.rs` 的测试模块叫 `mod stream_flag_tests`
//!    ⇒ 匹配不上 ⇒ `None` 分支 ⇒ **整个文件（含测试段）被当成生产段扫**。
//!
//! 2. **「第一个锚点之后全砍」这个形状本身就是错的。** 它假定测试模块是文件里最后一样
//!    东西。`main.rs` 不是：`mod stream_flag_tests` 在 182–247 行，而**真正的子命令
//!    dispatch 在 275–291 行，在它后面**。
//!
//! ⇒ 只把坑 1 修掉（放宽锚点）会**当场引爆坑 2**。所以本模块的剥法是
//! 「**逐个剥掉每个 `#[cfg(test)]` 模块**」，不是「第一个之后全砍」。
//!
//! # 自指陷阱（本仓连踩五次，别再踩）
//!
//! 锚点里的换行**必须**用转义写法（源码里是反斜杠 + `n` 两个字符），这样它与真正的换行
//! 不相等 ⇒ 扫源码时不会匹配到本行自己。同理，判「剥干净没」用的那个属性名要**运行时拼**。

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
/// 在列 0，而块内任何嵌套大括号都是缩进的。刻意不做完整的大括号配对：那要连字符串、
/// 原始字符串、注释一起解析，复杂度远超收益，而列 0 判据在两侧的文件上实测干净
/// （daemon 侧 `every_daemon_file_strips_clean`、monitor 侧 `every_monitor_file_strips_clean`）。
///
/// # ★ 必须先确认那一行以左大括号收尾（本函数第一版栽在这里，Phase D 审计当场逮出）
///
/// `#[cfg(test)]` 底下也可能是一条**无花括号体的模块声明**：
///
/// ```text
/// #[cfg(test)]
/// mod guard_support;      ← 就是这份剥法自己在 daemon main.rs 里的声明
/// ```
///
/// 锚点照样匹配，但这里没有块可剥 —— 于是「列 0 的右大括号」会一路找到**下一个顶层 item 的
/// 收尾大括号**，把中间**全部生产代码**当测试段吞掉。实测：加上这条声明后，
/// daemon `main.rs:26–179` 整段消失（`const BUILD_ID` / `const CAPABILITIES` / `const EMITS` /
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

/// 每个「带花括号体的 `#[cfg(test)] mod X { … }`」在 `src` 里的字节区间。
///
/// [`production_source`] 与 [`test_source`] **共用这一份判定** —— 它们是同一个事实的
/// 两半，各写一份迟早漂（本仓 E3：一个事实恰好一个权威源）。
/// 判定规则与它们各自的头注一致，改这里之前先读那两段。
fn test_module_ranges(src: &str) -> Vec<(usize, usize)> {
    // 转义写法 ⇒ 与真正的换行不相等 ⇒ 不会匹配到本行自己。
    let open = "\n#[cfg(";
    let close = "\n}";
    let mut out = Vec::new();
    let mut i = 0usize;
    loop {
        let Some(rel) = src[i..].find(open) else {
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
        // 紧接着的那一行必须是 `mod X {` 才是「带花括号体的测试模块」。
        let mod_start = (attr_end + 1).min(src.len());
        let mod_end = line_end(mod_start);
        let mod_line = src[mod_start..mod_end].trim();
        let is_test_mod = cfg_is_test_only(&src[attr_start..attr_end])
            && mod_line.starts_with("mod ")
            && mod_line.ends_with('{');
        if !is_test_mod {
            // 不是测试模块（非 test 的 cfg / 无花括号体的 `mod x;` 声明 / cfg 挂在别的 item 上）
            // ⇒ 不成区间，从属性行之后继续找。
            //
            // ★「无花括号体」那一条是 Phase D 审计逮出来的：`#[cfg(test)] mod guard_support;`
            //   若被当成模块体，「列 0 的右大括号」会一路吞到下一个顶层 item 的收尾，
            //   把中间**全部生产代码**当测试段丢掉（daemon main.rs 曾整段 26–179 行消失）。
            i = attr_end;
            continue;
        }
        match src[j..].find(close) {
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

/// `production_source` + 剥掉行注释。
///
/// 剥注释是必需的：两侧的注释**大量**在解释「为什么这里没有定时器了 / 哪些写模式被
/// 禁了 / 有哪些子命令」，逐字提到那些字面量。不剥的话守卫会被**解释它自己的那段散文**
/// 喂饱（daemon 的 `no_timer_guard` P4 实测被打红过；`build_id_guard` 的指纹会被注释里的
/// 子命令名污染）。
pub fn production_code(src: &str) -> String {
    production_source(src)
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
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

/// 遍历一棵源码树，对每个 `.rs` 文件断言 [`assert_no_test_code`]。
///
/// 抽出来是因为两侧各有一份一模一样的遍历（daemon `every_daemon_file_strips_clean`、
/// monitor `every_monitor_file_strips_clean`），而遍历本身也会坏 —— `min_files`
/// 就是那条计数自检：扫到的文件数低于它，说明**遍历坏了**，不是代码变干净了。
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
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
            // 用 `production_code`（**连注释一起剥**）而不是 `production_source`：
            // 散文里逐字提到测试属性是**正常的**（本文件的头注就在解释它），
            // 那不是「剥法坏了」。U8a-2a 实测：两侧各有一个文件因此假红
            // （`guard-core/src/lib.rs` 自己 + monitor `ccm_cli_contract.rs:181`）。
            assert_no_test_code(name, &production_code(&src));
            n += 1;
        }
    }
    assert!(
        n >= min_files,
        "只扫到 {n} 个 .rs 文件（期望至少 {min_files}）—— **遍历坏了**，本条此刻是空转的"
    );
}

/// 遍历源码树，**按构造摘除调用者自己那一份**。
///
/// # 它防的是本 crate 头注那一族的**兄弟病**
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
/// let files = guard_core::scan_tree!(&root, &["rs"]);   // 自动摘除调用者所在文件
/// ```
///
/// ⚠ 直接调本函数也行，但**别手写 `file!()` 以外的东西**当 `caller_file` ——
/// 那等于把摘除关掉，而关掉之后看起来和没关一模一样。
///
/// # Panics
///
/// 目录读不了 / 文件读不了时 panic（守卫语义，只在测试里调）。
pub fn scan_tree_excluding_self(
    root: &std::path::Path,
    exts: &[&str],
    caller_file: &str,
) -> Vec<(std::path::PathBuf, String)> {
    // `file!()` 给的是相对编译单元根的路径（如 `src/byte_cap_registry.rs`）；
    // 扫到的是绝对路径 ⇒ 用**后缀**比对。空串会退化成「摘除一切」，直接拒绝。
    assert!(
        !caller_file.is_empty(),
        "caller_file 为空 —— 摘除会退化成把整棵树都摘掉，那比不摘更坏（静默空集）"
    );
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
                stack.push(path);
                continue;
            }
            let ok_ext = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| exts.contains(&e));
            if !ok_ext {
                continue;
            }
            if path
                .to_string_lossy()
                .replace('\\', "/")
                .ends_with(caller_file)
            {
                continue; // ← 就是这一行：调用者自己那份进不来
            }
            let src =
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读 {path:?} 失败: {e}"));
            out.push((path, src));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// 遍历源码树并**摘除调用者自己**。见 [`scan_tree_excluding_self`]。
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
/// 与**连字符**（`remote-daemon-proto` 的下一段 `-X` 必须算，crate 名/YAML 值大量是连字符形）。
///
/// ⚠ 与 [`cfg_is_test_only`] 里那个 ASCII 版**刻意不共用**：那里判的是 Rust 属性里的标识符
/// （只可能是 ASCII），这里判的是任意源码文本里的「词」。合成一个会让其中一处变松。
fn ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// **把一个事实钉在恰好一处，且那一处不许被撑大**〔audit-0805 F24〕。
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
/// | F16 | `remote-daemon-proto` | `remote-daemon-proto-X` | 「跨 target check 走 daemon 的 lock」这个前提没了却不红 |
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
/// # ★ 它**不剥行尾注释**，这是刻意的
///
/// `let n = 5; // MAX_FOO = 99` 里那个 `MAX_FOO` **仍然会被扫到**。
/// 不剥的理由不是偷懒：按第一个 `//` 截断会砍坏**字符串字面量**里的 `//`
/// （`"http://host"`、`"a//b"` 这类），把好行截成半行、制造新的假阴性。
/// 谁的语料里确定没有这种字面量，谁自己在调用点截 —— `profile_installer.rs`
/// 就是这么做的（它扫的是自己生成的 shell/rc 片段，可控）。
///
/// ⚠ **别把这条边界读成「注释都剥干净了」** —— 只剥整行的那种。
pub fn strip_comment_lines(src: &str) -> String {
    src.lines()
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
        .join("\n")
}

/// **把一个事实钉成一整行**（trim 后逐字相等，且恰好一行）〔audit-0805 F24〕。
///
/// [`find_pinned`] 的边界判据挡不住「同一行被加长」中的一类：分隔符不是标识符字符时
/// （`working-directory: remote-daemon-proto` 后面接 `/sub`）边界看起来是干净的。
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

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ 摘除自身**真的生效** —— 与下一条互为**对照组**。
    ///
    /// 本 crate 的 `src/` 里只有 `lib.rs` 一个文件，也就是**只有调用者自己**：
    /// 摘除生效 ⇒ 结果恰好为空；摘除失效 ⇒ 结果里会有 `lib.rs`。
    /// 「恰好为空」在这里是**信号**不是零命中 —— 由下一条的非空证明。
    #[test]
    fn the_caller_never_gets_its_own_source_back() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let files = scan_tree!(&root, &["rs"]);
        assert!(
            files.is_empty(),
            "本 crate 的 src/ 只有 lib.rs（= 调用者自己），摘除生效时结果该为空，\
             实得 {:?} —— 摘除没生效。\
             ★ 那正是 audit-0805 五次「判据匹配到自己」的成因：判据要读的东西\
             和判据本身写在同一片文本里。",
            files.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );
    }

    /// ★ 对照组：把摘除条件换成一个匹配不上的名字 ⇒ `lib.rs` 必须回来。
    ///
    /// 没有这一条，上面那条「结果为空」可能只是**遍历本来就坏了**。
    #[test]
    fn without_the_exclusion_the_caller_would_be_included() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let files = scan_tree_excluding_self(&root, &["rs"], "does-not-match-anything.rs");
        assert!(
            files.iter().any(|(p, _)| p.ends_with("lib.rs")),
            "换成匹配不上的摘除名之后 `lib.rs` **仍然**不在结果里 —— \
             说明遍历本来就坏了，上一条的「为空」是假信号（零命中地绿）"
        );
    }

    /// 空 `caller_file` 会退化成「摘除一切」⇒ 必须拒绝，不许静默给空集。
    #[test]
    #[should_panic(expected = "caller_file 为空")]
    fn an_empty_caller_file_is_rejected_not_silently_swallowed() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let _ = scan_tree_excluding_self(&root, &["rs"], "");
    }

    /// 坑 2 的回归钉：测试模块在**文件中段**时，它后面的生产代码必须留下。
    #[test]
    fn keeps_production_code_that_follows_a_mid_file_test_module() {
        let src = "fn a() {}\n#[cfg(test)]\nmod some_tests {\n    fn t() {}\n}\nfn b() {}\n";
        let prod = production_source(src);
        assert!(prod.contains("fn a()"), "前半段丢了：{prod:?}");
        assert!(
            prod.contains("fn b()"),
            "**测试模块之后的生产代码被砍掉了** —— 这正是「第一个锚点之后全砍」的病：{prod:?}"
        );
        assert!(!prod.contains("fn t()"), "测试段没剥掉：{prod:?}");
    }

    /// ★ U8a-2a 追加：把 monitor 侧那个便宜近似与本剥法**摆在同一份输入上对照**。
    ///
    /// 输入照抄 `ssh_source.rs` 的形状：第一个测试模块在中段，真正要扫的生产代码在它**后面**。
    /// 近似做法会把后半段整个漏掉 —— 这条把「为什么要有这个 crate」变成一句可执行的话。
    #[test]
    fn the_cheap_split_approximation_would_lose_the_later_production_code() {
        let src = "fn early() {}\n#[cfg(test)]\nmod early_tests {\n    fn t() {}\n}\n\
                   fn parse_frame() { let _ = 0; }\n";
        let cheap = src.split("\n#[cfg(test)]").next().unwrap_or(src);
        assert!(
            !cheap.contains("fn parse_frame()"),
            "近似做法居然没漏 —— 那这条对照失去意义，检查夹具形状"
        );
        let good = production_source(src);
        assert!(
            good.contains("fn parse_frame()"),
            "本剥法也漏了后半段，那它不比近似强：{good:?}"
        );
        assert!(!good.contains("fn t()"), "测试段没剥掉：{good:?}");
    }

    /// 坑 1 的回归钉：模块名不叫 `tests` 也必须被剥掉。
    #[test]
    fn strips_test_modules_whose_name_is_not_tests() {
        let src = "fn a() {}\n#[cfg(test)]\nmod stream_flag_tests {\n    fn t() {}\n}\n";
        let prod = production_source(src);
        assert!(
            !prod.contains("fn t()"),
            "只认 `mod tests` 的老毛病还在：{prod:?}"
        );
    }

    /// ★ Phase D 审计逮出的那条：`#[cfg(test)] mod x;`（**无花括号体的声明**）不许被当成
    /// 测试模块 —— 否则「列 0 的右大括号」会一路吞到下一个顶层 item 的收尾，把中间的生产代码
    /// 全部丢掉。这条回归钉直接照着真实病灶写（daemon `main.rs` 里 `mod guard_support;` 的形状）。
    ///
    /// ⚠ 夹具**必须写成单行 `\n` 转义**，不能用带真实换行的多行字符串 ——
    /// 否则夹具里那些列 0 的右大括号会在本文件被 `this_crate_strips_clean` 扫到时
    /// 提前触发收尾，让本模块自己的测试段漏进「生产段」。
    /// （**实测过**：第一版就是多行写法，当场把那条自检打红、漏 4 个测试属性。
    ///  那条自检因此也证明了自己不是安慰剂。）
    #[test]
    fn a_bodyless_cfg_test_mod_declaration_swallows_nothing() {
        let src = "#[cfg(test)]\nmod helper;\nconst KEEP_ME: u8 = 1;\n\
                   fn important() {\n    let _ = 0;\n}\n\
                   #[cfg(test)]\nmod tests {\n    fn t() {}\n}\nfn after() {}\n";
        let prod = production_source(src);
        for keep in [
            "mod helper;",
            "const KEEP_ME",
            "fn important()",
            "fn after()",
        ] {
            assert!(
                prod.contains(keep),
                "`{keep}` 被吞了 —— 无体 mod 声明又被当成测试模块了：{prod:?}"
            );
        }
        assert!(!prod.contains("fn t()"), "真测试模块没剥掉：{prod:?}");
    }

    /// ★ U8a-2a 逮出的那条：`#[cfg(all(test, target_os = "linux"))]` 也是测试模块。
    ///
    /// 病灶原样照抄 `session_map.rs::linux_liveness`（U7d 加的）：只认逐字 `#[cfg(test)]`
    /// 的锚点会漏掉它，那 5 个 `#[test]` 就留在「生产段」里了。
    #[test]
    fn strips_test_modules_behind_a_compound_cfg() {
        let src = "fn a() {}\n#[cfg(all(test, target_os = \"linux\"))]\nmod linux_liveness {\n    fn t() {}\n}\nfn b() {}\n";
        let prod = production_source(src);
        assert!(
            !prod.contains("fn t()"),
            "复合 cfg 的测试模块没剥掉：{prod:?}"
        );
        assert!(prod.contains("fn b()"), "后面的生产代码丢了：{prod:?}");
    }

    /// ★ 反向：**不含 `test` 的 cfg 一律不许剥**。
    ///
    /// 放宽锚点最容易引进来的新 bug 就是这个 —— 平台 cfg 满地都是，
    /// 误剥一条就是把一整段生产代码从扫描面里抹掉。
    #[test]
    fn a_platform_cfg_module_is_never_mistaken_for_a_test_module() {
        let src = "#[cfg(all(unix, not(target_os = \"linux\")))]\nmod bsd_impl {\n    fn keep_me() {}\n}\nfn after() {}\n";
        let prod = production_source(src);
        assert!(
            prod.contains("fn keep_me()"),
            "平台模块被当测试段剥了：{prod:?}"
        );
        assert!(prod.contains("fn after()"), "后面的也丢了：{prod:?}");
        // `feature = "test-utils"` 里的 `test` 不是独立标识符。
        assert!(!cfg_is_test_only("#[cfg(feature = \"test-utils\")]"));
        assert!(cfg_is_test_only("#[cfg(test)]"));
        assert!(cfg_is_test_only("#[cfg(all(test, target_os = \"linux\"))]"));
        assert!(!cfg_is_test_only(
            "#[cfg(all(unix, not(target_os = \"linux\")))]"
        ));
    }

    /// 多个测试模块要逐个剥。
    #[test]
    fn strips_every_test_module_not_just_the_first() {
        let src = "fn a() {}\n#[cfg(test)]\nmod m1 {\n    fn t1() {}\n}\nfn b() {}\n\
                   #[cfg(test)]\nmod m2 {\n    fn t2() {}\n}\nfn c() {}\n";
        let prod = production_source(src);
        for keep in ["fn a()", "fn b()", "fn c()"] {
            assert!(prod.contains(keep), "{keep} 丢了：{prod:?}");
        }
        for drop in ["fn t1()", "fn t2()"] {
            assert!(!prod.contains(drop), "{drop} 没剥掉：{prod:?}");
        }
    }

    /// 剥注释：解释性散文里的字面量不许喂饱守卫。
    #[test]
    fn strips_line_comments_so_prose_cannot_feed_a_guard() {
        let src = "// 这里说明为什么不许 forbidden_token\nfn a() { let _ = 0; }\n";
        let prod = production_code(src);
        assert!(!prod.contains("forbidden_token"), "注释没剥掉：{prod:?}");
        assert!(prod.contains("fn a()"), "生产代码被剥掉了：{prod:?}");
    }

    /// `assert_no_test_code` 真的会咬人（否则它是安慰剂）。
    #[test]
    fn the_leak_check_actually_bites() {
        let attr = format!("#[{}]", "test");
        let leaked = format!("{attr}\nfn t() {{}}\n");
        let r = std::panic::catch_unwind(|| assert_no_test_code("自检", &leaked));
        assert!(r.is_err(), "喂进带测试属性的文本却没红 —— 判据形同虚设");
        // 反向：干净文本不许误报
        assert_no_test_code("自检", "fn a() {}\n");
    }

    /// 本 crate 自己的源码也要剥得干净（吃自己的狗粮）。
    #[test]
    fn this_crate_strips_clean() {
        assert_tree_strips_clean(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            1,
        );
    }

    /// ★ `production_source` 与 `test_source` 必须是**互补的两半**。
    ///
    /// 它们共用 `test_module_ranges`，这条钉的是「共用」这件事真的成立：
    /// 两半拼起来的长度等于原文（区间不重叠、不漏），且各自只装该装的东西。
    #[test]
    fn production_and_test_halves_partition_the_source() {
        let src = "fn a() {}\n#[cfg(test)]\nmod m1 {\n    fn t1() {}\n}\nfn b() {}\n\
                   #[cfg(test)]\nmod m2 {\n    fn t2() {}\n}\nfn c() {}\n";
        let prod = production_source(src);
        let test = test_source(src);
        assert_eq!(
            prod.len() + test.len(),
            src.len(),
            "两半拼不回原文 —— 区间重叠或漏了：\nprod={prod:?}\ntest={test:?}"
        );
        for t in ["fn t1()", "fn t2()"] {
            assert!(test.contains(t), "{t} 不在测试段里：{test:?}");
            assert!(!prod.contains(t), "{t} 漏进了生产段：{prod:?}");
        }
        for pcode in ["fn a()", "fn b()", "fn c()"] {
            assert!(prod.contains(pcode), "{pcode} 不在生产段里：{prod:?}");
            assert!(!test.contains(pcode), "{pcode} 漏进了测试段：{test:?}");
        }
    }

    // ── F24：匹配单位比事实小 ────────────────────────────────────────────
    //
    // 下面每一条都配一句**对照**：先证明裸 `contains` 在同一份输入上是绿的，
    // 再证明 `find_pinned` / `pin_line` 是红的。没有对照那半，就分不清
    // 「判据抓到了」和「输入本来就不含那个串」。

    /// `contains_word` 不要求唯一，但仍然要求有边界。
    #[test]
    fn contains_word_needs_a_boundary_but_not_uniqueness() {
        assert!(
            contains_word("let ccm = x; ccm.len(); ccm", "ccm"),
            "多次出现不该拒绝"
        );
        assert!(
            !contains_word("let ccm_raw = 1;", "ccm"),
            "`ccm_raw` 不是 `ccm` 这个词"
        );
        assert!(
            "let ccm_raw = 1;".contains("ccm"),
            "对照组前提不成立：裸 contains 在这里是绿的"
        );
        assert!(!contains_word("anything", ""), "空 needle 一律否");
    }

    /// ★ F05 的形状：中文短语被撑大（`起流` ⊂ `起流程`）。
    #[test]
    fn a_cjk_needle_that_got_stretched_is_rejected() {
        let hay = "[perf] 起流程 耗时 12ms";
        assert!(
            hay.contains("起流"),
            "对照组前提不成立：输入里本来就没有那个子串"
        );
        let e = find_pinned(hay, "起流").expect_err("被撑大的命中必须红");
        assert!(e.contains("被撑大"), "诊断没点明是被撑大：{e}");
        // 反向：真的是 `起流` 时不许红。
        assert!(find_pinned("[perf] 起流 耗时", "起流").is_ok());
    }

    /// ★ F16 的形状：连字符续接（`remote-daemon-proto` ⊂ `remote-daemon-proto-X`）。
    #[test]
    fn a_hyphen_extension_is_rejected() {
        let hay = "working-directory: remote-daemon-proto-X\n";
        assert!(hay.contains("remote-daemon-proto"), "对照组前提不成立");
        assert!(find_pinned(hay, "remote-daemon-proto").is_err());
        assert!(find_pinned(
            "working-directory: remote-daemon-proto\n",
            "remote-daemon-proto"
        )
        .is_ok());
    }

    /// ★ `polling_registry` 的活样本：`sleep 1` ⊂ `sleep 10`。
    ///
    /// 判据名与头注都写着「**每秒**」，而 `sleep 10` 让裸 `contains` 照样绿 ——
    /// 事实（每秒）变了，判据不知道。
    #[test]
    fn sleep_one_does_not_match_sleep_ten() {
        let ten = "    while :; do\n      sleep 10\n    done\n";
        assert!(
            ten.contains("sleep 1"),
            "对照组前提不成立：裸 contains 在这里本该是绿的"
        );
        assert!(
            find_pinned(ten, "sleep 1").is_err(),
            "`sleep 10` 被当成了 `sleep 1`"
        );
        let one = "    while :; do\n      sleep 1\n    done\n";
        assert!(find_pinned(one, "sleep 1").is_ok(), "真的是每秒时不许红");
    }

    /// ★ F19 的形状：needle 不唯一 ⇒ 断言指不明是哪一处。
    #[test]
    fn a_needle_that_matches_two_places_is_rejected() {
        let hay = "fn a(exe_suffix: &str) {}\nfn b(exe_suffix: &str) {}\n";
        assert!(hay.contains("exe_suffix: &str"), "对照组前提不成立");
        let e = find_pinned(hay, "exe_suffix: &str").expect_err("两处命中必须红");
        assert!(e.contains("命中 2 处"), "诊断没说清有几处：{e}");
    }

    /// 空 needle 会匹配到任何地方 ⇒ 拒绝，不许静默 `Ok`。
    #[test]
    fn an_empty_needle_is_rejected() {
        assert!(find_pinned("whatever", "").is_err());
    }

    /// 一处都没有时的诊断要说「空转」，不能只说「不匹配」——
    /// 抽取面画错和事实真没了是两回事，诊断得让人分得出来。
    #[test]
    fn a_missing_needle_says_the_guard_is_idling() {
        let e = find_pinned("nothing here", "absent_token").expect_err("找不到必须红");
        assert!(e.contains("空转"), "诊断没提醒可能是抽取面画错了：{e}");
    }

    /// ★ `pin_line`：`find_pinned` 的边界判据挡不住「同一行被加长」中分隔符非标识符的那类。
    #[test]
    fn pin_line_catches_what_the_boundary_check_cannot() {
        let hay = "defaults:\n  working-directory: remote-daemon-proto/sub\n";
        // `/` 不是标识符字符 ⇒ 边界看起来是干净的，`find_pinned` 会放过。
        assert!(
            find_pinned(hay, "working-directory: remote-daemon-proto").is_ok(),
            "本条的前提是 find_pinned 在这里放过 —— 前提变了就把这条一起改"
        );
        let e =
            pin_line(hay, "working-directory: remote-daemon-proto").expect_err("整行判据必须红");
        assert!(e.contains("撑大"), "诊断没点明事实被撑大：{e}");
        assert!(
            pin_line(
                "  working-directory: remote-daemon-proto\n",
                "working-directory: remote-daemon-proto"
            )
            .is_ok(),
            "trim 之后相等的行不许红"
        );
    }

    /// `pin_line` 的两行同名情形。
    #[test]
    fn pin_line_rejects_two_identical_lines() {
        let e = pin_line("a\nx\nb\nx\n", "x").expect_err("两行相同必须红");
        assert!(e.contains("有 2 行"), "诊断没说有几行：{e}");
    }

    /// `assert_tree_strips_clean` 的计数自检真的会咬人。
    #[test]
    fn the_tree_walk_floor_actually_bites() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let r = std::panic::catch_unwind(|| assert_tree_strips_clean(&root, 9_999));
        assert!(r.is_err(), "地板远高于实际文件数却没红 —— 计数自检是空转的");
    }
}
