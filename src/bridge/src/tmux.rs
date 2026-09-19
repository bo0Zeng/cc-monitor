//! F51 tmux 反查 / F60 画面预览(控制类命令走通道 B = russh exec,不干扰前台 PowerShell 终端)。
//!
//! tab 右键菜单打开时按需查询远端 tmux 会话列表,前端按 `pane_current_path==cwd +
//! pane_current_command==claude` 反查该 tab 的 Claude 正跑在哪个 tmux 会话,命中则一键
//! `ssh -t … tmux attach -t <名>`(交互走通道 A = PowerShell)。
//!
//! **最隐蔽的重写坑(调研 03 档 §3.1)**:`tmux ls -F` 的格式串**不解释**字面 `\t`——给什么
//! 字节原样输出。所以分隔符必须是**真 TAB 字节(0x09)**。Rust 里 `"\t"` 是真 TAB(勿写
//! `\\t`),`parse_tmux_ls` 按真 TAB `split`。F60 `capture_remote_pane` 已续挂本模块;kill/rename
//! 明确不做(见 MASTERPLAN 不做清单),F52 短路门未扩本模块。

use crate::ssh_source;
use serde::Serialize;
use tokio::io::{AsyncReadExt, BufReader};

/// `tmux ls -F` 的格式串。字段以**真 TAB**分隔(见模块注释):
/// name ⇥ pane_current_path ⇥ pane_current_command ⇥ attached(1/0) ⇥ windows ⇥ @ccm_sid。
/// **F74**:末列 `#{@ccm_sid}` 是 `__ccm_rbind` 写的 tmux user option = 「这个 tmux 此刻在跑
/// 哪个 CC sid」的权威信号(pane title 被 Claude 活动标题抢写、不可靠;user option Claude 碰
/// 不到)。**未设置的会话此列为空串**(老会话 / 未装 wrapper)→ 解析成 `sid: None`,消费方回退
/// 旧的 path/cmd 匹配,向后兼容。
const TMUX_LS_FMT: &str = "#{session_name}\t#{pane_current_path}\t#{pane_current_command}\t#{?session_attached,1,0}\t#{session_windows}\t#{@ccm_sid}";

/// `TMUX_LS_FMT` 的列数 —— [`tmux_tab_underflow`] 的 N。**改格式串必须同步这个数**
/// （格式串本身是红线 I8 双写点，见上面头注）。
const TMUX_LS_FMT_FIELDS: usize = 6;

/// ★★ **K-R12（09-04）：`-u` —— 让**远端**那个 tmux 客户端始终按 UTF-8 输出。**
///
/// # 病：不是「格式串写错了」，是 **tmux 自己把输出重写了**
///
/// 模块头注那条「格式串不解释字面 `\t`、必须给真 TAB」说的是**我们怎么写**；
/// 这一条说的是**tmux 怎么打**。tmux 对输出通道做 sanitize：**客户端不是 UTF-8 时，
/// 控制字符与非 ASCII 一律换成 `_`**（按**显示宽度**替换，不按字节数：实测 `文`(3B)→`__`）。
/// 我们靠来分列的真 TAB（0x09）首当其冲。沙箱实测（`tests/evidence/K-R12-locale-lab.md`，
/// 容器内 tmux 3.4 + 私有 socket + `od -c` 读字节）：POSIX 客户端下 `TMUX_LS_FMT`
/// 那**六列塌成 1 段** ⇒ [`parse_tmux_ls`] 的 `f.len() != 6` 把**每一行**都丢掉
/// ⇒ **右键菜单里一个 tmux 会话都没有，而 rc=0、stderr 空、一条日志都没有。**
///
/// # 判「是不是 UTF-8 客户端」的规则**不问 glibc**，所以配置推不出结果
///
/// 实测：tmux 取 `LC_ALL` → `LC_CTYPE` → `LANG` 的第一个非空值，做一次
/// **大小写不敏感的 `UTF-8`/`UTF8` 子串匹配**。`zz_ZZ.UTF-8`（locale 根本不存在）**干净**，
/// 而 `C` / `zh_CN.GB18030` / `LC_ALL=''` / 什么都不设 **全脏**。
///
/// # 🔴 为什么跨 SSH 这两处必须用 `-u`，不能学 daemon 那边挂 `LC_ALL`
///
/// 1. **这里没有本地 `Command` 可挂 env** —— 命令是一条字符串，交给 `russh` 的
///    `channel.exec` 在**对端**跑。
/// 2. 走 SSH 的 `request_env` 要赌**对端 sshd 的 `AcceptEnv`**：不认就**静默拒绝**，
///    我们这侧看不出任何区别 —— 拿一条静默失效去治另一条静默失效。
///    （全仓 `.env("LANG"/"LC_ALL"/"LC_CTYPE")` 命中 0 处，`channel.exec` 也不带 env 请求。）
/// 3. 在命令串里前缀 `LC_ALL=C.UTF-8 tmux …` 也能成（实测 dash 上成立），但它要求
///    **对端认得这个赋值前缀**；而 `-u` 实测**连 `env -i`（环境全清）都盖得住**，
///    **不需要对端装任何 locale、不需要 sshd 配合**。这是本仓对远端假设最少的一条。
/// 4. 位置是硬的：`-u` 必须在子命令**之前**。实测 `tmux ls -u -F …` 与
///    `tmux display-message -u -p …` 都是 `rc=1 + unknown flag -u` ⇒ **放错是响的**。
///
/// ⚠ `capture-pane -p` **不在人群里**：实测它抓回来的中文是原始 UTF-8 字节，
/// POSIX / `C.UTF-8` / `-u` 三种模式逐字节相同 ⇒ [`capture_remote_pane`]
/// **不受本件影响**，本拍**刻意不给它加 `-u`**（不扩面）。
/// 〔`设计/50`：原话还并列了 `account_usage.rs` 的用量探针 —— 那条整轴退役了。〕
///
/// ⚠ **`-u` 单独不够**，它的失效面是「某一处忘了插」，而那是静默的。
/// 处置那一半今天在 `list_remote_tmux` 上：[`tmux_tab_underflow`]（K-R12 `J1`）。
/// **预防（本条）与处置（那条）是两件事，缺一不可，且必须能共存。**
///
/// ⚠ `K-R72`（09-12）：处置那一半原先是**两条** —— 另一条是 `build_guarded_tmux_cmd`
/// 那道门用 `K-R23` 落的 `CCM_GUARD_UNPARSABLE`。那条门随送键与杀会话的桌面侧回落
/// 一起走了 ⇒ **跨 SSH 的 tmux 读今天只剩 `ls` 一条**，人群与处置一起缩到 1。
/// daemon 侧同族的那一条仍在（`control/gate.rs` 的 `CCM_TMUX_UNPARSABLE`），
/// 它守的是 daemon **本机**那次 `display-message`，与跨 SSH 这条不是同一处。
///
/// # ★★ K-R12 下一拍（09-04）：**这一份为什么留在这里** —— 两条路各自的论据
///
/// daemon 那两份上一拍是三份里的两份，本拍已经归位到**一个家**
/// （`src/backend/common/tmux_utf8.rs`，`control/` 与 `observe/` 两层各自 `use` 它，
/// 编译器兜住、漂不了）。本份是**第三份**，它跨的是二进制，两条路都量过：
///
/// - **乙 · 放进某个已有的 `*-core` 共享 crate**（那是本仓治「五份逐字节相同」的成方，
///   `shell-quote-core` 的头注逐字写着「两个二进制不共享源码树 ⇒ 共享 crate 是唯一载体」）。
///   逐个对职责，**七个都装不下**，其中两条是硬的、不是口味：
///   ① `guard-core` 在 daemon 那侧只是 `[dev-dependencies]` ⇒ 生产段**引不到**它，
///      而本 const 恰恰长在生产段（结构性不可能，不是取舍）；
///   ② `gate-core` 的边界头注逐字是「**本 crate 只判，不取**」，而「怎么起 tmux」正是**取**那一侧
///      （它自己接着写：把取值塞进来「共享当场破掉」）；
///   ③ `shell-quote-core` 头注逐字「**本 crate 只剩这一件事**」，而它缩到只剩一件的理由
///      逐字是「那批东西**不是共享的，是没处放的**」—— 往它塞一个无关口径，
///      正是 `P4b` 刚治完的那个病复发；
///   ④ `branch-core` / `usage-core` / `acct-core` / `creds-core` 是各自的域（分叉记录变换 /
///      用量 / 账号 / 凭据），职责上不沾。
///   ⚠ **新开一个 crate 不在本拍的选项里**（要动 `src/bridge/Cargo.toml` 的 workspace，PM 未裁）。
/// - **甲 · 留在这里 + 一条跨仓对拍**（本拍选它）。它不是妥协，有两条正面理由：
///   ① 本仓对**同一族**的同一个问题已经这么解过两次，而且就在本文件里：
///      `tmux_ls_fmt_double_write_point_stays_in_sync` 与
///      `observation_tokens_double_write_point_stays_in_sync` —— 那两条钉的
///      `TMUX_LS_FMT`、`OBS_*` 与本 const **读的是同一条 `tmux ls` 输出**。
///      同族的第三个口径用同一种机制钉，读的人对得起来；
///   ② 对拍的作用域**说得清**：它读两棵树、跑在 monitor 那格 cargo 里
///      （逐字见 `utf8_client_kou_jing_has_one_home_and_this_side_matches_it` 的头注）。
const UTF8_CLIENT_FLAG: &str = "-u";

