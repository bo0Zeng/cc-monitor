//! P6（zero-poll-liveness）：**零定时器护栏**。
//!
//! # `K-G6` `KG62`：性质与人群，两行逐字（**各自只许有一句**，`readonly_guard::g6_scope_pins` 钉着）
//!
//! - **它守的性质是**：daemon **自己的生产代码**里不出现会让线程 / 任务自己醒来的构件 —— 判活由内核事件驱动（pidfile inotify + pidfd · socket 目录 inotify · tmux hook → SIGUSR1），不靠节拍反复问。
//! - **它扫的人群是**：本 crate `src/` 递归全部 `.rs`（`SKIPPED_BY_NAME` 跳过自身）剥掉测试段之后的**源码文本**；依赖 crate 的源码、以及被起进程的行为，**都不在里面**。
//!
//! ⚠ **这两行今天对得上**（三道护栏里唯一一道），而它买到的东西**比字面小一格** —— 如实登记：
//! 人群按**源码文本**画，性质说的是**这个 crate 的代码**；
//! 而「这个**进程**里有没有东西在自己醒来」是另一件事：今天就有一条依赖 crate 的线程在做带超时的等待，
//! 由生产段亲手拉起，而本护栏一个字都看不见。
//! ⇒ **它今天真能拦住的形状全表在 [`g6_reach`]，那条反例也在那里。**
//!
//! 〔`K-G6` 订正〕本头注原先第一句把性质写成了**进程级**的全称，而判据只兑现 crate 级的那句；
//! 收窄措辞的同时由 `readonly_guard::g6_scope_pins` 立了一条**反向棘轮**，钉住那句全称不许回来。
//!
//! # 它守的是什么性质
//!
//! P0-P5 把三类判活逐个换成了事件源：pidfile inotify + pidfd（进程死）· socket 目录
//! inotify（server 生死）· tmux hook → SIGUSR1（会话开关）。P5 删掉最后一个 8s ticker
//! 之后，**这个 crate 里已经没有任何东西会「自己醒过来」**。
//!
//! 没有护栏的话，这条性质会以最不起眼的方式退化：某天有人为了「稳一点」加一个
//! `thread::sleep(2s)` 的兜底循环，测试全绿、行为看起来更稳，而整轮工作的收益悄悄没了。
//!
//! # 判据落在「周期性唤醒」，不是落在「出现过 `Duration`」
//!
//! 这个区别是本护栏设计上最要紧的一点。`Duration` 有大量**非定时器**的正当用途
//! （超时上限、去抖窗口、时间戳算术），把它们一并禁掉会逼着后来人绕开护栏 —— 那时护栏
//! 就从「防退化」变成了「防不了但很吵」。
//!
//! 故：**禁的是会让线程/任务自己醒来的那些构件**（见 `PERIODIC_WAKE_PATTERNS`），
//! 而**已知的非定时器 `Duration` 用途逐条登记**（见 `REGISTERED_DURATION_USES`）——
//! 登记表不是豁免清单，它是「这些我看过、确认不是定时器」的账，**多一条就要红一次**、
//! 逼人把新的那处也想清楚。
//!
//! # 范围
//!
//! **只钉 daemon crate（`src/backend/*.rs`）的生产段。** monitor 侧另有自己的
//! 轮询纪律（那边有 UI 刷新、重连退避等**正当**周期行为），把本护栏扩过去会立刻变成噪音
//! ⇒ 要钉那半得单独论证。**范围写清楚，别默认扩** —— 守卫范围必须等于它真正证明的性质。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空、零运行期开销、不改 daemon 行为。

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
    //! # 🔴〔`K-R103` 09-13〕本条不再是零命中守卫：**它今天恰好逮住一处，而那一处登记在案**
    //!
    //! 上一版这里有两句话，今天两句都不准了，逐句订正：
    //!
    //! 1. 「那条预信任轮询串住 `shared/ccm`，不在后端」—— **假了**。`K-R48`（09-11）把那个
    //!    bash 脚本删掉、整条搬进了 `control/ccm/`，它今天住 `control/ccm/plan.rs`。
    //! 2. 「后端侧今天零实例」—— **也假了**；它之所以还绿，是因为**人群够不着**：
    //!    上一版按**行**收人（同一行里有引号 **且** 有上面三根针之一），
    //!    而 `plan.rs` 那一条是 `format!(` 的**续行** ⇒ 三根针一根都不落在那一行上。
    //!
    //! ⇒ 病不在「针太窄」，在**匹配单位比事实小**（同族登记在
    //! `platform/cfgless_guard` 的 `hits` 头注里，本仓量到过三次）。
    //! 本轮的处置是**把匹配单位从「行」改成「表达式」**（从带针那一行起、括号配平到收尾），
    //! **不是**把针放宽成「任何字符串字面量」。
    //!
    //! # 为什么不放宽成「任何字符串字面量」：两把尺子现打（09-13，量于本 crate `src/` 生产段）
    //!
    //! | 改法 | 人群 | 判红 | 其中假红 |
    //! |---|---|---|---|
    //! | 上一版（按行收人） | 176 行 | 0 | — |
    //! | 放宽成「任何字符串字面量」 | 1704 个串 | 5 | **4**（`watch failed for …` ×2 · `watch_pid_until_exit` ×1 · 一处剥法漏出来的测试尾巴 ×1）|
    //! | 本轮（针不动，匹配单位改成表达式） | 224 条表达式 / 210 个串 | **1** | **0** |
    //!
    //! ⚠ 上表三行是**同一趟**用一份 Python 复刻的剥法量的（量具住址与逐处读数住
    //! `evidence/K-R103-deathvalue.md`）；本模块自己那两条地板是 Rust 侧现算的。
    //! 两把尺子的剥法不是同一份实现 ⇒ **个位数的出入是预期的**，如实登记。
    //!
    //! 放宽成「任何字符串字面量」买到的是 1 真 4 假 ⇒ **净变宽，人会绕开它**
    //! （纪律 ⑯：放宽越界闸之前先问它买得到什么）。改匹配单位买到的是 1 真 0 假 —— 选后者。
    //!
    //! # 「没扫到」与「刻意不扫」从今天起在盘上分得开
    //!
    //! 逮到的那一处**不是违规**，它是 `C14` 逐字登记的那个例外
    //! （逐字见 [`REGISTERED_EXTERNAL_BEATS`] 的签字栏）。⇒ 那张表**默认拒绝**：
    //! 没登记的当场红、登记了而今天一处都匹配不上的也当场红。
    //! 🔴 它买到的**不是**「后端没有外部节拍」，是「**每一条外部节拍都有人签过字**」。
    //! ⚠ 而「登记的那一条今天还在人群里」是另一格 ——
    //! 上一版就是死在那里 ⇒ 单立一条 [`the_registered_beat_is_actually_inside_the_population`]。

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

    /// **登记在案的外部节拍**：`(文件相对路径, 串里的逐字锚点, 定框依据, 签字)`。
    ///
    /// 🔴 **这不是免检名单，是默认拒绝**：没登记的当场红；登记了而今天一处都匹配不上的
    /// 也当场红（过期条目会让这张表慢慢变成一张没人敢动的名单 ——
    /// 同 `platform/cfgless_guard` 的 `REGISTERED` 那两个方向）。
    const REGISTERED_EXTERNAL_BEATS: &[(&str, &str, &str, &str)] = &[(
        "control/ccm/plan.rs",
        "Yes, I trust this folder",
        "C14",
        "`C14`〔实 08-01·inotify 看不见 pane 内容〕逐字：「**预信任的『等信任框』没有内核事件源** \
         —— 它本质就是轮询。**`C8` 的唯一登记例外**：`control/` 继续**以 shell 字符串形态**产出它\
         （由目标 shell 执行，因此与零定时器共存）。不写下来，实现期必然有人用 Rust 重写然后撞护栏」。\
         ⇒ 这一处**不是漏进来的**，是定框点名让它以这个形态住在 `control/` 的：\
         节拍由**目标 shell** 提供，后端进程自己一个定时器都没有。\
         🔴 要把它收掉得先回定框重裁 `C14`，不是在这里删一行。",
    )];

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

    /// ★ 正题：后端产出的 shell 串里，**每一条自带节拍的都要签过字**。两个方向都断。
    #[test]
    fn every_external_beat_the_backend_produces_is_registered() {
        let words = loop_words();
        let beats: Vec<(String, String, String)> = shell_string_literals()
            .into_iter()
            .filter(|(_, _, s)| words.iter().any(|w| s.contains(w.as_str())))
            .collect();
        let unsigned: Vec<String> = beats
            .iter()
            .filter(|(f, _, s)| {
                !REGISTERED_EXTERNAL_BEATS
                    .iter()
                    .any(|(p, anchor, _, _)| *p == f.as_str() && s.contains(*anchor))
            })
            .map(|(f, line, _)| format!("  {f}: {line}"))
            .collect();
        assert_eq!(
            unsigned,
            Vec::<String>::new(),
            "后端产出的 shell 串里出现了**没签字**的循环关键字 —— 那是 `C12` 的 ⚠ 点名的\n\
             「**周期跑一次外部命令**」形态：不用 sleep/interval，而是让别人的 shell 提供节拍，\n\
             于是本 crate 的零定时器护栏一个字都看不见。\n\
             出路两条：把那条节拍去掉；或者回定框重裁 `C12`/`C14` 之后在\n\
             `REGISTERED_EXTERNAL_BEATS` 上签一行字（写清定框依据）。\n{}",
            unsigned.join("\n")
        );
        let stale: Vec<String> = REGISTERED_EXTERNAL_BEATS
            .iter()
            .filter(|(p, anchor, _, _)| {
                !beats
                    .iter()
                    .any(|(f, _, s)| f.as_str() == *p && s.contains(*anchor))
            })
            .map(|(p, anchor, ..)| format!("  {p} :: {anchor}"))
            .collect();
        assert_eq!(
            stale,
            Vec::<String>::new(),
            "`REGISTERED_EXTERNAL_BEATS` 里这几条今天一处都匹配不上：\n{}\n\
             🔴 两种成因在盘上必须分得开：**那一处真的挪走/去掉了**（把这一行删掉）\n\
             与**人群又够不着它了**（本条此刻在空转）。\n\
             判之前先看 `the_registered_beat_is_actually_inside_the_population` 红没红。",
            stale.join("\n")
        );
    }

    /// ★★ 反空真：**「人群够不着」与「盘上没有」必须分得开。**
    ///
    /// 上一版正是死在这一格 —— 那条预信任串一直在盘上，而人群按行收人够不着它，
    /// 于是判据零命中地绿着，头注还写着「后端侧今天零实例」。
    /// 本条断的是**登记表上那一处真的落在人群里**。
    #[test]
    fn the_registered_beat_is_actually_inside_the_population() {
        assert!(
            !REGISTERED_EXTERNAL_BEATS.is_empty(),
            "登记表空了 —— 本条会变成「对空集全称成立」，恒绿"
        );
        let all = shell_string_literals();
        for (p, anchor, charter, _why) in REGISTERED_EXTERNAL_BEATS {
            let n = all
                .iter()
                .filter(|(f, _, s)| f.as_str() == *p && s.contains(*anchor))
                .count();
            assert!(
                n >= 1,
                "`{p}` 上那条 `{charter}` 登记的外部节拍（锚点 `{anchor}`）**不在人群里** ——\n\
                 那不是它没了，是本条的匹配单位又够不着它了（`K-R103` 治的正是这一形）。"
            );
        }
    }

    /// ★★ 拿合成夹具证明**匹配单位真的是表达式** —— 不靠真树上碰巧有没有病灶。
    ///
    /// 四刀，两正两反：
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

    /// ★〔`K-R87` 09-13〕**看门狗那条 argv 形的 shell 串，也不许自带节拍。**
    ///
    /// # 为什么要单独一条：上面那条**结构上看不见它**（`K-R103` 改完匹配单位之后**仍然看不见**）
    ///
    /// [`shell_string_literals`] 认的是「一条**含针的表达式**里的字符串字面量」
    /// （针 = [`NEEDLES`]）。而 `K-R87` 的看门狗走的是 **argv 直传**：脚本是一个**常量**、
    /// `sh` 与 `-c` 是两个独立的 argv 元素 ⇒ 那三根针**一根都不落在那条常量声明上**。
    ///
    /// 🔴 `K-R103` 把匹配单位从「行」改成「表达式」，关掉的是
    /// **`format!(` 续行**那个盲区（`control/ccm/plan.rs` 那条因此进了人群）；
    /// **argv 形常量**是另一个盲区，那一改**没有**顺带关掉它 —— 两个盲区不是一回事，
    /// 别把「上一条变严了」读成「这一条可以撤了」。
    /// ⚠ 那条 argv 形的另一面（它是 POSIX-only、按 `K33` 该住 `platform/`）由
    /// `platform/cfgless_guard` 的 `posix-shell-sh` 那根针接住 —— `K-R103` 把它一并放宽到
    /// 「整条字面量就是 `sh`」这一形，本模块不重复守那一格。
    /// ⇒ 本条**只盖 `control/oneshot_session.rs` 这一份文件**，
    /// 形状照 `readonly_guard::capture_is_read_only`（那一条同样自陈「只盖一份文件」）。
    #[test]
    fn the_oneshot_watchdog_script_carries_no_loop() {
        let prod = guard_core::production_code(include_str!("control/oneshot_session.rs"));
        // 反空真①：读到的得是真代码，不是一份被剥空的壳。
        assert!(
            prod.len() > 3_000,
            "剥完 `control/oneshot_session.rs` 只剩 {} 字节 —— 读错文件或剥过头了，\
             下面几格在空转",
            prod.len()
        );
        // 反空真②：本条声称在盯的那个常量真的在生产段里。
        let decl = format!("const WATCHDOG{}", "_SCRIPT");
        assert!(
            prod.contains(&decl),
            "生产段里找不到 `{decl}` —— 看门狗那条脚本换了住址或换了写法，回来重判本条"
        );
        // ★ 正题：那条脚本里没有任何一个循环关键字。
        let script = crate::control::oneshot_session::WATCHDOG_SCRIPT;
        for w in loop_words() {
            assert!(
                !script.contains(w.as_str()),
                "看门狗脚本里出现了循环关键字 `{w}` —— 那就从「到点一次」变成了\
                 「靠外部 shell 提供节拍」，也就是 C12 的 ⚠ 点名的那一形。实得：{script:?}"
            );
        }
        // ★ 反向自检：判定真的会咬人（否则上面那圈可能是「什么都认不出」地绿）。
        let synthetic = format!("{}:; do sleep 1; done", loop_words()[0]);
        assert!(
            loop_words().iter().any(|w| synthetic.contains(w.as_str())),
            "合成的带循环样本没被逮到 —— 上面那圈此刻是空转的：{synthetic:?}"
        );
        // ★ 那份文件把东西交给 shell 的口**恰好一处**（常量声明 1 ＋ 用它那一处 1）。
        //   多出来一处 ⇒ 有第二条串在往 shell 里送，而本条只盯着上面那个常量。
        const SHELL_HANDOFFS_TODAY: usize = 2;
        let flag = format!("SHELL_SCRIPT{}", "_FLAG");
        let n = prod.matches(flag.as_str()).count();
        assert_eq!(
            n, SHELL_HANDOFFS_TODAY,
            "`control/oneshot_session.rs` 生产段里 `{flag}` 出现 {n} 次（登记 {SHELL_HANDOFFS_TODAY} \
             次 = 常量声明一处 ＋ 组 argv 那一处）。\n\
             变多 ⇒ 那份文件多了一条交给 shell 的路，而本条只盯着那一个常量 ⇒ 回来重判；\n\
             变少 ⇒ 那条路换了写法，本条此刻盯的是一个没人用的常量。"
        );
    }
}

