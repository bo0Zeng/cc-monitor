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
//! 本模块经那一处转调的外部命令**今天是三条**，逐条说清写面：
//! · `cc-list` **只读**；
//! · `cc-send` 会写收件人的收件箱；
//! · `cc-log` **只读**（共享锁，不推已读位置）。
//!
//! ⚠ **别照这段散文数** —— 「今天转调哪几条」的唯一住址是
//! [`tests::TRANSCALLS`]（从生产段现算之后与它对拍）。这一段只讲写面。
//!
//! 三条都是**被起的那个进程**在写，与用户自己在终端里敲同一条命令没有区别
//!（同 `launch` 起 claude 的 D1 正例：收窄后的铁律管的是 **backend 进程自身**不写用户既有数据）。
//!
//! `readonly_guard::g6_reach` 那一格钉「只读铁律的豁免理由覆盖了哪几条」，与本文件的 [`tests::TRANSCALLS`]
//! **不是同一条规矩的两处住址**：后者钉「`K33`『不要 bash 脚本』对每一条各裁了什么」。

use copy_core::copy_text;
use std::path::{Path, PathBuf};

use crate::platform::child::Deadline;
use crate::plugin::invoke::Done;
use crate::plugin::invoke::NotRun;
use crate::plugin::invoke::TIMED_OUT_CODE;

use super::cc_bus_wire::{
    BusAgent, BusInbox, BusInboxReq, BusList, BusMessage, BusSendReq, BusSent,
};

/// `bus-list` 整条命令总期限的上限（`cc-list` ＋ 列 tmux 会话）。
pub(crate) const BUS_LIST_CAP: Deadline = Deadline::secs(13);
/// `bus-send` 整条命令总期限的上限（`cc-send` ＋ 问收件人在不在：`cc-list` ＋ 列 tmux 会话）。
pub(crate) const BUS_SEND_CAP: Deadline = Deadline::secs(28);

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
/// 「细节先按原本的就行」⇒ 不动 cc-bus 本体。
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
/// 其中最重的一格是 `[15]`：`CC_BUS_HOME` 一没，被调的那条命令就照着**另一个家里的空名册**
/// 判「这个名字还是原来那个人吗」，判错了人。
///
/// # 🔴 修法**不许**是「往那张白名单里加键」
///
/// 那等于让通用调用口认识一个具体插件 —— 而
/// `plugin::layer_guard::the_generic_port_names_no_concrete_plugin` 的针里
/// **逐字就有这个前缀**（那张表里那一条注着「某个插件的环境变量前缀」）。
/// ⇒ **谁的插件，谁交办**：这一族键的家只能是本模块。
///
/// 按**前缀**而不是逐个列名：这是 cc-bus 自己的命名空间，而它是一族**会长**的东西
/// （`CCBUS_RATE_*` · `CCBUS_POLICY_MODE` · `CCBUS_NUDGE_DEBOUNCE` …），
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
    crate::plugin::invoke::run(&bin, args, secs, &env, None).map_err(|e| match e {
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
        // 这条失败形没有原话位：系统原话记一行日志，句子只带原因词。
        NotRun::Failed(s) => ("failed", crate::common::said::IntoNote::into_note(s)),
    })
}

/// 超时那条的说法 —— 几个命令共用一份文案。`secs` ＝ 实际等了多久（[`waited_secs`]）。
fn timed_out_err(secs: u64) -> (String, String) {
    (
        crate::platform::child::TIMED_OUT.to_string(),
        copy_text(
            "beCcBus.timedOut.say",
            &[(
                "dur",
                &copy_core::format_duration(secs.saturating_mul(1000)),
            )],
        ),
    )
}

/// 这一趟实际等了多久：过了期限被收掉的 ⇒ 原语交回的那个数（被命令总期限截短时就是截短后的）；
/// 码是 124 却不是我们收的（它自己这么退的）⇒ 给它的那个期限。
pub(crate) fn waited_secs(out: &Done) -> u64 {
    out.waited_secs.unwrap_or_else(timeout_secs)
}

/// 机器可读形的首行标记（`cc-list --tsv` / `cc-log`）。老 cc-bus 不认 `--tsv`、
/// 照打人读表 ⇒ 见不到标记就明说「cc-bus 比后端旧」，不猜着按人读表解（命令是接口，接口认不出就说）。
/// `cc-register` 的「这一格会话编号」旗标（ccm 起会话的收尾登记时知道编号就带上，`plan::render_container_tail`）。
pub(crate) const REGISTER_SID: &str = "--sid";

