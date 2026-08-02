//! U6a（2026-08-02）：**`doc/IPC-PROTOCOL.md` 与真实协议面的对拍。**
//!
//! # 为什么需要它
//!
//! 那份文档是 daemon↔monitor（以及 aterm）之间的**权威契约**，而 U6 要在它上面加双向通道。
//! 摸底实测缺口（**以本护栏首跑的机检结果为准，不是手工 grep 的估数**——
//! 手工那版报 8 个字段 / 6 个子命令，两个数都不对）：
//!
//! - **7 个线上字段**从没进过文档：`agent_kind` `byte_offset` `codex_dir` `emits`
//!   `kinds` `liveness_confidence` `observation`。手工那版还多报了一个 `next` ——
//!   它是 `SeqCounter` 的进程内计数器，根本不上线（见下方 `wire_field_names` 的取法说明）。
//! - **2 个子命令**全文档零出现（`--account-trust-zero` `--tmux-notify`）；
//!   另有 `--fork-session` 出现在别处但不在一次性查询表里。手工那版把
//!   `--list-accounts` / `--search` / `--session-accounts` 也算成漏了，实际早在表里。
//! - 另有一处**比漏写更糟**的：`tmux_sessions` 帧的字段文档里叫 `classification`，
//!   而全仓（daemon + monitor）没有任何东西叫这个名字 —— 线上真名是 `observation`。
//!   照文档写的客户端会永远读到 `None`、退回「保守跳过」，正是这字段当初要修的 idle 灰灯。
//!
//! 拿这份文档当冻结基线，等于把这些缺口固化进新协议。
//!
//! 但「这次补齐」不解决问题：这些本来也是一条条加进代码时忘了同步文档的。
//! **没有机检，补完就会重新开始漂。** 计划原话：「没有它，那些漏列一条都发不出来」。
//!
//! # 判据
//!
//! - `wire.rs` 里每个 serde 字段名，**必须在文档里出现过**。
//! - **全部分派文件**（不只 `main.rs`）里的每个 `--子命令`，**必须落进 §10 的代码跨度**
//!   —— 不是「全文出现过就行」。⚠ 这句话在 U8a-2b 之前是**过期的宣称**：实现一直是
//!   `DOC.contains()` 全文子串，散文里提一句就算过。已收紧，两者现在对得上。
//! - 每条**入方向命令**必须落进 §10「入方向」那一**小节**的代码跨度（收到小节是因为
//!   §10 的帧字段表里本来就有 `status`/`name`/`sid` 这些词，一条同名命令能零文档白嫖）。
//! - 那份分派文件名单本身，必须囊括 `src/` 下每一个做分派的文件（`dispatch_registry_is_complete`）。
//!
//! 后两条都是 D 审计逼出来的收紧。第一版判据是「`main.rs` 的子命令在全文出现过」，两头都太松：
//! **判据松** —— 散文里顺带提一句就算过，而读者是照着表实现客户端的；
//! **抽取面松** —— `--read-session-from-offset` 分派在 `observe/history_query.rs`、文档零出现，
//! 护栏**结构上根本扫不到它**、照常报绿。后者更危险：少一条能看出来，少看一片地方看不出来。
//!
//! # 它挡不住什么（如实登记 —— 本仓在「宣称强度前先验证」上栽过）
//!
//! - 只查**出现**，不查**描述得对不对**。文档里写一句「`emits` 字段（已废弃）」也算通过。
//!   真正的语义正确性没有机器判据，只能靠人读。
//! - 只查一个方向：**文档里有而代码里没有**的字段/子命令不会红（那是「文档超前」，
//!   危害小于「文档滞后」——前者读者会发现对不上，后者读者根本不知道有这东西）。
//! - 字段名靠 `^    pub? name:` 形态的正则抽取。用 `#[serde(rename)]` 改过名的字段抽的是
//!   Rust 名而不是线上名 —— **今天 `wire.rs` 里零个 rename**（已核），真加了要同步改本护栏。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#![cfg(test)]

