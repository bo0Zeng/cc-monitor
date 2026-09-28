//! F51 tmux 反查 / F60 画面预览(控制类命令走通道 B = russh exec,不干扰前台 PowerShell 终端)。
//!
//! tab 右键菜单打开时按需查询远端 tmux 会话列表,前端按 `pane_current_path==cwd +
//! pane_current_command==claude` 反查该 tab 的 Claude 正跑在哪个 tmux 会话,命中则一键
//! `ssh -t … tmux attach -t <名>`(交互走通道 A = PowerShell)。
//!
//! **最隐蔽的重写坑(调研 03 档 §3.1)**:`tmux ls -F` 的格式串**不解释**字面 `\t`——给什么
//! 字节原样输出。所以分隔符必须是**真 TAB 字节(0x09)**。Rust 里 `"\t"` 是真 TAB(勿写
//! `\\t`),`parse_tmux_ls` 按真 TAB `split`。F60 抓屏曾续挂本模块（〔C4e〕已迁到界面 `src/tmux-control.ts`）;kill/rename
//! 明确不做(见 MASTERPLAN 不做清单),F52 短路门未扩本模块。

use crate::copy_table::copy_text;
use crate::ssh_source;
use serde::Serialize;

/// `tmux ls -F` 的格式串。字段以**真 TAB**分隔(见模块注释):
/// name ⇥ pane_current_path ⇥ pane_current_command ⇥ attached(1/0) ⇥ windows ⇥ @ccm_sid。
/// **F74**:末列 `#{@ccm_sid}` 是 `__ccm_rbind` 写的 tmux user option = 「这个 tmux 此刻在跑
/// 哪个 CC sid」的权威信号(pane title 被 Claude 活动标题抢写、不可靠;user option Claude 碰
/// 不到)。**未设置的会话此列为空串**(老会话 / 未装 wrapper)→ 解析成 `sid: None`,消费方回退
/// 旧的 path/cmd 匹配,向后兼容。
// 〔SH1〕生产段今天零处用它（跨 SSH 那条 `tmux ls` 改问后端了）；留着是因为它是与后端 watcher 那一份对拍的双写点
//   （帧流里推的 raw 就按这个格式串切，`TMUX_LS_FMT_FIELDS` 仍在用）。
#[cfg_attr(not(test), allow(dead_code))]
const TMUX_LS_FMT: &str = "#{session_name}\t#{pane_current_path}\t#{pane_current_command}\t#{?session_attached,1,0}\t#{session_windows}\t#{@ccm_sid}";

/// `TMUX_LS_FMT` 的列数 —— [`tmux_tab_underflow`] 的 N。**改格式串必须同步这个数**
/// （格式串本身是红线 I8 双写点，见上面头注）。
const TMUX_LS_FMT_FIELDS: usize = 6;

// 〔SH1 · 09-26〕这里原先住着跨 SSH 那条 `tmux ls` 用的 UTF-8 旗 `UTF8_CLIENT_FLAG`〔散文墓碑〕（K-R12 · `INVARIANTS §49`，与后端那个家跨仓对拍）。
//   `list_remote_tmux` 改问那台后端的 `tmux-list` 之后 monitor 侧**零处**跨 SSH 的 tmux 读 ⇒ 旗随之删（它的家在后端 `common/tmux_utf8.rs`，
//   那一趟 `tmux ls` 用的是 env 形 `UTF8_CLIENT_ENV`）；段数下溢的处置（下面那个谓词）照旧留着，仍与后端那个家对拍。

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
/// ⚠ **K-R12 下一拍（09-04）订正这条边界**：backend 那两份已经归位到**一个家**
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
/// 本函数还吃 **backend 推来的 `tmux_sessions` 帧**那份 raw，而那条路的入口不在本文件里
/// （backend 侧由 `watcher.rs::classify_tmux_probe` 把关；**老后端没有那道关**）。
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

