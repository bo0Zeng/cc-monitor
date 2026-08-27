//! `K-H2a` 的两条机检：**明文出口恰好一处**（`KS2`）· **记日志走白名单**（`KS4`）。
//!
//! # 为什么单住一个文件
//!
//! 与隔壁 `bind_guard.rs` / `nodelay_guard.rs` 同一个理由，它的头注逐字写着：
//! monitor 侧 `scanning_guard_registry` 立过一条递减棘轮 —— 扫描型判据不许裸遍历目录，
//! 要走 `guard_core::scan_tree!`，而那个宏**按构造摘掉调用者自己那一份**
//! ⇒ **判据与被扫的代码必须不在同一个文件**，否则「摘掉自己」正好把靶子摘了。
//!
//! # `KS4` 为什么是白名单，而本仓别处大量用黑名单
//!
//! `KS4` 逐字：「只记允许记的那几样（方法 · 路径 · 状态码 · 字节数）。**禁止**
//! 『记全部头，除了 `Authorization`』这种写法。理由：黑名单必漏
//! （`Proxy-Authorization` · `X-Api-Key` · `Cookie` · 各家自定义），
//! 而且上游多一种鉴权方式它就自动过期，**没有任何东西会告诉你它过期了**。」
//!
//! 白名单做得成的前提是**人群可枚举**（`structural_scan.rs` 头注逐字论证过这一条，
//! 而 `platform/fallback_guard.rs` 记着它做不成的那次：人群不同质就只能黑名单）。
//! 这里人群是可枚举的：`relay/` 生产段里的每一处 `eprintln!` / `println!` / `writeln!`。
//! ⇒ 枚举它们，要求**每一个被插进去的东西**都在下面那张表里。
//!
//! # 它**认不出**什么（诚实边界，别读成「日志不可能泄漏」）
//!
//! 1. **只扫 `relay/`**。中转之外别处印了什么，本条不管（今天 key 也只住 `relay/`）。
//! 2. **只认 `{ident}` 内联捕获与逗号分隔的位置实参**。有人写
//!    `let s = format!("{:?}", head.headers); eprintln!("{s}");` ⇒ 本条只看见 `s`，
//!    而 `s` 要进白名单得有人写一行理由 —— 拦得住「顺手」，拦不住「刻意绕」。
//! 3. **不判语义**：一个进了白名单的名字，将来被换成装别的东西，本条看不见。

#[cfg(test)]
mod tests {
    use crate::guard_support::production_code;

    /// 允许出现在中转日志里的东西。**默认拒绝**：不在表里的当场红。
    ///
    /// 每行 `(那个名字, 它为什么可以进日志)`。⚠ 加一行**就是在放宽**，要写得出理由。
    const ALLOWED_LOG_ARGS: &[(&str, &str)] = &[
        ("e", "错误对象本身（`std::io::Error` / 解析错误）—— 它不含请求头"),
        ("a", "监听地址（`local_addr()`）"),
        ("port", "端口号"),
        (
            "INFLIGHT_CONNECTIONS",
            "在途连接数上限，一个编译期常量",
        ),
        ("p", "凭据文件读不动 / 解析不了时的说法（`store::StoreError` 的文本，不含 key）"),
        ("how", "权限过宽宽在哪 —— `perm::judge` 造的句子，只含 mode 位 / SDDL 主体名"),
        ("fix", "怎么修 —— 一句固定的指引"),
        ("why", "为什么查不出权限 —— 只含平台与构建 feature"),
        (
            "loaded.path.display()",
            "凭据文件的路径。**`KS9` 的「路径文档化」就落在这一行**：一个「能手编但没人知道在哪」的文件等于不能手编",
        ),
        (
            "store::TEMPLATE",
            "文件不存在时印的模板。它里面那个 `api_key` 字段的值**是空串**（`store` 的判据钉着）",
        ),
        ("ms[0]", "耗时毫秒数（判据自己印的，生产段不出现）"),
        ("ms[1]", "同上"),
    ];

