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
/// 09-04 在本仓 `local_daemon.rs` 的幂等门上现打过这一刀：monitor 侧
/// **1275 条测试全绿、0 失败**，而生产上「点两下起出第二个 daemon」那个阻塞级缺陷回来了 ——
/// 连**专门为它写的**那条判据（`starting_twice_does_not_spawn_a_second_local_daemon`）
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
///    实测：`shared_crate_registry.rs::shared_crate_names` 里的 `.trim_end_matches('"')`
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
///    `remote-daemon-proto/src/relay/server.rs` 的 `STUB_LAUNCHER` 是一段 shell，
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
/// `evidence/K-R25-D2-unit-alignment.md`。
fn try_strip_block_comments(src: &str) -> Option<String> {
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
                    i += 1 + h;
                    continue;
                }
                i += 1;
                continue;
            }
            if in_str {
                if sb[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if sb[i] == b'"' {
                    in_str = false;
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
                i += consumed;
                continue;
            }
            if sb[i] == b'"' {
                in_str = true;
                i += 1;
                continue;
            }
            if sb[i] == b'/' && sb.get(i + 1) == Some(&b'/') {
                // 行注释：本行剩下的一律不看（`//` 那一半归 `strip_trailing_comments`）。
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
/// 本仓有两条守卫（monitor 的 `every_test_that_starts_the_real_daemon_demands_a_private_tmux`
/// 与它的姊妹 `every_real_daemon_e2e_demands_a_private_tmux_dir`）**先按 `#[test]` 切块、
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
/// 数与住址在 `evidence/K-R25-D2-unit-alignment.md`（量具 `evidence/K-R25-D1-strip-input-unit-census.py`）。
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
pub fn assert_block_comment_model_holds(
    root: &std::path::Path,
    min_files: usize,
    min_blocks: usize,
) {
    assert!(min_files > 0, "min_files 不得为 0 —— 那等于关掉计数自检");
    assert!(
        min_blocks > 0,
        "min_blocks 不得为 0 —— 那等于关掉块级计数自检"
    );
    let mut n = 0usize;
    let mut blocks = 0usize;
    let mut bad: Vec<String> = Vec::new();
    let mut bad_blocks: Vec<String> = Vec::new();
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
/// 喂饱（daemon 的 `no_timer_guard` P4 实测被打红过；`build_id_guard` 的指纹会被注释里的
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
/// 判据全绿（09-04 在 `local_daemon.rs` 的幂等门上现打：monitor **1275 条全绿 0 失败**）。
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
            // ★ **空列表 = 不按扩展名筛**〔`PS1` 08-13 补的能力〕。
            //
            // 为什么非补不可：`shared/cc-bus/` 那 17 个文件里，脚本（`cc-send` / `cc-spawn` / …）
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
            if path_suffix_matches(&path.to_string_lossy().replace('\\', "/"), &caller_norm) {
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

/// 剥掉 **`#` 整行注释**（YAML / shell 那一套）〔audit-0805 08-08〕。
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

/// 列出整棵树里的 **shell 脚本**（相对 `root` 的路径，已排序）〔audit-0805 08-08〕。
///
/// 判「是不是 shell 脚本」按**两种真实形态**取，不按后缀一种取：
/// `*.sh`，**或**「没有扩展名 + 首行 shebang 里带 `sh`」。后者不是边角料 ——
/// `shared/ccm`、`e2e/fake-claude`、vendored `cc-acct-iso` 都是这一形，
/// 而 CI 的 shellcheck 列表里逐个手写着它们。
///
/// ⚠ 与 [`scan_tree_excluding_self`] 不同，**本函数不摘除调用者**：调用者是 `.rs`，
/// 扫的树里没有它 —— 属「扫的树不含自己」那一类（`scanning_guard_registry` 头注的第四类），
/// 自匹配这个概念对它不成立。
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
/// 并摘除调用者自己（防自匹配）；这个只要路径，用于「这一类文件今天有哪些」的清点。
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

    /// ★ 回归钉〔09-09 Windows CI 现打〕：`file!()` 在 Windows 上给的是**反斜杠**形。
    ///
    /// 云端 windows-latest 的 panic 位置逐字是 `crates\guard-core\src\lib.rs`。
    /// 归一化此前只做在扫出来的那一边 ⇒ 针和草垛不同形 ⇒ 后缀比恒不命中、摘除空转。
    ///
    /// ⚠ **为什么不能只靠上面那条**：上面那条只在 Windows 上红，而本仓的 `cargo test`
    /// 在 Windows 上今天才第一次跑起来（此前先卡 fmt、再卡 clippy）——
    /// 也就是说「Linux 全绿」这件事**证明不了摘除是活的**。这一条把那个平台差
    /// 拉成一个**两个平台都跑**的输入：直接喂反斜杠形的 `caller_file`，摘除必须照样生效。
    #[test]
    fn a_windows_shaped_caller_file_still_excludes_the_caller() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let files = scan_tree_excluding_self(&root, &["rs"], r"crates\guard-core\src\lib.rs");
        assert!(
            files.is_empty(),
            "反斜杠形的 `caller_file` 没摘掉调用者 —— 摘除在 Windows 上是空转的，实得 {:?}",
            files.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );
    }

    /// 摘除的后缀比**断在路径分量边界上**：`a-src/lib.rs` 不许被当成 `src/lib.rs`。
    ///
    /// 松匹配的后果是**摘多了**：多摘一份不是调用者的源码，判据的人群悄悄小一份而照样绿。
    #[test]
    fn the_exclusion_suffix_is_anchored_at_a_path_component_boundary() {
        assert!(path_suffix_matches("/repo/x/src/lib.rs", "src/lib.rs"));
        assert!(path_suffix_matches("src/lib.rs", "src/lib.rs"));
        assert!(
            !path_suffix_matches("/repo/x/a-src/lib.rs", "src/lib.rs"),
            "松匹配把 `a-src/lib.rs` 也认成了 `src/lib.rs` —— 那会摘掉一份不是调用者的源码"
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

    /// ★★ `K-R3` 09-01：**行尾注释不许再喂饱存在型判据**。
    ///
    /// 语料是本仓退出臂那一刀的最小形（09-01 现打过真值：改之前门禁七格一个数不动）：
    /// 真调用被换掉、只在**行尾注释**里留下原来那句 —— 生产上功能没了，判据却绿。
    #[test]
    fn a_trailing_comment_can_no_longer_feed_an_existence_guard() {
        let holed = "fn exit() {\n    let _ = h.current_pid(); // h.stop();\n}\n";
        let prod = production_code(holed);
        assert!(
            !prod.contains(".stop()"),
            "行尾注释把 `.stop()` 带进了生产文本 —— 那正是「功能抽走、判据照绿」那个洞：{prod:?}"
        );
        // 非空对照：真调用还在时**必须**读得到（不然上面那条可能只是「什么都读不到」）。
        let real = "fn exit() {\n    h.stop(); // 勾了才收；缺省不收见 C8③\n}\n";
        assert!(
            production_code(real).contains(".stop()"),
            "真调用被误剥了 —— 这是拿假阳换真阳（铁律 18）"
        );
    }

    /// 🔴 `strip_comment_lines` 头注给「不剥行尾」写的理由是
    /// 「按第一个 `//` 截断会砍坏 `\"http://host\"`」——
    /// 本条钉住：新剥法**没有**落进那个坑，两侧都断。
    #[test]
    fn a_double_slash_inside_a_string_literal_is_not_a_comment() {
        let src = "fn a() {\n    let u = \"http://host/x\"; // 真注释在这儿\n}\n";
        let prod = production_code(src);
        assert!(
            prod.contains("\"http://host/x\""),
            "字符串字面量被当成注释砍掉了：{prod:?}"
        );
        assert!(
            !prod.contains("真注释"),
            "那一行真正的行尾注释没被剥掉：{prod:?}"
        );
    }

    /// 一个 `'\"'` 字符字面量不许把状态机整段带偏（此后所有 `//` 都被当成字符串内容）。
    #[test]
    fn a_quote_char_literal_does_not_derail_the_scanner() {
        let src = "fn a() {\n    if c == '\"' { let _ = 0; } // 尾注释\n}\n";
        let prod = production_code(src);
        assert!(!prod.contains("尾注释"), "字符字面量把剥法带瞎了：{prod:?}");
        assert!(
            prod.contains("if c == '\"'"),
            "字符字面量本身被砍了：{prod:?}"
        );
    }

    /// 保守边界的**正面兑现**：raw string 与跨行字符串里的 `//` 一个字都不许动。
    ///
    /// 这两种情形上的洞**仍然开着**（[`strip_trailing_comments`] 头注逐条写了）——
    /// 本条钉的不是「洞关了」，是「**没有为了关洞去砍字符串**」。
    #[test]
    fn raw_and_multiline_strings_are_left_alone() {
        let raw = "fn a() {\n    let s = r#\"keep // this\"#;\n}\n";
        assert!(
            production_code(raw).contains("keep // this"),
            "raw string 里的 `//` 被当注释砍了：{:?}",
            production_code(raw)
        );
        let multi = "fn a() {\n    let s = \"first\n    second // still inside\";\n}\n";
        assert!(
            production_code(multi).contains("second // still inside"),
            "跨行字符串里的 `//` 被当注释砍了：{:?}",
            production_code(multi)
        );
    }

    // ══════════════════════════════════════════════════════════════════
    // `K-R9` 09-04：块注释那一半
    // ══════════════════════════════════════════════════════════════════

    /// ★★ **本件的正题**：多行块注释不许再喂饱存在型判据。
    ///
    /// 形状照 09-04 在 `local_daemon.rs` 幂等门上现打的那一刀：真代码包进
    /// `/*` `*/`（两个符号各占一行 —— 编辑器「注释掉这几行」的默认产物）、换一个桩值。
    /// 改之前 monitor 侧 **1275 条全绿 0 失败**，而生产上那道门已经没了。
    #[test]
    fn a_block_comment_can_no_longer_feed_an_existence_guard() {
        let holed = "fn start() {\n    /*\n    if g.is_some() {\n        return Already;\n    }\n    */\n    let _shim = g.is_some();\n}\n";
        let prod = production_code(holed);
        assert!(
            !prod.contains("return Already"),
            "块注释把真代码带进了生产文本 —— 那正是「功能抽走、判据照绿」那个洞：{prod:?}"
        );
        assert!(
            !prod.contains("if g.is_some() {"),
            "块注释里的 `if g.is_some() {{` 仍被数进生产段：{prod:?}"
        );
        // 非空对照：真代码还在时**必须**读得到（不然上面两条可能只是「什么都读不到」）。
        let real = "fn start() {\n    if g.is_some() {\n        return Already;\n    }\n}\n";
        assert!(
            production_code(real).contains("return Already"),
            "真代码被误剥了 —— 这是拿假阳换真阳（铁律 18）"
        );
    }

    /// 误伤一：**字符串字面量里的 `/*` 不是注释**。
    ///
    /// 语料取自本仓真形：`shared_crate_registry.rs` 里逐字有 `crates/*/Cargo.toml`
    /// 与 `shared/**`，`relay/server.rs` 里有 shell 的 `${rest%%/*}`。
    #[test]
    fn a_block_open_inside_a_string_literal_is_not_a_comment() {
        let src = "fn a() {\n    let g = \"crates/*/Cargo.toml\";\n    let h = \"shared/** files\";\n    keep_me();\n}\n";
        let prod = production_code(src);
        assert!(
            prod.contains("\"crates/*/Cargo.toml\""),
            "串里的 `/*` 被当成块注释开头：{prod:?}"
        );
        assert!(
            prod.contains("keep_me()"),
            "串里那个 `/*` 把它后面的真代码吃掉了 —— 这正是「剥狠了」那一格：{prod:?}"
        );
    }

    /// 误伤一的**变体**：`'\"'` 这种字符字面量不许把串状态整段带偏。
    ///
    /// 实测语料：`shared_crate_registry.rs::shared_crate_names` 里的 `.trim_end_matches('\"')`
    /// —— 不掩码的话该文件扫到末尾深度停在 **5**，后面大片真代码会被当注释抹掉。
    #[test]
    fn a_quote_char_literal_does_not_derail_the_block_scanner() {
        let src = "fn a() {\n    let v = x.trim_end_matches('\"').to_string();\n    let g = \"crates/*/Cargo.toml\";\n    keep_me();\n}\n";
        assert!(
            block_comment_model_holds(src),
            "`'\"'` 把块注释词法带瞎了（走了兜底）"
        );
        assert!(
            production_code(src).contains("keep_me()"),
            "真代码被误剥了：{:?}",
            production_code(src)
        );
    }

    /// 误伤二：**嵌套块注释**要吃到最外层那个 `*/`，不是第一个。
    #[test]
    fn nested_block_comments_are_consumed_to_the_outer_close() {
        let src = "fn a() {\n    /* 外 /* 内 */ 还在注释里 hidden_token */\n    keep_me();\n}\n";
        let prod = production_code(src);
        assert!(
            !prod.contains("hidden_token"),
            "嵌套只吃到第一个 `*/`：{prod:?}"
        );
        assert!(prod.contains("keep_me()"), "嵌套吃过头了：{prod:?}");
    }

    /// 误伤三：**`/**` 与 `/*!` 文档注释**是散文，与 `///` 同等对待 —— 剥掉。
    /// 附两个退化形 `/**/` 与 `/***/`（它们必须**恰好**收口，不许多吃一格）。
    #[test]
    fn block_doc_comments_are_prose_and_two_degenerate_forms_close_exactly() {
        let doc = "/** 这里说明为什么不许 forbidden_token */\nfn a() { keep_me(); }\n";
        let prod = production_code(doc);
        assert!(
            !prod.contains("forbidden_token"),
            "块文档注释没剥掉：{prod:?}"
        );
        assert!(
            prod.contains("keep_me()"),
            "块文档注释吃掉了真代码：{prod:?}"
        );
        for degenerate in [
            "fn a() { /**/ keep_me(); }\n",
            "fn a() { /***/ keep_me(); }\n",
        ] {
            assert!(
                production_code(degenerate).contains("keep_me()"),
                "`{degenerate}` 这个退化形没有恰好收口：{:?}",
                production_code(degenerate)
            );
        }
    }

    /// 误伤四：**行注释里的 `/*` 不开块** —— 否则一句散文能吃掉它后面整份文件。
    #[test]
    fn a_block_open_inside_a_line_comment_does_not_swallow_the_file() {
        let src = "fn a() {\n    let _ = 0; // 见 /* 那一段\n    keep_me();\n}\n";
        assert!(
            production_code(src).contains("keep_me()"),
            "行注释里的 `/*` 开了块、把后面的真代码吃了：{:?}",
            production_code(src)
        );
    }

    /// 误伤五：**跨行原始串内部**一个字都不许动。
    ///
    /// 这条不是防御性编程 —— 语料是本仓 `relay/server.rs::STUB_LAUNCHER` 那段 shell 的最小形，
    /// 里面逐字有一个 `/*`（`${rest%%/*}`）和一个 `*/`（`${rest#*/}`）。
    /// 按「含 `r#` 的**那一行**整行不动」的便宜办法，中间这两行会被当块注释抹掉 18 个字节，
    /// 而它们是真的字符串内容。
    #[test]
    fn a_multiline_raw_string_holding_shell_globs_is_left_alone() {
        let src = "const S: &str = r#\"#!/usr/bin/env bash\nhostport=${rest%%/*}\npath=/${rest#*/}\nhost=${hostport%%:*}\n\"#;\nfn a() { keep_me(); }\n";
        let prod = production_code(src);
        assert!(
            prod.contains("hostport=${rest%%/*}") && prod.contains("path=/${rest#*/}"),
            "跨行原始串里的 shell 被当块注释抹了：{prod:?}"
        );
        assert!(
            prod.contains("keep_me()"),
            "原始串把后面的真代码吃了：{prod:?}"
        );
    }

    /// 抹成**等长空格** ⇒ 行数与字节数都不变 ⇒ [`pin_line`] 的行号、
    /// [`find_pinned`] 的字节偏移一格都不漂。
    #[test]
    fn stripping_block_comments_moves_neither_a_line_nor_a_byte() {
        let src = "fn a() {\n    /* 多字节也要\n       等长抹掉 */\n    keep_me();\n}\n";
        let out = strip_block_comments(src);
        assert_eq!(
            out.len(),
            src.len(),
            "字节数变了 ⇒ `find_pinned` 的偏移全体漂"
        );
        assert_eq!(
            out.split('\n').count(),
            src.split('\n').count(),
            "行数变了 ⇒ `pin_line` 的行号全体错位"
        );
        assert!(!out.contains("等长抹掉"), "没剥干净：{out:?}");
    }

    /// 🔴 兜底：模型崩了就**一个字都不剥**（宁可留洞，不许造假红），
    /// 而且这件事**说得出来**（`block_comment_model_holds` 为 `false`）。
    #[test]
    fn a_broken_model_strips_nothing_and_says_so() {
        let unterminated = "fn a() {\n    /* 开了不收口\n    keep_me();\n}\n";
        assert!(
            !block_comment_model_holds(unterminated),
            "没收口的块注释居然被判成模型站得住"
        );
        assert_eq!(
            strip_block_comments(unterminated),
            unterminated,
            "兜底没有原样返回 —— 那会把真代码剥掉、造一片假红"
        );
    }

    /// 本 crate 自己的源码上，块注释词法**不许走兜底**（吃自己的狗粮）。
    /// 树级的那两份（monitor / daemon）由两侧各自的判据钉着。
    /// ⚠ 09-04（`K-R25`）起两个单位一起量：整份文件 ＋ 按 `#[test]` 切出的每一块。
    #[test]
    fn this_crate_never_falls_back_to_not_stripping() {
        assert_block_comment_model_holds(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            1,
            30,
        );
    }

    /// 🔴🔴 **`K-R25` 的正题：钉「剥法的输入单位 = 看门判据的输入单位」这条关系。**
    ///
    /// # 它钉的**不是**一个形状
    ///
    /// 不是「剥法必须按文件跑」这句话（那是一个形状，换个写法就绕过去了），
    /// 而是**「先切块再剥」与「先剥再切块」这两条路答案不同**这件事本身。
    /// 只要它们不同，「剥法在哪个单位上跑」就是一个**承重**的选择，
    /// 而看门判据 [`assert_block_comment_model_holds`] 量的单位就必须把它包住。
    ///
    /// # 活体夹具（**不是空真**：它今天真的分得开两条路）
    ///
    /// `/*` 落在 A 块里、`*/` 落在 B 块里 —— **整份文件配平、单块不配平**。
    /// 对 rustc 这就是一条普通的跨行块注释（编得过），所以
    /// **看门判据的文件那一半在它上面是绿的**，而块那一半会红。
    /// 这一形是 `K-R9` 落定拍在真仓上实打出来的（`evidence/K-R9-R2-fallback-watch-scope.md §C 刀 2`：
    /// 那一趟两条判据全绿、全量 monitor `1278 passed; 0 failed`）。
    ///
    /// ⚠ **谁退掉哪一格会让本条红**：兜底改成「照剥」⇒ ② 断；
    /// 词法不再跨行延续状态 ⇒ ① 或 ④ 断；剥法不再抹掉注释内容 ⇒ ③ 断；
    /// 「剥完再切」不再收掉注释里那条边界 ⇒ ⑤ 断。
    #[test]
    fn cutting_first_and_stripping_first_do_not_give_the_same_answer() {
        // 自指：这两个串**运行时拼** —— 写死会让本文件自己多出一条边界 / 被判据数到。
        let attr = concat!("#[te", "st]");
        let gate = concat!("demand_tmux_", "shim(");
        // A 块开了块注释而没收口；收口的 `*/` 落在 B 块里。
        let straddling = format!(
            "{attr}\nfn a() {{\n    /*\n    let shim = {gate}\"x\");\n}}\n\n\
             {attr}\nfn b() {{\n    */\n    let _ = 0;\n}}\n"
        );
        // ① 整份文件那一档**是配平的** ⇒ 看门判据的文件那一半在它上面绿。
        assert!(
            block_comment_model_holds(&straddling),
            "夹具坏了：整份文件本该配平（一个 `/*` 一个 `*/`），否则下面证不出「文件级看不见」"
        );
        // ② 先切块再剥 ⇒ 单位是**块**，而块上模型崩了（兜底）。
        let cut_first = test_attr_chunks(&straddling);
        let broken: Vec<usize> = cut_first
            .iter()
            .enumerate()
            .filter(|(_, c)| !block_comment_model_holds(c))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            broken.len(),
            1,
            "先切块再剥：本该恰好 1 块掉进兜底（整份配平、单块不配平），实得 {broken:?}；\
             切出 {} 块。**这一格断了就说明两条路不再分得开** —— 那时本条钉的关系没了对象，\
             回来重新造夹具，别把本条删掉。",
            cut_first.len()
        );
        // ③ 兜底 = 一个字都不剥 ⇒ 注释里那句真调用**原样活着**，洞是开着的。
        assert!(
            strip_comment_lines(&cut_first[broken[0]]).contains(gate),
            "兜底那一块居然被剥了 —— 那么「宁可留洞，不许造假红」这条取舍变了，\
             本条的 ② 与它一起要重判"
        );
        // ④ 先剥整份再切块 ⇒ 每一块都干净，注释里那句话**一块都不剩**。
        let stripped_first = strip_comment_lines(&straddling);
        let after = test_attr_chunks(&stripped_first);
        for (i, c) in after.iter().enumerate() {
            assert!(
                block_comment_model_holds(c),
                "先剥再切之后第 {i} 块仍然掉进兜底 —— 剥法没把跨块的那条注释吃掉"
            );
            assert!(
                !c.contains(gate),
                "先剥再切之后第 {i} 块里还留着注释里那句真调用 —— 洞没关上：{c:?}"
            );
        }
        // ⑤ 前置问题的答案（`KR25D2` 甲的那一格）：**落在块注释里面的那条 `#[test]`
        //    边界会被剥掉**，于是前后两块合成一块。那是**对的** —— 注释里的 `#[test]`
        //    不是一条测试。而行数一个都不少（剥法等长抹空格 / 整行换空行）。
        assert_eq!(
            (cut_first.len(), after.len()),
            (2, 1),
            "边界数变了：剥之前该切出 2 块（第二条 `{attr}` 落在块注释里面）、\
             剥之后该只剩 1 块。这一格是「剥完再切，边界本身会不会被剥掉」那个前置问题的读数"
        );
        assert_eq!(
            stripped_first.lines().count(),
            straddling.lines().count(),
            "行数变了 ⇒ `pin_line` 的行号与 `.lines().take(n)` 的窗口全体错位"
        );
        // ⑥ 对照臂（**块内配平**那一形）：先切块再剥**照样剥得掉**
        //    ⇒ `K-R9` 买到的东西还在，本条治的是**作用域**不是词法。
        let contained = format!(
            "{attr}\nfn a() {{\n    /*\n    let shim = {gate}\"x\");\n    */\n    let _ = 0;\n}}\n\n\
             {attr}\nfn b() {{\n    let _ = 1;\n}}\n"
        );
        for (i, c) in test_attr_chunks(&contained).iter().enumerate() {
            assert!(
                block_comment_model_holds(c),
                "对照臂第 {i} 块本该配平（`/*` 与 `*/` 都在块内），夹具坏了"
            );
            assert!(
                !strip_comment_lines(c).contains(gate),
                "对照臂第 {i} 块：块内配平的块注释没被剥掉 —— **`K-R9` 买到的东西丢了**"
            );
        }
    }

    /// 行数不许变 —— [`pin_line`] 报的是行号，剥法改变行数就等于让它的读数全体漂一格。
    #[test]
    fn stripping_trailing_comments_keeps_the_line_count() {
        let src = "fn a() { let _ = 0; } // 尾\nfn b() {}\nlet u = \"a//b\";\n";
        assert_eq!(
            strip_trailing_comments(src).split('\n').count(),
            src.split('\n').count(),
            "行数变了 ⇒ pin_line 的行号全体错位"
        );
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
