//! ═══════════════════════════════════════════════════════════════════
//! # 要求住址：**`src/doc/INVARIANTS.md` 条 44**
//! ═══════════════════════════════════════════════════════════════════
//!
//! 🔴 **它与 `§41.6` D1 那条附加条件是两件事，而那正是条 44 存在的理由。**
//! 那一条逐字要求「起进程的面**逐条登记**…写明『做什么、
//! **为什么不违反收窄后的铁律**』」—— 它是**后端**只读铁律收窄时的强制条件。
//!
//! 而 **monitor 侧起进程不违反任何铁律**（monitor 本来就能写用户的东西）
//! ⇒ 那个理由栏**对这一侧没有意义**。
//!
//! ⇒ 条 44 要的是另一件事：**起进程不许悄悄扩散，每一族只许有一个出口。**
//! 它防的不是写盘，是「第二条出路」（同一件事两处起法，参数拼装 / 环境擦洗 /
//! 错误归因一定会漂）与「没人认领的子进程」。
//!
//! ⚠ 条 44 是 2026-09-22 才有的（`P20` 现打之后升格）；
//! 在那之前本表点不到任何要求。逐条依据住。
//!
use std::path::{Path, PathBuf};

/// 命令串的来历。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// 整条命令是编译期常量（没有任何插值面）。
    Const,
    /// 命令由一个受控构造器产出，构造器名写在这里。
    Builder(&'static str),
    /// 命令在本函数里拼，但自由文本过了 `shell_quote`（或插的只有常量）。
    Quoted,
    /// 只转发调用方给的命令，自己不构造 —— 真来历在调用方。
    PassThrough,
}

/// `(文件, 函数, 来历, 说法)`。**默认拒绝**：人群从源码派生，没登记的当场红。
const EXEC_SITES: &[(&str, &str, Origin, &str)] = &[
    // ── 整条命令是常量/无插值字面量
    // `cc_bus.rs / fetch_remote_cc_bus`（`CC_BUS_CAT_CMD`）出去了：驾驶舱读名册改走后端 `bus-state`。
    // `hooks_diag.rs / diagnose_remote_cc_bus_hooks` 出去了：事实改问那台后端（`footprint-probe` ＋ `files-peek`）。
    // `mcp.rs / fetch_remote_claude_json` 出去了：MCP 列表改问那台后端 `mcp-read`。
    // `ccm_probe.rs / probe_ccm_cli` 出去了：远端 `ccm` 会哪些改问那台后端 `ccm-probe`（`CCM_PROBE_CMD` 只剩本机那一跳）。〔散文墓碑〕
    // `sftp.rs` 里只问 `uname -m` 的那一处走了：部署前问机器改问 `uname -s -m`（`byte_table::probe_key`），
    //   走的是 `connect_and_exec_capture`（收全、有上限、带退出码）⇒ 不在本表人群（本表只数 `connect_and_exec_cmd(`）。
    // ── 受控构造器（构造器自己带校验/引用，各有行为判据）
    // 公钥推送那一行出去了：读 `.pub` · 组请求 · 一次 exec 都进了本机后端（`pubkey-push`，`src/backend/assets/pubkey.rs`）。
    // 🔴 **`K-R112`（09-13）：这里原来有两行，两行都出去了** ——
    //    `cc_bus.rs / check_cc_bus_agent_online`（`Builder("build_online_cmd")`）与
    //    `tmux.rs / capture_remote_pane`（`Builder("build_capture_pane_cmd")`）。
    //    形状与理由**与上面 `K-R72` / `K-R104` 那几笔逐字同形**：那两处的一次性 SSH exec
    //    整条没了（查在线改走帧 `bus-list`、抓屏改走帧 `capture-pane`）
    //    ⇒ 它们不再是「远端执行点」，两个构造器也随之整块删。
    //    留着它们，上面那条反向锚点（「申报了一处已经不存在的执行点」）会当场逮住 ——
    //    而且这一次连 `Builder` 那一支的机检也会红（构造器函数不存在了）。
    //    ⚠ **`Builder` 这一类今天零个样本**（最后那个公钥推送进了本机后端）：下面那条常驻自检
    //    只查 `PassThrough` 那一支还有活样本；`Builder` 收敛到 0 那天自检已一起改（归零一类 ⇒ 断言它恒零）。
    // ⚠ `K-R72`（09-12）：`kill_remote_tmux` / `tmux_send_keys` **从本表出去了** ——
    //    它们那两条一次性 SSH 回落删了，今天只走后端通道 ⇒ 不再是「远端执行点」。
    //    这一改是**结构性强制的随动**：上面那条反向锚点（「申报了一处已经不存在的执行点」）
    //    会当场逮住留在表里的两行。留着它们等于让申报表替真判据挡枪。
    // 🔴 **`K-R104`（09-13）：`account_usage.rs / account_usage` 这一行也出去了**
    //    ——与上面 `K-R72` 那两条**同一个形状、同一个理由**：它那条一次性 SSH exec
    //    整条没了（编排改走后端帧面）⇒ 它不再是一个「远端执行点」。
    //    留着它，上面那条反向锚点（「申报了一处已经不存在的执行点」）会当场逮住。
    // ── 本函数里拼，但自由文本过了 shell_quote
    // 🔴 **这里原来还有两行，两行都出去了**：
    //    `remote_history.rs / stream_read_remote_session`（`--read-session`）与
    //    `stream_source/snapshot.rs / fetch_snapshot`（`--read-session-tail`）。形状与理由同上面
    //    `K-R72` / `K-R104` / `K-R112` 那几笔：那两处的一次性 SSH exec 整条没了
    //    （改走长连接的 `history-read` / `history-tail`，`frame_query`）
    //    ⇒ 它们不再是「远端执行点」。
    // 🔴 **逐次拨号那一行也出去了**（`remote_history.rs` 的 `run_list_query`〔散文墓碑〕）：
    //    它的放行表 C4c 起是空的，已删 —— 同上面几笔的形状（那条一次性 SSH exec 整条没了）。
    // `stream_source/` 起远端流模式那一处出去了：那一形删了（远端只剩常驻，`remote_resident::attach`）。
    // `tmux.rs / list_remote_tmux` 出去了：列会话改问那台后端 `tmux-list`。
    // ── 只转发，不构造（命令来自调用方）
    // `stream_source/` 那一行（原语自己的定义）出去了：最后一个调用方（公钥推送）进了本机后端，原语随之删了
    //   ⇒ 本表空了，下面那条判据改成「零处」（带正控）。
    // `cc_bus.rs / exec_read` 出去了：读收件箱改走后端 `bus-inbox`。
    // 〔09-28 裁 2〕`acct_iso_deploy.rs / exec_collect` 出去了：跑安装脚本 · 核 PATH 两步换成那台后端的 `acct-iso-install`。
];

