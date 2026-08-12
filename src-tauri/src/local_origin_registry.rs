//! P4d-Y5：**「先查远端配置再干活」的地方，必须先分本机** —— 按形状收口，不按症状。
//!
//! # 这一族错误本轮出现了五次
//!
//! `load_remote_config_by_label(&origin)` 对 `<local>` 拿不到东西，于是报
//! **「未找到远端配置: `<local>`」** —— 一句与真实原因毫无关系的话。真实原因从来不是
//! 「配置没找到」，而是「这条路是远端专属的，本机根本不该走到这里」。
//!
//! 已逐个修过三处（`daemon_kill` / `list_remote_tmux` / `launch_remote_terminal`），
//! 而 `P3b` 的 E 阶段又量到第四处（`capture_remote_pane`）。
//!
//! ⇒ **别再一个一个修。** 一个一个修的问题不是慢，是它对「第六次」毫无办法：
//! 下一个人写下一处时，前面四处的教训对他不可见。
//!
//! # 本护栏钉什么
//!
//! 每一处 `load_remote_config_by_label(` 调用，**要么**它所在的函数在调用之前就分了本机，
//! **要么**它在 [`REMOTE_ONLY`] 里逐条登记「为什么 `<local>` 够不到这里」。
//!
//! ## 为什么是**位置**比较，不是「函数里提过 `LOCAL_ORIGIN`」
//!
//! 后者是**块粒度**，而块粒度在这个仓里栽过：一句该红的话紧挨着一句含关键词的话就能蒙混
//! （`launch.rs` 那条散文守卫按块扫时的真实读数）。这里的失败形状一模一样 ——
//! 函数末尾写一句「本机走另一条」的注释，不影响开头那句照样对 `<local>` 报假话。
//! ⇒ 分本机的动作必须**在**那次调用之前。
//!
//! ## 它挡什么、不挡什么（如实登记）
//!
//! - **挡**：新写一处「先查远端配置」而不先分本机、也不登记 ⇒ 红。
//! - **不挡**：分了本机但**分错了**（本机分支自己的逻辑是坏的）。本护栏只保证
//!   「本机这条路被单独想过一次」，不保证想对了。这条边界写在这里，
//!   免得下一个人以为它保证了更多。

#[cfg(test)]
const CALL: &str = "load_remote_config_by_label(";

/// 本机**结构性够不到**的调用点，逐条登记：(文件, 函数, 为什么 `<local>` 到不了这里)。
///
/// ⚠ 登记的是「够不到」，不是「还没做」。**「以后再说」不是理由** —— 那种进 [`TRIAGE_DEBT`]。
#[cfg(test)]
const REMOTE_ONLY: &[(&str, &str, &str)] = &[];

