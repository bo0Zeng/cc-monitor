//! issue #16 P1a：远端历史浏览的 monitor 侧。
//!
//! 每条查询走**独立 SSH 连接**一次性 exec `<daemon_path> --list-projects` 等
//! （方案权衡见 issue #16 计划评论：历史浏览用户驱动低频，握手开销可接受，
//! 完全不碰稳定的流式路径；连接建立复用 `ssh_source::connect_session` 全套
//! 指纹校验/鉴权）。
//!
//! 旧 daemon 兼容：不认参数的旧版会照常进流模式、首行发 hello 帧——这里检测
//! `"kind":"hello"` 即返回明确的"daemon 版本过旧"错误（优雅降级，前端 toast）。
//!
//! 只读铁律（INVARIANT § 1）：本模块只读远端；resume/delete 对远端在前端禁用。
//! INVARIANTS § 25：本路径是一次性读取（非 at-least-once 行流），SessionViewer
//! 每次 load 全新实例，无重投幂等义务。

use crate::history::{HistoryProject, HistorySessionEntry};
use crate::messages::JsonlRecord;
use crate::parser::parse_line;
use crate::ssh_source::{self, RemoteConfig};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

/// 查询超时：列举类命令整体限时（远端扫盘 + 传输）。
const LIST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 读单会话：不设整体超时（会话可能大、流式合法耗时），但 (a) 每次 read_line 加
/// 单次超时，防"连接活着却永不来数据"卡死；(b) 总字节上限兜底，防无 EOF / 无换行
/// 的巨型损坏文件吃爆内存。
///
/// ⚠〔audit-0805 F06〕**这里原本还有一句「正常会话毫秒级、远小于上限」——那句今天是假的。**
/// 实测本机最大会话 **270,103,105 字节 / 92,967 行**（就是那次审计对话本身），
/// 已经**越过** 256 MiB 这条线 1,667,649 字节；57 MB 以上的会话有 5 个，不是孤例。
/// ⇒ 上限**会被真实数据打到**，所以「打到之后怎么办」不能是静默。
const READ_LINE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
const MAX_SESSION_BYTES: u64 = 256 * 1024 * 1024;

/// 超限时给用户的话〔audit-0805 F06，定框 **E4/E5**〕。
///
/// # 它此前是**静默**的
///
/// 读法是 `stream.take(MAX_SESSION_BYTES)` + `if n == 0 { break; }` ——
/// 到限之后 `read_line` 返回 0，与**正常 EOF 完全同形** ⇒ 前端拿到一份「看起来完整」的历史，
/// 而后面的内容**无声消失**。同一份数据走 daemon 的 `--fork-session` 那条路会**硬报错**
/// （`common/fs.rs`），走这条路却什么都不说 —— 这正是定框 **E5** 要消灭的
/// 「同一份数据走不同路得到不同答案」。
///
/// 抽成纯函数是为了让它可判据：外面那圈是真 SSH 流，测不了。
fn session_truncated_message(read_bytes: u64, lines_shown: u32) -> String {
    format!(
        "这个会话超过 {MAX_SESSION_BYTES} 字节上限，只读到前 {read_bytes} 字节（{lines_shown} 行）；\
         后面的内容**没有显示**。完整历史仍在远端那个 jsonl 文件里。"
    )
}

pub(crate) fn require_cfg_by_label(label: &str) -> Result<RemoteConfig, String> {
    crate::load_remote_config_by_label(label)
        .ok_or_else(|| format!("远端 '{label}' 未配置或未启用"))
}