    /// `relay/` 生产段里每一处日志调用的**格式串起首**，`(所在文件, 起首, 它记的是什么)`。
    ///
    /// ⚠ **这是相等断言的另一半**：扫出来的处数必须等于本表行数。
    /// 加一处日志就要来加一行，那正是要的 —— 逼你说清「这一行记的是什么」。
    const LOG_SITES: &[(&str, &str, &str)] = &[
        ("server.rs", "[relay] cannot set connection deadline", "装期限失败"),
        ("server.rs", "[relay] refusing", "在途连接顶满，回 503"),
        ("server.rs", "[relay] connection ended", "一条连接以错误收尾"),
        ("server.rs", "[relay] cannot spawn connection thread", "起线程失败"),
        ("server.rs", "[relay] upstream connect failed", "连不上上游"),
        ("server.rs", "[relay] bad upstream base url", "上游基址解析不了"),
        ("server.rs", "[relay] cannot bind loopback port", "端口起不来"),
        ("server.rs", "[relay] listening on", "起来了，监听在哪"),
        ("server.rs", "[relay] listening (addr unknown", "起来了但问不到地址"),
        ("creds.rs", "[relay] credentials file:", "凭据文件在哪（`KS9` 路径文档化）"),
        ("creds.rs", "[relay] credentials problem:", "文件读不动 / 解析不了"),
        ("creds.rs", "[relay] credentials permissions too wide:", "权限过宽（`KS11`）"),
        ("creds.rs", "[relay] how to fix:", "怎么修（`KS11` 要求两样都有）"),
        ("creds.rs", "[relay] credentials permissions unknown:", "查不出权限，也要出声"),
        ("creds.rs", "[relay] credentials: configured", "配了 —— **只印这个布尔**"),
        ("creds.rs", "[relay] credentials: not configured", "没配"),
        ("creds.rs", "[relay] create that file to configure one", "没配时印模板"),
    ];

