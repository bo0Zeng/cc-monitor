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
/// POSIX / `C.UTF-8` / `-u` 三种模式逐字节相同 ⇒ [`capture_remote_pane`] 与
/// `account_usage.rs` 的用量探针**不受本件影响**，本拍**刻意不给它们加 `-u`**（不扩面）。
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

/// F10：一次性用量探针会话的命名前缀（`src/bridge/src/account_usage.rs` 唯一使用这个前缀建
/// 会话）。**不**用新 tmux user-option 打标——`TMUX_LS_FMT` 是机器化锁死的"双写点"（红线 I8，
/// 见本文件 `tmux_ls_fmt_double_write_point_stays_in_sync` 测试：daemon 的 `watcher.rs` 也用
/// 这个格式串做自己的 idle-tmux 对账轮询，两侧必须逐字节一致），改格式串代价和风险都不成
/// 比例。探针会话名完全由本功能自己控制，前缀足够独特，用它做识别零风险、不碰任何双写点。
const USAGE_PROBE_NAME_PREFIX: &str = "ccm-usage-";

/// 🔴 **`K-R104`（09-13）：探针会话的名字空间搬家了，本条跟着扩面。**
///
/// 编排搬上 daemon 帧面之后，探针会话**不再由 monitor 自己建**，而是由 daemon 的
/// `oneshot-session` 原语**铸**出来 —— 名字形状是 `ccm-oneshot-<slug>-cc`
/// （唯一住址 `src/backend/control/oneshot_session.rs::ONESHOT_PREFIX`
/// ＋ `ONESHOT_GATE_SUFFIX`）。
///
/// ⚠ **不扩面的后果不是「少过滤一个前缀」**：那条 `-cc` 尾巴让它**过得了** §34 Gate 2
/// 的名字半支（那正是它存在的理由），于是它在用户眼里长得跟一个正牌 `cc-*` 会话一样，
/// 会**混进会话列表**闪现几秒。上一版那条前缀过滤挡的就是这件事。
///
/// ⚠ **这里刻意写 `ccm-oneshot-` 这个字面量而不是引 daemon 那个常量**：
/// 两棵树是两个 crate，monitor 不依赖 daemon 的 crate（`layering` 那条线）。
/// 同族的跨轨字面量本仓已有先例（`LOCAL_ORIGIN` 两侧对拍）；这一处由
/// `tests::the_oneshot_prefix_matches_the_daemon_side` 逐字对拍，**不许各写各的**。
const ONESHOT_SESSION_NAME_PREFIX: &str = "ccm-oneshot-";

/// F10：判定一个 tmux 会话名是否是**一次性探针会话**（不该出现在用户的会话列表里）。
/// 纯字符串前缀匹配，不涉及 IO。
pub(crate) fn is_usage_probe_session(name: &str) -> bool {
    name.starts_with(USAGE_PROBE_NAME_PREFIX) || name.starts_with(ONESHOT_SESSION_NAME_PREFIX)
}

/// 列远端 tmux 会话(通道 B,一次性 exec)。`command -v tmux` 门控:无 tmux → 哨兵 `NO_TMUX`
/// → 返 `None`(前端隐藏 attach 项);有 tmux 但无会话 → `Some(空)`。
///
/// F10：过滤掉 `is_usage_probe_session` 命中的一次性用量探针会话——见
/// `parse_visible_tmux_sessions`。
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
    Ok(Some(parse_visible_tmux_sessions(&out)))
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
    Some(parse_visible_tmux_sessions(&raw))
}

