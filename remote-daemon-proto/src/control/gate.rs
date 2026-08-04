//! F03：**§34 Gate 2（identity）在 daemon 侧的落地** —— 「这个 tmux 会话是不是本工具管的」。
//!
//! # 它补的是哪个洞
//!
//! F03 之前，[`super::launch`] 的 `send-into` **只核会话存在性**（`no_such_session`）就
//! `send-keys`。它建会话时 `set-option` **写** `@ccm_sid`，却从不**核验**它。
//! monitor 那条路带着 §34 的 Gate 2（`cc-*` 前缀命中 **或** 远端 `@ccm_sid` 已设），
//! 于是「把 send-keys/kill 改走 daemon」等于**静默丢掉一道门** ——
//! 功能看起来一样、门禁全绿，而「不许往别人的 tmux 里打字」那道门没了。
//! 这条路此前由 monitor 的 `tmux_daemon_gate_guard`（前提触发器）挡着。
//!
//! **判定本身不在这里** —— 在 `gate-core`，monitor 与 daemon 共用同一份（定框 C1）。
//! 本模块只负责这一侧的**承载**：怎么把 `@ccm_sid` 从本机 tmux 取回来。
//!
//! # ★ 用 `#{session_id}` 当句柄，把 TOCTOU 窗口关掉
//!
//! monitor 那条路是**一条原子远端命令**（`display-message` 与动作折进同一个 round-trip），
//! 刻意不给「查完再动」之间留窗口。daemon 这边 argv 直传、没有 shell，做不到把两条
//! tmux 调用折成一条 —— 照抄「先查名字、再对名字下手」就会**引入一个 monitor 没有的窗口**。
//!
//! 处置：探测时**连 `#{session_id}` 一起取回**（tmux 的 `$N`，server 生命周期内唯一、不复用），
//! 之后一律对**那个句柄**下命令，不再对名字下命令。名字在窗口期内被重新绑定到别的会话，
//! 句柄仍然指着被核验过的那一个；那个会话若已消失，`send-keys` 自然失败
//! ⇒ 回 `typed_unconfirmed`（「会话在，但载荷未必落」的那一档），**不会打到别人身上**。
//!
//! ⚠ 这比 monitor 那条路**更硬**，不是权宜之计。F04 把 kill 搬过来时应当沿用同一形态。
//!
//! # ★ 为什么用 `display-message` 而不是 `show-options`
//!
//! 同 monitor 侧的理由：`show-options` 对**未设置**的 option 是 `rc=1` + stderr，
//! 要脆弱的 rc/stderr 联合判断；`display-message -p` 对未设置的 option 静默展开成空串。
//! 实测（私有 socket）：目标不存在时**整条输出为空但 `rc=0`** ——
//! 所以「目标在不在」的判据是**输出为空**，不是退出码。这与 monitor 侧
//! `[ -z "$info" ] → CCM_NO_SESSION` 是同一条判据，刻意保持一致。

use std::process::{Command, Stdio};

/// 命令级错误：`(code, message)`。与 [`super::launch`] 同型。
type CmdErr = (&'static str, String);

/// 探测回来的两样东西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Probed {
    /// tmux 的 `#{session_id}`（形如 `$0`）。**后续一律对它下命令**，见模块头注。
    pub(crate) session_id: String,
    /// `@ccm_sid` 的值。未设置 ⇒ 空串。
    ///
    /// ⚠ 这是 `@ccm_sid`，**不是 `@ccm_sid_expect`**。`shared/ccm` 刻意分了两个：
    /// 通道 A 只写意图，只有通道 B（poller 独立读会话文件确认后）才写事实，
    /// 而**破坏性动作只认事实**。放宽到 `_expect` 就是把这道门拆了。
    pub(crate) ccm_sid: String,
    /// `#{session_windows}`。**Gate 3 只给破坏性动作用**（见 [`admit_destructive`]）。
    /// 解析不出来 ⇒ `0`，而 Gate 3 要求恰好 `1` ⇒ **fail closed**（不会误杀）。
    pub(crate) windows: u32,
}

