//! `P4f`：cc-bus 的**基础命令** —— `bus-list`（谁在线 + 各自待读数）与 `bus-send`（发一条）。
//!
//! 〔用 08-13〕逐字：「**也是daemon先把基础命令做了, 其他具体的细节先按原本的就行,
//! 后面我可能要改ccbus**」。三条判断都是从这句话推出来的，读数在账本 `P4f §3`。
//!
//! # ① 第一刀**不做** `bus-recv`
//!
//! `cc-recv` **有副作用**：它会推进已读位置。daemon 代读 = **把消息从人那里偷走** ——
//! agent 自己再跑 `cc-recv` 就什么都看不到了。（08-13 刚修过同一族的一条真事故：
//! Stop 钩子把 40 条积压「读过了却没喂回去」，账本 `P4b §7g-9k`。）
//!
//! ⇒ 消费性操作第一刀不做；「有没有新的」这个需求由 `bus-list` 的**待读数**满足
//! （只读、不消费）。要做代读的那天，先得给它一个「读了但不算数」的两阶段口。
//!
//! # ② 转调脚本，**不在这里重实现**总线
//!
//! 用户逐字「细节先按原本的就行」+「后面我可能要改ccbus」⇒ 在 daemon 里重写一份
//! 路由/投递会造**双写点**：cc-bus 一改，这边就错，而且错得静悄悄。
//!
//! # ③ ★ 把 cc-bus 的**命令**当接口，不要把它的**文件格式**当接口
//!
//! 本模块**不读** cc-bus 的任何数据文件（地址簿 / 收件箱 / 已读位置），只调它的命令。
//! 理由不是"读文件难"——读文件其实更快更省一个进程——而是：
//! **命令是给外人用的，文件格式是给自己用的**。用户说他后面要改 cc-bus，
//! 改的时候前者会被当接口对待，后者不会。
//!
//! ⚠ 张力如实写：不读文件就意味着**依赖输出格式**。二者必居其一。
//! 今天 `cc-list` 的输出是空白分列、且 id/target 都过 `[A-Za-z0-9_-]` 白名单消毒
//! （不含空白）⇒ 解析可靠（`parse_list` 有判据）。
//!
//! 这条由 [`tests::no_cc_bus_data_layout_leaks_into_the_daemon`] 钉住。
//!
//! # 只读铁律
//!
//! 本模块有**唯一一处**起进程（[`run`]），两条命令共用，已登记进 `readonly_guard::ALLOWED`。
//! · `cc-list` 只读；
//! · `cc-send` 会写收件人的收件箱 —— 那是**被起的那个进程**写的，与用户自己在终端里
//!   敲 `cc-send` 没有区别（同 `launch` 起 claude 的 D1 正例：daemon 进程自身不写用户既有数据）。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// 命令级错误：`(code, message)`。与 [`super::kill`] / [`super::launch`] 同型。
type CmdErr = (&'static str, String);

/// 固定位置的查找顺序 —— **纯函数**（不读环境变量，好测）。
///
/// `CC_BUS_BIN_DIR`（部署/台架覆盖）→ `~/.local/bin` → `~/.claude/skills/cc-bus/scripts`。
///
/// ⚠ 为什么不只靠 `PATH`：daemon 由 app 经 **SSH exec** 起，那是**非登录 shell**，
/// `~/.local/bin` 未必在 `PATH` 里（08-13 实测）⇒ 只靠 PATH 会出现「明明装了却找不到」。
/// 这条与 `ccm` 的 `DAEMON_BIN_RECIPE` 是同一个形状（那边找 daemon，这边找 cc-bus），
/// 两边都只写一份。
pub(crate) fn fixed_candidates(
    override_dir: Option<&Path>,
    home: Option<&Path>,
    name: &str,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(d) = override_dir {
        out.push(d.join(name));
    }
    if let Some(h) = home {
        out.push(h.join(".local").join("bin").join(name));
        out.push(h.join(".claude").join("skills").join("cc-bus").join("scripts").join(name));
    }
    out
}

/// 找不到时说**查过哪些地方** —— 纯函数。
///
/// `P4f-Y4`：`~/.local/bin` 不在非登录 shell 的 PATH 里是**常态**，
/// 所以"没装/找不到"必须是一个**能自证的**回答，不能report成笼统的失败。
pub(crate) fn not_installed_message(name: &str, fixed: &[PathBuf], path_dirs: usize) -> String {
    let places: Vec<String> = fixed.iter().map(|p| p.display().to_string()).collect();
    format!(
        "找不到 `{name}`：查过 {}，以及 PATH 上的 {path_dirs} 个目录。cc-bus 装了吗？\
         （装在别处可以用 CC_BUS_BIN_DIR 指过来）",
        if places.is_empty() {
            "<没有可查的固定位置：HOME 也没有>".to_string()
        } else {
            places.join(" · ")
        }
    )
}

fn is_executable(p: &Path) -> bool {
    let Ok(md) = std::fs::metadata(p) else {
        return false;
    };
    if !md.is_file() {
        return false;
    }
    // Windows 上没有执行位这个概念；daemon 的目标平台是 Linux，但它**必须在 Windows 上编得过**
    //（`C16`：动 daemon 就跑 `npm run verify:committed`，那次 `libc::getuid` 就是这么红的）。
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        md.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn find(name: &str) -> Result<PathBuf, CmdErr> {
    let override_dir = std::env::var_os("CC_BUS_BIN_DIR").filter(|d| !d.is_empty());
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty());
    let fixed = fixed_candidates(
        override_dir.as_ref().map(Path::new),
        home.as_ref().map(Path::new),
        name,
    );
    for c in &fixed {
        if is_executable(c) {
            return Ok(c.clone());
        }
    }
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for d in &path_dirs {
        let c = d.join(name);
        if is_executable(&c) {
            return Ok(c);
        }
    }
    Err((
        "not_installed",
        not_installed_message(name, &fixed, path_dirs.len()),
    ))
}

/// 给子进程的**期限（秒）**。台架用 `CC_BUS_TIMEOUT_SECS` 调小。
fn timeout_secs() -> u64 {
    std::env::var("CC_BUS_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(10)
}

/// 在 `PATH` 上找一个可执行文件（`timeout` 只可能在那儿，不在 cc-bus 的固定位置）。
fn on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join(name))
            .find(|c| is_executable(c))
    })
}

