//! 后端零定时器（判活由内核事件驱动：pidfile inotify ＋ pidfd · socket 目录 inotify · tmux hook → SIGUSR1）里**编译器看不见的那两格**。
//!
//! 「会让线程 / 任务自己醒来的构件」（sleep · 带期限的等待 · interval · 掐表 · 造 `Duration`）由 clippy 挡：
//! `src/backend/clippy.toml` 的 `disallowed-methods`，后端与拨号核心两个包的 `[lints.clippy]` 都定成 deny；
//! 每用一处在那一处 `#[allow(clippy::disallowed_methods, reason = "…")]` 写清它为什么不是定时器。
//! 从前这里那几张逐处登记的表（期限 · 有界等待 · 掐表 · 一次性子命令的等待 · 常驻队列线程）换成了那些就地放行。
//!
//! clippy 看不见、留在这里的：
//! - [`f09_external_beat`]：产出一段自带循环的 shell 串交给别人跑，节拍由外部进程给 —— 那是字符串内容，不是调用。
//! - [`windows_arm`]：只在 Windows 上编的那一臂（门禁里 Windows 只做 `cargo check`，clippy 只在 Linux 上跑）。
//!
//! 买不到：特性 `shots` 下才编的那一段（截图台架的拨钟）clippy 默认不看；依赖 crate 里的线程（如某个依赖带超时的内部等待）。
//! 什么时候删：Windows 那一臂进了 clippy 的射程 ⇒ 删 [`windows_arm`]；后端不再产出交给 shell 的串 ⇒ 删 [`f09_external_beat`]。

#[cfg(test)]
mod f09_external_beat {
    //! F09（定框 `C12` 的那条 ⚠ 点名的活）：**「周期跑一次外部命令」也算自己醒过来。**
    //!
    //! `C12` 的 ⚠ 逐字写着：「护栏今天只钉『构件的源码形态』，**没钉住『周期跑一次外部命令』**
    //! —— 那是 F09 的活。」那个形态是：不用 `sleep`/`interval`，而是**产出一段带循环的
    //! shell 串**交给别人执行，靠外部进程提供节拍 —— 后端侧的护栏一个字都看不见。
    //!
    //! 针（逐字）：`sh -c` · `run-shell` · `format!`
    //!
    //! # 今天它是零命中守卫：后端产出的 shell 串里**一处自带节拍的都没有**
    //!
    //! 它曾逮到过一处 —— 容器路收尾那段「等 agent 的信任框、替用户按 Enter」的六轮 `sleep 0.5`
    //! （当时登记为例外）。那一处已删：信任由用户在会话里自己答，替它按中的是缺省那一项「不信任、退出」。
    //! 零命中不靠真树上碰巧有病灶来证明针没瞎：正控是 [`the_matching_unit_is_an_expression_not_a_line`]
    //! 的合成夹具（续行里的循环串收得进、也认得出是节拍）与 [`the_shell_string_scan_finds_candidates`] 的地板。
    //!
    //! # 为什么不放宽成「任何字符串字面量」：两把尺子现打（09-13，量于本 crate `src/` 生产段）
    //!
    //! | 改法 | 人群 | 判红 | 其中假红 |
    //! |---|---|---|---|
    //! | 上一版（按行收人） | 176 行 | 0 | — |
    //! | 放宽成「任何字符串字面量」 | 1704 个串 | 5 | **4**（`watch failed for …` ×2 · `watch_pid_until_exit` ×1 · 一处剥法漏出来的测试尾巴 ×1）|
    //! | 改后（针不动，匹配单位改成表达式） | 224 条表达式 / 210 个串 | **1** | **0** |
    //!
    //! ⚠ 上表三行是**同一趟**用一份 Python 复刻的剥法量的；本模块自己那两条地板是 Rust 侧现算的。
    //! 两把尺子的剥法不是同一份实现 ⇒ **个位数的出入是预期的**，如实登记。
    //!
    //! 放宽成「任何字符串字面量」买到的是 1 真 4 假 ⇒ **净变宽，人会绕开它**
    //! （纪律 ⑯：放宽越界闸之前先问它买得到什么）。改匹配单位买到的是 1 真 0 假 —— 选后者。