/// `tmux ls` 原始输出 → **前端可见**的会话列表：解析 + 滤掉一次性用量探针会话（F10）。
///
/// 探针会话对 `findClaudeTmux`/tab 徽章/kill 授权判据等全部下游消费者应当不可见——它们寿命
/// 以秒计、用完即清，混进正牌列表只会让 tab 右键菜单短暂冒出一个不属于任何 tab 的幽灵条目。
///
/// **为什么单独提一个函数**（F10 Phase D 审计）：原先「解析 + 过滤」内联在 `list_remote_tmux`
/// 里，而 `#[tauri::command]` 需要真实远端连接、单测碰不到；于是那条测试把过滤表达式在测试体里
/// **又抄了一遍**——删掉生产侧的 `.filter(...)` 它照样绿，是典型的伪测试。提成纯函数后，
/// 生产与测试走的是同一条代码路径，删过滤会立刻红。
pub(crate) fn parse_visible_tmux_sessions(raw: &str) -> Vec<TmuxSession> {
    parse_tmux_ls(raw)
        .into_iter()
        .filter(|s| !is_usage_probe_session(&s.name))
        .collect()
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
mod tests {
    /// ★ P8c（`U3` 08-11 裁定）：**三种 Skip 的原因必须彼此可分**。
    ///
    /// `U3` 的读数逐字记着不可分的后果：「`Unobservable` 计数 = 0，而**那个 0 是瞎的**
    /// —— 日志根本不记这一维 ⇒ 分母不存在。『0 次』与『记不下来』长得一模一样」。
    /// ⇒ 压成一个无载荷的 `Skip` 时，`#82` 想问的那个频率**问不出来**。
    #[test]
    fn the_three_skip_reasons_are_distinguishable() {
        use std::collections::HashSet;
        let reasons: HashSet<&str> = [
            // 远端没装 tmux —— 哨兵与显式字段两条路都该给同一个原因。
            classify_tmux_observation("NO_TMUX", None),
            classify_tmux_observation("", Some(OBS_NO_TMUX)),
            // daemon 自报观测失败 —— `#82` 要的就是这一格的频率。
            classify_tmux_observation("", Some(OBS_UNOBSERVABLE)),
            // 旧 daemon 的空串歧义。
            classify_tmux_observation("", None),
        ]
        .iter()
        .map(|v| match v {
            TmuxObservation::Skip(r) => r.as_str(),
            TmuxObservation::Backend(_) => panic!("这四种输入都该跳过"),
        })
        .collect();
        assert_eq!(
            reasons.len(),
            3,
            "三种原因必须彼此可分，实得 {reasons:?} —— 压成一个就等于这一维不可测"
        );
        // 标识必须是**机器可读**的稳定串（进日志后要能 grep/统计），不是给人读的句子。
        for r in &reasons {
            assert!(
                r.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "原因标识 {r:?} 不是机器可读的稳定串"
            );
        }
    }

    use super::*;

    // ---------- P1（zero-poll-liveness）：观测分类 ----------

    fn sids(o: &TmuxObservation) -> Vec<String> {
        match o {
            TmuxObservation::Backend(s) => {
                let mut v: Vec<String> = s.iter().cloned().collect();
                v.sort();
                v
            }
            TmuxObservation::Skip(r) => panic!("期望 Backend，实得 Skip({})", r.as_str()),
        }
    }

    /// ★ P1 的回归测试（**这条在修之前是红的**）：daemon 确证零会话 ⇒ 必须是**有效观测（空集）**，
    /// 不是跳过。这就是 `src/doc/INVARIANTS.md` §24bis 那条残留 bug 的机理：
    /// 杀掉某 origin 仅剩的 tmux 会话 → server 随之退出 → `tmux ls` 回空 →
    /// 旧代码保守跳过 → idle 灰灯卡到断连 flush 才清。
    #[test]
    fn zero_sessions_is_a_valid_observation_not_a_skip() {
        assert_eq!(
            classify_tmux_observation("", Some("zero_sessions")),
            TmuxObservation::Backend(std::collections::HashSet::new()),
            "daemon 确证零会话时必须进对账（空集），否则灰灯永不清"
        );
    }

    /// 旧 daemon（无 `observation` 字段）+ 空 raw ⇒ **保持今天的保守行为**。
    /// 空 raw 在旧 daemon 那里同时意味着「零会话」和「`tmux ls` 出错被 `|| true` 吞了」，
    /// 分不开 ⇒ 只能跳过。**新旧混搭不许回归。**
    #[test]
    fn old_daemon_empty_raw_still_skips() {
        assert_eq!(
            classify_tmux_observation("", None),
            // ★ P8c：连**原因**一起钉 —— 原来只钉「跳了」，而三种完全不同的原因
            // 压成同一个无载荷的 `Skip` 正是 `#82` 问不出频率的来源。
            TmuxObservation::Skip(SkipReason::LegacyAmbiguousEmpty),
            "旧 daemon 的空串语义不可分，必须保守跳过"
        );
    }

    /// 远端没装 tmux ⇒ 跳过（哨兵与显式分类**两条路都要认**）。
    #[test]
    fn no_tmux_skips_both_via_sentinel_and_field() {
        assert_eq!(
            classify_tmux_observation("NO_TMUX", None),
            TmuxObservation::Skip(SkipReason::NoTmux)
        );
        assert_eq!(
            classify_tmux_observation("NO_TMUX\n", Some("no_tmux")),
            TmuxObservation::Skip(SkipReason::NoTmux)
        );
    }

    /// 观测无效（`tmux ls` 非 0/1 退出、exec 失败）⇒ 跳过，**绝不当成零会话**。
    #[test]
    fn unobservable_skips() {
        assert_eq!(
            classify_tmux_observation("", Some("unobservable")),
            TmuxObservation::Skip(SkipReason::Unobservable),
            "观测失败当成零会话会批量误灰"
        );
    }

    /// 有会话时照常解析出 sid 集（`@ccm_sid` 为空的会话不进集合，见 `parse_tmux_ls`）。
    #[test]
    fn sessions_parse_into_sid_set() {
        let raw = "s1\t/p\tclaude\t1\t2\tsid-a\ns2\t/q\tnode\t0\t1\t\ns3\t/r\tbash\t0\t1\tsid-c";
        let o = classify_tmux_observation(raw, None);
        assert_eq!(sids(&o), vec!["sid-a".to_string(), "sid-c".to_string()]);
    }

    /// 向前兼容：未来 daemon 加了本 monitor 不认识的分类 ⇒ **落回 raw 判据**，
    /// 退化成今天的保守行为，不误灰。
    #[test]
    fn unknown_observation_falls_back_to_raw() {
        assert_eq!(
            classify_tmux_observation("", Some("some_future_kind")),
            TmuxObservation::Skip(SkipReason::LegacyAmbiguousEmpty)
        );
        let o = classify_tmux_observation("s1\t/p\tclaude\t1\t1\tsid-a", Some("some_future_kind"));
        assert_eq!(sids(&o), vec!["sid-a".to_string()]);
    }

    /// **P1 刻意保留的一处不对称**：`observation` 说有会话、但 raw 里一个 `@ccm_sid` 都没有
    /// （老会话 / 未装 wrapper）⇒ 仍然跳过。因为对账的判据是 sid 集，没有 sid 就无从判断，
    /// 而"有 tmux 会话但都没绑 sid"**不等于**"零会话"。
    #[test]
    fn sessions_without_any_ccm_sid_still_skips() {
        assert_eq!(
            classify_tmux_observation("s1\t/p\tbash\t0\t1\t", None),
            TmuxObservation::Skip(SkipReason::LegacyAmbiguousEmpty),
            "有会话但无 @ccm_sid ≠ 零会话，不许当空集喂进对账"
        );
    }

    /// P1：`observation` 取值集是 monitor↔daemon 的**第三个双写点**（前两个：`TMUX_LS_FMT` ·
    /// `NO_TMUX` 哨兵）。两个独立 crate 不能共享类型 ⇒ 用与
    /// `tmux_ls_fmt_double_write_point_stays_in_sync` 相同的办法钉住：`include_str!` 读 daemon 源
    /// + **锚定 const 定义行**（不是裸字面量——否则该串若出现在某条注释里会掩盖真漂移）。
    ///   **双向**：改 monitor 或 daemon 任一侧忘同步，本测即红。
    #[test]
    fn observation_tokens_double_write_point_stays_in_sync() {
        let daemon_src = include_str!(daemon_watcher_src!());
        for (name, value) in [
            ("OBS_ZERO_SESSIONS", OBS_ZERO_SESSIONS),
            ("OBS_NO_TMUX", OBS_NO_TMUX),
            ("OBS_UNOBSERVABLE", OBS_UNOBSERVABLE),
        ] {
            let expected_def = format!("const {name}: &str = \"{value}\";");
            assert!(
                daemon_src.contains(&expected_def),
                "observation 双写点漂移：daemon watcher.rs 不含 {expected_def:?}\n\
                 （改了分类取值就得两侧同步——同 TMUX_LS_FMT 的纪律）"
            );
        }
        // 反向自检：断言的是「扫到了 daemon 源」而不是「命中若干条」——阈值不能挂在
        // 被检查的量上（rust-ts-boundary 的教训）。
        assert!(
            daemon_src.len() > 1000,
            "include_str! 没读到 daemon 源，上面三条断言全是空转"
        );
    }

    /// A5：send-keys 目标白名单——只认本工具的 cc-* 会话名，拒用户别的 tmux。
    #[test]
    fn ccm_tmux_name_whitelist() {
        assert!(is_ccm_tmux_name("cc-abc12345"));
        // ★ S4b-3b：新命名 `<X>-cc`（撞名时 `<X>-cc-<N>`）也要本地命中，
        // 否则每次 kill/send-keys 都要多跑一趟远端去核 `@ccm_sid`。
        assert!(is_ccm_tmux_name("abc12345-cc"));
        assert!(is_ccm_tmux_name("abc12345-cc-2"));
        assert!(is_ccm_tmux_name("my-proj-cc"));
        // **老前缀必须继续命中** —— 用户机器上正跑着的会话就是这个形状，
        // 不认它们等于把它们变成 issue #76 那种「失管会话」。
        assert!(is_ccm_tmux_name("cc-proj"));
        // 退化名不该命中：`-cc` 前面得有东西。
        assert!(!is_ccm_tmux_name("-cc"));
        // 名字里恰好含 `-cc` 但不是以它结尾、也不是 `-cc-<数字>` ⇒ 不认
        //（那多半是别人的会话，误认会让我们跳过远端核验就去 kill）。
        assert!(!is_ccm_tmux_name("foo-ccx"));
        assert!(!is_ccm_tmux_name("foo-cc-bar"));
        assert!(is_ccm_tmux_name("cc-abc12345-2")); // pickFreshTmuxName 的 -N 变体
        assert!(!is_ccm_tmux_name("cc-")); // 只前缀无体
        assert!(!is_ccm_tmux_name("web")); // 用户自己的会话
        assert!(!is_ccm_tmux_name("mycc-x")); // 非前缀
        assert!(!is_ccm_tmux_name("cc-a b")); // 空格（注入面）
        assert!(!is_ccm_tmux_name("cc-a;rm")); // 分号
        assert!(!is_ccm_tmux_name("cc-a$x")); // 元字符
    }

    /// F04 Gate 1：**只有空 target** 恒被拒——`=:` 会解析成「当前会话」，是唯一真正危险的默认值。
    ///
    /// # ⚠ `K-R72`（09-12）：**人群没缩，只是换了住址**
    ///
    /// 送键与杀会话那两条桌面侧回落删掉之后 `build_kill_session_cmd` /  〔散文墓碑〕
    /// `build_send_keys_remote_cmd` 不在了 —— 但 Gate 1 **不是那两条回落的东西**：  〔散文墓碑〕
    /// 它守的是「任何拿 target 去做事的入口，都得先把空目标拒掉」。⇒ 本条改打
    /// **今天三条路各自真正的入口**，一条都没少：
    /// ① 谓词本体 [`gate1_reject_empty`]（`exact_target` 与两条后端命令共用的那一份）；
    /// ② `capture-pane` 构造器（经 [`exact_target`]，今天唯一还在拼 shell 串的那条）；
    /// ③④ 两条后端命令的**生产入口本体** —— 真调 [`tmux_send_keys`] / [`kill_remote_tmux`]，
    ///    断言它在**任何 IO 之前**就地拒。那一句同时是「本地校验先于一切往返」这条性质的读数：
    ///    它报的若是「后端通道不在」，就说明 Gate 1 跑到 IO 后面去了。
    ///
    /// 含 glob/元字符但非空的 target **不**在这一层被拒（`shell_quote` 已安全引号化，
    /// 字符集收紧是 TS 侧 `isValidNewTmuxName`/`isValidTmuxName` 的职责，
    /// 见 `is_safe_tmux_target` 头注）。
    #[test]
    fn gate1_rejects_only_empty_target() {
        // ① 谓词本体（正反各一格 —— 只钉「空的被拒」的话，把它焊死成恒拒也能绿）
        assert!(
            gate1_reject_empty("").is_err(),
            "空 target 应被 Gate 1 拒绝（谓词本体）"
        );
        assert!(
            gate1_reject_empty("cc-a b").is_ok(),
            "非空 target 不该被 Gate 1 拒绝（谓词本体）"
        );
        // ②③④ 三条后端命令：**真跑生产入口**〔`K-R112` 09-13：抓屏从「构造器那一格」
        //     挪进这一段 —— 它的构造器随那条 SSH 串一起删了，而生产入口比构造器强一格〕。
        // ⚠ 不必登记入方向通道 —— Gate 1 在 `daemon_*` 之前，根本走不到那一步。
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("建不出 runtime —— 本条无从判断，别读成绿");
        let sk = rt
            .block_on(tmux_send_keys(
                "no-such-origin".to_string(),
                String::new(),
                "/exit".to_string(),
                Some(true),
            ))
            .expect_err("空 target 的 send-keys 不该报成功");
        let kill = rt
            .block_on(kill_remote_tmux(
                "no-such-origin".to_string(),
                String::new(),
            ))
            .expect_err("空 target 的 kill 不该报成功");
        let cap = rt
            .block_on(capture_remote_pane(
                "no-such-origin".to_string(),
                String::new(),
            ))
            .expect_err("空 target 的 capture-pane 不该报成功");
        for (label, err) in [("send-keys", &sk), ("kill", &kill), ("capture-pane", &cap)] {
            assert!(
                err.contains("非法 tmux 目标（空）"),
                "{label} 的空 target 没被 Gate 1 就地拒。实得：{err}"
            );
            assert!(
                !err.contains("后端通道不在"),
                "{label} 走到后端那一步才失败 —— Gate 1 不再先于一切 IO 了。实得：{err}"
            );
        }
        // 非空、含元字符/glob 的 target 不被 Gate 1 拒（谓词本体那一格已在 ① 里断过；
        // 这里补一批真实形状 —— 收紧字符集是**另一层**的职责，不许在 Gate 1 顺手做）。
        for safe_nonempty in ["cc-a b", "cc-a;rm", "cc-a$x", "si*", "a'b"] {
            assert!(
                gate1_reject_empty(safe_nonempty).is_ok(),
                "非空 target {safe_nonempty:?} 不该被 Gate 1 拒绝"
            );
        }
    }

    /// ★★ **K-R12：跨 SSH 的 tmux 读要 `-u`，且必须在子命令之前。**
    ///
    /// # ⚠ `K-R72`（09-12）：人群**真的缩了一半**，名字跟着改
    ///
    /// 原名叫 `both_cross_ssh_tmux_reads_…`，那个「both」指的是
    /// `build_guarded_tmux_cmd` 的取值 `display-message` ＋ `list_remote_tmux` 的 `ls`。
    /// 前者随两条回落一起走了 ⇒ **monitor 侧今天只剩 `ls` 这一条跨 SSH 的 tmux 读**
    /// （`capture-pane` 实测不在人群里：它吐原始 UTF-8 字节，见 [`UTF8_CLIENT_FLAG`] 头注）。
    /// ⇒ 留「both」在名字里就是一句假话；性质本身**一个字没变**，只是分母从 2 变 1。
    ///
    /// 位置这一维必须单独钉：`-u` 放到子命令**后面**实测是
    /// `rc=1 + unknown flag -u`，而这条串把 stderr 与 rc 都丢了 ⇒ 静默退化。
    ///
    /// ⚠ **本条扫的是源码**（那条串拼在 `async fn` 里、外面取不到），如实标注：
    /// **盘上有 ≠ 被走到**。行为那一半的死值在 `tests/evidence/K-R12-deathvalue.md` ②/S5
    /// （真 tmux 3.4，改前段数 1 / 改后段数 6）。
    #[test]
    fn the_surviving_cross_ssh_tmux_read_asks_for_a_utf8_client_before_the_subcommand() {
        let prod = guard_core::production_code(include_str!("tmux.rs"));
        guard_core::assert_no_test_code("tmux.rs", &prod);
        // 抽取器自检：生产段塌了下面两条就零命中地绿。
        assert!(
            prod.len() > 5_000,
            "生产段只剩 {} 字节 —— 剥法坏了，本条此刻量不到东西",
            prod.len()
        );
        assert!(
            prod.contains("tmux {UTF8_CLIENT_FLAG} ls -F"),
            "`list_remote_tmux` 的命令串没有把 `-u` 放在 `ls` 之前"
        );
        assert!(
            !prod.contains("ls {UTF8_CLIENT_FLAG}") && !prod.contains("ls -u"),
            "`-u` 被放到了 `ls` 后面（实测 rc=1 + unknown flag）"
        );
        // ★ 反向自检：这把尺子分得清「放对了」与「放错了」——
        //   没有它，上面那两句在一份**根本没有 tmux 命令**的语料上也会「绿」。
        let bad = "tmux ls {UTF8_CLIENT_FLAG} -F '{TMUX_LS_FMT}'";
        assert!(
            !bad.contains("tmux {UTF8_CLIENT_FLAG} ls -F") && bad.contains("ls {UTF8_CLIENT_FLAG}"),
            "本条的两个针分不出「`-u` 在子命令前」与「在子命令后」—— 它此刻什么都没在守"
        );
    }

    /// daemon 侧那个「一个口径一个家」的家（相对**仓根**）—— 跨仓对拍的被读对象。
    ///
    /// 单一落点：路径写死在这里一处，daemon 再搬家只改这一行。
    const DAEMON_KOU_JING_HOME: &str = "src/backend/common/tmux_utf8.rs";

    /// ★★ **K-R12 下一拍（09-04）：「同一个口径只有一个家 + 另一侧引用它或有对拍」——
    /// 本 const 走的是**对拍**那一支。**
    ///
    /// # 这条钉的是**关系**，不是词表
    ///
    /// 三件事一起断言，缺一件就只买到一角：
    ///
    /// | # | 断的什么 | 缺了它会怎样 |
    /// |---|---|---|
    /// | ① | 本侧**只有一个**声明（本文件那一处，全 monitor 树无第二处） | 本侧自己先分了两份，对拍再准也没用 |
    /// | ② | 本侧那个值与 daemon 家里那一行**逐字相等**（值是从本侧 const **现取**的） | 两侧漂开而两边都不红 —— 正是本件治的那个形状 |
    /// | ③ | daemon 家里**两种表示都还在** | 有人把家「收口」成一种表示 ⇒ 另一类调用点静默失效 |
    ///
    /// ②③ 都是**读两棵树**才验得了的性质：两个 crate 不共享源码树，共用 `const` 拿不到
    /// （七个 `*-core` 的职责逐条都装不下，论据在 `UTF8_CLIENT_FLAG` 的头注里）。
    ///
    /// # ⚠ 作用域，逐条说清（`brief` 12：报一个数就要说清尺子）
    ///
    /// - **在哪跑**：monitor 那格 cargo（`cargo test --workspace --lib`）。
    ///   daemon 自己那格看不见它 —— 但门禁两格都跑，所以任一侧漂开都会在门禁里红。
    /// - **读了哪两棵树**：本侧 `include_str!("tmux.rs")`（编译期，同一半）+
    ///   daemon 侧 [`DAEMON_KOU_JING_HOME`]（**运行期** `read_to_string`）。
    /// - 🔴 **为什么 daemon 那一半刻意用运行期读、而不是 `include_str!`**：
    ///   `include_str!` 会新长出一条**跨半边的编译期边**，而那种边由
    ///   `cross_half_edge_registry::CROSS_EDGES` 逐条登记着（多一条就红），
    ///   **那个文件不在本拍写区**。运行期读在本仓是**既有做法**、不是绕道：
    ///   `cross_half_edge_registry` 自己就是运行期遍历 daemon 那棵树的
    ///   （`both_halves()` 扫 `src/backend`），`scanning_guard_registry::PENDING`
    ///   里也直接列着 daemon 的文件。而且它在该登记表关心的那一维上**更轻**：
    ///   daemon 换布局时这里是一句说得清的运行期失败，不是 `cargo test` 编不过。
    ///   ⚠ 代价如实写下：这条边因此**不出现在** `CROSS_EDGES` 里。
    ///   PM 若要它以编译期形态登记，改法是**两处一起动、不许只动一处**：
    ///   ① 把下面那句运行期读换成编译期读（`include_str!` 配 `concat!` / `env!` 拼路径，
    ///      形状照本文件已有的 `daemon_watcher_src` 那个单一落点宏）；
    ///   ② 同轮在 `CROSS_EDGES` 里加一条 `monitor→daemon` 的登记
    ///      （读者 `src/bridge/src/tmux.rs` · 被读 `src/backend/common/tmux_utf8.rs` ·
    ///      理由「跨轨对拍：口径的家在对面，本侧那一份必须与它逐字相等」）。
    ///   🔴 只动 ① 会让那张表的条数当场对不上 —— 它是**两个方向都查**的。
    /// - **不管什么**：它不证明「那个旗真的被走到了」（「盘上有 ≠ 被走到」）。
    ///   行为那一半的死值在 `tests/evidence/K-R12-deathvalue.md`（真 tmux 3.4 私有 socket）。
    #[test]
    fn utf8_client_kou_jing_has_one_home_and_this_side_matches_it() {
        let prod = guard_core::production_code(include_str!("tmux.rs"));
        guard_core::assert_no_test_code("tmux.rs", &prod);

        // ── ① 本侧只有一个声明 ────────────────────────────────────────────
        let decl_prefix = format!("{}: &str =", "UTF8_CLIENT_FLAG");
        guard_core::find_pinned(&prod, &decl_prefix)
            .unwrap_or_else(|e| panic!("本文件生产段里 `{decl_prefix}` 不是恰好一处：{e}"));
        let root = crate::guard_support::repo_root();
        let others = guard_core::scan_tree!(&root.join("src/bridge/src"), &["rs"]);
        assert!(
            others.len() >= 60,
            "monitor 树只采到 {} 个 .rs —— 遍历坏了，「无第二处」此刻是空转的",
            others.len()
        );
        let dup: Vec<String> = others
            .iter()
            .filter(|(_, raw)| guard_core::production_code(raw).contains(&decl_prefix))
            .map(|(p, _)| p.to_string_lossy().into_owned())
            .collect();
        assert!(
            dup.is_empty(),
            "monitor 侧有**第二个** `{decl_prefix}`：{dup:?}\n\
             ⇒ 从此两份靠人对齐。正解是引用本文件那一处。"
        );

        // ── ② 与 daemon 那个家逐字相等（值现取，不写死） ──────────────────
        let home_path = root.join(DAEMON_KOU_JING_HOME);
        let home = std::fs::read_to_string(&home_path).unwrap_or_else(|e| {
            panic!(
                "读不到 daemon 侧那个家 {home_path:?}：{e}\n\
                 它是 K-R12 下一拍建的「一个口径一个家」。文件被搬了 ⇒ 改本文件那个常量；\
                 家被删了 ⇒ 那个口径退回三份靠人对齐，先回件文件。"
            )
        });
        assert!(
            home.len() > 2_000,
            "daemon 那个家只有 {} 字节 —— 没读到内容，下面的对拍是空转的",
            home.len()
        );
        let want_flag = format!("{}: &str = {UTF8_CLIENT_FLAG:?};", "UTF8_CLIENT_FLAG");
        assert!(
            home.contains(&want_flag),
            "跨仓漂移：本侧的旗是 {UTF8_CLIENT_FLAG:?}，而 daemon 那个家里找不到 `{want_flag}`。\n\
             两侧漂开时**两边都不会因为别的判据变红** —— 那正是本件立件时的那个形状。"
        );
        // 段数下溢那个谓词是同一族的第二个口径：比的是**函数体**，不是名字。
        let body = format!("{}.count() < expected", ".split('\\t')");
        guard_core::find_pinned(&prod, &body)
            .unwrap_or_else(|e| panic!("本侧那个下溢谓词的体不是恰好一处：{e}"));
        assert!(
            home.contains(&body),
            "跨仓漂移：下溢谓词的体两侧不一致（本侧是 `{body}`，daemon 家里找不到）。\n\
             口径一致本身就是要买的东西：一侧改成 `!=` 就会开始误伤合法内容。"
        );

        // ── ③ daemon 家里两种表示都还在 ───────────────────────────────────
        // ⚠ 锚点只钉「那里有一个声明」（`const <名>:`），**不钉类型写法** ——
        //   带类型标注的锚点实测会被 `(&'static str, &'static str)` 这种合法写法误伤，
        //   而它印出来的话是「家里少了 env 形」：一句指向完全错误方向的诊断。
        //   ⚠ 同时**刻意收在 `:` 上**：收在标识符上时 `const <名>X:` 会被裸 `contains`
        //   当成命中（「匹配单位比事实小」那一族），于是「家改名了」这一形看不见。
        // 表里存**标识符**，锚点现拼 —— 反向自检那份「改了名」的夹具必须从标识符派生，
        // 从锚点文本派生的夹具会跟着锚点一起变松，于是「锚点变松了」这件事自己看不见
        // （daemon 那侧的同职判据实测栽过这一形，头注里逐字记着）。
        let anchor = |ident: &str| format!("const {ident}:");
        for (label, ident) in [
            ("argv 形（旗）", "UTF8_CLIENT_FLAG"),
            ("env 形", "UTF8_CLIENT_ENV"),
        ] {
            let needle = anchor(ident);
            assert!(
                home.contains(&needle),
                "daemon 那个家里少了**{label}**（找不到 `{needle}`）—— \
                 「一个口径两种表示」被收口成一种了，而两类调用点各需要一种：\
                 少了哪一种，那一类调用点就静默退回非 UTF-8 客户端。"
            );
            // 反向自检（③ 那一半的牙）：一个**改了名**的声明不许算命中。
            // 没有这一格，③ 就是「那个大文件里恰好有这个串」式的恒真。
            let renamed = anchor(&format!("{ident}X"));
            assert!(
                !format!("pub {renamed} (&str, &str) = (\"x\", \"y\");\n").contains(&needle),
                "③ 的锚点匹配单位比事实小：`{renamed}` 这样一个改了名的声明也算成 `{needle}` 在"
            );
        }
        // 本侧**不许**长出 env 形：跨 SSH 这一侧没有本地 `Command` 可挂 env，
        // 走 `request_env` 要赌对端 `AcceptEnv`（不认就静默拒绝）⇒ 拿一条静默失效治另一条。
        //
        // ⚠ **必须先剥注释再扫**：本文件的头注里逐字讨论过 `LC_ALL` 那条路为什么不走
        //   （那是**警告**，不是用法），不剥就当场误报 —— 本仓「判据数到注释」已栽过三次。
        let code = guard_core::strip_comment_lines(&prod);
        assert!(
            code.len() > 5_000,
            "剥注释之后只剩 {} 字节 —— 剥过头了，下面那条在空转",
            code.len()
        );
        assert!(
            !code.contains("LC_ALL"),
            "monitor 这一侧长出了 env 形 —— 跨 SSH 那两处该用旗，理由在 `UTF8_CLIENT_FLAG` 头注"
        );

        // ── 反向自检：上面那两条 `contains` 真的分得清 ────────────────────
        // 没有这一格，② 就可能是「随便什么串都在那个大文件里」式的恒真。
        let drifted = format!("{}: &str = \"{UTF8_CLIENT_FLAG}x\";", "UTF8_CLIENT_FLAG");
        assert!(
            !home.contains(&drifted),
            "喂一个**漂了的**值居然也在 daemon 那个家里命中（`{drifted}`）—— \
             ② 那条对拍此刻恒真，它什么都没在守"
        );
        assert!(
            !home.contains(&format!("{}.count() != expected", ".split('\\t')")),
            "daemon 家里同时存在 `!=` 那一版下溢谓词 —— 口径不一致，且 `!=` 会误伤合法内容"
        );
    }

    /// ★★ **K-R12 `J1` 死值验（monitor 这一侧）：段数下溢必须被判废。**
    ///
    /// 死值取自 `tests/evidence/K-R12-deathvalue.md` ①/S5：真 tmux 3.4 + POSIX 客户端，
    /// 六列塌成 1 段，连 `文档` 都按显示宽度变成了 `____`。
    ///
    /// 这一条同时把 `§5.4` 点名的那条**误伤**钉成一个可见的读数：**过溢的行今天照样被丢掉**。
    /// 处置本拍**刻意没改**（理由见 [`parse_tmux_ls`] 头注），所以这里断言的是**现状**——
    /// 哪天有人去修那条误伤，本条会红，那正是它该红的时候。
    #[test]
    fn a_dirty_line_underflows_and_an_overflowing_line_is_still_dropped_today() {
        const DIRTY: &str = "kr12_/tmp/kr12dv/____/proj_bash_0_1_cc-deadval1";
        const CLEAN: &str = "kr12\t/tmp/kr12dv/文档/proj\tbash\t0\t1\tcc-deadval1";
        const OVERFLOW: &str = "kr12\t/tmp/a\tb\tbash\t0\t1\tcc-deadval1";

        assert!(
            tmux_tab_underflow(DIRTY, TMUX_LS_FMT_FIELDS),
            "真脏字节必须判下溢"
        );
        assert!(
            !tmux_tab_underflow(CLEAN, TMUX_LS_FMT_FIELDS),
            "干净六段不许红"
        );
        assert!(
            !tmux_tab_underflow(OVERFLOW, TMUX_LS_FMT_FIELDS),
            "过溢是合法内容 ⇒ 判据不许红（这一格就是「下溢」而不是「不等于 6」的死值）"
        );

        assert!(parse_tmux_ls(DIRTY).is_empty(), "脏行不许进结果");
        let ok = parse_tmux_ls(CLEAN);
        assert_eq!(ok.len(), 1, "干净行必须解析出来（正对照）");
        assert_eq!(ok[0].name, "kr12");
        assert_eq!(ok[0].path, "/tmp/kr12dv/文档/proj");
        assert_eq!(ok[0].sid.as_deref(), Some("cc-deadval1"));
        assert!(
            parse_tmux_ls(OVERFLOW).is_empty(),
            "⚠ 现状：过溢的行**今天照样被整行丢掉**（`f.len() != 6`）—— \
             这是 K-R12 §5.4 点名的误伤，本拍只让它出声、没有改处置。\
             修它的那一拍会让本条红，那是对的。"
        );
    }

    /// ★ **每一处 `-t {…}` 的目标都必须出自 `exact_target`**〔audit-0805 08-07〕。
    ///
    /// 裸 `-t <名>` 是「精确 → 名字开头 → glob」三级解析。实测（tmux 3.6）只有 `sib-2`
    /// 存在时 `kill-session -t sib` 杀掉 `sib-2` 且 **rc=0**、`send-keys -t sib` 投进
    /// `sib-2`、`kill-session -t 'si*'` glob 命中。本仓必然踩
    /// （`pickFreshTmuxName` 造 `<sid8>-cc-2/-3`、终端 `cct` 造 `<dir>_cc-2/-3`）。
    ///
    /// # ⚠ `K-R72`（09-12）：人群从 4 处缩到 1 处，牙跟着换住址
    ///
    /// 那 4 处里有 3 处（`display-message` / `kill-session` / `send-keys`）随两条回落走了 ——
    /// 今天 monitor 侧**只有 `capture-pane` 还在把目标插进一条 tmux 命令串**。
    /// 顺带走的还有原来那半「委托给 `build_guarded_tmux_cmd` 也算」的闭环逻辑：
    /// **没有受托者了，判准就回到最简的那一条** —— 谁插目标谁调 `exact_target(`。
    ///
    /// 🔴 **分母掉到 1 之后，「地板 ≥ N」这种抽取器自检就买不到东西了**（1 处也过、
    /// 0 处才红，而 0 处那天本条本来就该重写）。⇒ 换成**喂一份合成的坏语料**：
    /// 同一把尺子必须在坏语料上红。这样「人群只剩一个」不等于「判据变空转」。
    #[test]
    fn every_target_placeholder_comes_from_exact_target() {
        // 把「找出每一处 `-t {…}` 所在的函数」抽成纯函数 —— 于是同一把尺子既量真生产段，
        // 也量下面那份合成的坏语料。**尺子只有一份**是本条的要点。
        fn sites(src: &str) -> Vec<(String, String)> {
            let lines: Vec<&str> = src.lines().collect();
            let mut out: Vec<(String, String)> = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                if !line.contains("-t {") {
                    continue;
                }
                // 往回找最近的 `fn 名字`，再取它的体（到下一个顶格行；
                // `where` / `)` 顶格的是头的一部分 —— 这一族本仓已栽过三次）。
                let mut s = i;
                while s > 0 && !lines[s].contains("fn ") {
                    s -= 1;
                }
                let name: String = lines[s]
                    .split(" fn ")
                    .nth(1)
                    .or_else(|| lines[s].strip_prefix("fn "))
                    .unwrap_or("")
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                let mut body = Vec::new();
                for (k, l) in lines[s..].iter().enumerate() {
                    let cont = l.starts_with("where") || l.starts_with(')') || l.trim() == "{";
                    if k > 0 && !l.is_empty() && !l.starts_with(char::is_whitespace) && !cont {
                        break;
                    }
                    body.push(*l);
                }
                out.push((name, body.join("\n")));
            }
            out
        }
        fn bad_of(src: &str) -> Vec<String> {
            sites(src)
                .into_iter()
                .filter(|(_, body)| !body.contains("exact_target("))
                .map(|(name, _)| name)
                .collect()
        }

        let prod = guard_core::production_code(include_str!("tmux.rs"));
        let found = sites(&prod);
        // 🔴 **`K-R112`（09-13）：人群从 1 掉到 0 —— 这是这条棘轮的终态，不是它坏了。**
        //   `build_capture_pane_cmd`〔散文墓碑〕是最后一个把目标插进 tmux 命令串的地方，抓屏改走
        //   `capture-pane` 帧之后它整块删了 ⇒ monitor 侧**再没有一处**在拼 tmux 目标。
        //   ⚠ 分母 0 的判据是**空真** —— 所以这一条的牙从此**全部**压在下面那份合成语料上：
        //   同一把尺子在坏语料上必须红。两样一起断，缺一条本条就成了摆设。
        assert!(
            found.is_empty(),
            "生产段又出现了把目标插进 tmux 命令串的地方：{:?}\n\
             —— 那条路 `K-R72`/`K-R112` 已经收干净了（三条命令全走后端帧面）。\n\
             真要新增一处，`exact_target` 今天只住 daemon 侧\n\
             （`src/backend/control/launch.rs`），别在这里重新长一份。",
            found.iter().map(|(n, _)| n).collect::<Vec<_>>()
        );
        // ★ 反向自检：同一把尺子在**合成的坏语料**上必须红。
        //   ⚠ 语料里刻意不出现 `exact_target`，断言也不取自夹具的名字（`6g` 那一族）。
        const SYNTHETIC_BAD: &str =
            "fn zzz_probe(x: &str) -> String {\n    format!(\"tmux kill-session -t {x} 2>&1\")\n}\n";
        assert_eq!(
            bad_of(SYNTHETIC_BAD),
            vec!["zzz_probe".to_string()],
            "本条的尺子在一份**明摆着裸目标**的语料上都不红 —— 它此刻什么都没在守"
        );
        // ★ 正向自检：同一把尺子在一份**调了 `exact_target` 的**语料上必须不红。
        //   只有反向那一格的话，一把「恒红」的坏尺子也能过 —— 那不是尺子，是常量。
        const SYNTHETIC_OK: &str =
            "fn zzz_ok(x: &str) -> String {\n    let t = exact_target(x);\n    format!(\"tmux kill-session -t {t} 2>&1\")\n}\n";
        assert!(
            bad_of(SYNTHETIC_OK).is_empty(),
            "本条的尺子把一份**调了 `exact_target` 的**语料也判红了 —— 它恒红，不是在守"
        );
    }

    // ════════════════════════════════════════════════════════════════════════
    // `K-R112`（09-13）：抓屏改走 `capture-pane` 帧。下面两条是 `KR112D2` 的机检。
    // ════════════════════════════════════════════════════════════════════════

    /// 「这段代码在**拼 / 跑一条 tmux shell 串**吗」—— 认形态，不认某一个符号。
    ///
    /// 🔴 失效方向（`KR112D2` 逐字点名的那个）：判「源码里还有没有 `Command::new`」是**判写法**，
    /// 它挡不住换个写法再拼一遍。⇒ 本谓词认的是「命令串」这件事的几种形态，
    /// 而它对每一种真的会响这件事，由 [`the_tmux_shell_line_detector_really_sees_each_shape`]
    /// 用活体语料证明。
    fn tmux_shell_line_markers(body: &str) -> Vec<&'static str> {
        // 逐条：送进远端 shell · 经命令构造器 · 直接写 tmux 命令 · 那两个老哨兵 ·
        // 为了拼进 shell 才需要的引用 · 问「这台远端怎么连」。
        [
            "connect_and_exec_cmd(",
            "_cmd(",
            "tmux capture-pane",
            "command -v tmux",
            "NO_PANE",
            "shell_quote(",
            "load_remote_config_by_label(",
        ]
        .into_iter()
        .filter(|m| body.contains(m))
        .collect()
    }

    /// ★ 上面那个谓词的**活体夹具**：它对每一种形态都得真的响。
    #[test]
    fn the_tmux_shell_line_detector_really_sees_each_shape() {
        // 形态一律**现拼**，免得夹具自己被真树上的扫描收进人群。
        let old = format!(
            "let cmd = build{u}capture{u}pane{u}cmd(&target)?;\n\
             let cfg = crate::load{u}remote{u}config{u}by{u}label(&origin)?;\n\
             let stream = ssh{u}source::connect{u}and{u}exec{u}cmd(&cfg, &cmd).await?;",
            u = "_"
        );
        assert!(
            tmux_shell_line_markers(&old).len() >= 3,
            "老那条路（构造器 + 远端配置 + 一次性 exec）没被认出来：{:?}",
            tmux_shell_line_markers(&old)
        );
        let inlined = format!(
            "let c = format!(\"if command {v} tmux; then tmux capture{d}pane -p -t {{t}}; fi\");",
            v = "-v",
            d = "-"
        );
        assert!(
            !tmux_shell_line_markers(&inlined).is_empty(),
            "**换个写法内联拼一份**没被认出来 —— 那正是「判写法」买不到的那一格"
        );
        // 反向：一段真的只调帧面原语的代码不许被误判。
        let clean = "let reply = client.call(CAPTURE_PANE, capture_pane_args(target), d).await";
        assert!(
            tmux_shell_line_markers(clean).is_empty(),
            "只调原语的代码被误判成拼串：{:?}",
            tmux_shell_line_markers(clean)
        );
    }

    /// ★★ `KR112D2` 刀①：**抓一屏走的是 `capture-pane` 帧，不是一次性 SSH。**
    ///
    /// 判的是**这条路**（`capture_remote_pane` → `capture_via_daemon`）上有没有命令串。
    #[test]
    fn the_capture_path_asks_the_backend_instead_of_composing_a_shell_line() {
        let prod = guard_core::production_code(include_str!("tmux.rs"));
        let body_of = |sig: &str| -> String {
            let at = guard_core::find_pinned(&prod, sig)
                .unwrap_or_else(|e| panic!("生产段找不到 {sig}（{e}）—— 判据在空转"));
            prod[at..]
                .lines()
                .skip(1)
                .take_while(|l| *l != "\u{7d}")
                .collect::<Vec<_>>()
                .join("\n")
        };
        let mut checked = 0usize;
        for sig in [
            "pub async fn capture_remote_pane(",
            "async fn capture_via_daemon(",
        ] {
            let body = body_of(sig);
            assert!(
                body.chars().count() > 60,
                "`{sig}` 的体只切出 {} 字 —— 抽取器坏了，本条在空转",
                body.chars().count()
            );
            let hits = tmux_shell_line_markers(&body);
            assert!(
                hits.is_empty(),
                "`{sig}` 这条路上又出现了命令串的痕迹 {hits:?}。\n\
                 抓一屏归 daemon 的 `capture-pane` 原语（`K-R86` 出、`K-R104` 上帧面）——\n\
                 拼一条 shell 串走 SSH 就是同一件事的第二份实现（`K33`「所有命令只许有一处」）。"
            );
            checked += 1;
        }
        assert_eq!(checked, 2, "只核到 {checked} 段 —— 本断言在空转");
        // 它真的调了那条原语，而且能力协商排在抓之前。
        let via = body_of("async fn capture_via_daemon(");
        let ask = via
            .find("accepts(CAPTURE_PANE)")
            .expect("`capture_via_daemon` 没有先问一句能力 —— 「这台后端太旧」永远说不出口");
        let call = via
            .find("CAPTURE_PANE,")
            .expect("`capture_via_daemon` 没在调那条原语 —— 判据的参照物没了");
        assert!(ask < call, "能力协商排在真抓之后 —— 那就永远走不到");
        assert!(
            via.contains("capture_pane_args(target)"),
            "参数不是走那份共用的构造器 —— 字段名一漂，症状是「命令发出去了、对面说缺字段」"
        );
        // 分流走那唯一的一份（`daemon_route` 的登记表逐字要求每个发送端表态）。
        assert!(
            via.contains("route_call_error"),
            "抓屏这个发送端自己在判「要不要回落」—— 那是分流规则的第二份实现"
        );
    }

    /// ★★ `KR112D2` 刀②：**本机那一支从「回一句还看不了」变成真去抓。**
    ///
    /// 老行为逐字是：`<local>` 直接早退，回一句「本机还看不了 …的画面预览」。
    /// 那句话当时诚实（daemon 没有抓屏原语），今天**前提到期**（`K-R86`/`K-R104`）。
    ///
    /// ⚠ **射程写清楚**：本条不证明「本机真抓得到一屏」—— 那要后端在、且有一个真 tmux 会话，
    /// 而沙箱里 `<local>` 上没有入方向通道。本条证的是**两件可判的事**：
    /// ① 那条早退（连同那句话）在盘上没了；② 本机与远端**走的是同一段代码**，
    /// 通道不在时两边只差一个称呼 —— 那正是「本机不再是死胡同」的可判形式。
    #[test]
    fn the_local_capture_is_no_longer_a_dead_end() {
        let prod = guard_core::production_code(include_str!("tmux.rs"));
        let at = guard_core::find_pinned(&prod, "pub async fn capture_remote_pane(")
            .expect("抓屏入口不在了");
        let body: String = prod[at..]
            .lines()
            .skip(1)
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            body.contains("capture_via_daemon("),
            "抽到的 `capture_remote_pane` 体里连主路都没有 —— 抽取器坏了，本条空转。实得 {} 字节",
            body.len()
        );
        // ① 那条本机早退没了：`origin` 是入参，不是分支。
        for forbidden in ["LOCAL_ORIGIN", "origin =="] {
            assert!(
                !body.contains(forbidden),
                "`capture_remote_pane` 里又出现了 `{forbidden}` —— 本机那条早退回潮了。\n\
                 它回的那句「本机还看不了」在 `K-R86`/`K-R104` 之后是**假话**：\n\
                 daemon 有 `capture-pane` 了，`<local>` 也是一个 origin。"
            );
        }
        // ② 两侧同一段代码：通道不在时只差一个称呼。
        let _guard = crate::inbound_client::local_origin_test_lock();
        assert!(
            crate::inbound_client::client_for(crate::inbound_client::LOCAL_ORIGIN).is_none(),
            "测试进程里 `<local>` 上居然有入方向通道 —— 本条的前提不成立，下面几句会空转"
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("建不出 runtime —— 本条无从判断，别读成绿");
        let local = rt
            .block_on(capture_remote_pane(
                crate::inbound_client::LOCAL_ORIGIN.to_string(),
                "cc-abc12345".to_string(),
            ))
            .expect_err("本机后端通道不在，这一趟不该报成功");
        let remote = rt
            .block_on(capture_remote_pane(
                "kr112-remote-label".to_string(),
                "cc-abc12345".to_string(),
            ))
            .expect_err("那台远端没配过，这一趟不该报成功");
        // 🔴 老那句话必须不在了 —— 它是「本机做不到」的字面形式。
        assert!(
            !local.contains("还看不了"),
            "本机仍在回那句「还看不了」—— 前提到期了，那句话今天是假的：{local}"
        );
        // 也不许退回那句与真实原因毫无关系的「未找到远端配置」（`P4d-Y5` 收口的那一族）。
        for e in [&local, &remote] {
            assert!(
                !e.contains("未找到远端配置"),
                "抓屏又报「未找到远端配置」—— 那是 `P4d-Y5` 收口的那一族假话：{e}"
            );
        }
        assert_ne!(
            local, remote,
            "本机与远端的「通道不在」共用了同一句话 —— 下一步不同却说同一句，\n\
             就等于把两件事压成一个读数"
        );
        assert!(
            remote.contains("kr112-remote-label") && !remote.contains("本机"),
            "远端那句话没点出是哪台机器、或者错用了本机那半。实得：{remote}"
        );
        // 空目标那道 Gate 1 仍在本地就地判（不该先花一次往返）。
        let empty = rt
            .block_on(capture_remote_pane(
                "kr112-remote-label".to_string(),
                String::new(),
            ))
            .expect_err("空目标必须被 Gate 1 拒");
        assert!(
            empty.contains("非法 tmux 目标"),
            "空目标不是被 Gate 1 拒的（`=:` 会被 tmux 解析成「当前会话」）：{empty}"
        );
    }

    /// ★ **P3 刀 2：本机 kill 不许回落到 SSH**〔08-11〕。
    ///
    /// # ⚠ `K-R72`（09-12）：性质**变强了**，判法跟着换 —— 不是这一条死了
    ///
    /// 它原来钉的是「那条 SSH 回落**之前**有本机的早退」（比的是两个位置的先后）。
    /// 今天那条 SSH 回落整个没了 ⇒ **「本机不许回落到 SSH」从一条纪律变成一条结构事实**。
    /// 位置判据在一个不存在的东西上无从谈起，但它买的那两件事一件都不许丢：
    /// ① **盘上没有第二条路** —— 生产段里再出现 `connect_and_exec_cmd` 就红（**回潮闸**）；
    /// ② **说的是真实原因** —— 对 `<local>` 报的不许是「未找到远端配置」那句与真实原因
    ///    毫无关系的话。**错的诊断比没有诊断更贵。**
    ///
    /// ⚠ 射程：本条**不证明**本机 kill 真的杀得掉（那要后端在、且有一个真 tmux 会话）。
    /// 后者今天没有 UI 入口（见 `K-R56#§0j`），所以也没有实测。
    #[test]
    fn the_local_kill_never_falls_back_to_ssh() {
        let prod = guard_core::production_code(include_str!("tmux.rs"));
        let at = guard_core::find_pinned(&prod, "pub async fn kill_remote_tmux(")
            .expect("kill 入口不在了");
        let body: String = prod[at..]
            .lines()
            .skip(1)
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n");
        // 抽取器自检：抽空了下面那条就恒绿。
        assert!(
            body.contains("daemon_kill::daemon_kill("),
            "抽到的 `kill_remote_tmux` 函数体里连主路都没有 —— 抽取器坏了，本条此刻空转。\n\
             实得 {} 字节",
            body.len()
        );
        // ① 回潮闸：这条命令里**不许再有** SSH 那条路。
        assert!(
            !body.contains("connect_and_exec_cmd"),
            "`kill_remote_tmux` 里又出现了 `connect_and_exec_cmd` —— 那条一次性 SSH 回落回潮了。\n\
             `K-R54` 表第 2 处判它删：daemon 那条先 `admit_destructive` 拿 `#{{session_id}}`\n\
             **句柄**再杀，而 SSH 那条杀的是 `=name:`（**名字**）—— 破坏性动作对名字下手\n\
             就把 TOCTOU 窗口留着。要恢复它先回 `K-R54` 重新裁定。"
        );
        // ② 真实原因：本机那句话不许说成「未找到远端配置」。
        let _guard = crate::inbound_client::local_origin_test_lock();
        assert!(
            crate::inbound_client::client_for(crate::inbound_client::LOCAL_ORIGIN).is_none(),
            "测试进程里 `<local>` 上居然有入方向通道 —— 本条的前提不成立，下面那句会空转"
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("建不出 runtime —— 本条无从判断，别读成绿");
        let err = rt
            .block_on(kill_remote_tmux(
                crate::inbound_client::LOCAL_ORIGIN.to_string(),
                "cc-abc12345".to_string(),
            ))
            .expect_err("本机后端通道不在，这一趟不该报成功");
        assert!(
            !err.contains("未找到远端配置"),
            "本机 kill 报的是「未找到远端配置」—— 那是 SSH 回落那条路的话，\n\
             而真实原因是本机后端通道不在。实得：{err}"
        );
        assert!(
            err.contains("本机后端通道不在"),
            "本机那条早退在，但它没说出真实原因。实得：{err}"
        );
    }

    /// ★★ **`send-keys` 这条路上，本机也不许悄悄回落到一次性 SSH**（`K-R56`，09-11 买到）。
    ///
    /// # ⚠ `K-R72`（09-12）：性质**变强了**，判法跟着换 —— 不是这一条死了
    ///
    /// `K-R56` 立它时，`tmux_send_keys` 有一条 SSH 回落，而 `<local>` 会掉进
    /// `load_remote_config_by_label("<local>")`，报 **「未找到远端配置: `"<local>"`」** ——
    /// 一句与真实原因（本机后端通道不在）毫无关系的话。**错的诊断比没有诊断更贵。**
    /// 今天那条回落整个删了 ⇒ 「本机不许回落到 SSH」从一条纪律变成一条**结构事实**。
    /// 本条因此加一格、并把原来那格保住：
    /// ① **回潮闸**（新）：生产段里再出现 `connect_and_exec_cmd` 就红；
    /// ② **说的是真实原因**（原有那格，一个字没改判法）：真调生产入口 [`tmux_send_keys`]，
    ///    看它到底报了哪句话；
    /// ③ **本机与远端的话不许一样**（新）：两条路的下一步不同 —— 一个是「先让本机后端跑起来」，
    ///    另一个是「先让那台机器上的后端连上」。压成一句就等于把两个处置合并成一个读数。
    ///
    /// # ⚠ 射程，逐条说清（`brief` 12：报一个性质就要说清尺子）
    ///
    /// - 钉的是「它报的是真实原因、而且盘上没有第二条路」。
    ///   **不证明**本机 send-keys 真的送得到 —— 那要后端在 + 一个真 tmux 会话，
    ///   而真 tmux 本区口径禁（`K-R56#§0d`）。
    /// - 🔴 **②③ 比的是 `origin`，不是 `target` 会话名**：判据逐字是
    ///   `origin == LOCAL_ORIGIN`（**逐字节相等**）。⇒
    ///   · **拦得住**：唯一那个前端/后端约定的哨兵串（`inbound_client::LOCAL_ORIGIN`，
    ///     由 `inbound_client.rs::the_local_origin_is_the_same_string_on_both_sides`
    ///     钉着它与前端 `daemon-policy.ts` 那份逐字相同）。
    ///   · **拦不住**：一台 label 起成 `localhost` / `127.0.0.1` / 本机主机名的**远端**
    ///     （即便它就是这台机器）—— 走的是远端那句话。⚠ 那**是对的**：它确实是一条远端传输。
    ///   · **也拦不住**：大小写 / 前后空白不同的写法（`<LOCAL>`、`" <local>"`）——
    ///     但那些今天进不来，`LOCAL_ORIGIN` 是常量、不是用户输入。
    ///     真正的撞名口子是 `LOCAL_ORIGIN` 自己头注逐字承认的那条：
    ///     「用户理论上可以把某台远端机器的 label 起成这个名字……**不做防御**」。
    ///   ⚠ **09-11 自查回打（`K-R56`）**：这一段第一版点的是一个**编出来的**判据名（盘上零处），
    ///     被 `structural_scan.rs` 那条「散文点名的名字必须在代码里」的机检当场逮住。
    ///     🔴 那个假名字与两趟判定行逐字抄在 `tests/evidence/K-R56-deathvalue.md`，
    ///     刻意不抄在这里：抄回来就又是一处「散文点名一个不存在的名字」。
    #[test]
    fn the_local_send_keys_never_falls_back_to_ssh() {
        // ① 回潮闸：这条命令里**不许再有** SSH 那条路。
        let prod = guard_core::production_code(include_str!("tmux.rs"));
        let at = guard_core::find_pinned(&prod, "pub async fn tmux_send_keys(")
            .expect("send-keys 入口不在了");
        let body: String = prod[at..]
            .lines()
            .skip(1)
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            body.contains("daemon_send_keys::daemon_send_keys("),
            "抽到的 `tmux_send_keys` 函数体里连主路都没有 —— 抽取器坏了，本条此刻空转。\n\
             实得 {} 字节",
            body.len()
        );
        assert!(
            !body.contains("connect_and_exec_cmd"),
            "`tmux_send_keys` 里又出现了 `connect_and_exec_cmd` —— 那条一次性 SSH 回落回潮了。\n\
             `K-R54` 表第 1 处判它删（`K-R56` 先把两条路的「探了没有」补齐才删得掉）。\n\
             要恢复它先回 `K-R54` 重新裁定。"
        );

        // ②③ 行为：真调生产入口，看它报了哪句话。
        // 登记表是**进程内全局**的 ⇒ 与别的会在 `<local>` 键上登记通道的用例串起来跑。
        let _guard = crate::inbound_client::local_origin_test_lock();
        // 前提自检：本条靠「`<local>` 上没有通道」才走得到 `NoChannel` 那一臂。
        assert!(
            crate::inbound_client::client_for(crate::inbound_client::LOCAL_ORIGIN).is_none(),
            "测试进程里 `<local>` 上居然有入方向通道 —— 本条的前提不成立，下面那句会空转"
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("建不出 runtime —— 本条无从判断，别读成绿");
        let send = |origin: &str| {
            rt.block_on(tmux_send_keys(
                origin.to_string(),
                "cc-abc12345".to_string(),
                "/compact".to_string(),
                Some(true),
            ))
            .expect_err("后端通道不在，这一趟不该报成功")
        };
        let local = send(crate::inbound_client::LOCAL_ORIGIN);
        assert!(
            !local.contains("未找到远端配置"),
            "本机 send-keys 报的是「未找到远端配置」—— 那是 SSH 回落那条路的话，\n\
             而真实原因是本机后端通道不在。**错的诊断比没有诊断更贵。**\n\
             实得：{local}"
        );
        assert!(
            local.contains("本机后端通道不在"),
            "本机那条早退在，但它没说出真实原因。实得：{local}"
        );
        // ③ 远端那条：话必须不一样，而且也说得出下一步。
        let remote = send("some-remote-label");
        assert_ne!(
            local, remote,
            "本机与远端的「通道不在」共用了同一句话 —— 处置相同没问题，\n\
             **下一步不同却说同一句**就等于把两件事压成一个读数（本机是「本机后端没起来」，\n\
             远端是「那台机器上的后端没连上」）。"
        );
        assert!(
            remote.contains("some-remote-label") && !remote.contains("本机"),
            "远端那句话没点出是哪台机器、或者错用了本机那半。实得：{remote}"
        );
    }

    /// F01 回归：tmux `-t` 目标**必须**精确匹配（`'=<名>:'`），绝不留裸目标。
    ///
    /// 删掉这条性质会让换号重启把 `/exit` 敲进**兄弟会话里还活着的 claude** 并 kill 它，
    /// 而 UI 报告「已重启」。**尾冒号不能省**：`send-keys`/`capture-pane` 收 target-pane，
    /// `=名`（无冒号）在那条路径上 rc=1 完全失效。
    ///
    /// # 🔴 `K-R112`（09-13）：**牙换住址 —— 两侧各一份变一份，而这一条跟过去数**
    ///
    /// `K-R72` 把产物侧人群从三个构造器缩到 `capture-pane` 一个；本件把最后那一个也走掉了，
    /// 连同它的产地 `exact_target`。⇒ **monitor 侧今天没有这条性质可断**。
    ///
    /// ⚠ 这时有两种写法，只有一种是诚实的：
    /// · 把本条删掉 ⇒ 「精确匹配」从此**无人在数**（而它仍然承重）；
    /// · 让本条**跟到新住址去数** ⇒ 就是下面这样。
    /// 判的是 daemon 那棵树（读文件，不跨 crate 调用）—— 同 `tmux_daemon_gate_guard`
    /// 那几条两树对拍的做法。
    ///
    /// ⚠ **它买不到什么**：只证明那两处**调了** `exact_target`，不证明 `exact_target`
    /// 自己产的形状对 —— 那由 daemon 那棵树自己的判据钉（本条够不着它的运行期）。
    #[test]
    fn tmux_targets_use_exact_match() {
        // ① monitor 侧：一处裸目标都不许再有（本件之后这一侧连命令串都没有了）。
        let mine = guard_core::production_code(include_str!("tmux.rs"));
        assert!(
            !mine.contains("-t {"),
            "monitor 的 `tmux.rs` 生产段又出现了 `-t {{…}}` —— 那条路已经收干净了"
        );
        // ①b [`exact_target`] 自己产的形状 —— 它今天零生产调用方，但**是 daemon 那条
        //     跨轨对拍的锚点**（见它的头注），所以这几格照旧断。
        assert_eq!(exact_target("cc-x").unwrap(), "'=cc-x:'");
        assert_eq!(exact_target("proj_cc-2").unwrap(), "'=proj_cc-2:'");
        // glob 名即便漏进来也被引号原样包住（不脱出成 shell glob）。
        assert_eq!(exact_target("si*").unwrap(), "'=si*:'");
        // 含单引号的名字仍被正确转义（`shell_quote` 的 `'\''` 形态）。
        assert!(exact_target("a'b").unwrap().starts_with("'=a"));
        assert!(exact_target("a'b").unwrap().ends_with("b:'"));
        // Gate 1：空 target 必须被拒（`=:` 会被 tmux 解析成「当前会话」）。
        assert!(exact_target("").is_err(), "空 target 必须被 Gate 1 拒绝");
        // ② 那条性质的新住址：daemon 侧抓屏与杀会话**都**过 `exact_target`。
        // 〔搬树 2026-09-17〕后端树从 `remote-daemon-proto/src/` 搬到 `<repo>/src/backend/`
        // ⇒ **中间那层 `src` 没了**。原来是 `.join("src/backend").join("src").join("control")`。
        let daemon = crate::guard_support::backend_src_root().join("control");
        let mut checked = 0usize;
        for (file, why) in [("capture_pane.rs", "抓屏"), ("kill.rs", "杀会话")] {
            let p = daemon.join(file);
            let raw = std::fs::read_to_string(&p).unwrap_or_else(|e| {
                panic!("读不到 {p:?}：{e} —— 本条的被测对象没了，它此刻在空转")
            });
            let prod = guard_core::production_code(&raw);
            assert!(
                prod.contains("exact_target("),
                "daemon 的 `control/{file}`（{why}）生产段里没有 `exact_target(` —— \n\
                 「`-t <名>` 必须是 `=<名>:` 精确形态」这条性质**今天两棵树上都没人守了**：\n\
                 monitor 侧 `K-R112` 已把它走掉（那一处的墓碑在本文件里），\n\
                 而这一处正是它唯一的新住址。裸目标会走 tmux 的\n\
                 「精确→名字开头→glob」三级解析 —— `cc-abc12345` 会命中 `cc-abc12345-2`。"
            );
            checked += 1;
        }
        assert_eq!(checked, 2, "只核到 {checked} 处 —— 本断言在空转");
    }

    #[test]
    fn parse_multi_session() {
        // 真 TAB 分隔(Rust "\t" = 0x09)。6 列,末列 @ccm_sid。
        let out = "cc-abc12345\t/home/pi/proj\tclaude\t1\t2\tsess-42\nweb\t/srv/web\tzsh\t0\t1\t\n";
        let s = parse_tmux_ls(out);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].name, "cc-abc12345");
        assert_eq!(s[0].path, "/home/pi/proj");
        assert_eq!(s[0].command, "claude");
        assert!(s[0].attached);
        assert_eq!(s[0].windows, 2);
        // @ccm_sid 有值 → Some;空串 → None(向后兼容老会话)。
        assert_eq!(s[0].sid.as_deref(), Some("sess-42"));
        assert!(!s[1].attached);
        assert_eq!(s[1].command, "zsh");
        assert_eq!(s[1].sid, None);
    }

    /// ★★ `K-R104` 跨轨对拍：**那个前缀两侧必须是同一个串**。
    ///
    /// 漂了**不会有任何东西报错** —— daemon 照旧铸它的名字，monitor 照旧过滤它以为的那个前缀，
    /// 而探针会话会开始在用户的会话列表里闪现。同 `LOCAL_ORIGIN` 那条跨轨对拍的形状：
    /// `include_str!` 读对面那份、抠出字面量、逐字比。
    #[test]
    fn the_oneshot_prefix_matches_the_daemon_side() {
        const DAEMON: &str = include_str!("../../backend/control/oneshot_session.rs");
        let line = DAEMON
            .lines()
            .find(|l| {
                l.trim_start()
                    .starts_with("pub(crate) const ONESHOT_PREFIX")
            })
            .expect("daemon 那份里找不到 `ONESHOT_PREFIX` —— 名字改了就来改这条");
        let lit = line
            .split('"')
            .nth(1)
            .expect("那一行不是 `pub(crate) const ONESHOT_PREFIX: &str = \"…\";` 的形状");
        assert_eq!(
            lit, ONESHOT_SESSION_NAME_PREFIX,
            "一次性会话前缀两侧漂了：daemon {lit:?} / monitor {:?}。\n\
             ⚠ 这种漂**不会有任何东西报错** —— 探针会话会开始在用户的会话列表里闪现。",
            ONESHOT_SESSION_NAME_PREFIX
        );
    }

    /// F10：一次性用量探针会话的识别——纯前缀匹配，不涉及新 tmux user-option（不碰
    /// `TMUX_LS_FMT` 双写点，见 `USAGE_PROBE_NAME_PREFIX` 头注）。
    #[test]
    fn usage_probe_session_name_prefix() {
        assert!(is_usage_probe_session("ccm-usage-z"));
        // `K-R104`：daemon 铸的那个名字空间也要被挡在会话列表之外。
        assert!(is_usage_probe_session("ccm-oneshot-usage-z-cc"));
        assert!(!is_usage_probe_session("ccm-oneshot")); // 无尾随连字符，不是前缀本身
        assert!(is_usage_probe_session("ccm-usage-z-2")); // 撞名重试的 -N 变体
        assert!(!is_usage_probe_session("cc-abc12345")); // 正牌会话前缀，不该被误判
        assert!(!is_usage_probe_session("web")); // 用户自己的会话
        assert!(!is_usage_probe_session("ccm-usage")); // 无尾随连字符，不是前缀本身
        assert!(!is_usage_probe_session("")); // 空
    }

    /// F10：`list_remote_tmux` 对探针会话的过滤逻辑——`parse_tmux_ls` 之后接一次
    /// `is_usage_probe_session` 过滤，验证两者组合后正牌会话保留、探针会话消失（不需要真的
    /// 发起 SSH 连接，`list_remote_tmux` 内部这段处理是纯数据变换，抽取同样的组合方式单测）。
    #[test]
    fn list_remote_tmux_filters_out_usage_probe_sessions() {
        let out = "cc-abc12345\t/home/pi/proj\tclaude\t1\t1\tsess-1\nccm-usage-z\t/home/z\tclaude\t1\t1\t\nweb\t/srv/web\tzsh\t0\t1\t\n";
        // 走**生产同一条**代码路径（`list_remote_tmux` 内联调的就是它）——此前这里把过滤表达式
        // 在测试体里抄了一遍，删掉生产侧的 filter 照样绿，是伪测试（F10 Phase D 审计发现）。
        let filtered = parse_visible_tmux_sessions(out);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|s| s.name == "cc-abc12345"));
        assert!(filtered.iter().any(|s| s.name == "web"));
        assert!(!filtered.iter().any(|s| s.name == "ccm-usage-z"));
    }

    #[test]
    fn parse_skips_malformed_and_handles_edges() {
        // 空输出 → 空。
        assert!(parse_tmux_ls("").is_empty());
        assert!(parse_tmux_ls("\n\n").is_empty());
        // 字段数不符(无 TAB / 少字段 / 旧 5 列)→ 跳过;name 空 → 跳过。
        let out = "no tabs here\nn\t/p\tsh\t0\n\t/p\tclaude\t1\t1\told5\t/p\tclaude\t1\t2\ngood\t/home/a b\tclaude\t1\t3\t";
        let s = parse_tmux_ls(out);
        assert_eq!(s.len(), 1, "只有最后一行(6 列)合法");
        assert_eq!(s[0].name, "good");
        // 路径含空格(非 TAB)保留。
        assert_eq!(s[0].path, "/home/a b");
        assert_eq!(s[0].windows, 3);
        // 末列空串 → sid None。
        assert_eq!(s[0].sid, None);
    }

    #[test]
    fn parse_windows_nonnumeric_falls_back_zero() {
        let s = parse_tmux_ls("n\t/p\tclaude\t1\tNaN\tsid-x");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].windows, 0);
        assert_eq!(s[0].sid.as_deref(), Some("sid-x"));
    }

    #[test]
    fn parse_sid_rejects_unexpanded_format_and_garbage() {
        // 极老 tmux 不展开 `#{@ccm_sid}` → 原样字面串(含 `#{}`)→ 当 None,否则 findClaudeTmux 的
        // anySidKnown 恒真、老 wrapper 用户永远走不到 cwd 回退(审计建议)。
        let s = parse_tmux_ls("n\t/p\tclaude\t1\t1\t#{@ccm_sid}");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].sid, None, "未展开格式串不当 sid");
        // 合法 sid 字符集(字母数字 + - + _)照收。
        let s2 = parse_tmux_ls("n\t/p\tclaude\t1\t1\tab_c-12");
        assert_eq!(s2[0].sid.as_deref(), Some("ab_c-12"));
    }

    /// ★★ `KR112D2`：**抓不到的五档分得开**，而且「认不出的码」不许被猜成某一档。
    ///
    /// ⚠〔`K-R112` 09-13〕它替掉的是原来那条按两个哨兵（`NO_TMUX` / `NO_PANE`）
    /// 判定的测试。那条测的东西今天**不存在**：
    /// 答案不再编码在 stdout 里，所以「屏幕内容恰好等于哨兵串」这个误判形状也随之消失
    /// —— 那正是这一刀买到的东西，不是判据被放宽。
    #[test]
    fn the_five_capture_refusals_stay_apart() {
        let msgs: Vec<String> = [
            "no_tmux",
            "no_server",
            "no_such_session",
            "invalid_args",
            "capture_failed",
        ]
        .iter()
        .map(|c| describe_capture_refusal("cc-x", c, "原话"))
        .collect();
        // ① 五句两两不同 —— 一句都不许被另一句吸收掉（老路把其中三件压成 `NO_PANE` 一句）。
        for (i, a) in msgs.iter().enumerate() {
            for b in msgs.iter().skip(i + 1) {
                assert_ne!(a, b, "两档被压成了同一句话");
            }
        }
        // ② 每一句都说得出是哪个会话，而且带上 daemon 的原话（诊断不许被吃掉）。
        for m in &msgs {
            assert!(m.contains("cc-x"), "没说是哪个会话：{m}");
            assert!(m.contains("原话"), "daemon 的原话被吃掉了：{m}");
        }
        // ③ 🔴 **认不出的码不许猜**：原样带出去，且不许长成任何一句已知档的样子。
        let unknown = describe_capture_refusal("cc-x", "zzz_new_code", "原话");
        assert!(
            unknown.contains("zzz_new_code"),
            "认不出的码没被原样带出去：{unknown}"
        );
        for m in &msgs {
            assert_ne!(
                &unknown, m,
                "认不出的码被猜成了一个已知档 —— 那是拿具体而错误的答案冒充知识"
            );
        }
    }

    #[test]
    fn fmt_uses_real_tab_not_literal_backslash_t() {
        // 回归调研 03 §3.1 坑:格式串里必须是真 TAB 字节,不能是字面 \t。
        assert!(TMUX_LS_FMT.contains('\t'), "格式串须含真 TAB");
        assert!(!TMUX_LS_FMT.contains("\\t"), "格式串不得含字面反斜杠-t");
    }

    #[test]
    fn tmux_ls_fmt_double_write_point_stays_in_sync() {
        // F08a：TMUX_LS_FMT 双写点断言（红线 I8 的机器化护栏）。monitor(本 const) 与 daemon
        // (`src/backend/observe/watcher.rs`) 分属两个独立 crate、不能共享 const，但两侧
        // `tmux ls -F` 格式串**必须逐字一致**（否则 daemon 推的列 monitor 解错位）。编译期
        // include_str! 读 daemon 源，把本 const 的真 TAB 折回源码里的 `\t` 转义再断言 daemon 源
        // 含该带引号字面量——**双向**：改 monitor 或 daemon 任一侧忘同步，本测即红。
        let daemon_src = include_str!(daemon_watcher_src!());
        let source_literal = TMUX_LS_FMT.replace('\t', "\\t");
        // 锚定到 const 定义行（非裸字面量）——否则该字面量若也出现在某条注释里，会掩盖真 const 漂移
        // （假阴性）。daemon 侧常量名同为 TMUX_LS_FMT（红线 I8 不许改），故按定义行精确比对。
        let expected_def = format!("const TMUX_LS_FMT: &str = \"{source_literal}\";");
        assert!(
            daemon_src.contains(&expected_def),
            "TMUX_LS_FMT 双写点漂移：daemon watcher.rs 不含与 monitor 侧一致的定义 {expected_def:?}\n\
             （改了 tmux ls 格式串就得两侧同步——红线 I8）"
        );
    }
}
