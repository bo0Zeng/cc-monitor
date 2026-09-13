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
    "远端 daemon 版本过旧（不支持历史查询）——请按 doc/REMOTE-PHASE0-DEPLOY.md 重新构建部署";

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

/// F88a-remote（#52）：远端用量聚合 fan-out。对**所有**已配置远端各 exec 一次
/// `<daemon> --usage`（daemon 在远端 CPU 服务端按 requestId 逐字段 MAX 聚合，避免拉整库回本地），
/// 把每行 camelCase `SessionUsageRow` JSON 反序列化、补 `origin = 该台 label`。无远端 → 空；
/// 〔`K-R59` 09-11：原先这里还写着「**daemonless 台跳过**」并真的 `filter` 掉了它们 ——
/// 那一档整个没了（定框 `K35`）⇒ 今天一台都不排。〕逐台失败 warn+跳过（不拖垮其余台）。
/// 复用 `run_list_query`（连接/超时/旧 daemon hello 检测——旧 daemon 不认 `--usage` → 优雅降级空）。
/// **口径与本地 `usage::accumulate_usage` 一字对齐**（daemon `usage_query.rs` 移植，改口径须同步两处）。
#[tauri::command]
pub async fn aggregate_remote_usage_all() -> Vec<crate::usage::SessionUsageRow> {
    let cfgs: Vec<RemoteConfig> = crate::load_remote_configs();
    if cfgs.is_empty() {
        return Vec::new();
    }
    // 并发 fan-out（各台独立、无序），墙钟从 Σ 降到 max；逐台错误隔离。
    let results =
        futures::future::join_all(cfgs.iter().map(|cfg| run_list_query(cfg, "--usage"))).await;
    let mut out = Vec::new();
    for (cfg, res) in cfgs.iter().zip(results) {
        let origin = cfg.origin_label();
        match res {
            Ok(lines) => {
                for line in lines {
                    match serde_json::from_str::<crate::usage::SessionUsageRow>(&line) {
                        Ok(mut row) => {
                            row.origin = Some(origin.clone());
                            out.push(row);
                        }
                        Err(e) => {
                            tracing::warn!("远端 [{origin}] --usage 行解析失败（跳过）: {e}");
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("远端 [{origin}] --usage 失败（跳过该台）: {e}");
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

/// `K-R92` 第一问的落点：**「这台机器上这个远端会话此刻活没活」谁来答。**
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
pub(crate) trait RemoteLiveness: Send + Sync {
    /// `origin` = 那台机器的稳定身份；`sid` = 会话 id。
    fn is_live(&self, origin: &str, sid: &str) -> Counted<bool>;
}

/// 🔴 **今天的生产绑定：它诚实地答「不知道」，而且全仓只有这一处这么答。**
///
/// # 现打过的四条路，没有一条今天接得上（`K-R92` 第一问的答案）
///
/// ① monitor 的 `SessionMap` —— 认的是**本机进程的 pid**，远端会话不在里面。
/// ② `lib.rs` 的 `run` 里那个 `remote_active` —— setup 闭包里的**局部** `Arc`，
///    没有 `manage` 出去，`#[tauri::command]` 够不着；`doc/INVARIANTS.md` §24 还钉着单写者。
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

impl RemoteLiveness for NoLivenessOracleYet {
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
/// （[`RemoteLiveness`]），今天的生产绑定 [`NoLivenessOracleYet`] 仍答「不知道」——
/// **区别在于「谁答不出来」变成了类型上说得清、判据喂得进的一格**，理由逐条写在那个绑定上。
///
/// 汇总口径：任一会话**确定活着** ⇒ `Known(true)`（有一个活的就够了，不确定的不影响）；
/// 没有确定活的、而有答不出的 ⇒ `Unknown`；全部**确定没活** ⇒ `Known(false)`。
pub(crate) fn project_counts(
    row: &serde_json::Value,
    metadata: &crate::history::HistoryMetadata,
    origin: &str,
    liveness: &dyn RemoteLiveness,
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

/// F76（#46）：远端来源列表结果 = 项目 + **失败台清单**。
///
/// 后端 fan-out 语义是「任一台成功即 `Ok`，失败台 warn+跳过」——前端单看项目列表无从区分
/// 「某台失败缺项」与「某台真的无项目」。带上 `failed_hosts` 让前端判断「部分失败」：部分失败
/// 时**不冻结 TTL 缓存**（下次 open 重试失败台），避免瞬断台的项目在缓存里消失整个 TTL 窗口。
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
    liveness: &dyn RemoteLiveness,
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
        for line in lines {
            let v: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("remote --list-projects 行解析失败（跳过）: {e}: {line}");
                    continue;
                }
            };
            let dir_name = v["dirName"].as_str().unwrap_or_default().to_string();
            if dir_name.is_empty() {
                continue;
            }
            let project_path = v["projectPath"].as_str().unwrap_or_default().to_string();
            // 对齐本地口径：projectName = cwd 最后一段；提取不到 cwd 时回退编码目录名
            let project_name = if project_path.is_empty() {
                dir_name.clone()
            } else {
                project_path
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(&dir_name)
                    .to_string()
            };
            // 🔴 `K-R83`：这里从前把 `starred_count` / `hidden_count` 写死成零、
            // `has_live` 写死成假（就在本函数构造 `HistoryProject` 那三行），旁边一句注释写着
            // 「远端不合并本地元数据计数（列表级开销不值得）」——**那句话说的是代价，
            // 落到数据里却成了一个断言**：零同时表示「查过了，是零」与「压根没查」。
            // 今天 daemon 那一行带上了会话 sid 清单 ⇒ star/hide **一次算得出真值**、
            // 零额外进程；活状态仍没有真相源，就如实报「不知道」而不是一个假。
            // 🔴 `K-R92` 补上后半段：那个「不知道」**现在过得了线**（下面三行的 `.known()`）——
            // `K-R83` 落地后它是算出来了又在过线那一步被自己丢掉，那比从没算过更坏。
            //
            // ⚠ 上面这段**刻意不逐字抄那几个字面量**：本文件末尾
            // `there_is_no_place_left_that_flattens_unknown_into_a_wire_value`
            // 是子串扫描、**不剥注释**（`readonly_guard` 头注记着同一族坑）——
            // 抄进来会自伤。实测红过一次，如实记在这里。
            let origin_label = cfg.origin_label();
            let counts = project_counts(&v, metadata, &origin_label, liveness);
            rows.push((
                HistoryProject {
                    project_path,
                    project_name,
                    // 远端的"懒加载 key"= 远端编码目录名（前端原样传回 stream_remote_history_sessions）
                    project_dir: dir_name,
                    session_count: v["sessionCount"].as_u64().unwrap_or(0) as u32,
                    // `K-R92`：`.known()` 把「不知道」如实过线成 `None`。
                    // 上一版这三行是 `.wire_placeholder()`（`Unknown` ⇒ `0`/`false`）——
                    // 那一步把刚算出来的那一维当场丢掉，而丢的地方没有任何东西说话。
                    starred_count: counts.starred.known(),
                    hidden_count: counts.hidden.known(),
                    last_activity: v["lastActivityMs"].as_i64().unwrap_or(0),
                    has_live: counts.has_live.known(),
                    origin: Some(origin_label),
                },
                counts,
            ));
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
    // 「不知道」要出声（定框 `E4`：静默失败一律给身份）。**按理由汇总一条**，
    // 不是每个项目一条 —— 一台旧 daemon 上有几百个项目，逐项目 warn 就是把日志刷成噪音，
    // 而噪音与静默在「谁都不会读」这件事上是同一个结局。
    let mut why_counts: std::collections::BTreeMap<&'static str, usize> = Default::default();
    for (_, c) in &rows {
        for w in c.unknowns() {
            *why_counts.entry(w.reason()).or_default() += 1;
        }
    }
    for (why, n) in why_counts {
        tracing::info!("远端项目列表：{n} 处数**不知道**（不是 0）—— {why}");
    }
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
    liveness: &dyn RemoteLiveness,
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
mod tests {
    use super::*;

    #[test]
    fn jsonl_stem_basics() {
        assert_eq!(
            jsonl_stem("/home/pi/.claude/projects/p/abc-123.jsonl").as_deref(),
            Some("abc-123")
        );
        assert_eq!(jsonl_stem("abc.jsonl").as_deref(), Some("abc"));
        assert_eq!(jsonl_stem("/x/y/note.txt"), None);
        assert_eq!(jsonl_stem(""), None);
    }

    #[test]
    fn old_daemon_hello_detected() {
        assert!(is_old_daemon_hello(
            r#"{"kind":"hello","v":1,"build_id":"phase0-proto","host_arch":"aarch64","claude_dir":"/home/pi/.claude"}"#
        ));
        // 查询模式的正常输出不含 kind
        assert!(!is_old_daemon_hello(
            r#"{"dirName":"-home-pi-proj","projectPath":"/home/pi/proj","sessionCount":3,"lastActivityMs":1}"#
        ));
        // jsonl 正文里聊到 hello 不该误判（必须是 kind 字段形态）
        assert!(!is_old_daemon_hello(
            r#"{"type":"user","message":{"content":"say hello"}}"#
        ));
    }

    #[test]
    fn shell_quote_via_ssh_source() {
        assert_eq!(
            crate::ssh_source::shell_quote("/a/b c.jsonl"),
            "'/a/b c.jsonl'"
        );
        assert_eq!(crate::ssh_source::shell_quote("a'b"), r"'a'\''b'");
    }
}

#[cfg(test)]
mod f06_tests {
    use super::*;

    /// ★ **超限必须说话，而且要说清「少了什么」**〔audit-0805 F06，定框 E4/E5〕。
    ///
    /// 此前是 `take(MAX)` + `if n == 0 { break; }` —— 到限与正常 EOF **完全同形**，
    /// 前端拿到一份「看起来完整」的历史而后面的内容无声消失。
    /// 而同一份数据走 daemon 的 `--fork-session` 会**硬报错**（`common/fs.rs`）：
    /// **同一份数据走两条路得到两个答案**，正是 E5 要消灭的。
    #[test]
    fn the_truncation_message_says_what_is_missing_and_where_it_still_is() {
        let m = session_truncated_message(MAX_SESSION_BYTES + 1, 92_967);
        assert!(m.contains("上限"), "要说清是撞了上限：{m}");
        assert!(
            m.contains("92967") || m.contains("92_967"),
            "★ 要报出**已显示多少行** —— 用户得知道自己看到的是哪一截：{m}"
        );
        assert!(
            m.contains("没有显示"),
            "★ 要明说后面的内容没显示 —— 不说这句就等于还是在静默：{m}"
        );
        assert!(
            m.contains("仍在远端"),
            "★ 要告诉用户完整历史还在（这条与丢帧不同：数据没丢，是没读完）：{m}"
        );
    }

    /// ★ **上限检查还接在路径上，不只是「登记过」**〔audit-0805 §5 1g，08-06 补〕。
    ///
    /// # 先量后写：1g 那句话对，但它给的理由只挡住一半
    ///
    /// §5 1g 逐字写着「把检测拆成 `if false` 全仓没有任何判据会红」。08-06 真做了这次变异
    /// （把下面那个条件换成 `if false {`）：monitor 全量 **passed=989 failed=0** ——
    /// 那句话在**行为层**成立。
    ///
    /// 但它给的**理由**是「住在吃真 SSH 流的 async 函数里，红线内造不出 >256 MiB 的远端流」。
    /// 这次变异与流有多大**毫无关系** —— 那个理由挡住的只是「撞到上限时运行起来真的会 Err」，
    /// 挡不住「判断被整个摘掉」。**阻塞四问 ①「挡住的是整件还是一部分」又一次命中。**
    ///
    /// 顺带说清另一件容易误读的事：`byte_cap_registry` 里**登记了**这处上限，
    /// 但它管的是「上限有没有被登记 + 语义有没有声明」，**不是「检查会不会触发」** ——
    /// 「有个登记表覆盖着」不等于「这条路上有人守着」。
    ///
    /// # 钉什么、不钉什么
    ///
    /// 钉**源码形态的三段链**，每一段单独被摘掉，静默截断都会回来：
    ///
    /// | 段 | 摘掉它会怎样 |
    /// |---|---|
    /// | `take(…+ 1)` 里的 **`+ 1`** | 到限时 `read_line` 返回 0，与正常 EOF **完全同形** ⇒ 无声截断 |
    /// | 那个 `read_bytes >` 条件 | 判断没了，读到 cap 就当读完了 |
    /// | 那一支的 `return Err(…)` | 换成 `break` 就是「读完了」，前端拿到一份看起来完整的历史 |
    ///
    /// **不钉**「撞到上限时运行起来真的会 Err」—— 那要把读循环从这个吃真 SSH 流的
    /// async fn 里抽出来（连 `tauri::ipc::Channel` 那个出口一起抽象）。本轮不做，
    /// §5 1g **保留**，但范围缩小到行为层那一半。
    ///
    /// # 对照组是自带的，不是另写一条
    ///
    /// 本判据的 needle 在**未剥测试段**的源码里有两处：生产一处 + 本测试的字面量一处。
    /// 下面第一条断言的就是「剥完之后它变少了」—— 于是「有人把 `production_source` 拿掉」
    /// 会当场红，而不是让本条静默地读到自己（F23 那一族，本区已犯过三次）。
    #[test]
    fn the_cap_check_is_still_wired_not_just_declared() {
        let raw = include_str!("remote_history.rs");
        let prod = guard_core::production_source(raw);

        const COND: &str = "if read_bytes > MAX_SESSION_BYTES {";
        assert!(
            raw.matches(COND).count() > prod.matches(COND).count(),
            "★ 对照组：剥掉测试段之后这个 needle 应当变少（本测试自己的字面量被剥走了）。\n\
             没变少 ⇒ `production_source` 没在起作用，下面三条就是在**读自己**、恒绿。"
        );

        // ① `+ 1`：它是「到限」与「正好读完」唯一的区分手段。
        guard_core::pin_line(
            &prod,
            "let mut reader = BufReader::new(stream.take(MAX_SESSION_BYTES + 1));",
        )
        .expect(
            "★ `take(MAX_SESSION_BYTES + 1)` 这一行不在了（或写法变了）。\n\
             只 take(MAX) 的话，到限时 read_line 返回 0，与正常 EOF **完全同形** ——\n\
             下面两条即使都在，也再没有任何东西能分辨「读完了」和「读到上限」。",
        );

        // ② 条件本身：这一条正是 08-06 那次 `if false` 变异摘掉的东西。
        let at = guard_core::pin_line(&prod, COND).expect(
            "★ 上限判断不在生产段里了。08-06 实测：把它换成 `if false {`，\n\
             monitor 全量 989 条**一条都不会红** —— 本条就是为那个洞补的。",
        );

        // ③ 那一支必须**报错**，不能是 break/continue：后者等于「读完了」。
        let body = prod
            .lines()
            .skip(at + 1)
            .find(|l| {
                let t = l.trim();
                !t.is_empty() && !t.starts_with("//")
            })
            .unwrap_or("");
        assert!(
            body.trim()
                .starts_with("return Err(session_truncated_message("),
            "★ 撞上限那一支的第一句是 `{}`，不是 `return Err(session_truncated_message(…))`。\n\
             换成 break/continue 就是「读完了」：前端拿到一份**看起来完整**的历史，\n\
             而同一份数据走 daemon 的 `--fork-session` 会硬报错 —— 同一份数据两条路两个答案，\n\
             正是定框 E5 要消灭的那种不一致。",
            body.trim()
        );
    }
}

/// `K-R83`（09-12）：**「不知道」不许再和「真的是 0」长成一个样。**
///
/// 三条判据（`KR83D1` / `KR83D2` / `KR83D3`）的落点，逐条对着件文件 `§1` 的死值验写。
#[cfg(test)]
mod kr83_tests {
    use super::*;
    use crate::history::{EntryMetadata, HistoryMetadata};

    /// daemon 那一行的夹具。**字段名从生产常量取** —— 见 [`REMOTE_SESSION_IDS_FIELD`]
    /// 的头注：名字写死在测试里，改名就会让判据红，那等于把「判能力」偷换成「判名字」，
    /// 正是 `KR83D1` 第 ③ 刀要治的。
    fn row(dir: &str, sids: &[&str]) -> serde_json::Value {
        let mut v = serde_json::json!({
            "dirName": dir,
            "projectPath": format!("/home/u/{dir}"),
            "sessionCount": sids.len(),
            "lastActivityMs": 1_700_000_000_000i64,
        });
        v[REMOTE_SESSION_IDS_FIELD] = serde_json::json!(sids);
        v
    }

    /// 旧 daemon（`K-R83` 之前）的那一行：四个字段，**没有**会话 sid 清单。
    fn row_without_ids(dir: &str, session_count: u64) -> serde_json::Value {
        serde_json::json!({
            "dirName": dir,
            "projectPath": format!("/home/u/{dir}"),
            "sessionCount": session_count,
            "lastActivityMs": 1_700_000_000_000i64,
        })
    }

    fn metadata_with(starred: &[&str], hidden: &[&str]) -> HistoryMetadata {
        let mut m = HistoryMetadata::default();
        for sid in starred {
            m.entries.entry((*sid).to_string()).or_default().starred = true;
        }
        for sid in hidden {
            m.entries.entry((*sid).to_string()).or_default().hidden = true;
        }
        m
    }

    fn cfg(label: &str) -> RemoteConfig {
        serde_json::from_value(serde_json::json!({
            "host": format!("{label}.example"),
            "label": label,
            "user": "u",
            "daemonPath": "/opt/cc-monitor-remote",
        }))
        .expect("夹具配置")
    }

    // ── `K-R92` 的假真相源：判据用它喂「答得出 / 答不出」两侧 ──────────────────
    //
    // ⚠ 它是**夹具**，不是第二份实现：生产那份是 [`NoLivenessOracleYet`]，
    // 两者的差别正是本件要判的东西（这条路端不端得动一个真值）。

    /// 点名哪几个 sid 活着，其余**确定没活**（答得出）。
    struct Oracle(&'static [&'static str]);
    impl RemoteLiveness for Oracle {
        fn is_live(&self, _origin: &str, sid: &str) -> Counted<bool> {
            Counted::Known(self.0.contains(&sid))
        }
    }

    /// 只对点名的那几个 sid 答不出，其余确定没活 —— 用来验「一行里混着答得出与答不出」。
    struct OracleBlindTo(&'static [&'static str]);
    impl RemoteLiveness for OracleBlindTo {
        fn is_live(&self, _origin: &str, sid: &str) -> Counted<bool> {
            if self.0.contains(&sid) {
                Counted::Unknown(WhyUnknown::NoRemoteLivenessOracle)
            } else {
                Counted::Known(false)
            }
        }
    }

    // ───────────────────────────── `KR83D1` ─────────────────────────────

    /// ★★ `KR83D1`：**daemon 那一行带得出算这三个数所需的东西。**
    ///
    /// 判的是**性质**：拿到那一行 ＋ 本机 metadata，`starred_count` / `hidden_count`
    /// 就地算得出**真值**。
    ///
    /// ⚠ 本条**不判字段叫什么名字**：夹具的那个 key 是从生产常量取的，
    /// 只改名、内容等价 ⇒ 本条照常绿（第 ③ 刀）。
    #[test]
    fn a_row_that_carries_session_ids_lets_us_compute_the_real_numbers() {
        let md = metadata_with(&["s2"], &["s1", "s3"]);
        let c = project_counts(
            &row("-home-u-p", &["s1", "s2", "s3", "s4"]),
            &md,
            "pi",
            &NoLivenessOracleYet,
        );

        assert_eq!(
            c.starred,
            Counted::Known(1),
            "★ 这个项目下 s2 是星标的 —— 算得出来就该是 1。\n\
             读到别的值 = 要么没在算（写死），要么 sid 对不上号。"
        );
        assert_eq!(
            c.hidden,
            Counted::Known(2),
            "★ s1 / s3 隐藏 ⇒ 2。这一格与上一格分开写是有意的：\
             两个数用同一份清单算，一起错和分别错要能分辨。"
        );
    }

    /// ★ `KR83D1` 第 ① 刀的正向：**把清单从那一行摘掉 ⇒ 算不出**（而不是算出 0）。
    ///
    /// 这一条同时是 `KR83D2` 第 ③ 刀的落点，两条判据在这里是**同一格**：
    /// 「算不出的那一档」必须与「真的是 0」区分得开。
    #[test]
    fn a_row_without_the_list_yields_unknown_and_unknown_is_not_zero() {
        let md = metadata_with(&["s1"], &[]);
        let c = project_counts(
            &row_without_ids("-home-u-p", 3),
            &md,
            "pi",
            &NoLivenessOracleYet,
        );

        assert_eq!(
            c.starred,
            Counted::Unknown(WhyUnknown::NoSessionIdList),
            "★ 旧 daemon 不带清单 ⇒ 这三个数是**不知道**"
        );
        assert_ne!(
            c.starred,
            Counted::Known(0),
            "🔴 **本件的全部题面就在这一行断言上。**\n\
             「不知道」退化成 `Known(0)` ⇒ 界面上「查过了，一个星标都没有」与「压根没查」\n\
             重新变成同一个值 —— 那正是 09-12 之前 `fanout_list_projects` 里那三处写死的病。\n\
             ⚠ 只把写死的 `0` 换成另一个写死值也治不了它：那还是一个值装两件事。"
        );
        assert_ne!(c.hidden, Counted::Known(0), "同上，hidden 那一格");
        assert_ne!(c.has_live, Counted::Known(false), "同上，has_live 那一格");
    }

    /// ★ `KR83D1` 第 ② 刀：**清单是空的、而这个项目下确实有会话 ⇒ 不许当成 0。**
    ///
    /// 「空清单」与「没有会话」是两件事：后者在 daemon 侧**根本不会出这一行**
    ///（`project_row` 返回 `None`）。所以出了行还空 = 这一行坏了 ⇒ 报「不知道」。
    #[test]
    fn an_empty_list_on_a_project_that_has_sessions_is_a_broken_row_not_a_zero() {
        let md = metadata_with(&["s1"], &["s1"]);
        let mut broken = row("-home-u-p", &[]);
        broken["sessionCount"] = serde_json::json!(3); // 有 3 个会话，清单却是空的

        let c = project_counts(&broken, &md, "pi", &NoLivenessOracleYet);
        assert_eq!(
            c.starred,
            Counted::Unknown(WhyUnknown::ListDisagreesWithCount),
            "★ 空清单 ≠ 没有 —— 拿它算出 0 就是把一行坏数据当成了真值"
        );
        assert_ne!(
            c.starred,
            Counted::Known(0),
            "🔴 空清单不许退化成「真的是 0」"
        );

        // 少一个也一样（不是只有「全空」才算坏）。⚠ 刻意**不**退化成
        // 「按拿得到的那两个算」：那会给出一个看起来像真值的少数，比明说不知道更糟。
        let mut short = row("-home-u-p", &["s1", "s2"]);
        short["sessionCount"] = serde_json::json!(3);
        assert_eq!(
            project_counts(&short, &md, "pi", &NoLivenessOracleYet).hidden,
            Counted::Unknown(WhyUnknown::ListDisagreesWithCount),
            "★ 短清单同理"
        );
    }

    /// ★ `KR83D1` 第 ③ 刀的**对照组**：真的把字段改名，判据必须还是绿的。
    ///
    /// 这里逐字模拟「只改字段名、内容等价」：拿生产常量以外的另一个名字造一行，
    /// 再用**同一个常量**去读 —— 于是「改名」这件事在判据眼里只是改了一个字符串常量。
    #[test]
    fn renaming_the_field_does_not_break_the_criterion() {
        let md = metadata_with(&["x1"], &[]);
        // 等价字段：换个名字、内容一字不改。
        let renamed = serde_json::json!({
            "dirName": "p",
            "projectPath": "/home/u/p",
            "sessionCount": 2,
            "lastActivityMs": 1i64,
            "sessionUuids": ["x1", "x2"],
        });
        // 判据读的是**常量指到的那个 key**：把常量指过去，算出来的数一字不变。
        let mut as_if_renamed = renamed.clone();
        as_if_renamed[REMOTE_SESSION_IDS_FIELD] = renamed["sessionUuids"].clone();
        assert_eq!(
            project_counts(&as_if_renamed, &md, "pi", &NoLivenessOracleYet).starred,
            Counted::Known(1),
            "★ 「或等价字段」那个口子要留着：内容等价的一行，算出来的数必须一样。"
        );
    }

    // ───────────────────────────── `KR83D2` ─────────────────────────────

    /// ★★ `KR83D2` 第 ① 刀：**恢复成今天的 `0 / 0 / false` 写死 ⇒ 必须红。**
    ///
    /// 走的是完整那条路（fan-out → 解析 → 算 → 压成线上形状），
    /// 所以「把 `starred_count:` 改回字面量 0」在这里当场红。
    #[tokio::test]
    async fn the_wire_row_carries_the_real_star_and_hide_counts_now() {
        let md = metadata_with(&["s1", "s2"], &["s3"]);
        let lines = vec![row("-home-u-p", &["s1", "s2", "s3"]).to_string()];
        let out = fanout_list_projects(&[cfg("pi")], &md, &NoLivenessOracleYet, |_| {
            let lines = lines.clone();
            async move { Ok(lines) }
        })
        .await
        .expect("一台成功");

        let wire = out.rows[0].0.clone();
        assert_eq!(
            (wire.starred_count, wire.hidden_count),
            (Some(2), Some(1)),
            "★ 09-12 之前这两格是字面量 `0` —— 改回去这一条当场红。\n\
             `K-R92` 起线上那一格是三态：`Some(n)` = 算过了，`None` = 不知道。"
        );
        assert_eq!(wire.session_count, 3);
        assert_eq!(wire.origin.as_deref(), Some("pi"));
    }

    /// ★★ `KR83D2` 第 ③ 刀 ＋ `KR92D1`：**算不出的那一档，必须与「真的是 0」区分得开，
    /// 而且这一次它要一路走到线上那一格。**
    ///
    /// 〔`K-R92` 改写〕上一版最后一条断言逐字写着「线上那一格今天仍是 `false`」，
    /// 并把那个有损投影记成「已知的暗账」。**那笔账现在平了** —— 线上那一格是 `Option`。
    #[test]
    fn liveness_has_no_oracle_here_so_it_says_so_instead_of_saying_false() {
        let md = metadata_with(&[], &[]);
        let c = project_counts(&row("-home-u-p", &["s1"]), &md, "pi", &NoLivenessOracleYet);
        assert_eq!(
            c.has_live,
            Counted::Unknown(WhyUnknown::NoRemoteLivenessOracle),
            "★ 「本机答不了」要说出口"
        );
        assert_ne!(
            c.has_live,
            Counted::Known(false),
            "🔴 `false` 是一句断言：它说「这台机器上这个项目此刻没有活会话」。\n\
             而本机根本没有远端的判活真相源 —— 说不出口的话不许说。"
        );
        assert_eq!(
            c.has_live.known(),
            None,
            "🔴 `KR92D1`：这一维**要过得了线**。上一版这里是 `wire_placeholder()` ⇒ `false`，\n\
             刚算出来的「不知道」在过线那一步被自己丢掉，而丢的那处没有任何东西说话。"
        );
    }

    /// ★★ `KR92D1` 第 ① 刀 ＋ 第 ③ 刀，**打在整条路上**：一行算不出的数
    /// 走完 fan-out 之后，线上那一格拿到的是「不知道」，**不是 `0`/`false`**。
    ///
    /// # 为什么这一条与 `history.rs` 那条不是同一条
    ///
    /// `history.rs::the_three_counts_can_say_i_do_not_know` 判的是**类型装不装得下**
    /// 第三态（它直接造一个 `HistoryProject`）；本条判的是**这条路会不会在中途把它压掉** ——
    /// 09-12 那半修就正是「类型分得开、过线那一步自己丢掉」。两条缺一条都有一整类漏网。
    ///
    /// 🔴 三格**逐格断**（第 ③ 刀：只修 star/hide 不修 `has_live` ⇒ 必须红）。
    #[tokio::test]
    async fn an_unknown_count_reaches_the_wire_as_unknown_not_as_zero() {
        let md = metadata_with(&["s1"], &["s1"]);
        // 旧 daemon 那一行（`K-R83` 之前的版本）：没有会话 sid 清单 ⇒ 三个数都算不出。
        let lines = vec![row_without_ids("-home-u-p", 3).to_string()];
        let out = fanout_list_projects(&[cfg("pi")], &md, &NoLivenessOracleYet, |_| {
            let lines = lines.clone();
            async move { Ok(lines) }
        })
        .await
        .expect("一台成功");
        let wire = out.rows[0].0.clone();

        assert_eq!(
            wire.starred_count, None,
            "🔴 本件的题面：这个数**算不出来**，线上那一格却收到了一个具体的数。\n\
             那个数会被人当成「查过了，一个星标都没有」。"
        );
        assert_eq!(wire.hidden_count, None, "🔴 同上，hidden 那一格");
        assert_eq!(
            wire.has_live, None,
            "🔴 第 ③ 刀就钉在这一行：三格是一族，不许挑软的做。\n\
             star/hide 治了而 `has_live` 还压着 ⇒ 本条红。"
        );

        // 对照组：**算得出**的那一行，线上那一格必须是实打实的数（不是「一律 None」）。
        let good = vec![row("-home-u-p", &["s1"]).to_string()];
        let ok = fanout_list_projects(&[cfg("pi")], &md, &NoLivenessOracleYet, |_| {
            let good = good.clone();
            async move { Ok(good) }
        })
        .await
        .expect("一台成功");
        assert_eq!(
            (ok.rows[0].0.starred_count, ok.rows[0].0.hidden_count),
            (Some(1), Some(1)),
            "★ 对照组：算得出就要给出真值 —— 否则本条可以靠「一律说不知道」作弊过关"
        );
    }

    // ─────────────── `KR92D2`：这条路上「活没活」不许写死 ───────────────

    /// ★★ `KR92D2` 第 ② 刀：**给一个真会答话的真相源，这条路端得动真值。**
    ///
    /// 这一条同时把 `project_counts` 的汇总口径钉住：
    /// 一个确定活着 ⇒ 整行 `Known(true)`（其余答不答得出都不改这个答案）。
    #[test]
    fn a_real_oracle_makes_liveness_a_real_value_all_the_way_to_the_wire() {
        let md = metadata_with(&[], &[]);
        let r = row("-home-u-p", &["s1", "s2"]);

        let live = project_counts(&r, &md, "pi", &Oracle(&["s2"]));
        assert_eq!(
            live.has_live,
            Counted::Known(true),
            "★ s2 活着 ⇒ 这一行有活的"
        );
        assert_eq!(live.has_live.known(), Some(true), "★ 真值要过得了线");

        let dead = project_counts(&r, &md, "pi", &Oracle(&[]));
        assert_eq!(
            dead.has_live,
            Counted::Known(false),
            "★ 真相源说两个都没活 ⇒ 这是**查过了的** `false`，与「不知道」不是同一个值"
        );
        assert_eq!(dead.has_live.known(), Some(false));
        assert_ne!(
            dead.has_live.known(),
            project_counts(&r, &md, "pi", &NoLivenessOracleYet)
                .has_live
                .known(),
            "🔴 本条的牙：「查过了，没活」与「没人查过」过线之后必须不同"
        );
    }

    /// ★★ `KR92D2` 第 ③ 刀：**一行里只要有一个会话答不出，整行就不许说「没有活会话」。**
    ///
    /// 失效方向：把答不出的那几个当成「没活」跳过去 —— 那样整行退化成一个
    /// **看起来像真值**的 `false`，比明说不知道更糟（同 `ListDisagreesWithCount` 那一档的道理）。
    #[test]
    fn one_session_we_cannot_answer_for_makes_the_whole_row_unknown() {
        let md = metadata_with(&[], &[]);
        let c = project_counts(
            &row("-home-u-p", &["s1", "s2"]),
            &md,
            "pi",
            &OracleBlindTo(&["s2"]),
        );
        assert_eq!(
            c.has_live,
            Counted::Unknown(WhyUnknown::NoRemoteLivenessOracle),
            "★ s1 确定没活、s2 答不出 ⇒ 整行是**不知道**"
        );
        assert_ne!(
            c.has_live,
            Counted::Known(false),
            "🔴 退化成 `false` = 拿「查得到的那几个」编出一个整行的断言"
        );
        // 反过来：答不出的那个之外还有一个**确定活着**的 ⇒ 整行确定活着（不确定的不影响）。
        assert_eq!(
            project_counts(
                &row("-home-u-p", &["s1", "s2"]),
                &md,
                "pi",
                &Oracle(&["s1"])
            )
            .has_live,
            Counted::Known(true)
        );
    }

    /// ★★ `KR92D2` 第 ① 刀：**`stream_remote_history_sessions` 那条路上的 `is_live`
    /// 不许写死** —— 恢复 `〔R83c〕` 那处写死当场红。
    #[test]
    fn the_streamed_remote_entry_does_not_hardcode_its_liveness() {
        let md = metadata_with(&["s1"], &[]);
        let line = serde_json::json!({
            "sessionId": "s1",
            "cwd": "/home/u/p",
            "startedAtMs": 1,
            "updatedAtMs": 2,
            "jsonlPath": "/home/u/.claude/projects/p/s1.jsonl",
            "messageCountApprox": 7,
        })
        .to_string();

        let unknown = remote_session_entry(&line, "p", &md, "pi", &NoLivenessOracleYet)
            .expect("这一行是好的");
        assert_eq!(
            unknown.is_live, None,
            "🔴 09-12 之前这一格是字面量 `false`（住 `remote_history.rs::stream_remote_history_sessions` \
             那个循环里）——\n\
             本机没有远端判活的真相源，说不出口的话不许说。\n\
             ⚠ 本条判的是**性质**（这条路上有没有写死的活状态），不钉那一行的行号。"
        );

        let live =
            remote_session_entry(&line, "p", &md, "pi", &Oracle(&["s1"])).expect("这一行是好的");
        assert_eq!(live.is_live, Some(true), "★ 第 ② 刀：真值要端得动");
        let dead = remote_session_entry(&line, "p", &md, "pi", &Oracle(&[])).expect("这一行是好的");
        assert_eq!(
            dead.is_live,
            Some(false),
            "★ 「查过了，没活」也是一个真值 —— 它与 `None` 是两个不同的答案"
        );
        assert_ne!(
            dead.is_live, unknown.is_live,
            "🔴 第 ③ 刀：答不出时退化成 `false` ⇒ 这一行与「查过了没活」同形，本条红"
        );
        // 顺带钉住：这条路仍旧合本机 metadata（star 是本机的事，不受判活影响）。
        assert!(
            unknown.starred,
            "★ star 按 sid 合本机 metadata，没被本件改坏"
        );
        assert_eq!(unknown.origin.as_deref(), Some("pi"));
    }

    /// ★ 每一档「不知道」都说得出**为什么** —— 定框 `E4`：静默失败一律给身份。
    #[test]
    fn every_unknown_can_say_why() {
        for w in [
            WhyUnknown::NoSessionIdList,
            WhyUnknown::ListDisagreesWithCount,
            WhyUnknown::NoRemoteLivenessOracle,
        ] {
            let r = w.reason();
            assert!(!r.is_empty(), "{w:?} 说不出理由");
            assert!(
                !r.contains("unknown") && !r.contains("None"),
                "★ 理由要是**人话**，不是把类型名抄一遍：{r}"
            );
        }
    }

    /// ★★ **把「不知道」压成线上那个值的地方，现在一处都没有了。**
    ///
    /// 〔`K-R92` 改写；上一版的名字里写着「**只有一处**」，钉的就是那句话
    ///（那一处是 `Counted` 上那个 `wire_placeholder`，`Unknown` ⇒ `0`/`false`）。
    /// `K-R92` 的裁定是**唯一一处也是一处** ⇒ 本条改钉「**零处**」。〕
    ///
    /// 行为判据看不见这条：谁在别处再写一句 `starred_count: Some(0)`，上面那些测试用的夹具
    /// 走的是有清单那条路，照样绿。这一条钉的是**代码里有没有把这四格写成常量**。
    ///
    /// ⚠ 本条同时是 `KR92D2` 的**性质**那一半：逐字**不钉那一行的行号**（挪个位置就瞎），
    /// 钉的是「这条路上不存在写死的活状态」。
    ///
    /// 对照组自带（F23 那一族本区已犯过三次）：needle 在未剥测试段里比生产段多，
    /// 剥不掉就说明 `production_source` 没在起作用、下面在读自己。
    #[test]
    fn there_is_no_place_left_that_flattens_unknown_into_a_wire_value() {
        let raw = include_str!("remote_history.rs");
        let prod = guard_core::production_source(raw);
        // 对照组：这个名字只住在测试段里（`K-R92` 的假真相源），生产段必须一个都没有。
        const ONLY_IN_TESTS: &str = "OracleBlindTo";
        assert!(
            raw.matches(ONLY_IN_TESTS).count() > 0 && prod.matches(ONLY_IN_TESTS).count() == 0,
            "★ 对照组：剥掉测试段后 needle 应当归零（现打 raw {} / prod {}）。\n\
             没归零 ⇒ `production_source` 没在起作用，下面那几条是在读自己、恒绿。",
            raw.matches(ONLY_IN_TESTS).count(),
            prod.matches(ONLY_IN_TESTS).count()
        );
        // ⚠ **再剥一层行注释**：`production_source` 剥的是 `#[cfg(test)]` 段，注释照留。
        // 而本条要判的是「**代码里**有没有把这四格写成常量」——
        // 讲这段历史的**散文里必然出现那几个字面量**（上面 `Counted` 的头注就是），
        // 不剥的话判据会被自己的文档喂饱（反过来：为了绕开判据而不敢把历史写清楚，更糟）。
        // 本仓已有先例逐字记着这一族：`readonly_guard` 头注「护栏是子串扫描、**不剥注释**」。
        let code: String = prod
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        assert_eq!(
            code.matches("wire_placeholder").count(),
            0,
            "★ 那个压点回来了。`K-R92` 起「不知道」一路走到线上那一格（`Option`），\n\
             不再有任何一处在过线时把它说成 `0`/`false`。"
        );

        // 过线点：`Counted` ⇒ `Option` 只发生在这几处（star / hide / has_live / is_live）。
        // 多一处 = 又开了一个线上面，要说清那一格是什么、谁读它。
        assert_eq!(
            code.matches(".known()").count(),
            4,
            "★ 过线点个数变了。现打生产代码：\n{}",
            code.lines()
                .filter(|l| l.contains(".known()"))
                .collect::<Vec<_>>()
                .join("\n")
        );

        // 🔴 这四格**任何一种写死的写法**都不许回来：`0`/`false` 是 09-12 之前的原样，
        // `Some(0)`/`Some(false)` 是同一句谎话换了个类型说一遍（`K-R92` 之后才可能出现的形状）。
        for hardcoded in [
            "starred_count: 0",
            "hidden_count: 0",
            "has_live: false",
            "is_live: false",
            "starred_count: Some(0)",
            "hidden_count: Some(0)",
            "has_live: Some(false)",
            "is_live: Some(false)",
            "has_live: Some(true)",
            "is_live: Some(true)",
        ] {
            assert!(
                !code.contains(hardcoded),
                "🔴 生产代码里又出现了 `{hardcoded}` —— 这条路上没有判活的真相源、\n\
                 也不该把 star/hide 写成常量。写死的活状态是一句没人查过的断言。"
            );
        }
    }

    // ───────────────────────────── `KR83D3` ─────────────────────────────

    /// ★★ `KR83D3`：**算这三个数的路径上，进程 spawn 次数不随项目数增长。**
    ///
    /// 判的是**「一次调用里 spawn 了几次」这个可数的事实** —— 逐字**不判**
    /// 「代码里有没有 for 循环」（那判的是写法不是复杂度）。
    ///
    /// 做法：把「去问远端」作为参数传进 [`fanout_list_projects`]，喂一个会计数的假查询。
    /// 一旦有人为了拿 star/hide 而在每个项目上补一句 `--list-sessions`，
    /// 计数当场从 `台数` 涨成 `台数 + 项目数`，本条红。
    #[tokio::test]
    async fn the_number_of_remote_execs_does_not_grow_with_the_number_of_projects() {
        let md = metadata_with(&["p7-s0"], &[]);

        async fn run_with(n_projects: usize, md: &HistoryMetadata) -> (usize, usize) {
            let lines: Vec<String> = (0..n_projects)
                .map(|i| row(&format!("p{i}"), &[&format!("p{i}-s0")]).to_string())
                .collect();
            let calls = std::cell::Cell::new(0usize);
            let out = fanout_list_projects(&[cfg("pi")], md, &NoLivenessOracleYet, |_| {
                calls.set(calls.get() + 1);
                let lines = lines.clone();
                async move { Ok(lines) }
            })
            .await
            .expect("一台成功");
            (calls.get(), out.rows.len())
        }

        let (few_calls, few_rows) = run_with(3, &md).await;
        let (many_calls, many_rows) = run_with(200, &md).await;

        assert_eq!((few_rows, many_rows), (3, 200), "夹具本身要真的变多");
        assert_eq!(
            few_calls, 1,
            "★ 1 台 3 个项目 ⇒ **1 次**远端 exec（现打 {few_calls}）"
        );
        assert_eq!(
            many_calls, 1,
            "🔴 1 台 200 个项目 ⇒ 仍然**1 次**（现打 {many_calls}）。\n\
             变成 201 = 有人给每个项目补了一次 `--list-sessions` ——\n\
             那是 N 次进程 spawn，而这是用户常开的界面（失效方向逐字记在\n\
             `local_read_surface_registry.rs` 那条退役条件里）。"
        );
    }

    /// ★ 多台时的口径：spawn 次数 = **台数**，与项目数无关（不是「恒为 1」）。
    #[tokio::test]
    async fn the_number_of_remote_execs_equals_the_number_of_hosts() {
        let md = metadata_with(&[], &[]);
        let calls = std::cell::Cell::new(0usize);
        let hosts = [cfg("pi"), cfg("nas"), cfg("box")];
        let out = fanout_list_projects(&hosts, &md, &NoLivenessOracleYet, |_| {
            calls.set(calls.get() + 1);
            async move {
                Ok((0..50)
                    .map(|i| row(&format!("p{i}"), &[&format!("p{i}-s0")]).to_string())
                    .collect())
            }
        })
        .await
        .expect("三台都成功");
        assert_eq!(out.rows.len(), 150, "3 台 × 50 个项目");
        assert_eq!(
            calls.get(),
            hosts.len(),
            "★ 3 台 ⇒ 3 次；150 个项目一次都不额外加"
        );
    }

    /// 逐台失败仍旧隔离（F76 的老性质，本件重构了这条路 ⇒ 顺手钉住没被改坏）。
    #[tokio::test]
    async fn a_failing_host_is_still_skipped_and_reported_not_fatal() {
        let md = metadata_with(&[], &[]);
        let out =
            fanout_list_projects(&[cfg("good"), cfg("bad")], &md, &NoLivenessOracleYet, |c| {
                let bad = c.origin_label() == "bad";
                async move {
                    if bad {
                        Err("连不上".to_string())
                    } else {
                        Ok(vec![row("p", &["s1"]).to_string()])
                    }
                }
            })
            .await
            .expect("有一台成功 ⇒ 整体 Ok");
        assert_eq!(out.failed_hosts, vec!["bad".to_string()]);
        assert_eq!(out.rows.len(), 1);
    }
}