    /// 循环关键字：shell 里提供节拍的三种写法。
    fn loop_words() -> Vec<String> {
        // 运行时拼，免得命中本模块自己的说明文字。
        [("whi", "le"), ("unti", "l"), ("fo", "r ")]
            .iter()
            .map(|(a, b)| format!("{a}{b}"))
            .collect()
    }

    /// 三根针 —— 「这一段串是产出给别人执行的」的判别式。
    ///
    /// 🔴 它与头注里那一行 `针（逐字）：…` **两向对拍**
    /// （[`the_head_note_lists_exactly_the_needles_in_use`]）：改一边不改另一边当场红。
    const NEEDLES: &[&str] = &["sh -c", "run-shell", "format!"];

    // ══════════════════════════ 匹配单位 ══════════════════════════

    /// `b[i]` 是双引号：跳过整个字符串字面量，返回收尾引号**之后**的位置。
    fn skip_string(b: &[u8], i: usize) -> usize {
        let mut j = i + 1;
        while j < b.len() {
            match b[j] {
                b'\\' => j += 2,
                b'"' => return j + 1,
                _ => j += 1,
            }
        }
        b.len()
    }

    /// `b[i] == b'\''`：**字符字面量**跳过整条，**生命周期**只跳这一个引号。
    ///
    /// 🔴 这一格不是洁癖：本 crate 生产段里现打有 **9 处**把双引号写成字符字面量的地方
    /// （`relay/tee.rs` 的转义器 · `observe/accounts_query.rs` 的 shell 元字符表 …）。
    /// 只认 `"` 的扫描器走到那儿**当场失步**，从此把代码读成串、把串读成代码。
    /// 而 `&'static str` 那一族又不能按字面量跳 ⇒ 两者必须分得开：
    /// 按**首字节算出那个字符占几个字节**，收尾引号正好落在它后面才算字面量。
    fn skip_quote(b: &[u8], i: usize) -> usize {
        if b.get(i + 1) == Some(&b'\\') {
            let mut j = i + 2;
            while j < b.len() && b[j] != b'\'' {
                j += 1;
            }
            return (j + 1).min(b.len());
        }
        let Some(&c) = b.get(i + 1) else {
            return i + 1;
        };
        let w = if c < 0x80 {
            1
        } else if c >> 5 == 0b110 {
            2
        } else if c >> 4 == 0b1110 {
            3
        } else {
            4
        };
        if b.get(i + 1 + w) == Some(&b'\'') {
            i + w + 2
        } else {
            i + 1
        }
    }

    /// 从 `from`（带针那一行的行首）起，这条**表达式**收尾在哪个字节。
    ///
    /// 收尾 = 第一次开过的那一层括号配平回来；整行一层都没开过就走到行尾。
    /// 🔴 这就是 `K-R103` 换掉的那一样东西 —— 上一版的匹配单位是**行**。
    fn expr_end(b: &[u8], from: usize) -> usize {
        let mut i = from;
        let mut depth = 0i32;
        let mut opened = false;
        while i < b.len() {
            match b[i] {
                b'"' => {
                    i = skip_string(b, i);
                    continue;
                }
                b'\'' => {
                    i = skip_quote(b, i);
                    continue;
                }
                b'(' | b'[' | b'{' => {
                    depth += 1;
                    opened = true;
                }
                b')' | b']' | b'}' => {
                    depth -= 1;
                    if opened && depth <= 0 {
                        return i + 1;
                    }
                }
                b'\n' if !opened => return i,
                _ => {}
            }
            i += 1;
        }
        b.len()
    }