#[cfg(test)]
mod tests {
    /// 会让线程/任务**自己醒过来**的构件。命中即违规。
    ///
    /// 逐条说明为什么它在表里：
    /// - `thread::sleep` / `tokio::time::sleep`：睡到点自己醒 —— 轮询的标准形态
    /// - `recv_timeout`：等不到就自己醒，等价于给循环装了节拍
    /// - `tokio::time::interval`：字面意义的节拍器
    /// - `Instant::now`：本 crate 里它只会用来做「距上次多久了」的节流判断
    ///   （真要打时间戳有 `SystemTime`，且 daemon 的帧不带时间戳）
    /// - `Duration::from_secs`：秒级 `Duration` 在这个 crate 里只可能是节流常量
    ///   （去抖窗口是毫秒级，超时上限也不该出现在 reader 路径上）
    ///
    /// **不在表里的**：`Duration` 本身、`Duration::from_millis`（见 `REGISTERED_DURATION_USES`）。
    /// 行里有没有 `name(` 这个**调用**（`name` 要是完整的词）。
    ///
    /// 与 monitor 侧 `rust_timer_registry::is_call_of` 同形 —— 两侧各一份是刻意的：
    /// 两个 crate 之间没有共享测试工具的通道（跨 crate 够不着 `guard_core` 的测试模块）。
    pub(super) fn is_call_of(line: &str, name: &str) -> bool {
        let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
        let mut from = 0usize;
        while let Some(i) = line[from..].find(name) {
            let at = from + i;
            from = at + name.len();
            if line[..at].chars().next_back().is_some_and(ident) {
                continue;
            }
            if line[from..].trim_start().starts_with('(') {
                return true;
            }
        }
        false
    }