/// 旧 daemon 检测：查询命令的输出行不可能含 wire 的 `"kind":"hello"`（查询模式
/// 输出裸 JSON 对象 / 裸 jsonl 行）；出现即说明远端 daemon 不认参数、进了流模式。
fn is_old_daemon_hello(line: &str) -> bool {
    line.contains(r#""kind":"hello""#) || line.contains(r#""kind": "hello""#)
}

const OLD_DAEMON_MSG: &str =
    "远端 daemon 版本过旧（不支持历史查询）——请按 src/doc/REMOTE-PHASE0-DEPLOY.md 重新构建部署";

/// 跑一条列举类查询，收集全部输出行（带整体超时 + 旧版检测）。
pub(crate) async fn run_list_query(cfg: &RemoteConfig, args: &str) -> Result<Vec<String>, String> {
    let cmd = format!("{} {}", ssh_source::shell_quote(&cfg.daemon_path), args);
    let collect = async {
        let stream = ssh_source::connect_and_exec_cmd(cfg, &cmd).await?;
        let mut reader = BufReader::new(stream);
        let mut lines = Vec::new();
        // ★〔G 审计〕原来是无界 `read_line` —— 与 F10b 修掉的那三处**同一个量**
        // （daemon 出方向单行），只是当时的人群只扫了 `ssh_source.rs`。
        // 外面那层 `LIST_TIMEOUT` 拦不住它：对端 30s 内不吐换行地灌字节，
        // `buf` 就是无界堆分配（daemon 侧同形态实测 RSS 6 MiB → 518 MiB）。
        let mut buf: Vec<u8> = Vec::new();
        loop {
            let text = match ssh_source::read_capped_line(
                &mut reader,
                &mut buf,
                ssh_source::DAEMON_FRAME_LINE_CAP,
            )
            .await
            .map_err(|e| format!("读取远端输出失败: {e}"))?
            {
                ssh_source::CappedLine::Eof => break, // EOF = 命令结束
                // 一次性查询的输出行是 JSON 记录，超上限说明对端不对劲。
                // **拒收+回错**：这条路有调用方接得住错，不像帧读那样只能横向报告。
                ssh_source::CappedLine::TooLong(bytes) => {
                    return Err(format!(
                        "远端输出的单行 {bytes} 字节，超过上限 {} —— 拒收，不拿截断的结果当完整的用",
                        ssh_source::DAEMON_FRAME_LINE_CAP
                    ));
                }
                ssh_source::CappedLine::Line => String::from_utf8_lossy(&buf).into_owned(),
            };
            let line = text.trim();
            if line.is_empty() {
                continue;
            }
            if lines.is_empty() && is_old_daemon_hello(line) {
                return Err(OLD_DAEMON_MSG.to_string());
            }
            lines.push(line.to_string());
        }
        Ok(lines)
    };
    tokio::time::timeout(LIST_TIMEOUT, collect)
        .await
        .map_err(|_| format!("远端查询超时（{}s）: {args}", LIST_TIMEOUT.as_secs()))?
}

/// 远端全文搜索 fan-out（issue #28）：对所有已配置远端各 exec 一次 `<daemon> --search`，
/// 把每行 camelCase `SessionHits` JSON 反序列化、补 `origin = 该台 label`。无远端 → 空；
/// 逐台失败 warn + 跳过（不拖垮其余台）。复用 `run_list_query`（连接/超时/旧 daemon 检测）。
///
/// `scope` 透传原始字符串（"user"/"assistant"/其它=不限）；只对 daemon 认的两值下发。
pub async fn search_remote_all(
    query: &str,
    include_tools: bool,
    scope: Option<&str>,
    after_ms: i64,
    limit: usize,
) -> Vec<crate::search::SessionHits> {
    let cfgs = crate::load_remote_configs();
    if cfgs.is_empty() {
        return Vec::new();
    }
    // 参数对所有台一致（不含 cfg），构建一次。经 shell_quote 防注入；daemon 侧再做 projects/ 白名单校验。
    let mut args = format!("--search {}", ssh_source::shell_quote(query));
    if include_tools {
        args.push_str(" --include-tools");
    }
    if let Some(s) = scope {
        if s == "user" || s == "assistant" {
            args.push_str(" --scope ");
            args.push_str(s);
        }
    }
    if after_ms > 0 {
        args.push_str(&format!(" --after-ms {after_ms}"));
    }
    args.push_str(&format!(" --limit {limit}"));

    // R9：并发 fan-out——各台查询独立、无序要求，join_all 同时查所有台（墙钟从 Σ 降到 max）。
    // 借用 cfg/args 即可（join_all 在当前任务并发 poll，不需 'static/Send）。逐台错误仍隔离。
    let results =
        futures::future::join_all(cfgs.iter().map(|cfg| run_list_query(cfg, &args))).await;
    let mut out = Vec::new();
    for (cfg, res) in cfgs.iter().zip(results) {
        let origin = cfg.origin_label();
        match res {
            Ok(lines) => {
                for line in lines {
                    match serde_json::from_str::<crate::search::SessionHits>(&line) {
                        Ok(mut sh) => {
                            sh.origin = Some(origin.clone());
                            out.push(sh);
                        }
                        Err(e) => {
                            tracing::warn!("远端 [{origin}] --search 行解析失败（跳过）: {e}");
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("远端 [{origin}] --search 失败（跳过该台）: {e}");
            }
        }
    }
    out
}

/// `K-R83`（09-12）：daemon 那一行里装着**每项目会话 sid 清单**的字段名。
///
/// # 🔴 它是常量，不是散在两处的字面量 —— 这一格是判据要求的
///
/// `KR83D1` 判的是「下游算不算得出那三个数」，**逐字不判字段叫什么名字**
///（`local_read_surface_registry.rs` 那条退役条件留了「**或等价字段**」这个口子）。
/// 而一条「喂一行进去、看算出什么」的判据，测试自己也要**造那一行** ——
/// 名字若在生产与测试里各写一遍，改名就会让判据红，那就等于**判了名字**。
/// ⇒ 名字只住这一处，两侧都从这里取：改名 = 改一行 = 判据照常绿。
pub(crate) const REMOTE_SESSION_IDS_FIELD: &str = "sessionIds";

/// 一个数**算出来了没有**。
///
/// # ★ 本件的全部题面：`Unknown` 与 `Known(0)` 不是同一个值
///
/// 09-12 之前 `remote_history.rs` 那三处是写死的 `starred_count: 0` / `hidden_count: 0` /
/// `has_live: false` —— 于是「查过了，这个项目一个星标都没有」与「压根没查」
/// **在数据里长得一模一样**。一个值装了两件事，界面上远端项目因此永远没星标、永远不活，
/// 而没有任何东西说得出这是「不知道」还是「真的是 0」。
///
/// 🔴 **〔`K-R92` 09-12 改写，上一版逐字留在下面〕这个类型现在上线了。**
///
/// 上一版这里写着：「⚠ **这个类型不上线**（今天线上那一格仍是 `HistoryProject` 的
/// `u32`/`bool`）—— 上线是 `K-R66` 的面……压成线上那个值的地方**只有一处、有名字**
/// （`Counted::wire_placeholder`）。」
///
/// 那句话记的是 `K-R83` 收窗口那一刻的世界，而它同时也是 `K-R92` 的题面：
/// **信息在系统里产生了、又被自己丢掉，且丢的那一处没有任何东西说话** ——
/// 半修比不修更危险，因为现在有人会以为那个 `0` 是准的。
/// ⇒ `K-R92` 把 `HistoryProject` / `HistorySessionEntry` 那四格改成三态，
/// [`Counted::known`] 把「不知道」如实过线成 `None`，**压平那一步整个没有了**
/// （判据 `there_is_no_place_left_that_flattens_unknown_into_a_wire_value`）。
///
/// ⚠ **仍然没做的那一半，写在这里免得变成暗账**：界面怎么把「不知道」显示出来
/// （chip 文案 / 徽标）是 `K-R66` 的面，`K-R92` 逐字只到数据层。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Counted<T> {
    /// 算出来了，就是这个数（**含真的是 0**）。
    Known(T),
    /// 算不出来，附上**为什么** —— 「不知道」要自己说得出话（定框 `E4`）。
    Unknown(WhyUnknown),
}

/// 为什么算不出来。**每一档都得说得出人话**，不许只有一个光秃秃的 `None`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WhyUnknown {
    /// daemon 那一行没带会话 sid 清单 —— 远端版本旧（`K-R83` 之前的 daemon）。
    NoSessionIdList,
    /// 带了，但**清单长度与 `sessionCount` 对不上**（或清单里混着非字符串）：
    /// 这一行坏了。⚠ 这一档**刻意不退化成「按拿得到的那几个算」** ——
    /// 那会给出一个「看起来是真值」的少数，比明说不知道更糟。
    ListDisagreesWithCount,
    /// 「这台机器上此刻有没有活会话」本机答不了：`SessionMap` 认的是**本机进程的 pid**，
    /// 远端会话不在里面。⚠ 这**不是**「远端没有活会话」。
    NoRemoteLivenessOracle,
}

impl WhyUnknown {
    /// 给日志/将来给界面用的一句人话。
    pub(crate) fn reason(self) -> &'static str {
        match self {
            // ⚠ 这句话里**刻意不用那个旧名字**（本文件上面两处历史文案里的那个词）：
            // `tool_registry::SITES` 那张旧名存量账**只许变少**（`K-R81`/`KR81D2`），
            // 新写的句子照 `K33`/`K36` 的名字来 —— 那是**那台机器的后端**。
            // 🔴 实测记一笔：本注释第一版把那个旧名字逐字抄进来解释「我没用它」，
            // 那条账当场红（登记 2、盘上 3）—— **它不剥注释**，同本文件末尾那条判据的坑。
            Self::NoSessionIdList => "远端那台的后端没带会话 sid 清单（版本旧）",
            Self::ListDisagreesWithCount => "远端那一行的 sid 清单与 sessionCount 对不上（行坏了）",
            Self::NoRemoteLivenessOracle => "远端会话的活状态本机答不了（SessionMap 只认本机进程）",
        }
    }
}

impl<T> Counted<T> {
    /// 过线：`Known(v)` ⇒ `Some(v)` · `Unknown(_)` ⇒ `None`。**这一步不丢那一维。**
    ///
    /// 🔴 它取代了 `K-R83` 那个 `wire_placeholder`（`Unknown` ⇒ `T::default()`，
    /// 也就是 `0` / `false`）。那个函数是**全仓唯一一处把「不知道」压成一个线上值**的地方，
    /// 而 `K-R92` 的裁定是：**唯一一处也是一处** —— 线上那一格装不下第三态，
    /// 就把线上那一格改成装得下，而不是在过线时把话说死。
    ///
    /// ⚠ **如实记一样被丢掉的东西**：`WhyUnknown` 那句人话不过线（它进日志，
    /// 按理由汇总一条，见 [`fanout_list_projects`] 末尾）。判据只要求下游**分得出**
    /// 「算过的」与「不知道」，**不要求**下游读得到理由 —— 把理由也送上去是 `K-R66` 的面。
    pub(crate) fn known(self) -> Option<T> {
        match self {
            Self::Known(v) => Some(v),
            Self::Unknown(_) => None,
        }
    }
}

/// `K-R92` 第一问的落点：**「这台机器上这个会话此刻活没活」谁来答。**
///
/// 〔`K-R97` 09-12 改名：`Remote` 那个前缀去掉了。〕本机那条路今天也走这个接口 ——
/// 它的绑定 `history::SessionMapLiveness` **答得出真值**（`SessionMap` 认本机 pid），
/// 远端那个绑定 [`NoLivenessOracleYet`] 仍答「不知道」。⇒ 名字里再写「远端」就是一句
/// 会误导下一个人的话（本仓 `K-R81` 那本旧名账记的正是这一族）。
///
/// # 为什么它是一个入参，而不是函数体里的一句 `false`
///
/// `KR92D2` 判的是**性质**（这条路上有没有写死的活状态），逐字**不判**那一行的行号 ——
/// 挪个位置行号就瞎了。做法与 `KR83D3` 那个 `query` 入参同源：**把答不出来的那一步
/// 做成参数**，于是「答案从哪来」在类型上就是显式的，判据也能喂一个真会答话的假真相源进来，
/// 直接看这条路**端不端得动一个真值**（第 ② 刀），以及**答不出时会不会退化成 `false`**（第 ③ 刀）。
///
/// ⚠ `Send + Sync` 是**编译器要求的，不是装饰**：`fanout_list_projects` 是 `async`，
/// 这个引用跨 `.await` 活着 ⇒ 那个 future 要 `Send`，而 `&T: Send` 要 `T: Sync`。
/// 去掉这两个 bound，`cargo test` 当场报 `future cannot be sent between threads safely`
///（实测过，如实记在这里）。
pub(crate) trait LivenessOracle: Send + Sync {
    /// `origin` = 那台机器的稳定身份（**本机那条路传空串** —— 它的真相源不看这个键）；
    /// `sid` = 会话 id。
    fn is_live(&self, origin: &str, sid: &str) -> Counted<bool>;
}

/// 🔴 **今天的生产绑定：它诚实地答「不知道」，而且全仓只有这一处这么答。**
///
/// # 现打过的四条路，没有一条今天接得上（`K-R92` 第一问的答案）
///
/// ① monitor 的 `SessionMap` —— 认的是**本机进程的 pid**，远端会话不在里面。
/// ② `lib.rs` 的 `run` 里那个 `remote_active` —— setup 闭包里的**局部** `Arc`，
///    没有 `manage` 出去，`#[tauri::command]` 够不着；`src/doc/INVARIANTS.md` §24 还钉着单写者。
/// ③ `ssh_source::announced_registry`（origin → sid → meta）—— 形状对得上，
///    **但它按 origin 的那个快照读口已经被删过一次**（`ssh_source.rs` 里那条注释逐字：
///    「其唯一读者是已删的 8s poller」），而且它只在**流式连接活着**时有效：
///    「这台没连上」与「这台没有活会话」在它眼里同形 ——
///    那**正是本件要治的病换个位置又长一次**。要用它，得先给它加一维「连没连上」，
///    那是设计，不是本件的数据层改造。
/// ④ daemon 的 `platform::proc::session_alive` —— **真相源在这**，但 `--list-projects`
///    跑在一次性查询模式（`p1a-history` 子进程），要判活得在那里再造一份 pidfile 扫描；
///    按用户 09-12 那条裁定（`DECISIONS.md#R52` 裁定一：Gate 判活改成**读观测快照、
///    且「询问即触发一次更新」**）该走的是同一条思路，而那是 daemon 的面。
///
/// ⇒ 本件**不新造真相源**，把「缺真相源」建成一等公民：这条路答「不知道」，
/// 而「不知道」现在**过得了线**（`Counted::known` ⇒ `None`）。接哪一条已报 PM 裁（`〔R92a〕`）。
pub(crate) struct NoLivenessOracleYet;

impl LivenessOracle for NoLivenessOracleYet {
    fn is_live(&self, _origin: &str, _sid: &str) -> Counted<bool> {
        Counted::Unknown(WhyUnknown::NoRemoteLivenessOracle)
    }
}

/// 一个远端项目那三个数的**来路**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectCounts {
    pub starred: Counted<u32>,
    pub hidden: Counted<u32>,
    pub has_live: Counted<bool>,
}