const ROSTER_TSV_HEAD: &str = "#cc-list-tsv\t1";
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

/// 名册一行（`cc-list --tsv` 的五格里本侧用到的三格：id · 地址 · 待读）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RosterRow {
    pub(crate) id: String,
    pub(crate) target: String,
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
                    unread,
                })
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
        // 路由层拦截（ACL / 限流），bus.log 里有 REJECT/THROTTLE 一行
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

/// 〔`INVARIANTS §47` ①〕agent id 在交给 `cc-send` / `cc-log` **之前**先过形状判定
/// （`shell_quote_core::bus_id_ok`：非空 · 不以 `-` 开头 · 只含 `[A-Za-z0-9_-]`，全仓唯一一份）。判不过 ⇒ `bad_id`，一个进程都不起。
///
/// 为什么在这里判、而不是「交给 cc-bus 自己去拒」：`§47` 逐字「**『对端会校验』不是理由**」—— id 会拼进对端的文件路径；
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
pub(crate) fn parse_send(req: &BusSendReq) -> Result<(String, String, Option<String>), CmdErr> {
    if req.to.trim().is_empty() {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`to` is empty"),
        ));
    }
    refuse_bad_bus_id(&req.to, |v| {
        copy_core::copy_text("beCcBus.parse.badRecipient", &[("id", v)])
    })?;
    // ★ `from` 可选：**不给就是今天的行为**（cc-whoami 在后端的处境里解不出身份 ⇒ `unknown`）。
    //   给了就以那个身份发 —— 收信人才知道是谁，回复才有地方去。
    //   ⚠ 合法性仍归 cc-bus（`cc-whoami` 自己会消毒成 `[A-Za-z0-9_-]`），这里只判形状。
    let from = given_sender(req.from.as_deref())?;
    Ok((req.to.clone(), req.text.clone(), from))
}