    /// ★ 调用匹配器的**负向**自检〔audit-0805 08-06〕。
    ///
    /// ⚠ 加它是因为一次实测：把 `is_call_of` 的词边界整段删掉，**六条判据照样绿** ——
    /// 也就是说这个匹配器的收紧那一侧当时**没有任何断言在行使**。
    /// 词边界松掉的后果是误红（`do_sleep(3)` 被当成周期唤醒），
    /// 而误红最省事的消法是把这条判据删掉。
    /// ⇒ 本区第四次踩「负向断言写不出来就等于没有」，这次在当轮就补上并验了。
    #[test]
    fn the_call_matcher_does_not_fire_on_lookalikes() {
        assert!(is_call_of("    sleep(d).await;", "sleep"), "真调用没认出来");
        assert!(
            is_call_of("    tokio::time::sleep(d);", "sleep"),
            "带路径的调用也要认"
        );
        assert!(
            !is_call_of("    let x = do_sleep(3);", "sleep"),
            "`do_sleep(` 被当成了 `sleep(` —— 词边界没守住"
        );
        assert!(
            !is_call_of("    self.my_interval(2);", "interval"),
            "`my_interval(` 被当成了 `interval(` —— 词边界没守住"
        );
        assert!(
            !is_call_of("    let sleep_ms = 5;", "sleep"),
            "只是同名变量、后面不是 `(`，不该算调用"
        );
    }

    pub(super) fn periodic_wake_patterns() -> Vec<String> {
        // 判据**运行时拼**：直接写字面量的话，本文件自己就会被下面的扫描命中
        //（同类自指陷阱在 P4 连踩七次，其中一次让守卫变成了安慰剂）。
        vec![
            format!("thread::{}", "sleep"),
            format!("time::{}", "sleep"),
            format!("recv{}", "_timeout"),
            format!("time::{}", "interval"),
            format!("{}::now", "Instant"),
            format!("Duration::{}", "from_secs"),
        ]
    }