impl ProjectCounts {
    fn all_unknown(why: WhyUnknown) -> Self {
        Self {
            starred: Counted::Unknown(why),
            hidden: Counted::Unknown(why),
            has_live: Counted::Unknown(why),
        }
    }

    /// 这三个数里**有没有哪个是「不知道」** —— 用来决定要不要出声（不静默）。
    fn unknowns(&self) -> Vec<WhyUnknown> {
        let mut out = Vec::new();
        if let Counted::Unknown(w) = self.starred {
            out.push(w);
        }
        if let Counted::Unknown(w) = self.hidden {
            out.push(w);
        }
        if let Counted::Unknown(w) = self.has_live {
            out.push(w);
        }
        out
    }
}

/// daemon 的一行 `--list-projects` ＋ 本机 metadata ⇒ 那三个数。
///
/// # 判的是「算不算得出」，不是「字段叫什么」
///
/// 三个数的真相源全在本机、且**全部按会话 sid 索引**（metadata 按 sid 查 star/hide；
/// `SessionMap` 按 sid 查活）—— 所以 daemon 那一行只要说得出「这个项目下有哪几个 sid」，
/// star / hide 就**当场算得出真值**，一次调用、零额外进程（`KR83D3`）。
///
/// # `has_live` 由传进来的真相源答；答不出就是 `Unknown`，**不是 `false`**
///
/// 〔`K-R92` 改写〕上一版这里把「没有真相源」写死在函数体里。现在它是入参
/// （[`LivenessOracle`]），今天的生产绑定 [`NoLivenessOracleYet`] 仍答「不知道」——
/// **区别在于「谁答不出来」变成了类型上说得清、判据喂得进的一格**，理由逐条写在那个绑定上。
///
/// 汇总口径：任一会话**确定活着** ⇒ `Known(true)`（有一个活的就够了，不确定的不影响）；
/// 没有确定活的、而有答不出的 ⇒ `Unknown`；全部**确定没活** ⇒ `Known(false)`。
pub(crate) fn project_counts(
    row: &serde_json::Value,
    metadata: &crate::history::HistoryMetadata,
    origin: &str,
    liveness: &dyn LivenessOracle,
) -> ProjectCounts {
    let session_count = row["sessionCount"].as_u64().unwrap_or(0);
    let Some(ids) = row.get(REMOTE_SESSION_IDS_FIELD).and_then(|v| v.as_array()) else {
        return ProjectCounts::all_unknown(WhyUnknown::NoSessionIdList);
    };
    let sids: Vec<&str> = ids.iter().filter_map(|v| v.as_str()).collect();
    // ★ **空清单 ≠ 没有**：daemon 侧的契约是「清单与 `sessionCount` 恒等长」
    //（`observe/history_query.rs::project_row` 里两者共用同一个守卫）。对不上 ⇒ 这一行坏了
    // ⇒ 报「不知道」，**不许**拿手上这几个算出一个看起来像真值的少数。
    if sids.len() as u64 != session_count {
        return ProjectCounts::all_unknown(WhyUnknown::ListDisagreesWithCount);
    }
    let mut starred = 0u32;
    let mut hidden = 0u32;
    let mut has_live = Counted::Known(false);
    for sid in sids {
        if let Some(m) = metadata.entries.get(sid) {
            if m.starred {
                starred += 1;
            }
            if m.hidden {
                hidden += 1;
            }
        }
        match liveness.is_live(origin, sid) {
            // 一个确定活着的就够了，后面答不答得出都不改变这一行的答案。
            Counted::Known(true) => {
                has_live = Counted::Known(true);
                break;
            }
            Counted::Known(false) => {}
            // 🔴 **答不出不许被「其余几个都没活」盖过去**：那样整行又变成一句
            // 「这个项目没有活会话」的断言，而其中有几个根本没人查过。
            Counted::Unknown(w) => has_live = Counted::Unknown(w),
        }
    }
    ProjectCounts {
        starred: Counted::Known(starred),
        hidden: Counted::Known(hidden),
        has_live,
    }
}