    fn relay_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/relay")
    }

    /// 整个 daemon crate 的生产段（逐文件）。`KS2` 的人群是**整个 crate**，不是 `relay/` ——
    /// 「取明文的地方恰好一处」这句话的分母如果只到 `relay/`，
    /// 那么有人在 `observe/` 里再取一次就不会红。**人群要恰好等于性质。**
    fn crate_production() -> Vec<(String, String)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        guard_core::scan_tree!(&dir, &["rs"])
            .into_iter()
            .map(|(p, raw)| (p.display().to_string(), production_code(&raw)))
            .collect()
    }

    /// ★★ `KS2`：**取明文的地方恰好一处**，而且就是换头那一行。
    #[test]
    fn the_plaintext_leaves_the_type_at_exactly_one_place_in_this_crate() {
        let files = crate_production();
        // 采集面自检：整个 crate 的 .rs 不止几个（**相等地板会误伤**，这里用有理由的下界）。
        assert!(
            files.len() >= 30,
            "只扫到 {} 个文件 —— 取法坏了，本断言在空转",
            files.len()
        );

        let mut header_sites: Vec<String> = Vec::new();
        let mut persist_sites: Vec<String> = Vec::new();
        for (path, prod) in &files {
            for (no, line) in prod.lines().enumerate() {
                if line.contains("expose_for_auth_header(") {
                    header_sites.push(format!("{path}:{}", no + 1));
                }
                if line.contains("expose_for_persisting(") {
                    persist_sites.push(format!("{path}:{}", no + 1));
                }
            }
        }

        assert_eq!(
            header_sites.len(),
            1,
            "把明文取出来写鉴权头的地方有 {} 处，应当**恰好 1** 处：{header_sites:?}\n\
             ⚠ `KS2` 逐字：加行是收紧、动断言是放宽。真要多一处，\
             **必须先在件计划里说清那一处是什么**，不许在实现里顺手把这个数改大。",
            header_sites.len()
        );
        assert!(
            header_sites[0].ends_with(".rs:0") || header_sites[0].contains("server.rs"),
            "唯一那处不在 `server.rs`（换头那一行）而在 {} —— 靶子挪了",
            header_sites[0]
        );
        // ★ 另一半：**落盘那个出口在本 crate 里应当一次都没有**（`K-H2a` 裁四：daemon 只读）。
        assert_eq!(
            persist_sites.len(),
            0,
            "daemon 里出现了「把 key 写回文件」的出口：{persist_sites:?}\n\
             裁四逐字：daemon 只许读那份文件，不许写。",

        );
    }

    /// 取出一处宏调用的实参面：从 `!(` 后到配平的 `)`。
    fn macro_args(src: &str, at: usize) -> Option<&str> {
        let open = src[at..].find('(')? + at;
        let b = src.as_bytes();
        let (mut depth, mut i) = (0i32, open);
        while i < src.len() {
            match b[i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&src[open + 1..i]);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// 从一段实参面里挑出「被插进日志的东西」：格式串里的 `{ident}` + 串之后的位置实参。
    fn interpolated(args: &str) -> (String, Vec<String>) {
        // 第一个字符串字面量 = 格式串（`writeln!(out, "…")` 的 `out` 在它之前，天然被跳过）。
        let Some(q0) = args.find('"') else {
            return (String::new(), Vec::new());
        };
        let bytes = args.as_bytes();
        let mut i = q0 + 1;
        while i < args.len() {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                break;
            }
            i += 1;
        }
        let fmt = args[q0 + 1..i.min(args.len())].to_string();
        let mut out: Vec<String> = Vec::new();
        // ① 内联捕获 `{name}` / `{name:?}`。
        let mut rest = fmt.as_str();
        while let Some(a) = rest.find('{') {
            let Some(b2) = rest[a..].find('}') else { break };
            let inner = &rest[a + 1..a + b2];
            rest = &rest[a + b2 + 1..];
            let name = inner.split(':').next().unwrap_or("").trim();
            if !name.is_empty() && !name.chars().all(|c| c.is_ascii_digit()) {
                out.push(name.to_string());
            }
        }
        // ② 格式串之后的位置实参（顶层逗号分隔）。
        let tail = args.get(i + 1..).unwrap_or("");
        let mut depth = 0i32;
        let mut cur = String::new();
        for c in tail.chars() {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                ',' if depth == 0 => {
                    let t = cur.trim().to_string();
                    if !t.is_empty() {
                        out.push(t);
                    }
                    cur.clear();
                    continue;
                }
                _ => {}
            }
            cur.push(c);
        }
        let t = cur.trim().to_string();
        if !t.is_empty() {
            out.push(t);
        }
        (fmt, out)
    }

    /// ★★ `KS4`：中转记日志走**白名单**。
    #[test]
    fn every_log_line_in_the_relay_only_carries_registered_fields() {
        let files = guard_core::scan_tree!(&relay_dir(), &["rs"]);
        assert!(
            files.len() >= 8,
            "只扫到 {} 个文件 —— 取法坏了，本断言在空转",
            files.len()
        );

        let mut found: Vec<(String, String)> = Vec::new();
        let mut bad: Vec<String> = Vec::new();
        for (path, raw) in &files {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let prod = production_code(raw);
            // ⚠⚠ **针不许互相包含** —— 这一行是改过一次的，经过记这里，因为它正是
            //    本工作区那族病的第一形（**针拼错**），而且是**相等断言逮住的**：
            //    第一版写的是 `["eprintln!", "println!", "writeln!"]`，
            //    而 `println!` **是 `eprintln!` 的子串** ⇒ `server.rs` 那 9 处各被数了两遍，
            //    实测「扫到 26 处，登记表 17 行」（26 = 8 + 9×2）。
            //    若当初写的是「处数 >= 登记表行数」这种松地板，这一刀**根本不会红**。
            //    今天只留两根**互不包含**的针：`println!` 顺带认得 `eprintln!`，
            //    `writeln!` 认得 `writeln!`；`print!` / `write!` 同理各认一对。
            //    ⚠ 射程：`tracing::` 那一族**不认**（今天 `relay/` 生产段零命中，如实记）。
            for m in ["print!", "println!", "write!", "writeln!"] {
                let mut from = 0usize;
                while let Some(rel) = prod[from..].find(m) {
                    let at = from + rel;
                    from = at + m.len();
                    let Some(args) = macro_args(&prod, at) else { continue };
                    let (fmt, items) = interpolated(args);
                    if fmt.is_empty() {
                        continue;
                    }
                    found.push((name.clone(), fmt.clone()));
                    for it in items {
                        if !ALLOWED_LOG_ARGS.iter().any(|(a, _)| *a == it) {
                            bad.push(format!("{name}: `{it}`（出现在 “{fmt}”）"));
                        }
                    }
                }
            }
        }

        assert!(
            bad.is_empty(),
            "中转日志里插进了**没有登记**的东西：\n  {}\n\n\
             `KS4` 要的是白名单：只记允许记的那几样（方法 · 路径 · 状态码 · 字节数）。\n\
             要加就往 `ALLOWED_LOG_ARGS` 加一行**并写出理由** —— 加不出理由的多半就不该记。\n\
             ⚠ 尤其别写「记全部头，除了 Authorization」：黑名单必漏\n\
             （`Proxy-Authorization` · `X-Api-Key` · `Cookie` · 各家自定义），\n\
             而且上游多一种鉴权方式它就自动过期，没有任何东西会告诉你它过期了。",
            bad.join("\n  ")
        );

        // ★ 相等断言：处数必须等于登记表行数（**不是松地板**）。
        assert_eq!(
            found.len(),
            LOG_SITES.len(),
            "扫到 {} 处日志调用，登记表里有 {} 行 —— 加了一处日志就来加一行登记，\n\
             说清它记的是什么。扫到的是：{:#?}",
            found.len(),
            LOG_SITES.len(),
            found
        );
        // 逐条对上（不是只对数量 —— 数量对得上而内容换了一批，那也是漂移）。
        for (file, head, _) in LOG_SITES {
            assert!(
                found
                    .iter()
                    .any(|(f, fmt)| f == file && fmt.starts_with(head)),
                "登记表里的这一行在盘上找不到了：{file} “{head}”"
            );
        }
    }
}
