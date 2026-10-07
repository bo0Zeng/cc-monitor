//! `P4f`：cc-bus 的**基础命令** —— `bus-list`（谁在线 + 各自待读数）与 `bus-send`（发一条）。
//!
//! 〔用 08-13〕逐字：「**也是后端先把基础命令做了, 其他具体的细节先按原本的就行,
//! 后面我可能要改ccbus**」。三条判断都是从这句话推出来的，读数在账本 `P4f §3`。
//!
//! # ① 第一刀**不做** `bus-recv`
//!
//! `cc-recv` **有副作用**：它会推进已读位置。backend 代读 = **把消息从人那里偷走** ——
//! agent 自己再跑 `cc-recv` 就什么都看不到了。（08-13 刚修过同一族的一条真事故：
//! Stop 钩子把 40 条积压「读过了却没喂回去」，账本 `P4b §7g-9k`。）
//!
//! ⇒ 消费性操作第一刀不做；「有没有新的」这个需求由 `bus-list` 的**待读数**满足
//! （只读、不消费）。要做代读的那天，先得给它一个「读了但不算数」的两阶段口。
//!
//! # ② 转调脚本，**不在这里重实现**总线
//!
//! 用户逐字「细节先按原本的就行」+「后面我可能要改ccbus」⇒ 在后端里重写一份
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
//! 名册只读 `cc-list --tsv` 那一形（首尾两行有标记，行形状归 cc-bus、id 形状本侧判）⇒ 一个解析器（[`parse_roster_tsv`]）。
//!
//! 这条由 [`tests::no_cc_bus_data_layout_leaks_into_the_backend`] 钉住。
//!
//! # ④ 找它 / 起它这套壳**搬走了**，本模块只留 cc-bus 自己的语义〔`K-W1A`，08-26〕
//!
//! 「按候选顺序找可执行文件」「argv 直传起进程、期限走 `timeout` 前缀」这两段是
//! **任何插件都一样**的形状，已经搬进 [`crate::plugin`]。本模块从此只提供**它自己**的那几样：
//! 候选路径（cc-bus 装在哪儿）· 期限从哪个环境变量读 · **退出码 → 语义码的映射表**。
//!
//! ⚠ 映射表**刻意不上收**：同一个码在不同插件里语义互斥
//!（`3` 在 cc-bus 是「路由层拒绝」，在另一个真实插件里是「撞名、该重试」）——
//! 把它抽进通用层就等于让宿主替所有插件解释退出码，那正是 `E6` 禁的事。
//!
//! # 只读铁律
//!
//! 起进程那一处已经搬去 [`crate::plugin::invoke`]（登记也跟着搬了，`readonly_guard::ALLOWED`
//! 里那一行的文件名同轮改掉 —— 键是 `(文件, 程序名)`，不改会当场红）。
//! 本模块经那一处转调的外部命令**今天是四条**〔`K-R113` 09-13 补第四条〕，逐条说清写面：
//! · `cc-list` **只读**；
//! · `cc-agents` **只读**（`K-R113` 新增：spawn 台账那一半 —— 它只读两份名册再问一次 tmux）；
//! · `cc-send` 会写收件人的收件箱；
//! · ★ `cc-kill` 是**破坏性**的 —— 它杀会话，还要清名册、清台账、清那个 id 的状态。
//!
//! ⚠ **别照这段散文数** —— 「今天转调哪几条」的唯一住址是
//! [`tests::TRANSCALLS`]（从生产段现算之后与它对拍）。这一段只讲写面。
//!
//! 四条都是**被起的那个进程**在写，与用户自己在终端里敲同一条命令没有区别
//!（同 `launch` 起 claude 的 D1 正例：收窄后的铁律管的是 **backend 进程自身**不写用户既有数据）。
//!
//! ⚠⚠ 这段话此前逐字写着「**两条命令**共用」，而 `cc-kill` 是 08-13 当天稍晚进来的
//!（那个 commit 动了 8 个文件，`readonly_guard.rs` 不在其中）⇒ 那条豁免理由**漏掉了今天真实的写面**，
//! 而三条判据全绿 —— 起进程登记的键里程序名是 `<非字面量>`，三条命令**共用同一个键**，
//! 加第三条不会红。这一句与 `ALLOWED` 那一行的理由**同轮一起订正**。
//!
//! ⚠ **这一次加第四条是红着加进来的**，而红它的不是上面那条登记：
//! `readonly_guard::g6_reach::the_non_literal_spawn_key_still_covers_exactly_three_commands`
//! 把「今天是三条」钉成**相等** ⇒ 加 `cc-agents` 当场红，逼人回去重读那条豁免理由。
//! 那一格与本文件的 [`tests::TRANSCALLS`] **不是同一条规矩的两处住址**：
//! 前者钉「只读铁律的豁免理由覆盖了哪几条」，后者钉「`K33`『不要 bash 脚本』对每一条各裁了什么」。

use copy_core::copy_text;
use std::path::{Path, PathBuf};

use crate::platform::child::Deadline;
use crate::plugin::invoke::Done;
use crate::plugin::invoke::NotRun;
use crate::plugin::invoke::TIMED_OUT_CODE;

/// `bus-list` 整条命令总期限的上限（`cc-list` ＋ 列 tmux 会话）。
pub(crate) const BUS_LIST_CAP: Deadline = Deadline::secs(13);
/// `bus-state` 整条命令总期限的上限（`cc-list` ＋ `cc-agents` ＋ 列 tmux 会话）。
pub(crate) const BUS_STATE_CAP: Deadline = Deadline::secs(28);
/// `bus-send` 整条命令总期限的上限（`cc-send` ＋ 问收件人在不在：`cc-list` ＋ 列 tmux 会话）。
pub(crate) const BUS_SEND_CAP: Deadline = Deadline::secs(28);
/// `bus-broadcast` 整条命令总期限的上限（`cc-list` ＋ 列 tmux 会话 ＋ 每个收件人一发 `cc-send`，整批共用；用完了剩下的各自进 `failed`）。
pub(crate) const BUS_BROADCAST_CAP: Deadline = Deadline::secs(28);

/// 找不到时那句话的**尾巴** —— 这是 cc-bus 自己的话，通用层不该认识它。
static NOT_INSTALLED_HINT: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beCcBus.find.notInstalled", &[]));

/// 命令级错误：`(code, message)`。与 [`super::kill`] / [`super::launch`] 同型。
type CmdErr = (&'static str, String);

/// 固定位置的查找顺序 —— **纯函数**（不读环境变量，好测）。
///
/// `CC_BUS_BIN_DIR`（部署/台架覆盖）→ `~/.local/bin` → `~/.claude/skills/cc-bus/scripts`。
///
/// ⚠ 为什么不只靠 `PATH`：backend 由 app 经 **SSH exec** 起，那是**非登录 shell**，
/// `~/.local/bin` 未必在 `PATH` 里（08-13 实测）⇒ 只靠 PATH 会出现「明明装了却找不到」。
/// 这条与 `ccm` 的 `BACKEND_BIN_RECIPE` 是同一个形状（那边找后端，这边找 cc-bus），
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
        out.push(
            h.join(".claude")
                .join("skills")
                .join("cc-bus")
                .join("scripts")
                .join(name),
        );
    }
    out
}

/// 找它 —— 候选顺序是 cc-bus 自己的，找法走通用口。
///
/// `P4f-Y4`：`~/.local/bin` 不在非登录 shell 的 PATH 里是**常态**，
/// 所以"没装/找不到"必须是一个**能自证的**回答，不能report成笼统的失败 ——
/// 那句话由通用口拼（它知道查过哪几处），本模块只给它 cc-bus 自己的那句尾巴。
fn find(name: &str) -> Result<PathBuf, CmdErr> {
    let override_dir = std::env::var_os("CC_BUS_BIN_DIR").filter(|d| !d.is_empty());
    let home = crate::platform::paths::home_dir();
    let fixed = fixed_candidates(override_dir.as_ref().map(Path::new), home.as_deref(), name);
    crate::plugin::discover::find(name, &fixed, true, &NOT_INSTALLED_HINT)
        .map_err(|msg| ("not_installed", msg))
}

