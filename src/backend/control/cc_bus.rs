//! `P4f`：cc-bus 的**基础命令** —— `bus-list`（谁在线 + 各自待读数）与 `bus-send`（发一条）。
//!
//! 〔用 08-13〕逐字：「**也是daemon先把基础命令做了, 其他具体的细节先按原本的就行,
//! 后面我可能要改ccbus**」。三条判断都是从这句话推出来的，读数在账本 `P4f §3`。
//!
//! # ① 第一刀**不做** `bus-recv`
//!
//! `cc-recv` **有副作用**：它会推进已读位置。daemon 代读 = **把消息从人那里偷走** ——
//! agent 自己再跑 `cc-recv` 就什么都看不到了。（08-13 刚修过同一族的一条真事故：
//! Stop 钩子把 40 条积压「读过了却没喂回去」，账本 `P4b §7g-9k`。）
//!
//! ⇒ 消费性操作第一刀不做；「有没有新的」这个需求由 `bus-list` 的**待读数**满足
//! （只读、不消费）。要做代读的那天，先得给它一个「读了但不算数」的两阶段口。
//!
//! # ② 转调脚本，**不在这里重实现**总线
//!
//! 用户逐字「细节先按原本的就行」+「后面我可能要改ccbus」⇒ 在 daemon 里重写一份
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
//! 今天 `cc-list` 的输出是空白分列、且 id/target 都过 `[A-Za-z0-9_-]` 白名单消毒
//! （不含空白）⇒ 解析可靠（`parse_list` 有判据）。
//!
//! 这条由 [`tests::no_cc_bus_data_layout_leaks_into_the_daemon`] 钉住。
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
//!（同 `launch` 起 claude 的 D1 正例：收窄后的铁律管的是 **daemon 进程自身**不写用户既有数据）。
//!
//! ⚠⚠ 这段话此前逐字写着「**两条命令**共用」，而 `cc-kill` 是 08-13 当天稍晚进来的
//!（那个 commit 动了 8 个文件，`readonly_guard.rs` 不在其中）⇒ 那条豁免理由**漏掉了今天真实的写面**，
//! 而三条判据全绿 —— 起进程登记的键里程序名是 `<非字面量>`，三条命令**共用同一个键**，
//! 加第三条不会红。这一句与 `ALLOWED` 那一行的理由**同轮一起订正**。
//!
//! ⚠ 〔`K-R113` 09-13〕**这一次加第四条是红着加进来的**，而红它的不是上面那条登记：
//! `readonly_guard::g6_reach::the_non_literal_spawn_key_still_covers_exactly_three_commands`
//! 把「今天是三条」钉成**相等** ⇒ 加 `cc-agents` 当场红，逼人回去重读那条豁免理由。
//! 那一格与本文件的 [`tests::TRANSCALLS`] **不是同一条规矩的两处住址**：
//! 前者钉「只读铁律的豁免理由覆盖了哪几条」，后者钉「`K33`『不要 bash 脚本』对每一条各裁了什么」。

use std::path::{Path, PathBuf};

use crate::plugin::invoke::Done;
use crate::plugin::invoke::NotRun;
use crate::plugin::invoke::TIMED_OUT_CODE;

/// 找不到时那句话的**尾巴** —— 这是 cc-bus 自己的话，通用层不该认识它。
const NOT_INSTALLED_HINT: &str = "cc-bus 装了吗？（装在别处可以用 CC_BUS_BIN_DIR 指过来）";

