//! **字节上限的登记表**〔audit-0805 F06 下半，报告 B-5〕。
//!
//! # 计数被改了两次，两次都还不够
//!
//! | 谁 | 说几处 |
//! |---|---|
//! | 报告 B-5 | **3** 处 |
//! | 核实台账（V1）| **6** 处（`accounts_query` ×3 · `fork_write` · `remote_history` · `ssh_source`）|
//! | 本轮系统扫描 | **14** 处 |
//! | 〔devbench F10b，08-10〕| **26** 处 —— 多出来的 12 里有 7 处是把**内联字面量**提成具名的
//!   （那一族此前整个在扫描面外，而盯着它的那条「前提触发器」一直在假绿，详见本文件末尾），
//!   另有 `DAEMON_FRAME_LINE_CAP` 是**压根没有上限**的那处补上的。
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
        // ⚠ 第八种，同样**论证后**加〔devbench F10b〕：daemon 出方向单行超限时
        // **丢掉那一行**，并往 `REMOTE_HEALTH` 发一条带 origin 的说明。
        //
        // ★ 它与「跳过+说清」的分界是**谁被告知**：那一档告的是**日志**（`warn!`），
        // 而这一档告的是**用户**（前端能看见的类型化事件）。为什么必须多这一档：
        // 帧读跑在一个后台 task 里，**没有调用方可以回错**（回 `Err` 会被主循环当成
        // 致命错误去重连，而超长行只是这一行坏了）；而只写日志的话用户看到的是
        // 「这条会话少了一行」且无从得知为什么。⇒ 两者都不够，需要一个新名字。
        "丢弃+带身份报告",
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
        // ── 〔devbench F10b〕以下七条此前**全都不在本表的扫描面里**。
        // 前六条是**内联字面量**（`.take(32 * 1024 * 1024)` 这种），本表头注把那一族
        // 划在范围外、交给一条「前提触发器」盯着别长大 —— 而那条触发器**一直在假绿**
        // （它抠 `.take(` 之后的连续数字再要求紧跟 `.read_to_end`，
        // `32 * 1024 * 1024` 抠出 `"32"`、后面是 `" * 1024"` ⇒ 形态不匹配 ⇒ 不计数）。
        // ⇒ F10b 把六处提成具名常量（于是自然进主扫描面），并把触发器改成按事实取样。
        //
        // ★ 更要紧的是：它们的超限语义此前**全是「静默截断」** —— `.take(N).read_to_end()`
        // 读满就停、缓冲区里是半份数据，而调用方拿它当完整的用。那正是本表这个封闭集合
        // **刻意排除**的那一种。⇒ 六处一律改成「多读一个字节 + 超了就回错」
        // （形态照抄 `remote-daemon-proto/src/common/fs.rs` 那条既有注释）。
        (
            "src-tauri/src/cc_bus.rs",
            "CC_BUS_TSV_CAP",
            32 * 1024 * 1024,
            "读远端 cc-bus 的两份登记表（`agents.tsv` + `spawned.tsv`）",
            "拒收+回错",
        ),
        (
            "src-tauri/src/cc_bus.rs",
            "ONLINE_PROBE_CAP",
            4096,
            "查单个 agent 是否在线的输出（预期 ~7 字节）",
            "拒收+回错",
        ),
        (
            "src-tauri/src/cc_bus.rs",
            "INBOX_READ_CAP",
            4 * 1024 * 1024,
            "读某个 agent 的 inbox",
            // ★〔G 审计改档〕原登记「拒收+回错」，那是**行为回归**：命令是 `tail -n 200`，
            // 而 `parse_inbox_jsonl` 的契约逐字是「坏行跳过并计数，不因坏行丢好行」——
            // 旧的截断行为下末行被跳过、前 199 条照常显示；改成回错之后**一条都不显示**。
            // 这是唯一一处调用方**明确**依赖宽容降级的地方。
            "截断+说清",
        ),
        (
            "src-tauri/src/cc_bus.rs",
            "CONTROL_REPLY_CAP",
            64 * 1024,
            "发消息 / spawn 的回显（是一句确认，不是数据）",
            // ★★〔G 审计改档〕原登记「拒收+回错」，那是**危险的行为回归**：
            // 这两条命令**有副作用**（agent 已经起来了 / 消息已经投递了），
            // 此时因回显太长回 Err，用户看到「失败」会重试 ⇒ **起两个 agent 在真烧额度**。
            // ⇒ 分界不是「哪个更严格」，是**截断有没有毒**：回显的截断从来不影响副作用。
            "截断+说清",
        ),
        (
            "src-tauri/src/mcp.rs",
            "REMOTE_CLAUDE_JSON_CAP",
            32 * 1024 * 1024,
            "读远端 `.claude.json`",
            "拒收+回错",
        ),
        (
            "src-tauri/src/hooks_diag.rs",
            "REMOTE_SETTINGS_CAP",
            4 * 1024 * 1024,
            "读远端 `settings.json`",
            "拒收+回错",
        ),
        // 第七条不是内联字面量，是**压根没有上限**：daemon 出方向单行此前走无界 `read_line`。
        // ⚠ 它的数**刻意不等于** daemon 侧的 `MAX_LINE_BYTES`（1 MiB，入方向命令信封）——
        // 实测本机 525,132 行 jsonl 里有 78 行超过 1 MiB、最长 2.97 MiB，
        // 抄过去就是丢真实数据。理由全文在 `ssh_source.rs::DAEMON_FRAME_LINE_CAP` 头注。
        (
            "src-tauri/src/ssh_source.rs",
            "DAEMON_FRAME_LINE_CAP",
            64 * 1024 * 1024,
            "daemon **出方向单行**（一帧 = 一条 Claude jsonl 行）",
            "丢弃+带身份报告",
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
            "remote-daemon-proto/src/control/cli_control.rs",
            "MAX_CLI_STDIN",
            1 << 20,
            "控制面 CLI 子命令（`--launch`/`--kill`/…）的 stdin args JSON",
            "拒收+回错",
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
            "全仓只扫到 {} 个字节上限常量（08-10 实测 26；08-06 那次是 14，F10b 把七处内联的提成了具名）—— 抽取器坏了",
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

    /// 内联读上限里**实参不是具名常量**的那些。
    /// `(相对仓根的路径, `take` 的实参逐字, 为什么它不必进 `CAPS`)`
    ///
    /// 只有一种正当情况：上限是**参数**，真值由调用方给（而调用方给的是具名常量）。
    const PARAMETRIC_READ_CAPS: &[(&str, &str, &str)] = &[
        (
            "src-tauri/src/cc_bus.rs",
            "cap + 1",
            "`exec_read` 是三条命令共用的助手，上限是入参。三个调用点给的都是具名常量\
             （`INBOX_READ_CAP` / `CONTROL_REPLY_CAP` ×2），那三个已在 `CAPS` 里。",
        ),
        (
            "remote-daemon-proto/src/common/fs.rs",
            "cap + 1",
            "`read_file_capped` 是 daemon 侧共用的有界读助手，上限是入参；\
             调用方给的是 `MAX_CLAUDE_JSON_BYTES` / `MAX_MANIFEST_BYTES` 等具名常量。",
        ),
    ];

    /// 异步流整读里**压根没有上限**的那些。
    /// `(相对仓根的路径:行, 读的是谁的输出, 为什么今天不加 + 谁退役它)`
    ///
    /// ⚠ **这不是豁免清单**：第三格必须写「退役归」（下面 `every_uncapped_stream_read_has_an_owner`
    /// 钉着），照 `polling_registry` 的先例。
    const UNCAPPED_STREAM_READS: &[(&str, &str, &str)] = &[
        (
            "src-tauri/src/account_usage.rs",
            "远端 `ccm` 用量探针的 stdout",
            "远端 SSH exec 输出，无 e2e 覆盖 ⇒ 改完无法验。正确修法是抽一个 \
             `exec_read_capped` 共享助手而不是撒八个 `.take()`，那是重构。**退役归 F10d**。",
        ),
        (
            "src-tauri/src/acct_iso_deploy.rs",
            "远端 `cc-acct-iso` 部署脚本的 stdout",
            "同上一条。**退役归 F10d**。",
        ),
        (
            "src-tauri/src/ccm_probe.rs",
            "远端 `ccm` 探针的 stdout",
            "同上一条。**退役归 F10d**。",
        ),
        (
            "src-tauri/src/sftp.rs",
            "远端 `uname -m` 架构探针的 stdout（`probe_remote_arch`）",
            "★〔G 审计扩针后才进人群〕同上一条族。预期输出 ~8 字节，但**没有上限**。\
             ⚠ 它此前不在人群里，因为第一版的针只认 `.read_to_end` / `.read_to_string`，\
             而它用的是 `.read_line` ——**判据的人群恰好排除了触发本件立项的那种拼法**。\
             ⚠ 注意 `sftp.rs` 同时也在 `REMOTE_WRITES` 里，两张表管的是它的两个不同面\
             （这张管「读进内存多少」，那张管「往谁的机器写」）。**退役归 F10d**。",
        ),
        (
            "src-tauri/src/pubkey.rs",
            "远端读公钥的 stdout",
            "同上一条。**退役归 F10d**。",
        ),
        (
            "src-tauri/src/tmux.rs",
            "远端 `tmux ls` / `capture-pane` 等四处的 stdout",
            "同上一条；四处同一族同一文件，故按文件登记（照 `polling_registry` 的口径）。\
             **退役归 F10d**。",
        ),
    ];

    /// ★ **前提触发器的重写**〔devbench F10b〕。
    ///
    /// # 它替掉的那条判据**一直在假绿**
    ///
    /// 旧条叫 `inline_literal_byte_caps_are_still_just_the_one`，自述逐字：
    /// 「本表的扫描面只认具名常量…范围要成立，得有个东西盯着『范围外那一族别长大』…
    /// 实测人群 **1**…人群变 2 的那天这个判断就该重做 —— **本条就是那个闹钟**」。
    ///
    /// 它抠 `.take(` 之后的 `is_ascii_digit()` 连续段，再要求**紧接着**是 `.read_to_end`。
    /// 于是 `.take(32 * 1024 * 1024)` 抠出 `"32"`、后面是 `" * 1024 …"` ⇒ 形态不匹配 ⇒ **不计数**。
    /// 实测那一族当时已有 **6 处**，它只看得见 1 处。**闹钟响过五次，一次都没听见。**
    ///
    /// ★ 病根与本仓反复记的那一条同族：**它按「一种拼法」取样，而事实是「有没有上限」**。
    /// 而它自己的注释里逐字写过这个失效模式（「匹配单位（一种拼法）比事实小」）——
    /// 写下那句话的判据，自己犯了那句话说的错。
    ///
    /// # 重写后的取样：按**事实**
    ///
    /// 人群 = 生产段里每一处「`.take(<任意实参>)` 紧邻一个字节读」。实参怎么拼不影响入群。
    /// 入群之后**默认拒绝**：实参要么是已登记的具名常量（`CAPS` 管它），
    /// 要么在 [`PARAMETRIC_READ_CAPS`] 里说清为什么不必进表。
    #[test]
    fn every_inline_read_cap_resolves_to_something_registered() {
        let sites = inline_read_cap_sites();
        // 自检：抠不到东西时下面的默认拒绝是空转的。
        assert!(
            sites.len() >= 6,
            "只扫到 {} 处内联读上限（08-10 实测 7：cc-bus ×3 · mcp · hooks_diag · daemon 的 fs 助手 · `--resolve` stdin）—— 抽取器坏了，本条此刻是空转的。\n\
             ⚠ 旧版就是**在这个位置**假绿了五次：它数的是「裸十进制字面量」而不是「有没有上限」。",
            sites.len()
        );
        let mut unresolved = Vec::new();
        for (file, arg) in &sites {
            // 剥掉 `+ 1`（「多读一个字节好分辨刚好读满与其实还有」那个惯用形态）。
            let bare = arg.split('+').next().unwrap_or(arg).trim().to_string();
            let is_named_const = !bare.is_empty()
                && bare
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                && bare.chars().next().is_some_and(|c| c.is_ascii_uppercase());
            if is_named_const {
                if CAPS.iter().any(|(_, n, ..)| *n == bare) {
                    continue;
                }
                unresolved.push(format!(
                    "  {file}: .take({arg}) —— `{bare}` 是具名常量但**不在 CAPS 里**"
                ));
                continue;
            }
            if PARAMETRIC_READ_CAPS
                .iter()
                .any(|(f, a, _)| f == file && a == arg)
            {
                continue;
            }
            unresolved.push(format!(
                "  {file}: .take({arg}) —— 实参不是具名常量，也没登记"
            ));
        }
        assert!(
            unresolved.is_empty(),
            "这些内联读上限没法追溯到任何登记：\n{}\n\n\
             ★ **裸字面量上限是本表五次假绿的病灶** —— 它不在主扫描面里，\n\
             所以「限什么量 / 超限怎么办」两问都没人回答，而实测那六处的答案全是\n\
             **静默截断**（`.take(N).read_to_end()` 读满就停、半份数据被当完整的用），\n\
             那正是 `ALLOWED_SEMANTICS` 刻意排除的那一种。\n\
             两条路：① 提成具名常量（那样它自然进主表，两问必须回答）；\n\
             ② 若上限真是**入参**，登记进 `PARAMETRIC_READ_CAPS` 并写清调用方给的是哪些具名常量。",
            unresolved.join("\n")
        );
        // 反向：登记不许留死行。
        for (file, arg, _) in PARAMETRIC_READ_CAPS {
            assert!(
                sites.iter().any(|(f, a)| f == file && a == arg),
                "`PARAMETRIC_READ_CAPS` 里的 `{file}: .take({arg})` 已经不在源码里了 —— \
                 删掉这条，别留死规则"
            );
        }
    }

    /// 抠出生产段里所有「`.take(<实参>)` 紧邻一个字节读」的位置。
    ///
    /// 返回 `(相对仓根的路径, 实参逐字)`。
    ///
    /// ⚠ 两个坑是旧版注释里逐字记过的，这里照样避开：
    /// ① **不能按字节切**（中文注释里会切在多字节字符中间直接 panic）⇒ 走 `Vec<char>`；
    /// ② **不能用宽窗口**（会把邻近另一行的读算进来，旧版实测抓出三个假阳）⇒ 要求紧邻。
    /// ⚠ 但**不再按拼法取实参** —— 走括号配平，`32 * 1024 * 1024` / `cap + 1` / `FOO` 一视同仁。
    fn inline_read_cap_sites() -> Vec<(String, String)> {
        const READS: &[&str] = &[".read_to_end", ".read_exact", ".read_to_string"];
        let root = repo_root();
        let mut out = Vec::new();
        for sub in ["src-tauri/src", "remote-daemon-proto/src"] {
            for (path, src) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
                let prod = guard_core::production_code(&src);
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let cs: Vec<char> = prod.chars().collect();
                let needle: Vec<char> = ".take(".chars().collect();
                let mut i = 0usize;
                while i + needle.len() <= cs.len() {
                    if cs[i..i + needle.len()] != needle[..] {
                        i += 1;
                        continue;
                    }
                    // 括号配平取实参。
                    let mut j = i + needle.len();
                    let mut depth = 1usize;
                    let mut arg = String::new();
                    while j < cs.len() && depth > 0 {
                        match cs[j] {
                            '(' => {
                                depth += 1;
                                arg.push('(');
                            }
                            ')' => {
                                depth -= 1;
                                if depth > 0 {
                                    arg.push(')');
                                }
                            }
                            c => arg.push(c),
                        }
                        j += 1;
                    }
                    // 紧邻：跳过空白之后必须直接是一个字节读。
                    let mut k = j;
                    while k < cs.len() && cs[k].is_whitespace() {
                        k += 1;
                    }
                    let tail: String = cs[k..(k + 16).min(cs.len())].iter().collect();
                    if READS.iter().any(|r| tail.starts_with(r)) {
                        out.push((rel.clone(), arg.trim().to_string()));
                    }
                    i = j.max(i + 1);
                }
            }
        }
        out.sort();
        out
    }

    /// ★ **默认拒绝：异步流整读要么有上限，要么有主人**〔devbench F10b〕。
    ///
    /// # 它补的洞
    ///
    /// 本表此前登记的是「**哪里有上限**」。而「**哪里该有却没有**」在任何表里都不存在 ——
    /// 与 `tool_registry` 那次（`NOT_MANAGED` 反向表）同一个形状：
    /// **一个东西不在表里，有「没人想起来」与「不属这张表」两种截然不同的原因，
    /// 而没有任何地方记着这个区分。**
    ///
    /// 人群取「把一整条**流**读进内存」这个事实：`.read_to_end` / `.read_to_string`
    /// 且紧邻处有 `.await`（同步的那些是 `std::fs::read_to_string(path)` 自由函数，
    /// 读的是**本机文件**、体量由磁盘兜着，不同族）。
    ///
    /// ⚠ 失效模式如实登记：① 先 `let fut = …;` 再 `await` 就漏出人群；
    /// ② 「附近有 `.take(`」是窗口启发式 —— 隔太远的真上限会假红、邻行的无关 `take` 会假绿。
    #[test]
    fn every_uncapped_stream_read_has_an_owner() {
        let mut population = 0usize;
        let mut orphans = Vec::new();
        let root = repo_root();
        // ★★〔G 审计逮到的〕**第一版这里只有 `read_to_end` / `read_to_string`。**
        //
        // 而 F10b 修掉的那三处，原来的写法是 `reader.read_line(&mut buf)` ——
        // 也就是说：**这条判据的人群，恰好排除了触发它立项的那一种拼法。**
        // 头注把「把一整条流读进内存」（事实）与「`.read_to_end`」（拼法）写成了等号，
        // 而本文件上方刚花十几行论证过同一个病根。**同一个 commit 里，同一句话又犯了一次。**
        //
        // 漏出来的是活的：`remote_history.rs::run_list_query` 无界 `read_line`
        // （只有外层 30s 超时兜着），三个 `#[tauri::command]` 调用方，生产路径；
        // 以及 `stream_read_remote_session` —— 它有 `MAX_SESSION_BYTES` 总量，
        // 但那是**读完再判**，一条超大行在 `read_line` 返回前就把内存吃光了。
        // 两处都已改走 `ssh_source::read_capped_line`。
        //
        // ⇒ 针按**读的动作**取，不按某一个方法名取。
        let reads: Vec<String> = ["to_end", "to_string", "line", "until"]
            .iter()
            .map(|m| format!(".read_{m}("))
            .collect();
        for sub in ["src-tauri/src", "remote-daemon-proto/src"] {
            for (path, src) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
                let prod = guard_core::production_code(&src);
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let lines: Vec<&str> = prod.lines().collect();
                for (i, l) in lines.iter().enumerate() {
                    if !reads.iter().any(|r| l.contains(r.as_str())) {
                        continue;
                    }
                    let lo = i.saturating_sub(4);
                    let hi = (i + 3).min(lines.len());
                    let window = lines[lo..hi].join("\n");
                    if !window.contains(".await") {
                        continue; // 同步读本机文件，不同族
                    }
                    population += 1;
                    if window.contains(".take(") {
                        continue;
                    }
                    if UNCAPPED_STREAM_READS.iter().any(|(f, ..)| *f == rel) {
                        continue;
                    }
                    orphans.push(format!("  {rel}:{}", i + 1));
                }
            }
        }
        assert!(
            population >= 14,
            "只扫到 {population} 处异步流读（08-10 G 审计后实测 18）—— 抽取器坏了，本条此刻是空转的"
        );
        assert!(
            orphans.is_empty(),
            "这些地方把一整条**流**读进内存，既没有上限也没有主人：\n{}\n\n\
             ★ 对端是**远端进程** —— 它坏掉、或者压根不是我们的 daemon，都会让\n\
             「无界读」变成「无界堆分配」。daemon 侧为此栽过一次实测：\n\
             喂 512 MiB 无换行的流 ⇒ RSS 从 6 MiB 涨到 518 MiB\n\
             （见 `remote-daemon-proto/src/inbound.rs` 头注）。\n\
             两条路：① 加上限（`.take(CAP + 1)` + 超了回错，形态见 `common/fs.rs`）；\n\
             ② 登记进 `UNCAPPED_STREAM_READS` 并写明**谁退役它**。",
            orphans.join("\n")
        );
        // 反向：登记不许留死行。
        for (f, ..) in UNCAPPED_STREAM_READS {
            assert!(
                root.join(f).is_file(),
                "`UNCAPPED_STREAM_READS` 里的 `{f}` 已经不在了 —— 删掉这条"
            );
        }
    }

    /// ★ **「丢弃+带身份报告」那一档的行为对拍**〔devbench F10b〕。
    ///
    /// # 为什么它需要一条**专属**判据
    ///
    /// 隔壁 `a_cap_registered_as_hard_error_is_not_swallowed_at_its_call_site` 是
    /// 「从常量被提到的那一行往下看 N 行找 marker」。那个形状对本档**不成立**：
    /// `DAEMON_FRAME_LINE_CAP` 在三个地方被提到（有界读的判断处 · 措辞函数 · 消费点的
    /// `warn!`），而报告只发生在**第三处** —— 按每处提及去要求 marker 会造出两条假红。
    ///
    /// ⇒ 换个取样单位：**处置分支本身**。人群 = `ssh_source.rs` 生产段里每一处
    /// `CappedLine::TooLong` 的处置臂。要求：
    /// ① 每一臂都得说点什么（至少 `warn!`）—— 定框 **E4**：静默失败要给身份；
    /// ② **至少有一臂**把它抬到用户能看见的那一层（`REMOTE_HEALTH`）——
    ///    那正是本档与「跳过+说清」的分界（告日志 vs 告用户）。
    ///
    /// ⚠ 失效模式如实登记：臂体窗口是 **12 行**的启发式。臂特别长 ⇒ 假红；
    /// 紧邻的别的语句里恰好有 marker ⇒ 假绿。与隔壁那条同源取舍。
    #[test]
    fn the_drop_and_report_semantics_is_honoured_at_every_over_limit_arm() {
        let root = repo_root();
        let raw = std::fs::read_to_string(root.join("src-tauri/src/ssh_source.rs"))
            .expect("ssh_source.rs 读不到 —— 文件搬了就把这条一起改");
        let prod = guard_core::production_code(&raw);
        let lines: Vec<&str> = prod.lines().collect();
        let mut arms = 0usize;
        let mut silent = Vec::new();
        let mut reported = 0usize;
        for (i, l) in lines.iter().enumerate() {
            // ⚠ **必须同时要求 `=>`**〔本条首跑就红在这里〕：`CappedLine::TooLong` 既是
            // **构造**（有界读函数里 `return Ok(… CappedLine::TooLong(seen) …)`）也是
            // **模式**（处置臂 `Ok(CappedLine::TooLong(bytes)) => {`）。
            // 第一版只按名字取样，于是把有界读里那两处构造当成了「什么都没说的处置臂」——
            // **匹配单位（名字出现）比事实（这是一处处置）大**，本仓那一族的又一次。
            if !l.contains("CappedLine::TooLong") || !l.contains("=>") {
                continue;
            }
            arms += 1;
            const WINDOW: usize = 12;
            let body = lines[i..(i + WINDOW).min(lines.len())].join("\n");
            if !body.contains("warn!") {
                silent.push(format!("  ssh_source.rs:{}", i + 1));
            }
            if body.contains("REMOTE_HEALTH") {
                reported += 1;
            }
        }
        assert!(
            arms >= 3,
            "只找到 {arms} 处超限处置臂（08-10 实测 3：主帧读 / 握手 / 应答泵）—— \
             抽取器坏了，本条此刻是空转的"
        );
        assert!(
            silent.is_empty(),
            "这些超限处置臂什么都没说：\n{}\n\n\
             ★ 丢一行**不说**就是静默失败，而 `ALLOWED_SEMANTICS` 刻意排除了那一种。\n\
             用户看到的会是「这条会话少了一行」且无从得知为什么。",
            silent.join("\n")
        );
        assert!(
            reported >= 1,
            "没有任何一处超限把话说到**用户**那一层（`REMOTE_HEALTH`）。\n\
             ★ 那是「丢弃+带身份报告」与「跳过+说清」的**唯一分界** —— \n\
             只写 `warn!` 的话本档就该改登记成「跳过+说清」，别占一个更强的名字。"
        );
    }

    /// ★ 登记「无上限」不许变成永久豁免：每条必须写**谁退役它**。
    ///
    /// 钉法照 `polling_registry` 的先例（那张表对 `data-poll` 也要求逐字「退役归」）。
    #[test]
    fn an_uncapped_read_is_not_a_permanent_exemption() {
        assert!(
            !UNCAPPED_STREAM_READS.is_empty(),
            "登记表空了 —— 若那八处真都加上上限了，请**删掉这条判据与那张表**"
        );
        for (f, what, why) in UNCAPPED_STREAM_READS {
            assert!(
                why.contains("退役归"),
                "`{f}` 登记成「无上限」却没说**谁退役它** —— \
                 没有主人的登记就是豁免清单，而豁免清单会一直在那里（{what}）"
            );
            assert!(
                what.chars().count() > 5,
                "`{f}` 没说清读的是**谁的输出**：「{what}」"
            );
        }
    }
}