/// 给子进程的**期限（秒）**。台架用 `CC_BUS_TIMEOUT_SECS` 调小。
fn timeout_secs() -> u64 {
    std::env::var("CC_BUS_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(10)
}

/// 转调一条 cc-bus 命令。起进程那一处住 [`crate::plugin::invoke`]。
///
/// argv 直传、**不过 shell** ⇒ 收件人/正文里的元字符不构成注入面。
///
/// # ★★ 期限住在**子进程**里，不在后端里〔08-13 实测事故 + 铁律冲突〕
///
/// 病先说清楚：`Command::output()` **无限等**。实测把 `cc-send` 换成 `sleep 300` 的桩，
/// `--bus-send` 25 秒都没回来（25 是我从外面掐的，backend 自己没有任何期限）。
/// 这不是假想 —— `cc-send` 的投递走 `flock`，**锁被别人占住就一直等**；
/// 而这两条命令是阻塞档，一条卡住就占死一个 tokio worker，且 `cancel` 对 `spawn_blocking`
/// 是**空操作**（`inbound` 那条判据逐字：「`cancel` 会对它撒谎」）。
///
/// ⚠ 我的第一版是在后端里等（先 `try_wait` 轮询、后 `recv_timeout`）——
/// **两版都被零定时器护栏当场逮住**，而它是对的：`IPC-PROTOCOL` 自己写着
/// 「backend 侧刻意不管超时…零定时器铁律不改，**超时一律推给客户端**」。
///
/// ⇒ 正确形状是**让子进程自己有期限**：能找到 `timeout(1)` 就用它当前缀
///（`ccm` 里问后端那条早就是这么写的，同一条纪律的另一侧）。
/// backend 这边仍然只是老老实实 `wait` 一个**注定会退出**的子进程 —— 零计时器。
///
/// ⚠ 找不到 `timeout(1)` 就**如实降级**：裸跑、没有期限。
/// 那种机器上这条路会退回「可能卡住」，判据与文档都照实写（不假装有保障）。
fn run(name: &str, args: &[&str]) -> Result<Done, CmdErr> {
    run_as(name, args, None)
}

/// 同 [`run`]，但可以指定**以谁的身份**跑（`CC_BUS_ID`）。
///
/// # 为什么是环境变量而不是给 cc-send 加参数
///
/// `cc-whoami` 的优先级第一条逐字就是 `$CC_BUS_ID`（可选覆盖）——**这是 cc-bus 现成的契约**。
/// 用户 08-13 逐字「细节先按原本的就行」⇒ 不动 cc-bus 本体。
///
/// # ★★ 这一族环境键从此**由本模块显式交办**，不再靠继承
///
/// `plugin::invoke::run` 今天先 `env_clear()`、再按它自己那张白名单
/// （`plugin::invoke::INHERITED_ENV_KEYS`）喂 —— 那一刀关掉的是「backend 进程内部的秘密
/// （常驻监听口的地址与令牌）顺着环境漏进插件」这条没人设计过的回程。
///
/// ⚠ **代价落在这里**：本插件族此前是靠**继承**拿到自己的配置的
/// （`CC_BUS_HOME` · `CCBUS_POLICY_MODE` · 不带 `from` 那一趟的 `CC_BUS_ID`），
/// 而本模块**一个都没显式交办过**。现打读数：只加 `env_clear()` 那一刀之后
/// `tests/e2e/backend-cc-bus.sh` 从 `PASS=50 FAIL=0` 掉到 `PASS=31 FAIL=19`，
/// 其中最重的一格是 `[15]`：`CC_BUS_HOME` 一没，`cc-kill` 就照着一份**空台账**
/// 判「这个名字还是原来那个人吗」，判成「是」，**把同名的无辜会话连进程一起杀了**。
///
/// # 🔴 修法**不许**是「往那张白名单里加键」
///
/// 那等于让通用调用口认识一个具体插件 —— 而
/// `plugin::layer_guard::the_generic_port_names_no_concrete_plugin` 的针里
/// **逐字就有这个前缀**（那张表里那一条注着「某个插件的环境变量前缀」）。
/// ⇒ **谁的插件，谁交办**：这一族键的家只能是本模块。
///
/// 按**前缀**而不是逐个列名：这是 cc-bus 自己的命名空间，而它是一族**会长**的东西
/// （`CCBUS_RATE_*` · `CCBUS_DEDUP_WINDOW` · `CCBUS_TTL` · `CCBUS_NUDGE_DEBOUNCE` …），
/// 逐个列名今天就会漏，明天更会漏。
/// ⚠ 诚实边界：前缀式交办把**本进程环境里**带这两个前缀的键**全部**交给子进程 ——
/// 它守的是「不再多给」（宿主的秘密不在这个命名空间里），**不是**「按需最小授权」。
/// 真要收到按键最小化，那是给这一族插件立一份「它认哪几个键」的清单，另一件。
fn run_as(name: &str, args: &[&str], as_id: Option<&str>) -> Result<Done, CmdErr> {
    let bin = find(name)?;
    let secs = timeout_secs();
    let owned: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| k.starts_with("CC_BUS_") || k.starts_with("CCBUS_"))
        .collect();
    let mut env: Vec<(&str, &str)> = owned
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    // `CC_BUS_ID` 是 cc-bus 自己的契约 ⇒ 由本模块拼，通用口只负责把 env 传下去。
    // 显式指定身份时它**压过**上面顺下来的那一份（`retain` 那一句就是这个意思，
    // 少了它 `invoke::run` 会看到同一个键两次，而「后一个赢」是它的实现细节、不是契约）。
    if let Some(id) = as_id {
        env.retain(|(k, _)| *k != "CC_BUS_ID");
        env.push(("CC_BUS_ID", id));
    }
    crate::plugin::invoke::run(&bin, args, secs, &env).map_err(|e| match e {
        // ★ **E2BIG 要单独说**〔08-13 实测〕：`{"code":"failed","message":"起不来 cc-send：
        //   Argument list too long"}` 有两处不对 —— ① `failed` 是兜底桶，调用方分不出
        //   「我的消息太长」（自己能修：发短点）和「cc-bus 坏了」（自己修不了）；
        //   ② 那句话**归错了因**：cc-send 好好的，是这条消息塞不进 argv。
        //   实测 200KB 正文必炸、120KB 能过 —— 内核的单参数上限 `MAX_ARG_STRLEN` = 128 KiB
        //  （`P4b §7g-8b` 量过：131000 OK / 131072 E2BIG）。
        // ⚠ **那句「不是 cc-bus 坏了」留在本模块**：通用口不知道被调的是谁，
        //   由它来说这句话只能说成「不是那个程序坏了」，而 e2e 逐字核的是前者。
        NotRun::ArgListTooLong => (
            "too_long",
            copy_text("beCcBus.notRun.tooLong", &[]).to_string(),
        ),
        NotRun::Failed(msg) => ("failed", msg),
    })
}

/// 超时那条的说法 —— 几个命令共用一份文案。`secs` ＝ 实际等了多久（[`waited_secs`]）。
fn timed_out_err(secs: u64) -> (String, String) {
    (
        "timed_out".to_string(),
        copy_text("beCcBus.timedOut.say", &[("secs", &secs.to_string())]),
    )
}

/// 这一趟实际等了多久：过了期限被收掉的 ⇒ 原语交回的那个数（被命令总期限截短时就是截短后的）；
/// 码是 124 却不是我们收的（它自己这么退的）⇒ 给它的那个期限。
pub(crate) fn waited_secs(out: &Done) -> u64 {
    out.waited_secs.unwrap_or_else(timeout_secs)
}

/// 机器可读形的首行标记（`cc-list --tsv` / `cc-agents --tsv` / `cc-log`）。老 cc-bus 不认 `--tsv`、
/// 照打人读表 ⇒ 见不到标记就明说「cc-bus 比后端旧」，不猜着按人读表解（命令是接口，接口认不出就说）。
const ROSTER_TSV_HEAD: &str = "#cc-list-tsv\t1";
const SPAWNED_TSV_HEAD: &str = "#cc-agents-tsv\t1";
const LOG_HEAD: &str = "#cc-log\t1";
/// 两张表的末行：`#skipped<TAB><坏行数>`。缺它 = 只读到半份。
const SKIPPED_TAIL: &str = "#skipped\t";