fn src_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::crate_src_root()
}

/// 某一行所在的函数名：往回找最近的一处 `fn <名>`（顶格或缩进都算）。
///
/// ⚠ 与 `cc_bus` 里那个「按名字取函数体」不是同一件事，故不共用：
/// 那个是「给名字要体」，这个是「给行要名字」。
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

/// 函数体：从 `at` 那行所在函数的签名行起，到下一个顶格行为止。
fn body_around(lines: &[&str], at: usize) -> String {
    let mut start = at;
    while start > 0 && !(lines[start].contains("fn ") && !lines[start].trim().is_empty()) {
        start -= 1;
    }
    let mut out = Vec::new();
    for (i, l) in lines[start..].iter().enumerate() {
        let top = !l.is_empty() && !l.starts_with(char::is_whitespace) && !l.starts_with(')');
        if i > 0 && top {
            break;
        }
        out.push(*l);
    }
    out.join("\n")
}

/// 函数体里**不属于错误路径**的行。
///
/// ⚠ 不加这道，`format!` 会命中 `map_err(|e| format!("读远端输出失败: {e}"))` ——
/// 把一个纯转发函数误判成「在构造命令」。**同一个坑本会话第二次**（上一件是
/// `cc_bus` 里最长字面量挑中了错误消息）⇒ 它不是偶发，是「扫源码找语义」的固有形态：
/// 错误消息与真逻辑用的是同一套字符串原语。
/// 错误路径的标志：`format!` 出现在这些组合子的闭包里时，它拼的是**错误消息**不是命令。
///
/// ⚠ 列成具名集合而不是一种一种追：第一版只排 `Err(`/`map_err`，
/// 于是 `ok_or_else(|| format!("未找到远端配置: …"))` 又把一处误判成「在拼命令」。
/// **一种一种追就是「按怎么写的取样」** —— 那正是本工作区反复在治的病。
/// **边界**：真在 `unwrap_or_else` 里拼命令的话本条看不见（今天全树无此形态）。
const ERROR_COMBINATORS: &[&str] = &["Err(", "map_err", "ok_or_else", "unwrap_or_else", "expect("];

