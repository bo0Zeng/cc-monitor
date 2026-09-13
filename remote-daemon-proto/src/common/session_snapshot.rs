//! `K-R96`（09-12）：**「这台机器上现在有哪些 tmux 会话」的那一张快照 —— 全 crate 只有一份。**
//!
//! # 它治的是什么
//!
//! 收拢之前，同一个问题在本 crate 里有**两条各自去问 tmux 的路**：
//!
//! | 谁 | 怎么问 | 拿它干嘛 |
//! |---|---|---|
//! | `observe/watcher.rs` | `sh -c 'tmux ls -F <TMUX_LS_FMT>'`（带 `timeout`） | 差分出「这一轮谁没了」，并把 raw 原样推给 monitor |
//! | `control/gate.rs` | `tmux -u list-sessions -F <三列>` | 判活（`bus-list` 的「这个成员还在吗」）＋ 铸名避让 |
//!
//! 用户 09-12 逐字裁定（`DECISIONS.md#R52` 裁定一）：
//! 「**Gate 能不能改成直接读那份快照. 可以 / 改为向快照发一次询问, 快照更新一次**」。
//!
//! ⇒ 本模块就是那份快照。两条纪律，**都在 API 形状上，不靠自觉**：
//!
//! 1. **问它一次 = 它更新一次。** [`SessionSnapshot::query`] 没有「只读缓存」这条路 ——
//!    它先探一次、再把探回来的那份存下来并返回。**拿到的值恒是这一刻的**。
//!    观测方（watcher）用 [`SessionSnapshot::publish`] 把它**焐热**，
//!    但焐热出来的值**不会**被 `query` 交出去（`query` 一定重探）。
//!    🔴 这条差别是本模块的立身之本：`R52` 允许 Gate 读快照的**前提**就是「读会触发更新」。
//!    退化成「读缓存」= 拿陈值判活 = 把一个刚死的会话报成活的（`bus-list` 会因此
//!    把敲门文字打进**陌生占用者**的屏幕 —— 08-13 实测过的那个后果）。
//! 2. **铸名避让只能问它。** [`TakenNames`] 的字段是模块私有的 ⇒
//!    **本模块之外造不出一份「已占用的名字」**。用户 `R52` 裁定二逐字：
//!    「不就是先校验冲突然后取名吗? **搞个 hash 表**不就好了」——
//!    那张表就是这里，而「另起一份名字集合」在类型层面就编译不过。
//!
//! # 为什么住 `common/` 而不是 `control/` 或 `observe/`
//!
//! `layering_guard` 钉死 `control/` 不许引用 `observe/`，而这份快照**两边都要**
//! （control 的 Gate 判活与铸名要问它，observe 的 watcher 要往里发布）。
//! ⇒ 按 `common/mod.rs` 那三条门槛：① ≥2 个上层用 ✅（control ＋ observe）·
//! ② 平台无关 ✅（起的是 `tmux` 这个跨平台程序，不认识任何 OS 的文件布局）·
//! ③ 无域知识 ✅（它只认识「会话名 + `@ccm_sid`」这两个字段，不认识 `WatchEvent` / `Plan`）。
//!
//! # 🔴 它**不**承诺什么
//!
//! - **它不是 `probe()` 的替代品。** `control/gate.rs::probe`（`display-message -p`）取的是
//!   `#{session_id}` 这个**句柄**，作用是把「查完再动」之间的 TOCTOU 窗口关掉。
//!   本模块交出来的是**名字**，名字会被重新绑定 ⇒ **不许拿它去下破坏性命令**。
//! - **它不带 `#{session_id}`。** 两个消费者（`cc_bus::join_identity` 与 `ccm` 的铸名避让）
//!   一个都不问那一列，而 watcher 那条路（`TMUX_LS_FMT`）里根本没有它 ——
//!   带一列谁都不用、且有一半发布者填不出来的值，就是下一处静默的空串。

use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

use crate::common::tmux_utf8::{tab_underflow, UTF8_CLIENT_FLAG};

/// 命令级错误：`(code, message)`。与 [`crate::control::gate`] 同型。
pub(crate) type CmdErr = (&'static str, String);

/// 快照里的一行：**只有名字与 `@ccm_sid`**（为什么不带 `session_id` 见模块头注）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionRow {
    /// `#{session_name}`。
    pub(crate) name: String,
    /// `@ccm_sid` 的值。未设置 ⇒ 空串（= 那个会话不是 `ccm` 起的，或还没绑 sid）。
    pub(crate) ccm_sid: String,
}

/// **已被占用的会话名。** 字段刻意是模块私有的 —— 本模块之外造不出一份。
///
/// 这就是 `R52` 裁定二那张「hash 表」的类型形态：铸名避让（`ccm::plan::build`）
/// 只收这个类型，于是「另起一份名字集合」不是靠注释劝阻，是**编译不过**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TakenNames(Vec<String>);