/// ★★ **K-R12 `J1`：段数下溢 —— 「拆不出段」不许被当成好数据。**
///
/// > 按 TAB 切 tmux 的打印通道，切出的段数 **< 预期 N** ⇒ 出声 **+ 拒绝把这行当好数据**。
///
/// **为什么是「下溢」而不是「恰好 N」**：实测**合法内容只会把段数推高，永远不会推低** ——
/// 会话名里的真 TAB 被 tmux 转义成字面 `\t` 两个字符（那行仍是 6 段），
/// 而 `pane_current_path` 里带真 TAB 的目录会切出 **7** 段。
/// ⇒ `< N` **零误报**；`!= N` **会误伤**（[`parse_tmux_ls`] 今天就在犯，见那边头注）。
///
/// **为什么下溢是完备检测器**：sanitize 是**每客户端全有全无**的 ⇒ 通道一脏，
/// 六个 TAB **全部**消失，段数必然从 6 塌到 1。**没有「内容被改写了但 TAB 还在」的中间态**
/// ⇒ 一条判据同时盖住「分隔符被吞」与「内容被改写」两半，**格式串一个字节不用动**。
///
/// ⚠ **K-R12 下一拍（09-04）订正这条边界**：daemon 那两份已经归位到**一个家**
/// （`src/backend/common/tmux_utf8.rs::tab_underflow`）——
/// 上一拍这里写的「各另有一份」今天只剩**跨仓那一份**（就是本函数）。
/// 两侧同形由 `utf8_client_kou_jing_has_one_home_and_this_side_matches_it` 对拍着
/// （它把本函数的**函数体**当成被比的东西，不是名字）。
fn tmux_tab_underflow(line: &str, expected: usize) -> bool {
    line.split('\t').count() < expected
}

/// 一个远端 tmux 会话(反查 + 未来管理用)。
///
/// G6：加 ts-rs 导出。此前前端在 `tabs.ts` 里**手抄了一份同名 interface**——两份各写各的，
/// 谁也不知道对方漂了没。既然要把它搬进包装层（`ipc/commands.ts` 的返回类型一律用生成物），
/// 顺手把手抄那份也换成生成物。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct TmuxSession {
    pub name: String,
    pub path: String,
    pub command: String,
    pub attached: bool,
    pub windows: u32,
    /// F74:`@ccm_sid` user option——此 tmux 当前所跑 CC 会话的 sid(`__ccm_rbind` 写,随
    /// `/branch` 漂移实时更新)。未设置(空串)→ `None`。cc-monitor 用它精确认「哪个 tmux 跑
    /// 目标 sid」,取代按目录/名字取第一个(同目录多 claude 会撞错会话)。
    pub sid: Option<String>,
}

/// 解析 `tmux ls -F '<TMUX_LS_FMT>'` 输出(真 TAB 分列)。字段数不符 / name 空的行跳过
/// (半截行、非法行不进结果);windows 非数字回退 0;末列 `@ccm_sid` 空串→ `None`。
///
/// # ★ K-R12（09-04）：那个 `!= 6` 是**两件事**，本拍把它们分开说，处置**都不动**
///
/// 上一拍量出来（`lab2.sh` E12 实测）：**合法内容只会把段数推高，永远不会推低**。
/// 于是 `f.len() != 6` 这一条同时挡着**方向相反**的两种行：
///
/// | | 什么时候发生 | 今天的处置 | 本拍怎么办 |
/// |---|---|---|---|
/// | **下溢** `< 6` | 通道被改写（客户端不是 UTF-8 ⇒ TAB 变 `_`），**六列塌成 1 段** | 丢行（对） | **加一句话**（原来完全静默） |
/// | **过溢** `> 6` | `pane_current_path` 里有真 TAB —— **一个带 TAB 的目录名就够**（实测切出 7 段） | 丢行（**误伤**：整个会话从界面上消失） | **只加一句话，处置不改**（理由见下） |
///
/// 🔴 **过溢那条为什么本拍不修**（这是个刻意的裁定，不是漏）：
/// 1. **它与本件的失效方向相反。** 本件是「通道脏」（内容不可信），过溢是「内容完全合法」。
///    把两者的处置搅在一起，会让「下溢判据零误报」这个论证失去干净的边界。
/// 2. **正确的重组要引入一个新假设**：得先钉死「这六列里只有 `pane_current_path`
///    可能含真 TAB」，才能从两端往中间拼（`name` 取头、`attached`/`windows`/`@ccm_sid`
///    取尾、中间归 path）。那个假设**本拍没有实测**，`pane_current_command` 那一列尤其没量过。
/// 3. 它需要自己的验收（带 TAB 的目录名那条 e2e）⇒ **该单独立件**，不该搭本件的车。
///
/// ⇒ 本拍买到的是：**它不再是静默的**。谁被它丢掉、因为哪个方向丢的，日志里说得出来。
///
/// ⚠ 另记一条边界：`f.len() != 6` 里的**下溢**这半在 [`list_remote_tmux`] 那条路上
/// **已经走不到**了（`J1` 在 raw 入口就把整份判废、回 `Err`）。它在这里仍然必须留着 ——
/// 本函数还吃 **daemon 推来的 `tmux_sessions` 帧**那份 raw，而那条路的入口不在本文件里
/// （daemon 侧由 `watcher.rs::classify_tmux_probe` 把关；**老 daemon 没有那道关**）。
pub fn parse_tmux_ls(output: &str) -> Vec<TmuxSession> {
    output
        .lines()
        .filter_map(|line| {
            if line.is_empty() {
                return None;
            }
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() != 6 || f[0].is_empty() {
                // K-R12：处置照旧（丢行），**但不再静默** —— 两个方向分开报。
                // 只有段数不对才报；`name` 空是另一档（半截行），本来就该安静丢掉。
                if f.len() < TMUX_LS_FMT_FIELDS && !line.trim().is_empty() {
                    tracing::warn!(
                        "CCM_TMUX_UNPARSABLE parse_tmux_ls 段数下溢（{} < {TMUX_LS_FMT_FIELDS}）—— \
                         tmux 打印通道被改写（K-R12），整行丢弃。原样行：{line:?}",
                        f.len()
                    );
                } else if f.len() > TMUX_LS_FMT_FIELDS {
                    tracing::warn!(
                        "parse_tmux_ls 段数过溢（{} > {TMUX_LS_FMT_FIELDS}）—— 多半是 \
                         `pane_current_path` 里有真 TAB（合法内容），**这一行的会话会从界面上消失**。\
                         这是 K-R12 §5.4 点名的误伤，处置待单独立件。原样行：{line:?}",
                        f.len()
                    );
                }
                return None;
            }
            Some(TmuxSession {
                name: f[0].to_string(),
                path: f[1].to_string(),
                command: f[2].to_string(),
                attached: f[3] == "1",
                windows: f[4].parse().unwrap_or(0),
                // 只认合法 sid 字符集 [A-Za-z0-9_-]:空串(未设 @ccm_sid)当 None;含别的字符也当
                // None——**极老 tmux(<3.0)可能不展开 `#{@ccm_sid}`、原样保留字面 `#{@ccm_sid}`**
                // (含 `#{}`),若当成 sid 会让 `findClaudeTmux` 的 anySidKnown 恒真 → 老 wrapper 用户
                // 永远走不到 cwd 回退。字符集校验一并挡掉未展开格式串与任何杂质(§30 见 src/doc/INVARIANTS.md)。
                sid: if !f[5].is_empty()
                    && f[5]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    Some(f[5].to_string())
                } else {
                    None
                },
            })
        })
        .collect()
}