    /// 一段文本里每个字符串字面量**内容**的 `(起, 止)` 字节区间（不含两端引号）。
    ///
    /// ⚠ **原始字符串 fail-closed**：`r"…"` / `r#"…"#` 的定界规则与这里不同，
    /// 本 crate 生产段今天**零处**（现打 09-13）⇒ 不实现它，但撞见就 panic，
    /// 不静默地按普通串读下去。
    fn literal_spans(seg: &str) -> Vec<(usize, usize)> {
        let b = seg.as_bytes();
        let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
        let mut out = Vec::new();
        let mut i = 0usize;
        while i < b.len() {
            match b[i] {
                b'r' if i == 0 || !ident(b[i - 1]) => {
                    let mut j = i + 1;
                    while j < b.len() && b[j] == b'#' {
                        j += 1;
                    }
                    assert!(
                        b.get(j) != Some(&b'"'),
                        "生产段里出现了原始字符串 —— 本扫描器的定界规则认不了它\
                         （`K-R103` 09-13 现打是零处）。回来把定界补上，\
                         别让它按普通串读进去。实得：{:?}",
                        seg.get(i..(i + 40).min(seg.len())).unwrap_or("")
                    );
                    i += 1;
                }
                b'\'' => i = skip_quote(b, i),
                b'"' => {
                    let end = skip_string(b, i);
                    out.push((i + 1, end.saturating_sub(1)));
                    i = end;
                }
                _ => i += 1,
            }
        }
        out
    }

    /// 一段生产文本里的人群 —— [`shell_string_literals`] 的**纯函数那一半**。
    ///
    /// 抬出来是为了让 [`the_matching_unit_is_an_expression_not_a_line`] 能拿合成夹具
    /// 量**同一把尺子**：副本一分叉，红灯就开始骗人。
    fn shell_strings_in(prod: &str) -> Vec<(usize, usize)> {
        let b = prod.as_bytes();
        let mut spans: Vec<(usize, usize)> = Vec::new();
        let mut line_start = 0usize;
        for line in prod.split('\n') {
            if NEEDLES.iter().any(|n| line.contains(n)) {
                let end = expr_end(b, line_start);
                for (a, z) in literal_spans(&prod[line_start..end]) {
                    spans.push((line_start + a, line_start + z));
                }
            }
            line_start += line.len() + 1;
        }
        spans.sort_unstable();
        spans.dedup();
        spans
    }