fn too_old(cmd: &str) -> (String, String) {
    (
        "failed".to_string(),
        copy_text("beCcBus.read.tooOld", &[("cmd", &cmd.to_string())]),
    )
}

/// 切出首尾两行之间的数据行 ＋ 末行的坏行数 —— 纯函数。首行不对 ⇒ 老 cc-bus；末行缺 ⇒ 半份，回错、不当完整的用。
fn framed<'a>(
    cmd: &str,
    text: &'a str,
    head: &str,
) -> Result<(Vec<&'a str>, usize), (String, String)> {
    let mut lines: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    if lines.first() != Some(&head) {
        return Err(too_old(cmd));
    }
    let skipped = (lines.len() >= 2)
        .then(|| lines[lines.len() - 1])
        .and_then(|l| l.strip_prefix(SKIPPED_TAIL))
        .and_then(|n| n.parse::<usize>().ok())
        .ok_or_else(|| {
            (
                "failed".to_string(),
                copy_text("beCcBus.read.truncated", &[("cmd", &cmd.to_string())]),
            )
        })?;
    Ok((lines[1..lines.len() - 1].to_vec(), skipped))
}

/// 名册一行（`cc-list --tsv`）：第 4 列 pane pid 是登记那一刻 pane 根进程的 pid（老格式为空）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RosterRow {
    pub(crate) id: String,
    pub(crate) target: String,
    pub(crate) registered_at: String,
    pub(crate) pane_pid: Option<u32>,
    pub(crate) unread: i64,
}

/// `cc-list --tsv` → `(名册, 坏行数)` —— 纯函数。行的形状归 cc-bus（恰好 5 格）；id 的形状由本侧判
/// （`INVARIANTS §47`，唯一一份 `bus_id_ok`），过不了的算坏行（`--help` 那种 cc-bus 自己的字符集放得过）。
pub(crate) fn parse_roster_tsv(text: &str) -> Result<(Vec<RosterRow>, usize), (String, String)> {
    let (rows, mut skipped) = framed("cc-list --tsv", text, ROSTER_TSV_HEAD)?;
    let mut out = Vec::new();
    for l in rows {
        let f: Vec<&str> = l.split('\t').collect();
        let unread = f.get(4).and_then(|u| u.parse::<i64>().ok());
        match unread {
            Some(unread) if f.len() == 5 && shell_quote_core::bus_id_ok(f[0]) => {
                out.push(RosterRow {
                    id: f[0].to_string(),
                    target: f[1].to_string(),
                    registered_at: f[2].to_string(),
                    pane_pid: f[3].parse().ok(),
                    unread,
                })
            }
            _ => skipped += 1,
        }
    }
    Ok((out, skipped))
}

/// `cc-agents --tsv` → `(台账, 坏行数)` —— 纯函数。状态机器词 → `bus-list` 同一套三态（`null` = 核不了，不是不在）。
pub(crate) fn parse_spawned_tsv(
    text: &str,
) -> Result<(Vec<serde_json::Value>, usize), (String, String)> {
    let (rows, mut skipped) = framed("cc-agents --tsv", text, SPAWNED_TSV_HEAD)?;
    let mut out = Vec::new();
    for l in rows {
        let f: Vec<&str> = l.splitn(5, '\t').collect();
        let live = match f.get(1) {
            Some(&"live") => Some(serde_json::Value::Bool(true)),
            Some(&"exited") => Some(serde_json::Value::Bool(false)),
            Some(&"unknown") => Some(serde_json::Value::Null),
            _ => None,
        };
        match live {
            Some(live) if f.len() == 5 && shell_quote_core::bus_id_ok(f[0]) => {
                out.push(serde_json::json!({
                    "id": f[0], "dir": f[2], "spawned_at": f[3], "task": f[4], "live": live
                }))
            }
            _ => skipped += 1,
        }
    }
    Ok((out, skipped))
}

/// `cc-send` 的退出码 → **语义码** —— 纯函数。
///
/// `P4f-Y5`：如实转达，**不在后端侧再写一份收件人白名单**。
/// 收件人合法性归 cc-bus 自己（它已有，rc=2）；这边再写一份的话两处规则会漂。
///
/// # ★ 这张表**刻意住在这里**，不许上收到 `plugin/`（`E6`）
///
/// 理由是读数不是风格：同一个 `3` 在这个插件是「路由层拒绝」、在另一个插件是
/// 「撞名该重试」—— **语义互斥**。抬进通用调用口就等于让宿主替所有插件解释退出码。
///
/// ⚠ 这句话此前**只是散文**：D1（08-26）实测把本函数原样抬进 `plugin/invoke.rs`、
/// 只擦掉 `cc-send` 这个名字 ⇒ `436 passed; 0 failed`，**一条不红**。
/// 现在钉着它的是 `plugin::layer_guard` 里形状那一族两条判据
///（`the_generic_port_does_not_translate_exit_codes` /
/// `the_only_exit_code_constants_here_are_the_registered_generic_ones`）——
/// 它们认的是**形状**、不认插件的名字，所以第二个插件的码表抬上去也会红。
/// 各自认不出什么，写在它们自己的头注里。
pub(crate) fn classify_send(
    code: Option<i32>,
    detail: &str,
    waited: u64,
) -> Result<(), (String, String)> {
    match code {
        Some(0) => Ok(()),
        // cc-send 自己的白名单校验（`仅 [A-Za-z0-9_-]`）
        Some(2) => Err((
            "bad_args".to_string(),
            copy_text("beCcBus.send.refused", &[("detail", &detail.to_string())]),
        )),
        // 路由层拦截（ACL / 限流 / 去重 / 灭环），bus.log 里有 REJECT/THROTTLE 一行
        Some(3) => Err((
            "rejected".to_string(),
            copy_text("beCcBus.send.rejected", &[("detail", &detail.to_string())]),
        )),
        Some(TIMED_OUT_CODE) => Err(timed_out_err(waited)),
        Some(c) => Err((
            "failed".to_string(),
            copy_text(
                "beCcBus.send.failed",
                &[("status", &c.to_string()), ("detail", &detail.to_string())],
            ),
        )),
        None => Err((
            "failed".to_string(),
            copy_text(
                "beCcBus.send.interrupted",
                &[("detail", &detail.to_string())],
            ),
        )),
    }
}

/// 〔`INVARIANTS §47` ①〕agent id / 账号名在交给 `cc-send` / `cc-kill` / `cc-spawn` **之前**先过形状判定
/// （`shell_quote_core::bus_id_ok`：非空 · 不以 `-` 开头 · 只含 `[A-Za-z0-9_-]`，全仓唯一一份）。判不过 ⇒ `bad_id`，一个进程都不起。
///
/// 为什么在这里判、而不是「交给 cc-bus 自己去拒」：`§47` 逐字「**『对端会校验』不是理由**」—— 收掉一个 agent 的后果是杀一棵进程树；
/// 界面那一道（C4e 第四次搬家住在 `src/frontend/ui/cc-bus-control.ts`）按删了，「本侧」从此是真把 id 交出去的这一侧（第五次搬家）。
/// ⚠ 判的是**形状**，不是**成员资格**：「这个名字登没登记过」仍归 cc-bus（`registered` 那一格照旧如实回）。
/// `said` 收那个值的 `{:?}` 形、给出那一句（文案走表：key 在各调用处写字面量，`copy-table.vitest.ts` 按调用点两向对拍）。
fn refuse_bad_bus_id(v: &str, said: impl FnOnce(&str) -> String) -> Result<(), CmdErr> {
    if shell_quote_core::bus_id_ok(v) {
        return Ok(());
    }
    Err(("bad_id", said(&format!("{v:?}"))))
}