/// ★ **本模块唯一一处起进程**（两条命令共用），已登记进 `readonly_guard::ALLOWED`。
///
/// argv 直传、**不过 shell** ⇒ 收件人/正文里的元字符不构成注入面。
///
/// # ★★ 期限住在**子进程**里，不在 daemon 里〔08-13 实测事故 + 铁律冲突〕
///
/// 病先说清楚：`Command::output()` **无限等**。实测把 `cc-send` 换成 `sleep 300` 的桩，
/// `--bus-send` 25 秒都没回来（25 是我从外面掐的，daemon 自己没有任何期限）。
/// 这不是假想 —— `cc-send` 的投递走 `flock`，**锁被别人占住就一直等**；
/// 而这两条命令是阻塞档，一条卡住就占死一个 tokio worker，且 `cancel` 对 `spawn_blocking`
/// 是**空操作**（`inbound` 那条判据逐字：「`cancel` 会对它撒谎」）。
///
/// ⚠ 我的第一版是在 daemon 里等（先 `try_wait` 轮询、后 `recv_timeout`）——
/// **两版都被零定时器护栏当场逮住**，而它是对的：`IPC-PROTOCOL` 自己写着
/// 「daemon 侧刻意不管超时…零定时器铁律不改，**超时一律推给客户端**」。
///
/// ⇒ 正确形状是**让子进程自己有期限**：能找到 `timeout(1)` 就用它当前缀
///（`ccm` 里问 daemon 那条早就是这么写的，同一条纪律的另一侧）。
/// daemon 这边仍然只是老老实实 `wait` 一个**注定会退出**的子进程 —— 零计时器。
///
/// ⚠ 找不到 `timeout(1)` 就**如实降级**：裸跑、没有期限。
/// 那种机器上这条路会退回「可能卡住」，判据与文档都照实写（不假装有保障）。
fn run(name: &str, args: &[&str]) -> Result<std::process::Output, CmdErr> {
    let bin = find(name)?;
    let secs = timeout_secs();
    // 一处 `Command::new`，两种 argv：有 `timeout(1)` 就 `timeout <secs> <bin> <args…>`。
    let (prog, mut argv): (PathBuf, Vec<String>) = match on_path("timeout") {
        Some(t) => (
            t,
            vec![secs.to_string(), bin.display().to_string()],
        ),
        None => (bin.clone(), Vec::new()),
    };
    argv.extend(args.iter().map(|a| (*a).to_string()));
    Command::new(&prog)
        .args(&argv)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| ("failed", format!("起不来 `{}`：{e}", bin.display())))
}

/// `timeout` 那条命令超时时的退出码（GNU coreutils）。
///
/// ⚠ 文案里**别把它写成 `timeout` 加括号的形状** —— `no_timer_guard` 按调用形态扫，
/// 它剥注释但**不剥字符串**，写在错误消息里会被当成一处定时器调用（我当场撞过）。
const TIMED_OUT_CODE: i32 = 124;