fn non_error_lines(body: &str) -> String {
    body.lines()
        .filter(|l| !ERROR_COMBINATORS.iter().any(|c| l.contains(c)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 取某一行 `connect_and_exec_cmd(a, b)` 里的**第二个实参**（去掉 `&` 与空白）。
///
/// ⚠ 这是本条真正关心的那个对象。第一版判的是「函数体里有没有 `format!`」——
/// 那是个**代理**，而 `format!` 在这些函数里还用来拼错误消息和 UI 文案：
/// 一路把 `map_err` / `ok_or_else` 排掉之后，最后栽在
/// `report(d, format!("[{origin}] ~/.claude/settings.json"), probe)` 上。
/// ⇒ **别用代理**：命令串是哪个实参，就去看那个实参。
fn command_arg(line: &str) -> String {
    let Some(i) = line.find("connect_and_exec_cmd(") else {
        return String::new();
    };
    let rest = &line[i + "connect_and_exec_cmd(".len()..];
    let inner = rest.split(')').next().unwrap_or("");
    inner
        .split(',')
        .nth(1)
        .unwrap_or("")
        .trim()
        .trim_start_matches('&')
        .trim()
        .to_string()
}

#[test]
fn every_remote_exec_declares_where_its_command_came_from() {
    let files = guard_core::scan_tree!(&src_root(), &["rs"]);
    let mut found: Vec<(String, String, String)> = Vec::new();
    for (path, src) in &files {
        let prod = guard_core::production_code(src);
        let lines: Vec<&str> = prod.lines().collect();
        let stem = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap()
            .to_string();
        for (i, l) in lines.iter().enumerate() {
            if l.contains("connect_and_exec_cmd(") {
                found.push((
                    stem.clone(),
                    enclosing_fn(&lines, i),
                    format!("{}\n{}", command_arg(l), body_around(&lines, i)),
                ));
            }
        }
    }
    assert!(
        found.is_empty(),
        "找到 {} 处 `connect_and_exec_cmd(`（原语已删）—— 有人把一次性 exec 字节流的原语加回来了",
        found.len()
    );
    // 正控：同一个抽取形状在合成语料上数得出（零处不是因为尺子瞎了）。
    assert!(
        command_arg("    let s = connect_and_exec_cmd(&cfg, &cmd).await?;") == "cmd",
        "实参抽取器认不出 `connect_and_exec_cmd(` 了 —— 零处可能是尺子瞎了"
    );

    let missing: Vec<String> = found
        .iter()
        .filter(|(f, n, _)| !EXEC_SITES.iter().any(|(sf, sn, _, _)| sf == f && sn == n))
        .map(|(f, n, _)| format!("  {f}::{n}"))
        .collect();
    assert!(
        missing.is_empty(),
        "这些远端执行点**没人申报命令串从哪来**：\n{}\n\n\
             ⚠ 08-07 实测：新增一处把自由文本直接拼进命令的执行点，全仓判据一条不红。\n\
             登记进 `EXEC_SITES`，三选一：`Const`（整条是常量）/ `Builder(名)`（受控构造器）/\n\
             `Quoted`（本函数里拼但过了 `shell_quote`）。三类都有机检，不是写上就算。",
        missing.join("\n")
    );

    // ★ 反向锚点：申报了一处已经不存在的执行点 ⇒ 它在替真判据挡枪。
    let stale: Vec<String> = EXEC_SITES
        .iter()
        .filter(|(f, n, _, _)| !found.iter().any(|(ff, nn, _)| ff == f && nn == n))
        .map(|(f, n, _, _)| format!("  {f}::{n}"))
        .collect();
    assert!(
        stale.is_empty(),
        "申报表里这些执行点在源码里找不到了（改名或删了）：\n{}\n\
             改名也要红 —— 名字变了就该有人重新回答一次「命令从哪来」。",
        stale.join("\n")
    );

    // ★★ **申报不是白申报**：四类各自有机检，且都直接判**那个实参**。
    let mut per_class = [0usize; 4];
    for (f, n, blob) in &found {
        let (arg, body) = blob.split_once('\n').expect("第一行是实参");
        let (_, _, origin, why) = EXEC_SITES
            .iter()
            .find(|(sf, sn, _, _)| sf == f && sn == n)
            .expect("上面已保证登记齐全");
        match origin {
            Origin::Const => {
                per_class[0] += 1;
                // 三种都算常量命令：实参就是字面量 · 实参是全大写常量 ·
                // 实参是个绑定到**字面量**的局部（`let cmd = "…"`，`ccm_probe` 就是这形）。
                let ok = arg.starts_with('"')
                    || (!arg.is_empty() && arg.chars().all(|c| c.is_ascii_uppercase() || c == '_'))
                    || body.contains(&format!("{arg} = \""));
                assert!(
                    ok,
                    "`{f}::{n}` 申报成 `Const`（{why}），但送进去的实参是 `{arg}` —— \
                         既不是字面量也不是全大写常量。它已经不是常量命令了。"
                );
            }
            Origin::Builder(b) => {
                per_class[1] += 1;
                // ⚠ 只要求「构造器被调用」，不要求 `{arg} = {b}(` 这个形状：
                // `let cmd = match builder(..) { .. }` 这种写法是合法的，卡形状会把它判红。
                // **代价写在头注「不守」里**：构造器被调用、而命令另拼一份，本条看不见。
                assert!(
                    body.contains(&format!("{b}(")),
                    "`{f}::{n}` 申报成走构造器 `{b}`（{why}），但函数体里根本没调它（实参 `{arg}`）—— \
                         构造器改名了？还是有人把命令改成就地拼了？后者正是本条要拦的。"
                );
            }
            Origin::Quoted => {
                per_class[2] += 1;
                assert!(
                    body.contains("shell_quote(")
                        || body.contains("TMUX_LS_FMT")
                        || body.contains("BACKEND_CMD"),
                    "`{f}::{n}` 申报成「在本函数里拼但引用了」（{why}），实参 `{arg}`，\
                         而函数体里既没有 `shell_quote(`、插的也不是那个受守的常量 —— \
                         自由文本正裸着进远端命令。"
                );
            }
            Origin::PassThrough => {
                per_class[3] += 1;
                let is_param = body.contains(&format!("{arg}: &str"));
                let is_definition = *f == "stream_source/exec.rs";
                assert!(
                    is_param || is_definition,
                    "`{f}::{n}` 申报成只转发（{why}），但实参 `{arg}` 不是它的 `&str` 入参 —— \
                         转发者一旦自己构造命令，就该按自己的来历重新申报。"
                );
            }
        }
    }
    // 常驻自检：某一类归零时上面那一支就没人行使，而它看起来照样绿。
    // ⚠ 本仓已连着六次栽在「新分支平时没人走」上，所以四类各要一个活样本。
    // `Const` 那一类收敛到零（最后一条 `probe_ccm_cli`〔散文墓碑〕 改问那台后端 `ccm-probe`）⇒ 那一支没有人群、自检只对余下三类。
    // `Quoted` 那一类也收敛到零（最后一条 `stream_source/` 起远端流模式那一处随那一形删了）⇒ 同上。
    // `Builder` 那一类也收敛到零（最后一条 `pubkey.rs` 的公钥推送进了本机后端 `pubkey-push`）⇒ 同上。
    assert_eq!(
        per_class,
        [0, 0, 0, 0],
        "四类里又长出了样本 —— 回来把它放回自检"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// **为什么还是一条拨号 shell**：逐处写理由，两向相等
// ═════════════════════════════════════════════════════════════════════════════
//
// 要求：「所有 SSH 由本机常驻后端持有」· 「其余一次性 exec 点……**逐处仍要核**」·
// 要求「9 处 shell 能换后端具名命令的逐处换，换不了的写理由并登记」。
//
// 上面那张 `EXEC_SITES` 管「命令串从哪来」（注入面）；本表管另一件事：**这一处为什么还没换成那台后端的具名命令**。
// 人群 = monitor 生产段里 `connect_and_exec_cmd(` ∪ `connect_and_exec_capture(` 的调用点（「文件::外层函数」，
// 两个原语自己的定义不算），**两向相等**：多一处 = 又长出一条自拼 shell 的路（先回答它为什么不能是后端命令）；
// 少一处 = 那一处换掉了（好事，删行）。
//
// 类别：`Bootstrap` —— 这一跳发生在那台的后端**还不存在 / 正在被装**的时候，问不了它；
//       `Deploy` —— 装的是后端之外的工具（「部署 …… 不迁」）；
//       `Pending(归谁)` —— **能换**，本路没换，写清卡在哪、归谁。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StillShell {
    Bootstrap,
    Deploy,
    Pending(&'static str),
}

const STILL_SHELL: &[(&str, &str, StillShell, &str)] = &[
    ("remote_resident.rs", "ensure", StillShell::Bootstrap,
     "起 / 找那台的**常驻后端本身**（`--resident-ensure`）—— 接上它之前没有后端可问"),
    ("remote_resident.rs", "stop", StillShell::Bootstrap,
     "停那台的常驻后端（`--resident-stop`）—— 不能请被停的那一位自己经它那条流停自己（流随之断、答不回来）"),
    // `sftp.rs` 那一行（部署后端之前扫落点那一份的身份戳）摘了：身份判定进了本机常驻后端（`deploy-plan` 沿池里那条 SSH 自己扫）。
    // 公钥推送那一行摘了：「那台后端还不在」那一形也进了本机后端（`pubkey-push`：可达表里没有那台 ⇒ 一次 exec，只写这一件）。
    // 〔09-28 裁 2〕`acct_iso_deploy.rs` 那一行摘了：部署那两步（跑安装脚本 · 核 PATH）换成那台后端的 `acct-iso-install`
    //   （链接走写面 `files-link`，装卸账记 skill 装记录）；字节照走部署那一条（SFTP 经本机后端）。
    // `mcp.rs` 那一行摘了：`agents::Adapter` 长了一格 MCP 读，后端 `mcp-read` 出成品（选的那一条）。
    // `hooks_diag.rs` 那一行摘了：换成那台后端的 `files-peek` ＋ `footprint-probe`（解锁条件兑现）。
    // `probe_ccm_cli`〔散文墓碑〕 那一行摘了：`ccm` 就是那台后端本身，「会哪些」改问那台后端 `ccm-probe`
    //   （与 `ccm --ccm-probe` 同一份），不再进交互 shell 查 `PATH`（写好的解锁条件兑现）。
    // `tmux.rs` 那一行摘了：后端新帧命令 `tmux-list`（同 watcher 那一趟 `tmux ls`），monitor 改问它。
    // `cc_bus.rs` 那两行（读名册 · 读收件箱）摘了：cc-bus 加了机器可读的读命令，后端 `bus-state` / `bus-inbox` 转调，界面经通道问。
];

fn still_shell_population() -> std::collections::BTreeSet<(String, String)> {
    let files = guard_core::scan_tree!(&src_root(), &["rs"]);
    let mut out = std::collections::BTreeSet::new();
    for (path, src) in &files {
        let prod = guard_core::production_code(src);
        let lines: Vec<&str> = prod.lines().collect();
        let stem = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap()
            .to_string();
        for (i, l) in lines.iter().enumerate() {
            let t = l.trim_start();
            if t.starts_with("pub async fn connect_and_exec_cmd(")
                || t.starts_with("pub async fn connect_and_exec_capture(")
            {
                continue;
            }
            if l.contains("connect_and_exec_cmd(") || l.contains("connect_and_exec_capture(") {
                out.insert((stem.clone(), enclosing_fn(&lines, i)));
            }
        }
    }
    out
}

#[test]
fn every_remaining_dial_shell_says_why_it_is_not_a_backend_command() {
    let found = still_shell_population();
    // 正控：人群里真有东西（起那台常驻后端那一问必在）—— 抽取器坏了会拿空集比。
    // 原来认 `byte_table.rs` 那一处（问 `uname`），那一问进了后端 ⇒ 换认 `remote_resident.rs::ensure`。
    assert!(
        found.contains(&("remote_resident.rs".to_string(), "ensure".to_string())),
        "抽取器没认出 `remote_resident.rs::ensure` —— 本条此刻在空转：{found:?}"
    );
    let want: std::collections::BTreeSet<(String, String)> = STILL_SHELL
        .iter()
        .map(|(f, n, _, _)| (f.to_string(), n.to_string()))
        .collect();
    assert_eq!(
        found, want,
        "monitor 里「经拨号链路跑 shell」的点与理由表对不上。\n\
         多出来的 ⇒ 先回答它为什么不能是那台后端的一条具名命令；\n\
         少了的 ⇒ 换掉了，删那一行。"
    );
    for (f, n, kind, why) in STILL_SHELL {
        assert!(why.chars().count() >= 20, "`{f}::{n}` 的理由太短：{why}");
        if let StillShell::Pending(owner) = kind {
            assert!(
                owner.chars().count() >= 4,
                "`{f}::{n}` 是「能换未换」却没写归谁"
            );
        }
    }
}