/// 形状校验：这组参数能不能构成一次有意义的调用。
///
/// ⚠ 与 `kill::parse_name` 同一条纪律：argv 直传不过 shell。`to` 的**形状**在这里判（[`refuse_bad_bus_id`]，§47 ①）；
/// 收件人**是否存在**（成员资格）仍归 cc-bus —— 见 [`classify_send`]。
fn parse_send(args: &serde_json::Value) -> Result<(String, String, Option<String>), CmdErr> {
    let obj = args.as_object().ok_or((
        "bad_args",
        crate::common::contract::malformed("args must be an object"),
    ))?;
    let to = obj.get("to").and_then(|v| v.as_str()).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `to`"),
    ))?;
    let text = obj.get("text").and_then(|v| v.as_str()).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `text`"),
    ))?;
    if to.trim().is_empty() {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`to` is empty"),
        ));
    }
    refuse_bad_bus_id(to, |v| {
        copy_core::copy_text("beCcBus.parse.badRecipient", &[("id", v)])
    })?;
    // ★ `from` 可选：**不给就是今天的行为**（cc-whoami 在后端的处境里解不出身份 ⇒ `unknown`）。
    //   给了就以那个身份发 —— 收信人才知道是谁，回复才有地方去。
    //   ⚠ 合法性仍归 cc-bus（`cc-whoami` 自己会消毒成 `[A-Za-z0-9_-]`），这里只判形状。
    let from = given_sender(obj)?;
    Ok((to.to_string(), text.to_string(), from))
}

/// `from`（以谁的身份发）：可选；给了就在交给 `cc-send`（作 `CC_BUS_ID`）**之前**先过同一个形状判定（[`refuse_bad_bus_id`]）。
///
/// 〔`INVARIANTS §47`「交给对端之前本侧先判」〕先前 `from` 原样交给 `cc-send`、一格都不判 ——
/// 它与收件人是同一种值（agent id），同样是交给对端去寻址的。`bus-send` 与 `bus-broadcast` 共用这一处。
fn given_sender(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Result<Option<String>, CmdErr> {
    let from = obj
        .get("from")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    if let Some(f) = from {
        refuse_bad_bus_id(f, |v| {
            copy_core::copy_text("beCcBus.parse.badSender", &[("id", v)])
        })?;
    }
    Ok(from.map(|s| s.to_string()))
}

/// 从总线地址（`proj_cc:0.0`）里取**会话名**那一段 —— 纯函数。
///
/// 身份空间的键是**会话名**；总线地址是 `名字:窗口.面板`。对账按前者。
pub(crate) fn session_name_of(target: &str) -> &str {
    target.split(':').next().unwrap_or(target)
}

/// 把总线成员**挂到身份空间上** —— 纯函数（真身份由调用方查好传进来）。
///
/// # 用户提的那条架构点〔用@08-13〕
///
/// 逐字：「**那他不应该是身份空间的子集吗? 他应该去调用身份空间啊**」。**对的。**
///
/// `agents.tsv` 是 cc-bus 的**第二套名单**，它的地址会过期（会话名被重用是常态）。
/// 08-13 实测过后果：敲门文字打进**陌生占用者**的屏幕。
/// ⇒ 这里不再复述那份名单，而是**对账**：
/// · `live` = 这个成员的会话名在**活着的 tmux 会话**里找得到吗（真身份说了算）；
/// · `ccm_sid` = 那个会话绑的 Claude sid（`@ccm_sid`），空串 ⇒ 回 `null`，不猜。
///
/// ⚠ 拿不到身份空间（没装 tmux / 起不来）⇒ `live` 回 `null` 而不是 `false`：
/// **「不知道」和「不在」是两件事**，混起来会让调用方把一屋子活人当成死人。
pub(crate) fn join_identity(
    agents: Vec<serde_json::Value>,
    sessions: Option<&[(String, String)]>,
) -> Vec<serde_json::Value> {
    agents
        .into_iter()
        .map(|mut a| {
            let target = a
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let name = session_name_of(&target).to_string();
            let (live, sid) = match sessions {
                None => (serde_json::Value::Null, serde_json::Value::Null),
                // `K-R96`：`gate::list_sessions` 今天回的是**二元组**（会话名 ＋ `@ccm_sid`）——
                // 中间那列 `#{session_id}` 本函数从来没问过，快照那边也不再取它。
                Some(list) => match list.iter().find(|(n, _)| *n == name) {
                    None => (serde_json::Value::Bool(false), serde_json::Value::Null),
                    Some((_, ccm_sid)) => (
                        serde_json::Value::Bool(true),
                        if ccm_sid.is_empty() {
                            serde_json::Value::Null
                        } else {
                            serde_json::Value::String(ccm_sid.clone())
                        },
                    ),
                },
            };
            if let Some(o) = a.as_object_mut() {
                o.insert("live".to_string(), live);
                o.insert("ccm_sid".to_string(), sid);
            }
            a
        })
        .collect()
}

/// 总线名单 —— `bus-list` 与 `bus-broadcast` 读它。与 `bus-state` 同读机器可读形（[`roster`]，
/// `cc-list --tsv`）：名册只有一个解析器。没重部署 cc-bus 的机器上老脚本不认 `--tsv` ⇒ 明说「cc-bus 比后端旧」（[`too_old`]），
/// 不退回去按人读表猜（命令是接口，接口认不出就说）。
fn agents_via_cc_list() -> Result<Vec<serde_json::Value>, (String, String)> {
    let (rows, skipped) = roster()?;
    if skipped > 0 {
        tracing::warn!("cc-bus 名册里有 {skipped} 行读不懂，这一趟没算进去");
    }
    // ★ 去问**身份空间**谁还活着（用户 08-13 那条架构点）。问不到就回 `null`，不假装知道。
    let sessions = super::gate::list_sessions().ok();
    Ok(join_identity(roster_agents(&rows), sessions.as_deref()))
}

/// 名册行 → `bus-list` 那一格的 `{id, target, unread}`（纯函数；`live` / `ccm_sid` 由 [`join_identity`] 补）。
pub(crate) fn roster_agents(rows: &[RosterRow]) -> Vec<serde_json::Value> {
    rows.iter()
        .map(|r| serde_json::json!({ "id": r.id, "target": r.target, "unread": r.unread }))
        .collect()
}

/// 转调一条只读的 cc-bus 命令、要求它 0 退出，交回 stdout —— `--tsv` 两条与 `cc-log` 共用。
fn read_via(name: &str, args: &[&str]) -> Result<String, (String, String)> {
    let out = run(name, args).map_err(|(c, m)| (c.to_string(), m))?;
    if out.timed_out() {
        return Err(timed_out_err(waited_secs(&out)));
    }
    match out.code {
        Some(0) => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Some(2) => Err((
            "bad_args".to_string(),
            copy_text(
                "beCcBus.run.refused",
                &[("name", &name.to_string()), ("detail", &out.diagnosis())],
            ),
        )),
        c => Err((
            "failed".to_string(),
            copy_text(
                "beCcBus.run.failed",
                &[
                    ("name", &name.to_string()),
                    ("status", &format!("{c:?}")),
                    ("detail", &out.diagnosis()),
                ],
            ),
        )),
    }
}

/// 名册（机器可读形）：`bus-state` 与杀会话顺手注销（D-g）读它。
pub(crate) fn roster() -> Result<(Vec<RosterRow>, usize), (String, String)> {
    parse_roster_tsv(&read_via("cc-list", &["--tsv"])?)
}

/// spawn 台账那一半 —— 转调 `cc-agents --tsv`。
fn spawned_via_cc_agents() -> Result<(Vec<serde_json::Value>, usize), (String, String)> {
    parse_spawned_tsv(&read_via("cc-agents", &["--tsv"])?)
}

pub(crate) fn list_for_inbound() -> Result<serde_json::Value, (String, String)> {
    Ok(list_reply(agents_via_cc_list()?))
}

/// `bus-list` 的成品 `{agents}` —— 从 [`list_for_inbound`] 里原样抽出来（逻辑不动），
/// 只为让跨语言金样 `tests/__fixtures__/cc-bus-control.golden.json` 拿**同一个**构造器对拍：
/// 界面（`src/frontend/ui/cc-bus-control.ts`）从此直接收这份成品，monitor 那一跳只搬字节。
pub(crate) fn list_reply(agents: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({ "agents": agents })
}

/// `bus-state`：**一次回全** —— 总线名单 ＋ spawn 台账。
///
/// # 为什么它是一条**具名读命令**，而不是让调用方读文件
///
/// monitor 侧驾驶舱读名册那条 `read_cc_bus_state`〔散文墓碑〕当年走的是一条 shell 串（两次 `cat`
/// 拼一个分隔标记），它的头注逐字写着解锁条件是「**格式契约稳下来**」，届时
/// 「正确形状多半**不是**把 shell 串搬过去，而是后端出一条**具名的读命令**」。
/// **本命令就是那一条。** 而「具名」的实质是：契约面从 *cc-bus 的文件布局*
/// 换成了 *后端的一条命令* —— 后者变的时候有人接得住（`REGISTRY` / 协议文档 / `BUILD_ID`），
/// 前者变的时候只会静悄悄地错。
///
/// # 为什么不是「让调用方发两条命令自己拼」
///
/// 两条命令 = 两个时刻。总线名单与 spawn 台账**互相引用**（`cc-agents` 判活时要去
/// `agents.tsv` 借 pid），两个时刻取的两份在客户端拼起来，会拼出一份**盘上从没存在过**的状态。
///
/// # ⚠ 「一次回全」的另一半：**要么两半都答，要么明说失败**
///
/// 任何一半失败都回错误，**不回一份看上去完整的半份**。
/// 半份的失效是静默的：`spawned: []` 与「问不到」在调用方那里长得一模一样。
///
/// 登记时间 · 派生时间 · 坏行数由 cc-bus 新加的机器可读形（`cc-list --tsv` / `cc-agents --tsv`）答，
/// 仍不读那两份 `.tsv`（`cc_bus_boundary_guard`）；驾驶舱经通道直接问本条（monitor 那条 shell 读退役）。
pub(crate) fn state_for_inbound() -> Result<serde_json::Value, (String, String)> {
    let (agents, sk_agents) = roster()?;
    let (spawned, sk_spawned) = spawned_via_cc_agents()?;
    let sessions = super::gate::list_sessions().ok();
    Ok(state_reply(
        agents,
        spawned,
        sk_agents + sk_spawned,
        sessions.as_deref(),
    ))
}

/// `bus-state` 的成品 —— 纯构造器（跨语言金样 `tests/__fixtures__/cc-bus-read.golden.json` 拿它对拍）。
/// 名册那一半照旧挂到身份空间上（`live` / `ccm_sid`，「登记 ≠ 在线」）；pane pid 不上线（界面用不着）。
pub(crate) fn state_reply(
    agents: Vec<RosterRow>,
    spawned: Vec<serde_json::Value>,
    skipped: usize,
    sessions: Option<&[(String, String)]>,
) -> serde_json::Value {
    let rows = agents
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id, "target": r.target, "registered_at": r.registered_at, "unread": r.unread
            })
        })
        .collect();
    serde_json::json!({
        "agents": join_identity(rows, sessions),
        "spawned": spawned,
        "skipped": skipped,
    })
}