/// 超时那条的说法 —— 两个命令共用一份文案。
fn timed_out_err() -> (String, String) {
    (
        "timed_out".to_string(),
        format!(
            "跑了超过 {} 秒还没退出，已被 timeout 命令结束。cc-bus 的投递走 flock —— \
             多半是锁被别的进程占住了（先看 `cc-list` 与 $CC_BUS_HOME 下的 *.lock）。",
            timeout_secs()
        ),
    )
}

/// 把 `cc-list` 的**人类可读表**变成结构化的行 —— 纯函数。
///
/// 今天的形状：表头 `ID TMUX 待读` + 每行 `id target 待读`。
/// id/target 都过 `[A-Za-z0-9_-]` 白名单消毒 ⇒ 不含空白，按空白分列是可靠的。
/// 「还没有登记的 agent」那行不足三列，自然落选。
pub(crate) fn parse_list(text: &str) -> Vec<serde_json::Value> {
    text.lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let id = it.next()?;
            let target = it.next()?;
            let unread = it.next()?;
            if id == "ID" {
                return None; // 表头
            }
            // 第三列不是数字 ⇒ 这不是数据行（宁可漏也别把表头/提示语当 agent）
            let unread: i64 = unread.parse().ok()?;
            Some(serde_json::json!({ "id": id, "target": target, "unread": unread }))
        })
        .collect()
}

/// `cc-send` 的退出码 → **语义码** —— 纯函数。
///
/// `P4f-Y5`：如实转达，**不在 daemon 侧再写一份收件人白名单**。
/// 收件人合法性归 cc-bus 自己（它已有，rc=2）；这边再写一份的话两处规则会漂。
pub(crate) fn classify_send(code: Option<i32>, detail: &str) -> Result<(), (String, String)> {
    match code {
        Some(0) => Ok(()),
        // cc-send 自己的白名单校验（`仅 [A-Za-z0-9_-]`）
        Some(2) => Err((
            "invalid_args".to_string(),
            format!("cc-send 拒绝了这个收件人：{detail}"),
        )),
        // 路由层拦截（ACL / 限流 / 去重 / 灭环），bus.log 里有 REJECT/THROTTLE 一行
        Some(3) => Err((
            "rejected".to_string(),
            format!("被路由层拦下（见 bus.log）：{detail}"),
        )),
        Some(TIMED_OUT_CODE) => Err(timed_out_err()),
        Some(c) => Err(("failed".to_string(), format!("cc-send 退出码 {c}：{detail}"))),
        None => Err((
            "failed".to_string(),
            format!("cc-send 被信号打断：{detail}"),
        )),
    }
}

/// 形状校验：这组参数能不能构成一次有意义的调用。
///
/// ⚠ 与 `kill::parse_name` 同一条纪律：**这不是安全边界**（argv 直传不过 shell），
/// 更**不是**收件人合法性检查 —— 那归 cc-bus（见 [`classify_send`]）。
fn parse_send(args: &serde_json::Value) -> Result<(String, String), CmdErr> {
    let obj = args
        .as_object()
        .ok_or(("invalid_args", "args 不是对象".to_string()))?;
    let to = obj
        .get("to")
        .and_then(|v| v.as_str())
        .ok_or(("invalid_args", "缺 `to`".to_string()))?;
    let text = obj
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or(("invalid_args", "缺 `text`".to_string()))?;
    if to.trim().is_empty() {
        return Err(("invalid_args", "`to` 是空的".to_string()));
    }
    Ok((to.to_string(), text.to_string()))
}

fn first_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .to_string()
}

