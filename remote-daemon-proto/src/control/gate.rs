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
pub(crate) type CmdErr = (&'static str, String);

/// ★★ **K-R12（09-04）：`-u` —— 让这一侧的 tmux 客户端始终按 UTF-8 输出。**
///
/// # 病：**格式串里的 TAB 会被 tmux 自己吃掉**，而且完全静默
///
/// tmux 对**输出通道**做 sanitize：客户端不是 UTF-8 时，`display-message -p` /
/// `list-sessions -F` 打出来的**控制字符与非 ASCII 一律换成 `_`** —— 我们靠来分列的
/// 那个真 TAB（0x09）首当其冲。沙箱实测（`evidence/K-R12-locale-lab.md`，容器内
/// tmux 3.4 私有 socket，`od -c` 读字节）：POSIX 客户端下 `1\t/tmp/x` 变成 `1_/tmp/x`。
/// ⇒ `line.split('\t')` 切不出段 ⇒ 本模块两个解析点全部拿到垃圾，**没有任何日志、rc 仍是 0**。
///
/// # 🔴 判「是不是 UTF-8 客户端」的规则**不问 glibc**，所以配置推不出结果
///
/// 实测（`lab2.sh` E11，同一台 server、只换客户端那一侧）：tmux 取
/// `LC_ALL` → `LC_CTYPE` → `LANG` 的**第一个非空值**，做一次**大小写不敏感的
/// `UTF-8`/`UTF8` 子串匹配**，`setlocale` 那条路它根本走不到。于是：
/// `zz_ZZ.UTF-8`（locale 压根不存在）**干净**、`utf8`（连点都没有）**干净**，
/// 而 `zh_CN.GB18030` **脏**、`C` **脏**、`LC_ALL=''`（设了但空）**脏**、什么都不设 **脏**。
/// ⇒ **「我们设没设对」这个问题的答案与结果之间隔着一条不直观的子串规则** ——
/// 任何「把 daemon 的 `LC_ALL`/`LANG` 打进日志」的判据都在量错的东西。
///
/// # 为什么这两处用 `-u` 而不是 `.env("LC_ALL", "C.UTF-8")`
///
/// 两种写法都能让客户端变成 UTF-8（实测都干净），但代价不对称。本模块这两处选 `-u`，
/// 五条理由（前两条是它相对 `LC_ALL` 的优势，后三条是「为什么这里不适用 `LC_ALL` 的优势」）：
///
/// 1. **`-u` 不依赖任何继承来的 env。** 实测：`env -i`（环境全清）+ `-u` 仍然干净；
///    `env -i` 不加 `-u` 就脏。而 daemon 的 env **恰恰是我们最不控制的那个** ——
///    它由 sshd/systemd/launchd 起，被剥干净是常态。
/// 2. **`-u` 与它保护的那个格式串在同一个 `.args([…])` 表达式里**：格式串与它的编码口径
///    一起被读到。`.env(…)` 是 builder 上另一步，重构时最容易被搬开而没人发觉。
/// 3. **这两处没有分支多重性。** `LC_ALL` 在 `observe/watcher.rs` 那两处胜出的唯一理由是
///    「一行 `.env` 盖住 `sh -c` 脚本里的两条 `exec` 分支」；这里是 argv 直传、一处就是一处，
///    那条优势在这里不存在。
/// 4. **与 monitor 侧同形。** `src-tauri/src/tmux.rs` 那两处跨 SSH 的也用 `-u`
///    （那边没有本地 `Command` 可挂 env）。`tmux_daemon_gate_guard` 钉的是两侧那道门等价，
///    两侧用同一种机制，读的人对得起来。
/// 5. **位置是硬的**：`-u` 必须在子命令**之前**。实测 `tmux display-message -u -p …`
///    是 `rc=1 + command display-message: unknown flag -u` ⇒ **放错位置是响的，不是静默的**。
///
/// ⚠ **`-u` 单独不够。** 它自己的失效面是「某一处忘了插」，而那是静默的
/// ⇒ 本模块两个解析点同时装了 [`tab_underflow`]（K-R12 的 `J1`）。
/// **预防（`-u`）与处置（`J1`）是两件事，缺一不可。**
const UTF8_CLIENT_FLAG: &str = "-u";

/// ★★ **K-R12 `J1`：段数下溢 —— 「拆不出段」不许长得像「字段是空的」。**
///
/// > 按 TAB 切 tmux 的打印通道，切出的段数 **< 预期 N** ⇒ 出声 **+ 拒绝把这行当好数据**。
///
/// # 为什么判据是「下溢」而不是「恰好 N」
///
/// 沙箱实测（`lab2.sh` E12）：**合法内容只会把段数推高，永远不会推低** ——
/// 会话名里的真 TAB 被 tmux 转义成字面 `\t` 两个字符（`new-session -s $'aa\tbb'` 之后那行
/// 仍是 6 段），而 `pane_current_path` 里的真 TAB 会切出 7 段。
/// ⇒ `< N` **零误报**；`!= N` 会误伤（见本文件 [`probe`] 与 `tmux.rs::parse_tmux_ls` 头注）。
///
/// # 为什么下溢是**完备**检测器
///
/// sanitize 是**每客户端全有全无**的（实测：同一台 server、同一条命令，只换客户端那一侧，
/// 输出要么整条干净、要么整条被改写）⇒ 通道一脏，格式串里的 TAB **全部**消失
/// ⇒ 段数必然从 N 塌到 **1**。**不存在「内容被改写了但 TAB 还在」的中间态**
/// ⇒ 这一条判据同时盖住「分隔符被吞」与「内容被改写」两半，而且**格式串一个字节不用动**。
///
/// # ⚠ 为什么它在本文件里又写了一份
///
/// `layering_guard` 钉死 **`control/` 不许引用 `observe/`**（「反向一条都不许」）。
/// 同样的判据在 `observe/watcher.rs` 与 monitor 的 `src-tauri/src/tmux.rs` 各有一份 ——
/// 那不是抄漏，是层界与仓界逼出来的。**三份的口径必须一致**：`< N` ⇒ 出声 + 丢这行。
/// 🔴 今天靠人对齐；`K-R12` 的 `J2`（登记表：凡按 TAB 切 tmux 输出的地方都要过同一口径，
/// 多一处红、少一处也红）**尚未装**，PM 那边还没裁。
fn tab_underflow(line: &str, expected: usize) -> bool {
    line.split('\t').count() < expected
}

/// 探测回来的两样东西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Probed {
    /// tmux 的 `#{session_id}`（形如 `$0`）。**后续一律对它下命令**，见模块头注。
    pub(crate) session_id: String,
    /// `@ccm_sid` 的值。未设置 ⇒ 空串。
    ///
    /// ⚠ 这是 `@ccm_sid`，**不是 `@ccm_sid_expect`**。刻意分了两个：
    /// 通道 A（`shared/ccm`）只写意图，只有通道 B 才写事实，而**破坏性动作只认事实**。
    /// 放宽到 `_expect` 就是把这道门拆了。
    /// ⚠ `U-NP④`（08-14）之后**通道 B 的写者是 daemon 自己**（[`super::identity_tag`]，
    /// 由 pidfile inotify 驱动），不再是 ccm 里那条每秒轮询 —— 分离更硬了（事实的写者
    /// 变成独立第三方，且打标前已过 `procStart` 冒名检查），但两个 key 的语义一个字没变。
    pub(crate) ccm_sid: String,
    /// `#{session_windows}`。**Gate 3 只给破坏性动作用**（见 [`admit_destructive`]）。
    /// 解析不出来 ⇒ `0`，而 Gate 3 要求恰好 `1` ⇒ **fail closed**（不会误杀）。
    pub(crate) windows: u32,
}