    /// **已登记的非定时器 `Duration` 用途**。
    ///
    /// 这不是豁免清单 —— 下面的断言要求生产段里的 `Duration::from_` 调用**恰好**等于
    /// 本表的条数。多出一处就红，逼人回答「这处是不是又把轮询请回来了」。
    ///
    /// # 〔`K-G6` `KG64`〕三元组扩成**五元组**
    ///
    /// `(文件名, 片段, why——为什么它不是定时器, 格——`§0a` 四情形表里的哪一格, unlock——什么条件满足之后这一条就能删)`
    ///
    /// 第四栏由 `crate::readonly_guard::g6_doctrine::is_cell` 做**枚举比对**（不是子串）。
    /// 三条今天**全部**落在 `收窄人群`：本护栏的性质是「不许有会自己醒来的构件」，
    /// 而按 `Duration::from_*` 画的人群会把**非定时器**的正当用途一并扫进来 ——
    /// 这张表就是把人群收窄回性质上的那一刀，**它是收紧，不是豁免**。
    pub(super) const REGISTERED_DURATION_USES: &[(&str, &str, &str, &str, &str)] = &[
        (
        "watcher.rs",
        "Duration::from_millis(DEBOUNCE_MS)",
        "notify-debouncer 的**事件合并窗口**：它不产生唤醒，只决定「同一批文件事件攒多久\
         再一起交付」。去掉它 inotify 照样推事件，只是更碎。不是定时器。",
        "收窄人群",
        "去抖窗口不再由本 crate 传参（换成事件源自带合并、或整条 watch 路径重做）的那天。\
         ⚠ 这一条的**邻居**是本护栏最硬的那个反例：拉起这个去抖器的依赖 crate 内部有一条\
         带超时的等待线程，形状逐字落在禁用表上而人群够不着 —— 见 `g6_reach`。",
    ),
        (
        "server.rs",
        "Duration::from_millis(30_000)",
        "中转**下游** socket 的 `SO_RCVTIMEO`/`SO_SNDTIMEO`（`relay/server.rs::DOWNSTREAM_DEADLINE`）：\
         它说的是「**这一次**阻塞的读/写最多等多久」——有字节就立刻返回，没字节就**报错**返回。\
         它**不让任何线程自己醒来**、不产生节拍、不驱动任何循环：期限一到那条连接就被结掉，\
         `pump` 与 `http1` 的读循环只对 `Interrupted` 重试、其余一律 `return Err`。\
         没有它，一条半开连接会把一条线程永久钉在 `read_head` 上（D3 §2.3 实测 64 条 ⇒ 线程 4→68）。不是定时器。",
        "收窄人群",
        "中转那半整个搬走、或 socket 期限改由内核/别处设定的那天。\
         ⚠ 这条登记理由里「`pump` 与 `http1` 的读循环只对 `Interrupted` 重试」那句\
         **`K-G6` 摸底没有重打**（登记在件文件 `§0b-7②`），要摘这一条之前先把它核实。",
    ),
        (
        "upstream.rs",
        "Duration::from_millis(600_000)",
        "中转**上游** socket 的 `SO_RCVTIMEO`/`SO_SNDTIMEO`（`relay/upstream.rs::UPSTREAM_DEADLINE`）：\
         性质同上一条（一次阻塞的上限，不是唤醒），值不同是因为这一跳等的是**模型在想** ——\
         SSE 长流上游几十秒不发字节是正常形态，所以它必须比下游那条宽得多。\
         同样不驱动任何循环：到点即结连接，没有任何一层重试。不是定时器。",
        "收窄人群",
        "同上一条：中转那半搬走、或期限改由别处设定的那天一起摘。\
         ⚠ 「没有任何一层重试」这句同样是**登记理由里的话**，没被独立重打过。",
    ),
        (
        "dial/mod.rs",
        "Duration::from_millis(30_000)",
        "`K-P6b` 的拨号代理交给 `russh` 的 **SSH 层 keepalive 间隔**（`client::Config`）。\
         🔴 **这一条的理由与上面三条不同族，别照着上面读**：它**不是**「不是定时器」——\
         `russh` 拿到这个值之后，它自己的任务**确实会周期性醒来**去发 keepalive。\
         登记在这里的准确说法是：**醒来的那个东西不在本护栏的人群里**。\
         人群按「本 crate `src/` 的源码文本」画，而那条节拍长在依赖 crate 的任务里 ——\
         这与 `g6_reach` 那条反例（notify-debouncer 内部那条带超时的等待线程，同样由\
         本 crate 生产段亲手拉起）**是同一族**，头注第三段已经把这一格如实登记过。\
         为什么非要它：长连接的死链**只能**靠 keepalive 超时 + EOF 检出（界面侧\
         `connect_session` 的 FIX 1 逐字同一条理由）；不设它，一条断掉的 SSH 会话会\
         静默挂着不 EOF，界面那头永远等不到重连。\
         ⇒ 这一条**不许被读成「这里没有定时器」**，它说的是「这里有一个，而它长在人群外面」。",
        "收窄人群",
        "`K-P6b` 那条代理路整个撤掉（候选 E 被否）、或 keepalive 改由别处（内核 TCP \
         keepalive / 远端 sshd 的 ClientAlive）设定的那天。\
         ⚠ 另一条更强的解锁：哪天本护栏的人群从「本 crate 源码文本」扩到「本进程」\
         （那正是 `g6_reach` 说的那一格），这一条就该从「登记」改成「正面回答」。",
    )];

    use crate::guard_support::production_code;