/// daemon 的一行 `--list-projects` ＋ 本机 metadata ＋ 判活真相源 ⇒ **一条项目行**。
/// 这一行没有 `dirName`（拿不到懒加载的键）⇒ `None`，跳过。
///
/// # 🔴 为什么它是一个函数 ——〔`K-R97` 09-12〕**本机那条路今天也走它**
///
/// `K-R83`/`K-R92` 落地时这段还内联在 [`fanout_list_projects`] 里，因为只有远端一条路吃它。
/// `K-R97` 把 `history::list_history_projects` 也改成问本机后端的同一条 `--list-projects`
/// ⇒ **「一行 JSON 怎么读成一个项目」有了第二个消费者**。抄一份是 `K33`「所有命令只许有一处」
/// 最常见的破法（`K-R54` 那张 16 处的表整张都是这么长出来的）⇒ 收成一处，两侧都调它。
///
/// `origin`：`Some(label)` = 那台远端；**`None` = 本机**（`HistoryProject::origin` 随之为 `None`，
/// 前端据此判「这是本地项目」）。判活按 `origin.unwrap_or("")` 提问 —— 本机那个真相源
/// （`history::SessionMapLiveness`）根本不看这个键。
///
/// 🔴 `K-R83`：这里从前把 `starred_count` / `hidden_count` 写死成零、`has_live` 写死成假，
/// 旁边一句注释写着「远端不合并本地元数据计数（列表级开销不值得）」——**那句话说的是代价，
/// 落到数据里却成了一个断言**：零同时表示「查过了，是零」与「压根没查」。
/// 今天那一行带上了会话 sid 清单 ⇒ star/hide **一次算得出真值**、零额外进程。
/// 🔴 `K-R92` 补上后半段：那个「不知道」**现在过得了线**（下面那几个 `.known()`）——
/// `K-R83` 落地后它是算出来了又在过线那一步被自己丢掉，那比从没算过更坏。
///
/// ⚠ 上面这段**刻意不逐字抄那几个字面量**：本文件末尾
/// `there_is_no_place_left_that_flattens_unknown_into_a_wire_value`
/// 是子串扫描、**不剥注释**（`readonly_guard` 头注记着同一族坑）—— 抄进来会自伤。
pub(crate) fn history_project_from_row(
    v: &serde_json::Value,
    metadata: &crate::history::HistoryMetadata,
    origin: Option<&str>,
    liveness: &dyn LivenessOracle,
) -> Option<(HistoryProject, ProjectCounts)> {
    let dir_name = v["dirName"].as_str().unwrap_or_default().to_string();
    if dir_name.is_empty() {
        return None;
    }
    let project_path = v["projectPath"].as_str().unwrap_or_default().to_string();
    // 两侧同一口径：projectName = cwd 最后一段；提取不到 cwd 时回退编码目录名。
    let project_name = if project_path.is_empty() {
        dir_name.clone()
    } else {
        project_path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&dir_name)
            .to_string()
    };
    let counts = project_counts(v, metadata, origin.unwrap_or(""), liveness);
    Some((
        HistoryProject {
            project_path,
            project_name,
            // 「懒加载 key」= 后端给的编码目录名，前端原样传回
            // （远端 `stream_remote_history_sessions` / 本机 `stream_history_sessions_in_project`）。
            project_dir: dir_name,
            session_count: v["sessionCount"].as_u64().unwrap_or(0) as u32,
            // `K-R92`：`.known()` 把「不知道」如实过线成 `None`。
            starred_count: counts.starred.known(),
            hidden_count: counts.hidden.known(),
            last_activity: v["lastActivityMs"].as_i64().unwrap_or(0),
            has_live: counts.has_live.known(),
            origin: origin.map(str::to_string),
        },
        counts,
    ))
}