/// 命令级错误：`(code, message)`。与 [`super::kill`] / [`super::launch`] 同型。
type CmdErr = (&'static str, String);

/// 固定位置的查找顺序 —— **纯函数**（不读环境变量，好测）。
///
/// `CC_BUS_BIN_DIR`（部署/台架覆盖）→ `~/.local/bin` → `~/.claude/skills/cc-bus/scripts`。
///
/// ⚠ 为什么不只靠 `PATH`：daemon 由 app 经 **SSH exec** 起，那是**非登录 shell**，
/// `~/.local/bin` 未必在 `PATH` 里（08-13 实测）⇒ 只靠 PATH 会出现「明明装了却找不到」。
/// 这条与 `ccm` 的 `DAEMON_BIN_RECIPE` 是同一个形状（那边找 daemon，这边找 cc-bus），
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
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty());
    let fixed = fixed_candidates(
        override_dir.as_ref().map(Path::new),
        home.as_ref().map(Path::new),
        name,
    );
    crate::plugin::discover::find(name, &fixed, true, NOT_INSTALLED_HINT)
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
/// # ★★ 期限住在**子进程**里，不在 daemon 里〔08-13 实测事故 + 铁律冲突〕
///
/// 病先说清楚：`Command::output()` **无限等**。实测把 `cc-send` 换成 `sleep 300` 的桩，
/// `--bus-send` 25 秒都没回来（25 是我从外面掐的，daemon 自己没有任何期限）。
/// 这不是假想 —— `cc-send` 的投递走 `flock`，**锁被别人占住就一直等**；
/// 而这两条命令是阻塞档，一条卡住就占死一个 tokio worker，且 `cancel` 对 `spawn_blocking`
/// 是**空操作**（`inbound` 那条判据逐字：「`cancel` 会对它撒谎」）。
///
/// ⚠ 我的第一版是在 daemon 里等（先 `try_wait` 轮询、后 `recv_timeout`）——
/// **两版都被零定时器护栏当场逮住**，而它是对的：`IPC-PROTOCOL` 自己写着
/// 「daemon 侧刻意不管超时…零定时器铁律不改，**超时一律推给客户端**」。
///
/// ⇒ 正确形状是**让子进程自己有期限**：能找到 `timeout(1)` 就用它当前缀
///（`ccm` 里问 daemon 那条早就是这么写的，同一条纪律的另一侧）。
/// daemon 这边仍然只是老老实实 `wait` 一个**注定会退出**的子进程 —— 零计时器。
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
/// # ★★ 这一族环境键从此**由本模块显式交办**，不再靠继承〔`K-R26` 09-05〕
///
/// `plugin::invoke::run` 今天先 `env_clear()`、再按它自己那张白名单
/// （`plugin::invoke::INHERITED_ENV_KEYS`）喂 —— 那一刀关掉的是「daemon 进程内部的秘密
/// （常驻监听口的地址与令牌）顺着环境漏进插件」这条没人设计过的回程。
///
/// ⚠ **代价落在这里**：本插件族此前是靠**继承**拿到自己的配置的
/// （`CC_BUS_HOME` · `CCBUS_POLICY_MODE` · 不带 `from` 那一趟的 `CC_BUS_ID`），
/// 而本模块**一个都没显式交办过**。现打读数：只加 `env_clear()` 那一刀之后
/// `tests/e2e/daemon-cc-bus.sh` 从 `PASS=50 FAIL=0` 掉到 `PASS=31 FAIL=19`，
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
            "这条消息塞不进一次命令调用（内核的单参数上限是 128 KiB）—— 发短一点。\
             ⚠ 不是 cc-bus 坏了。"
                .to_string(),
        ),
        NotRun::Failed(msg) => ("failed", msg),
    })
}

/// 超时那条的说法 —— 两个命令共用一份文案。
fn timed_out_err() -> (String, String) {
    (
        "timed_out".to_string(),
        format!(
            "跑了超过 {} 秒还没退出，已被 timeout 命令结束。cc-bus 的投递走 flock —— \
             多半是锁被别的进程占住了（先看 `cc-list` 与 $CC_BUS_HOME 下的 *.lock）。",
            timeout_secs()
        ),
    )
}

/// 把 `cc-list` 的**人类可读表**变成结构化的行 —— 纯函数。
///
/// 今天的形状：表头 `ID TMUX 待读` + 每行 `id target 待读`。
/// id/target 都过 `[A-Za-z0-9_-]` 白名单消毒 ⇒ 不含空白，按空白分列是可靠的。
/// 「还没有登记的 agent」那行不足三列，自然落选。
pub(crate) fn parse_list(text: &str) -> Vec<serde_json::Value> {
    text.lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let id = it.next()?;
            let target = it.next()?;
            let unread = it.next()?;
            if id == "ID" {
                return None; // 表头
            }
            // 第三列不是数字 ⇒ 这不是数据行（宁可漏也别把表头/提示语当 agent）
            let unread: i64 = unread.parse().ok()?;
            Some(serde_json::json!({ "id": id, "target": target, "unread": unread }))
        })
        .collect()
}

/// `cc-agents` 状态列的三个字面量 —— **它们是 cc-bus 的输出契约，不是我们的枚举**。
///
/// 逐字取自 `src/shared/cc-bus/scripts/cc-agents`（09-13 现打）：`活` / `活?` / `已退`，
/// 它那张表的头注逐字写着为什么是三态而不是两态 ——「把『核不了』并进『活』是今天
/// 这一族所有事故的共同起点」。
///
/// ⚠ **拿输出当接口的代价就在这三个串上**：cc-bus 换个说法，这里认不出来 ⇒
/// 那一行**落选**（而不是被当成 `已退`）。落选的方向是刻意选的：宁可少报一个 spawn 记录，
/// 也不要把一个还活着的会话报成死的。理由与 [`parse_list`] 那句「宁可漏也别把表头当 agent」同一条。
const SPAWNED_LIVE: &str = "活";
const SPAWNED_UNVERIFIED: &str = "活?";
const SPAWNED_EXITED: &str = "已退";

/// `cc-agents` 状态列 → `bus-list` 那一套**同一个三态** —— 纯函数。
///
/// `true` / `false` / `null`，与 [`join_identity`] 的 `live` **逐字同一套语义**：
/// `null` = 「不知道」，不是「不在」。`cc-agents` 的 `活?` 正是「核不了」那一档
///（那份 spawn 台账没有身份列，借不到 pid 就核不动）⇒ 映到 `null`。
///
/// 认不出的串回 `None`（= 这不是数据行）。
fn spawned_live_of(state: &str) -> Option<serde_json::Value> {
    match state {
        SPAWNED_LIVE => Some(serde_json::Value::Bool(true)),
        SPAWNED_EXITED => Some(serde_json::Value::Bool(false)),
        SPAWNED_UNVERIFIED => Some(serde_json::Value::Null),
        _ => None,
    }
}

