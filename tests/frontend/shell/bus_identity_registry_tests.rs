use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// cc-bus 的脚本清单 —— 走 `guard_core::shell_scripts`，**不裸 `read_dir`**。
///
/// ⚠ `scanning_guard_registry` 逮住过我这里的第一版（裸遍历）。它的理由是
/// 「判据在自己的语料里找到自己 ⇒ 恒绿」——
/// 本条扫的是 **shell 脚本目录**、判据自己是 `.rs`，自含不可能发生；
/// 但**规则不为个案开口子**：走共用助手，顺带白拿它的过滤（跳二进制、认 shebang）。
fn cc_bus_scripts() -> Vec<String> {
    let want = format!("shared{}cc-bus{}scripts{}", "/", "/", "/");
    guard_core::shell_scripts(&repo_root())
        .into_iter()
        .filter(|p| p.replace('\\', "/").contains(&want))
        .collect()
}

/// **不需要身份核对的**（文件名, 为什么）。今天为空 —— 刻意保留这个形态：
/// 下一个真有正当理由的人有地方落，而不用去改判据本身。
const EXEMPT: &[(&str, &str)] = &[];

/// 一行是不是「用变量点名 tmux」。
///
/// 形状：`-t "=$x"` / `-t '=$x'` / `-t "=${x}"`。**必须带 `=`** ——
/// 那是 tmux 的精确名匹配前缀，也正是"按名字下手"的标志。
fn targets_by_variable(line: &str) -> bool {
    for pat in ["-t \"=$", "-t '=$"] {
        if line.contains(pat) {
            return true;
        }
    }
    false
}

#[test]
fn every_name_based_tmux_target_in_cc_bus_verifies_identity() {
    let files = cc_bus_scripts();
    assert!(
        files
            .iter()
            .any(|f| f.replace('\\', "/").ends_with("/scripts/cc-kill")),
        "列到的 cc-bus 脚本里没有 `cc-kill` —— 清单取法坏了，本断言在空转：{files:?}"
    );
    let mut targeting: Vec<String> = Vec::new();
    let mut unverified: Vec<String> = Vec::new();
    for rel in &files {
        let path = repo_root().join(rel);
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let raw = std::fs::read_to_string(&path).unwrap_or_default();
        // 只看生产段：注释里写着「原来是 `-t "=$id"`」的那些话不算数
        //（本仓栽过四次「判据读到自己的散文」）。
        let prod = guard_core::strip_hash_comment_lines(&raw);
        if !prod.lines().any(targets_by_variable) {
            continue;
        }
        targeting.push(name.clone());
        if EXEMPT.iter().any(|(f, _)| *f == name) {
            continue;
        }
        // 身份核对的证据：这个文件**真的去问了那一下** ——
        // 同一行里既有 `display-message` 又有 `pane_pid`。
        //
        // ⚠ 证据第一版只要求「文件里提到 pane_pid」，**变异当场存活**：
        //   把 `cc-kill` 的核对换成恒等（`_have_pid="$_want_pid"`）之后，
        //   文件里**别处**仍有 `pane_pid`（收进程树那句 `list-panes … '#{pane_pid}'`）
        //   ⇒ 判据照样绿。★ 证据要选**只有做了那件事才会出现**的东西。
        let verified = prod
            .lines()
            .any(|l| l.contains("display-message") && l.contains("pane_pid"));
        if !verified {
            unverified.push(name);
        }
    }
    targeting.sort();
    // 正控：`cc-kill` 按名字点名 tmux（收会话那一下），它必须被认出来。
    assert!(
        targeting.iter().any(|n| n == "cc-kill"),
        "按名字点名 tmux 的脚本里没认出 `cc-kill` —— 抽取坏了，本断言在空转：{targeting:?}"
    );
    assert!(
        unverified.is_empty(),
        "这些 cc-bus 脚本拿 id 去点名 tmux，却**没有核身份**：{unverified:?}\n\
             ⇒ cc-bus 的地址是名字型的，而名字会被重用（`cc-spawn` 按目录基名取名）。\n\
             不核身份的后果 08-13 实测过三次：敲门打进陌生人的屏幕 · **杀掉无辜进程与会话** ·\n\
             假报「活」。\n\
             核法：拿 `agents.tsv` 第 2 列那个**完整地址**去问 `#{{pane_pid}}`，与第 4 列比。\n\
             ⚠ 别用 `list-panes -t \"=<id>\"` —— 那取的是**当前窗口**的 pane，\n\
             用户开个新窗口就假阳性（`cc-kill` 第一版栽过；假阳性比不查更坏）。\n\
             实在不需要核的，登记进 `EXEMPT` 并写明理由。"
    );
}

/// ★ 反向自检：**这条判据真的能红**。
///
/// 拿一段"按名字点名、且不提 pane_pid"的假脚本喂给同一个判定，它必须被判成未核。
/// 没有这一格的话，`targets_by_variable` 哪天被改坏（比如永远回 `false`），
/// 上面那条会安静地全绿 —— 那正是本仓「判据在空转」那一族。
#[test]
fn the_check_itself_would_catch_a_new_unverified_site() {
    let bad = "id=\"$1\"\ntmux kill-session -t \"=$id\" 2>/dev/null\n";
    assert!(
        bad.lines().any(targets_by_variable),
        "判定认不出「按变量点名 tmux」—— 上面那条此刻是空转的"
    );
    let bad_prod = guard_core::strip_hash_comment_lines(bad);
    assert!(
        !bad_prod
            .lines()
            .any(|l| l.contains("display-message") && l.contains("pane_pid")),
        "反向样本自己就带着身份核对 —— 它证明不了什么"
    );
    // 注释里的那一行不许算数（判据的语料不许混进散文）
    let only_comment = "# 这里原来写的是 tmux kill-session -t \"=$id\"\necho hi\n";
    let prod = guard_core::strip_hash_comment_lines(only_comment);
    assert!(
        !prod.lines().any(targets_by_variable),
        "注释里的写法被当成了真的调用"
    );
}