/// 「不知道」要出声（定框 `E4`：静默失败一律给身份）。**按理由汇总一条**，
/// 不是每个项目一条 —— 一台旧后端上有几百个项目，逐项目 warn 就是把日志刷成噪音，
/// 而噪音与静默在「谁都不会读」这件事上是同一个结局。
///
/// 〔`K-R97`〕两条路共用：远端 fan-out 与本机那条各传自己的 `what`。
pub(crate) fn log_unknown_reasons(what: &str, rows: &[(HistoryProject, ProjectCounts)]) {
    let mut why_counts: std::collections::BTreeMap<&'static str, usize> = Default::default();
    for (_, c) in rows {
        for w in c.unknowns() {
            *why_counts.entry(w.reason()).or_default() += 1;
        }
    }
    for (why, n) in why_counts {
        tracing::info!("{what}：{n} 处数**不知道**（不是 0）—— {why}");
    }
}

/// F76（#46）：远端来源列表结果 = 项目 + **失败台清单**。
///
/// 后端 fan-out 语义是「任一台成功即 `Ok`，失败台 warn+跳过」——前端单看项目列表无从区分
/// 「某台失败缺项」与「某台真的无项目」。带上 `failed_hosts` 让前端判断「部分失败」：部分失败
/// 时**不冻结 TTL 缓存**（下次 open 重试失败台），避免瞬断台的项目在缓存里消失整个 TTL 窗口。
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct RemoteProjectsResult {
    pub projects: Vec<HistoryProject>,
    /// 本次 fan-out 中查询失败（已跳过）的台的 origin label。空 = 全部台成功。
    pub failed_hosts: Vec<String>,
}

/// 一次 fan-out 的**全部**产出：上线的那一份，**以及被压掉的那一份**。
///
/// 项目与它那三个数的来路**绑在同一个元组里**，不是两个同序数组 ——
/// 平行数组一旦哪天有人在中间 `retain` 一下就静默错位，而错位的后果正是本件要治的病。
pub(crate) struct FanoutOutcome {
    pub rows: Vec<(HistoryProject, ProjectCounts)>,
    pub failed_hosts: Vec<String>,
}

impl FanoutOutcome {
    /// 取上线的那一份。〔`K-R92`〕上一版这行写着「**只有这一处**在丢「不知道」那一维」——
    /// 那一维现在不丢了（[`Counted::known`] ⇒ `Option`）；本函数只是把
    /// `ProjectCounts`（带 `WhyUnknown` 那句人话）留在进程里、不往前端送。
    fn into_wire(self) -> RemoteProjectsResult {
        RemoteProjectsResult {
            projects: self.rows.into_iter().map(|(p, _)| p).collect(),
            failed_hosts: self.failed_hosts,
        }
    }
}