/// 把 `cc-agents` 的**人类可读表**变成结构化的行 —— 纯函数。
///
/// 今天的形状（`src/shared/cc-bus/scripts/cc-agents` 现打）：
/// 表头 `ID 状态 目录 初始任务` ＋ 每行 `printf '%-18s %-6s %-40s %s\n' id 状态 目录 任务`。
///
/// ⚠⚠ **这一层有一处认不准，写在这里而不是藏起来**：定宽 `printf` 的列之间只有空格，
/// **没有唯一分隔符** ⇒ 目录名里含空格时，第三列会只取到第一段，余下的并进 `task`。
/// id 与状态两列不受影响（id 过 `[A-Za-z0-9_-]` 白名单、状态是上面那三个字面量之一）。
/// ⇒ **要治它得给 cc-bus 加一条机器可读的输出**（`cc_bus_boundary_guard` 的诊断逐字就是
/// 「正确做法是给 cc-bus 加一条命令，不是绕到它背后读文件」），而那不在 `K-R113` 的写区里。
///
/// ⚠ 本函数**不回** spawn 时间：`cc-agents` 读了 `spawned.tsv` 的第 3 列却**不打印它**
///（现打）⇒ 命令面今天答不出这个字段。同上，要它就得给 cc-bus 加一条命令。
pub(crate) fn parse_spawned(text: &str) -> Vec<serde_json::Value> {
    text.lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let id = it.next()?;
            let state = it.next()?;
            if id == "ID" {
                return None; // 表头
            }
            // 第二列不是那三个状态之一 ⇒ 这不是数据行（`(还没 spawn 过会话)` 那句就落在这里）
            let live = spawned_live_of(state)?;
            let dir = it.next().unwrap_or_default().to_string();
            let task = it.collect::<Vec<&str>>().join(" ");
            Some(serde_json::json!({ "id": id, "live": live, "dir": dir, "task": task }))
        })
        .collect()
}

/// `cc-send` 的退出码 → **语义码** —— 纯函数。
///
/// `P4f-Y5`：如实转达，**不在 daemon 侧再写一份收件人白名单**。
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
pub(crate) fn classify_send(code: Option<i32>, detail: &str) -> Result<(), (String, String)> {
    match code {
        Some(0) => Ok(()),
        // cc-send 自己的白名单校验（`仅 [A-Za-z0-9_-]`）
        Some(2) => Err((
            "invalid_args".to_string(),
            format!("cc-send 拒绝了这个收件人：{detail}"),
        )),
        // 路由层拦截（ACL / 限流 / 去重 / 灭环），bus.log 里有 REJECT/THROTTLE 一行
        Some(3) => Err((
            "rejected".to_string(),
            format!("被路由层拦下（见 bus.log）：{detail}"),
        )),
        Some(TIMED_OUT_CODE) => Err(timed_out_err()),
        Some(c) => Err((
            "failed".to_string(),
            format!("cc-send 退出码 {c}：{detail}"),
        )),
        None => Err((
            "failed".to_string(),
            format!("cc-send 被信号打断：{detail}"),
        )),
    }
}

