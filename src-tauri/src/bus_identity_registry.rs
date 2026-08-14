//! **cc-bus 里凡是「拿 id 去点名 tmux」的地方，都要先核身份**〔08-13〕。
//!
//! # 这一族今天出了六处，其中三处是真事故
//!
//! cc-bus 的地址是**名字型**的（`proj_cc:0.0`），而名字会被重用 ——
//! `cc-spawn` 就按目录基名取会话名。于是「同名会话在」被当成了「它还活着」：
//!
//! | 处 | 症状 |
//! |---|---|
//! | 敲门（`cc-bus-lib.sh`） | 敲门文字（带 Enter）**打进陌生占用者的屏幕** |
//! | `cc-kill` | **杀掉无辜进程与会话**（`kill -9` 整棵树 + 删收件箱） |
//! | `cc-agents` | 假报「活」 |
//!
//! 另外三处不是事故但同一个问题（`bus-list` / `bus-send` 的三态 · UI 那盏在线灯），
//! 全在 monitor / daemon 侧，用的是同一份证据。
//!
//! # 唯一的证据：登记时记下的 pane 根进程 pid
//!
//! `agents.tsv` 第 4 列（08-13 加）。核法是：拿**登记的完整地址**（第 2 列，`sess:win.pane`）
//! 去问 `#{pane_pid}`，与第 4 列比。
//!
//! ⚠ 用**完整地址**而不是 `list-panes -t "=<id>"` —— 后者取的是**当前窗口**的 pane，
//! 用户在自己 agent 的会话里开个新窗口就会假阳性（`cc-kill` 第一版栽过，
//! 而**假阳性比不查更坏**：它会让人以为这道保护坏了，进而删掉它）。
//!
//! # 本登记表钉什么
//!
//! 「按变量点名 tmux」的脚本，**要么**同一个文件里做了身份核对（提到 `pane_pid`），
//! **要么**登记在 [`EXEMPT`] 里写明为什么不需要。
//!
//! ★ 为什么不扫「自报身份」那两处（`cc-register` / `cc-whoami`）：它们用的是
//! **自己的** `$TMUX_PANE`，问的是「我是谁」而不是「那个名字是不是它」——
//! 天然不在这个形状里，而且它们也没有可核的对象。**判据的人群要等于它真正证明的那件事。**
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf()
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

    /// 今天有几个文件按变量点名 tmux。**相等断言**，不是地板。
    ///
    /// 变多 ⇒ 新增了一处「拿名字动手」的地方，必须有人看一眼它核没核身份；
    /// 变少 ⇒ 多半是抽取坏了（或那处被删了，那也该有人知道）。
    const TARGETING_FILES_TODAY: usize = 3;

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
            files.len() >= 8,
            "只列到 {} 个 cc-bus 脚本 —— 清单取法坏了，本断言在空转：{files:?}",
            files.len()
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
        assert!(
            targeting.len() >= 2,
            "只扫到 {} 个按名字点名 tmux 的脚本 —— 抽取坏了，本断言在空转：{targeting:?}",
            targeting.len()
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
        assert_eq!(
            targeting.len(),
            TARGETING_FILES_TODAY,
            "按名字点名 tmux 的脚本从 {TARGETING_FILES_TODAY} 个变成了 {} 个：{targeting:?}\n\
             **这不是改数字了事** —— 先回答：新增的那处核身份了吗？\n\
             （今天这三个是 `cc-agents` / `cc-bus-lib.sh` / `cc-kill`。）",
            targeting.len()
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
}