/// `bus-inbox` 缺省看几行 / 最多看几行 / 回显最多带多少字节（超了保尾、说 `truncated`）。
const INBOX_LINES_DEFAULT: u64 = 200;
const INBOX_LINES_MAX: u64 = 2000;
const INBOX_CAP: usize = 4 * 1024 * 1024;

/// `bus-inbox` 的入参 —— 纯函数。id 先过形状判定（`§47`：交给 `cc-log` 之前本侧先判，拒码 `bad_id`、一个进程都不起）。
pub(crate) fn parse_inbox(args: &serde_json::Value) -> Result<(String, u64), (String, String)> {
    let obj = args.as_object();
    let id = obj
        .and_then(|o| o.get("id"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            (
                "bad_args".to_string(),
                crate::common::contract::malformed("missing `id`"),
            )
        })?;
    refuse_bad_bus_id(id, |v| copy_text("beCcBus.inbox.badId", &[("id", v)]))
        .map_err(|(c, m)| (c.to_string(), m))?;
    let lines = match obj.and_then(|o| o.get("lines")) {
        None | Some(serde_json::Value::Null) => INBOX_LINES_DEFAULT,
        Some(v) => v
            .as_u64()
            .filter(|n| (1..=INBOX_LINES_MAX).contains(n))
            .ok_or_else(|| {
                (
                    "bad_args".to_string(),
                    crate::common::contract::malformed(&format!(
                        "`lines` must be an integer in 1..={INBOX_LINES_MAX}"
                    )),
                )
            })?,
    };
    Ok((id.to_string(), lines))
}

/// `bus-inbox`：只读看一个 agent 收件箱的尾巴（转调 `cc-log`，不推已读位置 —— 不是 `bus-recv`）。
pub(crate) fn inbox_for_inbound(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (String, String)> {
    let (id, lines) = parse_inbox(args)?;
    let text = read_via("cc-log", &[&id, "-n", &lines.to_string()]).map_err(|(c, m)| {
        if c == "not_installed" {
            (
                c,
                copy_text(
                    "beCcBus.inbox.tooOld",
                    &[("reason", m.trim_end_matches('。'))],
                ),
            )
        } else {
            (c, m)
        }
    })?;
    inbox_reply(&text)
}

/// `cc-log` 的输出 → `bus-inbox` 成品 —— 纯函数。逐行解析（坏行跳过计数、不抛、不因坏行丢好行）；
/// 超上限保尾（这是回显，不是清单）并说 `truncated`。
pub(crate) fn inbox_reply(text: &str) -> Result<serde_json::Value, (String, String)> {
    let body = match text.split_once('\n') {
        Some((head, rest)) if head.trim_end_matches('\r') == LOG_HEAD => rest,
        None if text.trim_end_matches('\r') == LOG_HEAD => "",
        _ => return Err(too_old("cc-log")),
    };
    let (body, truncated) = if body.len() > INBOX_CAP {
        let cut = body.len() - INBOX_CAP;
        let from = body[cut..]
            .find('\n')
            .map(|i| cut + i + 1)
            .unwrap_or(body.len());
        (&body[from..], true)
    } else {
        (body, false)
    };
    let mut messages = Vec::new();
    let mut skipped = 0usize;
    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            skipped += 1;
            continue;
        };
        let get = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let (from, text) = (get("from"), get("text"));
        // from 与 text 全空 = 这行不是一条消息（可能是别的工具写进来的）。
        if from.is_empty() && text.is_empty() {
            skipped += 1;
            continue;
        }
        messages.push(serde_json::json!({
            "from": from, "ts": get("ts"), "text": text, "class": get("class")
        }));
    }
    Ok(serde_json::json!({ "messages": messages, "skipped": skipped, "truncated": truncated }))
}

