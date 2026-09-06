//! ★ **每一个会写用户机器的落点都要申报**〔audit-0805 08-07，Phase G 第 38 件〕。
//!
//! # 它关掉的是 `ROADMAP §5 4b` 那条诚实边界的一半
//!
//! 4b 逐字写着：`tool_registry` 的 15 条判据全是**声明表内部的自洽**，
//! **人群是「声明了的工具」，不是「真实发生的安装动作」** ——
//! 「今天表里 6 条看着全，那是**人现在记得**，不是有东西钉着」。
//!
//! 它当初还写明了为什么不做机检：试过一条口径——**扫生产段的安装落点字面量**，
//! 实测 21 个命中里绝大多数是用户可见文案与探测命令，真落点只有两三个
//! ⇒ 噪声压过信号。那个判断当时是对的。
//!
//! ## 但那是**错的人群**
//!
//! 「路径字面量」是按**怎么写的**取样（一个字符串长得像不像路径），
//! 而要钉的事实是**做了什么**（这段代码有没有往用户机器上写东西）。
//! 换成后者之后人群当场干净：08-07 实测**生产段写盘调用 36 处、分布在 9 个文件**，
//! 每一处都机器可判、零文案噪声。
//!
//! ⇒ 这正是本工作区反复收敛出的那条：**判据锚在「怎么写」上就会又漏又吵；
//! 换成「做了什么」之后，原本被判为「做不了」的机检往往当场可行。**
//! ⚠ 教训的另一半：**「刻意不做」也会过期**。4b 的解锁条件写的是
//! 「安装动作先收敛到一个可枚举的落点」——今天回头量，它**早就成立了**，
//! 只是没人回来重量一次。
//!
//! # 它守什么、不守什么
//!
//! **守**：新增一个写盘落点（新文件、新函数）而不申报 ⇒ 当场红。
//! 申报为「安装动作」的必须点名 `tool_registry::TOOLS` 里**真实存在**的 `id`，
//! 于是「真实动作 ↔ 声明表」这条连线第一次有东西钉着。
//!
//! **不守**：① 一个已申报函数**内部**多写一个文件（人群键是「文件::函数」，
//! 不是逐次调用）——那要判语义，且同一个函数里多一次 `create_dir_all` 是常态；
//! ② 写的**内容**对不对（那是各模块自己的行为判据）；
//! ③ 通过 `Command` 起外部进程间接写盘（本条只看 Rust 侧的 `fs::` 调用面）。
//! ⚠ ③ 是真缺口，不是措辞：`ccm` 的部署有一部分走 shell。已进 `ROADMAP §5`。

/// ★ **「谁在写」的唯一权威源**〔audit-0805 08-07〕，供只读模块的守卫复用。
///
/// 只读模块（`config_surface` / `hooks_diag`）的守卫扫的是 `fs::` 前缀 ——
/// 那只认「自己写」，认不出**把写交给别人**：08-07 实测，两个模块各加一句
/// `crate::utils::atomic_write_json(p, v)`，**全仓 982 条判据一条不红**，
/// 而它们的判据名逐字写着 `only_reads` / `never_writes`。
///
/// ⇒ 与其在每个只读模块里各写一张「已知写者」清单（那是下一个漂移源），
/// 不如让它们都问同一张表：**本模块的 `WRITE_SITES` 就是那张表**。
/// ★ **本机起进程的落点也要申报**〔audit-0805 08-08，Phase G 第 52 件〕。
///
/// 写盘那一侧（本模块正题）与远端执行那一侧（`exec_site_registry`）都已经有人数了，
/// **本机 `Command::new` 这一侧没有** —— 08-08 实测：往 `session_map.rs` 加一句
/// `Command::new("sh").arg("-c").arg(arg)`，**全仓 983 条判据一条不红**。
///
/// ⚠ 先核查出关键一半：**daemon 侧早就有这张表**（`readonly_guard` 的 `ALLOWED` +
/// `SPAWN_SITES_TODAY`），monitor 侧从来没有。又是「同一形态在另一半原样存在」，
/// 只是这次缺的是 monitor（第 34、44 件是反过来）。
///
/// 两半各有一张表**不违反 E3**：「谁在本半起进程」本来就是两个事实，
/// 各自的权威源在各自那一半。这里只对齐**形状**，不共享清单。
#[cfg(test)]
mod spawn_sites {
    use std::path::{Path, PathBuf};

