//! ★ **每一处远端执行都要申报命令的来历**〔audit-0805 08-07，Phase G 第 41 件〕。
//!
//! # 接线层又一次是空的
//!
//! 引用原语这一侧钉得很密：`shell_quote` 的**实现**只有一个家
//! （`quote_singleton_guard`），逃逸形态四种写法都认。个别调用点也有自己的行为判据
//! （`cc_bus::send_cmd_makes_free_text_one_word` / `hooks_diag` 钉自己那处用常量）。
//!
//! **但没有一条判据在数「一共有多少处远端执行、每一处的命令串从哪来」。**
//! 08-07 实测：新增一处 `connect_and_exec_cmd(cfg, &format!("echo {note} > …"))`
//! ——自由文本直接拼进命令、不过引用——全仓 **975 条判据一条不红**。
//!
//! ⇒ 这正是 F+ 第二问反复报的那个形状：**判据的注意力跟着「哪里好写」走，
//! 纯函数层（引用怎么写）钉满，接线层（谁在调、拿什么调）为零。**
//!
//! # 钉法
//!
//! 人群**从源码派生**（所有 `connect_and_exec_cmd(` 调用点），每一处申报命令来历：
//! `Const` / `Builder(名)` / `Quoted`。**默认拒绝**：新增一处不申报就红。
//! 申报不是白申报 —— 三类各自有机检：`Const` 要求实参是全大写常量、
//! `Builder` 要求外层函数体里真调了那个构造器、`Quoted` 要求外层函数体里真有 `shell_quote(`。
//!
//! # 它守什么、不守什么
//!
//! **守**：新增执行点没人认领 · 申报了 `Quoted` 却没引用 · 申报了构造器却没调它。
//! **不守**：① 引用**用对了没有**（那是 `shell_quote` 自己的判据与各调用点的行为判据）；
//! ★ ②「构造器被调用了、但命令另拼一份」看不见 —— 机检只要求构造器出现在函数体里，
//!    不卡 `cmd = builder(..)` 那个形状（`account_usage` 里是 `match probe_command_for(..)`，
//!    卡形状会把合法写法判红）。这是**刻意的取舍**，不是没想到；
//! ③ 一个函数里有多处执行、来历各不相同（人群键是「文件::函数」）；
//! ④ `PassThrough` **不追调用链**：`cc_bus::exec_read` 的三个调用方各走 `build_*_cmd`，
//!    那是它们自己那条 singleton 判据在守，本条只确认转发者自己不构造；
//! ⑤ 走 SFTP / 本机 `Command` 的路（本条只管 `connect_and_exec_cmd` 这一个扼流点）。
//!    ★〔devbench F10c〕**SFTP 那半已经有人接了**：`remote_write_registry` 按「谁拿得到
//!    SFTP 会话」取样，接的正是本条划出去的这道缝。
//!    ★★ **订正〔P3t-Y2，08-11〕：这里原本写「`Command` 那半仍无人接」——那句话是错的。**
//!    `write_site_registry::spawn_sites::every_local_spawn_is_declared` 就在接它
//!    （人群 = `src/` 整棵树 + `build.rs` 里所有 `Command::new`，默认拒绝）。
//!    实测：本件新加一处 `Command::new("bash")` 忘了申报，**当场被那条判红**。
//!    ⇒ 这条头注自己就是「注释声称的缝比真实的缝大」的样本；说有缝而其实有人守，
//!    与说没缝而其实有缝一样坏 —— 它会让下一个人去补一张已经存在的表。