/// 远端项目列表（多机 #30：fan-out 所有已配置远端）。无远端 → 空列表（前端无感合并）；
/// 单台查询失败 → warn + 跳过该台（不拖垮其余台）。各 project 带 `origin = 该台 label`。
#[tauri::command]
pub async fn list_remote_history_projects() -> Result<RemoteProjectsResult, String> {
    let cfgs = crate::load_remote_configs();
    if cfgs.is_empty() {
        return Ok(RemoteProjectsResult {
            projects: Vec::new(),
            failed_hosts: Vec::new(),
        });
    }
    // 条目级元数据（star/hide 按 session_id 存**本机**，远端会话同样适用 ——
    // `stream_remote_history_sessions` 早就是这么合的）。**整趟只读一次**：
    // 它是本机文件，与台数、项目数都无关。
    let metadata = crate::history::load_metadata().unwrap_or_default();
    fanout_list_projects(
        &cfgs,
        &metadata,
        &NoLivenessOracleYet,
        |cfg: RemoteConfig| async move { run_list_query(&cfg, "--list-projects").await },
    )
    .await
    .map(FanoutOutcome::into_wire)
}

/// `--list-projects` fan-out 的**本体**；「去问远端」这件事**是参数**。
///
/// # 🔴 为什么查询要作为参数传进来 —— `KR83D3` 判的那个可数的事实
///
/// `KR83D3` 逐字：**别判「代码里有没有 for 循环」**（那判的是写法），
/// 要判**「一次调用里 spawn 了几次」**。而真 SSH 在红线内跑不了 ⇒
/// 把 spawn 那一步做成入参，判据就能拿一个**会计数的假查询**喂进来，
/// 直接数出「1 台 × N 个项目 ⇒ 查询被调了几次」。
///
/// 失效方向（本函数存在的理由）：一旦有人为了拿 star/hide 而在下面那个循环里
/// 补一句 `--list-sessions`，计数当场从 `台数` 涨成 `台数 + 项目数`，判据红。
pub(crate) async fn fanout_list_projects<Q, F>(
    cfgs: &[RemoteConfig],
    metadata: &crate::history::HistoryMetadata,
    liveness: &dyn LivenessOracle,
    query: Q,
) -> Result<FanoutOutcome, String>
where
    // ⚠ 吃 `RemoteConfig`（clone）而不是 `&RemoteConfig`：后者要 HRTB
    // （`for<'a> Fn(&'a RemoteConfig) -> impl Future + 'a`），今天的 Rust 表达不出来。
    // 每台 clone 一次是一次结构体拷贝，与「每台一次 SSH 握手」比可以忽略。
    Q: Fn(RemoteConfig) -> F,
    F: std::future::Future<Output = Result<Vec<String>, String>>,
{
    let mut rows: Vec<(HistoryProject, ProjectCounts)> = Vec::new();
    let mut any_ok = false;
    let mut last_err = String::new();
    let mut failed_hosts: Vec<String> = Vec::new();
    // R9：并发 fan-out 所有台（各台独立、无序要求），墙钟从 Σ(各台) 降到 max(各台)。逐台错误仍隔离。
    let results = futures::future::join_all(cfgs.iter().cloned().map(&query)).await;
    for (cfg, res) in cfgs.iter().zip(results) {
        let lines = match res {
            Ok(l) => {
                any_ok = true;
                l
            }
            Err(e) => {
                // 逐台失败不拖垮整体：该台 warn + 跳过，其余台照常返回。记进 failed_hosts
                // 让前端识别「部分失败」（F76：部分失败不冻结缓存、下次 open 重试该台）。
                tracing::warn!(
                    "远端 [{}] --list-projects 失败（跳过该台）: {e}",
                    cfg.origin_label()
                );
                failed_hosts.push(cfg.origin_label());
                last_err = e;
                continue;
            }
        };
        let origin_label = cfg.origin_label();
        for line in lines {
            let v: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("remote --list-projects 行解析失败（跳过）: {e}: {line}");
                    continue;
                }
            };
            // 〔`K-R97` 09-12〕这一段原本内联在这里，现在住 [`history_project_from_row`] ——
            // 本机那条路也吃同一份解释了，抄一份就是 `K-R54` 那张表的长法。
            if let Some(row) = history_project_from_row(&v, metadata, Some(&origin_label), liveness)
            {
                rows.push(row);
            }
        }
    }
    // 配了远端但**全部**台查询都失败 → 返回 Err（前端可 toast），避免与"无远端配置"的
    // 空列表（cfgs.is_empty 早返）混淆，让用户能区分"没配"和"配了但连不上"。
    if !any_ok {
        return Err(format!(
            "所有远端历史查询失败（{} 台），最后一个错误: {last_err}",
            cfgs.len()
        ));
    }
    log_unknown_reasons("远端项目列表", &rows);
    tracing::info!(
        "list_remote_history_projects: {} projects from {} host(s), {} failed",
        rows.len(),
        cfgs.len(),
        failed_hosts.len()
    );
    Ok(FanoutOutcome { rows, failed_hosts })
}

