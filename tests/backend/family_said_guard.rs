//! **一家对人的叫法只住 `agents::family_said_among` 一处**。
//!
//! 「这一家对用户叫什么」= 它行头（`Adapter`）上的 `display_name`；没声明 · 注册表里没有 ⇒ 文案表里照实的那一句（不露 kind）。
//! 各处要用就问 `family_said_among`，不自己读 `display_name`、不自己挑说不出时的退路。
//!
//! 判据数「自己拼家名」那一形：生产代码里读 `.display_name` 的行。人群是后端源码，扣掉：
//! - `agents/asset_face.rs`（`family_said_among` 本身住这里）；
//! - `footprint/`：那里的 `display_name` 是足迹表里一件工具的名字，另一个概念，与家无关。
//!
//! 同一形还数读 `.speaker_name` 的行：消息流里说话一方的短名（卡头 · 全文搜索与会话内查找的「谁」）是起会话面上另一格，
//! 后端读它只经 `agents::speaker_said_among`（住 `agents/launch_face.rs`，扣这一份）：没有起会话面 · 认不出 ⇒ 问 `family_said_among`。
//!
//! 登记表 [`tests::SELF_SPELLED`] 只许减：实得与表逐条对齐，多一处红、少一处也红（少了就把表改短）。
//!
//! 第二形「kind 顶替家名」：文案调用里装家名的那几格（`{agent}` · `{owner}` · `{who}`，即 `("agent", <值>)` …）的值
//! 不经 `family_said_among` · `speaker_said_among`，把 kind 或路由名原样交上界面。人群是整个后端源码（不扣住处与足迹表）。
//! 登记表 [`tests::KIND_IN_SLOT`] 同样只许减、逐条对齐。
//! 那一格装的不是 kind 的，登在 [`tests::NOT_A_KIND`]，逐处写明为什么放过，处数同样逐条对齐（多一处就得重判）：
//! - 「未知 agent {agent}」回显请求里那个认不出的码（`agents/launch_face.rs` 的 `unknown_agent_said`，ccm 那一行的渲染也经它）：
//!   注册表里没有它，没有叫法可问，照抄用户敲的原码是唯一说得出的；
//! - 值是前面已经问过 `family_said_among` 的叫法（`faces/quota_read.rs` 的 `family_tag` · `footprint/chores/mod.rs` 的
//!   `Relay.name` 出自 `settings_env_families` · `ccm_invocation.rs` 的 `Refusal::AgentNoAccounts.agent`
//!   · `observe/search_query.rs` 命中头一行照抄的 `FindHit.who`，由 `who_of_among` 问 `speaker_said_among` 写）；
//! - `{who}` 同字不同义：终端板那一行头上的号名（`faces/terminals_board_face.rs`），不是家。
//!
//! ccm 命令行那几处不放过：解析过的 `o.agent` 是认出来的 kind（不带 `--agent` 时是默认那一家，用户一个字都没敲），
//! 派活口交进来的 `ask.agent` 是调用方交的码，都问 `family_said_among`（认不出的码也只说名称未声明，不回显那个码）。
//!
//! ⚠ 诚实边界：认的是字面 `.display_name`。先把那一格拷进别的名字再读（`let n = face; n.display_name` 仍认得；
//! 换成解构 `Adapter { display_name, .. }` 就认不出；`.speaker_name` 同样）。第二形认的是字面 `("agent", …)` · `("owner", …)` · `("who", …)`：格名换了写法（`(KEY, v)`）就认不出；
//! 格名同字不同义的也照数（文案表里 `{owner}` 另有「文件属主」那一义，如 `credsPerm.judge.notOwner`，今天只在后端源码之外用）：
//! 后端哪天用上这一义，那一处登进 `NOT_A_KIND`、写明为什么；
//! 放过的那几处只按文件处数对齐，同一文件里拿一处坏的换掉一处放过的认不出。值里只看有没有 `family_said_among`
//! 这串字：一支问过、一支露 kind 的条件式（`if c { family_said_among(..) } else { k }`）认不出；括号配平不跳过字面量，
//! 值里有不配平的 `"("` 可能吞进后面的代码。只看 `{agent}` · `{owner}` · `{who}` 三格，`{own}` 这类别的格名不数。
//!
//! 第三形「默认那一家不写另判一次」：默认那一家不写家名 · 不画家 · 不写格，只在 `agents::unless_default` 判。
//! 认的是语义：拿默认那一家做比较（`==` · `!=`，哪一边都算），比的结果拿来定留下还是成缺 ——
//! 比较所在的括号（一层层往外，碰到别的调用的实参那一对就停）后面接 `.then_some(` · `.then(`、在 `.filter(` 里，
//! 或是 `if` 的条件、两支里有一支是 `None` / `return None`。
//! 「默认那一家」＝ 调 [`tests::DEFAULT_SOURCES`] 之一 · `account_kind_among(…, None)` 得到的，或同一函数里 `let` 从它绑上的名字；
//! 从形参进来的，同一文件里调它的那一处交的是默认那一家才认。与名字无关：经 `launch_face_among` 取的模型默认同名同形也放过。
//! 人群是整个后端源码，只扣住处那一个函数（[`tests::DEFAULT_HOME`]）；登记表 [`tests::DEFAULT_TWINS`] 只许减、逐条对齐。
//! 比了默认那一家、结果不定留不留的（「是不是账号库那一家」）不数。
//!
//! ⚠ 第三形的诚实边界：形参那一跳只在同一文件里追（跨文件交进来的默认那一家认不出）；`match` 的两臂、
//! `Option::xor` 这类不经比较的写法认不出；交默认那一家的函数换了名字（`DEFAULT_SOURCES` 自检只看还在不在）、
//! 或拷进结构体的一格再读，也认不出。会多认的：从默认那一家的行头再取别的格（`default_launch_among(r).and_then(|(_, f)| f.model_default)`）
//! 也当默认那一家；不看比较方向、不看留下的是不是被比的那个（`.filter(|f| Some(f.kind) != main)` 列别家也数）——
//! 多认了就看那一处是不是真另判了「默认那一家不写」，不是就改判据，不登表。
//! 今天真实源码里一处都没数到（住处的默认那一家从形参进来、同一文件里没有调用方交它），扣住处那一步在真实扫描里用不上；
//! 判据认得住处那一形、扫得到文件，靠的是 `the_default_home_is_the_one_exempted` 往住处拼一个调用方的自检。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    /// 「自己拼家名」还剩的地方：`(相对 src/backend 的文件, 处数)`。**只许减**。
    const SELF_SPELLED: &[(&str, usize)] = &[];

    /// 不在人群里的前缀（相对 `src/backend/`）：住处本身 · 另一个概念的足迹表。
    const EXEMPT: &[&str] = &["agents/asset_face.rs", "footprint/"];

    /// 「kind 顶替家名」还剩的地方：`(相对 src/backend 的文件, 处数)`。**只许减**。
    const KIND_IN_SLOT: &[(&str, usize)] = &[];

    /// `{agent}` · `{owner}` 那一格装的不是 kind（头注逐条写了为什么）：`(文件, 处数)`，逐条对齐。
    const NOT_A_KIND: &[(&str, usize)] = &[
        // 「未知 agent {agent}」：回显请求里认不出的那个码。
        ("agents/launch_face.rs", 1),
        // `AgentNoAccounts.agent`（造它时已问过 `family_said_among`）。
        ("control/launch_render/ccm_invocation.rs", 1),
        // `family_tag` 给的叫法（它问 `family_said_among`）。
        ("faces/quota_read.rs", 1),
        // `Relay.name` 出自 `settings_env_families`（它问 `family_said_among`）。
        ("footprint/chores/mod.rs", 1),
        // `{who}`：终端板那一行头上是号名（`beTermBoard.meta.line`），不是家。
        ("faces/terminals_board_face.rs", 1),
        // `{who}`：命中头一行三形照抄 `FindHit.who`，那一格由 `who_of_among` 写（它问 `speaker_said_among`）。
        ("observe/search_query.rs", 3),
    ];

    /// 一份生产代码里「自己拼家名」的行数。
    fn self_spelled_lines(prod: &str) -> usize {
        prod.lines().filter(|l| l.contains(".display_name")).count()
    }

    /// 说话一方的短名（`LaunchFace::speaker_name`）同一形：读它只在 `agents/launch_face.rs` 的 `speaker_said_among`。
    const SPEAKER_EXEMPT: &[&str] = &["agents/launch_face.rs"];

    /// 一份生产代码里自己读说话一方短名的行数。
    fn speaker_spelled_lines(prod: &str) -> usize {
        prod.lines().filter(|l| l.contains(".speaker_name")).count()
    }

    /// 文案里装家名的那几格的格名，与那一格的值问过叫法的认法：`{agent}` · `{owner}` 装家名，只认 `family_said_among`；
    /// `{who}` 装说话一方的短名，认 `speaker_said_among`（它说不出就问家名）与 `family_said_among`。
    const SLOTS: &[(&str, &[&str])] = &[
        ("\"agent\"", &["family_said_among"]),
        ("\"owner\"", &["family_said_among"]),
        ("\"who\"", &["family_said_among", "speaker_said_among"]),
    ];

    /// 一份生产代码里文案装家名那几格（[`SLOTS`]）的值没问过叫法的处数（折行的也认）。
    fn kind_in_slot(prod: &str) -> usize {
        SLOTS
            .iter()
            .map(|(k, asked)| kind_in_slot_of(prod, k, asked))
            .sum()
    }

    fn kind_in_slot_of(prod: &str, slot: &str, asked: &[&str]) -> usize {
        let b = prod.as_bytes();
        let mut n = 0;
        let mut at = 0;
        while let Some(i) = prod[at..].find(slot) {
            let key = at + i;
            at = key + slot.len();
            // 那一格 ＝ `(` 空白* 格名 空白* `,`（rustfmt 怎么折行都是这一形）。
            let Some(open) = prod[..key].trim_end().len().checked_sub(1) else {
                continue;
            };
            if b[open] != b'(' {
                continue;
            }
            // `can_of("agent", …)` 这类是函数调用的实参，不是文案那一格。
            if open > 0 && (b[open - 1].is_ascii_alphanumeric() || b[open - 1] == b'_') {
                continue;
            }
            let after = prod[at..].trim_start();
            if !after.starts_with(',') {
                continue;
            }
            let from = prod.len() - after.len() + 1;
            let mut depth = 1usize;
            let mut end = prod.len();
            for (j, c) in prod[from..].char_indices() {
                match c {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = from + j;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if !asked.iter().any(|a| prod[from..end].contains(a)) {
                n += 1;
            }
        }
        n
    }

    /// 两样合起来：读 `.display_name`（扣 [`EXEMPT`]）· 读 `.speaker_name`（扣 [`SPEAKER_EXEMPT`]）。
    fn measured() -> Vec<(String, usize)> {
        let mut got = measured_by(self_spelled_lines, EXEMPT);
        for (f, n) in measured_by(speaker_spelled_lines, SPEAKER_EXEMPT) {
            match got.iter_mut().find(|(g, _)| *g == f) {
                Some(w) => w.1 += n,
                None => got.push((f, n)),
            }
        }
        got.sort();
        got
    }

    fn measured_by(count: fn(&str) -> usize, exempt: &[&str]) -> Vec<(String, usize)> {
        let root = guard_core::addr::backend_src_root();
        let mut got: Vec<(String, usize)> = guard_core::scan_tree_excluding(&root, &["rs"], &[])
            .into_iter()
            .filter_map(|(p, src)| {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                if exempt.iter().any(|e| rel.starts_with(e)) {
                    return None;
                }
                let n = count(&guard_core::production_code(&src));
                (n > 0).then_some((rel, n))
            })
            .collect();
        got.sort();
        got
    }

    /// 实得与登记表逐条对齐。
    #[test]
    fn family_names_are_asked_not_spelled() {
        let got = measured();
        let want: Vec<(String, usize)> = SELF_SPELLED
            .iter()
            .map(|(f, n)| ((*f).to_string(), *n))
            .collect();
        assert_eq!(
            got, want,
            "\n「自己拼家名」（读 `.display_name`）的处数与登记表对不上。\n\
             多了 ⇒ 改成问 `agents::family_said_among(registry, kind)`；少了 ⇒ 把 `SELF_SPELLED` 改短。"
        );
    }

    fn table(t: &[(&str, usize)]) -> Vec<(String, usize)> {
        t.iter().map(|(f, n)| ((*f).to_string(), *n)).collect()
    }

    /// `{agent}` · `{owner}` 那一格的值都问 `family_said_among`：实得 ＝ 待改的（只许减）＋ 放过的，逐条对齐。
    #[test]
    fn the_family_slots_hold_a_family_name_not_a_kind() {
        let got = measured_by(kind_in_slot, &[]);
        let mut want = table(KIND_IN_SLOT);
        for (f, n) in table(NOT_A_KIND) {
            match want.iter_mut().find(|(g, _)| *g == f) {
                Some(w) => w.1 += n,
                None => want.push((f, n)),
            }
        }
        want.sort();
        assert_eq!(
            got, want,
            "\n文案 `{{agent}}` · `{{owner}}` 那一格不经 `family_said_among` 的处数与登记表对不上。\n\
             多了 ⇒ 改成 `(\"agent\", &family_said_among(registry, kind))`（`{{owner}}` 同样）（装的确实不是 kind ⇒ 登进 `NOT_A_KIND`、头注写明为什么）；\
             少了 ⇒ 把 `KIND_IN_SLOT` 改短。"
        );
    }

    /// 第二形的抽取：kind · 路由名直接进那一格认得（单行 · 折行都认），问过 `family_said_among` 的放过，函数实参不算。
    #[test]
    fn the_slot_detector_sees_a_synthetic_site() {
        let bad = r#"copy_text("k", &[("agent", agent)]); copy_text("k", &[("a", a), ("agent", &o.agent.to_string())]);"#;
        let folded = "copy_text(\n    \"k\",\n    &[(\"agent\",\n        kind)],\n)";
        let good =
            r#"copy_text("k", &[("agent", &crate::agents::family_said_among(registry, kind))])"#;
        let call = r#"can_of("agent", &life, false)"#;
        assert_eq!(kind_in_slot(bad), 2);
        assert_eq!(kind_in_slot(folded), 1);
        // rustfmt 把格名也折下去的那一形（`&[(\n "agent",\n 值,\n)]`）：坏的认得，问过的放过。
        let head_folded =
            "copy_text(\n    \"k\",\n    &[(\n        \"agent\",\n        kind,\n    )],\n)";
        let head_folded_good =
            "&[(\n    \"agent\",\n    &crate::agents::family_said_among(reg, kind),\n)]";
        assert_eq!(kind_in_slot(head_folded), 1);
        assert_eq!(kind_in_slot(head_folded_good), 0);
        // 只有成对的 `("agent", 值)` 才算那一格：键后面不是逗号的（`s("agent")` · `"agent" =>`）不算。
        assert_eq!(
            kind_in_slot(r#"s("agent").into(); match k { "agent" => 1 }"#),
            0
        );
        assert_eq!(kind_in_slot(good), 0);
        assert_eq!(kind_in_slot(call), 0);
        // `{owner}` 那一格同样数：kind 进去认得，问过的放过。
        assert_eq!(
            kind_in_slot(r#"copy_text("k", &[("name", n), ("owner", owner)])"#),
            1
        );
        assert_eq!(
            kind_in_slot(r#"&[("owner", &crate::agents::family_said_among(reg, owner))]"#),
            0
        );
        // `{who}` 那一格同样数：kind 进去认得，问过说话一方短名的放过。
        assert_eq!(kind_in_slot(r#"copy_text("k", &[("who", agent)])"#), 1);
        // 家名那两格不认说话一方的短名（短名不是家名）。
        assert_eq!(
            kind_in_slot(r#"&[("agent", &crate::agents::speaker_said_among(reg, agent))]"#),
            1
        );
        assert_eq!(
            kind_in_slot(r#"&[("who", &crate::agents::speaker_said_among(reg, agent))]"#),
            0
        );
    }

    // ───────── 第三形：「默认那一家不写」另判一次 ─────────

    /// 交「默认那一家」的函数：调它得到的就是默认那一家（或装着它的 `Family`）。
    /// `account_kind_among` 只有第二格写 `None` 时交的是默认那一家（[`ACCOUNT_DEFAULT`]），另一格是「这个号是哪一家」。
    const DEFAULT_SOURCES: &[&str] =
        &["default_launch_among", "asset_default_among", "main_family"];
    const ACCOUNT_DEFAULT: &str = "account_kind_among";

    /// 住处：`agents/asset_face.rs` 的 `fn unless_default`（只扣这一个函数，同一文件里别处照数）。
    const DEFAULT_HOME: (&str, &str) = ("agents/asset_face.rs", "unless_default");

    /// 「默认那一家不写」在住处之外另判的地方：`(相对 src/backend 的文件, 处数)`。**只许减**。
    const DEFAULT_TWINS: &[(&str, usize)] = &[];

    fn ident_byte(c: u8) -> bool {
        c.is_ascii_alphanumeric() || c == b'_'
    }

    /// `b[open]` 是开括号 ⇒ 与它配对的闭括号的位置（配不上 ⇒ 文本尾）。
    fn matching(b: &[u8], open: usize) -> usize {
        let mut depth = 0usize;
        for (j, &c) in b.iter().enumerate().skip(open) {
            match c {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return j;
                    }
                }
                _ => {}
            }
        }
        b.len()
    }

    /// `s` 里 `word` 作为整词出现的每一处（起点）。
    fn word_at(s: &str, word: &str) -> Vec<usize> {
        let b = s.as_bytes();
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(i) = s[from..].find(word) {
            let at = from + i;
            let end = at + word.len();
            from = end;
            if (at == 0 || !ident_byte(b[at - 1])) && (end >= b.len() || !ident_byte(b[end])) {
                out.push(at);
            }
        }
        out
    }

    /// `s[open]` 是 `(` ⇒ 括号里按顶层逗号分开的各格（去掉首尾空白）。
    fn args_at(s: &str, open: usize) -> Vec<&str> {
        let b = s.as_bytes();
        let close = matching(b, open).min(s.len());
        let mut out = Vec::new();
        let mut depth = 0usize;
        let mut from = open + 1;
        for j in open + 1..close {
            match b[j] {
                b'(' | b'[' | b'{' | b'<' => depth += 1,
                b')' | b']' | b'}' | b'>' => depth = depth.saturating_sub(1),
                b',' if depth == 0 => {
                    out.push(s[from..j].trim());
                    from = j + 1;
                }
                _ => {}
            }
        }
        let last = s[from..close].trim();
        if !last.is_empty() {
            out.push(last);
        }
        out
    }

    /// 这一截代码交的是不是默认那一家：调了 [`DEFAULT_SOURCES`] 之一 · `account_kind_among(…, None)`，或用了已认出的默认那一家的名字。
    fn is_default(expr: &str, names: &[String]) -> bool {
        let call = |f: &str| {
            word_at(expr, f)
                .into_iter()
                .filter_map(|at| {
                    let rest = &expr[at + f.len()..];
                    let open = at + f.len() + (rest.len() - rest.trim_start().len());
                    (expr.as_bytes().get(open) == Some(&b'(')).then_some(open)
                })
                .collect::<Vec<_>>()
        };
        DEFAULT_SOURCES.iter().any(|f| !call(f).is_empty())
            || call(ACCOUNT_DEFAULT)
                .into_iter()
                .any(|open| args_at(expr, open).last() == Some(&"None"))
            || names.iter().any(|n| !word_at(expr, n).is_empty())
    }

    /// 一个函数：名字 · 形参（按序，`self` 不算）· 函数体那一截。
    struct Func<'a> {
        name: &'a str,
        params: Vec<&'a str>,
        body: &'a str,
    }

    /// 按 `fn <名字>` 切成一个个函数（嵌在函数里的函数另算一个；闭包归它所在的函数）。
    fn funcs(code: &str) -> Vec<Func<'_>> {
        let b = code.as_bytes();
        let starts = word_at(code, "fn");
        let mut out = Vec::new();
        for (k, &at) in starts.iter().enumerate() {
            let end = starts.get(k + 1).copied().unwrap_or(code.len());
            let head = &code[at + 2..end];
            let name_len = head
                .trim_start()
                .bytes()
                .take_while(|c| ident_byte(*c))
                .count();
            let lead = head.len() - head.trim_start().len();
            let name = &head[lead..lead + name_len];
            if name.is_empty() {
                continue;
            }
            let Some(open) = code[at..end].find('(').map(|i| at + i) else {
                continue;
            };
            let params = args_at(code, open)
                .into_iter()
                .filter_map(|p| {
                    let p = p.split(':').next()?.trim();
                    let p = p.strip_prefix("mut ").unwrap_or(p).trim();
                    (!p.is_empty() && !p.ends_with("self")).then_some(p)
                })
                .collect();
            let body_from = matching(b, open).min(end);
            out.push(Func {
                name,
                params,
                body: &code[body_from..end],
            });
        }
        out
    }

    /// 函数体里装着默认那一家的名字：`seed`（形参里接的）＋ `let <样式> = <交默认那一家的>` 绑上的（`if let` · `while let` 也算）。
    fn default_names(body: &str, seed: &[String]) -> Vec<String> {
        let b = body.as_bytes();
        let mut names = seed.to_vec();
        for at in word_at(body, "let") {
            let after = at + 3;
            // 样式到顶层那个 `=`（不是 `==` · `=>`）为止。
            let mut depth = 0usize;
            let mut eq = None;
            for j in after..b.len() {
                match b[j] {
                    b'(' | b'[' | b'{' | b'<' => depth += 1,
                    b')' | b']' | b'}' | b'>' => depth = depth.saturating_sub(1),
                    b';' => break,
                    b'=' if depth == 0
                        && b.get(j + 1) != Some(&b'=')
                        && b.get(j + 1) != Some(&b'>')
                        && !matches!(b[j - 1], b'=' | b'!' | b'<' | b'>') =>
                    {
                        eq = Some(j);
                        break;
                    }
                    _ => {}
                }
            }
            let Some(eq) = eq else { continue };
            let in_cond =
                body[..at].trim_end().ends_with("if") || body[..at].trim_end().ends_with("while");
            let mut depth = 0usize;
            let mut end = b.len();
            for j in eq + 1..b.len() {
                match b[j] {
                    b'{' if in_cond && depth == 0 => {
                        end = j;
                        break;
                    }
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => {
                        if depth == 0 {
                            end = j;
                            break;
                        }
                        depth -= 1;
                    }
                    b';' if depth == 0 => {
                        end = j;
                        break;
                    }
                    _ => {}
                }
            }
            if !is_default(&body[eq + 1..end], &names) {
                continue;
            }
            let pat = body[after..eq].split(':').next().unwrap_or("");
            for w in pat
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .filter(|w| !w.is_empty())
                .filter(|w| !matches!(*w, "Some" | "Ok" | "Err" | "None" | "mut" | "ref" | "_"))
                .filter(|w| !w.starts_with(|c: char| c.is_ascii_uppercase() || c.is_ascii_digit()))
            {
                names.push(w.to_string());
            }
        }
        names
    }

    /// `b[i..i + 2]` 是 `==` 或 `!=` ⇒ 它两边的那两截（左：往回到顶层的分隔为止；右：往前同样）。
    fn operands(s: &str, i: usize) -> (&str, &str) {
        let b = s.as_bytes();
        let mut depth = 0usize;
        let mut from = 0;
        for j in (0..i).rev() {
            match b[j] {
                b')' | b']' | b'}' => depth += 1,
                b'(' | b'[' | b'{' if depth == 0 => {
                    from = j + 1;
                    break;
                }
                b'(' | b'[' | b'{' => depth -= 1,
                b',' | b';' | b'|' if depth == 0 => {
                    from = j + 1;
                    break;
                }
                b'&' if depth == 0 && j > 0 && b[j - 1] == b'&' => {
                    from = j + 1;
                    break;
                }
                b'=' if depth == 0
                    && !matches!(b.get(j + 1), Some(b'='))
                    && !matches!(b[j.saturating_sub(1)], b'=' | b'!' | b'<' | b'>') =>
                {
                    from = j + 1;
                    break;
                }
                _ => {}
            }
        }
        let mut depth = 0usize;
        let mut to = b.len();
        for j in i + 2..b.len() {
            match b[j] {
                b'(' | b'[' => depth += 1,
                b'{' if depth == 0 => {
                    to = j;
                    break;
                }
                b'{' => depth += 1,
                b')' | b']' | b'}' if depth == 0 => {
                    to = j;
                    break;
                }
                b')' | b']' | b'}' => depth -= 1,
                b',' | b';' | b'|' if depth == 0 => {
                    to = j;
                    break;
                }
                b'&' if depth == 0 && b.get(j + 1) == Some(&b'&') => {
                    to = j;
                    break;
                }
                _ => {}
            }
        }
        (&s[from..i], &s[i + 2..to])
    }

    /// 比较的结果是不是拿来定「留下它还是成缺」：
    /// - 比较所在的括号（一层层往外，到所在的块为止）后面接 `.then_some(` · `.then(`，或那一对括号是 `.filter(` 的；
    /// - 比较是 `if` 的条件，那两支里有一支就是 `None`。
    fn decides_kept(s: &str, i: usize) -> bool {
        let b = s.as_bytes();
        let mut depth = 0usize;
        let mut j = i;
        while j > 0 {
            j -= 1;
            match b[j] {
                b')' | b']' | b'}' => depth += 1,
                b'(' | b'[' | b'{' if depth > 0 => depth -= 1,
                b'(' => {
                    let close = matching(b, j);
                    let after = s.get(close + 1..).unwrap_or("").trim_start();
                    if after.starts_with(".then_some(") || after.starts_with(".then(") {
                        return true;
                    }
                    let before = s[..j].trim_end();
                    if before.ends_with(".filter") {
                        return true;
                    }
                    // 别的调用的实参：比的结果交给那个调用，不再往外看。
                    if before.bytes().next_back().is_some_and(ident_byte) {
                        break;
                    }
                }
                b'[' => {}
                b'{' => {
                    // 闭包体 `|k| { … }` 接着往外看，别的块到此为止。
                    if !s[..j].trim_end().ends_with('|') {
                        break;
                    }
                }
                b';' if depth == 0 => break,
                _ => {}
            }
        }
        // `if <条件> { … } else { … }`：条件里的比较，往回在顶层碰到 `if`。
        let mut depth = 0usize;
        let mut j = i;
        let mut is_if = false;
        while j > 0 {
            j -= 1;
            match b[j] {
                b')' | b']' | b'}' if depth == 0 && b[j] == b'}' => break,
                b')' | b']' | b'}' => depth += 1,
                b'(' | b'[' | b'{' if depth == 0 => break,
                b'(' | b'[' | b'{' => depth -= 1,
                b';' if depth == 0 => break,
                b'f' if depth == 0
                    && j > 0
                    && b[j - 1] == b'i'
                    && (j < 2 || !ident_byte(b[j - 2]))
                    && !ident_byte(*b.get(j + 1).unwrap_or(&b' ')) =>
                {
                    is_if = true;
                    break;
                }
                _ => {}
            }
        }
        if !is_if {
            return false;
        }
        let mut depth = 0usize;
        let mut open = None;
        for (k, &c) in b.iter().enumerate().skip(i + 2) {
            match c {
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth = depth.saturating_sub(1),
                b'{' if depth == 0 => {
                    open = Some(k);
                    break;
                }
                _ => {}
            }
        }
        let Some(open) = open else { return false };
        let close = matching(b, open).min(s.len().saturating_sub(1));
        let then = s[open + 1..close].trim();
        let rest = s.get(close + 1..).unwrap_or("").trim_start();
        let other = rest
            .strip_prefix("else")
            .map(str::trim_start)
            .and_then(|r| {
                let at = s.len() - r.len();
                (b.get(at) == Some(&b'{')).then(|| s[at + 1..matching(b, at).min(s.len())].trim())
            });
        let none = |t: &str| matches!(t, "None" | "return None" | "return None;");
        none(then) || other.is_some_and(none)
    }

    /// 一份代码（已剥成只剩生产代码 · 抹掉字面量）里「默认那一家不写」另判的处数：
    /// 拿默认那一家做比较（`==` · `!=`，任一边是它，[`is_default`]）、比的结果拿来定留下还是成缺（[`decides_kept`]）。
    /// 默认那一家从形参进来的，同一文件里调它的那一处交的是默认那一家才认（同一文件内一跳）。`skip` ＝ 不数的那个函数（住处）。
    fn default_twins(code: &str, skip: Option<&str>) -> usize {
        let fs = funcs(code);
        let mut seeds: Vec<Vec<String>> = vec![Vec::new(); fs.len()];
        for _ in 0..2 {
            let names: Vec<Vec<String>> = fs
                .iter()
                .zip(&seeds)
                .map(|(f, s)| default_names(f.body, s))
                .collect();
            for (f, ns) in fs.iter().zip(&names) {
                for (t, callee) in fs.iter().enumerate() {
                    for at in word_at(f.body, callee.name) {
                        let open = at + callee.name.len();
                        if f.body.as_bytes().get(open) != Some(&b'(')
                            || f.body[..at].trim_end().ends_with('.')
                        {
                            continue;
                        }
                        for (k, a) in args_at(f.body, open).into_iter().enumerate() {
                            if let Some(p) = callee.params.get(k) {
                                if is_default(a, ns) && !seeds[t].iter().any(|s| s == p) {
                                    seeds[t].push((*p).to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut n = 0;
        for (f, s) in fs.iter().zip(&seeds) {
            if Some(f.name) == skip {
                continue;
            }
            let names = default_names(f.body, s);
            let b = f.body.as_bytes();
            for i in 0..b.len().saturating_sub(1) {
                let op = (b[i] == b'=' || b[i] == b'!') && b[i + 1] == b'=';
                let clean = op
                    && b.get(i + 2) != Some(&b'=')
                    && (i == 0 || !matches!(b[i - 1], b'=' | b'<' | b'>' | b'!'));
                if !clean {
                    continue;
                }
                let (l, r) = operands(f.body, i);
                if (is_default(l, &names) || is_default(r, &names)) && decides_kept(f.body, i) {
                    n += 1;
                }
            }
        }
        n
    }

    /// 一份源文件 ⇒ 只剩生产代码、字面量抹掉的那一份（[`default_twins`] 吃它）。
    fn code_of(src: &str) -> String {
        guard_core::code_only(&guard_core::production_code(src))
    }

    /// 「默认那一家不写」只判在 `agents::unless_default`：住处之外另判的处数与 [`DEFAULT_TWINS`] 逐条对齐（只许减）。
    #[test]
    fn the_default_family_unwritten_is_judged_in_one_place() {
        let root = guard_core::addr::backend_src_root();
        let mut got: Vec<(String, usize)> = guard_core::scan_tree_excluding(&root, &["rs"], &[])
            .into_iter()
            .filter_map(|(p, src)| {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                let skip = (rel == DEFAULT_HOME.0).then_some(DEFAULT_HOME.1);
                let n = default_twins(&code_of(&src), skip);
                (n > 0).then_some((rel, n))
            })
            .collect();
        got.sort();
        assert_eq!(
            got,
            table(DEFAULT_TWINS),
            "\n另判了一次「默认那一家不写」（拿默认那一家比、比的结果定留下还是成缺）：改问 `agents::unless_default`；\
             少了 ⇒ 把 `DEFAULT_TWINS` 改短。"
        );
    }

    /// 第三形认的是语义：拿默认那一家比、结果定留下还是成缺，写成哪一形都认；比的不是默认那一家 · 比了不定留不留的放过。
    #[test]
    fn the_default_detector_sees_every_spelling() {
        let one = |body: &str| {
            default_twins(
                &format!("fn f(reg: &[Adapter], a: A, k: &str) {{ {body} }}"),
                None,
            )
        };
        // 那一形，括号里带调用的也认。
        assert_eq!(one("let main = crate::agents::account_kind_among(reg, None); (Some(a.kind) != main).then_some(a.kind)"), 1);
        assert_eq!(
            one("(Some(a.kind()) != crate::agents::asset_default_among(reg)).then_some(a.kind())"),
            1
        );
        // 两边换了位置 · `.then(|| …)`。
        assert_eq!(one("let main = default_launch_among(reg).map(|(k, _)| k); (main != Some(k)).then(|| k)"), 1);
        // `if … { None } else { Some(…) }`（两支换过来也认）。
        assert_eq!(
            one("let d = asset_default_among(reg); if Some(k) == d { None } else { Some(k) }"),
            1
        );
        assert_eq!(
            one("let d = asset_default_among(reg); if Some(k) != d { Some(k) } else { None }"),
            1
        );
        // `.filter(|k| …)`（闭包体带大括号也认）。
        assert_eq!(
            one("let d = asset_default_among(reg); Some(k).filter(|k| Some(*k) != d)"),
            1
        );
        assert_eq!(
            one("let d = asset_default_among(reg); Some(k).filter(|k| { d != Some(*k) })"),
            1
        );
        // 跟别的条件一起：`(… && Some(k) != d).then_some(k)`。
        assert_eq!(
            one("let d = asset_default_among(reg); (!k.is_empty() && Some(k) != d).then_some(k)"),
            1
        );
        // `if let Some(d) = …` 绑上的。
        assert_eq!(
            one(
                "if let Some(d) = asset_default_among(reg) { (k != d).then_some(k) } else { None }"
            ),
            1
        );
        // 默认那一家从形参进来、同一文件里调它的那一处交的是默认那一家。
        let via = "fn caller(reg: &[Adapter], k: &str) -> Option<&str> { let d = asset_default_among(reg); helper(k, d) }\n\
                   fn helper<'a>(k: &'a str, main: Option<&str>) -> Option<&'a str> { (Some(k) != main).then_some(k) }";
        assert_eq!(default_twins(via, None), 1);
        // 比的不是默认那一家：模型默认（同名同形也放过）· `account_kind_among` 第二格不是 `None`（那是「这个号是哪一家」）。
        assert_eq!(one("let default = launch_face_among(R, &a).and_then(|f| f.model_default).map(|c| c.value); (Some(m.as_str()) != default).then_some(m)"), 0);
        assert_eq!(one("let default = launch_face_among(R, &a).and_then(|f| f.model_default).map(|c| c.value); (Some(m) != default).then_some(m)"), 0);
        assert_eq!(
            one("let w = account_kind_among(reg, a.agent.as_deref()); (Some(k) != w).then_some(k)"),
            0
        );
        // 比了默认那一家、结果不定留不留：「是不是账号库那一家」。
        assert_eq!(
            one("let own = k == crate::agents::account_kind_among(reg, None);"),
            0
        );
        assert_eq!(
            one("let top = main_family(REGISTRY).is_some_and(|f| f.kind == k); top"),
            0
        );
        assert_eq!(
            one("let d = asset_default_among(reg); if Some(k) == d { 1 } else { 2 }"),
            0
        );
        // 比较只是别的调用的实参：外面那个调用的结果再 `.then` 不算比较定了留不留。
        assert_eq!(
            one("let d = asset_default_among(reg); foo(bar(k == d)).then(|| 1)"),
            0
        );
        // 提前返回那一形：`if … { return None; }`。
        assert_eq!(
            one("let d = asset_default_among(reg); if Some(k) == d { return None; } Some(k)"),
            1
        );
        // 绑定只在它所在的那个函数里算。
        assert_eq!(
            default_twins("fn a(reg: R) { let d = asset_default_among(reg); }\nfn b(k: &str, d: Option<&str>) -> Option<&str> { (Some(k) != d).then_some(k) }", None),
            0
        );
    }

    /// 住处认得出（扣的确实是那一形，判据不在空转）：住处那个函数接到默认那一家时会被数到，扣掉它之后住处那份文件是 0。
    #[test]
    fn the_default_home_is_the_one_exempted() {
        let root = guard_core::addr::backend_src_root();
        let src = std::fs::read_to_string(root.join(DEFAULT_HOME.0)).expect("读住处");
        let code = code_of(&src);
        assert!(
            word_at(&code, "fn")
                .iter()
                .any(|at| code[at + 2..].trim_start().starts_with(DEFAULT_HOME.1)),
            "住处 `{}` 里找不到 `fn {}` —— 住处挪了，`DEFAULT_HOME` 跟着改",
            DEFAULT_HOME.0,
            DEFAULT_HOME.1
        );
        let fed = format!(
            "{code}\nfn __caller(reg: &[Adapter], k: &str) {{ let d = asset_default_among(reg); {}(k, d); }}",
            DEFAULT_HOME.1
        );
        assert!(
            default_twins(&fed, None) >= 1,
            "住处那一形认不出：判据认的形与住处不同"
        );
        assert_eq!(
            default_twins(&fed, Some(DEFAULT_HOME.1)),
            0,
            "住处那份文件里另有一处"
        );
        for f in DEFAULT_SOURCES.iter().chain([&ACCOUNT_DEFAULT]) {
            let found = guard_core::scan_tree_excluding(&root, &["rs"], &[])
                .iter()
                .any(|(_, src)| guard_core::contains_word(&code_of(src), &format!("fn {f}")));
            assert!(
                found,
                "交默认那一家的 `{f}` 不在后端源码里了 —— `DEFAULT_SOURCES` 跟着改"
            );
        }
    }

    /// 抽取认得那一形、放过问 `family_said_among` 的写法；住处本身确实在扣除里（否则判据在空转）。
    #[test]
    fn the_detector_sees_a_synthetic_site() {
        let bad = "let n = launch_face_among(REGISTRY, kind).map_or(kind, |l| l.display_name);";
        let good = "let n = crate::agents::family_said_among(REGISTRY, kind);";
        assert_eq!(self_spelled_lines(bad), 1);
        assert_eq!(self_spelled_lines(good), 0);
        assert_eq!(
            speaker_spelled_lines(
                "launch_face_among(REGISTRY, agent).map_or(agent, |l| l.speaker_name)"
            ),
            1
        );
        assert_eq!(
            speaker_spelled_lines("speaker_said_among(REGISTRY, agent)"),
            0
        );
        let root = guard_core::addr::backend_src_root();
        let home =
            std::fs::read_to_string(root.join("agents/asset_face.rs")).expect("读 asset_face.rs");
        let home = guard_core::production_code(&home);
        assert!(
            guard_core::contains_word(&home, "fn family_said_among") && self_spelled_lines(&home) > 0,
            "住处 `agents/asset_face.rs` 里找不到 `family_said_among` 读 `display_name` —— 住处挪了，扣除表跟着改"
        );
        let speaker =
            std::fs::read_to_string(root.join(SPEAKER_EXEMPT[0])).expect("读 launch_face.rs");
        let speaker = guard_core::production_code(&speaker);
        assert!(
            guard_core::contains_word(&speaker, "fn speaker_said_among")
                && speaker_spelled_lines(&speaker) > 0,
            "住处 `agents/launch_face.rs` 里找不到 `speaker_said_among` 读 `speaker_name` —— 住处挪了，扣除表跟着改"
        );
    }
}