/// 形状校验：这组参数能不能构成一次有意义的调用。
///
/// ⚠ 与 `kill::parse_name` 同一条纪律：**这不是安全边界**（argv 直传不过 shell），
/// 更**不是**收件人合法性检查 —— 那归 cc-bus（见 [`classify_send`]）。
fn parse_send(args: &serde_json::Value) -> Result<(String, String, Option<String>), CmdErr> {
    let obj = args
        .as_object()
        .ok_or(("invalid_args", "args 不是对象".to_string()))?;
    let to = obj
        .get("to")
        .and_then(|v| v.as_str())
        .ok_or(("invalid_args", "缺 `to`".to_string()))?;
    let text = obj
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or(("invalid_args", "缺 `text`".to_string()))?;
    if to.trim().is_empty() {
        return Err(("invalid_args", "`to` 是空的".to_string()));
    }
    // ★ `from` 可选：**不给就是今天的行为**（cc-whoami 在 daemon 的处境里解不出身份 ⇒ `unknown`）。
    //   给了就以那个身份发 —— 收信人才知道是谁，回复才有地方去。
    //   ⚠ 合法性仍归 cc-bus（`cc-whoami` 自己会消毒成 `[A-Za-z0-9_-]`），这里只判形状。
    let from = obj
        .get("from")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    Ok((to.to_string(), text.to_string(), from))
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

/// 总线名单那一半 —— **`bus-list` 与 `bus-state` 走的是这同一个函数**〔`K-R113` 09-13〕。
///
/// 抽出来的理由不是省行数：两条命令各写一遍「转调 `cc-list` → 解析 → 挂身份空间」，
/// 就是同一份语义两处实现，而它们**必须逐字同形**（`bus-state` 的 `agents` 那一半
/// 若与 `bus-list` 有一个字段不同，调用方就得知道自己问的是哪一条命令）。
fn agents_via_cc_list() -> Result<Vec<serde_json::Value>, (String, String)> {
    let out = run("cc-list", &[]).map_err(|(c, m)| (c.to_string(), m))?;
    if out.timed_out() {
        return Err(timed_out_err());
    }
    if out.code != Some(0) {
        return Err((
            "failed".to_string(),
            format!("cc-list 退出码 {:?}：{}", out.code, out.diagnosis()),
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // ★ 去问**身份空间**谁还活着（用户 08-13 那条架构点）。问不到就回 `null`，不假装知道。
    let sessions = super::gate::list_sessions().ok();
    Ok(join_identity(parse_list(&text), sessions.as_deref()))
}

/// spawn 台账那一半 —— 转调 `cc-agents`。
fn spawned_via_cc_agents() -> Result<Vec<serde_json::Value>, (String, String)> {
    let out = run("cc-agents", &[]).map_err(|(c, m)| (c.to_string(), m))?;
    if out.timed_out() {
        return Err(timed_out_err());
    }
    if out.code != Some(0) {
        return Err((
            "failed".to_string(),
            format!("cc-agents 退出码 {:?}：{}", out.code, out.diagnosis()),
        ));
    }
    Ok(parse_spawned(&String::from_utf8_lossy(&out.stdout)))
}

pub(crate) fn list_for_inbound() -> Result<serde_json::Value, (String, String)> {
    Ok(serde_json::json!({ "agents": agents_via_cc_list()? }))
}

/// `bus-state`：**一次回全** —— 总线名单 ＋ spawn 台账〔`K-R113` 09-13〕。
///
/// # 为什么它是一条**具名读命令**，而不是让调用方读文件
///
/// monitor 侧 `cc_bus.rs::read_cc_bus_state` 今天走的是一条 shell 串（两次 `cat`
/// 拼一个分隔标记），它的头注逐字写着解锁条件是「**格式契约稳下来**」，届时
/// 「正确形状多半**不是**把 shell 串搬过去，而是 daemon 出一条**具名的读命令**」。
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
/// # ⚠ 它今天**答不出**哪几个字段（如实写，别让接线的人自己撞）
///
/// monitor 那一侧今天渲染的 `registered_at`（登记时间）· `spawned_at`（spawn 时间）·
/// `skipped`（坏行数）**本命令一个都没有** —— 不是漏了，是 cc-bus 的**命令面**答不出：
/// `cc-list` 不打印登记时间、`cc-agents` 读了 spawn 时间却不打印它、两者都**静默跳过**坏行。
/// 要它们，正确做法是**给 cc-bus 加一条机器可读的输出**，不是绕到它背后读那两份 `.tsv`
///（那条边界由 `cc_bus_boundary_guard` 钉着）。⇒ **接线那一件要先拿这一条去问产品**。
pub(crate) fn state_for_inbound() -> Result<serde_json::Value, (String, String)> {
    Ok(serde_json::json!({
        "agents": agents_via_cc_list()?,
        "spawned": spawned_via_cc_agents()?
    }))
}

/// 收件人**有没有人会读** —— `(在名单里吗, 会话活着吗)`。
///
/// `live` 与 `bus-list` 同一套三态：`true` / `false` / `null`（问不到身份空间，或不在名单里）。
/// ⚠ 任何一步失败都**不影响投递**：这是投完之后的"顺带说一句"，不是前置条件。
fn recipient_status(to: &str) -> (bool, serde_json::Value) {
    let Ok(out) = run("cc-list", &[]) else {
        return (false, serde_json::Value::Null);
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let rows = parse_list(&text);
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
/// # 为什么不是走 daemon 自己那条带三道门的 `kill`
///
/// 两者做的**不是同一件事**：`kill` 只杀 tmux 会话；`cc-kill` 还要清名册、清台账、
/// 清那个 id 的状态 —— 那是 cc-bus 的语义，只有它自己知道要清哪些文件。
///
/// # 门在哪
///
/// daemon 的 `kill` 用 §34 三道门，因为它的归属证据**弱**（名字前缀 / `@ccm_sid`）。
/// `cc-kill` 今天用的是**强证据**：`agents.tsv` 第 4 列（登记时记下的 pane 根进程 pid）
/// 与登记的完整地址一起核 —— 08-13 实测过不核的后果：**杀掉占了同名的无辜进程与会话**。
/// ⇒ 门住在懂那套语义的那一侧，不在这里重写一遍。
///
/// ⚠ 「多窗口要不要拦」是产品判断（`U18` 待裁）；今天照收，但 `cc-kill` 会把窗口数打出来。
pub(crate) fn kill_for_inbound(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (String, String)> {
    let id = args
        .as_object()
        .and_then(|o| o.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ("invalid_args".to_string(), "缺 `id`".to_string()))?
        .to_string();
    // ⚠ **不加 `--`**〔08-13 真跑撞出来的〕：`cc-send` 会解析旗标（所以那边要显式结束），
    //   而 `cc-kill` **不解析** —— 它直接取 `$1`。传了 `--` 的后果是它去杀一个名叫 `--`
    //   的 agent（`-` 在它的白名单里，连报错都不会），三种情形全回 `killed:false`
    //   而真 agent 的会话好好活着。★ 抄参数形状之前先看被调方怎么解析。
    let out = run("cc-kill", &[&id]).map_err(|(c, m)| (c.to_string(), m))?;
    let detail = out.diagnosis();
    match out.code {
        Some(0) => {}
        Some(TIMED_OUT_CODE) => return Err(timed_out_err()),
        // cc-kill 自己的白名单校验（非法 id）
        Some(2) => {
            return Err((
                "invalid_args".to_string(),
                format!("cc-kill 拒绝了这个 id：{detail}"),
            ))
        }
        Some(c) => {
            return Err((
                "failed".to_string(),
                format!("cc-kill 退出码 {c}：{detail}"),
            ))
        }
        None => {
            return Err((
                "failed".to_string(),
                format!("cc-kill 被信号打断：{detail}"),
            ))
        }
    }
    // ★ **回值要说清"到底动了什么"**：会话是被杀了，还是身份对不上只摘了登记？
    //   cc-kill 两种情况都 exit 0 —— 把它自己的说法读出来，别让调用方以为都一样。
    let said =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    let killed = said.contains("已杀会话");
    let stale_only = said.contains("已摘掉");
    Ok(serde_json::json!({
        "id": id,
        "killed": killed,
        // 身份对不上 ⇒ 只摘了陈旧登记，**会话与收件箱都没动**
        "stale_only": stale_only,
    }))
}

pub(crate) fn send_for_inbound(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (String, String)> {
    let (to, text, from) = parse_send(args).map_err(|(c, m)| (c.to_string(), m))?;
    // `--` 显式结束旗标：收件人万一以 `--` 开头也当收件人，不会被 cc-send 当成选项。
    let out = run_as("cc-send", &["--", &to, &text], from.as_deref()).map_err(|(c, m)| {
        // 只有这条路带得动大载荷 ⇒ 把**实测长度**补进去（诊断里给数，别让人自己去量）。
        if c == "too_long" {
            return (
                c.to_string(),
                format!("{m}（这条正文 {} 字节）", text.len()),
            );
        }
        (c.to_string(), m)
    })?;
    let detail = out.diagnosis();
    classify_send(out.code, &detail)?;
    // ★ 投出去之后，把「有没有人会读」也一并回答〔用@08-13 那条架构点的另一半〕。
    //
    // 病：今天两种「没人会读」都只回 `sent:true` —— ① 收件人**压根没登记**
    //（cc-send 会在 stderr 警告，但那句话到不了 daemon 的调用方）；
    // ② 登记过、**会话早没了**（cc-bus 那份名单会过期）。
    // ⇒ 投递照旧（先发后到是正当用法），但**说清楚**：`registered` + 三态 `live`。
    let (registered, live) = recipient_status(&to);
    Ok(serde_json::json!({
        "to": to, "sent": true, "registered": registered, "live": live,
        // 回显**以谁的身份发的**：不给 `from` 时是 `null`，那时收信人看到的是
        // cc-whoami 在 daemon 处境里解出来的东西（实测：`unknown`）——
        // 回显出来，调用方才看得见这件事，而不是等收信人来问「谁发的」。
        "from": from
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 〔`KR113D2` 09-13〕**本模块每一条「转调 shell 脚本」的裁定，逐条写成数据。**
    ///
    /// `(被调命令, 写面, 这一趟为什么不收进后端, 解锁条件)`
    ///
    /// # 为什么有这张表
    ///
    /// 定框 `K33` 逐字「**不要有什么 bash 脚本**」，而本模块今天**每一条**都在转调
    /// cc-bus 的脚本。`K-R111` 摸底把这件事登记为「后端那一侧也还有活」，
    /// `KR113D2` 逼出一次裁定：**收，还是不收，都要落成盘上看得见的东西**。
    /// 本轮裁的是**乙（不收）** —— 理由逐条写在下面第三栏，**不是一句「以后再说」**。
    ///
    /// 形状抄 `readonly_guard::spawn_registry::ALLOWED` 与 `cli_control::NOT_ON_CLI`
    ///（逐字：「把『为什么这条不上』写成**数据**，好让机检对着它比 —— 散文里说一遍，
    /// 下一个人加命令时看不见」）。
    ///
    /// # ⚠ 它与 `readonly_guard::g6_reach` 那一格**不是同一条规矩的两处住址**
    ///
    /// 那一格钉的是「只读铁律的豁免理由今天覆盖了哪几条被调命令」（`ALLOWED` 的键
    /// 分不出被调命令，那是它唯一的机器提醒）；本表钉的是「`K33` 对每一条各裁了什么」。
    /// 两者今天的成员集相同，**而都不许手抄** —— 各自从生产段现算。
    const TRANSCALLS: &[(&str, &str, &str, &str)] = &[
        (
            "cc-list",
            "只读",
            "cc-bus 的**名册与已读位置是它自己的私有格式**（`agents.tsv` ＋ `inbox/*.jsonl` \
             ＋ `state/*.pos`，`cc-list` 现打就是在做这个 join）。收进后端 = 把那份格式\
             再实现一遍 ⇒ 一处改、一处漏，而且错得静悄悄。用户 08-13 逐字「后面我可能要改ccbus」\
             ⇒ 今天焊进来的每一个字段，都是他改那天的一笔返工。",
            "cc-bus 那份文件格式**稳下来**，或者 cc-bus 本身被收成后端的一部分（那时它就不是\
             『外部插件』了，本表整张作废）。⚠ 解锁的判据不是『过了多久』，是**格式契约有没有定**。",
        ),
        (
            "cc-agents",
            "只读",
            "同 `cc-list`：它读的是 `spawned.tsv` ＋ 回头去 `agents.tsv` 借 pid 核身份。\
             **那次核身份正是不能重写的那一段** —— 08-13 的事故逐字记在它的头注里\
             （只按名字判活 ⇒ 会话名被重用 ⇒ 把占了同名的无辜进程当成 agent）。\
             强证据（登记时记下的 pane 根进程 pid）只在 cc-bus 那份名册的第 4 列里，\
             **命令面看不见它** ⇒ 后端重实现这一段只能退回弱证据，那是把已修的事故改回去。",
            "同上；另加一条**更便宜的中间路**：给 cc-bus 加一条机器可读的输出\
             （`cc_bus_boundary_guard` 的诊断逐字「要拿的东西命令给不出来时，正确做法是\
             **给 cc-bus 加一条命令**」）。那条路能一并治掉本模块 `parse_spawned` 那处\
             『目录名含空格就切不准』与『答不出 spawn 时间』。",
        ),
        (
            "cc-send",
            "写：收件人的收件箱（`inbox/<id>.jsonl`）",
            "投递这条路上住着**路由层**（ACL / 限流 / 去重 / 灭环）与 `flock`，\
             而它们是用户自己在改的东西（`CCBUS_RATE_*` / `CCBUS_DEDUP_WINDOW` / `CCBUS_TTL` …）。\
             后端重实现一份 = 两套路由规则同时在跑，而**被拦的那一条不会报错、只会不见**。",
            "同 `cc-list`。⚠ 若哪天要收，**先收读面再收写面** —— 写面出错是不可见的。",
        ),
        (
            "cc-kill",
            "破坏性：杀会话 ＋ 进程树，清名册 · 清台账 · 清那个 id 的状态",
            "它做的事**比后端自己那条 `kill` 多**（后者只杀 tmux 会话），而多出来的那几样\
             全是 cc-bus 的私有文件。更要紧的是它的**门**：`agents.tsv` 第 4 列那个 pid \
             ＋ 登记的完整地址一起核 —— 08-13 实测过不核的后果是杀掉无辜进程与会话。\
             ⇒ 门要住在懂那套语义的一侧，本模块不重写一遍。",
            "同 `cc-agents`：pid 那一列能从命令面拿到的那天。**这一条是四条里最后收的**\
             —— 破坏性动作的门重写错一次的代价，本仓已经付过。",
        ),
    ];

    /// 从**生产段**现算：本模块今天经 [`crate::plugin::invoke`] 转调了哪几条 cc-bus 命令。
    ///
    /// 取法与 `readonly_guard::g6_reach` 那一格**刻意相同**（`run("` / `run_as("` 之后
    /// 那一个字符串字面量）—— 两处认的是同一件事实，取法不同才会各说各话。
    fn transcalled_today() -> Vec<String> {
        let prod = crate::guard_support::production_code(include_str!("cc_bus.rs"));
        let mut out: Vec<String> = Vec::new();
        for opener in ["run(\"", "run_as(\""] {
            let mut from = 0usize;
            while let Some(k) = prod[from..].find(opener) {
                let at = from + k + opener.len();
                let end = prod[at..]
                    .find('"')
                    .expect("被调命令的字面量没有闭合 —— 抽取坏了");
                out.push(prod[at..at + end].to_string());
                from = at + end;
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// ★★ `KR113D2`〔09-13 裁**乙**：这一趟不收〕：**每一条转调，都要有一条写在盘上的裁定。**
    ///
    /// # 它逮哪三形
    ///
    /// ① **登记被删掉** ⇒ 那条转调没有裁定了 —— 下一个人只会看见一句「这里在调 shell」，
    ///    看不见「这是裁过的」，于是要么当成待办去动它，要么当成默认继续躺着。
    /// ② **新加一条转调而不裁** ⇒ `K33` 那条定框上又多一笔债，而没有任何人被通知。
    /// ③ **裁定过期**（表里那条命令今天已经不转调了）⇒ 幽灵条目
    ///    （`spawn_registry` 那张表里躺过一条 13 天的幽灵，形状逐字相同）。
    ///
    /// # ⚠ 它买不到什么
    ///
    /// **不判那条理由说得对不对**（`writing.md`：住址级判得了，语义判不了）。
    /// 它买到的是「这一族里没有一条是**没人裁过**就躺在那儿的」。
    #[test]
    fn every_shelled_out_command_carries_a_written_ruling() {
        let today = transcalled_today();
        // 反空真地板：抽取塌了的话，下面两个差集都会是空的，本条会零命中地绿。
        assert!(
            today.len() >= 3,
            "生产段只抠到 {} 条转调（地板 3，实测 4）—— 抽取塌了，本条此刻在空转：{today:?}",
            today.len()
        );
        let ruled: Vec<&str> = TRANSCALLS.iter().map(|(c, ..)| *c).collect();
        let unruled: Vec<&String> = today
            .iter()
            .filter(|c| !ruled.contains(&c.as_str()))
            .collect();
        assert!(
            unruled.is_empty(),
            "这几条转调 cc-bus 脚本的命令**没有一条写在盘上的裁定**：{unruled:?}\n\
             定框 `K33` 逐字「不要有什么 bash 脚本」⇒ 每一条要么收进后端，要么在 `TRANSCALLS` \n\
             里写明「写面 · 这一趟为什么不收 · 解锁条件」。写「以后再说」不算理由。"
        );
        let ghosts: Vec<&&str> = ruled
            .iter()
            .filter(|c| !today.contains(&c.to_string()))
            .collect();
        assert!(
            ghosts.is_empty(),
            "`TRANSCALLS` 里这几条**今天已经不转调了**：{ghosts:?} —— 裁定过期了，\n\
             收进后端的那一拍要**同轮**把它这一行删掉（不删就是一条幽灵，\n\
             下一个人会照着一份不成立的理由做判断）。"
        );
        for (cmd, face, why, unlock) in TRANSCALLS {
            assert!(
                !face.trim().is_empty(),
                "`{cmd}` 没写写面 —— 那一栏正是只读铁律那条豁免理由要引用的东西"
            );
            assert!(
                why.chars().count() >= 40,
                "`{cmd}` 的理由只有 {} 字 —— 那是占位不是理由",
                why.chars().count()
            );
            assert!(
                unlock.chars().count() >= 20,
                "`{cmd}` 没写解锁条件 —— 没有解锁条件的裁定会一直躺着，\
                 而『躺着』与『裁过』在盘上长得一模一样"
            );
        }
    }

    /// ★ `KR113D1`：`bus-state` **一次回全**，而且 `agents` 那一半与 `bus-list` **同源**。
    ///
    /// 钉的是**数据流**，不是「函数存在」：两条命令必须落到同一个 `agents_via_cc_list`，
    /// 否则 `bus-state.agents` 与 `bus-list.agents` 会各自漂。
    /// ⚠ 起子进程那一步在沙箱里跑不了（没装 cc-bus）⇒ 本条扫的是生产段的**接线**，
    /// 真跑由 `tests/e2e/daemon-cc-bus.sh` 那一族负责。
    #[test]
    fn bus_state_answers_both_halves_from_one_call() {
        let prod = crate::guard_support::production_code(include_str!("cc_bus.rs"));
        let body = prod
            .split("pub(crate) fn state_for_inbound")
            .nth(1)
            .expect("`state_for_inbound` 不在生产段里了 —— `bus-state` 的本体没了");
        let body = &body[..body.find("\n}").expect("函数体没有收尾 —— 抽取坏了")];
        for half in ["agents_via_cc_list()", "spawned_via_cc_agents()"] {
            assert!(
                body.contains(half),
                "`state_for_inbound` 里没有 `{half}` —— 「一次回全」少了一半。\n\
                 少的那一半会让调用方拿到一份**看上去完整**的答案（`spawned: []` 与\n\
                 「问不到」在它那儿长得一模一样）。"
            );
        }
        assert!(
            prod.contains("fn list_for_inbound() -> Result<serde_json::Value, (String, String)> {\n    Ok(serde_json::json!({ \"agents\": agents_via_cc_list()? }))"),
            "`bus-list` 不再走 `agents_via_cc_list` 了 —— 两条命令的 `agents` 从此会各漂各的"
        );
    }

    /// ★ `cc-agents` 那张表的三态，逐态一格；**认不出的行落选，不当成「已退」**。
    #[test]
    fn parse_spawned_reads_the_table_and_keeps_the_three_states() {
        let text = "ID                 状态   目录                                     初始任务\n\
                    proj_cc            活     /home/zbl/proj                           跑门禁\n\
                    ghost_cc           已退   /home/zbl/ghost                          收尾\n\
                    old_cc             活?    /home/zbl/old                            \n";
        let got = parse_spawned(text);
        assert_eq!(got.len(), 3, "表头没被跳过或数据行丢了：{got:?}");
        assert_eq!(got[0]["id"], "proj_cc");
        assert_eq!(got[0]["live"], json!(true));
        assert_eq!(got[0]["dir"], "/home/zbl/proj");
        assert_eq!(got[0]["task"], "跑门禁");
        assert_eq!(got[1]["live"], json!(false));
        // ★ 「核不了」必须是 `null`，不是 `false` —— 与 `bus-list` 的 `live` 同一套三态。
        //   把这一档并进「已退」正是 cc-bus 自己头注里记着的那族事故的共同起点。
        assert_eq!(got[2]["live"], serde_json::Value::Null);
        assert_eq!(got[2]["task"], "");
    }

    #[test]
    fn non_data_lines_never_become_spawned_rows() {
        for line in [
            "(还没 spawn 过会话)",
            "ID 状态 目录 初始任务",
            "",
            "只有一列",
            "some_id 不是状态 /d 任务",
        ] {
            assert!(
                parse_spawned(line).is_empty(),
                "这行不该被当成 spawn 记录：{line:?}"
            );
        }
    }

    /// ★ 三态的**字面量**取自 cc-bus 的输出，不是我们自己发明的枚举 —— 三者互不相同。
    ///
    /// ⚠ 钉「互不相同」而不是逐条对字符串：把 `活?` 并进 `活`，这一格才是它要拦的东西。
    #[test]
    fn the_three_spawned_states_stay_three_different_answers() {
        let live = spawned_live_of(SPAWNED_LIVE).expect("`活` 认不出来了");
        let unver = spawned_live_of(SPAWNED_UNVERIFIED).expect("`活?` 认不出来了");
        let exited = spawned_live_of(SPAWNED_EXITED).expect("`已退` 认不出来了");
        assert_ne!(live, unver, "「活着」与「核不了」必须分得开");
        assert_ne!(exited, unver, "「已退」与「核不了」必须分得开");
        assert_ne!(live, exited);
        assert!(
            spawned_live_of("活着").is_none(),
            "认不出的状态串必须落选（回 None），不许猜一个具体答案"
        );
    }

    #[test]
    fn parse_list_reads_the_table_and_skips_everything_else() {
        let text = "ID           TMUX               待读\n\
                    agent-communication_cc agent-communication_cc:0.0 0\n\
                    x_cc         x_cc:0.0           2\n";
        let got = parse_list(text);
        assert_eq!(got.len(), 2, "表头没被跳过或数据行丢了：{got:?}");
        assert_eq!(got[0]["id"], "agent-communication_cc");
        assert_eq!(got[0]["unread"], 0);
        assert_eq!(got[1]["target"], "x_cc:0.0");
        assert_eq!(got[1]["unread"], 2);
    }

    /// ★ 超长 id 会把固定宽度的列**挤在一起** —— 实测那时列间仍有一个空格，
    /// 所以按空白分列仍然对。这一格钉的就是「挤了也还认得出来」。
    #[test]
    fn a_long_id_that_overflows_the_column_is_still_parsed() {
        let one = parse_list("verylongagentname_that_overflows verylongagentname:0.0 7\n");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0]["unread"], 7);
    }

    #[test]
    fn non_data_lines_never_become_agents() {
        for line in [
            "(还没有登记的 agent)",
            "ID TMUX 待读",
            "",
            "只有两列 x",
            "id target 不是数字",
        ] {
            assert!(
                parse_list(line).is_empty(),
                "这行不该被当成 agent：{line:?}"
            );
        }
    }

    /// `P4f-Y4`：找不到时要说**查过哪儿**。
    ///
    /// ⚠ 拼那句话的活搬去通用口了，**这一格没跟着搬**：它核的是 cc-bus 自己那三样
    ///（两处固定位置的形状 + [`NOT_INSTALLED_HINT`] 这句尾巴），而 `tests/e2e/daemon-cc-bus.sh` 的
    /// 第 6 组逐字 `grep` 的正是这三条。搬走它等于把那三条 e2e 的单测对位丢掉。
    #[test]
    fn the_not_installed_message_names_the_places_it_looked() {
        let home = PathBuf::from("/home/u");
        let fixed = fixed_candidates(None, Some(&home), "cc-list");
        assert_eq!(fixed.len(), 2, "固定位置应当是两处：{fixed:?}");
        let msg = crate::plugin::discover::not_installed_message(
            "cc-list",
            &fixed,
            9,
            NOT_INSTALLED_HINT,
        );
        assert!(msg.contains("/home/u/.local/bin/cc-list"), "{msg}");
        assert!(
            msg.contains(".claude/skills/cc-bus/scripts/cc-list"),
            "{msg}"
        );
        assert!(msg.contains("9 个目录"), "PATH 那半没说：{msg}");
        assert!(msg.contains("CC_BUS_BIN_DIR"), "没告诉人怎么指过去：{msg}");
    }

    #[test]
    fn the_override_dir_wins_over_the_fixed_places() {
        let over = PathBuf::from("/opt/ccbus");
        let home = PathBuf::from("/home/u");
        let fixed = fixed_candidates(Some(&over), Some(&home), "cc-send");
        assert_eq!(fixed[0], PathBuf::from("/opt/ccbus/cc-send"));
        assert_eq!(fixed.len(), 3);
    }

    /// `P4f-Y5`：三档退出码各自映射成**不同**的语义码。
    ///
    /// ⚠ 判据钉「互不相同」而不是逐条对字符串 —— 前者才是这条 DoD 的内容
    /// （把两档并成一个码，用户就分不出「名字写错了」和「被 ACL 拦了」）。
    #[test]
    fn the_three_exit_codes_map_to_three_different_meanings() {
        assert!(classify_send(Some(0), "").is_ok());
        let bad = classify_send(Some(2), "x").unwrap_err().0;
        let rej = classify_send(Some(3), "x").unwrap_err().0;
        let other = classify_send(Some(9), "x").unwrap_err().0;
        let killed = classify_send(None, "x").unwrap_err().0;
        assert_ne!(bad, rej, "「收件人非法」与「被路由层拦」必须分得开");
        assert_ne!(bad, other);
        assert_ne!(rej, other);
        assert_eq!(killed, other, "被信号打断与其它失败同档（都是 failed）");
    }

    /// `P4f-Y5` 的另一半：daemon **不重复校验收件人合法性**。
    ///
    /// 传一个 cc-bus 自己会拒的名字（含 `/`），本侧必须**放行到 cc-send 那一步**——
    /// 由它去拒（rc=2 → `invalid_args`）。这样白名单只有一份。
    #[test]
    fn the_daemon_does_not_re_implement_the_recipient_charset_rule() {
        let ok = parse_send(&json!({ "to": "a/b", "text": "x" }));
        assert!(
            ok.is_ok(),
            "本侧不该判收件人字符集 —— 那会造出第二份规则：{ok:?}"
        );
        // 形状还是要判的（这不是安全边界，是「能不能构成一次有意义的调用」）
        for bad in [
            json!({}),
            json!({ "to": "x" }),
            json!({ "text": "x" }),
            json!({ "to": "", "text": "x" }),
            json!({ "to": "  ", "text": "x" }),
            json!({ "to": 1, "text": "x" }),
        ] {
            assert!(parse_send(&bad).is_err(), "形状不对却放行了：{bad:?}");
        }
    }
}