/// ★★ **本轮没有逐条量过的存量**（P4d-Y5，08-12）。
///
/// # 为什么它不是 [`REMOTE_ONLY`] 的一部分
///
/// 这 19 处**我没有逐个读过**。给它们各编一句「本机够不到，因为……」很容易，
/// 而那正是 `P3b` 刚刚立判据去抓的东西：**B 类假理由（写下时就没验证过）**。
/// B 类的特点是时间线扫不到它 —— 它从来没真过，所以没有「哪天变假的」那一刻可查。
/// 在这里造 19 条，等于亲手制造一批下次审计要花力气才能识别的假话。
///
/// ⇒ 本表逐字承认：**这是欠账，不是裁定。** 它对本护栏的作用只有一个 ——
/// 挡住**新增**。存量该怎么处置，归 ROADMAP 那件事，不藏在护栏的白名单里。
///
/// ⚠ **只许变短。** 下面那个数是等号不是地板：少一条要回来改它（那是好事，说明有人真去量了），
/// 多一条同样会红（新增的必须走 `REMOTE_ONLY` 或者去加本机分支）。
#[cfg(test)]
const TRIAGE_DEBT: &[(&str, &str)] = &[
    ("account_usage.rs", "account_usage"),
    ("accounts.rs", "cfg_for"),
    ("cc_bus.rs", "cfg_of"),
    ("cc_bus.rs", "check_cc_bus_agent_online"),
    ("cc_bus.rs", "read_cc_bus_state"),
    ("ccm_probe.rs", "probe_ccm_cli"),
    ("hooks_diag.rs", "diagnose_remote_cc_bus_hooks"),
    ("launch.rs", "build_remote_ssh_ps_command"),
    ("mcp.rs", "list_remote_mcp_project_dirs"),
    ("mcp.rs", "read_remote_mcp_servers"),
    ("mcp.rs", "read_remote_project_mcp"),
    ("mcp.rs", "remove_remote_mcp_server"),
    ("mcp.rs", "write_remote_mcp_server"),
    ("port_forward.rs", "start_forward"),
    ("remote_branch.rs", "create_remote_branch_session"),
    ("remote_history.rs", "require_cfg_by_label"),
    ("ssh_source.rs", "connect_via_jump"),
    ("tmux.rs", "list_remote_tmux"),
    ("tmux.rs", "tmux_send_keys"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn src_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// 一行是不是**顶层** `fn` 声明。
    ///
    /// ⚠ 这里栽过一次，值得留着：第一版用的是一张固定前缀表
    /// （`"\nfn "` / `"\npub fn "` / `"\npub(crate) fn "` / `"\nasync fn "`），
    /// **漏了 `pub async fn`** —— 而这个仓里的 `#[tauri::command]` 几乎全是那种写法。
    /// 后果不是漏报，是**归错**：调用点被算进了它前面那个函数的跨度里，
    /// 于是位置比较量的是一段**不相干的代码**，绿或红都不作数。
    /// ⇒ 改成剥前缀，别再手抄组合 —— 组合数会随语言特性增长，手抄的表追不上。
    fn is_fn_decl(line: &str) -> bool {
        if line.starts_with(char::is_whitespace) {
            return false;
        }
        let mut rest = line;
        loop {
            let before = rest;
            for p in [
                "pub(crate) ",
                "pub(super) ",
                "pub(in crate) ",
                "pub ",
                "async ",
                "const ",
                "unsafe ",
                "extern \"C\" ",
                "default ",
            ] {
                if let Some(r) = rest.strip_prefix(p) {
                    rest = r;
                }
            }
            if rest.len() == before.len() {
                break;
            }
        }
        rest.starts_with("fn ")
    }

    /// 取 `at` 这个字节位置所在的那个顶层 `fn` 的起点。
    ///
    /// 找不到就退回文件头 —— 那种情况下位置比较退化成「文件里有没有」，**会更宽**，
    /// 所以下面对「退回文件头」的次数有上界自检。
    fn fn_start(src: &str, at: usize) -> (usize, bool) {
        let mut best: Option<usize> = None;
        let mut off = 0usize;
        for line in src.split_inclusive('\n') {
            if off >= at {
                break;
            }
            if is_fn_decl(line) {
                best = Some(off);
            }
            off += line.len();
        }
        match best {
            Some(i) => (i, false),
            None => (0, true),
        }
    }

    /// ★ P4d-Y5：每处「先查远端配置」都必须先分本机，否则要有登记的理由。
    #[test]
    fn every_remote_config_lookup_deals_with_the_local_origin_first() {
        let files = guard_core::scan_tree!(&src_root(), &["rs"]);
        assert!(
            files.len() >= 60,
            "只扫到 {} 个源文件 —— 遍历坏了，本断言在空转",
            files.len()
        );
        let mut sites = 0usize;
        let mut fell_back_to_file_head = 0usize;
        let mut offenders: Vec<String> = Vec::new();
        for (path, raw) in &files {
            let rel = path
                .strip_prefix(src_root())
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            // 本护栏自己逐字写着那个调用名 —— 跳过自己，否则它会把自己判成一处调用点。
            if rel == "local_origin_registry.rs" {
                continue;
            }
            let prod = guard_core::production_code(raw);
            let mut from = 0usize;
            while let Some(rel_at) = prod[from..].find(CALL) {
                let at = from + rel_at;
                from = at + CALL.len();
                // `fn load_remote_config_by_label(` 是定义本身，不是调用点。
                let before = &prod[..at];
                if before.ends_with("fn ") {
                    continue;
                }
                sites += 1;
                let (start, fell_back) = fn_start(&prod, at);
                let fn_name = prod[start..]
                    .split_once("fn ")
                    .and_then(|(_, t)| {
                        let end = t.find(|c: char| !(c.is_alphanumeric() || c == '_'))?;
                        Some(t[..end].to_string())
                    })
                    .unwrap_or_else(|| "<文件头>".to_string());
                if fell_back {
                    fell_back_to_file_head += 1;
                }
                let body = &prod[start..at];
                // 分本机的动作长什么样：读 `LOCAL_ORIGIN`，或逐字比 `<local>`。
                let deals_with_local =
                    body.contains("LOCAL_ORIGIN") || body.contains("\"<local>\"");
                if !deals_with_local {
                    offenders.push(format!("{rel}::{fn_name}"));
                }
            }
        }
        // 反向自检之一：**调用点必须真的数到了**。
        // 08-12 实测 28 处；写成地板是因为这个数天天在变，写等号会天天假红
        // （铁律 18：假阳会训练人绕过判据）。但地板要贴着实测，别写个 5 装样子。
        assert!(
            sites >= 20,
            "只数到 {sites} 处 `{CALL}` —— 抽取坏了，本断言在空转（08-12 实测 28 处）"
        );
        // 反向自检之二：**函数定位不许大面积退回文件头**。
        // 退回文件头会让位置比较退化成「文件里有没有」——那比本护栏声称的弱，
        // 而且是**静默**变弱的。
        assert!(
            fell_back_to_file_head * 4 <= sites,
            "{fell_back_to_file_head}/{sites} 处定位不到所在函数、退回了文件头 —— \n\
             位置比较已退化成「文件里有没有」，本护栏此刻比它声称的弱。"
        );
        let mut registered: Vec<String> = REMOTE_ONLY
            .iter()
            .map(|(f, n, _)| format!("{f}::{n}"))
            .collect();
        registered.extend(TRIAGE_DEBT.iter().map(|(f, n)| format!("{f}::{n}")));
        // ★ 存量表**只许变短**：等号不是地板。
        // 地板在「变大」这个方向上是瞎的 —— 这个仓因为这件事栽过三次
        // （`shell_lint_registry` 的账逐字：「`≥` 正是它落后三次的成因」）。
        const TRIAGE_DEBT_TODAY: usize = 19;
        assert_eq!(
            TRIAGE_DEBT.len(),
            TRIAGE_DEBT_TODAY,
            "存量表现在 {} 条，登记时是 {TRIAGE_DEBT_TODAY} 条。\n\
             变少 ⇒ 有人真去量了某一处，把这个数一起改小（好事）；\n\
             变多 ⇒ **不许** —— 新增的要么去加本机分支，要么进 `REMOTE_ONLY` 写明为什么够不到。",
            TRIAGE_DEBT.len()
        );
        // ★ 表里不许有**今天已经不是问题**的条目：那是过期的登记，会让下一个人
        //   以为还有欠账没还（与 `P3b` 抓的「理由过期」同一族）。
        let stale: Vec<&String> = registered.iter().filter(|r| !offenders.contains(r)).collect();
        assert!(
            stale.is_empty(),
            "登记表里这些今天已经先分本机了：{stale:?} —— 把它们从表里删掉，并把计数改小。"
        );
        offenders.retain(|o| !registered.contains(o));
        offenders.sort();
        assert!(
            offenders.is_empty(),
            "这些地方先去查远端配置、却没先分本机：\n  {}\n\n\
             对 `<local>` 它们会报「未找到远端配置: \"<local>\"」—— \n\
             一句与真实原因毫无关系的话（本轮同族第五次）。\n\
             两条出路：① 在调用**之前**加一条本机分支，说真实原因；\n\
             ② 若本机结构性够不到这里，进 `REMOTE_ONLY` 逐条写明为什么。\n\
             已登记的：{registered:?}",
            offenders.join("\n  ")
        );
        for (file, name, why) in REMOTE_ONLY {
            assert!(
                why.chars().count() >= 30,
                "`REMOTE_ONLY` 里 {file}::{name} 的理由只有 {} 字 —— 那是占位不是理由",
                why.chars().count()
            );
        }
    }
}