    /// 那一处所在的整行（在**生产文本**里，逐字 —— 它同时是住址与校验位）。
    ///
    /// ⚠ 刻意不报行号：生产文本是剥过测试段的，行号与文件对不上
    /// （`brief` 13c：指进本树的行号要么带校验位、要么不写）。
    fn line_at(prod: &str, at: usize) -> String {
        let s = prod[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let e = prod[at..].find('\n').map(|i| at + i).unwrap_or(prod.len());
        prod[s..e].trim().to_string()
    }

    /// 后端生产段里**产出给别人执行的 shell 串** —— 逐条 `(文件, 那一行逐字, 串的内容)`。
    fn shell_string_literals() -> Vec<(String, String, String)> {
        let dir = crate::guard_support::src_root();
        let mut out = Vec::new();
        let mut stack = vec![dir.clone()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                    continue;
                }
                let rel = p
                    .strip_prefix(&dir)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                // 本模块自己的说明里逐字写着那三根针 —— 排除掉（同族坑记过四次）。
                if rel == "no_timer_guard.rs" {
                    continue;
                }
                let prod =
                    guard_core::production_code(&std::fs::read_to_string(&p).unwrap_or_default());
                for (a, z) in shell_strings_in(&prod) {
                    out.push((rel.clone(), line_at(&prod, a), prod[a..z].to_string()));
                }
            }
        }
        out
    }

    /// ★ 抽取器自检：抽不到候选串时下面那几条会零命中地绿。
    ///
    /// 地板是**现打值留了余量**（09-13 沙箱实测：**212** 个串；份数那一格由 Python 复刻的剥法
    /// 量到 40，Rust 侧没单独打印 —— 第一条断言先红，第二条根本没跑到，如实登记）。
    /// 数掉下来 ⇒ 剥法或遍历坏了，**不是树变干净了**。
    #[test]
    fn the_shell_string_scan_finds_candidates() {
        let all = shell_string_literals();
        assert!(
            all.len() >= 120,
            "只抽到 {} 个可能的 shell 串 —— 抽取器坏了，下面那几条会空转变绿",
            all.len()
        );
        let mut files: Vec<&str> = all.iter().map(|(f, _, _)| f.as_str()).collect();
        files.sort_unstable();
        files.dedup();
        assert!(
            files.len() >= 25,
            "这些串只来自 {} 份文件 —— 遍历塌了",
            files.len()
        );
    }

    /// 这条 shell 串**自带节拍**吗（含循环关键字）—— 正题与合成夹具量同一把尺子。
    fn is_beat(s: &str) -> bool {
        loop_words().iter().any(|w| s.contains(w.as_str()))
    }

    /// 一段生产文本里自带节拍的那几条 shell 串。
    fn beats_in(prod: &str) -> Vec<String> {
        shell_strings_in(prod)
            .into_iter()
            .map(|(a, z)| prod[a..z].to_string())
            .filter(|s| is_beat(s))
            .collect()
    }

    /// ★ 正题：后端产出的 shell 串里**没有一条自带节拍**（零命中；正控见下一条的合成夹具）。
    #[test]
    fn the_backend_produces_no_external_beat() {
        let beats: Vec<String> = shell_string_literals()
            .into_iter()
            .filter(|(_, _, s)| is_beat(s))
            .map(|(f, line, _)| format!("  {f}: {line}"))
            .collect();
        assert_eq!(
            beats,
            Vec::<String>::new(),
            "后端产出的 shell 串里出现了循环关键字 —— 那是「**周期跑一次外部命令**」形态：\n\
             不用 sleep/interval，而是让别人的 shell 提供节拍，于是本 crate 的零定时器护栏一个字都看不见。\n\
             先回答它的内核事件源是什么、为什么后端接不了。\n{}",
            beats.join("\n")
        );
    }

    /// ★★ 拿合成夹具证明**匹配单位真的是表达式** —— 不靠真树上碰巧有没有病灶。
    ///
    /// 四刀，两正两反（外加一刀：续行里带循环的串认得出是节拍）：
    /// ① 针与串同一行 ⇒ 收得进 · ② 针在上一行、串在**续行** ⇒ **也要收得进**
    /// （这一刀就是 `K-R103` 之前那个洞，上一版在这里是 0）·
    /// ③ 一根针都没有的串 ⇒ 不收 · ④ 表达式收尾之后**下一条**语句里的串 ⇒ 不收（窗口不许越界）。
    #[test]
    fn the_matching_unit_is_an_expression_not_a_line() {
        let n = |src: &str| shell_strings_in(src).len();
        let same_line = format!("let a = {}(\"tmux {} x\");\n", "format!", "run-shell");
        assert_eq!(n(&same_line), 1, "针与串同一行都收不进 —— 抽取器坏了");

        let continued = format!(
            "let a = {}(\n    \" && (do sleep 1; done)\"\n);\n",
            "format!"
        );
        assert_eq!(
            n(&continued),
            1,
            "针在上一行、串在续行 ⇒ 收不进 —— 匹配单位又退回「行」了（`K-R103` 那个洞）"
        );
        let beat = format!(
            "let a = {}(\n    \" && (for _i in 1 2; do sleep 1; done)\"\n);\n",
            "format!"
        );
        assert_eq!(
            beats_in(&beat).len(),
            1,
            "续行里带循环的串没被认成节拍 —— 正题那条会零命中地绿"
        );

        let innocent = "let a = String::from(\"just a plain string\");\n";
        assert_eq!(n(innocent), 0, "一根针都没有的串被收进来了 —— 人群净变宽");

        let after = format!(
            "let a = {}(\n    \"in\"\n);\nlet b = String::from(\"out\");\n",
            "format!"
        );
        assert_eq!(
            n(&after),
            1,
            "窗口越过了表达式收尾，把下一条语句里的串也收进来了"
        );
    }

    /// ★★ 词法自检：**字符字面量与生命周期分得开** —— 分不开就整段失步。
    #[test]
    fn the_lexer_tells_char_literals_from_lifetimes() {
        // `'"'` 那一形：只认 `"` 的扫描器在这里失步，把后面的代码读成字符串。
        let tricky = format!("let q = '\"'; let s = {}(\"a for b\");\n", "format!");
        let spans = shell_strings_in(&tricky);
        assert_eq!(
            spans.len(),
            1,
            "含 `'\"'` 的那一行把扫描器带失步了：{:?}",
            spans
                .iter()
                .map(|(a, z)| &tricky[*a..*z])
                .collect::<Vec<_>>()
        );
        assert_eq!(&tricky[spans[0].0..spans[0].1], "a for b");
        // 生命周期不许被当成字符字面量、把后面那截整段吃掉。
        let lt = format!("let x: &'a str = \"z\"; let s = {}(\"y\");\n", "format!");
        let got: Vec<&str> = shell_strings_in(&lt)
            .iter()
            .map(|(a, z)| &lt[*a..*z])
            .collect();
        assert_eq!(
            got,
            vec!["z", "y"],
            "生命周期 `'a` 被当成了字符字面量 —— 它后面那截被整段吃掉"
        );
    }

    /// ★★★ `KR103D1`：**头注那一行说的针，与 [`NEEDLES`] 逐条相等** —— 两个方向都断。
    ///
    /// 改人群不改头注 ⇒ 红；改头注不改人群 ⇒ 也红。
    /// 「没扫到」与「刻意不扫」之所以在盘上分得开，靠的就是这一格 ＋ 登记表那两个方向：
    /// 头注说得出它扫哪几根针，登记表说得出它**刻意放过**哪一处、依据是哪条定框。
    #[test]
    fn the_head_note_lists_exactly_the_needles_in_use() {
        let src = include_str!("no_timer_guard.rs");
        // 标记运行时拼 —— 免得本条自己这一行被当成头注那一行。
        let mark = format!("//! 针（逐{}）：", "字");
        let hits: Vec<&str> = src
            .lines()
            .filter(|l| l.trim_start().starts_with(mark.as_str()))
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "头注里以 `{mark}` 打头的行有 {} 行（应恰好 1 行）—— \
             少了：读的人没有一句可引的话；多了：两句话可以互相矛盾",
            hits.len()
        );
        let tail = hits[0]
            .split_once('：')
            .expect("上一格已经保证它是那一行")
            .1;
        let mut listed: Vec<String> = tail
            .split('·')
            .map(|s| s.trim().trim_matches('`').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        listed.sort();
        let mut used: Vec<String> = NEEDLES.iter().map(|s| (*s).to_string()).collect();
        used.sort();
        assert_eq!(
            listed, used,
            "头注那一行列的针与 `NEEDLES` 对不上 —— 人群与它自陈的人群分叉了。\n\
             🔴 两个方向都要看：加了一根针而不写进头注 ⇒ 下一个人按头注把它读成「刻意不扫」；\n\
             头注多写一根而人群里没有 ⇒ 那是一句今天就假的话。"
        );
    }
}

