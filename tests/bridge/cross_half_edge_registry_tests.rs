
use super::CROSS_EDGES;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// 两棵树里所有 `.rs`（相对仓根，`/` 分隔）。
fn both_halves() -> Vec<String> {
    let root = repo_root();
    let mut out = Vec::new();
    // 🔴 〔搬树 2026-09-18 · `设计/16 §5.4b` 纪律 3〕**每半边各两棵树。**
    //
    // 跨半边的编译期边**无一例外都长在判据里**（本模块头注逐字），而判据剖分之后
    // 整个住进了 `tests/`。只扫 `src/` 那两棵的话，16 条边里有 12 条整批掉出扫描面
    // ⇒ 「实得 4 / 登记 16」。⚠ 四棵树**互不包含**（`§5.4b` 纪律 1 要的那一问）。
    for sub in [
        "src/bridge/src",
        "src/backend",
        "tests/bridge",
        "tests/backend",
    ] {
        let mut stack = vec![root.join(sub)];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(
                        p.strip_prefix(&root)
                            .unwrap_or(&p)
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
    }
    out.sort();
    out
}

/// 哪一半：`monitor` / `daemon` / 其它（`doc/` `shared/` `tests/e2e/` `src/` …）。
///
/// ⚠ 只有 monitor ↔ daemon 这两个方向算「两半互相咬」。
/// 读 `doc/` `shared/` `tests/e2e/` 与前端 `src/*.ts` 的边**另有 13 处**，它们不是这件的标的 ——
/// 那些是「判据去读文档/脚本/前端源」，两半的关系不在其中。
/// ⚠ 这条口径是**量出来的**：全仓 123 处 `include_*!`，跨侧 24 处，
/// 其中 monitor↔daemon 恰好 **8 + 2**。别把 24 当成这件的数。
fn half_of(rel: &str) -> &'static str {
    // 🔴 〔搬树 2026-09-18〕每一半各有**两个**住址前缀：生产段与测试段。
    // ⚠ `tests/e2e/` 与前端 `tests/*.ts` **仍然是「外部」** —— 本模块头注逐字：
    //   「读 `doc/` `shared/` `tests/e2e/` 与前端 `src/*.ts` 的边另有 13 处，
    //   它们不是这件的标的」。所以这里按 `tests/bridge/` · `tests/backend/`
    //   **逐个前缀**认，不写成「凡是 `tests/` 都算」。
    if rel.starts_with("src/bridge/") || rel.starts_with("tests/bridge/") {
        "monitor"
    } else if rel.starts_with("src/backend/") || rel.starts_with("tests/backend/") {
        "daemon"
    } else {
        "外部"
    }
}

/// 一个文件里的所有 `include_str!` / `include_bytes!` 目标（原样字面量 + 归一化后的相对仓根路径）。
fn includes_in(rel: &str) -> Vec<(String, String)> {
    let root = repo_root();
    let raw = std::fs::read_to_string(root.join(rel)).unwrap_or_default();
    includes_of(rel, &raw)
}

/// 同上，但只看**给定文本**（生产段那条断言要在剥完的切片上跑）。
fn includes_in_text(text: &str) -> Vec<(String, String)> {
    includes_of("", text)
}

/// 编译期把别的文件**拉进本 crate** 的三个宏。运行时拼，免得命中本文件自己的说明。
///
/// ⚠ 〔audit-0805 08-06〕**`include!` 是补上的** —— 它与另外两个同族
/// （都是编译期边，都会让「一半的源码布局变了另一半编不过」），
/// 而原来的动词表只有两个。今天全仓零命中，所以补它是**堵明天**，不是修今天。
fn verbs() -> Vec<String> {
    vec![
        format!("include_{}!", "str"),
        format!("include_{}!", "bytes"),
        format!("inclu{}!", "de"),
    ]
}

/// 一个文件里 `include_*!` 的**调用次数**（后面跟 `(` 的才算，散文里提到名字不算）。
fn include_invocations(raw: &str) -> usize {
    let mut n = 0usize;
    for verb in verbs() {
        let mut from = 0usize;
        while let Some(at) = raw[from..].find(verb.as_str()) {
            let i = from + at;
            from = i + verb.len();
            if raw[from..].trim_start().starts_with('(') {
                n += 1;
            }
        }
    }
    n
}