/// 列远端 tmux 会话(通道 B,一次性 exec)。`command -v tmux` 门控:无 tmux → 哨兵 `NO_TMUX`
/// → 返 `None`(前端隐藏 attach 项);有 tmux 但无会话 → `Some(空)`。
#[tauri::command]
pub async fn list_remote_tmux(origin: String) -> Result<Option<Vec<TmuxSession>>, String> {
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
    // `tmux ls` 无会话时非零退出("no server running")→ `|| true` 吞掉,得空输出=空列表。
    // K-R12：`-u` 在子命令**之前**（`tmux ls -u -F` 是 rc=1 的响错）。见 `UTF8_CLIENT_FLAG`。
    let cmd = format!(
        "if command -v tmux >/dev/null 2>&1; then tmux {UTF8_CLIENT_FLAG} ls -F '{TMUX_LS_FMT}' 2>/dev/null || true; else printf 'NO_TMUX\\n'; fi"
    );
    let stream = ssh_source::connect_and_exec_cmd(&cfg, &cmd).await?;
    let mut reader = BufReader::new(stream);
    // lossy 解码(对齐全批 exec 输出读取:非 UTF-8 字节不该整体失败)。
    let mut buf: Vec<u8> = Vec::new();
    reader
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("读 tmux 列表失败: {e}"))?;
    let out = String::from_utf8_lossy(&buf);
    if out.trim() == "NO_TMUX" {
        return Ok(None);
    }
    // ★★ K-R12 `J1`：**raw 的入口**（这条路上 raw 只从这里进）。段数下溢 ⇒ 通道被改写。
    //
    // 🔴 处置必须是 `Err`，不能是 `Ok(Some(vec![]))`：后者与「远端真的一个会话都没有」
    // **逐字相同**，正是本件那条「没有任何判据看得见它」的成因。也不能是 `Ok(None)` ——
    // 那一档的语义是「远端没装 tmux」（前端据此隐藏 attach 项），同样是另一件事。
    // ⇒ 三档不共用读数：没装 tmux = `None`、零会话 = `Some([])`、通道脏 = `Err`。
    // 调用方（`tabs.ts` / `fork-flow.ts`）对 `Err` 一律走失败路径 ⇒ **fail-closed**。
    if let Some(bad) = out
        .lines()
        .find(|l| !l.trim().is_empty() && tmux_tab_underflow(l, TMUX_LS_FMT_FIELDS))
    {
        return Err(format!(
            "CCM_TMUX_UNPARSABLE tmux ls 有行切出 {} 段 < {TMUX_LS_FMT_FIELDS} —— \
             远端 tmux 的打印通道被改写（K-R12：客户端不是 UTF-8 ⇒ TAB 与非 ASCII 变 `_`）。\
             这一趟的会话列表**整份作废**，不当成「远端零会话」。原样回包：{bad:?}",
            bad.split('\t').count()
        ));
    }
    Ok(Some(parse_tmux_ls(&out)))
}

/// P3-刀2-UI：**本机今天有哪些 tmux 会话** —— 与远端 `list_remote_tmux` 同形。
///
/// # 为什么不是 `list_remote_tmux` 加一条本机分支
///
/// 那条今天对 `<local>` 会去 `load_remote_config_by_label("<local>")`，报
/// **「未找到远端配置: "<local>"」** —— 一句与真实原因毫无关系的错（同 P3 刀 2 在 `daemon_kill`
/// 那里治过的形态）。但**不能**简单地给它加一条读快照的本机分支：
/// `tabs.ts::awaitExitFor` 等的是「**pane 前台命令**从 claude 变回 shell」，
/// 而那个变化**不触发任何 tmux hook** ⇒ 快照在那个场景下永不刷新
/// ⇒ 本机会退化成「每次都等到 10s 超时再降级 kill」（`ssh_source::tmux_raw_registry` 头注
/// 逐字记着这条，devbench F08 已裁「刻意不开 IPC 出口」）。
///
/// # 那为什么本条可以开
///
/// **因为它问的是另一个问题。** 那条裁定的论据是「快照对 *pane 前台命令变化* 不刷新」；
/// 本条要的是**会话名的集合**，而快照的刷新正由 tmux hook 驱动，
/// `HOOK_EVENTS` 逐字是 `["session-created", "session-closed", "session-renamed"]`
/// —— **恰好就是改变名字集合的那三件事**。
/// ⇒ 对「哪些名字被占了」，这份快照不是陈旧的，是**权威的**。
/// 由 `the_name_set_question_is_exactly_what_the_hooks_cover` 钉住这条推理的前提。
///
/// # 为什么必须有它
///
/// 两个消费者，问的是同一件事的两半：
/// ① **铸名**（P3t-Y2b）：`mintTmuxName` 要一个 `existing` 集合。远端从 `list_remote_tmux` 拿；
///    本机没有 SSH 那条路，不给读口就只能「不避让」= issue #76。
/// ② **杀会话的菜单**（P3 刀 2 的 UI 半）：要认出「哪个 tmux 跑着本 tab 的 sid」。
///    这一格**必须有 `@ccm_sid`，光有名字不行** —— 按 `<sid8>-cc` 前缀去猜，
///    与 `INVARIANTS §30` 逐字禁的「按目录回退猜」是同一类错（都是拿命名巧合当身份）。
///
/// ★★ **它从「只回名字」放宽到「回整条会话」是 P3 刀 2 的 scope-changed**，理由如上 ②。
/// 放宽**没有**碰 devbench F08 锁住的那扇门 —— 那条锁的是「拿这份快照替换 `awaitExitFor`
/// 那个 1s 轮询」，而 `awaitExitFor` 等的是 **pane 前台命令**变化（无 hook ⇒ 快照对它永不刷新）。
/// 本条的两个消费者都不问那个：①问名字集合、②问 `@ccm_sid` 归属，
/// 而这两样都由 `session-created/closed/renamed` 三条 hook 覆盖。
///
/// ⚠ **诚实边界**：返回值里的 `command` 那一列**可能是陈旧的**（它正是无 hook 的那一列）。
/// 后果是菜单上「杀死会话」与「kill 空 tmux」的**文案**可能选错一个，kill 本身照样打得中。
/// ⇒ 依赖 `command` 判活的流程（换号重启的 `awaitExitFor`）**不许**改读本机这条，
/// 它今天由 `tabs.ts` 的 `origin === null` 闸挡着（A7 前不支持本地重启）。
///
/// 拿不到快照（本机 daemon 通道没起 / 还没推过帧）⇒ 回 `None`，**不是空表**：
/// 空表会让调用方以为「一个会话都没有」，那是把「不知道」当成「知道没有」。
#[tauri::command]
pub fn list_local_tmux() -> Option<Vec<TmuxSession>> {
    let raw = ssh_source::tmux_raw_for(crate::inbound_client::LOCAL_ORIGIN)?;
    Some(parse_tmux_ls(&raw))
}