impl TakenNames {
    /// 给避让算法用的那一份。
    pub(crate) fn as_slice(&self) -> &[String] {
        &self.0
    }
}

/// 探回来一份的那个动作。测试用假的替掉，生产用 [`probe_tmux`]。
type Prober = Box<dyn Fn() -> Result<Vec<SessionRow>, CmdErr> + Send + Sync>;

/// 那一张快照。
pub(crate) struct SessionSnapshot {
    rows: Mutex<Vec<SessionRow>>,
    probe: Prober,
}

impl SessionSnapshot {
    fn new(probe: Prober) -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
            probe,
        }
    }

    /// **向快照发一次询问 ⇒ 快照更新一次**（`R52` 裁定一逐字）。
    ///
    /// 🔴 **没有「只读」这条路。** 交出去的恒是**刚探回来**的那一份，不是上一次存的。
    /// 把它改成「先看缓存、有就直接回」是本模块唯一那种会静默失效的改法 ——
    /// `KR96D1` 的死值验第二刀钉的就是这一下。
    pub(crate) fn query(&self) -> Result<Vec<SessionRow>, CmdErr> {
        let fresh = (self.probe)()?;
        // 存一份供 `publish` 的同居者读（并让「刚探到的」与「焐着的」始终是同一份）。
        if let Ok(mut g) = self.rows.lock() {
            g.clone_from(&fresh);
        }
        Ok(fresh)
    }

    /// 铸名避让要的那份「已占用的名字」。**同样会触发一次更新**（它就是 `query` 的投影）。
    pub(crate) fn taken_names(&self) -> Result<TakenNames, CmdErr> {
        Ok(TakenNames(
            self.query()?.into_iter().map(|r| r.name).collect(),
        ))
    }

    /// 观测方把刚看到的一份**焐进来**。
    ///
    /// ⚠ **焐热不等于「问过了」**：`query` 一定重探，绝不把这里存的值交出去。
    /// 它买的是「同一张表」这件事本身 —— 观测与判活从此看的是一个数据结构，
    /// 而不是两条各自去起 tmux 的路。
    pub(crate) fn publish(&self, rows: Vec<SessionRow>) {
        if let Ok(mut g) = self.rows.lock() {
            *g = rows;
        }
    }

    /// 最后一次存进来的那份（**陈值**）。
    ///
    /// 🔴 生产代码一处都不许调它 —— 它只是让「陈值确实与这一刻不同」这件事**测得出来**。
    /// 由 `KR96D1` 的判据钉住「生产段零调用」。
    #[cfg(test)]
    pub(crate) fn peek(&self) -> Vec<SessionRow> {
        self.rows.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// 测试用：拿一个脚本化的探测器造一份快照。
    #[cfg(test)]
    pub(crate) fn with_prober(
        probe: impl Fn() -> Result<Vec<SessionRow>, CmdErr> + Send + Sync + 'static,
    ) -> Self {
        Self::new(Box::new(probe))
    }
}

/// 进程内那一份。
pub(crate) fn global() -> &'static SessionSnapshot {
    static ONE: OnceLock<SessionSnapshot> = OnceLock::new();
    ONE.get_or_init(|| SessionSnapshot::new(Box::new(probe_tmux)))
}

/// 探测格式串。**两列**用 TAB 分隔 —— 会话名被 tmux 转义成字面 `\t`、
/// `@ccm_sid` 的字符集在 `launch::parse_request` 里收到了 `[A-Za-z0-9_-]`
/// ⇒ 下溢与过溢都不会由合法内容触发，下溢只可能是打印通道被改写。
const LIST_FMT: &str = "#{session_name}\t#{@ccm_sid}";

/// `LIST_FMT` 的列数 —— [`tab_underflow`] 的 N。**改格式串必须同步这个数。**
const LIST_FMT_FIELDS: usize = 2;

/// 全 crate **唯一**一处「一次列全部 tmux 会话」的起进程点。
///
/// ⚠ **一次调用列全部**，不是每个成员探一次：用户那台的总线有 86 行，
/// 逐个探就是 86 次起进程。
///
/// ⚠ **不看退出码**（同 `control/gate.rs::probe`）：没有任何会话时 `tmux ls` 是
/// 非零 + 空输出，那不是错误，是「一个都没有」。
///
/// 🔴 **`K-R12`：`-u` 必须排在子命令之前。** 放到后面是 `rc=1 + unknown flag -u`，
/// 而这里刻意不看退出码 ⇒ 那个响错会被压成「一个会话都没有」= 又一次静默失效。
/// 由本模块测试段那条判据钉住次序（判据从 `control/gate.rs` 随这处调用点一起搬来）。
fn probe_tmux() -> Result<Vec<SessionRow>, CmdErr> {
    let out = Command::new("tmux")
        .args([UTF8_CLIENT_FLAG, "list-sessions", "-F", LIST_FMT])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| {
            (
                "no_tmux",
                format!("起不来 tmux（远端装了吗？PATH 里有吗？）：{e}"),
            )
        })?;
    Ok(parse_rows(&String::from_utf8_lossy(&out.stdout)))
}

