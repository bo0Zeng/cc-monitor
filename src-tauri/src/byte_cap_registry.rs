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
//!    ⚠ **08-06 收窄这句话**：它只对**具名常量**成立 —— 扫描面是「名字含 MAX/CAP/LIMIT/BYTES
//!    且类型是 u64/usize/u32 的 const」。**内联字面量上限整个溜过**。
//!    实测人群：这样的内联上限全仓只有 **1 处** —— `cc_bus.rs` 的 `stream.take(4096)`
//!    （查 agent 在线：`tmux has-session … && echo ONLINE || echo OFFLINE`，预期输出 ~7 字节）。
//!    它是**防御性读上限**（防远端吐出无界输出），超限语义就是「读到上限就停」，
//!    与本表担心的「用户数据被静默截断」不同族 ⇒ **不收进表**，但也不假装已覆盖。
//!    人群只有 1 时不值得为它建通用扫描器（那是仪式）—— 由下面的前提触发器盯着它别变成 2。
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
        // ⚠ 第六种同样是**论证后**加的〔audit-0805 §5 1x，08-06〕：
        // `accounts_query::session_accounts` 读单个会话 pidfile 超限时**跳过那一条**，
        // 既不是硬报错（不会让整份账号清单失败 —— 一个坏文件不该毁掉全部归属），
        // 也不是截断（没有把半份数据当成完整的用）。
        // ★ 它与被刻意排除的「静默截断」的分界**就在那个 `warn!`**：
        // 跳过本身不是问题，**跳过而不说是谁**才是（定框 E4）。
        // ⇒ 名字里带「说清」不是修辞：没有 warn 的跳过**不许**登记成这一项。
        "跳过+说清",
        // ⚠ 第七种，同样**论证后**加〔audit-0805 §5 1x，08-06 对拍查出〕：
        // `local_accounts::load` 读 manifest 超限时**整份结果降级返回**
        // （`available: true`、账号清单为空、错误塞进 `meta`）—— 既不是硬报错
        // （不 `?` 上去），也不是跳过一条（降的是整份结果）。
        // ★ 它与「静默」的分界同样在**错误有没有被带出去**：这里带在 `meta` 里，看得见。
        // ⚠ 顺带如实登记一处**没修**的：同一个 `Err` 臂同时处理「manifest 不存在」（正常）
        // 与「manifest 超限」（异常），两者被合并成同一种降级 —— 分开报要动账号错误面的
        // UX，超出 1x 的范围，进 `ROADMAP §5`。
        "降级+说清",
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
        // ── 〔audit-0805 08-06〕默认拒绝上线后，把「名字没关键词的尺寸类常量」逐个判过。
        // **七个全都不是字节上限** —— 也就是说旧的名字关键词过滤今天恰好完整；
        // 本表登记的是**这个判断本身**，好让下一个不叫 `*_CAP` 的真上限没法悄悄溜过去。
        (
            "CHUNK",
            "**传输步长**不是上限：SFTP 每次读写多少字节，读完继续读，没有「超了怎么办」。\
             ⚠ 它是这七个里最像字节量的一个 —— 正因如此才要写下来为什么不算。",
        ),
        (
            "PROGRESS_EVERY",
            "**进度上报间隔**（每传够这么多字节报一次进度），量的是节奏不是容量。",
        ),
        (
            "NET_EPOCH_TO_WIN32_FILETIME_TICKS",
            "**时间纪元差**（.NET 与 Win32 FILETIME 的起点相差多少个 100ns tick）。单位是时间不是字节。",
        ),
        (
            "PROC_START_TOLERANCE_TICKS",
            "**时间容差**（判进程是不是同一个时允许的启动时刻误差）。单位是时间不是字节。",
        ),
        (
            "STARTTIME_IDX_AFTER_COMM",
            "**字段下标**（`/proc/<pid>/stat` 里 `starttime` 在 `comm` 之后的第几个字段）。不是量。",
        ),
        (
            "CREATE_NEW_CONSOLE",
            "**Win32 进程创建标志位**（`CreateProcess` 的 flag）。是位掩码不是尺寸。",
        ),
        (
            "CREATE_NO_WINDOW",
            "同上：Win32 进程创建标志位。",
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
            "降级+说清",
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
            "跳过+说清",
        ),
    ];

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf()
    }

    /// ★ **小到这个数以下的就不当体量看**〔audit-0805 §5 1y，08-06 给它一个名字〕。
    ///
    /// 这条过滤本身是个取舍：名字里带 `MAX`/`CAP`/`LIMIT`/`BYTES` 的小整数，绝大多数是
    /// 「条数 / 次数 / 重试上限」而不是字节体量；真出现一个比它还小的**体量**上限，
    /// 会被**静默跳过**（1y 逐字登记着这一点，并说明只有 `the_exclusion_list_is_not_dead_wood`
    /// 间接盯着它）。
    ///
    /// ⚠ 从裸字面量提成常量，是为了让 §5 那一行**只留名字、不抄值** ——
    /// 抄进计划里的值没有任何东西读它，改了这里它就成了假陈述（`plan-lint` 判据 3.9）。
    /// 名字**刻意不含** `MAX`/`CAP`/`LIMIT`/`BYTES`：那几个词正是上面 `scan()` 的钩子，
    /// 含了就会让本常量把自己当成一处待登记的上限。
    const SMALLEST_PLAUSIBLE_SIZE: u64 = 1024;

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
    /// 生产段里**全部**尺寸类 const：`const NAME: u64|usize|u32 = <expr>`。
    ///
    /// 〔audit-0805 08-06〕**抽成共享量具**：`scan()` 是它按名字关键词过滤后的子集，
    /// 而下面那条默认拒绝判据量的是它本身 —— 两者共用同一份遍历，
    /// 免得「自检量了一份副本」（本会话在 `session_name_registry` 上刚踩过）。
    fn size_typed_consts() -> Vec<(String, String, Option<u64>)> {
        let root = repo_root();
        let mut out = Vec::new();
        for sub in ["src-tauri/src", "remote-daemon-proto/src"] {
            for (f, raw) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
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
                    let Some((ty, expr)) = tail.split_once('=') else {
                        continue;
                    };
                    if !matches!(ty.trim(), "u64" | "usize" | "u32") {
                        continue;
                    }
                    match eval_cap(expr) {
                        Some(v) if v >= SMALLEST_PLAUSIBLE_SIZE => {
                            out.push((rel.clone(), name.trim().to_string(), Some(v)))
                        }
                        Some(_) => {}
                        None => out.push((rel.clone(), name.trim().to_string(), None)),
                    }
                }
            }
        }
        out.sort();
        out
    }

    /// ★〔audit-0805 08-06〕**默认拒绝**：尺寸类常量要么进人群，要么登记为「不是体量」。
    ///
    /// # 它补的洞
    ///
    /// `scan()` 的人群靠**名字里有没有 MAX/CAP/LIMIT/BYTES**。那是**按怎么写取样** ——
    /// 一个叫 `TRUNCATE_AT` / `CEILING` / `ROOM` 的字节上限整条隐形，而没有任何信号。
    ///
    /// ⚠ 实测**今天没有活的漏网**：名字不含关键词的尺寸类常量只有 4 个，
    /// 逐个读过都不是字节上限（Win32 时间纪元 · SFTP 传输步长 · 进度上报间隔 · 时间容差）。
    /// 所以本条钉的是**明天**。
    ///
    /// # 为什么不按「用法」派生（量过之后否掉的）
    ///
    /// 试过「被拿去和长度比较 / 截断的常量才算」：实测它**漏掉最像字节量的那个**
    ///（`CHUNK`，因为它用在 `read_exact` 的缓冲区长度上而不是比较里），
    /// 却**圈进两个时间常量**（`PROGRESS_EVERY` / `PROC_START_TOLERANCE_TICKS` 都用 `>=` 比）。
    /// ⇒ 派生错人群比手写清单更糟（本会话第四次量到同一条），改走默认拒绝：
    /// **人群取可机判的超集**（所有 ≥1024 的尺寸类 const），
    /// 「是不是字节上限」这个语义判断**登记成人的答案**，而不是猜。
    #[test]
    fn every_size_typed_constant_is_either_a_cap_or_registered_as_not_one() {
        let all = size_typed_consts();
        assert!(
            all.len() >= 10,
            "只扫到 {} 个尺寸类常量（08-06 实测 20+）—— 抽取器坏了，本条会零命中地绿",
            all.len()
        );
        let mut unclassified = Vec::new();
        for (rel, name, v) in &all {
            let in_population = ["MAX", "CAP", "LIMIT", "BYTES"]
                .iter()
                .any(|k| name.contains(k));
            let excused = NOT_A_SIZE_CAP.iter().any(|(n, _)| n == name);
            if !in_population && !excused {
                unclassified.push(format!("  {rel}: {name} = {v:?}"));
            }
        }
        assert!(
            unclassified.is_empty(),
            "这些尺寸类常量既不在字节上限人群里（名字没有 MAX/CAP/LIMIT/BYTES），\n\
             也没登记为「不是体量」：\n{}\n\
             ⚠ **名字不是判据** —— 一个叫 `TRUNCATE_AT` 的字节上限同样是字节上限。\n\
             要么改名进人群并在 `CAPS` 里说清「限什么 / 超了怎么办」（定框 E5 要求成对），\n\
             要么加进 `NOT_A_SIZE_CAP` 并写明它量的是什么（条数 / 时间 / 步长 …）。",
            unclassified.join("\n")
        );
    }

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
                    // 下界见 `SMALLEST_PLAUSIBLE_SIZE`：更小的多半是「条数 / 次数」而不是体量。
                    // ⚠ 这条过滤本身是个取舍，`the_exclusion_list_is_not_dead_wood` 盯着它。
                    match eval_cap(expr) {
                        Some(v) if v >= SMALLEST_PLAUSIBLE_SIZE => {
                            out.push((rel.clone(), name.to_string(), Some(v)))
                        }
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
    /// ★ **登记的语义要和调用点的处置对得上**〔audit-0805 §5 1x，08-06〕。
    ///
    /// # 1x 说的「登记的不是验过的」，这条是它可判定的那一半
    ///
    /// 本表此前只查「语义在封闭集合里」，**不查代码真的那么做了**。
    /// 08-06 逐条对之后当场查出一处不符：`MAX_SESSION_FILE_BYTES` 登记「硬报错」，
    /// 而调用点是 `Err(_) => continue` —— **静默跳过**，而「静默」正是这张封闭集合
    /// **刻意排除**的那一项（F06 上半就是为它建的）。
    ///
    /// ⇒ 本条钉：登记为 **硬报错** 的上限，它的调用点**不许把错误吞掉**。
    /// 吞掉的形态（`Err(_) =>` / `.ok()` / `unwrap_or_default` / `unwrap_or(`）出现在
    /// 用到该常量的那几行里就红。
    ///
    /// ⚠ **只查「硬报错」这一档**：其余四档（截断/拒收/分轮/索引截断/跳过）各有各的正当
    /// 处置形态，一条规则套不下 —— 硬做就是把判据的匹配面画得比事实大（本区反复记的那族）。
    /// 其余档的行为对拍**如实留在 `ROADMAP §5` 1x**，不假装覆盖了。
    #[test]
    fn a_cap_registered_as_hard_error_is_not_swallowed_at_its_call_site() {
        let root = repo_root();
        let mut checked = 0usize;
        let mut bad = Vec::new();
        for (file, name, _v, _what, sem) in CAPS {
            if !["硬报错", "跳过+说清", "降级+说清"].contains(sem) {
                continue;
            }
            let raw = std::fs::read_to_string(root.join(file))
                .unwrap_or_else(|e| panic!("{file} 读不到：{e} —— 文件搬了就把登记一起改"));
            // 剥**注释 + 测试段**：
            // ① 注释里常常逐字写着这些形态（本区第四次踩这个坑就不该再踩）；
            // ② 测试段里也会用这些常量（`accounts_query.rs` 的 symlink 用例就是），
            //    而测试里怎么处置错误与生产语义无关 —— 本条第一次跑就在那里误报。
            let src = guard_core::production_code(&raw);
            let lines: Vec<&str> = src.lines().collect();
            for (i, l) in lines.iter().enumerate() {
                if !guard_core::contains_word(l, name) || l.contains("const ") {
                    continue;
                }
                checked += 1;
                // ⚠ 窗口 **10 行是个启发式参数**，不是量出来的分界。
                //
                // 它要覆盖的是「`match` 的 `Err` 臂里返回一个结构体」这种最长的正当形态
                // （`local_accounts` 那处占 7 行）。第一版给 3 行、第二版给 6 行，
                // 都是**差一行**误报 —— 与其一路加，不如把它写成启发式并交代失效模式：
                //   · 臂特别长 ⇒ **假红**（marker 落在窗口外）；
                //   · 紧邻的**别的**语句里恰好有 marker ⇒ **假绿**。
                // 两种都靠人读诊断分辨。已登记进 `ROADMAP §5`。
                const WINDOW: usize = 10;
                let window = lines[i..(i + WINDOW).min(lines.len())].join("\n");
                // ★ **正向要求**，不是黑名单。
                //
                // 第一版列了吞掉的形态（`Err(_) =>` / `.ok()` / …）。变异当场证明它没用：
                // 把 `Err(_)` 写成 `Err(e)` 就绕过去了 —— **匹配单位（一种拼法）比事实
                // （把错误吞了）小**，本区那一族的又一次。
                // 正向问「传上去了吗」只有一个答案形态：`?`。拼写变体绕不过去。
                let (need, why) = if *sem == "硬报错" {
                    ("?", "把错误传上去")
                } else if *sem == "降级+说清" {
                    // 降级也得**把错误带出去**（塞进 `meta`/`notice`/`error` 任一）。
                    // 不带 = 静默降级，那正是这张封闭集合刻意排除的东西。
                    ("meta", "把错误带进返回值")
                } else {
                    // 「跳过+说清」那一档：跳过本身不是问题，**跳过而不说是谁**才是（E4）。
                    // 变异实测：去掉那句 `warn!`，此前**没有任何判据会红** —— 这一档
                    // 当时只是个名字。
                    ("warn!", "说清跳过的是哪一个")
                };
                // ★ **「硬报错」那档改用「同一条语句」而不是窗口**〔变异逼出来的〕。
                //
                // 窗口版当场被证伪：把 `…?` 换成 `.unwrap_or_default()`，**判据照样绿** ——
                // 因为**下一条语句**的 `?` 落进了窗口。那正是上面注释里刚写下的假绿模式，
                // 写完立刻就撞上了。
                // ⇒ 传播是**语句级**的性质：从常量所在行扫到**第一条以 `;` 收尾的行**为止。
                // 其余两档的 marker 在 `match` 臂**块内**，语句级切不到，仍用窗口。
                let scope = if *sem == "硬报错" {
                    let mut k = i;
                    while k < lines.len() && !lines[k].trim_end().ends_with(';') {
                        k += 1;
                    }
                    lines[i..(k + 1).min(lines.len())].join("\n")
                } else {
                    window.clone()
                };
                if !scope.contains(need) {
                    bad.push(format!(
                        "  {file}:{} 用 {name}（登记「{sem}」）却没有 `{need}` —— 没有{why}",
                        i + 1
                    ));
                }
            }
        }
        assert!(
            checked >= 4,
            "只找到 {checked} 个「硬报错」上限的使用点 —— 抽取器坏了，本条此刻是空转的"
        );
        assert!(
            bad.is_empty(),
            "登记的语义与调用点对不上：\n{}\n\n\
             ★ 登记「硬报错」而调用点把错误吞掉 = **登记是假的**。\n\
             08-06 实测过一处（`MAX_SESSION_FILE_BYTES` 登记硬报错、实为静默跳过）。\n\
             两条路：把调用点改成真的传上去；或者把登记改成实话 —— \n\
             若真实处置是跳过，**必须同时给它身份**（`warn!` 说清是哪一个），\n\
             才够得上「跳过+说清」那一档。**没有身份的跳过不许登记。**",
            bad.join("\n")
        );
    }

    /// 〔audit-0805 08-06〕**前提触发器：内联字面量上限只许有那一处。**
    ///
    /// 本表的扫描面只认**具名常量**（见头注）。这不是缺陷，是**范围**——
    /// 但范围要成立，得有个东西盯着「范围外那一族别长大」。
    ///
    /// 实测人群 **1**：`cc_bus.rs` 的 `stream.take(4096)`（防御性读上限，超限即停）。
    /// 人群是 1 时，为它建通用扫描器是仪式；人群变 2 的那天，这个判断就该重做 ——
    /// 本条就是那个闹钟。
    #[test]
    fn inline_literal_byte_caps_are_still_just_the_one() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut found: Vec<String> = Vec::new();
        for sub in ["src", "../remote-daemon-proto/src"] {
            let dir = root.join(sub);
            if !dir.is_dir() {
                continue;
            }
            for (path, src) in guard_core::scan_tree!(&dir, &["rs"]) {
                let prod = guard_core::production_code(&src);
                // 形态：`.take(<字面量>)` 之后跟着 `read_to_end`（即按字节读的上限）。
                let mut it = prod.match_indices(".take(");
                while let Some((i, _)) = it.next() {
                    // ⚠ 两处都是**匹配单位**的坑，第一版各犯一次：
                    //   ① 不能按字节切（中文注释里会切在多字节字符中间直接 panic）；
                    //   ② 不能用「120 字符窗口内出现 read_to_end」当判据 —— 那会把
                    //      **邻近另一行**的读操作算进来（实测抓出三个假阳：`take(32)`/`take(4)`）。
                    //   ⇒ 要求 `.take(<字面量>)` **紧邻**着 `.read_to_end` / `.read_exact`。
                    let tail: String = prod[i..].chars().take(120).collect();
                    let digits: String = tail
                        .chars()
                        .skip(6)
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    let after: String = tail
                        .chars()
                        .skip(6 + digits.chars().count())
                        .skip_while(|c| *c == ')' || c.is_whitespace())
                        .take(14)
                        .collect();
                    let is_bytes =
                        after.starts_with(".read_to_end") || after.starts_with(".read_exact");
                    if is_bytes && !digits.is_empty() {
                        found.push(format!(
                            "{}: take({digits})",
                            path.strip_prefix(root).unwrap_or(&path).to_string_lossy()
                        ));
                    }
                }
            }
        }
        // 诊断按方向分开写 —— 「变多」与「变没」要采取的动作完全不同，
        // 一句通用的「处数变了」会让人看着诊断还得再想一遍。
        assert!(
            !found.is_empty(),
            "内联字面量字节上限现在**一处都没有**了。\n\
             多半是那处已改成具名常量 —— **那是好事**：它会自然进本表的主扫描面。\n\
             ⇒ 请**删掉本条**（它的全部意义是盯着范围外那一族），并确认新常量已在主表登记。"
        );
        assert_eq!(
            found.len(),
            1,
            "内联字面量字节上限**变多了**（实得 {found:?}）。\n\n\
             ⚠ 本表的扫描面**只认具名常量**，这一族在范围外。人群是 1 时不建扫描器（那是仪式）；\n\
             变成 2 就说明它在长大 —— 请重做那个判断：要么把这一族也纳入扫描面，\n\
             要么把新增那处改成具名常量（那样它自然进表）。"
        );
        assert!(
            found[0].contains("cc_bus.rs"),
            "那唯一一处不再是 `cc_bus.rs` 了（现在是 {}）—— 换了地方就换了语境，\n\
             请重新判断它的超限语义是不是仍然「读到上限就停」这种无害形态。",
            found[0]
        );
    }
}