/// 收件人**有没有人会读** —— `(在名单里吗, 会话活着吗)`。
///
/// `live` 与 `bus-list` 同一套三态：`true` / `false` / `null`（问不到身份空间，或不在名单里）。
/// ⚠ 任何一步失败都**不影响投递**：这是投完之后的"顺带说一句"，不是前置条件。
fn recipient_status(to: &str) -> (bool, serde_json::Value) {
    let Ok((roster, _)) = roster() else {
        return (false, serde_json::Value::Null);
    };
    let rows = roster_agents(&roster);
    let Some(row) = rows
        .iter()
        .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(to))
    else {
        return (false, serde_json::Value::Null);
    };
    let joined = join_identity(
        vec![row.clone()],
        super::gate::list_sessions().ok().as_deref(),
    );
    let live = joined
        .first()
        .and_then(|r| r.get("live"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    (true, live)
}

/// `bus-kill`：收掉一个总线成员（转调 `cc-kill`）。
///
/// # 为什么不是走后端自己那条带三道门的 `kill`
///
/// 两者做的**不是同一件事**：`kill` 只杀 tmux 会话；`cc-kill` 还要清名册、清台账、
/// 清那个 id 的状态 —— 那是 cc-bus 的语义，只有它自己知道要清哪些文件。
///
/// # 门在哪
///
/// backend 的 `kill` 用 §34 三道门，因为它的归属证据**弱**（名字前缀 / `@ccm_sid`）。
/// `cc-kill` 今天用的是**强证据**：`agents.tsv` 第 4 列（登记时记下的 pane 根进程 pid）
/// 与登记的完整地址一起核 —— 08-13 实测过不核的后果：**杀掉占了同名的无辜进程与会话**。
/// ⇒ 门住在懂那套语义的那一侧，不在这里重写一遍。
///
/// ⚠ 「多窗口要不要拦」是产品判断（`U18` 待裁）；今天照收，但 `cc-kill` 会把窗口数打出来。
pub(crate) fn kill_for_inbound(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (String, String)> {
    let id = parse_kill(args)?;
    kill_id(&id)
}

/// 转调 `cc-kill <id>` 并读出它到底动了什么 —— `bus-kill` 与杀会话顺手注销（D-g）共用这一处。
/// ⚠ 调用方先过 `bus_id_ok`（`INVARIANTS §47`：交给 `cc-kill` 之前本侧先判）。
fn kill_id(id: &str) -> Result<serde_json::Value, (String, String)> {
    let id = id.to_string();
    // ⚠ **不加 `--`**〔08-13 真跑撞出来的〕：`cc-send` 会解析旗标（所以那边要显式结束），
    //   而 `cc-kill` **不解析** —— 它直接取 `$1`。传了 `--` 的后果是它去杀一个名叫 `--`
    //   的 agent（`-` 在它的白名单里，连报错都不会），三种情形全回 `killed:false`
    //   而真 agent 的会话好好活着。★ 抄参数形状之前先看被调方怎么解析。
    let out = run("cc-kill", &[&id]).map_err(|(c, m)| (c.to_string(), m))?;
    let detail = out.diagnosis();
    match out.code {
        Some(0) => {}
        Some(TIMED_OUT_CODE) => return Err(timed_out_err(waited_secs(&out))),
        // cc-kill 自己的白名单校验（非法 id）
        Some(2) => {
            return Err((
                "bad_args".to_string(),
                copy_text("beCcBus.kill.refused", &[("detail", &detail.to_string())]),
            ))
        }
        Some(c) => {
            return Err((
                "failed".to_string(),
                copy_text(
                    "beCcBus.kill.failed",
                    &[("status", &c.to_string()), ("detail", &detail.to_string())],
                ),
            ))
        }
        None => {
            return Err((
                "failed".to_string(),
                copy_text(
                    "beCcBus.kill.interrupted",
                    &[("detail", &detail.to_string())],
                ),
            ))
        }
    }
    // ★ **回值要说清"到底动了什么"**：会话是被杀了，还是身份对不上只摘了登记？
    //   cc-kill 两种情况都 exit 0 —— 把它自己的说法读出来，别让调用方以为都一样。
    let said =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    let killed = said.contains("已杀会话");
    let stale_only = said.contains("已摘掉");
    Ok(kill_reply(&id, killed, stale_only))
}

/// 名册里**登记在这组 pane 上**的 id —— 纯函数。认人核第 4 列 pane pid（登记那一刻 pane 根进程的 pid），
/// 不按会话名猜；第 4 列空的老格式行核不了 ⇒ 不挑；id 形状不过 `bus_id_ok` 的不挑（`§47`）。
pub(crate) fn ids_on_panes(rows: &[RosterRow], pane_pids: &[u32]) -> Vec<String> {
    rows.iter()
        .filter(|r| r.pane_pid.is_some_and(|p| pane_pids.contains(&p)))
        .filter(|r| shell_quote_core::bus_id_ok(&r.id))
        .map(|r| r.id.clone())
        .collect()
}

/// monitor 杀会话成功之后：对登记在那个会话 pane 上的每个 id 调 `cc-kill`（名册 · 台账 · 状态 · 收件箱一起清）。
/// 这一步不改杀会话的结局：cc-bus 没装就安静跳过；读不到名册 / 某个 `cc-kill` 失败 ⇒ warn 一句说清。
pub(crate) fn unregister_panes(session: &str, pane_pids: &[u32]) -> BusCleanup {
    let mut out = BusCleanup::default();
    if pane_pids.is_empty() {
        return out;
    }
    let rows = match roster() {
        Ok((rows, _)) => rows,
        Err((code, _)) if code == "not_installed" => return out,
        Err((code, msg)) => {
            tracing::warn!("杀了会话 {session:?}，但读不到 cc-bus 名册（{code}：{msg}）—— 登记在它上面的 id 没注销");
            out.unread = Some(msg);
            return out;
        }
    };
    for id in ids_on_panes(&rows, pane_pids) {
        match kill_id(&id) {
            Ok(v) => {
                tracing::info!("杀了会话 {session:?}，顺手从 cc-bus 收掉登记在它上面的 {id}：{v}");
                out.removed.push(id);
            }
            Err((code, msg)) => {
                tracing::warn!(
                    "杀了会话 {session:?}，但从 cc-bus 收掉 {id} 失败（{code}：{msg}）—— 名册里那一行还在"
                );
                out.failed.push((id, msg));
            }
        }
    }
    out
}

/// 杀会话顺手注销的结局（进 `kill` 成品的 `bus` 那一格；界面说一句）。
/// 全空 ＝ 没有要注销的（没装 cc-bus / 这个会话上没登记 / 读不到 pane）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BusCleanup {
    /// 注销掉了的 id。
    pub(crate) removed: Vec<String>,
    /// 没注销成的 `(id, 原话)`。
    pub(crate) failed: Vec<(String, String)>,
    /// 名册读不到（原话）⇒ 登记在上面的一个都没注销。
    pub(crate) unread: Option<String>,
}

impl BusCleanup {
    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "removed": self.removed,
            "failed": self.failed.iter().map(|(id, why)| serde_json::json!({ "id": id, "why": why })).collect::<Vec<_>>(),
            "unread": self.unread,
        })
    }
}

/// `bus-kill` 的入参 —— 纯函数〔C4e：从 [`kill_for_inbound`] 里原样抽出（逻辑不动），跨语言金样拿它核请求样例〕。
/// 交给 `cc-kill` 之前先过形状判定（[`refuse_bad_bus_id`]）—— 这是破坏性的那一条，更不能靠对端。
pub(crate) fn parse_kill(args: &serde_json::Value) -> Result<String, (String, String)> {
    let id = args
        .as_object()
        .and_then(|o| o.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            (
                "bad_args".to_string(),
                crate::common::contract::malformed("missing `id`"),
            )
        })?;
    refuse_bad_bus_id(id, |v| {
        copy_core::copy_text("beCcBus.parse.badKillId", &[("id", v)])
    })
    .map_err(|(c, m)| (c.to_string(), m))?;
    Ok(id.to_string())
}

/// `bus-kill` 的成品 `{id, killed, stale_only}` —— 从 [`kill_for_inbound`] 里原样抽出来（逻辑不动），
/// 理由同 [`list_reply`]。
pub(crate) fn kill_reply(id: &str, killed: bool, stale_only: bool) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "killed": killed,
        // 身份对不上 ⇒ 只摘了陈旧登记，**会话与收件箱都没动**
        "stale_only": stale_only,
    })
}