/// 列远端 tmux 会话。〔SH1〕问那台后端的 `tmux-list`（与流里推的那份观测同一趟 `tmux ls`），不再经拨号链路跑 shell。
/// 三档不共用读数：没装 tmux = `None`（前端隐藏 attach 项）· 零会话 = `Some([])` · 通道脏 / 问不到 = `Err`（fail-closed）。
#[tauri::command]
pub async fn list_remote_tmux(origin: String) -> Result<Option<Vec<TmuxSession>>, String> {
    let o = crate::origin::Origin(origin);
    let v = crate::backend::control::frame_query::call(
        &o,
        "tmux-list",
        serde_json::json!({}),
        crate::backend::control::frame_query::Deadline::within(TMUX_LIST_BUDGET),
    )
    .await?;
    let who = crate::backend::control::frame_query::who(&o);
    let (installed, lines) =
        decode_tmux_list(&v).ok_or_else(|| copy_text("rsTmux.list.shape", &[("who", &who)]))?;
    if !installed {
        return Ok(None);
    }
    // K-R12 `J1`：段数下溢 ⇒ 通道被改写 ⇒ `Err`（后端那一侧已判过一次；老后端没有那道关，这里照旧再判）。
    if let Some(bad) = lines
        .iter()
        .find(|l| !l.trim().is_empty() && tmux_tab_underflow(l, TMUX_LS_FMT_FIELDS))
    {
        return Err(copy_text(
            "rsTmux.list.unparsable",
            &[
                ("split", &(bad.split('\t').count()).to_string()),
                ("fields", &TMUX_LS_FMT_FIELDS.to_string()),
                ("bad", &format!("{:?}", bad)),
            ],
        ));
    }
    Ok(Some(parse_tmux_ls(&lines.join("\n"))))
}

/// 问 `tmux-list` 的期限（后端那一趟 `tmux ls` 自带 5 s 上界）。
const TMUX_LIST_BUDGET: std::time::Duration = std::time::Duration::from_secs(15);