// ---------- P1（zero-poll-liveness）：`TmuxSessions.observation` 的取值 ----------
//
// **第三个双写点**（前两个：`TMUX_LS_FMT` · `NO_TMUX` 哨兵）。monitor 与 daemon 分属两个
// 独立 crate、不能共享类型，所以这三个字符串两侧各写一份，由
// `observation_tokens_double_write_point_stays_in_sync` 测试逐字节钉住（同 `TMUX_LS_FMT`
// 那条守卫的做法：`include_str!` daemon 源 + 锚定 const 定义行，改任一侧忘同步即红）。
//
// **为什么用字符串而不是布尔**：P3 会加「server 已死」vs「server 活着但零会话」的细分
// （两者对 retire 决策等价、只对复活监视有意义）。字符串枚举加一个取值是 additive；
// 布尔字段加第二个就得改帧形状。
/// daemon 确证零会话（`tmux ls` rc=0 但 stdout 空 = `exit-empty off`；或 rc=1 = server 不在）。
const OBS_ZERO_SESSIONS: &str = "zero_sessions";
/// 远端没装 tmux（`command -v tmux` 失败）——与既有 `NO_TMUX` 哨兵同义，显式化。
const OBS_NO_TMUX: &str = "no_tmux";
/// 观测无效（`tmux ls` 以非 0/1 退出、或 exec 本身失败）⇒ 必须跳过，**绝不当零会话**。
const OBS_UNOBSERVABLE: &str = "unobservable";

/// P1（zero-poll-liveness）：一帧 `TmuxSessions` 的**观测分类**结果。
///
/// 存在的理由：这个判断原先是 `ssh_source::stream_loop` 里那条
/// `if raw.trim() != "NO_TMUX" { … if !backend.is_empty() { … } }` 内联 if——
/// 它把**五种语义完全不同的观测压成两条路**，而且住在一个需要真远端连接的
/// `async fn` 里、单测碰不到。提成纯函数后生产与测试走同一条路径。
/// P8c：`Skip` 的原因。**机器可读**（进日志后要能被 grep/统计），不是给人读的句子。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkipReason {
    /// 远端没装 tmux（旧 daemon 的 `NO_TMUX` 哨兵，或新 daemon 的 `no_tmux`）。
    NoTmux,
    /// daemon **自报**这一轮观测失败（`unobservable`）—— 那正是 `#82` 想知道频率的那一格。
    Unobservable,
    /// 旧 daemon 的空串歧义：零会话与「`|| true` 吞掉的错」同形 ⇒ 保守跳过。
    /// **新 daemon 走不到这里**（它零会话报 `zero_sessions`、出错报 `unobservable`）。
    LegacyAmbiguousEmpty,
}