pub(crate) fn send_for_inbound(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (String, String)> {
    let (to, text, from) = parse_send(args).map_err(|(c, m)| (c.to_string(), m))?;
    deliver(&to, &text, from.as_deref())?;
    // ★ 投出去之后，把「有没有人会读」也一并回答〔用@08-13 那条架构点的另一半〕。
    //
    // 病：今天两种「没人会读」都只回 `sent:true` —— ① 收件人**压根没登记**
    //（cc-send 会在 stderr 警告，但那句话到不了后端的调用方）；
    // ② 登记过、**会话早没了**（cc-bus 那份名单会过期）。
    // ⇒ 投递照旧（先发后到是正当用法），但**说清楚**：`registered` + 三态 `live`。
    let (registered, live) = recipient_status(&to);
    Ok(send_reply(&to, registered, live, from.as_deref()))
}

/// 投一条（转调 `cc-send`）—— `bus-send` 与 `bus-broadcast` 共用这**一处**起进程〔C4e：从 [`send_for_inbound`] 里原样抽出〕。
///
/// 抽出来的理由：广播逐个投递时不该每一条都再问一遍「有没有人会读」（那是 `recipient_status` 的
/// 一次 `cc-list` ＋ 一次身份空间查询）—— 广播挑人时已经问过了。起 `cc-send` 的仍只有这一处。
fn deliver(to: &str, text: &str, from: Option<&str>) -> Result<(), (String, String)> {
    // `--` 显式结束旗标：收件人万一以 `--` 开头也当收件人，不会被 cc-send 当成选项。
    let out = run_as("cc-send", &["--", to, text], from).map_err(|(c, m)| {
        // 只有这条路带得动大载荷 ⇒ 把**实测长度**补进去（诊断里给数，别让人自己去量）。
        if c == "too_long" {
            return (
                c.to_string(),
                copy_text(
                    "beCcBus.deliver.tooLong",
                    &[("reason", &m), ("n", &text.len().to_string())],
                ),
            );
        }
        (c.to_string(), m)
    })?;
    let detail = out.diagnosis();
    classify_send(out.code, &detail, waited_secs(&out))
}

/// `bus-send` 的成品 —— 从 [`send_for_inbound`] 里原样抽出来（逻辑不动），理由同 [`list_reply`]。
pub(crate) fn send_reply(
    to: &str,
    registered: bool,
    live: serde_json::Value,
    from: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "to": to, "sent": true, "registered": registered, "live": live,
        // 回显**以谁的身份发的**：不给 `from` 时是 `null`，那时收信人看到的是
        // cc-whoami 在后端处境里解出来的东西（实测：`unknown`）——
        // 回显出来，调用方才看得见这件事，而不是等收信人来问「谁发的」。
        "from": from
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// `bus-broadcast`：给总线上**在线**的成员群发一条
// ═══════════════════════════════════════════════════════════════════════════
//
// # 它治的是什么
//
// 广播此前是 **monitor 里的组合**（`bus-list` 挑人 ＋ 逐个 `bus-send`，monitor `cc_bus.rs` 的
// `broadcast_via_backend` / `pick_broadcast_targets`〔散文墓碑〕）。界面改成经通道直接说后端之后
// （业务解释只有一个家），一个组合要么在界面里再写一遍（第二份挑人规则），要么收进后端 ——
// 收进来：挑人与逐个投递都在这一台机器上做，界面只收一份成品（三个数分开说）。
//
// # 挑人规则（逐字承接 monitor 那一份，P4f 08-13 实测出来的）
//
// · 身份空间答得上（有 `true` / `false`）⇒ **只发 `live == true` 的**（老 `cc-broadcast` 发给 `agents.tsv` 的
//   每一行，用户机器实测 86 行登记、只有 8 个会话还活着 ⇒ 78 个幽灵收件箱）；
// · 全是 `null`（问不到 tmux）⇒ 退回「发给所有登记的」并标 `liveness_unknown`（「问不到」≠「都不在」）；
// · 不发给自己（`from`）。
//
// # 部分失败不回错
//
// 已经投出去一部分之后再失败，**不许**整条回错（调用方会以为一条都没发、再发一遍 ⇒ 一部分人收到两遍）。
// ⇒ 成品里 `failed` 逐个列出（`{id, error, detail}`，`error` 是 `bus-send` 那一套码），`sent` 数照实。
// 只有「一条都还没发」的失败（列名单那一步）才整条回错。

/// `bus-broadcast` 的入参 —— 纯函数。`text` 必须非空（空广播不是缺省）；`from` 可选（同 [`parse_send`]）。
fn parse_broadcast(args: &serde_json::Value) -> Result<(String, Option<String>), CmdErr> {
    let obj = args.as_object().ok_or((
        "bad_args",
        crate::common::contract::malformed("args must be an object"),
    ))?;
    let text = obj.get("text").and_then(|v| v.as_str()).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `text`"),
    ))?;
    if text.trim().is_empty() {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`text` is empty"),
        ));
    }
    // `from` 给了就先判（同 `bus-send` 那一处）：判不过 ⇒ 整条 `bad_id`，一个人都没发。
    let from = given_sender(obj)?;
    Ok((text.to_string(), from))
}

/// 广播挑出来的人。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BroadcastPlan {
    pub(crate) targets: Vec<String>,
    pub(crate) skipped_offline: usize,
    pub(crate) liveness_unknown: bool,
}

/// 广播要发给谁 —— **纯函数**（`agents` 是 `bus-list` 那一份，已挂过身份空间）。挑人规则见本节头注。
pub(crate) fn pick_broadcast_targets(agents: &[serde_json::Value], me: &str) -> BroadcastPlan {
    let known: bool = agents
        .iter()
        .any(|a| a.get("live").map(|v| !v.is_null()).unwrap_or(false));
    let mut targets = Vec::new();
    let mut skipped_offline = 0usize;
    for a in agents {
        let Some(id) = a.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        if id == me {
            continue; // 不发给自己（同 cc-broadcast）
        }
        let live = a.get("live").and_then(|v| v.as_bool());
        if known && live != Some(true) {
            skipped_offline += 1;
            continue;
        }
        targets.push(id.to_string());
    }
    BroadcastPlan {
        targets,
        skipped_offline,
        liveness_unknown: !known,
    }
}

/// 广播名单里的一个收件人过不过形状判定 —— 纯函数：过 ⇒ `None`；不过 ⇒ 成品 `failed` 里那一格（`{id, error:"bad_id", detail}`）。
///
/// 〔`INVARIANTS §47`〕名单是 `cc-list` 的输出 —— **对端来的**值（「这是我们自己的数据」不是理由：
/// 盘上真出现过 `--help.jsonl`）。先前原样交给 `cc-send`；今天与 `bus-send` 的收件人同一个判定（[`refuse_bad_bus_id`]）。
/// 不整条回错：到这一步时可能已经投出去几个了（本节头注「部分失败不回错」那一条），判不过的这一个照实列进 `failed`。
pub(crate) fn recipient_refused(id: &str) -> Option<serde_json::Value> {
    refuse_bad_bus_id(id, |v| {
        copy_core::copy_text("beCcBus.parse.badRecipient", &[("id", v)])
    })
    .err()
    .map(|(error, detail)| serde_json::json!({ "id": id, "error": error, "detail": detail }))
}

/// `bus-broadcast` 的成品 —— 纯函数：三个数分开（发到几个 · 因不在线跳过几个 · 失败几个，失败逐个列）。
pub(crate) fn broadcast_reply(
    plan: &BroadcastPlan,
    sent: usize,
    failed: Vec<serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "sent": sent,
        "skipped_offline": plan.skipped_offline,
        "liveness_unknown": plan.liveness_unknown,
        "failed": failed,
    })
}

/// `bus-broadcast`：列名单（同 `bus-list` 那一个函数）→ 挑人 → 逐个投递（同 `bus-send` 那一处起进程）。
pub(crate) fn broadcast_for_inbound(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (String, String)> {
    let (text, from) = parse_broadcast(args).map_err(|(c, m)| (c.to_string(), m))?;
    let agents = agents_via_cc_list()?;
    let plan = pick_broadcast_targets(&agents, from.as_deref().unwrap_or(""));
    let mut sent = 0usize;
    let mut failed = Vec::new();
    for id in &plan.targets {
        // 名单里的收件人（`cc-list` 的输出，对端来的）交给 `cc-send` 之前先判形状；判不过的不发、进 `failed`（不静默跳过）。
        if let Some(entry) = recipient_refused(id) {
            failed.push(entry);
            continue;
        }
        match deliver(id, &text, from.as_deref()) {
            Ok(()) => sent += 1,
            // ⚠ 键刻意不叫 `code` / `message`：那一对是**整条失败**的错误信封（`readonly_guard` 的信封普查按键认它）；
            //   这里是一份**成功**成品里逐个列出的投递失败 —— 形状不同的两件事，不借同一对键。
            Err((error, detail)) => failed.push(serde_json::json!({
                "id": id, "error": error, "detail": detail
            })),
        }
    }
    Ok(broadcast_reply(&plan, sent, failed))
}

// ═══════════════════════════════════════════════════════════════════════════
// `bus-spawn`：派生一个协作 agent（转调 `cc-spawn`）
// ═══════════════════════════════════════════════════════════════════════════
//
// # 它治的是什么
//
// 这条原语登记之前，monitor 侧派生对 `<local>` 一律拒绝，理由逐字：「cc-bus 的这一面在本机
// 没有对侧（backend 侧没有它的原语）…剩下这一条等的是**后端先长出那条原语**，不是等谁记得接线」。
// ⇒ **本节就是那条原语**。登记之后本机与远端都改走它（`K-R98` 给发消息、`K-R112` 给收掉
// 做过的同一件事），远端那条拼 `cc-spawn …` shell 串走 SSH 的老路同轮删掉。
//
// # 登记与 `BUILD_ID`
//
// 登记 = `inbound::COMMANDS` / `REGISTRY` 各一行 ＋ `SUBCOMMANDS` 一行 `--bus-spawn`
// ⇒ 子命令集指纹变了 ⇒ `build_id_guard::adding_a_subcommand_forces_a_build_id_bump` 红。
// 那一条红是**预期的**：加了命令就该 bump。
//
// # 它**不**做的
//
// · **不重写起会话**：命名避让 / 总线登记 / 台账全在 `cc-spawn`（它内部再经 `ccm`），
//   本模块只转调 —— 同本模块头注 ②「转调脚本，不在这里重实现」。
// · **不替用户选账号**：`account` 与 `base` 必须二选一**显式**给（不传 ⇒ ccm 落 manifest 的
//   默认号 ⇒ 点两下就在一个没人选过的号上烧额度，B03 审计重要-5 那条）。
// · **不白名单 agent 种类**：`tool` 只判非空，认不认归 `cc-spawn`（见 [`parse_spawn`]）。
// · 发给 `cc-spawn` 的旗标登记在 `protocol_doc_guard::CHILD_PROCESS_FLAGS`（它们不是后端 argv）。

/// `bus-spawn` 的入参（形状校验过的）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpawnArgs {
    pub(crate) tool: String,
    pub(crate) dir: String,
    pub(crate) task: String,
    /// `Some(名)` ⇒ `--account <名>`；`None` ⇒ `--base`（**显式**不注入）。
    pub(crate) account: Option<String>,
}