/// 参数形如 `some_macro!()` 时，从**同一文件**里找 `macro_rules! some_macro`
/// 的单臂展开体，取其中第一个字符串字面量。
///
/// 只认单臂、只认同文件 —— 够用且不会误判：本仓这个形态今天恰好两处，都在 `tmux.rs`。
/// 认不出来就返回 `None`，那一处会落进 `NON_LITERAL_INCLUDES` 的对拍里逼人登记。
fn macro_arm_literal(whole: &str, tail: &str) -> Option<String> {
    let name: String = tail
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() || !tail[name.len()..].trim_start().starts_with('!') {
        return None;
    }
    let at = whole.find(&format!("macro_rules! {name}"))?;
    let body = &whole[at..];
    let end = body.find("\n}\n").map_or(body.len(), |e| e);
    let seg = &body[..end];
    let q = seg.find('"')?;
    let rest = &seg[q + 1..];
    let e = rest.find('"')?;
    Some(rest[..e].to_string())
}

fn includes_of(rel: &str, raw: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    // 运行时拼，免得命中本文件自己的说明文字。
    //
    // ⚠ **不许要求 `!(` 与 `"` 连着。** 第一版要求连着，于是 **rustfmt 折行的那两条边
    // 直接消失了**（`daemon_kill.rs` 与 `daemon_send_keys.rs` 的路径太长，被折成
    // `include_str!(\n    "…"\n)`）—— 抽取器读出 8 条，真值 10 条。
    // 那个洞**只被「条数自检」那一条断言逮住**，别的断言都会跟着一起错得很一致。
    for verb in verbs() {
        let mut from = 0usize;
        while let Some(at) = raw[from..].find(verb.as_str()) {
            let mut s = from + at + verb.len();
            // 跳过 `(` 与任意空白/换行，落到那个字面量的第一个引号上。
            let b = raw.as_bytes();
            while s < b.len() && (b[s] as char).is_whitespace() {
                s += 1;
            }
            if s < b.len() && b[s] == b'(' {
                s += 1;
            }
            while s < b.len() && (b[s] as char).is_whitespace() {
                s += 1;
            }
            if s >= b.len() || b[s] != b'"' {
                // 〔audit-0805 08-06〕**参数是同文件里的单臂宏时，把它展开**。
                //
                // `tmux.rs` 有两处 `include_str!(daemon_watcher_src!())`，
                // 而那个宏展开成 `"../../backend/observe/watcher.rs"` ——
                // **两条真的 monitor→daemon 跨界边**，此前整条不在登记表里。
                // 讽刺的是 `tmux.rs` 自己写着为什么要用宏：`include_str!` 只接字面量 token，
                // 用宏是为了「单一落点」—— 一个为了减少重复的好做法，
                // 恰好把这条边从本护栏的视野里摘了出去。
                if let Some(lit) = macro_arm_literal(raw, &raw[s..]) {
                    let dir = Path::new(rel).parent().unwrap_or(Path::new(""));
                    let joined = dir.join(&lit).to_string_lossy().replace('\\', "/");
                    let mut parts: Vec<&str> = Vec::new();
                    for c in joined.split('/') {
                        match c {
                            "." | "" => {}
                            ".." => {
                                parts.pop();
                            }
                            other => parts.push(other),
                        }
                    }
                    out.push((lit, parts.join("/")));
                }
                from = at + from + verb.len();
                continue;
            }
            s += 1;
            let Some(len) = raw[s..].find('"') else { break };
            let lit = &raw[s..s + len];
            let dir = Path::new(rel).parent().unwrap_or(Path::new(""));
            let joined = dir.join(lit).to_string_lossy().replace('\\', "/");
            // 手工归一化（`..` 不碰文件系统，符号链接在这仓里不存在）。
            let mut parts: Vec<&str> = Vec::new();
            for c in joined.split('/') {
                match c {
                    "." | "" => {}
                    ".." => {
                        parts.pop();
                    }
                    other => parts.push(other),
                }
            }
            out.push((lit.to_string(), parts.join("/")));
            from = s + len;
        }
    }
    out
}