/// `tmux-list` 的成品 → `(装了没有, 原样行)` —— 纯函数，严格收（恰好两格、类型对）；对不上 ⇒ `None`。
pub(crate) fn decode_tmux_list(v: &serde_json::Value) -> Option<(bool, Vec<String>)> {
    let o = v.as_object().filter(|o| o.len() == 2)?;
    let installed = o.get("installed")?.as_bool()?;
    let lines = o
        .get("lines")?
        .as_array()?
        .iter()
        .map(|l| l.as_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()?;
    Some((installed, lines))
}

/// P3-刀2-UI：**本机今天有哪些 tmux 会话** —— 〔MIG-1〕与远端同一条路：问本机常驻后端 `tmux-list`（[`list_remote_tmux`] 那一问，`<local>`）。
///
/// 两个消费者：① 铸名（`mintTmuxName` 要一个 `existing` 集合，issue #76）② 杀会话的菜单（认出哪个 tmux 跑着本 tab 的 sid，
/// 必须有 `@ccm_sid`，`INVARIANTS §30`）。〔从前读 monitor 自己存的那份 `tmux ls` 原文（`ssh_source::tmux_raw_for`〔散文墓碑〕）；
///  那本账随会话账本搬进后端删了 —— 本机现问一趟，与远端同一个口径。〕
///
/// 回值保持本机那三档：列表 = 知道（没装 tmux ⇒ 空表：一个名字都没占）· `None` = **不知道**（本机后端不在 / 回话脏）——
/// 空表会让调用方以为「一个会话都没有」，那是把「不知道」当成「知道没有」。
#[tauri::command]
pub async fn list_local_tmux() -> Option<Vec<TmuxSession>> {
    match list_remote_tmux(crate::backend::control::inbound_client::LOCAL_ORIGIN.to_string()).await
    {
        Ok(Some(v)) => Some(v),
        Ok(None) => Some(Vec::new()),
        Err(e) => {
            tracing::warn!("本机 tmux 会话问不到（{e}）⇒ 当「不知道」");
            None
        }
    }
}

// 〔MIG-1 · `99 §2.1 ⑬`〕这里原来住着 `TmuxSessions.observation` 在 monitor 这一侧的分类（`classify_tmux_observation`〔散文墓碑〕·
//   `SkipReason` · `OBS_*` 三个双写点字面量）—— 喂 monitor 那两份收割器的。收割搬进后端会话账本之后零调用方，删了；
//   「这一份观测能不能拿来收割」只剩后端一个家（`src/backend/observe/watcher.rs::tmux_frame_is_observable`）。

// 〔C4e · 第四波 4C〕这里原来住着抓屏那一族在 monitor 侧的解释：帧命令名常量 `CAPTURE_PANE`、
//   五个拒绝码的人话 `describe_capture_refusal`〔散文墓碑〕（再往前是 `K-R112` 删掉的 `classify_capture_output`〔散文墓碑〕）。
//   抓屏改由界面经通道直接问那台机器的后端（`src/tmux-control.ts::capturePane`），「五档分得开、认不出的码原样带出去」
//   那条口径随之搬到 TS 的 `captureRefusal`（`tests/tmux-control.vitest.ts` 逐码钉着）。

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
/// 唯一真正的危险默认值（抓屏刻意不过身份门，见后端 `control/capture_pane.rs` 头注）。
///
/// 〔DUP3 · 主会话 09-26 裁〕Gate 1 **并进 `gate-core` 的 tmux 名那一族**：目标都是**已有会话**（抓屏 · 结束 · 送键），
/// 判定就是那一族里「已有会话」那一条 `gate_core::existing_tmux_name_issue`（V131 ②：空 · 控制符 · 视觉欺骗字符）——
/// 先前这里的「只拒空」（本文件一个私有谓词 ＋ 界面 `tmux-control.ts` 一份）是它的真子集，两份住址并成那一份。
/// **仍然不收紧 glob / 元字符**（`*` / `;` / `$` / 空格）：`shell_quote` 已安全引号化，已有会话名里真有 glob 字符
/// （`si*` 这类合法目标，`tmux_targets_use_exact_match` 钉着「glob 名被引号原样包住、不脱出」）；
/// 新建路径禁 glob 是 `new_tmux_name_issue` 的事（INVARIANTS §31a「第二道防线」）。
///
/// ⚠ `K-R72`（09-12）把这一问从 [`exact_target`] 里**分出来（不是复制一份）**：`exact_target` 产的是**给 shell 用的**
/// 精确串 `'=<名>:'`，让别的入口为一次校验去要一个用不上的串就是「一个值装了两件事」。今天它只剩 [`exact_target`]
/// 这个跨轨对拍锚点在用（下面那段写着为什么锚点还得留着）；抓屏 · 送键 · 杀会话三条的目标名由后端入口判
/// （界面那一份 TS 零；〔TAIL〕后端那两份也并成 `kill.rs::admit_existing_name` → 同一条 `gate-core`）。
/// 判据 `tests::gate1_admits_an_existing_target_by_the_one_gate_core_rule`（正反各一格）。
fn gate1_admit_target(target: &str) -> Result<(), String> {
    match gate_core::existing_tmux_name_issue(target) {
        None => Ok(()),
        Some(gate_core::TmuxNameIssue::Empty) => Err(copy_text(
            "rsTmux.target.empty",
            &[("target", &format!("{:?}", target))],
        )),
        Some(gate_core::TmuxNameIssue::Control(c) | gate_core::TmuxNameIssue::Deceptive(c)) => {
            Err(copy_text(
                "rsTmux.target.badChar",
                &[
                    ("target", &format!("{:?}", target)),
                    ("c", &format!("U+{:04X}", c as u32)),
                ],
            ))
        }
        // 「已有会话」那一条只回上面三种（前导 `-` · 目标语法 · 超长是新建那一条的事）。
        Some(other) => Err(copy_text(
            "rsTmux.target.badChar",
            &[
                ("target", &format!("{:?}", target)),
                ("c", &format!("{other:?}")),
            ],
        )),
    }
}

// ★★ `K-R112`（09-13）：**这里原来住着 `build_capture_pane_cmd`**（抓屏那条 SSH 串的构造器）。〔散文墓碑〕
//
// 它是 [`exact_target`] 在 monitor 侧**最后一个**消费者（`K-R72` 走了另外三个）。
// 抓屏改走 `capture-pane` 帧之后它零生产调用点 ⇒ 整块删（`KR72D1` 逐字禁止
// 「把回落改成恒失败的桩留在原地 —— 那不是删，那是把一份实现变成一句谎话」）。
//
// 🔴 **那条「必须精确匹配」的性质今天真正在跑的那一份住 backend**：
// `src/backend/control/capture_pane.rs::capture_on` 逐字
// 「Gate 1（`=name:` 精确匹配）—— 裸 `-t <名>` 会被 tmux 按『精确名 → 名字开头 → glob』解析」，
// 它调的是后端自己那份 `launch::exact_target`，与 `kill` 共用同一份。
//
// **Gate 1 的空目标那一格**：`=:` 会被 tmux 解析成「当前会话」。〔C4e〕三条命令（抓屏 / 送键 / 杀会话）迁到界面之后
// 那一格曾在界面就地判；〔DUP3〕Gate 1 并进 `gate-core` 那一族、TS 零 —— 空目标原样交给后端，由后端入口拒
// （`kill.rs::parse_name` · `launch.rs::parse_request`，`invalid_args`）。

/// F04：`exact_target` 是 fallible——Gate 1 折进这一个函数本身。
///
/// **裸 `-t <名>` 不是精确匹配**：tmux 依次按「精确名 → **名字开头** → **glob**」解析，
/// 只有 `sib-2` 存在时 `-t sib` 命中的是 `sib-2`（tmux 3.6 实测）。
/// 尾冒号不能省：`send-keys`/`capture-pane` 收的是 target-**pane**，`=名`（无冒号）rc=1 完全失效。
///
/// # 🔴 `K-R112`（09-13）：**本函数今天没有生产调用方了 —— 而它必须留着，理由如实写**
///
/// 最后一个消费者 `build_capture_pane_cmd` 随抓屏改走帧面一起删了（上面那块墓碑）〔散文墓碑〕。
/// **不能顺手删掉它**：backend 侧
/// `control/launch.rs::tests::exact_target_shape_matches_the_monitor_side`
/// 拿 monitor 这一处当**跨轨对拍锚点** —— 它 `include_str!` 本文件，要求里面找得到
/// `={target}:` 那个形状，理由逐字「两侧必须同形，否则一边打到兄弟会话上而另一边不会」。
/// 而**那条性质今天仍然成立、仍然值得守**（backend 的 `capture_pane` / `kill` 都在用它那份）。
/// ⇒ 留下这个壳，`#[allow(dead_code)]` 明写「今天没人调」，**不假装它在路上**
/// （形状抄 `K-R72` 给 `is_ccm_tmux_name` 那个转调壳留的先例）。
///
/// ⚠ **想真删它，得先在后端那棵树上给那条对拍换个锚点** —— `src/backend/**`
/// 不在本件写区（归 `K-R113`）。**已上报，别当它没有主人。**
#[allow(dead_code)]
pub(crate) fn exact_target(target: &str) -> Result<String, String> {
    gate1_admit_target(target)?;
    Ok(ssh_source::shell_quote(&format!("={target}:")))
}

// 〔C4e · 第四波 4C〕这里原来住着「后端通道不在」那一档的用户可见文案 `no_channel_message`〔散文墓碑〕（`K-R72` / `K-R112`）：
//   本机与远端两句话不同（下一步不同），抓屏 · 送键 · 杀会话三条共用。三条都迁到界面之后，
//   那两句随之搬进文案表（`tmuxControl.channel.localDown` / `remoteDown`），「本机与远端的话不许一样」
//   由 `tests/tmux-control.vitest.ts`「通道不在」那一条钉着。

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
// **§34 那三道门没有消失，只是只剩一个家**：Gate 1 的判定住 `gate-core`（〔DUP3〕已有会话那一条；三条路的空目标今天由
// 后端入口拒，界面那一份删了）· Gate 2 / Gate 3 住后端的 `control/gate.rs`
// （`admit` / `admit_destructive`，判定本体转调 `gate-core`，金表 `gate2-golden.tsv`
// 由后端侧与 `backend/control/gate2_parity.rs` 两条轨道共读）。
// 回潮闸在 `tmux_backend_gate_guard`：〔C4e〕monitor 生产段里再长出一处破坏性的 tmux 动词（`kill-session` / `send-keys`）就红。

// 〔C4e · 第四波 4C〕这里原来住着抓屏的发送端 `capture_via_backend` 与 Tauri 命令 `capture_remote_pane`〔散文墓碑〕：
//   空目标先拒（〔DUP3〕这一件今天归后端入口，界面那一份删了）· 预问那台后端认不认 · 转 `capture-pane` · 取 `screen`。其余三件今天只在界面一处
//   （`src/tmux-control.ts::capturePane`），monitor 那一跳只搬字节（`chan/webview.rs::chan_call`）。
//   `K-R112` 买到的三样（本机也能预览 · 五档分开 · 精确形态只剩后端一份）一样没丢：本机仍经 `<local>` 那条长连接、
//   五档在 TS 逐码分开、`=name:` 仍只住后端 `control/capture_pane.rs::capture_on`。

// 〔C4e · 第四波 4C〕这里原来住着杀会话与送键两条 Tauri 命令 `kill_remote_tmux` / `tmux_send_keys`〔散文墓碑〕
//   （F79 / A5；`K-R72` 起只剩后端一条路、三态分流）。两条迁到界面：`src/tmux-control.ts::killSession` /
//   `sendKeys` 经通道直接说后端的 `kill` / `launch`（`send-into`；〔RST 续 · V41〕裸键 mode `send-keys-raw` 随 V154 无调用者删了）。
//   **它们买到的东西一样没丢**：Gate 1 空目标仍被拒（〔DUP3〕由后端入口拒，界面那一份删了）· Gate 2 / 3 仍只在后端 `control/gate.rs` ·
//   `Refused` 与 `NoChannel` 仍是两句话 · 仍然没有第二条路（界面那一侧结构上没有 SSH）。

/// 本工具建的 tmux 会话名判定：`<X>-cc[-N]` 后缀形（今天产的那种）**或**老的 `cc-` 前缀形，
/// 且只含 `[A-Za-z0-9_-]`。
///
/// ⚠ 〔`K-R96` 09-12 订正〕这一行**原本写反了**：写的是「`cc-` 前缀 + …（`cc-<sid8>[-N]` 恒满足）」，
/// 而 S4b-3b（用户 2026-07-31）早就把前缀反转成了**后缀** —— 判定本体（`gate-core`）两种都认，
/// 只有这句散文停在反转之前。⚠ 而 `<X>` 今天也不是 `<sid8>`：`K-R96` 之后是 **`<项目名>`**
/// （用户 `R55`：「要是可读的名字 / 不要id」）。**sid 不在名字里，它骑在 `@ccm_sid` 上。**
///
/// F04：**不再是唯一身份判据**，降级为 Gate 2（identity）union 的本地半支——`@ccm_sid` 已设
/// 是远端半支（`K-R72` 起只在 backend `control/gate.rs::admit` 里核验；先前 monitor 侧
/// `build_guarded_tmux_cmd` 那条 SSH 串里还有第二份，随两条回落一起删了）。
/// 命中此判据即可跳过远端核验（零 IO，覆盖今天
/// 100% 的真实流量）；未命中不代表拒绝，只代表"需要问远端 `@ccm_sid`"。**不删除**——F02 之前的
/// 老 `cc-*` 会话没有 `@ccm_sid`，只靠这条名字判据仍必须可 kill/send-keys，否则是向后兼容回归。
///
/// **F03：实现搬进 `gate-core`**（定框 C1「一份代码、两种承载」）—— backend 的
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

/// backend 侧 `watcher.rs` 的源码路径 —— **跨 crate 硬路径的单一落点**。
///
/// # 为什么要有这个常量
///
/// monitor 的两条对拍守卫用 `include_str!` 读后端的源码（两个 crate 不能共享 `const`，
/// 只能靠「读对方源码 + 断言」防跨语言/跨 crate 漂移）。U2 的 Phase D 审计点名过：
/// **这类硬路径在后端重构时会一起断，而且断的是编译期**。
///
/// U3 把 `watcher.rs` 搬进 `observe/` 时它**当场兑现** —— `cargo test --lib` 直接
/// `couldn't read src/../../backend/observe/watcher.rs`。
/// 好消息是它**响**（编译错，不是静默假绿）；坏消息是它有两处、还散着。收进一个常量，
/// 下次后端再搬家只改这一行。
///
/// ⚠ **必须是 `macro_rules!` 不能是 `const`**：`include_str!` 只接受**字面量 token**，
/// 喂给它一个 `const` 会报 `argument must be a string literal`（我第一版就这么写的）。
/// 宏能展开成字面量，于是既拿到了单一落点、又满足 `include_str!` 的要求。
// 🔴 **步 7b（16 §6 第 3 批）把这个宏搬进了测试文件本体**
// （`tests/bridge/backend/control/tmux_tests.rs` 的开头）。理由是 `16 §5.4a` 规则 1 的反面：
// `include_str!` 按**调用点所在文件**解析相对路径，而调用点已经搬去 `tests/bridge/`——
// 宏留在这里就意味着字面量要写成「相对另一个文件」，那是个会骗人的住址。
// ⇒ 单一落点这条好处一点没丢，只是落点跟着它唯一的消费者走。
#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/tmux_tests.rs"]
mod tests;