    /// `(文件, 函数, 起的是什么, 为什么必须起进程)`。**默认拒绝**：人群从源码派生。
    const SPAWNS: &[(&str, &str, &str, &str)] = &[
        // ── 构建期（`build.rs`）：**每次 `cargo build`／`cargo check` 都在开发者机器上真跑**。
        // 08-08 并进本表之前，它整个在所有登记表的扫描面之外。
        ("build.rs", "check_vendor_freshness", "`git`（读 vendor 目录的最后一次改动）",
         "vendor 新鲜度自检：只读地问 git，参数是仓内固定路径、不吃用户输入。\
          它必须起进程是因为「vendor 目录相对上游有没有漂」这件事只有 git 知道"),
        ("build.rs", "check_acct_iso_vendor_freshness", "`sh -c`（算 vendored 脚本的指纹）",
         "同上的第二半，对 `cc-acct-iso` 那份 vendor 算摘要；命令串是常量，\
          唯一的变量是仓内路径。⚠ 它跑在**构建期**，比运行时的任何一处都早"),
        ("ccm_probe.rs", "probe_with", "`bash -lic <常量探测串>`",
         "P3t-Y2：本机 ccm 的**能力集**探测。命令串是 `CCM_PROBE_CMD` —— 与远端那条**逐字同一个常量**，\
          零插值。必须起进程的理由是「本机装没装 ccm、装的是哪一版」只有这台机器自己知道；\
          而 shell 里的 `command -v` 那种探法只答得了「在不在」，答不了能力集，\
          拿它当依据渲染就会在老 ccm 上渲出带未知 flag 的命令且**已经没有回落可走**（fail-open）。\
          `bash -lic` 那层与远端同语义（PATH/别名/函数按交互终端解析），`ccm` 正是靠它才被找到。\
          ⚠ **命令是参数**（D 阶段补审为了能测「挂住」而开）——生产侧唯一实参是 `CCM_PROBE_CMD`，\
          由 `the_only_production_probe_command_is_the_constant` 按源码钉住，别读成「这里能跑任意命令」"),
        ("cc_bus.rs", "local_shell_read", "`bash -lc <cc-bus 的读串>`",
         "P4a-Y1：本机 cc-bus 的**读**面（清单 / 在线 / inbox）。必须起进程的理由是\
          **不许有第二份文件布局知识** —— `CC_BUS_CAT_CMD` 逐字知道 `~/.cc-bus/agents.tsv` 长什么样，\
          本机若自己 `read_to_string` 那两个文件，仓里就有了同一件事的两种表示，而它们会各自漂。\
          ⇒ 照 `P3t-Y2` 的先例：**同一条串，远端包进 ssh，本机交给 bash**（`C1` 逐字「只是远端走 ssh」）。\
          ⚠ **命令是参数**，但生产侧的三个实参各有来历：`CC_BUS_CAT_CMD`（常量）· `build_online_cmd` · \
          `build_inbox_cmd`（两个构造器都过 `is_valid_bus_id`），由 `exec_site_registry` 那条按源码钉住。\
          用 `-lc` 而不是 `-lic`：只要 `$HOME`/`$CC_BUS_HOME`，不需要交互式 rc"),
        ("ssh_source.rs", "spawn_dial_proxy", "`<代理二进制> --dial`（子进程，常驻到某一头断开）",
         "`K-P6b`：**daemon 那条长连接流的 SSH 握手交给这个子进程去跑**，界面只收字节。\
          起的是什么：`cc-monitor-remote`（本仓 `remote-daemon-proto` 的产物）——\
          发版包里它就在 `monitor.exe` 旁边（`externalBin` sidecar），\
          解析口 `resolve_dial_proxy` 只认两处：环境变量 `CCM_DIAL_PROXY` 与 exe 旁那份。\
          **argv 只有一个常量 flag，零插值**；主机名 / 用户名 / 私钥**路径**走环境变量 \
          `CCM_DIAL_REQUEST`（`argv` 是世界可读的，`/proc/<pid>/environ` 不是）。\
          为什么必须起进程：这正是本件的**目的** —— 让那一跳拨号不发生在界面进程的地址空间里。\
          ⚠ 它 `kill_on_drop(true)`：界面退出 = 句柄 drop = 代理跟着走。\
          🔴 **别把这一行读成「拨号搬出去了」**：`connect_session` 的 7 处生产调用点里\
          这条只覆盖 1 处，逐处登记在 `ssh_source::dial_move_judge::DIAL_SITES`"),
        ("account_usage.rs", "run_local_probe", "`sh -c <载荷>`",
         "本机用量探针：载荷由 `probe_command_for` 构造并引用过（`exec_site_registry` 里那条 Builder 行管它）"),
        ("launch.rs", "launch_local_posix_via", "用户配置的终端 argv[0]",
         "在用户的终端里起会话 —— 承接 C13「最后那次 exec 在用户终端里」，这是本产品的主用途"),
        ("launch.rs", "launch_powershell_window", "`wt.exe` / `powershell.exe`",
         "Windows 侧同上；两个名字都是常量，不吃用户输入"),
        ("launch.rs", "ssh_client_available", "探测用的 `ssh`",
         "只探测「本机有没有 ssh」，不带用户参数"),
        ("lib.rs", "open_with_os", "`cmd` / `open` / `xdg-open`",
         "按平台打开日志目录：三个名字都是常量，路径是 monitor 自己的目录"),
        ("local_backend.rs", "supervise_with_stdio", "被监护的 daemon 二进制",
         "本机后端监护：二进制路径来自 `candidates`（有 `candidates_never_point_into_a_build_tree` 守着）。\
          ⚠ P2 起它的 stdin 可能是 `piped()` 而不再恒为 `null` —— 那是本机入方向通道的管子\
          （`local_stdio_consumer`）。`supervise` 只是它 `stdio=None` 的薄壳，真正 spawn 的是这一个"),
        ("local_query.rs", "run_query", "daemon 二进制 + 只读子命令",
         "本机只读查询：`bin` 同上来自候选表，`args` 是本模块构造的固定子命令"),
        ("ssh_source.rs", "resolve_ssh_host", "`ssh -G <host>`",
         "解析 ssh_config 的别名 —— 只读一次配置，不建连接"),
        // ── `K-P1`：常驻那条路 ──────────────────────────────────────────────
        ("local_daemon.rs", "spawn_detached", "被脱离起来的 daemon 二进制",
         "本机后端**脱离宿主**起：`process_group(0)` + stdio 全 null + 协议改走回环监听口。\
          二进制路径来自 `resolve_daemon_bin`（exe 旁的 sidecar 或释放出来的内嵌那份，\
          与 `local_backend::start_or_extract` 同一个顺序，由一条对拍判据钉着）。\
          ⚠ 它必须住在**宿主知识层**而不是 `backend/`：`process_group` 来自 \
          `std::os::unix::process::CommandExt`，而 `std::os::unix` 在 \
          `backend/mod.rs::the_backend_half_stays_platform_agnostic` 的禁针里 —— 写进去当场红，\
          而「加一条平台例外」被那张表的递减棘轮堵着（`PLATFORM_EXCEPTIONS.len() <= 1`，今天正好 1）"),
        ("local_daemon.rs", "signal_term", "`kill -TERM <pid>`",
         "停掉一个**不是本 monitor 起的**常驻实例（上一次 monitor 脱离起的那个）。\
          必须起进程的理由是：monitor 今天**没有 `libc` 这条直接依赖**（它只在依赖树里），\
          为一次「停」按钮加一条直接依赖是更大的代价。\
          ⚠ 参数是**我们自己算出来的 pid**、零用户输入；而且杀之前先过 `kill_adopted` 的身份核对\
          （`/proc/<pid>/exe` 必须是同一个二进制）—— pid 会被复用，杀错一个无关进程是不可逆的"),
    ];