/// 列出本机所有 tmux 会话的**身份三元组**：`(会话名, session_id, @ccm_sid)`。
///
/// # 它是给「谁是真的」用的〔P4f 续刀 08-13，用户提的架构点〕
///
/// 用户逐字：「**那他不应该是身份空间的子集吗? 他应该去调用身份空间啊**」。
///
/// cc-bus 的 `agents.tsv` 是一份**第二套名单**：它记 `id → session:win.pane`，
/// 而那个地址**会过期**（会话名被重用是常态 —— `cc-spawn` 就按目录基名取名；
/// 用户盘上那份有 86 行、最早 07-18）。08-13 实测过它的后果：敲门文字被打进**陌生占用者**的屏幕。
///
/// ⇒ 正确的从属关系是：**总线成员 ⊆ 活着的 tmux 会话**。谁活着由**这里**说了算，
/// `agents.tsv` 只回答「谁登记过 + 邮箱里还有几条没读」。
///
/// ⚠ **一次调用列全部**，不是每个成员探一次：用户那台的总线有 86 行，
/// 逐个探就是 86 次起进程。
///
/// `@ccm_sid` 空 = 那个会话不是 `ccm` 起的（或还没绑 sid）—— 如实回空串，不猜。
pub(crate) fn list_sessions() -> Result<Vec<(String, String, String)>, CmdErr> {
    const LIST_FMT: &str = "#{session_name}\t#{session_id}\t#{@ccm_sid}";
    /// `LIST_FMT` 的列数 —— [`tab_underflow`] 的 N。三列都不可能含真 TAB
    /// （会话名被 tmux 转义成字面 `\t`；`session_id` 恒是 `$<数字>`；
    /// `@ccm_sid` 的字符集在 `launch::parse_request` 里收到了 `[A-Za-z0-9_-]`）
    /// ⇒ 这里**下溢与过溢都不会由合法内容触发**，下溢只可能是通道被改写。
    const LIST_FMT_FIELDS: usize = 3;
    let out = Command::new("tmux")
        // K-R12：`-u` 必须在子命令**之前**（放后面是 rc=1 的响错，见 `UTF8_CLIENT_FLAG`）。
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
    // ⚠ **不看退出码**（同本模块 `probe`）：没有任何会话时 `tmux ls` 是非零 + 空输出，
    //   那不是错误，是「一个都没有」。
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            // K-R12 `J1`：段数下溢 ⇒ 通道被改写 ⇒ **出声 + 整行不当好数据**。
            // 空行不算下溢（它本来就该被下面那句 `name.is_empty()` 丢掉，不是病）。
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
            Some((
                name.to_string(),
                it.next().unwrap_or_default().trim().to_string(),
                it.next().unwrap_or_default().trim().to_string(),
            ))
        })
        .collect())
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