/// `from`（以谁的身份发）：可选；给了就在交给 `cc-send`（作 `CC_BUS_ID`）**之前**先过同一个形状判定（[`refuse_bad_bus_id`]）。
///
/// 〔`INVARIANTS §47`「交给对端之前本侧先判」〕先前 `from` 原样交给 `cc-send`、一格都不判 ——
/// 它与收件人是同一种值（agent id），同样是交给对端去寻址的。
fn given_sender(from: Option<&str>) -> Result<Option<String>, CmdErr> {
    let from = from.map(str::trim).filter(|s| !s.is_empty());
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
pub(crate) fn liveness(
    target: &str,
    sessions: Option<&[(String, String)]>,
) -> (Option<bool>, Option<String>) {
    let name = session_name_of(target);
    match sessions {
        None => (None, None),
        // `K-R96`：`gate::list_sessions` 今天回的是**二元组**（会话名 ＋ `@ccm_sid`）——
        // 中间那列 `#{session_id}` 本函数从来没问过，快照那边也不再取它。
        Some(list) => match list.iter().find(|(n, _)| n == name) {
            None => (Some(false), None),
            Some((_, ccm_sid)) => (Some(true), (!ccm_sid.is_empty()).then(|| ccm_sid.clone())),
        },
    }
}

/// 总线名单 —— `bus-list` 读它。读机器可读形（[`roster`]，
/// `cc-list --tsv`）：名册只有一个解析器。没重部署 cc-bus 的机器上老脚本不认 `--tsv` ⇒ 明说「cc-bus 比后端旧」（[`too_old`]），
/// 不退回去按人读表猜（命令是接口，接口认不出就说）。
fn agents_via_cc_list() -> Result<Vec<BusAgent>, (String, String)> {
    let (rows, skipped) = roster()?;
    if skipped > 0 {
        tracing::warn!("cc-bus 名册里有 {skipped} 行读不懂，这一趟没算进去");
    }
    // ★ 去问**身份空间**谁还活着。问不到就回 `null`，不假装知道。
    let sessions = super::gate::list_sessions().ok();
    Ok(roster_agents(&rows, sessions.as_deref()))
}

/// 名册行 → `bus-list` 那一格（纯函数；`live` / `ccm_sid` 挂到身份空间上，见 [`liveness`]）。
pub(crate) fn roster_agents(
    rows: &[RosterRow],
    sessions: Option<&[(String, String)]>,
) -> Vec<BusAgent> {
    rows.iter()
        .map(|r| {
            let (live, ccm_sid) = liveness(&r.target, sessions);
            BusAgent {
                id: r.id.clone(),
                target: r.target.clone(),
                unread: r.unread,
                live,
                ccm_sid,
            }
        })
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

/// 名册（机器可读形）：`bus-list` 与 `bus-send` 问收件人在不在读它。
pub(crate) fn roster() -> Result<(Vec<RosterRow>, usize), (String, String)> {
    parse_roster_tsv(&read_via("cc-list", &["--tsv"])?)
}

pub(crate) fn list_for_inbound() -> Result<BusList, (String, String)> {
    Ok(list_reply(agents_via_cc_list()?))
}

/// `bus-list` 的成品 `{agents}` —— 从 [`list_for_inbound`] 里原样抽出来（逻辑不动），
/// 只为让跨语言金样 `tests/__fixtures__/cc-bus-control.golden.json` 拿**同一个**构造器对拍：
/// 界面（`src/frontend/ui/cc-bus-control.ts`）从此直接收这份成品，monitor 那一跳只搬字节。
pub(crate) fn list_reply(agents: Vec<BusAgent>) -> BusList {
    BusList { agents }
}

/// `bus-inbox` 缺省看几行 / 最多看几行 / 回显最多带多少字节（超了保尾、说 `truncated`）。
const INBOX_LINES_DEFAULT: u64 = 200;
const INBOX_LINES_MAX: u64 = 2000;
const INBOX_CAP: usize = 4 * 1024 * 1024;

/// `bus-inbox` 的入参 —— 纯函数。id 先过形状判定（`§47`：交给 `cc-log` 之前本侧先判，拒码 `bad_id`、一个进程都不起）。
pub(crate) fn parse_inbox(req: &BusInboxReq) -> Result<(String, u64), (String, String)> {
    if req.id.is_empty() {
        return Err((
            "bad_args".to_string(),
            crate::common::contract::malformed("missing `id`"),
        ));
    }
    refuse_bad_bus_id(&req.id, |v| copy_text("beCcBus.inbox.badId", &[("id", v)]))
        .map_err(|(c, m)| (c.to_string(), m))?;
    let lines = match req.lines {
        None => INBOX_LINES_DEFAULT,
        Some(n) if (1..=INBOX_LINES_MAX).contains(&n) => n,
        Some(_) => {
            return Err((
                "bad_args".to_string(),
                crate::common::contract::malformed(&format!(
                    "`lines` must be an integer in 1..={INBOX_LINES_MAX}"
                )),
            ))
        }
    };
    Ok((req.id.clone(), lines))
}

/// `bus-inbox`：只读看一个 agent 收件箱的尾巴（转调 `cc-log`，不推已读位置 —— 不是 `bus-recv`）。
pub(crate) fn inbox_for_inbound(req: &BusInboxReq) -> Result<BusInbox, (String, String)> {
    let (id, lines) = parse_inbox(req)?;
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
pub(crate) fn inbox_reply(text: &str) -> Result<BusInbox, (String, String)> {
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
        messages.push(BusMessage {
            from,
            ts: get("ts"),
            text,
            class: get("class"),
        });
    }
    Ok(BusInbox {
        messages,
        skipped,
        truncated,
    })
}

/// 收件人**有没有人会读** —— `(在名单里吗, 会话活着吗)`。
///
/// `live` 与 `bus-list` 同一套三态：`true` / `false` / `null`（问不到身份空间，或不在名单里）。
/// ⚠ 任何一步失败都**不影响投递**：这是投完之后的"顺带说一句"，不是前置条件。
fn recipient_status(to: &str) -> (bool, Option<bool>) {
    let Ok((roster, _)) = roster() else {
        return (false, None);
    };
    match roster.iter().find(|r| r.id == to) {
        None => (false, None),
        Some(row) => (
            true,
            liveness(&row.target, super::gate::list_sessions().ok().as_deref()).0,
        ),
    }
}

pub(crate) fn send_for_inbound(req: &BusSendReq) -> Result<BusSent, (String, String)> {
    let (to, text, from) = parse_send(req).map_err(|(c, m)| (c.to_string(), m))?;
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

/// 投一条（转调 `cc-send`）—— 起 `cc-send` 的只有这一处。
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
    live: Option<bool>,
    from: Option<&str>,
) -> BusSent {
    BusSent {
        to: to.to_string(),
        sent: true,
        registered,
        live,
        // 回显**以谁的身份发的**：不给 `from` 时是 `null`，那时收信人看到的是
        // cc-whoami 在后端处境里解出来的东西（实测：`unknown`）——
        // 回显出来，调用方才看得见这件事，而不是等收信人来问「谁发的」。
        from: from.map(str::to_string),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/cc_bus_tests.rs"]
mod tests;