    /// 遍历 `src/` 下**全部**（含子目录）`.rs`，返回 `(相对路径, 生产段)`。
    ///
    /// # 为什么必须递归（2026-08-01，U-1）
    ///
    /// 原来是单层 `read_dir` + `extension != "rs"` 跳过 —— **目录没有扩展名，于是被整个跳过**。
    /// `readonly_guard::scan` 在 Phase G 已经改成递归并留了警示注释，**这条没跟**。
    ///
    /// 失效形态实测（unified-backend 计划自审）：一旦生产代码搬进
    /// `src/<子目录>/`，扫到的文件只剩顶层那几个 mod 声明 + 本护栏 + `wire.rs`，
    /// 而当时的地板 `files.len() >= 5` **照样满足** ⇒ 护栏一行业务代码都没扫、全绿。
    /// 那正是本仓在「守卫范围 ≠ 性质范围」上栽过的第四次。
    pub(super) fn daemon_sources() -> Vec<(String, String)> {
        let root = crate::guard_support::src_root();
        let mut out = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                // 跳过本护栏自身：它的模式表与说明文字必然含这些子串。
                let base = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if SKIPPED_BY_NAME.contains(&base) {
                    continue;
                }
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let src = std::fs::read_to_string(&path).expect("read rs file");
                out.push((rel, production_code(&src)));
            }
        }
        out.sort();
        out
    }

    /// 采集时**按文件名主动跳过**的文件。
    ///
    /// 单独成表，是为了让「数量相等」那条判据算得出跳过了几个 —— 原来写死 `files.len() + 1`，
    /// 那个 `1` 与下面 `daemon_sources()` 里的跳过逻辑隔空耦合（Phase E 审计建议）。
    /// U1b 若要再跳过一个（如 `control/` 的窄写护栏自身），只改这张表，判据自动跟上。
    const SKIPPED_BY_NAME: &[&str] = &["no_timer_guard.rs"];

    /// 扫到的**真代码总量**下限。
    ///
    /// # 为什么是字节数而不是文件数
    ///
    /// 「文件数 >= N」挡不住本护栏真正的失效形态：**代码搬进子目录、顶层只剩壳**。
    /// 那时文件数照样够，而扫到的是一堆 `mod x;` 声明。字节数直接度量「扫到的是不是真代码」，
    /// 且**对拆分免疫**（把一个文件拆成三个，总字节不变）—— 而本工作区接下来做的正是拆分。
    ///
    /// # 这里刻意**不记**当前实测值
    ///
    /// 这段注释踩过两次同一个坑：U1a 时写「119_454」被审计算出实际 121_131（**我抄错了**）；
    /// 订正成 121_131 之后，U2 建出 `platform/` + `common/` 当场又过期，
    /// 被 Phase D 审计第二次逮到 —— 而那一版注释自己第一句写的就是「**别手抄这个数**」。
    ///
    /// **教训不是「下次抄仔细点」**：注释里但凡出现一个会随代码变的数字，
    /// 光写「别手抄」挡不住任何人。要么给复测办法，要么根本别记。这里两条都做：
    ///
    /// **复测办法** —— 把下面的常量临时改成一个大数跑 `cargo test no_timer`，
    /// 失败信息里的「只扫到 N 字节」就是当前实测值。**那条断言恒打印实时值、不依赖本注释**，
    /// 所以真值永远在你手上。
    ///
    /// **要下调这个数之前先问：是真的删了那么多生产代码，还是扫描面又缩了？**
    ///
    /// ⚠ **它一条挡不住「单个文件被剥空/剥过头」** —— 最大的那个文件（`watcher.rs`，约占三成）
    /// 整个消失，总量仍会高于本地板、照样绿。所以字节地板必须与下面那条**数量相等**判据
    /// 配着用（Phase D 审计 I1 指出的缺口）。
    const MIN_SCANNED_CODE_BYTES: usize = 80_000;

    /// 登记表的键（裸文件名）与采集到的相对路径是否指同一个文件。
    ///
    /// # 为什么单独抽成函数
    ///
    /// U-1 Phase E 把匹配从「路径全等」放宽成「全等 **或** 以 `/文件名` 结尾」，为的是
    /// U2/U3 把文件搬进子目录时不误红。但 Phase D 审计指出：**那条 `ends_with` 分支
    /// 今天被执行了却恒为 false** —— 登记表只有 `watcher.rs` 一条，而它还在顶层，
    /// 命中永远由左边的全等短路给出。⇒ **「返回 true」那一侧零覆盖**，
    /// U3 搬 `watcher.rs` 进 `observe/` 的那一刻它才第一次真生效。
    ///
    /// 而它一旦写错，表现出来是「登记表里的 watcher.rs 已经不在生产代码里了，请清理登记」
    /// —— 一条**指向完全错误方向**的诊断（表没腐烂，只是文件搬了家），而 §4.1 红线又盯着
    /// 这张表不许乱动。抽出来钉三行，比到时候排查便宜得多。
    ///
    /// ⇒ **U3 实测兑现**：`watcher.rs` 搬进 `observe/` 的那一刻这个分支第一次真生效。
    /// 变异（退回全等）当场产出上面那句预言过的误导性诊断。
    ///
    /// # 为什么这里可以裸文件名，而 `readonly_guard` 的白名单不行
    ///
    /// U3 刚花整段论证「按裸文件名钉写盘白名单 = 给写盘能力开第二个洞」，本函数却正是那种匹配。
    /// **两者方向相反，风险不对称**：
    ///
    /// | | `readonly_guard` 白名单 | 本登记表 |
    /// |---|---|---|
    /// | 匹配命中的后果 | **放行** —— 该文件获得写盘豁免 | **认账** —— 承认「这处 `Duration` 我看过、不是定时器」 |
    /// | 误配一个同名文件 | 那个文件获得它不该有的写盘权限，**静默开洞** | 少红一次；而**另一半判据 `code.contains(snippet)` 仍要求那份代码里真有那段片段** |
    /// | 失败方向 | fail-**open** | fail-closed 偏保守 |
    ///
    /// 换句话说：白名单是「谁可以为所欲为」，登记表是「我确认过这一处」。
    /// 前者误配等于授权，后者误配还要再过一道内容检查。**所以这里不路径化不是双标，
    /// 是判据性质不同** —— 但这条区别必须写下来，否则下一个人只会看到两条相反的做法。
    fn matches_registered(scanned_rel: &str, registered_file: &str) -> bool {
        scanned_rel == registered_file || scanned_rel.ends_with(&format!("/{registered_file}"))
    }

    /// 独立走一遍目录树，只数 `.rs` 个数 —— 用来与 `daemon_sources()` 的产出做**数量相等**核对。
    ///
    /// 刻意与 `daemon_sources` 分开写：那边还要读文件、剥生产段、跳过自身，
    /// 这边只做「树上有几个 `.rs`」这一件事，两者对不上就说明采集环节漏了东西。
    fn count_rs_in_tree() -> usize {
        let root = crate::guard_support::src_root();
        let mut n = 0usize;
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    n += 1;
                }
            }
        }
        n
    }

    /// ★ 生产段不许有任何**周期性唤醒**构件。
    #[test]
    fn daemon_production_code_has_no_periodic_wakeups() {
        let files = daemon_sources();
        // 反向自检：**扫到的必须是真代码**。断言的是「命中数 == 0」+「扫到了东西」，
        // 而不是「命中数 < N」—— 阈值绝不能挂在被优化的那个量上（rust-ts-boundary 的教训）。
        //
        // U-1：这里从「文件数 >= 5」换成**字节数**，理由见 `MIN_SCANNED_CODE_BYTES`。
        let bytes: usize = files.iter().map(|(_, c)| c.len()).sum();
        assert!(
            bytes >= MIN_SCANNED_CODE_BYTES,
            "只扫到 {bytes} 字节生产代码（下限 {MIN_SCANNED_CODE_BYTES}），\
             护栏多半没在扫该扫的东西。扫到的文件：{:?}",
            files.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>()
        );
        // ★ 反向自检之二（Phase D 审计 I1 补）：**数量相等**。
        // 字节地板挡不住「某个文件整体掉出扫描面」（实测 `watcher.rs` 占 29%，它整个消失
        // 总量仍 84_948 ≥ 80_000、照样绿）。这条用一遍独立的目录树计数来兜死那一类：
        // 采集到的 + 主动跳过的（本护栏自身）必须等于树上的 `.rs` 总数，一个都不许漏。
        let tree = count_rs_in_tree();
        assert_eq!(
            files.len() + SKIPPED_BY_NAME.len(),
            tree,
            "扫到 {} 个 .rs + 主动跳过 {} 个（{SKIPPED_BY_NAME:?}）≠ 树上的 {tree} 个 —— 采集漏了文件。扫到的：{:?}",
            files.len(),
            SKIPPED_BY_NAME.len(),
            files.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>()
        );
        // 反向自检之三：`src/` 下**只要存在含 `.rs` 的子目录**，扫到的集合里就必须有带 `/` 的项。
        // 这条专门钉住「有人把遍历改回非递归」——那正是 U-1 修的那个 bug 的形状。
        // **要求子目录里真有 `.rs`**（Phase D 审计 S1）：否则一个空目录 / `snapshots/` /
        // `testdata/` 就会把它打成误红，那种守卫最后会被人删掉。
        let root = crate::guard_support::src_root();
        let has_rs_subdir = std::fs::read_dir(&root)
            .expect("read src dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .any(|e| {
                let mut stack = vec![e.path()];
                while let Some(d) = stack.pop() {
                    let Ok(rd) = std::fs::read_dir(&d) else {
                        continue;
                    };
                    for x in rd.flatten() {
                        let p = x.path();
                        if p.is_dir() {
                            stack.push(p);
                        } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
                            return true;
                        }
                    }
                }
                false
            });
        if has_rs_subdir {
            assert!(
                files.iter().any(|(n, _)| n.contains('/')),
                "src/ 下有含 .rs 的子目录，但扫到的全是顶层文件 —— 遍历退化成非递归了：{:?}",
                files.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>()
            );
        }
        // ★〔audit-0805 08-06〕**再按「调用」扫一遍** —— 上面那批针全是路径拼法。
        //
        // 实测：往 `inbound.rs` 写
        //   `use tokio::time::{self as _t, sleep};`
        //   `async fn probe() { loop { sleep(Duration::new(5, 0)).await; } }`
        // ——一个货真价实的无限周期唤醒，**六条判据全绿**：
        // `time::sleep` 对不上（导入写成了花括号组）、`Duration::from_` 也对不上
        //（用的是 `Duration::new`）⇒ 两道防线**同时**被同一种改写绕过去。
        //
        // 补的是**调用形态**（名字要是完整的词、后面紧跟 `(`），与怎么导入无关。
        // ⚠ 补之前量过误红面：这八个名字在 daemon 生产段今天**全为 0 处**。
        for (name, code) in &files {
            for call in [
                "sleep",
                "interval",
                "interval_at",
                "recv_timeout",
                "park_timeout",
                "wait_timeout",
                "timeout",
                "tick",
            ] {
                if let Some(line) = code.lines().find(|l| {
                    let t = l.trim_start();
                    !t.starts_with("//") && is_call_of(l, call)
                }) {
                    panic!(
                        "零定时器护栏违规（P6，**按调用形态**逮到）：生产代码 {name} 里有 `{call}(` 调用：\n  {}\n\
                         daemon 的判活全部由内核事件驱动（inotify / pidfd / tmux hook）。\n\
                         ⚠ 换个 import 写法或换个 `Duration` 构造器**绕不过这一条** —— 它只看调用。",
                        line.trim()
                    );
                }
            }
        }
        for (name, code) in &files {
            for pat in periodic_wake_patterns() {
                assert!(
                    !code.contains(&pat),
                    "零定时器护栏违规（P6）：生产代码 {name} 含周期性唤醒构件 `{pat}`。\n\
                     daemon 的判活全部由内核事件驱动（inotify / pidfd / tmux hook→SIGUSR1）；\n\
                     加回定时器等于把 P0-P5 的收益悄悄退掉。确有必要请先改本护栏的登记表并说明理由。"
                );
            }
        }
    }

    /// ★ `Duration::from_*` 的每一处都必须在登记表里 —— **多一处就红**。
    ///
    /// 这条与上一条互补：上一条禁「会自己醒的构件」，这条防「用没被列名的方式
    /// 把节拍偷渡回来」（比如 `from_millis(8000)`）。
    #[test]
    fn every_duration_use_is_registered_as_non_timer() {
        let needle = format!("Duration::{}", "from_");
        let mut found: Vec<(String, usize)> = Vec::new();
        for (name, code) in daemon_sources() {
            let n = code.matches(&needle).count();
            if n > 0 {
                found.push((name, n));
            }
        }
        let total: usize = found.iter().map(|(_, n)| n).sum();
        assert_eq!(
            total,
            REGISTERED_DURATION_USES.len(),
            "生产段 `Duration::from_*` 有 {total} 处，登记表里只有 {} 条：{found:?}\n\
             新增的那处若确实不是定时器，把它加进 REGISTERED_DURATION_USES 并写明理由；\n\
             若它是节流常量 —— 那就是本护栏要拦的东西。",
            REGISTERED_DURATION_USES.len()
        );
        // 登记表里的每条都必须**还在**（删了代码却留着登记 = 表在腐烂）。
        //
        // ★ 匹配按**文件名**不按完整相对路径（Phase E 审计 R4）：`daemon_sources()` 现在返回
        // `observe/watcher.rs` 这种相对路径，而本表的键是裸文件名。若这里用全等，U2/U3 把
        // `watcher.rs` 搬进 `observe/` 的那一刻，本断言就会以「登记表在腐烂」红掉 ——
        // 那是一条**误导性诊断**：表没腐烂，只是文件搬了家。而 §4.1 红线又盯着这张表不许乱动，
        // 于是下一个人只能在「改红线表」和「改护栏」之间二选一。⇒ 现在就让纯搬家不触碰它。
        // （真删掉那处代码仍会红 —— `code.contains(snippet)` 那半边管这个。）
        for (file, snippet, ..) in REGISTERED_DURATION_USES {
            let hit = daemon_sources()
                .into_iter()
                .any(|(n, code)| matches_registered(&n, file) && code.contains(snippet));
            assert!(
                hit,
                "登记表里的 {file} / `{snippet}` 已经不在生产代码里了，请清理登记"
            );
        }
    }

    /// `matches_registered` 的真值表 —— 尤其是**今天还没被真跑过**的 `ends_with` 那一侧。
    #[test]
    fn registered_file_matching_survives_a_move_into_a_subdir() {
        // 今天走的分支：文件还在顶层，全等命中。
        assert!(matches_registered("watcher.rs", "watcher.rs"));
        // U3 之后才会走的分支：搬进子目录仍要命中（否则「纯搬家」会被误报成「登记腐烂」）。
        assert!(matches_registered("observe/watcher.rs", "watcher.rs"));
        assert!(matches_registered("a/b/watcher.rs", "watcher.rs"));
        // 反向：**不许**把名字相近的当成同一个。这几条是这条判据真正的价值所在。
        assert!(!matches_registered("notwatcher.rs", "watcher.rs"));
        assert!(!matches_registered("observe/notwatcher.rs", "watcher.rs"));
        assert!(!matches_registered("watcher.rs.bak", "watcher.rs"));
        assert!(!matches_registered("watcher.rsx", "watcher.rs"));
        assert!(!matches_registered("", "watcher.rs"));
    }

    /// 登记表每条都要有非空理由 —— 不写理由的登记等于无声豁免。
    ///
    /// 〔`K-G6` `KG64`〕同轮加了两栏：**走的是哪一格**（闭集比对）与**解锁条件**（长度地板）。
    #[test]
    fn registered_uses_all_have_reasons() {
        for (file, snippet, why, cell, unlock) in REGISTERED_DURATION_USES {
            assert!(
                why.len() > 20,
                "{file} / `{snippet}` 的登记理由太短，说不清它为什么不是定时器"
            );
            assert!(
                crate::readonly_guard::g6_doctrine::is_cell(cell),
                "{file} / `{snippet}` 的第四栏是 `{cell}` —— 它不在 `§0a` 四情形表的闭集里（`{}`）。\n\
                 往这张表里加一条**必须说得出它走的是哪一格** —— \n\
                 ★★ 「加白名单」不是那四格里的任何一格：**放宽 ≠ 加白名单**。",
                crate::readonly_guard::g6_doctrine::cell_names().join(" / ")
            );
            assert!(
                unlock.trim().chars().count() >= 20,
                "{file} / `{snippet}` 没写**解锁条件**（实得 {} 字）—— 要写的是「什么条件满足之后\
                 这一条就能删」，不是「为什么现在不能删」",
                unlock.trim().chars().count()
            );
        }
    }
}