    fn src_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// 语料 = `src/` 整棵树 **+ `build.rs`**〔audit-0805 08-08〕。
    ///
    /// ★ 为什么非把 `build.rs` 并进来：本表问的是「**谁能碰这台机器**」，
    /// 而构建脚本每次 `cargo build`／`cargo check` 都在开发者机器上真跑
    /// （它起 `sh` 与 `git`、往 `OUT_DIR` 复制内嵌 daemon）。
    /// 08-08 实测：全仓所有登记表/守卫的扫描根都是 `src-tauri/src` · `remote-daemon-proto/src`
    /// · `src-tauri/crates` · `src` · `doc` —— **`src-tauri/build.rs` 一张表都没扫到**，
    /// 它是这些扫描面共同的盲点（与 F65「三张表共享同一个没写下来的前提」同族）。
    fn corpus() -> Vec<(PathBuf, String)> {
        let mut files = guard_core::scan_tree!(&src_root(), &["rs"]);
        let bs = Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs");
        let src = std::fs::read_to_string(&bs).expect("读不到 build.rs —— 它是本表的一部分");
        files.push((bs, src));
        files
    }

    /// 某一行所在的函数名（往回找最近的 `fn`）。
    fn enclosing_fn(lines: &[&str], at: usize) -> String {
        for l in lines[..=at].iter().rev() {
            if let Some(rest) = l.split(" fn ").nth(1).or_else(|| l.strip_prefix("fn ")) {
                let n: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !n.is_empty() {
                    return n;
                }
            }
        }
        "<找不到外层函数>".to_string()
    }