/// daemon 的一行 `--list-sessions` ＋ 本机 metadata ＋ 判活真相源 ⇒ 一条会话行。
/// 行坏了（解析不了 / 没有 `sessionId`）⇒ `None`（跳过，同上一版的行为）。
///
/// # 🔴 为什么它是一个函数，而不是那个 `#[tauri::command]` 里的一段
///
/// 与 daemon 侧 `history_query::project_row` 同一条理由：那条命令的出口是一条 SSH ＋
/// 一个 `tauri::ipc::Channel`，红线内测不了；而 `KR92D2` 要判的是**这一行带了什么**。
/// ⇒ 把「算出那一行」与「把它发出去」分开，判据就喂得进一个会答话的假真相源。
fn remote_session_entry(
    line: &str,
    project_dir: &str,
    metadata: &crate::history::HistoryMetadata,
    origin: &str,
    liveness: &dyn LivenessOracle,
) -> Option<HistorySessionEntry> {
    let v: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("remote --list-sessions 行解析失败（跳过）: {e}");
            return None;
        }
    };
    let session_id = v["sessionId"].as_str().unwrap_or_default().to_string();
    if session_id.is_empty() {
        return None;
    }
    let cwd = v["cwd"].as_str().unwrap_or_default().to_string();
    let project_name = cwd
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(project_dir)
        .to_string();
    let meta = metadata
        .entries
        .get(&session_id)
        .cloned()
        .unwrap_or_default();
    // 🔴 `K-R92`〔`〔R83c〕` 那一处〕：上一版这一格是**写死的假** —— 一句
    // 「这个远端会话没活着」的断言，而本机根本没有远端的判活真相源。
    // 现在它由 `liveness` 答；答不出 ⇒ `None`（不知道），**不许退化成 `Some(false)`**。
    let live = liveness.is_live(origin, &session_id).known();
    Some(HistorySessionEntry {
        session_id,
        project_path: cwd,
        project_name,
        ai_title: v["aiTitle"].as_str().map(String::from),
        // Batch11-F32：p1h daemon 附 isBg；旧 daemon 缺字段 → false 安全降级
        is_bg: v["isBg"].as_bool().unwrap_or(false),
        first_user_excerpt: v["firstUserExcerpt"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        started_at: v["startedAtMs"].as_i64().unwrap_or(0),
        updated_at: v["updatedAtMs"].as_i64().unwrap_or(0),
        jsonl_path: v["jsonlPath"].as_str().unwrap_or_default().to_string(),
        is_live: live,
        message_count_approx: v["messageCountApprox"].as_u64().unwrap_or(0) as u32,
        starred: meta.starred,
        custom_title: meta.custom_title,
        hidden: meta.hidden,
        // P1a：daemon 不提取 fork 关系，远端会话在 fork 树上呈平铺
        forked_from_session_id: None,
        forked_from_message_uuid: None,
        origin: Some(origin.to_string()),
    })
}

/// 远端某项目的历史会话列表（流式 Channel，对齐本地 stream_history_sessions_in_project）。
/// `project_dir` = 远端编码目录名（list_remote_history_projects 给出的 projectDir）。
#[tauri::command]
pub async fn stream_remote_history_sessions(
    project_dir: String,
    origin: String,
    on_entry: tauri::ipc::Channel<HistorySessionEntry>,
) -> Result<u32, String> {
    let cfg = require_cfg_by_label(&origin)?;
    // 防穿越：目录名不允许含分隔符（daemon 侧同样校验，双层防御）
    if project_dir.contains('/') || project_dir.contains('\\') || project_dir.contains("..") {
        return Err(format!("非法项目目录名: {project_dir}"));
    }
    let args = format!("--list-sessions {}", ssh_source::shell_quote(&project_dir));
    let lines = run_list_query(&cfg, &args).await?;
    // 条目级元数据（star/rename/hide）按 session_id 存本地，远端会话同样适用
    let metadata = crate::history::load_metadata().unwrap_or_default();
    let origin_label = cfg.origin_label();
    let mut total = 0u32;
    for line in lines {
        let Some(entry) = remote_session_entry(
            &line,
            &project_dir,
            &metadata,
            &origin_label,
            &NoLivenessOracleYet,
        ) else {
            continue;
        };
        if on_entry.send(entry).is_err() {
            tracing::info!("stream_remote_history_sessions: 前端取消");
            return Ok(total);
        }
        total += 1;
    }
    tracing::info!("stream_remote_history_sessions({project_dir}): {total} sessions");
    Ok(total)
}

