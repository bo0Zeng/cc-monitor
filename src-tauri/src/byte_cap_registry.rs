//! **字节上限的登记表**〔audit-0805 F06 下半，报告 B-5〕。
//!
//! # 计数被改了两次，两次都还不够
//!
//! | 谁 | 说几处 |
//! |---|---|
//! | 报告 B-5 | **3** 处 |
//! | 核实台账（V1）| **6** 处（`accounts_query` ×3 · `fork_write` · `remote_history` · `ssh_source`）|
//! | 本轮系统扫描 | **14** 处 |
//!
//! 台账漏掉的八处里，`ssh_source.rs` 自己就还有三处（`EXEC_CAPTURE_MAX_BYTES` /
//! `DAEMONLESS_READ_CAP` / `DAEMONLESS_DISCOVER_CAP`）。
//! ⇒ **E1「筛子不是免检章」这一轮兑现在计数上**：筛过一次的数字仍然可能是错的。
//!
//! # 账本 S3 的「最终形态」写错了，本轮改形
//!
//! S3 原文：「**一个**常量单一来源 + **一种**超限语义」。
//! 那是 Phase A 在「只有三处、且是同一个量」的假设下写的。核实之后：
//! **14 处、至少 9 种不同的量**（账号清单 / 单会话 jsonl / 首连快照 / exec 输出 / 单行 / 编辑体量…）。
//!
//! 硬收成一个常量就是**把不同的东西按数字凑到一起** —— 那会让下一次调整某一处时连累其余。
//! ⇒ 最终形态改成：
//!
//! 1. **每一处都说清它管的是什么量、超限怎么办**（新增没登记的就红）；
//! 2. **同一个量只许有一处权威**，跨 crate 的那几对**靠机检不靠人抄**；
//! 3. **超限语义不许有「静默」那一种**（F06 上半已经修掉最后一处）。
//!
//! ⚠ 这与 S5 上一轮那次订正同族：**账本的「最终形态」列也会写错**，
//! 而它是后来者判断「这一面算不算做完」的唯一依据。

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// 超限之后怎么办。**刻意是个封闭集合** —— 多出第四种就得回来论证。
    ///
    /// ⚠ 里面**没有「静默截断」**：报告 B-5 点名的那一处（`remote_history`）
    /// 已由 F06 上半改成「截断 + 说清」。这个集合就是那次修复的护栏。
    const ALLOWED_SEMANTICS: &[&str] = &[
        "硬报错",
        "截断+说清",
        "拒收+回错",
        "分轮续读",
        // ⚠ 第五种是本轮**论证后**加的，不是顺手加的：搜索索引的字符封顶截掉的是
        // **可搜性**，不是数据 —— 原始 jsonl 一个字节没动，只是超过封顶的那段搜不到。
        // 它与「静默截断」的分界就在这里：**有没有丢掉用户的东西**。
        "索引截断（不丢数据）",
    ];

    /// 扫到了但**不是体量上限**的，逐条写清为什么排除。
    ///
    /// ⚠ 排除表也会腐：下面第一条会检查每条排除**真的还扫得到**，
    /// 否则它就是一条永远不匹配的死规则，而死规则会在下次有人往这个名字上写真上限时悄悄放行。
    const NOT_A_SIZE_CAP: &[(&str, &str)] = &[
        (
            "CHANNEL_CAPACITY",
            "**条数**不是体量（mpsc 通道能排多少帧）。它的溢出语义由 `Overflow` 帧管，见 F03。",
        ),
        // ⚠ `MIN_SCANNED_CODE_BYTES` 那条已删（08-06）：它是**测试段里的地板**，
        //    本表原来扫整份文件才需要排它；扫描面收窄到生产段之后它成了死规则，
        //    而本表自己的 `the_exclusion_list_is_not_dead_wood` 当场要求删。
    ];

    /// `(相对仓根的路径, 常量名, 字节数, 它管的是什么量, 超限怎么办)`。
    ///
    /// ⚠ **登记表不是豁免清单**：新增一处没登记的 ⇒ 下面第一条红。
    /// 值改了也会红（第二条把表里的数字与源码对拍）——**那正是该重新想「这个量该多大」的时刻**。
    #[allow(clippy::type_complexity)]
    const CAPS: &[(&str, &str, u64, &str, &str)] = &[
        // ---- monitor 侧 ----
        (
            "src-tauri/src/local_accounts.rs",
            "MANIFEST_CAP",
            8 * 1024 * 1024,
            "本机读账号 manifest",
            "硬报错",
        ),
        (
            "src-tauri/src/remote_history.rs",
            "MAX_SESSION_BYTES",
            256 * 1024 * 1024,
            "读一整份远端会话 jsonl",
            "截断+说清",
        ),
        (
            "src-tauri/src/sftp_pool.rs",
            "MAX_EDIT_BYTES",
            256 * 1024,
            "SFTP 在线编辑的文件体量",
            "拒收+回错",
        ),
        (
            "src-tauri/src/ssh_source.rs",
            "SNAPSHOT_MAX_BYTES",
            512 * 1024 * 1024,
            "首连快照单会话体量",
            "截断+说清",
        ),
        (
            "src-tauri/src/ssh_source.rs",
            "EXEC_CAPTURE_MAX_BYTES",
            4 * 1024 * 1024,
            "一次 exec 的 stdout/stderr 各自收集量",
            "截断+说清",
        ),
        (
            "src-tauri/src/ssh_source.rs",
            "DAEMONLESS_READ_CAP",
            8 * 1024 * 1024,
            "daemonless 单文件**单轮**读字节",
            "分轮续读",
        ),
        (
            "src-tauri/src/ssh_source.rs",
            "DAEMONLESS_DISCOVER_CAP",
            4 * 1024 * 1024,
            "daemonless 发现命令的 stdout",
            "截断+说清",
        ),
        (
            "src-tauri/src/launch.rs",
            "MAX_REMOTE_CMD",
            4096,
            "远端命令串长度（字节）",
            "拒收+回错",
        ),
        (
            "src-tauri/src/search.rs",
            "MAIN_CAP",
            20_000,
            "单条 main 文本进索引的**字符**数",
            "索引截断（不丢数据）",
        ),
        (
            "src-tauri/src/search.rs",
            "TOOL_CAP",
            4_000,
            "单条 tool 文本进索引的**字符**数",
            "索引截断（不丢数据）",
        ),
        // ---- daemon 侧 ----
        (
            "remote-daemon-proto/src/control/fork_write.rs",
            "MAX_SESSION_JSONL_BYTES",
            256 * 1024 * 1024,
            "分叉时读源会话 jsonl",
            "硬报错",
        ),
        (
            "remote-daemon-proto/src/control/launch.rs",
            "MAX_FIELD_BYTES",
            8 * 1024,
            "启动请求单字段（载荷/名字/cwd）",
            "拒收+回错",
        ),
        (
            "remote-daemon-proto/src/control/resolve_query.rs",
            "MAX_RESOLVE_STDIN",
            1 << 20,
            "`--resolve` 的 stdin",
            "截断+说清",
        ),
        (
            "remote-daemon-proto/src/inbound.rs",
            "MAX_LINE_BYTES",
            1 << 20,
            "入方向单行",
            "拒收+回错",
        ),
        (
            "remote-daemon-proto/src/observe/accounts_query.rs",
            "MAX_CLAUDE_JSON_BYTES",
            32 * 1024 * 1024,
            "读 `.claude.json`",
            "硬报错",
        ),
        (
            "remote-daemon-proto/src/observe/accounts_query.rs",
            "MAX_MANIFEST_BYTES",
            8 * 1024 * 1024,
            "daemon 侧读账号 manifest",
            "硬报错",
        ),
        (
            "remote-daemon-proto/src/observe/search_query.rs",
            "MAIN_CAP",
            20_000,
            "daemon 侧同一封顶（注释逐字「对齐本地封顶」）",
            "索引截断（不丢数据）",
        ),
        (
            "remote-daemon-proto/src/observe/search_query.rs",
            "TOOL_CAP",
            4_000,
            "daemon 侧同一封顶",
            "索引截断（不丢数据）",
        ),
        (
            "remote-daemon-proto/src/observe/accounts_query.rs",
            "MAX_SESSION_FILE_BYTES",
            1024 * 1024,
            "读单个 `sessions/<PID>.json`",
            "硬报错",
        ),
    ];

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf()
    }

    /// 从源码里把 `const NAME: ty = <expr>;` 的字节数算出来。
    ///
    /// 只认本仓在用的两种写法：`A * 1024 * 1024` 与 `1 << N`。
    /// ⚠ 认不出来就返回 `None` 让调用方红 —— **不许猜**，猜错会让对拍变成一句空话。
    fn eval_cap(expr: &str) -> Option<u64> {
        let e = expr
            .split("//")
            .next()
            .unwrap_or(expr)
            .trim()
            .trim_end_matches(';')
            .trim();
        let e = e.replace('_', "");
        if let Some((l, r)) = e.split_once("<<") {
            let (l, r) = (l.trim().parse::<u64>().ok()?, r.trim().parse::<u32>().ok()?);
            return l.checked_shl(r);
        }
        let mut acc: u64 = 1;
        for part in e.split('*') {
            acc = acc.checked_mul(part.trim().parse::<u64>().ok()?)?;
        }
        Some(acc)
    }

    /// 扫两棵树，抠出所有「像字节上限」的常量：名字含 MAX/CAP/LIMIT/BYTES 且值里有 1024 或 `<<`。
    ///
    /// ⚠ 走 `guard_core::scan_tree!` 而不是自己 `read_dir` —— 它**按构造摘除调用者自己那份**。
    /// 本文件的 `CAPS` 表里就写着一堆 `MAX_*` 名字；今天它们的类型不是 `u64/usize` 所以扫不中，
    /// 但那是**运气**不是设计。〔audit-0805 **F23**：这一族已实测栽过五次〕
    fn scan() -> Vec<(String, String, Option<u64>)> {
        let root = repo_root();
        let mut out = Vec::new();
        for sub in ["src-tauri/src", "remote-daemon-proto/src"] {
            for (f, raw) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
                // ★ 只扫**生产段**〔08-06〕：本条原来扫整份文件，于是**测试里的夹具常量**
                // 也被当成生产上限（`common/fs.rs` 的顺序判据里那个 `const CAP` 当场被误报）。
                // 判据扫生产段是本仓通行做法（`guard_core` 头注那一族）—— 这里此前是例外。
                let body = guard_core::production_source(&raw);
                let rel = f
                    .strip_prefix(&root)
                    .unwrap_or(&f)
                    .to_string_lossy()
                    .replace('\\', "/");
                for line in body.lines() {
                    let t = line.trim();
                    let Some(rest) = t
                        .strip_prefix("pub(crate) const ")
                        .or_else(|| t.strip_prefix("pub const "))
                        .or_else(|| t.strip_prefix("const "))
                    else {
                        continue;
                    };
                    let Some((name, tail)) = rest.split_once(':') else {
                        continue;
                    };
                    let name = name.trim();
                    if !["MAX", "CAP", "LIMIT", "BYTES"]
                        .iter()
                        .any(|k| name.contains(k))
                    {
                        continue;
                    }
                    let Some((ty, expr)) = tail.split_once('=') else {
                        continue;
                    };
                    if !matches!(ty.trim(), "u64" | "usize" | "u32") {
                        continue;
                    }
                    if NOT_A_SIZE_CAP.iter().any(|(n, _)| *n == name) {
                        continue;
                    }
                    // 下界 1024：更小的多半是「条数 / 次数」而不是体量。
                    // ⚠ 这条过滤本身是个取舍，`the_exclusion_list_is_not_dead_wood` 盯着它。
                    match eval_cap(expr) {
                        Some(v) if v >= 1024 => out.push((rel.clone(), name.to_string(), Some(v))),
                        Some(_) => {}
                        None => out.push((rel.clone(), name.to_string(), None)),
                    }
                }
            }
        }
        out.sort();
        out
    }

    /// ★ 正题一：**每一处字节上限都得登记它管什么量、超限怎么办**。
    #[test]
    fn every_byte_cap_says_what_it_bounds_and_what_happens_past_it() {
        let found = scan();
        // 抽取器自检：抠不到东西时下面的对拍会两边都空、静默变绿。
        assert!(
            found.len() >= 12,
            "全仓只扫到 {} 个字节上限常量（08-06 实测 14）—— 抽取器坏了",
            found.len()
        );

        let missing: Vec<String> = found
            .iter()
            .filter(|(f, n, _)| !CAPS.iter().any(|(g, m, ..)| g == f && m == n))
            .map(|(f, n, v)| format!("  {f}  {n} = {v:?}"))
            .collect();
        assert!(
            missing.is_empty(),
            "有字节上限没登记：\n{}\n\n\
             登记时要回答两件事，**都不许留空**：\n\
             ① 它管的是**什么量**（账号清单？一整份会话？一次 exec 的输出？）——\n\
                报告 B-5 说「三处上限该收成一个常量」，核实之后是 **14 处、至少 9 种不同的量**，\n\
                硬收成一个就是把不同的东西按数字凑到一起。\n\
             ② **超限怎么办**，且只能是 {ALLOWED_SEMANTICS:?} 之一 —— \n\
                **没有「静默截断」这一项**（那正是报告 B-5 点名、F06 上半修掉的那处）。",
            missing.join("\n")
        );
        // 反向：登记的必须真在（改名/删掉了就该删条目）。
        for (f, n, ..) in CAPS {
            assert!(
                found.iter().any(|(g, m, _)| g == f && m == n),
                "登记表里的 `{f}::{n}` 已经不在源码里了 —— 删掉这条，别留僵尸账"
            );
        }
    }

    /// ★ 正题二：**表里的数字必须与源码一致** —— 否则这张表只是一份会腐的散文。
    #[test]
    fn the_registered_numbers_still_match_the_source() {
        for (f, n, want, what, _) in CAPS {
            let got = scan()
                .into_iter()
                .find(|(g, m, _)| g == f && m == n)
                .map(|(_, _, v)| v);
            let Some(Some(v)) = got else {
                panic!(
                    "`{f}::{n}` 的值算不出来（写法不是 `A * 1024 * …` 也不是 `1 << N`）。\
                     ⚠ **不许猜** —— 猜错会让这张表变成一句空话。要么把写法改回来，要么扩 `eval_cap`。"
                );
            };
            assert_eq!(
                v, *want,
                "`{f}::{n}`（{what}）源码是 {v} 字节，表里写的是 {want}。\
                 ★ **改上限的时候正是该重新想「这个量该多大、超了怎么办」的时刻** —— \
                 别只把表里的数字改成新的就完事。"
            );
        }
    }

    /// ★ 正题三：**跨 crate 那几对「与另一侧同值」的注释，变成机检**（账本 S3 的钉法）。
    ///
    /// ⚠ 三对的**口径不一样**，不能一把梭都钉「相等」：
    /// 第三对的注释写的是「同**量级**」，而它们**今天就不等**（8 KiB vs 4 KiB）——
    /// 钉「相等」会造出一条假判据。
    #[test]
    fn the_cross_crate_twins_are_machine_checked_not_hand_copied() {
        let by = |f: &str, n: &str| -> u64 {
            scan()
                .into_iter()
                .find(|(g, m, _)| g == f && m == n)
                .and_then(|(_, _, v)| v)
                .unwrap_or_else(|| panic!("抠不到 `{f}::{n}` —— 本条会零命中地绿"))
        };

        // 对 A：注释逐字「与 daemon 侧**同值**」⇒ 钉相等。
        let a1 = by("src-tauri/src/local_accounts.rs", "MANIFEST_CAP");
        let a2 = by(
            "remote-daemon-proto/src/observe/accounts_query.rs",
            "MAX_MANIFEST_BYTES",
        );
        assert_eq!(
            a1, a2,
            "两侧读同一份 manifest 的上限漂开了（monitor {a1} / daemon {a2}）。\
             `local_accounts.rs` 的注释逐字写着「与 daemon 侧同值」—— \
             那句话此前**没有任何东西守着**，靠人抄。"
        );

        // 对 B：两边都是「一整份会话 jsonl」这同一个量 ⇒ 钉相等。
        let b1 = by("src-tauri/src/remote_history.rs", "MAX_SESSION_BYTES");
        let b2 = by(
            "remote-daemon-proto/src/control/fork_write.rs",
            "MAX_SESSION_JSONL_BYTES",
        );
        assert_eq!(
            b1, b2,
            "「一整份会话 jsonl」的上限两侧漂开了（monitor {b1} / daemon {b2}）。\
             ⚠ `fork_write.rs` 的注释写的是「同一**量级**」，而本条钉的是**相等** —— \
             因为它们是同一个量。要刻意分开就把这条判据与那句注释**一起**改。"
        );

        // 对 D：搜索索引封顶。daemon 侧注释逐字「**对齐本地封顶**」⇒ 钉相等。
        for cap in ["MAIN_CAP", "TOOL_CAP"] {
            let d1 = by("src-tauri/src/search.rs", cap);
            let d2 = by("remote-daemon-proto/src/observe/search_query.rs", cap);
            assert_eq!(
                d1, d2,
                "搜索索引封顶 `{cap}` 两侧漂开了（monitor {d1} / daemon {d2}）。\
                 daemon 侧注释逐字写着「对齐本地封顶防超长粘贴/大 dump」—— \
                 那句话此前**没有任何东西守着**。漂开的后果是**同一个查询在本地与远端命中不一样**，\
                 而两边都不会报错。"
            );
        }

        // 对 C：注释写的是「同**量级**」，而且**今天就不等** ⇒ 钉比值，不钉相等。
        let c1 = by(
            "remote-daemon-proto/src/control/launch.rs",
            "MAX_FIELD_BYTES",
        );
        let c2 = by("src-tauri/src/launch.rs", "MAX_REMOTE_CMD");
        let (hi, lo) = if c1 >= c2 { (c1, c2) } else { (c2, c1) };
        assert!(
            lo > 0 && hi <= lo * 4,
            "「一条人能读的启动命令」两侧差得太远（daemon {c1} / monitor {c2}）。\
             ⚠ 这一对**刻意不同值**（今天 8192 vs 4096），注释写的是「同量级」；\
             这里把「量级」形式化成**不超过 4 倍** —— 那是个约定，不是实测阈值。"
        );
    }

    /// ★ **排除表不许长草**（上面 `scan()` 的注释引了这条 —— 铁律 14：
    /// 指向不存在的判据比没有注释更坏，所以它必须真的存在）。
    ///
    /// 排除掉的名字必须**真的还在源码里**：否则那条排除是一条永远不匹配的死规则，
    /// 而死规则会在下一次有人往这个名字上写一个**真的**体量上限时**悄悄放行**。
    #[test]
    fn the_exclusion_list_is_not_dead_wood() {
        let root = repo_root();
        let mut all = String::new();
        for sub in ["src-tauri/src", "remote-daemon-proto/src"] {
            for (_, raw) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
                let body = guard_core::production_source(&raw);
                all.push_str(&body);
            }
        }
        assert!(
            all.len() > 100_000,
            "只读到 {} 字节源码 —— 抽取器坏了，下面每条都会零命中地绿",
            all.len()
        );
        for (name, why) in NOT_A_SIZE_CAP {
            assert!(
                all.contains(&format!("const {name}")),
                "排除表里的 `{name}` 已经不在源码里了 —— 删掉这条，别留死规则（{why}）"
            );
            assert!(
                why.chars().count() > 15,
                "`{name}` 的排除理由太短，像是占位：「{why}」。\
                 排除一个名字要说清**它为什么不是体量上限**（条数？地板？次数？）"
            );
        }
    }

    /// 每条登记的超限语义必须在封闭集合里，且**不许出现「静默」**。
    #[test]
    fn no_cap_is_allowed_to_fail_silently() {
        for (f, n, _, what, sem) in CAPS {
            assert!(
                ALLOWED_SEMANTICS.contains(sem),
                "`{f}::{n}`（{what}）的超限语义写的是「{sem}」，不在 {ALLOWED_SEMANTICS:?} 里。\
                 多出一种就得回来论证 —— 定框 **E5**：上限与超限语义**成对定义**。"
            );
            assert!(
                !sem.contains("静默") && !what.contains("静默"),
                "`{f}::{n}` 出现了「静默」。报告 B-5 点名的就是这一种（`remote_history` 那处），\
                 F06 上半已经修掉。**这个集合就是那次修复的护栏。**"
            );
        }
        // 语义集合不许长草：登记表里每一种都得真有人用。
        for sem in ALLOWED_SEMANTICS {
            assert!(
                CAPS.iter().any(|(_, _, _, _, s)| s == sem),
                "允许集合里的「{sem}」今天一处都没有人用 —— 删掉它，别留一个谁都能往里塞的口子"
            );
        }
    }
}