/// 遍历两棵树，找出所有**跨半边**的编译期边：`(方向, 读者, 被读)`。
fn discovered_edges() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for rel in both_halves() {
        let mine = half_of(&rel);
        for (_, target) in includes_in(&rel) {
            let theirs = half_of(&target);
            if theirs != mine && theirs != "外部" && mine != "外部" {
                out.push((format!("{mine}→{theirs}"), rel.clone(), target));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// ★ 抽取器自检：遍历与解析都没坏。坏了的话下面两条会零命中地绿。
///
/// ⚠ 中间量都断言：**扫到几个文件** · **看见几处 `include_*!`** · **跨侧几处**。
/// 只比最终数是不够的 —— F+08 那轮四个抽取器出错，全是中间量没人看。
#[test]
fn the_edge_scan_sees_both_trees_and_actually_parses() {
    let files = both_halves();
    let monitor = files.iter().filter(|f| half_of(f) == "monitor").count();
    let daemon = files.iter().filter(|f| half_of(f) == "daemon").count();
    // 实测（F20 摸底）：monitor 侧 77 个 `.rs`、daemon 侧 37 个。
    assert!(
        monitor >= 70 && daemon >= 30,
        "只扫到 monitor {monitor} 个 / daemon {daemon} 个 `.rs` —— 遍历坏了\
             （摸底实测 77 / 37）"
    );
    let total: usize = files.iter().map(|f| includes_in(f).len()).sum();
    assert!(
        total >= 90,
        "两棵树里只解析出 {total} 处 `include_*!` —— 解析坏了（摸底实测全仓 123 处，\
             其中这两棵树占绝大多数）"
    );
    let n = discovered_edges().len();
    assert_eq!(
        n,
        CROSS_EDGES.len(),
        "跨半边的编译期边**条数**变了（实得 {n}，登记 {}）：\n{:#?}\n\
             ⚠ 别改这个数了事：新增一条就把它登记进 `CROSS_EDGES` 并写清\
             「为什么必须编译期读」；少一条说明有判据被删了或路径改了。",
        CROSS_EDGES.len(),
        discovered_edges()
    );
}

/// ★★ 遍历发现的边 == 登记表，**两个方向都查**。
/// ★〔audit-0805 08-06〕**解析不出路径的 `include_*!` 必须登记**（默认拒绝）。
///
/// # 它补的洞
///
/// 抽取器要求宏参数是**字面量**（`(` 之后跳过空白必须是 `"`）。
/// 不是字面量的（`concat!(env!("OUT_DIR"), "/x")` 这种）**被静默跳过** ——
/// 跳过是对的，但**没有任何东西记着跳过了几处**。
/// 于是 `include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../backend/wire.rs"))`
/// 这样一条**真的跨界边**会整条隐形，而它与今天已经在用的写法只差一个路径。
///
/// ⚠ 这不是假想形态：本仓**今天就有两处** `include_bytes!(concat!(env!("OUT_DIR"), …))`。
/// 它们读的是 build script 的产物、不是对面那一半的源码，所以**不是**跨界边 ——
/// 但那是**读了才知道**的，而此前没有任何地方写着这件事。
///
/// ⇒ 改成默认拒绝：解析不出来的每一处都要在 [`NON_LITERAL_INCLUDES`] 里登记 + 说明理由。
/// 新写一处非字面量 include ⇒ 红 ⇒ 逼人回答「它指向哪一半」。
#[test]
fn every_non_literal_include_is_registered_with_a_reason() {
    /// 解析不出字面量路径的 `include_*!`：`(文件, 处数, 为什么不是跨界边)`。
    const NON_LITERAL_INCLUDES: &[(&str, usize, &str)] = &[(
        "src/bridge/src/sftp.rs",
        2,
        "`include_bytes!(concat!(env!(\"OUT_DIR\"), \"/daemon-<arch>\"))` —— 读的是 \
             build script 放进 `OUT_DIR` 的**产物**（内嵌 daemon 二进制），\
             不是对面那一半的**源码** ⇒ 不属两半编译期互咬。\
             ⚠ 但它确实是一条编译期边：`OUT_DIR` 里没有那个文件就编不过 —— \
             那条边由 `build.rs` 与 `shared_crate_registry` 的 CI 步骤那侧管。",
    )];
    let mut found: Vec<(String, usize)> = Vec::new();
    let mut total_invocations = 0usize;
    for rel in both_halves() {
        // 本文件自己要排除：上面那段说明里逐字写着两种**非字面量**的调用示例，
        // 不排除就会把自己的病历当成病（F23 那一族，本区第六次）。
        if rel.ends_with("cross_half_edge_registry.rs") {
            continue;
        }
        let raw = std::fs::read_to_string(repo_root().join(&rel)).unwrap_or_default();
        let inv = include_invocations(&raw);
        total_invocations += inv;
        let parsed = includes_of(&rel, &raw).len();
        if inv > parsed {
            found.push((rel, inv - parsed));
        }
    }
    // 抽取器自检：调用数为零 ⇒ 下面的对拍会两边都空地绿。
    assert!(
        total_invocations >= 20,
        "两半里只数到 {total_invocations} 个 `include_*!` 调用（08-06 实测 40+）—— 计数器坏了"
    );
    found.sort();
    let mut want: Vec<(String, usize)> = NON_LITERAL_INCLUDES
        .iter()
        .map(|(f, n, _)| ((*f).to_string(), *n))
        .collect();
    want.sort();
    assert_eq!(
        found, want,
        "\n解析不出路径的 `include_*!` 与登记表对不上。\n\
             **多出来的**：抽取器跳过了它，而跳过是**静默**的 —— 先回答「它指向哪一半」。\n\
             指向对面那一半 ⇒ 那是一条跨界边，得进 `CROSS_EDGES`；\n\
             指向 `OUT_DIR` / 本半自己 ⇒ 登记进 `NON_LITERAL_INCLUDES` 并写明理由。\n\
             **少了的**：那处改成字面量了或没了 ⇒ 把登记删掉（登记表腐烂比没有登记更糟）。"
    );
}

#[test]
fn every_cross_half_edge_is_registered_with_a_reason() {
    let mut found: Vec<(String, String, String)> = discovered_edges();
    let mut registered: Vec<(String, String, String)> = CROSS_EDGES
        .iter()
        .map(|(d, r, t, _)| (d.to_string(), r.to_string(), t.to_string()))
        .collect();
    found.sort();
    registered.sort();
    assert_eq!(
        found, registered,
        "两半之间的编译期边与登记表对不上。\n\
             多出来的边请登记进 `CROSS_EDGES`，第四列写清**为什么必须编译期读**\n\
             （「跨轨对拍：两侧必须同形」是正当理由；「顺手方便」不是）；\n\
             登记表里多出来的说明那条边被删了或路径改了。"
    );
    for (d, r, t, why) in CROSS_EDGES {
        assert!(
            why.chars().count() >= 12,
            "{d} {r} → {t} 的理由太短（{why:?}）—— 这一列的读者是下一个想加边的人"
        );
    }
}

/// ★★★ **本件真正的那条**：没有一条跨半边的边长在**生产段**。
///
/// 这是「部署路径不受影响」那句话的机检形态：
/// 十条边全在判据里 ⇒ `cargo build`（目标机原生构建走的就是它）看不见它们，
/// 摸底那次决定性实验实测过 —— 把两条 daemon→monitor 的路径打断，
/// `cargo build` 仍然 **exit=0 / 0 error**，只有 `cargo test --no-run` 炸。
///
/// 一旦有人把跨半边的 `include_str!` 写进生产段，两半就**真的**在编译期咬住了
/// （daemon 的部署构建从此需要 monitor 那棵树），本条当场红。
///
/// ⚠ 用的是仓里那把尺子 `guard_core::production_code`，不自己手搓
/// （F+08 的教训：手搓的粗切在 `lib.rs` 第 57 行砍掉了 2108 行）。
/// ⚠ 中间量自检：生产段剥完必须还有东西（否则这条零命中地绿），
/// 且原文里必须真能看见那个 `include_*!`（否则是读错文件了）。
#[test]
fn no_cross_half_edge_lives_in_production_code() {
    let root = repo_root();
    let verb = format!("include_{}!", "str");
    let mut offenders = Vec::new();
    // ⚠ **剥法健康是个「全局」性质，不是逐文件性质。**
    // 第一版逐文件断言「生产段 > 200 字节」，而 `polling_registry.rs` 的生产段
    // 实测 **0 字节** —— 那不是剥法坏了，是那个模块**整体就是判据**
    // （`lib.rs` 里它的 `mod` 前面带着 `#[cfg(test)]`，与 `parity_ledger` 同族）。
    // 0 字节在那儿恰恰是「这条边不在生产段」最强的证明。
    // ⇒ 改成数「有几个读者有非空生产段」：实测 10 个读者里 **9 个**有
    //（只有 `polling_registry.rs` 整体是判据）。地板留一格余量。
    let mut with_production = 0usize;
    for (dir, reader, target, _) in CROSS_EDGES {
        let raw = std::fs::read_to_string(root.join(reader)).unwrap_or_default();
        assert!(
            raw.len() > 500,
            "读 {reader} 只拿到 {} 字节 —— 读不到的文件只会静默返回空串",
            raw.len()
        );
        // 🔴 〔搬树 2026-09-18 · `设计/16 §5.4b`〕**住 `tests/` 的读者按构造不是生产段。**
        //
        // 剖分之前这些判据住在生产文件的 `#[cfg(test)]` 段里，所以「这条边在不在生产段」
        // 要靠 `production_code` 剥一遍才答得出来。今天它们整份住 `tests/` ——
        // 那棵树**一行都不进 `cargo build`**，问题按构造就没了。
        // ⚠ 不许把 `production_code` 硬套在 `tests/` 那一侧：那里没有 `#[cfg(test)] mod`
        //   可剥，剥法会原样交回整份文件，于是 `assert_no_test_code` 逐字报
        //   「剥完仍残留 28 个测试属性 —— 剥法坏了」。**那是尺子瞄错了地方，不是剥法坏了。**
        // ⇒ 换成一条**正控**：住 `tests/` 的读者必须真的是测试文件（里面有 `#[test]`）。
        //   没有这一条，「它在 tests/ 下所以不是生产代码」就成了一句没人核的假设。
        let in_tests_tree = reader.starts_with("tests/");
        let prod = if in_tests_tree {
            let attr = format!("#[{}]", "test");
            assert!(
                raw.contains(attr.as_str()),
                "{reader} 住在 `tests/` 下，却一个 `{attr}` 都没有 —— \
                     「它按构造不是生产代码」这句话此刻没有证据"
            );
            String::new()
        } else {
            let prod = guard_core::production_code(&raw);
            guard_core::assert_no_test_code(reader, &prod);
            prod
        };
        if prod.len() > 200 {
            with_production += 1;
        }
        // 中间量：原文里真的看得见这条边（不然是路径/文件对错了）。
        let leaf = Path::new(target)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        assert!(
            raw.contains(&leaf) && raw.contains(verb.as_str()),
            "{reader} 里看不到 `{leaf}` 或任何 `include_*!` —— 登记表与现实漂了"
        );
        // 真正的断言：生产段里不许出现指向对面那一半的 include。
        for (lit, resolved) in includes_in(reader) {
            // ⚠ 同一个坑的第二处：生产段那条断言也不能要求连写 ⇒
            // 改成「生产段里存在一处 include，且它的字面量就是这一条」。
            let in_prod = includes_in_text(&prod).iter().any(|(l, _)| *l == lit);
            if resolved == *target && in_prod {
                offenders.push(format!("  [{dir}] {reader} 的**生产段**读 {target}"));
            }
        }
    }
    // 🔴 〔搬树 2026-09-18〕**这条地板从 8 拧到 3，逐份点名它为什么该拧。**
    //
    // 它量的是「`production_code` 还在不在干活」。分母 = 有生产段的读者数，
    // 而 16 个读者里今天**只有 3 个还住生产树**：
    //   ① `src/backend/control/gate.rs`   ② `src/backend/control/launch.rs`
    //   ③ `src/backend/relay/route.rs`
    // 另外 13 个逐份点名，一个都不是「文件没了」：
    //   ④ `src/bridge/src/polling_registry.rs` —— **早就是 0**（整个模块带
    //      `#[cfg(test)]`，上一版那句「十个里九个」说的就是它这一格）；
    //   ⑤–⑯ 12 个判据**整份搬去了 `tests/`**（`daemon_kill_tests.rs` ·
    //      `daemon_launch_tests.rs` · `daemon_send_keys_tests.rs` · `history_tests.rs`×2 ·
    //      `inbound_client_tests.rs`×2 · `local_daemon_tests.rs` ·
    //      `search_kou_jing_guard.rs` · `ssh_source_emits_parity.rs` ·
    //      `ssh_source_f032_idle_tests.rs` · `tmux_tests.rs`）——
    //      它们**按构造**没有生产段，上面那条正控逐个核过「它真的是测试文件」。
    // ⚠ 拧下去的是**分母**，不是灵敏度的门槛：3 个全都掉到空串，这条照样红。
    assert!(
        with_production >= 3,
        "住生产树的那 3 个读者里只有 {with_production} 个有非空生产段 —— \
             `guard_core::production_code` 多半坏了，此刻本条在拿空串做零命中\
             （逐份点名见上面那段注释）"
    );
    assert!(
        offenders.is_empty(),
        "跨半边的编译期边长进了生产段：\n{}\n\
             ⇒ 从此 daemon 的部署构建（`cargo build`）需要 monitor 那棵树在，\
             而它的 `Cargo.toml` 逐字写着 standalone。\n\
             正解：把这次对拍搬进 `#[cfg(test)]`；真需要在运行期拿到那份内容，\
             就把它做成协议字段或夹具，别做成编译期依赖。",
        offenders.join("\n")
    );
}