/// 形状校验 —— 纯函数。
///
/// ⚠ 与 [`parse_send`] 同一条纪律：argv 直传不过 shell。
/// 它判的是「这组参数能不能构成一次**有意义且表过态**的调用」：
/// `tool` 是注册表里由我们起的一家（缺 / 空 ⇒ 默认那一家；认法住 `agents::pick_kind`）· `dir` 非空 ·
/// `account` 与 `base:true` **恰好给一个**（都不给 ⇒ 拒：那是替用户选了默认号）·
/// 给了 `account` 就先过形状判定（[`refuse_bad_bus_id`]，`§47` ①）。
pub(crate) fn parse_spawn(args: &serde_json::Value) -> Result<SpawnArgs, CmdErr> {
    let obj = args.as_object().ok_or((
        "bad_args",
        crate::common::contract::malformed("args must be an object"),
    ))?;
    let s = |k: &str| obj.get(k).and_then(|v| v.as_str()).unwrap_or("").trim();
    // 哪一家问注册表（不写名字白名单）：认不出 ⇒ `bad_args`，那句话列出认得的几家；交给 cc-spawn 的是解析好的 kind。
    let (tool, _) = crate::agents::pick_kind(Some(s("tool"))).map_err(|say| ("bad_args", say))?;
    let dir = s("dir");
    if dir.is_empty() {
        return Err((
            "bad_args",
            crate::common::contract::malformed("missing `dir` (working directory)"),
        ));
    }
    let base = obj.get("base").and_then(|v| v.as_bool()).unwrap_or(false);
    let account = s("account");
    // 账号名交给 `cc-spawn --account` 之前先过同一条形状判定（C4e 那一道原来住界面 `checkSpawnShape`）。
    if !account.is_empty() {
        refuse_bad_bus_id(account, |v| {
            copy_core::copy_text("beCcBus.parse.badAccount", &[("account", v)])
        })?;
    }
    let account = match (account.is_empty(), base) {
        (false, false) => Some(account.to_string()),
        (true, true) => None,
        (false, true) => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("`account` and `base` are mutually exclusive"),
            ))
        }
        (true, false) => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("one of `account` or `base:true` is required")
                    .to_string(),
            ))
        }
    };
    Ok(SpawnArgs {
        tool: tool.to_string(),
        dir: dir.to_string(),
        task: s("task").to_string(),
        account,
    })
}

/// 拼给 `cc-spawn` 的 argv —— 纯函数。
///
/// **`--` 不能省**：`cc-spawn` 的旗标循环跑在取位置参数之前，`dir` 若是 `--new` 这类词会被它
/// 自己吃成旗标（B03 审计建议那一条，当年记在 monitor 侧那条 SSH 构造器的头注里；那条路 BS1b 删了，
/// 这件事搬到了这里）。任务为空就不传。
pub(crate) fn spawn_argv(a: &SpawnArgs) -> Vec<String> {
    let mut v = vec!["--tool".to_string(), a.tool.clone()];
    match &a.account {
        Some(acct) => {
            v.push("--account".to_string());
            v.push(acct.clone());
        }
        None => v.push("--base".to_string()),
    }
    v.push("--".to_string());
    v.push(a.dir.clone());
    if !a.task.is_empty() {
        v.push(a.task.clone());
    }
    v
}

/// 从 `cc-spawn` 的回显里取新会话的 id —— 纯函数。
///
/// 今天的形状（`src/shared/cc-bus/scripts/cc-spawn` 末尾现打）：`已 spawn: <id>   (目录: …)`。
/// ⚠ 拿输出当接口的代价（同 [`parse_roster_tsv`]）：cc-bus 换个说法这里就认不出 ⇒ 回 `None`，
/// **不猜**。调用方拿到 `None` 时要说「起了，但 id 没认出来」，而不是「没起来」。
pub(crate) fn spawned_id_of(said: &str) -> Option<String> {
    said.lines().find_map(|l| {
        let rest = l.trim_start().strip_prefix("已 spawn:")?;
        let id = rest.split_whitespace().next()?;
        (!id.is_empty()
            && !id.starts_with('-')
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .then(|| id.to_string())
    })
}

/// `bus-spawn`：派生一个协作 agent。**会起一个真 agent 进程（烧额度）**。
///
/// # 退出码 → 语义码（这张表只在本模块，理由见 [`classify_send`] 头注）
///
/// | `cc-spawn` rc | 码 | 说法 |
/// |---|---|---|
/// | 0 | —— | 回 `{spawned:true, id, said}`；`id` 认不出是 `null`（**起了**，只是没认出名字） |
/// | 2 | `bad_args` | 它自己的参数校验（目录不存在 / 未知 tool / 账号互斥 / ccm 太旧）|
/// | 124 | `timed_out` | 🔴 **会话可能已经起来了** —— 说法里明写「先看 `bus-state` 再决定要不要重来」|
/// | 其它 / 信号 | `failed` | 原样带上它的诊断 |
///
/// ⚠ 期限仍住在子进程里（`timeout` 前缀，默认 10 秒，`CC_BUS_TIMEOUT_SECS` 可调），零定时器铁律不动。
pub(crate) fn spawn_for_inbound(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (String, String)> {
    let a = parse_spawn(args).map_err(|(c, m)| (c.to_string(), m))?;
    let argv = spawn_argv(&a);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    let out = run_as("cc-spawn", &refs, None).map_err(|(c, m)| (c.to_string(), m))?;
    classify_spawn(out.code, &out.diagnosis(), waited_secs(&out))?;
    let said = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(spawn_reply(&said))
}

/// `bus-spawn` 的成品 `{spawned, id, said}` —— 从 [`spawn_for_inbound`] 里原样抽出来（逻辑不动），
/// 理由同 [`list_reply`]。
pub(crate) fn spawn_reply(said: &str) -> serde_json::Value {
    serde_json::json!({
        "spawned": true,
        "id": spawned_id_of(said),
        "said": said,
    })
}

/// `cc-spawn` 的退出码 → 语义码 —— 纯函数（表在 [`spawn_for_inbound`] 头注）。
pub(crate) fn classify_spawn(
    code: Option<i32>,
    detail: &str,
    waited: u64,
) -> Result<(), (String, String)> {
    match code {
        Some(0) => Ok(()),
        Some(2) => Err((
            "bad_args".to_string(),
            copy_text("beCcBus.spawn.refused", &[("detail", &detail.to_string())]),
        )),
        Some(TIMED_OUT_CODE) => {
            let (c, m) = timed_out_err(waited);
            Err((c, copy_text("beCcBus.spawn.timedOut", &[("reason", &m)])))
        }
        Some(c) => Err((
            "failed".to_string(),
            copy_text(
                "beCcBus.spawn.failed",
                &[("status", &c.to_string()), ("detail", &detail.to_string())],
            ),
        )),
        None => Err((
            "failed".to_string(),
            copy_text(
                "beCcBus.spawn.interrupted",
                &[("detail", &detail.to_string())],
            ),
        )),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/cc_bus_tests.rs"]
mod tests;