/// 〔`K-G6` `KG61`〕**本护栏今天真能拦住的形状全表 + 一个今天就通过了的反例。**
///
/// # 先列全表，再谈放宽（`§0c 裁五`）
///
/// 这一道判据本身的质量很高（三条反空真 + 调用形态 + 相等断言），**它不是「写得松」**。
/// 它买不到的东西全部落在**人群**上：人群按「本 crate 的源码文本」画，
/// 而「这个进程里有没有东西在自己醒来」是**进程**层面的事实。
/// ⇒ 下面的反例既不是漏洞、也不需要放宽 —— 它是「**它本来就拦不住**」那一侧的活体。
#[cfg(test)]
mod g6_reach {
    use super::tests::{
        daemon_sources, is_call_of, periodic_wake_patterns, REGISTERED_DURATION_USES,
    };

    /// 按**调用形态**扫的那八个名字（与判据本体同一份清单，抄第二份必然漂开）。
    const CALL_FORMS: &[&str] = &[
        "sleep",
        "interval",
        "interval_at",
        "recv_timeout",
        "park_timeout",
        "wait_timeout",
        "timeout",
        "tick",
    ];

    /// ★ 全表①：**按源码文本子串**禁掉的六个构件，逐形一刀。
    ///
    /// ⚠ 射程如实登记：这一层是**子串**匹配、**没有词边界** ——
    /// 名字里含这几个字的东西也会命中（那一侧是「人群画大了」，靠人一看即知排除）。
    /// 带词边界的那半是下面的**调用形态**层，两层是互补的，不是重复的。
    #[test]
    fn every_banned_construct_is_still_matched_by_substring() {
        let pats = periodic_wake_patterns();
        assert_eq!(
            pats.len(),
            6,
            "禁用构件表从 6 条变成 {} 条了 —— **全表的分母变了**。\n\
             变少 = 有人从表里拿掉了一个构件，那是放宽：先摆出「它今天拦得住什么」再谈。",
            pats.len()
        );
        let mut uniq = pats.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), pats.len(), "禁用构件表里有重复项：{pats:?}");
        for pat in &pats {
            assert!(
                pat.chars().count() >= 6,
                "禁用构件 `{pat}` 短到会大面积误伤 —— 子串层没有词边界"
            );
            // 判据本体用的就是 `code.contains(pat)`：这一刀量的是同一个对象。
            assert!(
                format!("    let _ = {pat}(x);").contains(pat.as_str()),
                "子串层对 `{pat}` 这个形状不响了 —— 全表里这一格今天是空的"
            );
        }
    }

    /// ★ 全表②：**按调用形态**（词边界 + 后面紧跟括号）扫的那八个名字，
    /// 逐形**两刀**：真调用要认出来、形近的不许误认。
    ///
    /// 这一格是本护栏最有牙的一格 —— 它是实测换来的：
    /// 「换个 import 写法 + 换个 `Duration` 构造器」曾让六条判据同时全绿。
    #[test]
    fn every_call_form_fires_on_a_real_call_and_not_on_a_lookalike() {
        assert_eq!(
            CALL_FORMS.len(),
            8,
            "调用形态表从 8 条变成 {} 条了 —— 全表的分母变了",
            CALL_FORMS.len()
        );
        for name in CALL_FORMS {
            assert!(
                is_call_of(&format!("    {name}(d).await;"), name),
                "调用形态 `{name}(` 认不出来了 —— 全表里这一格今天是空的"
            );
            assert!(
                is_call_of(&format!("    tokio::time::{name}(d);"), name),
                "带路径的 `{name}(` 认不出来了"
            );
            assert!(
                !is_call_of(&format!("    let x = do_{name}(3);"), name),
                "`do_{name}(` 被当成了 `{name}(` —— 词边界没守住，误红会让人把这条判据删掉"
            );
            assert!(
                !is_call_of(&format!("    let {name}_ms = 5;"), name),
                "只是同名变量、后面不是括号，不该算调用"
            );
        }
    }

    /// ★ 全表③：`Duration::from_*` 那条是**恰好相等**，分母 = 登记表条数。
    ///
    /// ⚠ 下面刻意先把条数落到一个局部变量再断言：判据本体那条断言的实参行
    /// （`REGISTERED_DURATION_USES.len(),`）被 `ratchet_guard::PINS` 按
    /// 「整行相等 + **恰好 1 次**」钉着，在这里原样再写一遍会让它变成 2 次 ⇒ 棘轮当场红，
    /// 而红的原因与本条无关。**这一格是本轮现打出来的，不是想出来的。**
    #[test]
    fn the_duration_registry_is_an_equality_not_a_floor() {
        let registered = REGISTERED_DURATION_USES.len();
        // 〔`K-P6b` 09-06〕3 → **4**：新增的那一条是 `dial/mod.rs` 的 SSH keepalive 间隔。
        // ⚠ 那一条的登记理由**与前三条不同族** —— 它没说「这不是定时器」，
        //   它说的是「有一个，而它长在依赖 crate 里、在本护栏的人群外面」。
        //   ⇒ 这个分母涨了 1，而**护栏能拦住的形状一个都没多**。别把涨读成变强。
        assert_eq!(
            registered, 4,
            "登记表从 4 条变成 {registered} 条了 —— 这个数就是那条相等断言的分母，\
             改它等于改判据的射程"
        );
    }

    /// 〔`KG61` ②〕**今天在盘上、形状与本护栏声称要拦的相同、而它放过了的那一处。**
    ///
    /// `(住址, 生产段里的片段, 形状对得上的是禁用表里的哪几条, why, unlock)`
    const KNOWN_PASSING_COUNTEREXAMPLES: &[(&str, &str, &str, &str, &str)] = &[(
        "observe/watcher.rs",
        "new_debouncer(",
        "recv_timeout · Instant::now",
        "生产段亲手拉起一个**依赖 crate** 的去抖器，而那个 crate 内部起了一条具名线程，\
         循环里做**带超时的接收**、并用「现在几点」算下一次期限 —— \
         这两个构件逐字就是本护栏禁用表里的第 3 条与第 5 条，\
         护栏头注还专门解释过其中一条「等不到就自己醒，等价于给循环装了节拍」。\
         **形状对得上，而它一个字都看不见**：人群是本 crate `src/` 的源码文本，\
         依赖 crate 的源码不在里面。\
         ⚠ **诚实边界，别把它说过头**：那条线程**不是自由跑的节拍器** —— \
         有待合并事件时才带期限等，空闲时是无期限阻塞。\
         ⇒ 它**不构成**「daemon 在轮询」的证据；它构成的是\
         「护栏按名字禁掉的构件，此刻正在这个进程里运行」的证据。两件事别混。",
        "去抖改由本 crate 自己实现（那时它落回人群内、由判据管），\
         或人群从「本 crate 源码文本」扩到「进程里实际跑着什么」的那天 —— \
         后者今天做不到，所以这一条会躺很久，**而躺着正是它的岗位**：\
         它让「本护栏的射程」写在盘上、不许腐烂。",
    )];

    /// ★ 反例仍在盘上、仍然通过；而它撞的那两个构件仍然在禁用表里。
    ///
    /// ★★ 本件专属陷阱（件文件 `§3`）在这一格的答案：这条路**未经任何放宽就已经通过**
    /// —— 人群（`daemon_sources()`，分母 = 本 crate `src/` 树上的 `.rs` 减去跳过的那一个）
    /// 里根本没有那个依赖 crate 的源码。⇒ 正确说法不是「放宽了没红」，是「**它对这条路本来就不响**」。
    /// ⚠ 这句话是对**今天的判据与人群**说的；历史上它有没有因为别的原因红过，我没查。
    #[test]
    fn the_counterexample_is_still_on_the_board_and_still_passes() {
        assert_eq!(KNOWN_PASSING_COUNTEREXAMPLES.len(), 1, "反例表的条数变了");
        let files = daemon_sources();
        let bytes: usize = files.iter().map(|(_, c)| c.len()).sum();
        // 本条自己的反空真地板。**刻意不复用判据本体那个常量**：
        // 那一行被 `ratchet_guard::PINS` 按「整行相等 + 恰好 1 次」钉着，
        // 在这里再写一遍会让它变成 2 次 ⇒ 棘轮当场红，而红的原因与本条无关。
        const FLOOR_FOR_THIS_CHECK: usize = 60_000;
        assert!(
            bytes >= FLOOR_FOR_THIS_CHECK,
            "只扫到 {bytes} 字节生产代码 —— 采集坏了，下面几条在空转"
        );
        for (rel, frag, shapes, why, unlock) in KNOWN_PASSING_COUNTEREXAMPLES {
            let owner = files
                .iter()
                .find(|(n, _)| n.as_str() == *rel)
                .unwrap_or_else(|| panic!("反例住址 `{rel}` 今天不在人群里了 —— 幽灵条目"));
            assert!(
                owner.1.contains(frag),
                "反例 `{rel}` 的生产段里找不到 `{frag}` 了 —— 改掉了就**同轮摘登记**，\
                 并回来重判本护栏的射程"
            );
            // 形状真的对得上：它撞的那两个构件今天**仍在**禁用表里。
            let pats = periodic_wake_patterns();
            for shape in shapes.split(" · ") {
                assert!(
                    pats.iter().any(|p| p.contains(shape)),
                    "反例声称撞的 `{shape}` 已经不在禁用表里了 —— \
                     那这条反例证不了「形状对得上」，回来重判"
                );
            }
            assert!(why.trim().chars().count() >= 20, "`{rel}` 的 why 太短");
            assert!(unlock.trim().chars().count() >= 20, "`{rel}` 没写解锁条件");
        }
        // ★ 它今天**真的通过** —— 全人群对那两个构件零命中。
        let pats = periodic_wake_patterns();
        let mut hits: Vec<String> = Vec::new();
        for (name, code) in &files {
            for pat in &pats {
                if code.contains(pat.as_str()) {
                    hits.push(format!("  {name}: {pat}"));
                }
            }
        }
        assert!(
            hits.is_empty(),
            "禁用构件在人群里出现了：\n{}\n\
             ⇒ 那是本护栏的正题在红，不是本条 —— 先修那个，再回来看反例表",
            hits.join("\n")
        );
    }

    /// ★ 一条**查过、但不算反例**的路，如实登记（免得下一轮有人再查一遍）。
    ///
    /// `plugin/invoke.rs` 用 `timeout` 当命令前缀交给子进程。
    /// `is_call_of` 认不出 `Command::new("timeout")`（前面是引号、后面不是括号）⇒ **确实通过**，
    /// 但它是**一次性期限**，不是周期唤醒 —— **形状不同，不算反例**。
    /// 这一条钉住那个判断今天仍然成立：那处仍然只有命令前缀这一种用法。
    #[test]
    fn the_timeout_command_prefix_is_checked_and_is_not_a_counterexample() {
        assert!(
            !is_call_of("    let mut cmd = Command::new(\"timeout\");", "timeout"),
            "`Command::new(\"timeout\")` 被当成了 `timeout(` 调用 —— \
             那会是一次误红，而误红最省事的消法是把判据删掉"
        );
        assert!(
            is_call_of("    let r = timeout(d, fut).await;", "timeout"),
            "真的 `timeout(` 调用必须认出来，否则上面那条靠「什么都认不出」恒真"
        );
    }
}