/// 做子命令分派的**全部**文件。
///
/// 第一版只有 `main.rs` —— U6a 的 D 审计当场抓到代价：`--read-session-from-offset`
/// 在 `observe/history_query.rs` 里分派、文档**零出现**，而护栏**结构上扫不到它**。
/// 同族还有 `--list-projects` / `--list-sessions` / `--read-session-tail`（也在 history_query）
/// 与 `--include-tools` / `--scope` / `--after-ms` / `--limit`（在 search_query）。
///
/// 「抽取面画小了」是比「少写一条」更隐蔽的失效：护栏照常报绿，而它压根没看那片地方。
/// 所以下面 `dispatch_registry_is_complete` 会**反向核对**这份名单没漏文件。
const DISPATCH_FILES: &[(&str, &str)] = &[
    ("main.rs", include_str!("main.rs")),
    (
        "observe/history_query.rs",
        include_str!("observe/history_query.rs"),
    ),
    (
        "observe/accounts_query.rs",
        include_str!("observe/accounts_query.rs"),
    ),
    (
        "observe/search_query.rs",
        include_str!("observe/search_query.rs"),
    ),
    // 它不做 match 分派，只在用法串里提自己的名字 —— 但 D 审计正是把一个
    // `pub const CTRL_FLAG: &str = "--ccm-hidden-ctrl";` 藏在这里绕过了护栏。
    // 放宽后的探测把它揪了出来。
    ("control/tmux_hook.rs", include_str!("control/tmux_hook.rs")),
];

