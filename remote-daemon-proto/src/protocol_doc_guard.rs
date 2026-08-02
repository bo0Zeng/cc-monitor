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
//! - **全部分派文件**（不只 `main.rs`）里的每个 `--子命令`，**必须落进 §10 的两张表之一**
//!   （流模式 flag 表 / 一次性查询表）—— 不是「全文出现过就行」。
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

#[cfg(test)]
mod tests {
    const DOC: &str = include_str!("../../doc/IPC-PROTOCOL.md");

    /// `wire.rs` 生产段里所有**会上线**的字段名。
    ///
    /// # 只看 `derive(..., Serialize)` 的类型
    ///
    /// 第一版扫全文件的 `name: Type` 形态，把 `SeqCounter { next: HashMap<..> }` 也报了出来 ——
    /// 那是**进程内的每文件序号计数器，根本不上线**。
    ///
    /// 这是一次「红得不对」：护栏该在文档漏字段时红，而不是在文档没提一个内部字段时红。
    /// **报绿要怀疑夹具，报红同样要**（本仓已有多起「rc=101 其实是编译失败＝假红」的记录）。
    ///
    /// 取法：找每个含 `Serialize` 的 `#[derive(...)]` ⇒ **跳过它后面可能还跟着的列 0 属性行**
    /// ⇒ 跳过类型声明行 ⇒ 区间体一直到下一个从列 0 起的非空行（收尾大括号那行）。
    ///
    /// 前两版都栽在同一个坑的两种形态上，都被 `fields.len() >= 15` 自检抓住（**没有一次
    /// 是悄悄变绿的**，这条自检是这个夹具能信的唯一理由）：
    /// 1. 逐行维护「当前是否可序列化」的开关 —— 被 doc 注释与 `#[serde(...)]` 属性行搅乱，抽到 0 个。
    /// 2. 改成按区间取，但假定 `#[derive(...)]` 的下一行就是类型声明 —— `wire.rs` 里
    ///    第一个 derive 后面跟的是 `#[serde(rename_all = "snake_case")]`，于是区间收在
    ///    真正的声明行上，又抽到 0 个。
    fn wire_field_names() -> Vec<String> {
        let src = crate::guard_support::production_code(include_str!("wire.rs"));
        let lines: Vec<&str> = src.lines().collect();
        let mut out: Vec<String> = Vec::new();
        let mut i = 0usize;
        while i < lines.len() {
            if !lines[i].starts_with("#[derive(") {
                i += 1;
                continue;
            }
            // `Serialize` **或** `Deserialize`：U6b-1 加了入方向的 `Request`（只 Deserialize），
            // 第一版只认 Serialize ⇒ 它的 `cmd` / `args` 根本不被扫。出方向漏字段和
            // 入方向漏字段对下游是同一件事。
            let is_ser = lines[i].contains("Serialize") || lines[i].contains("Deserialize");
            i += 1;

            // derive 之后**还可能跟着别的列 0 属性行**（wire.rs 里就是
            // `#[serde(rename_all = "snake_case")]`）。第一版没跳它们，把属性行
            // 当成了类型声明行 ⇒ 区间当场收在真正的声明行上 ⇒ 抽到 0 个字段。
            while i < lines.len() && lines[i].starts_with("#[") {
                i += 1;
            }
            i += 1; // 类型声明行本身

            // 区间体 = 直到下一个从列 0 起的非空行（那就是收尾大括号所在行）。
            //
            // 刻意不去搜那个右大括号的字面量：在字符串里写一个**不配对的**右大括号，
            // 会把 `readonly_guard::strip_cfg_test` 的括号配平提前收尾，让整个
            // `protocol_doc_guard.rs` 的 `#[cfg(test)]` 段漏进生产段
            // （`no_test_code_leaks_into_any_production_section` 实测抓到，§41.4 第 1 条纪律）。
            let body_start = i;
            while i < lines.len() && (lines[i].is_empty() || lines[i].starts_with(' ')) {
                i += 1;
            }
            if is_ser {
                for line in &lines[body_start..i] {
                    let line = *line;
                    let lt = line.trim_start();
                    let indent = line.len() - lt.len();
                    if indent != 4 && indent != 8 {
                        continue;
                    }
                    let rest = lt.strip_prefix("pub ").unwrap_or(lt);
                    let Some((name, tail)) = rest.split_once(':') else {
                        continue;
                    };
                    if tail.starts_with(':') || name.is_empty() {
                        continue;
                    }
                    if !name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                    {
                        continue;
                    }
                    let n = name.to_string();
                    if !out.contains(&n) {
                        out.push(n);
                    }
                }
            }
        }
        out.sort();
        out
    }

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
    ];

    /// 分派里出现的所有 `--子命令` / `--选项`（跨 [`DISPATCH_FILES`] 全部文件）。
    fn dispatched_subcommands() -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (_, raw) in DISPATCH_FILES {
            let src = crate::guard_support::production_code(raw);
            let mut from = 0usize;
            while let Some(rel) = src[from..].find("\"--") {
                let i = from + rel + 1;
                let tail = &src[i..];
                if let Some(end) = tail[1..].find('"') {
                    let tok = &tail[..end + 1];
                    if tok.len() > 2 && tok[2..].chars().all(|c| c.is_ascii_lowercase() || c == '-')
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
                    // 分派有两种写法，都得认：`Some("--x") =>`（main/history/accounts）
                    // 与裸 `"--x" =>`（search_query 的选项 match）。第一版只认前者，
                    // 扫到 3 个文件、漏了 `search_query.rs` —— 被本断言的 `>= 4` 地板抓住。
                    //
                    // 只看生产段：测试夹具里的 `Some("--x")` 之类不算分派点。
                    let prod = crate::guard_support::production_code(&raw);
                    if prod.contains("Some(\"--") || prod.contains("\" =>") && prod.contains("\"--")
                    {
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
        let documented = code_span_identifiers(DOC);
        assert!(
            documented.len() >= 60,
            "只从文档的代码跨度里切出 {} 个标识符 —— 抽取坏了，本断言在空转",
            documented.len()
        );
        let missing: Vec<&String> = fields.iter().filter(|f| !documented.contains(*f)).collect();
        assert!(
            missing.is_empty(),
            "这些 wire 字段在 `doc/IPC-PROTOCOL.md` 里**一次都没出现**：{missing:?}\n\
             那份文档是 daemon↔monitor↔aterm 的权威契约。字段加进代码却没进文档，\n\
             下游只能靠读源码或抓包才知道它存在。"
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
        let missing: Vec<&String> = cmds.iter().filter(|c| !DOC.contains(c.as_str())).collect();
        assert!(
            missing.is_empty(),
            "这些子命令在 `doc/IPC-PROTOCOL.md` 里**一次都没出现**：{missing:?}"
        );
    }
}