    #[test]
    fn every_local_spawn_is_declared() {
        let files = corpus();
        let mut found: Vec<(String, String)> = Vec::new();
        for (path, src) in &files {
            let prod = guard_core::production_code(src);
            let lines: Vec<&str> = prod.lines().collect();
            let stem = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap()
                .to_string();
            for (i, l) in lines.iter().enumerate() {
                if l.contains(concat!("Command::", "new(")) {
                    found.push((stem.clone(), enclosing_fn(&lines, i)));
                }
            }
        }
        found.sort();
        found.dedup();
        assert!(
            found.len() >= 5,
            "全树只找到 {} 处本机起进程（08-08 实测 8 个「文件::函数」）—— 抽取器坏了，本条此刻无效",
            found.len()
        );

        let missing: Vec<String> = found
            .iter()
            .filter(|(f, n)| !SPAWNS.iter().any(|(sf, sn, _, _)| sf == f && sn == n))
            .map(|(f, n)| format!("  {f}::{n}"))
            .collect();
        assert!(
            missing.is_empty(),
            "这些地方**会在用户机器上起一个进程，但没人申报**：\n{}\n\n\
             ⚠ 08-08 实测：往生产段加一句 `Command::new(\"sh\").arg(\"-c\")`，全仓判据一条不红。\n\
             登记进 `SPAWNS`：写清**起的是什么**、**为什么必须起进程**。\n\
             daemon 侧同类表在 `readonly_guard`（`ALLOWED` + `SPAWN_SITES_TODAY`）。",
            missing.join("\n")
        );

        let stale: Vec<String> = SPAWNS
            .iter()
            .filter(|(f, n, _, _)| !found.iter().any(|(ff, nn)| ff == f && nn == n))
            .map(|(f, n, _, _)| format!("  {f}::{n}"))
            .collect();
        // ⚠ **红了还要讲对成因**〔08-08〕：死行有两种完全不同的来路 ——
        // ① 那处代码真的改名/删了；② **扫描面缩了**（语料不再包含那个文件）。
        // 实测把 `build.rs` 从语料里拿掉，旧诊断说「已经不起进程了（改名或删了）」，
        // 而 `build.rs` 里那两处一个字没动 —— 照它去查会查错方向。
        // 本区反复吃过这个亏（「讲错成因的红灯比不红更坏」），所以这里先分辨再说话。
        let scanned: Vec<String> = files
            .iter()
            .filter_map(|(p, _)| p.file_name().and_then(|s| s.to_str()).map(str::to_string))
            .collect();
        let out_of_corpus: Vec<&str> = SPAWNS
            .iter()
            .map(|(f, _, _, _)| *f)
            .filter(|f| !scanned.iter().any(|s| s == f))
            .collect();
        assert!(
            stale.is_empty(),
            "申报表里这些落点在语料里找不到了：\n{}\n\
             ★ 两种来路，先分清再动手：\n\
             ① 那处代码真的改名/删了 ⇒ 更新登记（改名也要红 —— 名字变了就该有人重新看一眼它起的是什么）；\n\
             ② **扫描面缩了** —— 这些文件压根不在本次语料里：{out_of_corpus:?}\n\
                （非空就说明是这一种：去看 `corpus()` 少扫了什么，别去改登记表）",
            stale.join("\n")
        );
    }
}

#[cfg(test)]
pub(crate) mod writers {
    /// 所有会写盘的函数名（去重）。**从 `WRITE_SITES` 派生**，不是手写。
    pub(crate) fn names() -> Vec<&'static str> {
        let mut v: Vec<&'static str> = super::tests::WRITE_SITES
            .iter()
            .map(|(_, f, _, _)| *f)
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// 一段生产代码里调到的写者。`self_file` 是调用方自己的文件名 ——
    /// 它自己的写点不算「委托」（那由 `WRITE_SITES` 直接管）。
    pub(crate) fn called_by(prod: &str, self_file: &str) -> Vec<&'static str> {
        super::tests::WRITE_SITES
            .iter()
            .filter(|(f, _, _, _)| *f != self_file)
            .map(|(_, fname, _, _)| *fname)
            .filter(|fname| prod.contains(&format!("{fname}(")))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// 写盘调用的**形态清单**。人群按「做了什么」取：这些是 Rust 侧真正落盘的动作。
    ///
    /// ⚠ 只读的 `fs::read*` / `metadata` / `exists` **不在其中** —— 本条钉的是「写」。
    const WRITE_CALLS: &[&str] = &[
        "fs::write(",
        "fs::create_dir_all(",
        "fs::create_dir(",
        "fs::copy(",
        "fs::remove_file(",
        "fs::remove_dir_all(",
        "fs::remove_dir(",
        // ⚠ 这一个**刻意不带左括号**。带上就成了 `::rename(`，而
        // `atomic_replace_registry` 正是按那个形态数「原子替换调用点」——
        // 本文件一加它当场红（实测）。往那张表里塞一条豁免是最省事的消法，
        // 但那会把它变成废纸；去掉括号后：我这边照样认得出真调用，
        // 而**哪天有人在本文件里真写了一次 `fs::rename(...)`，那张表仍会逮住它**。
        // 代价是这个 needle 稍宽（会命中 `fs::rename_xxx` 之类），今天全树无此形态。
        "fs::rename",
        "File::create(",
    ];

