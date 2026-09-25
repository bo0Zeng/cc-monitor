use super::*;

fn src_root() -> std::path::PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::crate_src_root()
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
            let deals_with_local = body.contains("LOCAL_ORIGIN") || body.contains("\"<local>\"");
            if !deals_with_local {
                offenders.push(format!("{rel}::{fn_name}"));
            }
        }
    }
    // 反向自检之一：**调用点必须真的数到了**。
    // 08-12 实测 28 处；写成地板是因为这个数天天在变，写等号会天天假红
    // （铁律 18：假阳会训练人绕过判据）。但地板要贴着实测，别写个 5 装样子。
    // 🔴 `K-R104`（09-13）：地板 20 → **19**，理由与上面 `TRIAGE_DEBT` 那一条是**同一件事**：
    //    `account_usage` 不再自己去查远端配置（它整条走后端通道，`client_for(origin)`
    //    对 `<local>` 与远端一视同仁）⇒ 这个人群**恰好少一处**。
    //    ⚠ 地板守的是「抽取器还够得到东西」；人群真的少了一个成员时不跟着改，
    //    才是让它替真判据挡枪（`K-G8`）。
    // 🔴 `K-R112`（09-13）：地板 19 → **17**，理由与上面 `K-R104` 那一条**同形**：
    //    `cc_bus.rs::check_cc_bus_agent_online` 与 `tmux.rs::capture_remote_pane`
    //    不再自己去查远端配置（两条都整条走后端通道，`client_for(origin)` 对 `<local>`
    //    与远端一视同仁）⇒ 这个人群**恰好少两处**。
    // 🔴 〔SR1b · 09-24〕地板 17 → **16**：`inproc_dial.rs` 里跳板那一跳的 `connect_via_jump`〔散文墓碑〕（查配置，只服务 SFTP）
    //    随那份文件整份删了（界面进程零 SSH）⇒ 这个人群**恰好少一处**（登记表那一行同拍还掉）。
    assert!(
        sites >= 16,
        "只数到 {sites} 处 `{CALL}` —— 抽取坏了，本断言在空转（08-12 实测 28 处，\
             `K-R104` 09-13 现打 19，`K-R112` 09-13 现打 17，SR1b 09-24 现打 16）"
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
    // K-R56（09-11）：16 → 15，`tmux.rs::tmux_send_keys` 真去分了本机。
    // 🔴 `K-R104`（09-13）：15 → **14**。用量探针那一条
    //    随编排搬上帧面而**真的还掉了**（理由逐字在表里那条注释）。
    //    〔`设计/50`：那一族今天连功能都不在了 —— 这个数**不动**，因为它当时就已经出表了。〕
    //    ★ 这是本表第二次往下走，而「变少 ⇒ 好事」正是它自己报错文案里写的那一句。
    // 〔RW1 · 第四波 09-24〕14 → **12**：`mcp.rs` 远端写 / 删两个分支随「用户文件改经后端写」不再查远端配置。
    // 〔SR1b · 第四波 09-24〕12 → **11**：`inproc_dial.rs` 里的 `connect_via_jump`〔散文墓碑〕（只服务 SFTP 的跳板查配置）随那份文件整份删了。
    // 〔C4c · 第四波 4B〕11 → **10**：账号面那一处查远端配置（`cfg_for`，原住 `accounts.rs`）随那两条命令改走通道删了〔散文墓碑〕。
    const TRIAGE_DEBT_TODAY: usize = 10;
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
    let stale: Vec<&String> = registered
        .iter()
        .filter(|r| !offenders.contains(r))
        .collect();
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