/// 流式读取远端单个会话（对齐本地 stream_read_session_jsonl 的 chunk 口径：
/// 每 100 条一发，payload 带 origin=Some(host)，SessionViewer 零改动复用）。
#[tauri::command]
pub async fn stream_read_remote_session(
    jsonl_path: String,
    origin: String,
    on_chunk: tauri::ipc::Channel<Vec<crate::bridge::JsonlLinePayload>>,
) -> Result<u32, String> {
    const CHUNK_SIZE: usize = 100;
    let cfg = require_cfg_by_label(&origin)?;
    // 深度防御（与 stream_remote_history_sessions 的 project_dir 校验对称）：jsonl_path 来自
    // 前端，monitor 侧先做廉价校验（拒 `..` + 强制 .jsonl 后缀）。真正的越权读由 daemon 侧
    // canonicalize + projects/ 前缀 + symlink 逃逸校验兜底，这里补齐不对称的防御缺口。
    if jsonl_path.contains("..") || !jsonl_path.ends_with(".jsonl") {
        return Err(format!("非法会话路径: {jsonl_path}"));
    }
    let started = std::time::Instant::now();
    // 与本地 history.rs 的 file_stem 口径一致：剥**一个** ".jsonl" 后缀（strip_suffix
    // 是字面后缀，不是 trim_end_matches 的字符集语义）。
    let file_name = jsonl_path.rsplit(['/', '\\']).next().unwrap_or("");
    let session_id = file_name
        .strip_suffix(".jsonl")
        .unwrap_or(file_name)
        .to_string();
    let args = format!("--read-session {}", ssh_source::shell_quote(&jsonl_path));
    let cmd = format!("{} {}", ssh_source::shell_quote(&cfg.daemon_path), args);
    let stream = ssh_source::connect_and_exec_cmd(&cfg, &cmd).await?;
    // F06：`+ 1` 是为了**能分辨「到限」与「正好读完」** —— 只 take(MAX) 的话，
    // 到限时 read_line 返回 0，与正常 EOF 完全同形，于是静默截断。
    let mut reader = BufReader::new(stream.take(MAX_SESSION_BYTES + 1));
    let mut read_bytes: u64 = 0;
    let mut buf = String::new();
    let mut line_buf: Vec<u8> = Vec::new();
    let mut cwd_seen: Option<String> = None;
    let mut chunk: Vec<crate::bridge::JsonlLinePayload> = Vec::with_capacity(CHUNK_SIZE);
    let mut total = 0u32;
    let mut next_seq: u64 = 0;
    let mut first_line = true;
    loop {
        // ★〔G 审计〕原来是无界 `read_line`。下面那条 `MAX_SESSION_BYTES` 是**总量**且
        // **读完再判** —— 一条 10 GiB 的行会在 `read_line` 返回**之前**就把内存吃光，
        // 那条总量检查根本轮不到跑。这正是 daemon 侧 `inbound.rs` 头注逐字警告的
        // 「上限必须在**读的时候**生效，不能读完再判」，而当时那次实测是 RSS 6 MiB → 518 MiB。
        // ⇒ 补一层**单行**上限（与 daemon 出方向单行同量），总量那条保持不动。
        let n = match tokio::time::timeout(
            READ_LINE_TIMEOUT,
            ssh_source::read_capped_line(
                &mut reader,
                &mut line_buf,
                ssh_source::DAEMON_FRAME_LINE_CAP,
            ),
        )
        .await
        .map_err(|_| "读取远端会话超时（单次读取卡住）".to_string())?
        .map_err(|e| format!("读取远端会话失败: {e}"))?
        {
            ssh_source::CappedLine::Eof => break,
            // 超单行上限与超总量**同一档处置**（截断+说清）：都是「这份会话读不完整了，
            // 而且明说为什么」。措辞分开，因为用户要采取的动作不同。
            ssh_source::CappedLine::TooLong(bytes) => {
                return Err(format!(
                    "远端会话里有一行 {bytes} 字节，超过单行上限 {} —— 已停止读取。\
                     这不是会话太大（那会报另一句），是**单条记录**异常巨大，多半该直接看源文件。",
                    ssh_source::DAEMON_FRAME_LINE_CAP
                ));
            }
            ssh_source::CappedLine::Line => {
                buf.clear();
                buf.push_str(&String::from_utf8_lossy(&line_buf));
                buf.len()
            }
        };
        read_bytes += n as u64;
        if read_bytes > MAX_SESSION_BYTES {
            // F06：**不许静默截断**。同一份数据走 daemon 的 `--fork-session` 会硬报错，
            // 走这条路却假装读完了 —— 定框 E5 要的是「同一份数据走不同路得到同一个答案」。
            return Err(session_truncated_message(read_bytes, total));
        }
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        if first_line {
            first_line = false;
            if is_old_daemon_hello(trimmed) {
                return Err(OLD_DAEMON_MSG.to_string());
            }
        }
        // 与本地 stream_read_session_jsonl 同口径：parse + displayable 过滤 + per-file seq
        let rec = match parse_line(trimmed) {
            Ok(Some(r)) if r.is_displayable() => r,
            _ => continue,
        };
        if let JsonlRecord::User { cwd, .. } = &rec {
            if cwd_seen.is_none() {
                cwd_seen = cwd.clone();
            }
        }
        let seq = next_seq;
        next_seq += 1;
        chunk.push(crate::bridge::JsonlLinePayload {
            session_id: session_id.clone(),
            cwd: cwd_seen.clone(),
            path: jsonl_path.clone(),
            seq,
            origin: Some(cfg.origin_label()),
            message: rec,
        });
        total += 1;
        if chunk.len() >= CHUNK_SIZE {
            let full = std::mem::replace(&mut chunk, Vec::with_capacity(CHUNK_SIZE));
            if on_chunk.send(full).is_err() {
                tracing::info!("stream_read_remote_session({session_id}): 前端取消于 {total} 条");
                return Ok(total);
            }
        }
    }
    if !chunk.is_empty() {
        let _ = on_chunk.send(chunk);
    }
    tracing::info!(
        "stream_read_remote_session({session_id}): {total} records in {}ms",
        started.elapsed().as_millis()
    );
    Ok(total)
}

/// 删除一个远端历史会话的 jsonl（issue 未拆，F11）。**只读铁律豁免（SS-G）**：用户
/// 显式删除 + 前端二次确认 + `sftp::remove_remote_file` 双重路径守卫。删除后清本地元数据。
#[tauri::command]
pub async fn delete_remote_history_session(
    origin: String,
    jsonl_path: String,
) -> Result<(), String> {
    let cfg = require_cfg_by_label(&origin)?;
    crate::sftp::remove_remote_file(&cfg, &jsonl_path).await?;
    // 清本地元数据（注解按 sid = jsonl 文件名 stem）。
    if let Some(sid) = jsonl_stem(&jsonl_path) {
        crate::history::remove_metadata_entry(&sid);
    }
    Ok(())
}

/// 远端 POSIX 路径取文件名 stem（去目录、去 `.jsonl`）。非 jsonl / 无文件名 → None。
fn jsonl_stem(path: &str) -> Option<String> {
    let name = path.rsplit('/').next()?;
    name.strip_suffix(".jsonl").map(str::to_string)
}

#[cfg(test)]
#[path = "../../../tests/bridge/remote_history_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/bridge/remote_history_f06_tests.rs"]
mod f06_tests;

/// `K-R83`（09-12）：**「不知道」不许再和「真的是 0」长成一个样。**
///
/// 三条判据（`KR83D1` / `KR83D2` / `KR83D3`）的落点，逐条对着件文件 `§1` 的死值验写。
#[cfg(test)]
#[path = "../../../tests/bridge/remote_history_kr83_tests.rs"]
mod kr83_tests;