    /// 每个落点的申报：`(文件, 函数, 属于哪个已声明工具, 说法)`。
    ///
    /// 第三列 `Some(id)` = **这是那个工具的安装/卸载动作**，`id` 必须在
    /// `tool_registry::TOOLS` 里真实存在（下面有对拍）；`None` = **不是安装动作**，
    /// 第四列要写清它写的是什么。
    ///
    /// ⚠ **默认拒绝**：人群从源码派生，没在这张表里的落点当场红。
    /// 「谁进人群」由机器定，「它是不是安装动作」才是人的答案。
    #[allow(clippy::type_complexity)]
    pub(crate) const WRITE_SITES: &[(&str, &str, Option<&str>, &str)] = &[
        // ── P2z：单 exe 自释放内嵌 daemon。**不是安装动作** —— 它写的是 monitor 自己的缓存。
        ("local_backend.rs", "extract_embedded_to", None,
         "把内嵌的 daemon 二进制释放到 `~/.cc-monitor/bin/cc-monitor-local-<build_id>`，\
          供 `start_or_extract` 在 exe 旁没有 sidecar 时起进程。写的是 monitor 自己的目录，\
          不碰用户既有环境、不注册到任何用户配置里；按 build_id 命名 ⇒ 幂等、不覆盖别的版本"),
        // ── P2t：收掉自己留下的 `.partial` 残骸。**不是安装动作** —— 它只**删**，且只删自己那套命名。
        ("local_backend.rs", "sweep_stale_partials", None,
         "删 `~/.cc-monitor/bin/.<释放名>.<pid>.partial` 里**够老**（≥24h）的残骸 —— \
          那是 `extract_embedded_to` 崩在 rename 之前留下的自家垃圾。\
          ⚠ 三重收窄，因为这个目录**与远端自部署共用**：① 只认自己那套命名（`.<释放名>.<pid>.partial`）；\
          ② 只删够老的（正常释放是毫秒级的顺序写，24h 给了五个数量级余量 —— \
          宁可多留一天垃圾，也不要把**正在写的**那份删掉，那等于自己制造半截文件）；\
          ③ 删不掉就算了，**清扫失败绝不挡住释放**。\
          ★ 为什么会有残骸：临时名从固定名改成**带 pid**（防两个 monitor 写同一个 `.partial`）之后，\
          崩掉的那些不会再被下一次覆盖 ⇒ 得自己收。"),
        // ── PS1：把内嵌的 cc-bus 装到 `<claude_dir>/skills/cc-bus/`。**这是安装动作**。
        ("cc_bus_deploy.rs", "deploy_into", Some("cc-bus"),
         "写 `<claude_dir>/skills/cc-bus/` 的 17 个文件（内嵌自 `shared/cc-bus/`）。\
          ⚠ 这是本仓**第一处往 `<claude_dir>` 写的地方** —— 只读铁律原本禁它，\
          `U10b`〔用@08-13〕裁「开」之后写成 `INVARIANTS` 的**第 7 条例外**，四个配套一条不省：\
          ① **用户显式动作**（只由设置页按钮调，绝不在启动/后台路径上跑）；\
          ② **独立 realpath 白名单**（`fenced_dest`：canonicalize 后必须仍在 claude_dir 下，\
             挡「skills 是指向别处的软链」）；\
          ③ **幂等**（逐文件比内容，一致就一个字节都不写、也不留备份）；\
          ④ **可撤销**（覆盖前把旧目录整个 rename 成 `cc-bus.bak-<ts>`）。",
        ),
        ("cc_bus_deploy.rs", "fenced_dest", None,
         "`mkdir -p <claude_dir>/skills` —— 只为**建出围栏要归一的那一层**（`canonicalize` 需要\
          路径真实存在）。**不是安装动作**：它一个内容文件都不写；真正的安装在 `deploy_into`。\
          ⚠ 它同时是那道围栏本身：归一之后必须仍在 `claude_dir` 下，否则**拒收**。"),
        ("cc_bus_deploy.rs", "backup_existing", None,
         "把已装的 `cc-bus` 整个 `rename` 成 `cc-bus.bak-<ts>`。**不是安装动作**，是它的\
          **可撤销**那一格（`U10b` 第 7 条例外的四个配套之一）。\
          ⚠ 只在**内容确有变化**时才做：全一致时不备份，否则每点一次就多一份垃圾备份。"),
        // ── 构建期写盘：**不碰用户既有环境**，只往 `OUT_DIR` 放构建产物。
        // 单列在这里是因为它此前**整个在扫描面之外**（08-08 并入），
        // 而它确实在开发者机器上写文件 —— 「不是安装动作」得由人说出来，不是靠没人看见。
        ("build.rs", "embed_daemons", None,
         "把 `embedded-daemons/cc-monitor-remote-<arch>` 复制进 `OUT_DIR`，\
          供 `include_bytes!` 内嵌。写的是 cargo 自己的构建目录，不碰用户环境；\
          ⚠ 它读的那份清单由 `sftp.rs` 的身份见证判据守着（`id_from_manifest` 不许写死）"),
        // ── devbench F03：skill 接入面的收件箱写入。**不是安装动作**。
        ("skill_host.rs", "write_skill_file", None,
         "写用户**自己项目里**的 `.claude/planned-build/INBOX.txt`（planned-build skill 的\
          「结构化注入」进件口）。**不碰 Claude 的数据、不碰用户环境、不装任何东西。**\n\
          分界沿用 `doc/INVARIANTS.md:23` 那条 F47 澄清的口径：用户亲自驱动、\
          每次写都是面板内一次直接手势、写的不是 Claude 的 jsonl/pidfile。\n\
          三道围栏（`skill_host::resolve_editable`）：① 路径 `canonicalize` **之后**\
          做集合判定（集合来自声明表的 `editable`，不是一串 if）② 过\
          `sftp_pool::is_protected_claude_data_path` 纵深③ 目标必须**已存在**\
          （本功能是「编辑收件箱」不是「创建任意文件」）。\n\
          写本身走 `verified_write::verify_and_rollback`（备份 → 写 → 读回逐字节比对 →\
          不符即回滚），**没有自造第四份写入实现** —— 那个模块头注记着本仓曾有 4 处\
          独立实现且校验强度不一致（两处只比长度）。"),
        // ── 安装动作：写的是**用户既有的环境/配置**，且对应声明表里的一个工具
        ("profile_installer.rs", "install_to_profile", Some("ccm"),
         "往用户 shell profile 的 BEGIN/END 块里装 ccm 启动器（写前先备份）"),
        ("profile_installer.rs", "uninstall_from_profile", Some("ccm"),
         "从 profile 里摘掉那个块（同样先备份）"),
        ("profile_installer.rs", "atomic_write_string", Some("ccm"),
         "上面两个动作唯一的落盘漏斗：临时文件 + rename"),
        ("profile_installer.rs", "atomic_replace_path", Some("ccm"),
         "跨设备回退的 rename。⚠ 这是**四份平台原语副本之一**，四份都已登记在 `atomic_replace_registry`（承接 C10）——本条不重复判它，只记它是个写点"),
        ("mcp.rs", "write_json_atomic", Some("project-mcp"),
         "写项目级 MCP 服务器配置（`TOOLS` 里 `project-mcp` 那条的真落点）"),
        // ── 不是安装动作：写的是 monitor 自己的东西
        ("bind.rs", "spawn", None, "monitor 自己的运行时目录/落地文件"),
        ("bind.rs", "process_await_file", None, "monitor 自己的等待文件"),
        ("bind.rs", "cleanup_dead", None, "清理 monitor 自己留下的死文件"),
        ("config.rs", "save_config", None, "monitor 自己的配置文件"),
        // K-H2a：第三方 API key 那份文件。**monitor 自己的文件**（不是用户的、也不是某个工具的安装动作）。
        // ⚠ 它与人手编是同一份文件的两个写者 ⇒ 写的那一刻才读盘、未知键一个不吃、
        //   字段顺序按名字排、原子替换（复用 `config::atomic_replace`）、写完立刻收窄成只给本人。
        ("creds_store.rs", "write_key_at", None, "monitor 自己的凭据文件（中转那把第三方 API key）"),
        ("config.rs", "atomic_replace", None, "原子替换原语的本地副本（同上，归 `atomic_replace_registry` 判）"),
        ("lib.rs", "open_log_dir", None, "打开日志目录前确保它存在"),
        ("logging.rs", "build_rolling_appender", None, "monitor 自己的滚动日志"),
        ("logging.rs", "write_diagnostics_to_config", None, "把诊断信息写进 monitor 自己的配置"),
        ("logging.rs", "atomic_replace", None, "原子替换原语的本地副本（头注自陈是从 config.rs 复制的）"),
        ("session_map.rs", "run_watcher", None, "monitor 自己的会话映射状态"),
        ("sftp_pool.rs", "download_inner", None, "把远端文件落到本地缓存；写的不是用户既有环境"),
        ("utils.rs", "atomic_write_json", None, "通用原子写原语，调用方各自申报"),
        ("utils.rs", "atomic_replace_path", None, "同上，原语的本地副本"),
        // ── `K-P1`：常驻那条路要写两样东西。**都不是安装动作** —— 写的是 monitor 自己的目录。
        ("local_daemon.rs", "ensure_listen_token", None,
         "写 `~/.cc-monitor/listen-token`（**`0600`**，`create_new` 只创建一次）。\
          ★ 它买的是**权限位**：回环 TCP 上同机任何进程（含别的用户）都连得上，\
          Unix socket 有权限位而它没有，收窄只能靠一个 token；而 **daemon 只读铁律不许它自己写文件**\
          ⇒ token 只能由宿主生成、当 env 传进去。**这一格是一条真裁决，不是实现细节。**\
          幂等：已存在就读回（重写会让上一个宿主留下的那个 daemon 当场变成接不上的孤儿）"),
        ("local_daemon.rs", "write_listen_pid", None,
         "写 `~/.cc-monitor/listen-<port>.pid` —— 「谁在听那个口」。\
          它**不是**真相源（真相源永远是「那个口连不连得上」），只在**停**那一步用，\
          且用之前还要过一道 `/proc/<pid>/exe` 的身份核对。\
          没有它，接管来的那个实例按不动「停」——那时按钮就成了一句骗人的话"),
        // ── 既不是安装、也不是「monitor 自己的」：**删用户数据**
        ("history.rs", "delete_history_session", None,
         "★ 删的是用户 `~/.claude/projects/**` 下的会话文件（用户主动发起）。\
          它不是安装动作，但也不是 monitor 自己的东西。⚠ **08-07 订正**：本行原写「围栏由 \
          `validate_delete_target` 与它自己的判据守着」—— 那句只对一半：围栏函数有五条穿越 \
          防护判据，但**没有任何东西钉住那条路真的过了围栏**（实测跳过围栏，全仓 978 条不红）。 \
          现由 `history.rs::the_delete_entry_point_actually_goes_through_the_fence` 端到端钉住"),
    ];