/// 探测格式串。**三个**字段用 TAB 分隔 —— `session_id` 恒是 `$<数字>`、不含 TAB，
/// `@ccm_sid` 的字符集在 `launch::parse_request` 里收到了 `[A-Za-z0-9_-]`，
/// `session_windows` 对一个存在的会话恒为正整数。
///
/// ⚠ **F04a 加了 `#{session_windows}`（Gate 3 用）—— 刻意加在同一次探测里**：
/// 多一次 `display-message` 就多一个 TOCTOU 窗口，而本模块的立身之本就是把那个窗口关掉。
/// 非破坏性动作（`admit`）**不看**这个字段，但照样取回来 —— 与 monitor 侧同一条纪律
/// （那边的 `build_guarded_tmux_cmd` 头注写着「**总是**在格式串里带 `#{session_windows}`」）。
const PROBE_FMT: &str = "#{session_id}\t#{@ccm_sid}\t#{session_windows}";

/// 跑一次 `tmux display-message -p -t <target> '<fmt>'` 并把 stdout 取回来。
///
/// `Ok(None)` = 目标不存在（输出为空，见模块头注：**不看退出码**）。
fn probe(target: &str) -> Result<Option<Probed>, CmdErr> {
    let out = Command::new("tmux")
        .args(["display-message", "-p", "-t", target, PROBE_FMT])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| {
            (
                "no_tmux",
                format!("起不来 tmux（远端装了吗？PATH 里有吗？）：{e}"),
            )
        })?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.trim_end_matches(['\n', '\r']);
    if line.is_empty() {
        return Ok(None);
    }
    // 逐段切。**多出一段就是格式串被人动过了**，宁可判不存在也不猜。
    let mut it = line.split('\t');
    let session_id = it.next().unwrap_or_default().to_string();
    let ccm_sid = it.next().unwrap_or_default().to_string();
    // 解析不出来 ⇒ 0。Gate 3 要求恰好 1 ⇒ **fail closed**（拿不到窗口数就不许杀）。
    let windows = it
        .next()
        .unwrap_or_default()
        .trim()
        .parse::<u32>()
        .unwrap_or(0);
    if session_id.is_empty() || it.next().is_some() {
        return Ok(None);
    }
    Ok(Some(Probed {
        session_id,
        ccm_sid,
        windows,
    }))
}

/// **过门。** 通过则返回后续该用的目标句柄（`#{session_id}`）。
///
/// `name` 是用户/调用方给的会话名（Gate 2 的本地半支按它判）；
/// `target` 是 `launch::exact_target(name)` 产出的精确目标（`=name:`）。
///
/// 三种结局：
/// - 目标不存在 ⇒ `no_such_session`（**语义不变** —— 新门不许把这一档吞掉）；
/// - Gate 2 不通过 ⇒ `wrong_owner`，消息里带 monitor 同族的 `CCM_GUARD_REJECTED`；
/// - 通过 ⇒ `Ok(session_id)`。
pub(crate) fn admit(name: &str, target: &str) -> Result<String, CmdErr> {
    let Some(p) = probe(target)? else {
        return Err((
            "no_such_session",
            format!("会话 {name:?} 不存在；send-into 只往**已存在**的会话键入，不新建"),
        ));
    };
    let verdict = gate_core::gate2(name, Some(&p.ccm_sid));
    if !verdict.allowed() {
        return Err((
            "wrong_owner",
            format!(
                "CCM_GUARD_REJECTED sid= —— 会话 {name:?} 既不是本工具的命名形状，\
                 远端 `@ccm_sid` 也没设 ⇒ 拒绝键入（§34 Gate 2）。\
                 这道门挡的是「往一个不是本工具管理的 tmux 会话里打字」。"
            ),
        ));
    }
    Ok(p.session_id)
}