#[cfg(test)]
mod windows_arm {
    //! Windows 那一臂里带期限的内核等待只有一处：`platform/win_proc.rs::wait_within`（一次性子命令 `--resident-stop` 等进程退出的宽限）。

    /// Win32 里会带着期限醒过来的几个等待（字面名；生产段里调一次、期限不是无限就算）。
    const WAITS: &[&str] = &[
        "WaitForSingleObject",
        "WaitForMultipleObjects",
        "SleepEx",
        "SetWaitableTimer",
        "CreateTimerQueueTimer",
        "MsgWaitForMultipleObjects",
    ];
    const HOME: &str = "platform/win_proc.rs";

    #[test]
    fn the_only_timed_windows_wait_is_the_one_shot_stop_grace() {
        let root = crate::guard_support::src_root();
        let mut hits = Vec::new();
        let mut files = 0usize;
        for (p, src) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
            files += 1;
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let prod =
                guard_core::strip_comment_lines(&crate::guard_support::production_code(&src));
            for l in prod.lines() {
                // 声明那一行（`fn WaitForSingleObject(…)`）与无期限的等（`INFINITE` / `WAIT_FOREVER`）不算。
                if l.contains("fn ") || l.contains("INFINITE") || l.contains("WAIT_FOREVER") {
                    continue;
                }
                for w in WAITS {
                    if guard_core::contains_word(l, w) && l.contains(&format!("{w}(")) {
                        hits.push(format!("{rel}: {w}"));
                    }
                }
            }
        }
        assert!(files > 200, "只扫到 {files} 份 .rs —— 遍历坏了");
        assert_eq!(
            hits,
            vec![format!("{HOME}: WaitForSingleObject")],
            "Windows 那一臂里带期限的内核等待只许是 `{HOME}` 那一处（一次性子命令的宽限）；常驻后端不许有会自己醒来的等待"
        );
    }
}