    /// 一行 `use ... fs ...` 该放行还是该拦。`Ok(())` = 放行。
    ///
    /// # 为什么要有这道
    ///
    /// 本模块的人群靠 **`fs::` 这个前缀**认写盘调用（`WRITE_CALLS` 每一条都带它），
    /// `hooks_diag::this_module_never_writes` 的白名单也是扫 `fs::`。
    /// 两条判据因此**共享同一个前提**：`std::fs` 只能以带前缀的形态出现。
    ///
    /// 08-07 实测这个前提没人守：往 `hooks_diag.rs` 里加
    /// `use std::fs as sysio;` + `sysio::write(p, s)`（一个名叫
    /// `this_module_never_writes` 的模块里真写一次盘），全仓 **974 条判据一条不红** ——
    /// 连本模块上一轮刚建的写点人群都漏掉了。⚠ 头一次变异我把别名取成 `ffs`，
    /// 而 `ffs::write(` 里**含有** `fs::write(` 子串 ⇒ 两条判据都红了，
    /// 差点被我读成「有人守着」。**变异要造得像，巧合的红比不红更骗人。**
    ///
    /// 修法照 daemon 侧 `readonly_guard` 的先例：**堵逃生口**，别去追那些改写形态。
    fn fs_import_verdict(line: &str) -> Result<(), String> {
        let s = line.trim();
        if !s.starts_with("use ") || !s.contains("fs") {
            return Ok(());
        }
        // 放行两种，两种今天都真实存在（各自的理由写在这里，不是「看着眼熟」）：
        // · `use std::fs;`      —— 调用处必然写成 `fs::xxx(`，前缀还在
        // · `use std::fs::File;` —— File 的写入口 `File::create(` 自己就在 `WRITE_CALLS` 里
        if s == "use std::fs;" || s == "use std::fs::File;" {
            return Ok(());
        }
        if s.contains("std::fs as ") || s.contains("fs as ") {
            return Err("别名导入：调用处不再带 `fs::` 前缀，两条判据同时瞎掉".into());
        }
        if s.contains("std::fs::*") {
            return Err("glob 导入：写函数变成裸名字，前缀扫描扫不到".into());
        }
        if s.contains("std::fs::") {
            return Err("条目导入：`use std::fs::write;` 之后 `write(..)` 不带前缀".into());
        }
        Ok(())
    }