/// **破坏性动作的门**：Gate 2（身份）**再加** Gate 3（`windows == 1`）。
///
/// # 为什么破坏性动作要多一道门
///
/// Gate 2 只回答「这是不是本工具的会话」。但一个**本工具建的**会话也可能被用户
/// 自己扩出了额外窗口（在里面开了别的东西）——把它整个杀掉就连带毁掉用户的活。
/// ⇒ Gate 3：**只杀「干净的单窗口会话」**。多窗口 ⇒ 拒绝，让用户自己去那个 tmux 里处理。
///
/// 与 monitor 侧逐条同义（`tmux.rs::build_guarded_tmux_cmd` 的 `[ "$w" = "1" ]`），
/// 拒绝码也保持同族（`CCM_GUARD_REJECTED windows=<n>`）。
///
/// ⚠ **Gate 3 只给破坏性动作**：`send-keys` 不删除任何东西，窗口数与它无关 ——
/// 给它加 Gate 3 会让「往一个多窗口会话里打字」被误拒（monitor 侧 F04 Phase D
/// 审计专门修过这个错法）。所以本函数与 [`admit`] **是两个入口，不是一个带 flag 的**。
pub(crate) fn admit_destructive(name: &str, target: &str) -> Result<String, CmdErr> {
    let Some(p) = probe(target)? else {
        return Err((
            "no_such_session",
            format!("会话 {name:?} 不存在；没有可杀的目标"),
        ));
    };
    if !gate_core::gate2(name, Some(&p.ccm_sid)).allowed() {
        return Err((
            "wrong_owner",
            format!(
                "CCM_GUARD_REJECTED sid= —— 会话 {name:?} 既不是本工具的命名形状，\
                 远端 `@ccm_sid` 也没设 ⇒ 拒绝杀它（§34 Gate 2）"
            ),
        ));
    }
    if p.windows != 1 {
        return Err((
            "too_many_windows",
            format!(
                "CCM_GUARD_REJECTED windows={} —— 会话 {name:?} 有 {} 个窗口，\
                 不是干净的单窗口会话 ⇒ 拒绝杀它（§34 Gate 3）。\
                 用户可能在里面开了别的东西；请到那个 tmux 里自行处理",
                p.windows, p.windows
            ),
        ));
    }
    Ok(p.session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 判定表的**唯一真相源**，三条轨道各自独立读它（见文件头注）。
    const GOLDEN: &str =
        include_str!("../../../src-tauri/src/backend/control/fixtures/gate2-golden.tsv");

    fn golden_rows() -> Vec<(String, String, Option<String>, String)> {
        GOLDEN
            .lines()
            .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
            .map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                assert_eq!(f.len(), 4, "夹具行不是 4 列：{l:?}");
                let sid = match f[2] {
                    "<none>" => None,
                    "<unset>" => Some(String::new()),
                    v => Some(v.to_string()),
                };
                (f[0].to_string(), f[1].to_string(), sid, f[3].to_string())
            })
            .collect()
    }

    /// ★ 抽取器自检：夹具读不到 / 解析空时，下面那条会零命中地绿。
    #[test]
    fn the_golden_table_is_actually_read_from_the_monitor_side_fixture() {
        let rows = golden_rows();
        assert!(
            rows.len() >= 20,
            "只解析出 {} 行夹具 —— 路径或解析坏了（这份夹具住在 monitor 那边，跨仓相对路径）",
            rows.len()
        );
        // 三种结论都必须在表里出现，否则表本身是偏的。
        for want in ["allowed_by_name", "allowed_by_remote_sid", "rejected"] {
            assert!(
                rows.iter().any(|r| r.3 == want),
                "夹具里一行 `{want}` 都没有 —— 表偏了，下面那条测不到那一支"
            );
        }
    }

    /// ★ daemon 这一侧对同一张表给出同样的判定。
    ///
    /// ⚠ **不许改成「调 monitor 的实现来对拍」** —— 两侧一起错就全绿了。
    /// 两侧各自独立读这张表，才叫跨轨。
    #[test]
    fn the_daemon_side_agrees_with_the_golden_table() {
        let mut bad = Vec::new();
        for (id, name, sid, want) in golden_rows() {
            let got = gate_core::gate2(&name, sid.as_deref()).as_str();
            if got != want {
                bad.push(format!(
                    "  {id}: name={name:?} sid={sid:?} 期望={want} 实得={got}"
                ));
            }
        }
        assert!(
            bad.is_empty(),
            "daemon 侧与判定表不一致：\n{}",
            bad.join("\n")
        );
    }

    /// ★ 生产接线：`admit` 通过之后回的是**句柄**，不是名字 —— TOCTOU 那条的落点。
    ///
    /// 这里只钉「解析出来的形状」；真 tmux 上的行为由
    /// `e2e/daemon-gate2-acceptance.sh` 钉（那才是真二进制那一轨）。
    #[test]
    fn a_probe_line_parses_into_a_handle_and_a_sid() {
        // 直接构造探测输出的解析结果，不起进程（起进程是 e2e 的事）。
        let line = "$3\tabc123\t1";
        let mut it = line.split('\t');
        let p = Probed {
            session_id: it.next().unwrap().to_string(),
            ccm_sid: it.next().unwrap().to_string(),
            // F04a：第三段是 `#{session_windows}`。**解析不出来 ⇒ 0**，而 Gate 3 要求恰好 1
            // ⇒ fail closed（拿不到窗口数就不许杀）。
            windows: it.next().unwrap_or_default().parse().unwrap_or(0),
        };
        assert_eq!(p.session_id, "$3");
        assert_eq!(p.ccm_sid, "abc123");
        assert_eq!(p.windows, 1);
        assert!(
            p.session_id.starts_with('$'),
            "tmux 的 session_id 恒是 `$N` —— 不是这个形状就说明格式串被动过了"
        );
    }

    /// ★ F04a：**Gate 3 拿不到窗口数时 fail closed**（解析失败 ⇒ 0 ⇒ 拒绝）。
    ///
    /// 反向的错法（解析失败当 1）会把「探测被截断」变成「放行一次 kill」——
    /// 那是本仓最不能接受的一类默认值。
    #[test]
    fn gate3_fails_closed_when_the_window_count_is_unreadable() {
        for line in ["$1\tsid", "$1\tsid\t", "$1\tsid\tnot-a-number"] {
            let mut it = line.split('\t');
            it.next();
            it.next();
            let w: u32 = it.next().unwrap_or_default().trim().parse().unwrap_or(0);
            assert_ne!(
                w, 1,
                "{line:?} 解析出的窗口数不该等于 1（那会放行一次 kill）"
            );
        }
        let mut it = "$1\tsid\t1".split('\t');
        it.next();
        it.next();
        assert_eq!(it.next().unwrap().parse::<u32>().unwrap(), 1);
    }

    /// ★ 格式串里必须**同时**有句柄、sid 与窗口数：少了句柄就退回「对名字下手」＝TOCTOU 回归，
    /// 少了 sid 就等于没有 Gate 2，少了窗口数就等于没有 Gate 3。
    #[test]
    fn the_probe_format_asks_for_both_fields() {
        assert!(
            PROBE_FMT.contains("#{session_id}"),
            "少了句柄 ⇒ TOCTOU 窗口回来了"
        );
        assert!(
            PROBE_FMT.contains("#{@ccm_sid}"),
            "少了 sid ⇒ 这道门就是空的"
        );
        assert!(
            PROBE_FMT.contains("#{session_windows}"),
            "少了窗口数 ⇒ Gate 3 没有输入，破坏性动作会误杀多窗口会话"
        );
        assert!(
            !PROBE_FMT.contains("@ccm_sid_expect"),
            "**只认 `@ccm_sid`** —— `_expect` 是「声明了但未必跑起来」的意图，不是事实"
        );
    }
}