/// `PROBE_FMT` 的列数 —— [`tab_underflow`] 的 N。
///
/// 🔴 **顺带订正一条上一拍写错的话**：`K-R12` 的 `§5.4` 说
/// 「`gate.rs` 那条 `it.next().is_some()` 在多出一段时静默丢掉整个会话 —— 一个带 TAB 的
/// 目录名就能触发」。**在本处不成立**：`PROBE_FMT` 里根本没有路径列
/// （`pane_current_path` 只在 `TMUX_LS_FMT` 里），三列各自的取值域都排除了真 TAB
/// （`$<数字>` / `[A-Za-z0-9_-]` / 正整数）。
/// ⇒ 本处的**过溢只可能来自「有人手工把 `@ccm_sid` 设成含 TAB 的值」或格式串被改**，
/// 那两种都该拒 ⇒ 既有的 fail-closed 处置是对的，**本拍不动它**。
/// 那条误伤是真的、但只在 `src-tauri/src/tmux.rs::parse_tmux_ls` 那一处（见该处头注）。
const PROBE_FMT_FIELDS: usize = 3;

/// 跑一次 `tmux display-message -p -t <target> '<fmt>'` 并把 stdout 取回来。
///
/// `Ok(None)` = 目标不存在（输出为空，见模块头注：**不看退出码**）。
///
/// ⚠ `target` **不限于会话名**：tmux 的目标解析会把 pane id（`%N`）也归到它所属的会话上
///（08-14 私有 socket 实测）。[`super::identity_tag`] 正是拿 `TMUX_PANE` 当 target 调它 ——
/// 复用这一处等于**不新增起进程点**。⚠ 空串 target 会被 tmux 静默解析成「某个会话」，
/// 调用方必须自己挡（`identity_tag::pane_is_safe` 就是那道门）。
pub(crate) fn probe(target: &str) -> Result<Option<Probed>, CmdErr> {
    let out = Command::new("tmux")
        // K-R12：`-u` 必须在子命令**之前**（`display-message -u -p` 是 rc=1 的响错）。
        .args([UTF8_CLIENT_FLAG, "display-message", "-p", "-t", target, PROBE_FMT])
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
    // ★ K-R12 `J1`：**段数下溢 ⇒ 通道被改写**，出声 + 拒绝把这行当好数据。
    //
    // 🔴 不装这一条的后果，与 `K-R23` 在 monitor 侧治的是**同一个形状**：通道脏时
    // `session_id` 会拿到**整行**、`ccm_sid` 拿到**空串** ⇒ `admit` 报出来的话是
    // 「远端 `@ccm_sid` 也没设」—— 与「远端真的没设」**逐字相同**。
    // 两件不同的事共用一个读数，下一个人还得把成因重查一遍。
    // ⇒ 处置沿用既有的 `Ok(None)`（= 目标不存在，fail-closed，`admit` 回 `no_such_session`），
    //   **但话不一样**：这一条 warn 里带原样回包，一眼看得见远端到底吐了什么。
    if tab_underflow(line, PROBE_FMT_FIELDS) {
        tracing::warn!(
            "CCM_TMUX_UNPARSABLE display-message 切出 {} 段 < {PROBE_FMT_FIELDS} —— \
             tmux 打印通道被改写（K-R12：客户端不是 UTF-8 ⇒ TAB 变 `_`），\
             判成「探不到」而不是「@ccm_sid 没设」。原样回包：{line:?}",
            line.split('\t').count()
        );
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

    // ── audit-0805 F20 下半：把「skip 不是通过」那个刻意决定钉住 ──────────────
    //
    // F20 的处境：`meta_dollar`（会话名 `cc-a$x`）在 CI 上报 `no_such_session`，
    // **根因至今未知** —— 复现要那台机器的 tmux，红线禁真 tmux、且已裁定不再推 CI。
    // 上半做的是「把一条误导性的失败改成一条**会自证**的失败」：建完用 `=name:` 复核，
    // 找不到就 **skip 并打出 tmux 实况**。
    //
    // 而 §3 那个决定 —— **地板不动（36），skip 会让 PASS 少一个 ⇒ 地板照样红** ——
    // 今天**只是一段散文**。有人为了让 CI 变绿把 36 改成 35，这个意图就静默消失了，
    // 而那正是本区 F25 那一族（一个刻意的决定没有判据看着）。
    //
    // ⚠ 本组**不修根因，也不假装修了** —— 它只保证那个决定不会被悄悄推翻。

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf()
    }

    /// ★ `daemon-gate2` 的地板**只许涨**，而且必须**盖得住判定表的行数**。
    ///
    /// 地板低于用例数时，「有一格没验到」就不会让任何东西变红 —— skip 变成了免费的。
    #[test]
    fn the_gate2_floor_still_makes_a_skip_hurt() {
        let root = repo_root();
        let ci =
            std::fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("ci.yml 读不到");
        let mark = "assert-pass-floor.sh daemon-gate2 ";
        let at = ci
            .find(mark)
            .expect("ci.yml 里没有 `assert-pass-floor.sh daemon-gate2 <地板>` 调用行");
        let floor: usize = ci[at + mark.len()..]
            .split_whitespace()
            .next()
            .and_then(|t| t.trim().parse().ok())
            .expect("地板值解析不出来 —— 调用行的形状变了");

        let golden = std::fs::read_to_string(
            root.join("src-tauri/src/backend/control/fixtures/gate2-golden.tsv"),
        )
        .expect("判定表读不到");
        let rows = golden
            .lines()
            .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
            .count();
        // 抽取器自检：判定表解析不出行时，下面那条会零命中地绿。
        assert!(
            rows >= 20,
            "判定表只解析出 {rows} 行（08-06 实测 25）—— 抽取器坏了，下面那条此刻是空转的"
        );

        // ★ 只许涨。08-06 实测 36：判定表 25 行 + 抽取器自检 + 其余固定项。
        const FLOOR_TODAY: usize = 36;
        assert!(
            floor >= FLOOR_TODAY,
            "`daemon-gate2` 的地板被降到了 {floor}（08-06 是 {FLOOR_TODAY}）。\n\
             ★ **降它就是把「skip 不是通过」这个决定推翻了**：`meta_dollar` 在某些 tmux 上会 skip，\n\
             PASS 因此少一个；地板不动 ⇒ 红 ⇒ 「有一格没验到」看得见。\n\
             地板降下去，那一格就静默消失了 —— 而**根因至今未知**（F20 §2）。\n\
             真要降，先把根因查清并说明为什么那一格不必再验。"
        );
        assert!(
            floor > rows,
            "地板 {floor} 没盖住判定表的 {rows} 行 —— skip 一格也不会让它红，\n\
             那个决定就成了空话。加用例时地板要跟着抬。"
        );
    }

    /// ★ 那个决定的**前提**：脚本里那条「建完复核、找不到就 skip 并打实况」还在。
    ///
    /// 它一没，失败就退回**误导性**的那种（看起来像 Gate 2 判错，实际是夹具没准备好）——
    /// 那正是 F20 上半修掉的东西。
    #[test]
    fn the_selfevidencing_skip_branch_is_still_there() {
        let sh = std::fs::read_to_string(repo_root().join("e2e/daemon-gate2-acceptance.sh"))
            .expect("e2e 脚本读不到");
        for needle in ["has-session -t \"=$name:\"", "实际会话："] {
            assert!(
                sh.contains(needle),
                "e2e 脚本里找不到 `{needle}` —— 「建完复核 + skip 时打出 tmux 实况」那段没了。\n\
                 ★ 它一没，`meta_dollar` 的失败就退回**误导性**的那种：\n\
                 看起来像「Gate 2 判错了」，实际是「夹具没准备好」。那是 F20 上半修掉的东西。\n\
                 ⚠ 同时上面那条地板判据也失去意义 —— 它保护的正是这条 skip 的可见性。"
            );
        }
    }
}