impl SkipReason {
    /// 进日志用的稳定标识。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SkipReason::NoTmux => "no_tmux",
            SkipReason::Unobservable => "unobservable",
            SkipReason::LegacyAmbiguousEmpty => "legacy_ambiguous_empty",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TmuxObservation {
    /// 有效观测：某后端自报正在跑的 sid 集。
    ///
    /// **可能为空集**——空集 = daemon **确证**该主机零会话（不是"观测失败"）。
    /// 空集照常进对账、照常累计缺失，**这正是 P1 修掉的那个 bug**：
    /// 原先空 backend 一律保守跳过 ⇒ 当被杀的是该 origin 最后一个 tmux 会话时
    /// （server 随之退出、`tmux ls` 回空）⇒ 对账整段跳过 ⇒ idle 灰灯**卡到断连才清**。
    Backend(std::collections::HashSet<String>),
    /// 观测无效 ⇒ 本轮跳过、**不累计缺失**（否则 ssh 抖动会批量误灰）。
    ///
    /// ★★ **P8c（`U3` 08-11 的裁定）：它带上「为什么」。**
    ///
    /// 原来三种完全不同的原因（远端没装 tmux / daemon 自报观测失败 / 旧 daemon 的空串歧义）
    /// 被压成同一个无载荷的 `Skip` ⇒ **这一维在日志里根本不可见**。
    /// `U3` 的读数逐字记着这件事的后果：
    /// 「`Unobservable` 计数 = 0，而**那个 0 是瞎的** —— 日志根本不记这一维 ⇒ 分母不存在。
    /// 『0 次』与『记不下来』在这份数据里长得一模一样，而后者才是事实」。
    ///
    /// ⇒ 裁定是「**先补一行可观测性，让这个数变得可测**，再拿真实使用量去裁 `#82`」。
    /// 载荷就是那一行的原料。取值是**稳定的机器可读串**（不是给人读的措辞）。
    Skip(SkipReason),
}

/// P1：把一帧 `TmuxSessions` 分类。`observation` = daemon 的显式分类字段
/// （P1 起的 additive wire 字段；旧 daemon 为 `None`）。
///
/// **判据只用 rc + stdout 空否**（daemon 侧已折成 `observation`），**绝不看 stderr 文本**——
/// P0 实测 stderr 有两种措辞（`no server running on …` / `error connecting to … `），
/// 且拿英文消息当判据本身就是错的。
///
/// 未知的 `observation` 取值 → **落回 raw 判据**（向前兼容：未来 daemon 加新分类时，
/// 老 monitor 退化成今天的保守行为，不会误灰）。
pub(crate) fn classify_tmux_observation(raw: &str, observation: Option<&str>) -> TmuxObservation {
    // NO_TMUX 哨兵（旧 daemon 唯一能表达的"后端不存在"）：远端没装 tmux ⇒ 无从对账。
    if raw.trim() == "NO_TMUX" {
        return TmuxObservation::Skip(SkipReason::NoTmux);
    }
    // P1：daemon 的显式分类优先。**未知取值刻意不在此匹配** ⇒ 落回下方 raw 判据（向前兼容）。
    match observation {
        // ★ 两者压成一个 `Skip` 是对的（都不该累计缺失），但**原因必须分开记** ——
        // `#82` 要知道的正是 `unobservable` 的频率，把它和「远端没装 tmux」混在一起就问不出来。
        Some(OBS_NO_TMUX) => return TmuxObservation::Skip(SkipReason::NoTmux),
        Some(OBS_UNOBSERVABLE) => return TmuxObservation::Skip(SkipReason::Unobservable),
        // 只在 raw 确实为空时认这条——帧内部自相矛盾（说零会话却带着会话行）时以
        // **数据**为准、落回 raw 判据，不凭一个字符串把明明在跑的会话判死。
        Some(OBS_ZERO_SESSIONS) if raw.trim().is_empty() => {
            return TmuxObservation::Backend(std::collections::HashSet::new());
        }
        _ => {}
    }
    let backend: std::collections::HashSet<String> = parse_tmux_ls(raw)
        .iter()
        .filter_map(|s| s.sid.clone())
        .collect();
    // 旧 daemon 的空串语义不可分（零会话 / `|| true` 吞掉的错，两者同形）⇒ 保守跳过。
    // **新 daemon 走不到这里**：它零会话时带 `zero_sessions`、出错时带 `unobservable`。
    if backend.is_empty() {
        return TmuxObservation::Skip(SkipReason::LegacyAmbiguousEmpty);
    }
    TmuxObservation::Backend(backend)
}

/// daemon 那条**抓一屏**原语的名字（`K-R86` 出 CLI 面 · `K-R104` 搬上帧面）。
///
/// 闭集只许有一个住址：调用点不写字面量。
const CAPTURE_PANE: &str = "capture-pane";

// ★★ `K-R112`（09-13）：**这里原来住着 `classify_capture_output`（两个哨兵的判定）**。〔散文墓碑〕
//
// 它读的是那条一次性 SSH 串的 stdout：`NO_TMUX`（没装 tmux）/ `NO_PANE`（会话不存在**或**
// 抓屏失败）。⚠ 它自己的头注逐字承认过一个边角：「pane 内容 `trim_end` 后恰等于某哨兵串
// → 误判」—— 那不是概率问题，是**把答案编码进 stdout** 这种做法的固有形状。
//
// 换成帧面之后**答案与内容分开走**（`{name, screen}` ＋ 一个错误码），
// 于是「屏幕上恰好只有 NO_PANE 这几个字」再也不是一次误判。
// 五档怎么分见 [`describe_capture_refusal`]。

/// 把 `capture-pane` 的**拒绝码**讲成人话 —— 纯函数。
///
/// 🔴 **五档分得开，这才是换掉那条 SSH 串真正买到的东西。**
/// 老那条串只有两个哨兵，而「tmux 没装」「一个 server 都没有」「这个会话不存在」
/// 「抓屏本身失败」四件事里有三件被压进 `NO_PANE` 一个读数 ——
/// 它们的下一步各不相同（装 tmux / 那台机器上没有会话在跑 / 刷新列表 / 看 tmux 原话）。
/// 那五个码是 daemon 按**退出码 ＋ stderr 命中哪张针表**分出来的
/// （`src/backend/control/capture_pane.rs` 头注那张表），不是这一侧猜的。
///
/// ⚠ **认不出的码不许猜**：原样带出去。「压成一个具体而错误的答案」正是 daemon 那侧
/// 兜底档（`capture_failed` ＋ stderr 原样回包）写下来要避免的形状 —— 这一侧照抄那条纪律。
fn describe_capture_refusal(target: &str, code: &str, message: &str) -> String {
    match code {
        "no_tmux" => format!("抓不了 `{target}` 的画面：那台机器上起不来 tmux（{message}）"),
        // ⚠ 「tmux 的 server」不是啰嗦：写成「tmux server」会被 `tmux_daemon_gate_guard`
        //    那条「远端 tmux 动词」守卫读成一个叫 `server` 的动词（它按字面「tmux 空格 小写词」
        //    取，刻意不问上下文 —— 那是它 fail-closed 的方式）。**说的是同一件事，不许改语义。**
        "no_server" => format!(
            "抓不了 `{target}` 的画面：tmux 在，但那台机器上**一个 tmux 的 server 进程都没有**（{message}）"
        ),
        "no_such_session" => {
            format!("抓不了 `{target}` 的画面：这个会话不存在，可能刚结束（{message}）")
        }
        "invalid_args" => format!("抓不了 `{target}` 的画面：这个会话名后端不收（{message}）"),
        "capture_failed" => format!("抓 `{target}` 那一屏失败了，tmux 原话：{message}"),
        other => format!("抓 `{target}` 那一屏被拒：{other}：{message}"),
    }
}

/// F01：tmux `-t <target>` 的**精确匹配**包装。
///
/// **裸 `-t <名>` 不是精确匹配**：tmux 依次按「精确名 → **名字开头** → **glob**」解析。
/// 实测（tmux 3.6，隔离 `-L` socket）——只有 `sib-2` 存在时：
///   - `kill-session -t sib` → **杀掉 `sib-2` 且 rc=0**（当成功回报）
///   - `send-keys -t sib 'HELLO' Enter` → 投进 `sib-2`
///   - `capture-pane -p -t sib` → 抓的是 `sib-2`
///   - `kill-session -t 'si*'` → glob 命中并杀掉
/// 本仓必然踩：`pickFreshTmuxName` 刻意造 `<sid8>-cc-2/-3`，终端 `cct` 造 `<dir>_cc-2/-3`。
///
/// **为什么是 `=name:` 而不是 `=name`**（别"简化"掉尾冒号）：`=` 前缀只在 target-**session**
/// 解析路径上被识别。`send-keys`/`capture-pane` 收的是 target-**pane**，`set-option`/`show-options`
/// 走 pane 解析后上溯——这些路径上 `=name` 直接 `can't find pane`、**rc=1 完全失效**（实测）。
/// 尾冒号把串强制成 `session:` 形态（当前 window、活动 pane），`=` 才落在会话名段上被正确识别。
/// `=name:` 是唯一在全部动词上都既通用又精确的形式。矩阵见 `.claude/planned-build/unify-launch/MASTERPLAN.md` §5.3。
///
/// 删掉它会让换号重启把 `/exit` 敲进**兄弟会话里还活着的 claude** 并 kill 它，而 UI 报告「已重启」。
///
/// F04 Gate 1（恒强制）：**空 target 必须被拒**——`=:` 会被 tmux 解析成「当前会话」，是本模块
/// 唯一真正的危险默认值（今天 `capture_remote_pane` 是唯一无门的入口，见其函数头注）。
///
/// **只查空串，不额外收紧字符集**——glob/元字符（`*`/`;`/`$`/空格）不在这里挡：`shell_quote`
/// 已经把任意内容安全引号化（不会脱出 shell），字符集层面的收紧是**另一层职责**（TS 侧
/// `isValidNewTmuxName` 只在**创建路径**禁 glob，`isValidTmuxName` 对 attach 到已有会话故意
/// 宽松——见 INVARIANTS §31a"第二道防线"）。这里若也收紧字符集会让 `si*` 这类合法 attach 目标
/// （已有会话名里含 glob 字符）在 Gate 1 就被拒，与既有 `tmux_targets_use_exact_match` 测试
/// 钉死的"glob 名被引号原样包住、不脱出"这一既定行为冲突——**空** 是唯一需要在这一层拦的语义
/// 陷阱（`=:` 落到当前会话），其余交给引号化 + 上层校验。
fn is_safe_tmux_target(target: &str) -> bool {
    !target.is_empty()
}

/// Gate 1（恒强制）的**谓词本体**：**只拒空 target**——`=:` 会被 tmux 解析成「当前会话」，
/// 是唯一真正需要在这一层拦的语义陷阱（判据 [`tests::gate1_rejects_only_empty_target`]）。
///
/// ⚠ `K-R72`（09-12）把它从 [`exact_target`] 里**分出来（不是复制一份）**：
/// `exact_target` 产的是**给 shell 用的**精确串 `'=<名>:'`，而送键 / 杀会话今天走后端、
/// 不拼任何 shell 串 —— 让它们为了一次校验去要一个用不上的串，就是本区反复判过的那条
/// 「一个值装了两件事」。⇒ **谓词一份，住这里**；[`exact_target`] 调它，
/// 三条路（capture-pane · send-keys · kill）走的是同一份判定、同一句话。
fn gate1_reject_empty(target: &str) -> Result<(), String> {
    if !is_safe_tmux_target(target) {
        return Err(format!("非法 tmux 目标（空）：{target:?}"));
    }
    Ok(())
}

// ★★ `K-R112`（09-13）：**这里原来住着 `build_capture_pane_cmd`**（抓屏那条 SSH 串的构造器）。〔散文墓碑〕
//
// 它是 [`exact_target`] 在 monitor 侧**最后一个**消费者（`K-R72` 走了另外三个）。
// 抓屏改走 `capture-pane` 帧之后它零生产调用点 ⇒ 整块删（`KR72D1` 逐字禁止
// 「把回落改成恒失败的桩留在原地 —— 那不是删，那是把一份实现变成一句谎话」）。
//
// 🔴 **那条「必须精确匹配」的性质今天真正在跑的那一份住 daemon**：
// `src/backend/control/capture_pane.rs::capture_on` 逐字
// 「Gate 1（`=name:` 精确匹配）—— 裸 `-t <名>` 会被 tmux 按『精确名 → 名字开头 → glob』解析」，
// 它调的是 daemon 自己那份 `launch::exact_target`，与 `kill` 共用同一份。
//
// **Gate 1 的空目标那一格仍留在本侧**（[`gate1_reject_empty`]）：`=:` 会被 tmux 解析成
// 「当前会话」，而**空目标不该先花一次往返**才被拒 —— 三条命令（抓屏 / 送键 / 杀会话）
// 今天都在本地就地判它，同一份判定、同一句话。

/// F04：`exact_target` 是 fallible——Gate 1 折进这一个函数本身。
///
/// **裸 `-t <名>` 不是精确匹配**：tmux 依次按「精确名 → **名字开头** → **glob**」解析，
/// 只有 `sib-2` 存在时 `-t sib` 命中的是 `sib-2`（tmux 3.6 实测）。
/// 尾冒号不能省：`send-keys`/`capture-pane` 收的是 target-**pane**，`=名`（无冒号）rc=1 完全失效。
///
/// # 🔴 `K-R112`（09-13）：**本函数今天没有生产调用方了 —— 而它必须留着，理由如实写**
///
/// 最后一个消费者 `build_capture_pane_cmd` 随抓屏改走帧面一起删了（上面那块墓碑）〔散文墓碑〕。
/// **不能顺手删掉它**：daemon 侧
/// `control/launch.rs::tests::exact_target_shape_matches_the_monitor_side`
/// 拿 monitor 这一处当**跨轨对拍锚点** —— 它 `include_str!` 本文件，要求里面找得到
/// `={target}:` 那个形状，理由逐字「两侧必须同形，否则一边打到兄弟会话上而另一边不会」。
/// 而**那条性质今天仍然成立、仍然值得守**（daemon 的 `capture_pane` / `kill` 都在用它那份）。
/// ⇒ 留下这个壳，`#[allow(dead_code)]` 明写「今天没人调」，**不假装它在路上**
/// （形状抄 `K-R72` 给 `is_ccm_tmux_name` 那个转调壳留的先例）。
///
/// ⚠ **想真删它，得先在 daemon 那棵树上给那条对拍换个锚点** —— `src/backend/**`
/// 不在本件写区（归 `K-R113`）。**已上报，别当它没有主人。**
#[allow(dead_code)]
pub(crate) fn exact_target(target: &str) -> Result<String, String> {
    gate1_reject_empty(target)?;
    Ok(ssh_source::shell_quote(&format!("={target}:")))
}

/// 「后端通道不在」这一档的**唯一一份**用户可见文案（`K-R72`，09-12）。
///
/// # 它是那笔代价的出口
///
/// ⚠〔`K-R112` 09-13〕**第三个消费者进来了**：抓屏（`capture_remote_pane`）也走这一份。
/// 它的代价与那两条不同、要单记：抓屏此前对 `<local>` 是**一句「还看不了」**（根本没有那条路），
/// 今天两侧同一条路 ⇒ 本机从「做不到」变成「做得到，除非后端没起来」。
///
/// 送键与杀会话删掉过渡期 SSH 回落之后，`NoChannel` 从「换条路悄悄做掉」变成**明确失败**
/// （`K-R54` 表第 1 处逐字点的代价）。受影响的真实动作是换号重启那三处
/// （`src/account-restart.ts` 的 `/compact` · `Escape` · `/exit`）加上 kill 那处，
/// 它们**各自把这个串原样弹成 toast** ⇒ **这一句就是用户看得见的那句话**，
/// 不许让它静默、也不许让它说一件与真实原因无关的事。
///
/// # 为什么两条路的话不一样
///
/// 先前那条 SSH 回落对 `<local>` 会去 `load_remote_config_by_label("<local>")` 拿不到东西，
/// 报一句 **「未找到远端配置: `"<local>"`」** —— 与真实原因（本机后端通道不在）毫无关系。
/// **错的诊断比没有诊断更贵**：那正是 [`tests::the_local_kill_never_falls_back_to_ssh`]
/// 与 [`tests::the_local_send_keys_never_falls_back_to_ssh`] 当初买下来的东西。
/// 今天那条 SSH 路整个没了，两条判据改钉「**盘上没有第二条路**」，而这里保住它们买到的
/// 另一半：**本机与远端的下一步不同，话就不许一样**。
///
/// ⚠ 射程与那两条判据同源：比的是 **origin 逐字节等于哨兵串**，不是 target 会话名。
/// 一台 label 起成 `localhost` / 本机主机名的**远端**走的是下面那半 —— 那是对的，
/// 它确实是一条远端传输。
fn no_channel_message(action: &str, origin: &str, target: &str, why: &str) -> String {
    if origin == crate::inbound_client::LOCAL_ORIGIN {
        format!(
            "本机后端通道不在，{action} `{target}`：{why}\n\
             （抓屏、送键与杀会话只走后端这一条路 —— 先让本机后端跑起来。\
             K-R72 / K-R112 起没有 SSH 兜底那条路了，所以这不是「再试一次」能过去的）"
        )
    } else {
        format!(
            "`{origin}` 的后端通道不在，{action} `{target}`：{why}\n\
             （抓屏、送键与杀会话只走后端这一条路 —— 先让那台机器上的后端连上。\
             K-R72 / K-R112 起没有 SSH 兜底那条路了，所以这不是「再试一次」能过去的）"
        )
    }
}

// ★★ `K-R72`（09-12）：**这里原来住着送键与杀会话那两条桌面侧回落的执行面。**
//
// 走掉的是三个符号 —— `gate_guard_expr`（渲染那条 shell 守卫表达式）、  〔散文墓碑〕
// `PROBE_ONLY_FMT`（只探存在性那一格的格式串）、`build_guarded_tmux_cmd`
// （把 Gate 2 远端半支 + Gate 3 + 动作折成一条原子远端 shell 串），
// 外加它的两个消费者 `build_kill_session_cmd` / `build_send_keys_remote_cmd`。  〔散文墓碑〕
//
// **为什么整块走而不是留个壳**：`K-R54` 的裁定表第 5 处逐字写着
// 「`build_guarded_tmux_cmd` 的**全部消费者就是 kill 与 send-keys 那两条回落**；
// 1、2 删完它自动成为死代码」。而 `KR72D1` 逐字禁止「把回落改成恒失败的桩留在原地」——
// **那不是删，那是把一份实现变成一句谎话**。
//
// **§34 那三道门没有消失，只是只剩一个家**：Gate 1 住 [`gate1_reject_empty`]
// （本文件，三条路共用）· Gate 2 / Gate 3 住 daemon 的 `control/gate.rs`
// （`admit` / `admit_destructive`，判定本体转调 `gate-core`，金表 `gate2-golden.tsv`
// 由 daemon 侧与 `backend/control/gate2_parity.rs` 两条轨道共读）。
// 回潮闸在 `tmux_daemon_gate_guard`：那两条命令里**再出现** `connect_and_exec_cmd` 就红。

/// 抓一屏走 daemon 的 `capture-pane` 帧〔`K-R112` 09-13〕。
///
/// 回 `Err(Routed)` 而不是 `Err(String)`：**「一个字节都没发出去」与「后端说了话」不是一回事**，
/// 而这一层判不了该怎么对用户说 —— 那归调用方（同 `daemon_kill` / `daemon_send_keys` 的分法）。
///
/// ⚠ **分流走那唯一的一份**（`daemon_route::route_call_error`）：本模块不许自己 match
/// 一遍错误枚举 —— 分流规则一旦有第二份实现，「被门拒绝」就会在某一份里被洗成「换条路重做」。
async fn capture_via_daemon(
    origin: &str,
    target: &str,
) -> Result<String, crate::backend::control::daemon_route::Routed> {
    use crate::backend::control::daemon_route::{no_channel, route_call_error, Routed};
    let Some(client) = crate::inbound_client::client_for(origin) else {
        return Err(no_channel(origin));
    };
    // 能力协商放在抓之前：抓一屏是 `K-R86`/`K-R104` 之后才有的原语，老 daemon 上没有。
    // **「这台的后端太旧」是问得出答案的**，不许与超时同形（同 `cc_bus::send_via_daemon`）。
    if !client.accepts(CAPTURE_PANE) {
        return Err(Routed::NoChannel(format!(
            "`{origin}` 的 daemon 没声明 `{CAPTURE_PANE}` 能力 —— \
             抓一屏是后来才上帧面的原语，**重装那台机器的后端**就有了"
        )));
    }
    let reply = client
        .call(
            CAPTURE_PANE,
            crate::inbound_client::capture_pane_args(target),
            std::time::Duration::from_secs(20),
        )
        .await
        .map_err(|e| {
            route_call_error(&e, |code, message| {
                describe_capture_refusal(target, code, message)
            })
        })?;
    // ⚠ **空屏是合法的成功**（daemon 侧头注逐字）：一个刚建起来、什么都没打印的 pane
    //   抓回来就是空串。所以这里判的是**字段在不在**，不是「内容空不空」。
    reply
        .as_ref()
        .and_then(|v| v.get("screen"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            Routed::Refused(format!(
                "抓 `{target}` 的应答里没有 `screen` 字段 —— 这一端与那一端的契约漂开了"
            ))
        })
}

/// F60:抓一个 tmux 会话当前窗口/pane 的屏幕文本(**只读快照,非 attach**)。
///
/// # ★★ `K-R112`（09-13）：**改走 daemon 的 `capture-pane` 帧，本机那一支跟着通了**
///
/// 在本件之前这条命令是**一次性 SSH**〔散文墓碑〕（`build_capture_pane_cmd` 拼一条带 `command -v tmux`
/// 门控与 `NO_PANE` 哨兵的 shell 串 → `connect_and_exec_cmd`），而**本机那一支直接回一句
/// 「还看不了」**。那句话当时是诚实的：daemon 没有抓屏原语。
/// `K-R86`（09-13）补了 CLI 面、`K-R104` 把它搬上帧面 ⇒ **前提到期了**，这一条不再拒本机。
///
/// 换来的三样，逐条都是具体的：
/// 1. **本机能预览了** —— `<local>` 也是一个 origin，`client_for` 两侧都答得出。
/// 2. **五档错误分得开**（[`describe_capture_refusal`]）：老路两个哨兵把四件事压成两个读数。
/// 3. **精确形态只剩一份**：`=name:` 那道 Gate 1 今天只住 daemon 侧
///    （`control/capture_pane.rs::capture_on`），monitor 不再拼第二份。
///
/// ⚠ **代价如实写**：后端通道不在时，抓屏从「远端还能靠一次性 SSH 抓到」变成**明确失败**
/// （出口 [`no_channel_message`]）。远端那一侧这是**净损失一条路**，值不值由这三样换 ——
/// 而 `K33` 逐字「所有命令只许有一处」把它判成了值。
///
/// ⚠ **只读快照，刻意不过身份门（Gate 2）** —— 两侧口径一致，daemon 侧
/// `control/capture_pane.rs::capture` 头注逐字记着同一句。别顺手给它加门。
#[tauri::command]
pub async fn capture_remote_pane(origin: String, target: String) -> Result<String, String> {
    use crate::backend::control::daemon_route::Routed;
    // Gate 1 仍在**本地就地**判（同 `tmux_send_keys` / `kill_remote_tmux`）：
    // 空目标不该先花一次往返（`=:` 会被 tmux 解析成「当前会话」）。
    gate1_reject_empty(&target)?;
    match capture_via_daemon(&origin, &target).await {
        Ok(screen) => Ok(screen),
        Err(Routed::Refused(why)) => Err(why),
        Err(Routed::NoChannel(why)) => Err(no_channel_message("抓不了", &origin, &target, &why)),
        Err(Routed::Done) => {
            Err("抓屏的分流器判成「已完成」，这不该发生 —— 那一档没有屏幕内容可回".to_string())
        }
    }
}

/// F79(#38)：杀死远端 tmux 会话。**破坏性操作**——前端二次确认后才调。
/// 杀完 tab 变灰由 #60-A 的 tmux 存活对账兜（本命令不主动 archive，守 §24）。
///
/// # ★★ `K-R72`（09-12）：**只剩后端这一条路**（`C7` 的过渡期 SSH 回落已删）
///
/// `F04b` 把主路切到 daemon 时留了一条「monitor 自己拼一条 SSH 串杀会话」的过渡期回落
/// （`C7`）。`K-R54` 的逐处裁定表第 2 处判它**留 daemon、删回落**，理由是实的：
/// daemon 那条先 `admit_destructive` 拿 `#{session_id}` **句柄**再杀，而那条 SSH 串杀的是
/// `=name:`（**名字**）—— 破坏性动作对名字下手就把 TOCTOU 窗口留着。
///
/// ⚠ **代价如实写**：远端后端通道不在时，杀会话从「换一条路悄悄做掉」变成**明确失败**。
/// 它会被 `src/account-restart.ts` 原样弹成 `level: "error"` 的「重启已中止」，
/// 所以这一句必须说得出真实原因与下一步，出口是 [`no_channel_message`]。
///
/// ⚠ **三态分流仍在，而且今天三条都 `return`**：`Refused` 是**门做的决定**
/// （`wrong_owner` / `too_many_windows`），`NoChannel` 是**通道不在**。
/// 先前那条回落最危险的形状正是把 `Refused` 洗成另一条路的成功；今天连那条路都没有了，
/// 但两个读数仍然不许合并 —— 由 `tmux_daemon_gate_guard` 那条同名判据钉着。
#[tauri::command]
pub async fn kill_remote_tmux(origin: String, target: String) -> Result<(), String> {
    // Gate 1 仍在**本地就地**判（同 `tmux_send_keys`）。
    gate1_reject_empty(&target)?;
    match crate::backend::control::daemon_kill::daemon_kill(&origin, &target).await {
        crate::backend::control::daemon_route::Routed::Done => Ok(()),
        crate::backend::control::daemon_route::Routed::Refused(why) => Err(why),
        crate::backend::control::daemon_route::Routed::NoChannel(why) => {
            Err(no_channel_message("杀不了", &origin, &target, &why))
        }
    }
}

/// A5：向远端 tmux 会话发按键（换号重启前在旧号上 send `/compact`、优雅退出的 `Escape`/`/exit`）。
/// **只发按键、不杀不建。**
///
/// # ★★ `K-R72`（09-12）：**只剩后端这一条路**（`C7` 的过渡期 SSH 回落已删）
///
/// `F04c` 把主路切到 daemon 时留了一条「monitor 自己拼一条 SSH 串往别人会话里打字」的
/// 过渡期回落（`C7`）。`K-R54` 的逐处裁定表第 1 处判它**留 daemon、删回落**：
/// 那条回落在 `cc-*` 形状名上 `need_sid`/`need_windows` 双 false ⇒ 落退化分支，
/// 而 daemon 的 `control/gate.rs::admit` 恒先 `probe` 拿 `#{session_id}` 句柄。
/// `K-R56`（09-11）先把「探了没有」这一维补齐、让两条路等价，本件才删得掉第二条。
///
/// `enter` 落在 daemon 的**两个 mode 名**上，不是一个字段：
/// `true` → 既有的 `send-into` · `false` → `F04c` 新增的 `send-keys-raw`。
/// **为什么不能是字段**：daemon 的 `parse_request` 手工取键、不 deny unknown fields ⇒
/// 旧版本会**静默忽略**它、照样附 `Enter` ⇒ 把「打断当前回合」变成
/// 「**提交用户输入框里排队的文本**」。新 mode 名则天然 fail-closed：旧 daemon 回
/// `invalid_args`，我们拿到明确错误。
///
/// ⚠ **代价如实写**（`K-R54` 表第 1 处逐字点的那一条）：远端后端通道不在时，送键从
/// 「悄悄发出去」变成**明确失败**。受影响的真实动作是换号重启那三处
/// （`src/account-restart.ts` 里 `/compact` · `Escape` · `/exit`）—— 它们各自把这里的
/// 错误串原样弹成 toast，所以这一句必须说得出真实原因，出口是 [`no_channel_message`]。
///
/// ⚠ **三态分流仍在，而且今天三条都 `return`**：`Refused` 是**门做的决定**，
/// `NoChannel` 是**通道不在**。压成一句就等于把「你不能往那个会话里打字」与
/// 「后端没连上」混成一个读数。
#[tauri::command]
pub async fn tmux_send_keys(
    origin: String,
    target: String,
    keys: String,
    enter: Option<bool>,
) -> Result<(), String> {
    // 缺省（前端旧调用不传）→ true，与 A5 原行为逐字节等价。
    let enter = enter.unwrap_or(true);
    // Gate 1 仍在**本地就地**判：空目标不该先花一次往返（`=:` 会被 tmux 解析成「当前会话」）。
    gate1_reject_empty(&target)?;
    match crate::backend::control::daemon_send_keys::daemon_send_keys(
        &origin, &target, &keys, enter,
    )
    .await
    {
        crate::backend::control::daemon_route::Routed::Done => Ok(()),
        crate::backend::control::daemon_route::Routed::Refused(why) => Err(why),
        crate::backend::control::daemon_route::Routed::NoChannel(why) => {
            Err(no_channel_message("送不了按键给", &origin, &target, &why))
        }
    }
}

/// 本工具建的 tmux 会话名判定：`<X>-cc[-N]` 后缀形（今天产的那种）**或**老的 `cc-` 前缀形，
/// 且只含 `[A-Za-z0-9_-]`。
///
/// ⚠ 〔`K-R96` 09-12 订正〕这一行**原本写反了**：写的是「`cc-` 前缀 + …（`cc-<sid8>[-N]` 恒满足）」，
/// 而 S4b-3b（用户 2026-07-31）早就把前缀反转成了**后缀** —— 判定本体（`gate-core`）两种都认，
/// 只有这句散文停在反转之前。⚠ 而 `<X>` 今天也不是 `<sid8>`：`K-R96` 之后是 **`<项目名>`**
/// （用户 `R55`：「要是可读的名字 / 不要id」）。**sid 不在名字里，它骑在 `@ccm_sid` 上。**
///
/// F04：**不再是唯一身份判据**，降级为 Gate 2（identity）union 的本地半支——`@ccm_sid` 已设
/// 是远端半支（`K-R72` 起只在 daemon `control/gate.rs::admit` 里核验；先前 monitor 侧
/// `build_guarded_tmux_cmd` 那条 SSH 串里还有第二份，随两条回落一起删了）。
/// 命中此判据即可跳过远端核验（零 IO，覆盖今天
/// 100% 的真实流量）；未命中不代表拒绝，只代表"需要问远端 `@ccm_sid`"。**不删除**——F02 之前的
/// 老 `cc-*` 会话没有 `@ccm_sid`，只靠这条名字判据仍必须可 kill/send-keys，否则是向后兼容回归。
///
/// **F03：实现搬进 `gate-core`**（定框 C1「一份代码、两种承载」）—— daemon 的
/// `control/gate.rs` 调的是同一份。本地保留这个名字是为了调用点零改，
/// 同 `shell_quote_core::posix_quote` 的收口手法。
/// ⚠ **不许在这里重新实现一遍** —— `gate_singleton_guard` 机检钉着「全仓只有一份」。
///
/// # 🔴 `K-R72`（09-12）：**本转调今天没有生产调用方了 —— 而它必须留着，理由如实写**
///
/// 它先前唯一的两个调用方是 `build_kill_session_cmd` / `build_send_keys_remote_cmd`  〔散文墓碑〕
/// （算 `need_sid = !name_owned`），两条回落一删就没人调了。
/// **不能顺手删掉它**：`gate_singleton_guard::the_monitor_wrapper_really_delegates`
/// 逐字要求「`tmux.rs` 的生产段里有 `gate_core::is_ccm_tmux_name`」——
/// 那条判据守的是「monitor 侧不许自己再实现一份 Gate 2 身份判定」，
/// 而**那条性质今天仍然成立、仍然值得守**（哪天有人在 monitor 侧重新长出一份，它会红）。
/// ⇒ 留下这个转调壳，`#[allow(dead_code)]` 明写「今天没人调」，不假装它在路上。
///
/// ⚠ **这是一笔如实登记的欠账，不是一个干净的收尾**：更好的形状是把那条守卫改成
/// 「monitor 侧**不许出现**第二份判定」的纯反向锚点（不需要一个转调壳当锚），
/// 但 `gate_singleton_guard.rs` **不在 `K-R72` 的写区** ⇒ 交回 PM（件文件 `§8` 记号 `〔R72c〕`）。
#[allow(dead_code)]
fn is_ccm_tmux_name(name: &str) -> bool {
    // S4b-3b（用户 2026-07-31）的新形 `<X>-cc` 与老形 `cc-*` 都在那边认；
    // 「老形绝不删」的理由（issue #76 失管会话）也逐字写在那边的头注里。
    gate_core::is_ccm_tmux_name(name)
}

/// daemon 侧 `watcher.rs` 的源码路径 —— **跨 crate 硬路径的单一落点**。
///
/// # 为什么要有这个常量
///
/// monitor 的两条对拍守卫用 `include_str!` 读 daemon 的源码（两个 crate 不能共享 `const`，
/// 只能靠「读对方源码 + 断言」防跨语言/跨 crate 漂移）。U2 的 Phase D 审计点名过：
/// **这类硬路径在 daemon 重构时会一起断，而且断的是编译期**。
///
/// U3 把 `watcher.rs` 搬进 `observe/` 时它**当场兑现** —— `cargo test --lib` 直接
/// `couldn't read src/../../backend/observe/watcher.rs`。
/// 好消息是它**响**（编译错，不是静默假绿）；坏消息是它有两处、还散着。收进一个常量，
/// 下次 daemon 再搬家只改这一行。
///
/// ⚠ **必须是 `macro_rules!` 不能是 `const`**：`include_str!` 只接受**字面量 token**，
/// 喂给它一个 `const` 会报 `argument must be a string literal`（我第一版就这么写的）。
/// 宏能展开成字面量，于是既拿到了单一落点、又满足 `include_str!` 的要求。
// 只在 `#[cfg(test)]` 的两条对拍守卫里用 —— 不加这个属性会留一条
// `unused macro definition` 告警（Phase D 审计 I6）。
#[cfg(test)]
macro_rules! daemon_watcher_src {
    () => {
        "../../backend/observe/watcher.rs"
    };
}

#[cfg(test)]
#[path = "../../../tests/bridge/tmux_tests.rs"]
mod tests;