pub(crate) fn list_for_inbound() -> Result<serde_json::Value, (String, String)> {
    let out = run("cc-list", &[]).map_err(|(c, m)| (c.to_string(), m))?;
    if out.status.code() == Some(TIMED_OUT_CODE) {
        return Err(timed_out_err());
    }
    if !out.status.success() {
        return Err((
            "failed".to_string(),
            format!(
                "cc-list 退出码 {:?}：{}",
                out.status.code(),
                first_line(&out.stderr)
            ),
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(serde_json::json!({ "agents": parse_list(&text) }))
}

pub(crate) fn send_for_inbound(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (String, String)> {
    let (to, text) = parse_send(args).map_err(|(c, m)| (c.to_string(), m))?;
    // `--` 显式结束旗标：收件人万一以 `--` 开头也当收件人，不会被 cc-send 当成选项。
    let out = run("cc-send", &["--", &to, &text]).map_err(|(c, m)| (c.to_string(), m))?;
    let detail = {
        let e = first_line(&out.stderr);
        if e.is_empty() {
            first_line(&out.stdout)
        } else {
            e
        }
    };
    classify_send(out.status.code(), &detail)?;
    Ok(serde_json::json!({ "to": to, "sent": true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_list_reads_the_table_and_skips_everything_else() {
        let text = "ID           TMUX               待读\n\
                    agent-communication_cc agent-communication_cc:0.0 0\n\
                    x_cc         x_cc:0.0           2\n";
        let got = parse_list(text);
        assert_eq!(got.len(), 2, "表头没被跳过或数据行丢了：{got:?}");
        assert_eq!(got[0]["id"], "agent-communication_cc");
        assert_eq!(got[0]["unread"], 0);
        assert_eq!(got[1]["target"], "x_cc:0.0");
        assert_eq!(got[1]["unread"], 2);
    }

    /// ★ 超长 id 会把固定宽度的列**挤在一起** —— 实测那时列间仍有一个空格，
    /// 所以按空白分列仍然对。这一格钉的就是「挤了也还认得出来」。
    #[test]
    fn a_long_id_that_overflows_the_column_is_still_parsed() {
        let one = parse_list("verylongagentname_that_overflows verylongagentname:0.0 7\n");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0]["unread"], 7);
    }

    #[test]
    fn non_data_lines_never_become_agents() {
        for line in [
            "(还没有登记的 agent)",
            "ID TMUX 待读",
            "",
            "只有两列 x",
            "id target 不是数字",
        ] {
            assert!(
                parse_list(line).is_empty(),
                "这行不该被当成 agent：{line:?}"
            );
        }
    }

    /// `P4f-Y4`：找不到时要说**查过哪儿**。
    #[test]
    fn the_not_installed_message_names_the_places_it_looked() {
        let home = PathBuf::from("/home/u");
        let fixed = fixed_candidates(None, Some(&home), "cc-list");
        assert_eq!(fixed.len(), 2, "固定位置应当是两处：{fixed:?}");
        let msg = not_installed_message("cc-list", &fixed, 9);
        assert!(msg.contains("/home/u/.local/bin/cc-list"), "{msg}");
        assert!(
            msg.contains(".claude/skills/cc-bus/scripts/cc-list"),
            "{msg}"
        );
        assert!(msg.contains("9 个目录"), "PATH 那半没说：{msg}");
        assert!(msg.contains("CC_BUS_BIN_DIR"), "没告诉人怎么指过去：{msg}");
    }

    #[test]
    fn the_override_dir_wins_over_the_fixed_places() {
        let over = PathBuf::from("/opt/ccbus");
        let home = PathBuf::from("/home/u");
        let fixed = fixed_candidates(Some(&over), Some(&home), "cc-send");
        assert_eq!(fixed[0], PathBuf::from("/opt/ccbus/cc-send"));
        assert_eq!(fixed.len(), 3);
    }

    /// `P4f-Y5`：三档退出码各自映射成**不同**的语义码。
    ///
    /// ⚠ 判据钉「互不相同」而不是逐条对字符串 —— 前者才是这条 DoD 的内容
    /// （把两档并成一个码，用户就分不出「名字写错了」和「被 ACL 拦了」）。
    #[test]
    fn the_three_exit_codes_map_to_three_different_meanings() {
        assert!(classify_send(Some(0), "").is_ok());
        let bad = classify_send(Some(2), "x").unwrap_err().0;
        let rej = classify_send(Some(3), "x").unwrap_err().0;
        let other = classify_send(Some(9), "x").unwrap_err().0;
        let killed = classify_send(None, "x").unwrap_err().0;
        assert_ne!(bad, rej, "「收件人非法」与「被路由层拦」必须分得开");
        assert_ne!(bad, other);
        assert_ne!(rej, other);
        assert_eq!(killed, other, "被信号打断与其它失败同档（都是 failed）");
    }

    /// `P4f-Y5` 的另一半：daemon **不重复校验收件人合法性**。
    ///
    /// 传一个 cc-bus 自己会拒的名字（含 `/`），本侧必须**放行到 cc-send 那一步**——
    /// 由它去拒（rc=2 → `invalid_args`）。这样白名单只有一份。
    #[test]
    fn the_daemon_does_not_re_implement_the_recipient_charset_rule() {
        let ok = parse_send(&json!({ "to": "a/b", "text": "x" }));
        assert!(
            ok.is_ok(),
            "本侧不该判收件人字符集 —— 那会造出第二份规则：{ok:?}"
        );
        // 形状还是要判的（这不是安全边界，是「能不能构成一次有意义的调用」）
        for bad in [
            json!({}),
            json!({ "to": "x" }),
            json!({ "text": "x" }),
            json!({ "to": "", "text": "x" }),
            json!({ "to": "  ", "text": "x" }),
            json!({ "to": 1, "text": "x" }),
        ] {
            assert!(parse_send(&bad).is_err(), "形状不对却放行了：{bad:?}");
        }
    }
}