    /// ★★ **没有一种导入形态能让写盘调用丢掉 `fs::` 前缀**〔audit-0805 08-07〕。
    ///
    /// 这是上面那条人群、以及 `hooks_diag::this_module_never_writes` 白名单的**共同前提**。
    /// 前提没人守的时候，两条判据会**同时**瞎掉且都保持绿色 —— 08-07 实测过（见
    /// [`fs_import_verdict`] 的头注）。
    #[test]
    fn no_alias_or_item_import_can_hide_a_write_call() {
        // ① 常驻正反例：真代码里今天**没有**违规样本，那一支平时没人行使 ——
        //    本仓已连着五次栽在「新分支平时没人走」上，所以样本写死在这里。
        for ok in [
            "use std::fs;",
            "use std::fs::File;",
            "use std::path::Path;",
            "let x = 1;",
        ] {
            assert!(
                fs_import_verdict(ok).is_ok(),
                "{ok:?} 该放行却被拦了 —— 分类器过严会逼人去改合法代码"
            );
        }
        for bad in [
            "use std::fs as sysio;",
            "use std::fs as f;",
            "use std::fs::*;",
            "use std::fs::write;",
            "use std::fs::{self, write};",
        ] {
            assert!(
                fs_import_verdict(bad).is_err(),
                "{bad:?} 该被拦却放行了 —— 这正是 08-07 那次真写盘溜过去的形态"
            );
        }

        // ② 全树扫描。
        let files = corpus();
        let mut checked = 0usize;
        let mut offenders = Vec::new();
        for (path, src) in &files {
            let prod = guard_core::production_code(src);
            for line in prod.lines() {
                if line.trim().starts_with("use ") && line.contains("fs") {
                    checked += 1;
                    if let Err(why) = fs_import_verdict(line) {
                        offenders.push(format!("  {}: {} —— {why}", path.display(), line.trim()));
                    }
                }
            }
        }
        // 抽取器自检：一条 `use ... fs ...` 都没扫到 ⇒ 上面整段在空转。
        assert!(
            checked >= 3,
            "全树只扫到 {checked} 行含 fs 的 `use`（08-07 实测 5：`use std::fs;` ×1 + `use std::fs::File;` ×4）\
             —— 剥法或扫描面坏了，本条此刻无效"
        );
        assert!(
            offenders.is_empty(),
            "这些导入会让写盘调用**丢掉 `fs::` 前缀**：\n{}\n\n\
             ⚠ 前缀一没，两条判据同时瞎掉且都保持绿色：\n\
             · 本模块的写点人群（`WRITE_CALLS` 每一条都带 `fs::`）；\n\
             · `hooks_diag::this_module_never_writes` 的白名单（它扫的就是 `fs::`）。\n\
             08-07 实测：`use std::fs as sysio;` + `sysio::write(p, s)` 放进 `hooks_diag.rs`，\n\
             974 条判据一条不红 —— 而那个模块判据的名字逐字写着「never writes」。\n\
             修法：用 `use std::fs;` 走全前缀，或直接写 `std::fs::xxx(`。别改本条去迁就它。",
            offenders.join("\n")
        );
    }