/// 分派里出现的所有 `--子命令` / `--选项`（跨 [`DISPATCH_FILES`] 全部文件）。
pub(crate) fn dispatched_subcommands() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (_, raw) in DISPATCH_FILES {
        let src = crate::guard_support::production_code(raw);
        let mut from = 0usize;
        while let Some(rel) = src[from..].find("\"--") {
            let i = from + rel + 1;
            let tail = &src[i..];
            if let Some(end) = tail[1..].find('"') {
                let tok = &tail[..end + 1];
                // 字符集必须**宽于**今天用到的形态。旧版是 `[a-z-]`，D 审计实测
                // `--control-v2`（数字）/ `--ccm_hidden`（下划线）/ 大写全被**静默丢弃**
                // —— 护栏对它们不是「查过觉得没问题」，是**根本没看见**。
                if tok.len() > 2
                    && tok[2..]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    let s = tok.to_string();
                    if !out.contains(&s) {
                        out.push(s);
                    }
                }
            }
            from = i + 2;
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::{dispatched_subcommands, DISPATCH_FILES};

    const DOC: &str = include_str!("../../doc/IPC-PROTOCOL.md");

    /// `wire.rs` 生产段里所有**会上线**的字段名。
    ///
    /// # 取法：括号配平，**不是**逐行认形状
    ///
    /// 前三版都是按行认形状（`#[derive(` 开头、缩进 4 或 8、`pub ` 前缀……），
    /// D 审计一口气攻破了**五种 fmt-clean 的写法**，每一种都让整个类型或整行字段隐形，
    /// 而 `fields.len() >= N` 的自检**不响**（部分隐形，总数还在地板之上）：
    ///
    /// | 写法 | 旧版为什么瞎 |
    /// |---|---|
    /// | derive 列表超 100 列 | **rustfmt 自己**折成一行一个，`Serialize` 不在 `#[derive(` 那行 |
    /// | 子模块里的类型 | derive 行缩进 4，`starts_with("#[derive(")` 不命中 |
    /// | `pub(crate) f: T` | `strip_prefix("pub ")` 失配，名字变成 `pub(crate) f` 被字符集丢掉 |
    /// | 带 `where` 子句 | rustfmt 把 `where` 放列 0，区间当场收尾、body 为空 |
    /// | 单行 variant | `TurnEnd { session_id: String, uuid: String }` —— 只切第一个冒号，名字带上了 `TurnEnd {` |
    ///
    /// 最后一条**今天就在漏**：`uuid`（TurnEnd）与 `dropped`（Overflow）从来没被扫到过，
    /// 抽到 25 个而实际 27 个。它们碰巧在文档里，所以没暴雷。
    ///
    /// 现在改成：把 `#[derive(…)]` 当**可能跨行的属性**读到配平的 `)]`；类型体按
    /// **大括号配平**取；体内按「标识符 + `:`（非 `::`）且前一个非空白是 `{` / `,` / 行首」
    /// 抽字段 —— 缩进、`pub(crate)`、单行 variant、`where` 全都不再影响它。
    fn wire_field_names() -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (_, _, fields) in serializable_types() {
            for f in fields {
                if !out.contains(&f) {
                    out.push(f);
                }
            }
        }
        out.sort();
        out
    }

    /// 每个 `derive(Serialize|Deserialize)` 的类型：(类型声明行, 体是否含字段, 抽到的字段)。
    fn serializable_types() -> Vec<(String, bool, Vec<String>)> {
        let src = crate::guard_support::production_code(include_str!("wire.rs"));
        let b = src.as_bytes();
        let mut out: Vec<(String, bool, Vec<String>)> = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = src[from..].find("#[derive(") {
            let d = from + rel;
            // 属性可能跨行：从 `#[` 起按方括号配平读到底。
            let Some(attr_end) = balanced(b, d + 1, b'[', b']') else {
                break;
            };
            let is_ser = src[d..=attr_end].contains("Serialize")
                || src[d..=attr_end].contains("Deserialize");
            // 类型体 = 声明之后第一个 `{` 起、按大括号配平。tuple struct（`;` 结尾、无体）跳过。
            let semi = src[attr_end..].find(';').map(|k| attr_end + k);
            let Some(open) = src[attr_end..].find('{').map(|k| attr_end + k) else {
                break;
            };
            if semi.is_some_and(|s| s < open) {
                from = attr_end + 1;
                continue;
            }
            let Some(close) = balanced(b, open, b'{', b'}') else {
                break;
            };
            if is_ser {
                let body = &src[open + 1..close];
                let decl = src[attr_end + 1..open]
                    .trim()
                    .lines()
                    .next_back()
                    .unwrap_or("?")
                    .trim()
                    .to_string();
                // 「体里有字段」= 存在一个非 `::` 的冒号。fieldless enum（如 `RemovalCause`）没有。
                let has_fields = body.match_indices(':').any(|(i, _)| {
                    body.as_bytes().get(i + 1) != Some(&b':')
                        && (i == 0 || body.as_bytes()[i - 1] != b':')
                });
                let mut fields = Vec::new();
                collect_fields(body, &mut fields);
                out.push((decl, has_fields, fields));
            }
            from = close + 1;
        }
        out
    }

    /// ★ 每个**有字段的**可序列化类型都必须至少产出一个字段。
    ///
    /// # 为什么光有 `fields.len() >= N` 的地板不够
    ///
    /// D 审计的核心发现：那条地板只挡得住「抽取器**整体**坏掉」。它挡不住**部分隐形** ——
    /// rustfmt 把某个 derive 折行、某个类型挪进子模块、某个字段写 `pub(crate)`，
    /// 那个类型整个消失，而总数还在地板之上，护栏**照常报绿**。
    /// 实测四种 fmt-clean 的写法都能这么骗过去。
    ///
    /// 这条是按类型的：一个有字段的类型抽出 0 个，当场红 —— 与总数无关。
    #[test]
    fn every_serializable_type_yields_at_least_one_field() {
        let types = serializable_types();
        assert!(
            types.len() >= 3,
            "只扫到 {} 个可序列化类型 —— 抽取坏了，本断言在空转",
            types.len()
        );
        let blind: Vec<&String> = types
            .iter()
            .filter(|(_, has, f)| *has && f.is_empty())
            .map(|(d, _, _)| d)
            .collect();
        assert!(
            blind.is_empty(),
            "这些可序列化类型体里有字段，但抽取器一个都没抽到 —— **它对这个类型是瞎的**：{blind:?}\n\
             总数地板挡不住这种部分隐形（D 审计实测四种 fmt-clean 写法都能这么过）。"
        );
    }

    /// 从 `at`（该处即 `open`）起找配平的闭合符，返回它的下标。
    fn balanced(b: &[u8], at: usize, open: u8, close: u8) -> Option<usize> {
        let mut depth = 0i32;
        for (i, &c) in b.iter().enumerate().skip(at) {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        None
    }

    /// 从类型体里抽字段名。
    ///
    /// 判据：标识符紧跟 `:`（且不是 `::`），且它**前面第一个非空白字符**是
    /// `{` / `,` / 什么都没有（体的开头）。这样：
    /// - 单行 variant 的第二个及以后的字段（前面是 `,`）也抽得到；
    /// - 类型里的 `Vec<HashMap<String, u64>>` 不会被误当字段（`String` 后面不是 `:`）；
    /// - `#[serde(rename = "x")]` 里的 `rename` 前面是 `(`，不匹配。
    fn collect_fields(body: &str, out: &mut Vec<String>) {
        // 先把可见性前缀抹成空白：`pub` / `pub(crate)` / `pub(super)` / `pub(in …)`。
        // 不抹的话 `pub id: String` 里 `id` 前面的非空白是 `b`，会被下面的 prev 判据否掉
        // （旧版靠 `strip_prefix("pub ")`，正是 D 审计用 `pub(crate)` 攻破的那处）。
        let body = &blank_out_visibility(body);
        let bb = body.as_bytes();
        let mut i = 0usize;
        while i < bb.len() {
            if !(bb[i].is_ascii_alphabetic() || bb[i] == b'_') {
                i += 1;
                continue;
            }
            let start = i;
            while i < bb.len() && (bb[i].is_ascii_alphanumeric() || bb[i] == b'_') {
                i += 1;
            }
            let name = &body[start..i];
            // 后面必须是（可选空白 +）`:` 且不是 `::`
            let mut j = i;
            while j < bb.len() && (bb[j] as char).is_whitespace() {
                j += 1;
            }
            if j >= bb.len() || bb[j] != b':' || bb.get(j + 1) == Some(&b':') {
                continue;
            }
            // 前面第一个非空白必须是 `{` / `,` / 体开头
            let mut k = start;
            while k > 0 && (bb[k - 1] as char).is_whitespace() {
                k -= 1;
            }
            // 前一个非空白：`{`（体开头后第一个字段）/ `,`（同级下一个字段）/
            // `]`（前面是 `#[serde(...)]` 之类的属性）/ 体的最开头。
            let prev_ok = k == 0 || bb[k - 1] == b'{' || bb[k - 1] == b',' || bb[k - 1] == b']';
            if !prev_ok {
                continue;
            }
            if name.chars().any(|c| c.is_ascii_uppercase()) {
                continue; // 类型名而非字段名
            }
            let n = name.to_string();
            if !out.contains(&n) {
                out.push(n);
            }
        }
    }

    /// 把 `pub` / `pub(crate)` / `pub(super)` / `pub(in …)` 抹成等长空白。
    ///
    /// 等长是为了不打乱后面按下标取的名字切片。
    fn blank_out_visibility(body: &str) -> String {
        let mut out = body.to_string();
        while let Some(i) = find_word(&out, "pub") {
            let mut end = i + 3;
            let bytes = out.as_bytes();
            let mut j = end;
            while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                j += 1;
            }
            if bytes.get(j) == Some(&b'(') {
                if let Some(c) = balanced(bytes, j, b'(', b')') {
                    end = c + 1;
                }
            }
            out.replace_range(i..end, &" ".repeat(end - i));
        }
        out
    }

    /// 找 `word` 作为**完整单词**出现的第一处（两侧都不是标识符字符）。
    fn find_word(hay: &str, word: &str) -> Option<usize> {
        let b = hay.as_bytes();
        let mut from = 0usize;
        while let Some(rel) = hay[from..].find(word) {
            let i = from + rel;
            let before_ok = i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
            let j = i + word.len();
            let after_ok = j >= b.len() || !(b[j].is_ascii_alphanumeric() || b[j] == b'_');
            if before_ok && after_ok {
                return Some(i);
            }
            from = i + word.len();
        }
        None
    }

    /// ★ `wire.rs` 里**一个 serde 改名都不许有**。
    ///
    /// [`wire_field_names`] 抽的是 **Rust 名**。一旦有 `#[serde(rename = "x")]` 或
    /// `rename_all`，线上名就与 Rust 名分家 —— 护栏会拿 Rust 名去文档里找、找到了就放行，
    /// 而**线上那个名字在文档里 0 命中**。D 审计实测这条能骗过护栏。
    ///
    /// 旧版头注写着「今天 wire.rs 里零个 rename（已核）」—— 那是**一句没有判据的前提**。
    /// 现在它有判据了。真要加 rename，就得同时教会 `wire_field_names` 抽线上名。
    ///
    /// # 什么算「改字段名」
    ///
    /// | 写法 | 判定 | 为什么 |
    /// |---|---|---|
    /// | 字段上的 `#[serde(rename = "…")]` | **红** | 直接改字段的线上名 |
    /// | **struct** 上的 `rename_all` | **红** | 改的就是它所有字段 |
    /// | **enum** 上的 `rename_all` | 放行 | 改的是 **variant 名**（`Frame` 的 `kind` 取值、`RemovalCause` 的取值），不碰字段。`Frame` 与 `RemovalCause` 今天都靠它 |
    /// | **enum** 上的 `rename_all_fields` | **红** | 这个才是改 variant 里的字段 |
    #[test]
    fn wire_rs_has_no_serde_rename_on_fields() {
        let src = crate::guard_support::production_code(include_str!("wire.rs"));
        let lines: Vec<&str> = src.lines().map(str::trim).collect();
        let mut offenders: Vec<String> = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            if !(l.starts_with("#[serde(") && l.contains("rename")) {
                continue;
            }
            // `rename_all_fields` 改的是 variant 里的字段 —— 无论挂谁身上都红。
            if l.contains("rename_all_fields") {
                offenders.push((*l).to_string());
                continue;
            }
            // 往下找它修饰的是什么：enum 上的 `rename_all` 改 variant 名，放行。
            let target = lines[i + 1..]
                .iter()
                .find(|x| !x.starts_with("#[") && !x.starts_with("///") && !x.is_empty());
            let on_enum = target.is_some_and(|t| t.contains("enum "));
            if l.contains("rename_all") && on_enum {
                continue;
            }
            offenders.push((*l).to_string());
        }
        assert!(
            offenders.is_empty(),
            "wire.rs 出现了 serde 改名：{offenders:?}\n\
             `wire_field_names` 抽的是 **Rust 名**，改名之后线上名与它分家 ——\n\
             护栏会拿 Rust 名去文档里找、找到就放行，而线上那个名字文档里 0 命中。\n\
             要加改名，先教会 `wire_field_names` 抽线上名。"
        );
    }

    /// ★ [`DISPATCH_FILES`] 必须囊括 `src/` 下**每一个**做子命令分派的文件。
    ///
    /// 这条是给上面那个抽取面兜底的：漏登记一个文件，等于那个文件里的子命令
    /// 永远不受文档对拍约束，而护栏**照常报绿**。U6a 之前就正是这个状态。
    #[test]
    fn dispatch_registry_is_complete() {
        fn walk(dir: &std::path::Path, hits: &mut Vec<String>, root: &std::path::Path) {
            for e in std::fs::read_dir(dir).expect("读 src/ 失败").flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, hits, root);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let raw = std::fs::read_to_string(&p).unwrap_or_default();
                    // 整份文件都是 `#![cfg(test)]` 的（护栏自己就是）：release 构建里
                    // 根本不存在，不可能分派任何东西。`production_code` 只剥
                    // `#[cfg(test)] mod` 块，剥不掉文件级的内层属性 —— 得在这里跳。
                    if raw.contains("#![cfg(test)]") {
                        continue;
                    }
                    // 判据放宽到「生产段里出现 `"--` 字面量」。
                    //
                    // 前两版按**分派语法**探测（`Some("--x") =>` / 裸 `"--x" =>`），
                    // D 审计一击即破：把 `pub const CTRL_FLAG: &str = "--ccm-hidden-ctrl";`
                    // 放进 `control/tmux_hook.rs`（那文件两种语法都没有）⇒ 两条护栏全绿。
                    //
                    // 现在只要**提到**一个 `--token` 就得登记。宁可多登记几个文件，
                    // 也不要让一个 token 因为「写法不是 match 臂」而隐形。
                    let prod = crate::guard_support::production_code(&raw);
                    if prod.contains("\"--") {
                        hits.push(
                            p.strip_prefix(root)
                                .unwrap()
                                .to_string_lossy()
                                .replace('\\', "/"),
                        );
                    }
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        walk(&root, &mut found, &root);
        found.sort();

        assert!(
            found.len() >= 4,
            "只扫到 {} 个分派文件 —— 抽取坏了，本断言在空转：{found:?}",
            found.len()
        );
        let mut registered: Vec<String> =
            DISPATCH_FILES.iter().map(|(n, _)| n.to_string()).collect();
        registered.sort();
        assert_eq!(
            found, registered,
            "\n做子命令分派的文件集变了，但 `DISPATCH_FILES` 没跟上。\n\
             漏登记的文件里，所有 `--子命令` 都不受 IPC-PROTOCOL.md 对拍约束，\n\
             而 `every_dispatched_subcommand_appears_in_the_protocol_doc` **照常报绿**。\n\
             把新文件加进 `DISPATCH_FILES`（含 `include_str!`）。"
        );
    }

    /// 文档里所有**反引号代码跨度**内出现的标识符（按非标识符字符切词）。
    ///
    /// 为什么要这一步而不是直接 `DOC.contains(name)`：见调用处。一句话——
    /// 散文里的常见词不算「文档化」，而逗号连写的跨度里的词算。
    fn code_span_identifiers(doc: &str) -> std::collections::HashSet<String> {
        let mut out = std::collections::HashSet::new();
        // 按反引号切：奇数段（下标为奇）是跨度内容。三反引号围栏也被这么切，
        // 段落切得碎但对「取词」无影响（我们只关心词的集合）。
        for (i, seg) in doc.split('`').enumerate() {
            if i % 2 == 0 {
                continue;
            }
            for tok in seg.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                if !tok.is_empty() {
                    out.insert(tok.to_string());
                }
            }
        }
        out
    }

    /// 取所有反引号代码跨度的**原文**拼起来。
    ///
    /// 与 [`code_span_identifiers`] 的区别：那个按「标识符字符」切词，`--usage` 会被切成
    /// `usage`，于是「文档里写了 `usage` 但没写 `--usage`」也算过。子命令那条要的是**原样**。
    fn code_span_text(doc: &str) -> String {
        doc.split('`')
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, seg)| seg)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// ★ 文档必须提到 wire 的每一个字段。
    #[test]
    fn every_wire_field_appears_in_the_protocol_doc() {
        let fields = wire_field_names();
        assert!(
            fields.len() >= 15,
            "只抽到 {} 个 wire 字段 —— 抽取坏了，本断言在空转：{fields:?}",
            fields.len()
        );
        // ★ 判据是「作为标识符出现在某个**代码跨度之内**」，不是「字面量在全文任何地方出现过」。
        //
        // 松判据在 U6b-1 当场失效：新加的 `Reply { id, ok, code, message }` 四个字段
        // **一条都没写进文档**，护栏却全绿 —— `ok` / `id` 这种词在 570 行中文文档里必然出现过。
        //
        // 但也不能要求「自成一个跨度」（`` `ok` ``）：文档里有 `v, build_id, host_arch, …`
        // 这种**逗号连写在同一个跨度**的写法，那样会把 3 个既有的、确实有文档的字段误报。
        // ⇒ 取所有反引号跨度的内容、按标识符切词，字段名必须是其中一个**完整词**。
        // ★ **只在 §10「远端 daemon wire 协议」那一节里找**（U6b-3，据 D 审计收紧）。
        //
        // 收紧前判据是「全文的代码跨度」。审计实测强度：把 §10 的帧字段表整段
        // （12 002 字节 / 69 行）从文档里挖掉，**31 个字段里 16 个照样通过** ——
        // 它们命中的是**毫不相干的代码跨度**：`sid` 命中文件名 `sid-hwnd-cache.json`、
        // `attachable` 命中 BUILD_ID 名 `p1v-attachable`、`path`/`raw` 命中一段
        // PowerShell 片段、`message` 命中一个前端事件的 payload。
        //
        // 也就是说：**一半的字段就算权威表被删干净，护栏也照样绿。**
        // 子命令那条早已收紧成「必须落进 §10 的两张表之一」，字段这条一直停在全文。
        let sec = DOC
            .find("## 10. 远端 daemon wire 协议")
            .expect("文档里找不到 §10 —— 抽取坏了还是文档被大改了？");
        let sec_end = DOC[sec..]
            .find("\n## ")
            .map(|k| sec + k)
            .unwrap_or(DOC.len());
        let wire_section = &DOC[sec..sec_end];
        assert!(
            wire_section.len() > 8000,
            "§10 区间只抽到 {} 字节 —— 抽取坏了，本断言在空转",
            wire_section.len()
        );
        let documented = code_span_identifiers(wire_section);
        assert!(
            documented.len() >= 60,
            "只从 §10 的代码跨度里切出 {} 个标识符 —— 抽取坏了，本断言在空转",
            documented.len()
        );
        let missing: Vec<&String> = fields.iter().filter(|f| !documented.contains(*f)).collect();
        assert!(
            missing.is_empty(),
            "这些 wire 字段不在 `doc/IPC-PROTOCOL.md` **§10 wire 协议节**里：{missing:?}\n\
             那份文档是 daemon↔monitor↔aterm 的权威契约。字段加进代码却没进文档，\n\
             下游只能靠读源码或抓包才知道它存在。"
        );
    }

    /// ★ **入方向命令名**也必须在 §10 里（U7-5 补的第三条）。
    ///
    /// # 这条是 U7-5 扫「自洽夹具」时顺带发现的空白
    ///
    /// wire 字段有对拍、`--子命令` 有对拍，**入方向命令名一直没有**。
    /// `hello_commands_match_the_dispatch_table` 钉的是 `COMMANDS ↔ dispatch 分派臂`，
    /// 两边都在代码里 —— **文档不在其中**。
    ///
    /// 实测：把 `ping` 在 `COMMANDS` 与分派臂里**同时**改名（= 正常地重命名一条命令），
    /// 只有 `ping_replies_ok` 这条**行为测试**红了 —— 而重命名时那条自然会被一起改。
    /// 改完之后文档里的 `ping` 就成了一个不存在的命令，**没有任何东西会响**。
    ///
    /// 客户端是照文档发命令的：文档说 `ping`、daemon 只认 `heartbeat`，
    /// 表现是 `unknown_command`，而两边各自看都"对"。
    #[test]
    fn every_inbound_command_appears_in_the_protocol_doc() {
        let sec = DOC
            .find("## 10. 远端 daemon wire 协议")
            .expect("文档里找不到 §10 —— 抽取坏了");
        let sec_end = DOC[sec..]
            .find("\n## ")
            .map(|k| sec + k)
            .unwrap_or(DOC.len());
        // ★ **收到「入方向」那一小节**（U8a-2b）。
        //
        // 原来扫的是 §10 整节，而 §10 里的帧字段表本来就有 `status` / `path` / `name` / `sid`
        // 这些词 ⇒ **一条叫 `status` 的命令零文档也直接通过**（D 设计审计 · 视角 A · P3）。
        // 收到入方向小节之后，命令名要想白嫖就得恰好撞上入方向小节里的某个词，面小得多。
        let inbound_at = DOC[sec..sec_end]
            .find("### 入方向：流连接上的命令信封")
            .map(|k| sec + k)
            .expect("§10 里找不到「入方向」小节 —— 抽取坏了还是文档被大改了？");
        let inbound_end = DOC[inbound_at + 4..sec_end]
            .find("\n### ")
            .map(|k| inbound_at + 4 + k)
            .unwrap_or(sec_end);
        let documented = code_span_identifiers(&DOC[inbound_at..inbound_end]);
        assert!(
            documented.len() >= 25,
            "只从「入方向」小节切出 {} 个标识符 —— 抽取坏了，本断言在空转",
            documented.len()
        );
        let cmds = crate::inbound::COMMANDS;
        assert!(
            cmds.len() >= 2,
            "只有 {} 条命令 —— 抽取坏了，本断言在空转",
            cmds.len()
        );
        let missing: Vec<&&str> = cmds.iter().filter(|c| !documented.contains(**c)).collect();
        assert!(
            missing.is_empty(),
            "这些入方向命令不在 `doc/IPC-PROTOCOL.md` §10 里：{missing:?}\n\
             客户端是照文档发命令的 —— 文档说一个名字、daemon 只认另一个，\n\
             表现是 `unknown_command`，而两边各自看都「对」。"
        );
    }

    /// ★ 文档必须提到每一个被分发的子命令。
    #[test]
    fn every_dispatched_subcommand_appears_in_the_protocol_doc() {
        let cmds = dispatched_subcommands();
        assert!(
            cmds.len() >= 8,
            "只抽到 {} 个子命令 —— 抽取坏了，本断言在空转：{cmds:?}",
            cmds.len()
        );
        // ★ **收紧到 §10 的代码跨度**（U8a-2b）。
        //
        // 原实现是 `DOC.contains(c)` —— **全文子串**，散文里提一句就算过。
        // 而本文件头注两处都写着「必须落进 §10 的两张表之一 —— 不是『全文出现过就行』」。
        // 那是一份**过期的宣称**，正是本仓最忌讳的那种：护栏自称的强度比实际高一档。
        // （D 设计审计 · 视角 A · P7 点名。字段那条早在 U6a 就收到 §10 了，这条没跟。）
        let sec = DOC
            .find("## 10. 远端 daemon wire 协议")
            .expect("文档里找不到 §10 —— 抽取坏了");
        let sec_end = DOC[sec..]
            .find("\n## ")
            .map(|k| sec + k)
            .unwrap_or(DOC.len());
        let spans = code_span_text(&DOC[sec..sec_end]);
        assert!(
            spans.len() > 2000,
            "只从 §10 切出 {} 字节的代码跨度 —— 抽取坏了，本断言在空转",
            spans.len()
        );
        let missing: Vec<&String> = cmds
            .iter()
            .filter(|c| !spans.contains(c.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "这些子命令没有落进 `doc/IPC-PROTOCOL.md` §10 的代码跨度里：{missing:?}\n\
             （在散文里提一句不算 —— §10 是给仓外读的冻结契约，命令要出现在表里。）"
        );
    }
}