#[cfg(test)]
mod tests {
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
        ("cc_bus.rs", "fetch_remote_cc_bus", Origin::Const, "`CC_BUS_CAT_CMD`：读两张 tsv，零插值"),
        ("hooks_diag.rs", "diagnose_remote_cc_bus_hooks", Origin::Const, "`REMOTE_HOOKS_CMD`：只读探测"),
        ("mcp.rs", "fetch_remote_claude_json", Origin::Const, "`CMD`：读远端 ~/.claude.json"),
        ("ccm_probe.rs", "probe_ccm_cli", Origin::Const, "`CCM_PROBE_CMD`：字面量，零插值。P3t-Y2 起本机探针与它**共用同一个常量**"),
        ("sftp.rs", "probe_remote_arch", Origin::Const, "`\"uname -m\"` 直接当实参"),
        // ── 受控构造器（构造器自己带校验/引用，各有行为判据）
        ("cc_bus.rs", "check_cc_bus_agent_online", Origin::Builder("build_online_cmd"), "id 过白名单"),
        ("pubkey.rs", "push_public_key", Origin::Builder("build_authorized_keys_cmd"), "公钥经 shell_quote"),
        ("tmux.rs", "capture_remote_pane", Origin::Builder("build_capture_pane_cmd"), "target 过 Gate（`tmux_daemon_gate_guard`）"),
        ("tmux.rs", "kill_remote_tmux", Origin::Builder("build_kill_session_cmd"), "同上"),
        ("tmux.rs", "tmux_send_keys", Origin::Builder("build_send_keys_remote_cmd"), "同上"),
        ("account_usage.rs", "account_usage", Origin::Builder("probe_command_for"), "载荷与会话名都在构造器里引用"),
        // ── 本函数里拼，但自由文本过了 shell_quote
        ("remote_history.rs", "run_list_query", Origin::Quoted, "daemon 路径经引用后拼 args"),
        ("remote_history.rs", "stream_read_remote_session", Origin::Quoted, "daemon 路径与 jsonl 路径各引用一次"),
        ("ssh_source.rs", "connect_and_exec", Origin::Quoted, "daemon 路径经引用"),
        ("ssh_source.rs", "fetch_snapshot", Origin::Quoted, "daemon 路径与目标路径各引用一次"),
        ("tmux.rs", "list_remote_tmux", Origin::Quoted, "唯一插值是常量 `TMUX_LS_FMT`（双写点由 `tmux.rs`/daemon 对拍守）；\
          本函数不吃自由文本 —— 归 Quoted 是因为它在函数里拼，机检只要求「拼的地方要么有引用、要么插的是常量」"),
        // ── 只转发，不构造（命令来自调用方）
        ("ssh_source.rs", "connect_and_exec_cmd", Origin::PassThrough, "★ 这是原语**自己的定义**，不是调用点"),
        ("cc_bus.rs", "exec_read", Origin::PassThrough, "`cmd: &str` 入参；真来历在三个调用方（inbox/send/spawn，各走 build_*_cmd）"),
        ("acct_iso_deploy.rs", "exec_collect", Origin::PassThrough, "`cmd: &str` 入参；来历在调用方"),
    ];

    fn src_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
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
    const ERROR_COMBINATORS: &[&str] =
        &["Err(", "map_err", "ok_or_else", "unwrap_or_else", "expect("];

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
            found.len() >= 12,
            "全树只找到 {} 处 `connect_and_exec_cmd(` 调用（08-07 实测 16）—— 抽取器坏了，本条此刻无效",
            found.len()
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
                        || (!arg.is_empty()
                            && arg.chars().all(|c| c.is_ascii_uppercase() || c == '_'))
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
                    // `account_usage` 里是 `let cmd = match probe_command_for(..) { .. }`，
                    // 卡形状会把合法写法判红。**代价写在头注「不守」里**：构造器被调用、
                    // 而命令另拼一份，本条看不见。
                    assert!(
                        body.contains(&format!("{b}(")),
                        "`{f}::{n}` 申报成走构造器 `{b}`（{why}），但函数体里根本没调它（实参 `{arg}`）—— \
                         构造器改名了？还是有人把命令改成就地拼了？后者正是本条要拦的。"
                    );
                }
                Origin::Quoted => {
                    per_class[2] += 1;
                    assert!(
                        body.contains("shell_quote(") || body.contains("TMUX_LS_FMT"),
                        "`{f}::{n}` 申报成「在本函数里拼但引用了」（{why}），实参 `{arg}`，\
                         而函数体里既没有 `shell_quote(`、插的也不是那个受守的常量 —— \
                         自由文本正裸着进远端命令。"
                    );
                }
                Origin::PassThrough => {
                    per_class[3] += 1;
                    let is_param = body.contains(&format!("{arg}: &str"));
                    let is_definition = *f == "ssh_source.rs";
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
        for (i, name) in ["Const", "Builder", "Quoted", "PassThrough"]
            .iter()
            .enumerate()
        {
            assert!(
                per_class[i] >= 1,
                "`{name}` 这一类今天一个样本都没有（08-07 实测 5/6/5/3）—— \
                 那一支的机检在空转，而它看起来照样绿。真收敛掉了就把这条自检一起改。"
            );
        }
    }
}