/// 把 `LIST_FMT` 的 stdout 切成行。
///
/// `K-R12 J1`：段数下溢 ⇒ 打印通道被改写 ⇒ **出声 + 整行不当好数据**。
/// 空行不算下溢（它本来就该被 `name.is_empty()` 丢掉，不是病）。
fn parse_rows(text: &str) -> Vec<SessionRow> {
    text.lines()
        .filter_map(|line| {
            if !line.trim().is_empty() && tab_underflow(line, LIST_FMT_FIELDS) {
                tracing::warn!(
                    "CCM_TMUX_UNPARSABLE list-sessions 切出 {} 段 < {LIST_FMT_FIELDS} —— \
                     tmux 打印通道被改写（K-R12：客户端不是 UTF-8 ⇒ TAB 变 `_`），\
                     整行不当好数据。原样回包：{line:?}",
                    line.split('\t').count()
                );
                return None;
            }
            let mut it = line.split('\t');
            let name = it.next()?.trim();
            if name.is_empty() {
                return None;
            }
            Some(SessionRow {
                name: name.to_string(),
                ccm_sid: it.next().unwrap_or_default().trim().to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, sid: &str) -> SessionRow {
        SessionRow {
            name: name.to_string(),
            ccm_sid: sid.to_string(),
        }
    }

    /// ★★ **`KR96D1` 死值验第二刀：读快照必须触发更新，拿到的值是「这一刻的」。**
    ///
    /// 🔴 **失效方向（件文件逐字）：只判「有没有调那个快照函数」。**
    /// 那种判据在「`query` 被改成先看缓存」时**照样绿** —— 而那正是本件要挡的那一下。
    /// ⇒ 这里的夹具是一个**会变的世界**：探测器第 1 次回 A、第 2 次回 B。
    /// 判的是**第二次问到的是 B**，不是「问了」。
    #[test]
    fn asking_the_snapshot_refreshes_it_so_the_answer_is_from_this_moment() {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let snap = SessionSnapshot::with_prober(move || {
            let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(vec![row(&format!("world-{n}-cc"), "sid-x")])
        });

        let first = snap.query().expect("第一次问得到");
        assert_eq!(first, vec![row("world-0-cc", "sid-x")]);

        // 世界变了（探测器的下一次回的是另一份）。**没有任何人再调 publish**。
        let second = snap.query().expect("第二次问得到");
        assert_eq!(
            second,
            vec![row("world-1-cc", "sid-x")],
            "第二次问回来的还是第一次那份 —— `query` 退化成了「读缓存」。\n\
             ★ `R52` 裁定一允许 Gate 读快照的**前提**就是「问一次、更新一次」；\n\
               读陈值判活 = 把一个刚死的会话报成活的。"
        );
    }

    /// ★★ **焐热不等于问过了** —— `publish` 进去的值，`query` 不许交出去。
    ///
    /// 这一条挡的是另一种改法：「`query` 先看有没有人 publish 过，有就直接回」。
    /// 它比上一条更隐蔽 —— 有 watcher 在跑的时候那个值**看起来**是新的
    /// （watcher 每次观测都焐一遍），于是单测里一切正常，
    /// 而 watcher 观测不到（`NoTmux` / `Unobservable`）的那些时刻它就是陈的。
    #[test]
    fn a_warmed_value_is_never_what_the_query_hands_back() {
        let snap = SessionSnapshot::with_prober(|| Ok(vec![row("probed-cc", "sid-p")]));
        snap.publish(vec![row("warmed-cc", "sid-w")]);
        assert_eq!(snap.peek(), vec![row("warmed-cc", "sid-w")], "焐进去了");
        assert_eq!(
            snap.query().expect("问得到"),
            vec![row("probed-cc", "sid-p")],
            "`query` 把 `publish` 焐进来的那份交了出去 —— 那是陈值。"
        );
        assert_eq!(
            snap.peek(),
            vec![row("probed-cc", "sid-p")],
            "问完之后焐着的那份也该是刚探到的（同一张表，不是两份）"
        );
    }

    /// ★ `taken_names` 是 `query` 的投影 ⇒ 它同样会触发更新（避让不许拿陈名单）。
    #[test]
    fn the_taken_name_table_is_also_refreshed_by_the_asking() {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let snap = SessionSnapshot::with_prober(move || {
            let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok((0..=n).map(|k| row(&format!("p-{k}-cc"), "")).collect())
        });
        assert_eq!(snap.taken_names().expect("问得到").as_slice(), ["p-0-cc"]);
        assert_eq!(
            snap.taken_names().expect("问得到").as_slice(),
            ["p-0-cc", "p-1-cc"],
            "避让拿到的名单是陈的 —— 它会让新会话撞上刚建出来的那个"
        );
    }

    /// ★ 陈值那个口子（`peek`）**只许测试用** —— 生产段一处都不许调。
    ///
    /// 它是本模块唯一一个「读了不更新」的口，留着是为了让上面两条测得出来。
    /// 漏进生产段就等于把 `R52` 的前提拆了，所以在这里数一遍。
    #[test]
    fn the_stale_read_door_never_appears_in_production_code() {
        // ⚠ **必须走 `scan_tree!`，不许自己 `read_dir`** —— 它按构造摘掉调用者自己那一份。
        //   裸遍历会让判据在**自己的语料**里找到自己 ⇒ 恒绿；
        //   那条纪律由 monitor 侧 `scanning_guard_registry` 机检（本条第一版就是这么红的）。
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut checked = 0usize;
        let mut hits: Vec<String> = Vec::new();
        for (path, raw) in guard_core::scan_tree!(&src_dir, &["rs"]) {
            checked += 1;
            if crate::guard_support::production_code(&raw).contains(".peek()") {
                hits.push(
                    path.strip_prefix(&src_dir)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        assert!(
            checked >= 20,
            "只扫到 {checked} 个源文件 —— 遍历坏了，本条在空转"
        );
        assert!(
            hits.is_empty(),
            "这些文件的**生产段**里调了 `.peek()`（快照的陈值口）：{hits:?}\n\
             ★ 陈值只许测试读。生产上要「这一刻」就调 `query()`。"
        );
    }

    /// ★★ **`K-R12`：这一处起 tmux，`-u` 必须排在子命令之前。**
    ///
    /// 判据本体从 `control/gate.rs` 随 `list-sessions` 这处调用点一起搬来
    /// （那边今天只剩 `display-message` 一处，它那条判据也随之收成一个动词）。
    /// 为什么这一条只能是「扫源码」、以及它守不住什么，见 `control/gate.rs` 同名判据的头注
    /// （行为那一半的死值在 `evidence/K-R12-deathvalue.md`）。
    #[test]
    fn the_one_list_sessions_call_asks_for_a_utf8_client_before_the_subcommand() {
        const FLAG_IDENT: &str = "UTF8_CLIENT_FLAG";
        const ARGS_OPEN: &str = ".args([";
        let prod = crate::guard_support::production_code(include_str!("session_snapshot.rs"));
        crate::guard_support::assert_no_test_code("common/session_snapshot.rs", &prod);

        let starts = prod.matches("Command::new(\"tmux\")").count();
        assert_eq!(
            starts, 1,
            "本模块起 tmux 的处数变了（实得 {starts}，登记 1）—— 它是**唯一**一处\
             「一次列全部会话」的起进程点，多一处就说明快照又被绕开了。"
        );
        let flat: String = prod.chars().filter(|c| !c.is_whitespace()).collect();
        let quoted = "\"list-sessions\"";
        let at = guard_core::find_pinned(&prod, quoted).expect("`list-sessions` 定得了位");
        let open = prod[..at].rfind(ARGS_OPEN).expect("它前面有一个 `.args([`");
        let before = &prod[open + ARGS_OPEN.len()..at];
        assert!(
            !before.contains(']'),
            "`list-sessions` 与它前面那个 `{ARGS_OPEN}` 之间隔着一个右方括号 —— 本条在断言别人的 argv"
        );
        assert!(
            before.contains(FLAG_IDENT),
            "`list-sessions` 那一处没把 `-u` 放在子命令**之前**。\
             放到后面是 rc=1 的响错，而这里不看退出码 ⇒ 会退化成「一个会话都没有」"
        );
        assert!(
            !flat.contains(&format!("{quoted},{FLAG_IDENT}")),
            "`-u` 被放到了子命令后面（`tmux -u ls -u` 同样 rc=1）"
        );
    }

    /// ★ `K-R12 J1`：段数下溢 ⇒ 整行不当好数据（本处这一侧）。
    #[test]
    fn a_tab_starved_line_is_dropped_instead_of_becoming_a_session() {
        // 通道被改写：TAB 变 `_` ⇒ 整行只切出 1 段。
        let dirty = "kr96_cc-deadval1\n";
        assert!(parse_rows(dirty).is_empty(), "下溢的行被当成了一个会话名");
        // 好行照常出。
        assert_eq!(
            parse_rows("a-cc\tsid-a\nb-cc\t\n\n"),
            vec![row("a-cc", "sid-a"), row("b-cc", "")]
        );
    }
}