    fn src_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// 语料 = `src/` 整棵树 **+ `build.rs`**〔audit-0805 08-08〕。
    ///
    /// ★ 为什么非把 `build.rs` 并进来：本表问的是「**谁能碰这台机器**」，
    /// 而构建脚本每次 `cargo build`／`cargo check` 都在开发者机器上真跑
    /// （它起 `sh` 与 `git`、往 `OUT_DIR` 复制内嵌 daemon）。
    /// 08-08 实测：全仓所有登记表/守卫的扫描根都是 `src-tauri/src` · `remote-daemon-proto/src`
    /// · `src-tauri/crates` · `src` · `doc` —— **`src-tauri/build.rs` 一张表都没扫到**，
    /// 它是这些扫描面共同的盲点（与 F65「三张表共享同一个没写下来的前提」同族）。
    fn corpus() -> Vec<(PathBuf, String)> {
        let mut files = guard_core::scan_tree!(&src_root(), &["rs"]);
        let bs = Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs");
        let src = std::fs::read_to_string(&bs).expect("读不到 build.rs —— 它是本表的一部分");
        files.push((bs, src));
        files
    }

    /// 从一份源码的**生产段**里抠出「有写盘调用的 (函数名)」。
    ///
    /// ⚠ 归属按「上一处 `fn 名字`」判 —— 粗，但**只会把落点归给更靠前的函数**，
    /// 归错了会让申报表里出现一个对不上的名字，那是**会红**的方向（不是静默变绿）。
    fn write_fns(src: &str) -> Vec<String> {
        let prod = guard_core::production_code(src);
        let mut cur = String::new();
        let mut out: Vec<String> = Vec::new();
        for line in prod.lines() {
            if let Some(rest) = line
                .split(" fn ")
                .nth(1)
                .or_else(|| line.strip_prefix("fn "))
            {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    cur = name;
                }
            }
            if WRITE_CALLS.iter().any(|c| line.contains(c))
                && !cur.is_empty()
                && !out.contains(&cur)
            {
                out.push(cur.clone());
            }
        }
        out
    }

    /// ★ 正题：**每个写盘落点都得申报**，安装动作那一类还要点名真实存在的工具。
    #[test]
    fn every_write_site_is_declared_and_installers_name_a_real_tool() {
        let files = corpus();
        let mut found: Vec<(String, String)> = Vec::new();
        for (path, src) in &files {
            let stem = path
                .file_name()
                .and_then(|s| s.to_str())
                .expect("文件名")
                .to_string();
            for f in write_fns(src) {
                found.push((stem.clone(), f));
            }
        }
        // ★ 抽取器自检：扫不到东西时下面整条会零命中地绿。
        assert!(
            found.len() >= 15,
            "全树只找到 {} 个写盘落点（08-07 实测 19 个「文件::函数」）—— 抽取器坏了，本条此刻无效",
            found.len()
        );

        let missing: Vec<String> = found
            .iter()
            .filter(|(f, n)| !WRITE_SITES.iter().any(|(sf, sn, _, _)| sf == f && sn == n))
            .map(|(f, n)| format!("  {f}::{n}"))
            .collect();
        assert!(
            missing.is_empty(),
            "这些地方**会往盘上写东西，但没人申报它是不是安装动作**：\n{}\n\n\
             ⚠ `ROADMAP §5 4b` 记的正是这个缺口：`tool_registry` 只守声明表自洽，\n\
             人群是「声明了的工具」而不是「真实发生的安装动作」——\n\
             于是新增一个写点，声明表可以一直不知道。\n\
             登记进 `WRITE_SITES`：是某个工具的安装动作就点名它的 `id`（要在 `TOOLS` 里真实存在），\n\
             不是就写清它写的是什么（例如「monitor 自己的缓存」）。",
            missing.join("\n")
        );

        // ★ 反向锚点：申报了一个已经不存在的落点 ⇒ 它在替真判据挡枪。
        let stale: Vec<String> = WRITE_SITES
            .iter()
            .filter(|(f, n, _, _)| !found.iter().any(|(ff, nn)| ff == f && nn == n))
            .map(|(f, n, _, _)| format!("  {f}::{n}"))
            .collect();
        assert!(
            stale.is_empty(),
            "申报表里这些落点**已经不写盘了**（改名、收口或删掉了）：\n{}\n\
             改名也要红 —— 名字变了就该有人重新回答一次「它是不是安装动作」。",
            stale.join("\n")
        );

        // ★★ 4b 要的那条连线：安装动作必须点名 `TOOLS` 里真实存在的 id。
        let table = guard_core::production_code(include_str!("tool_registry.rs"));
        let mut checked = 0usize;
        for (f, n, tool, _) in WRITE_SITES {
            let Some(id) = tool else { continue };
            checked += 1;
            assert!(
                table.contains(&format!("id: \"{id}\"")),
                "`{f}::{n}` 申报成工具 `{id}` 的安装动作，但 `tool_registry::TOOLS` 里没有这个 id。\n\
                 要么 id 写错了，要么那个工具被删了而真实的安装动作还留着 —— 后者更值得查。"
            );
        }
        // 常驻自检：一条安装动作都没有时，上面那个循环空转，而它看起来照样绿。
        assert!(
            checked >= 3,
            "申报表里只有 {checked} 条「安装动作」（08-07 实测 5）—— \
             要么真收口了（那很好，把这个数调下来），要么有人把它们改成了 `None` 绕过对拍。"
        );
    }
}
