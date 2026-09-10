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
//! 本模块经那一处转调的外部命令**今天是三条**，逐条说清写面：
//! · `cc-list` **只读**；
//! · `cc-send` 会写收件人的收件箱；
//! · ★ `cc-kill` 是**破坏性**的 —— 它杀会话，还要清名册、清台账、清那个 id 的状态。
//!
//! 三条都是**被起的那个进程**在写，与用户自己在终端里敲同一条命令没有区别
//!（同 `launch` 起 claude 的 D1 正例：收窄后的铁律管的是 **daemon 进程自身**不写用户既有数据）。
//!
//! ⚠⚠ 这段话此前逐字写着「**两条命令**共用」，而 `cc-kill` 是 08-13 当天稍晚进来的
//!（那个 commit 动了 8 个文件，`readonly_guard.rs` 不在其中）⇒ 那条豁免理由**漏掉了今天真实的写面**，
//! 而三条判据全绿 —— 起进程登记的键里程序名是 `<非字面量>`，三条命令**共用同一个键**，
//! 加第三条不会红。这一句与 `ALLOWED` 那一行的理由**同轮一起订正**。

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
/// `e2e/daemon-cc-bus.sh` 从 `PASS=50 FAIL=0` 掉到 `PASS=31 FAIL=19`，
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
    sessions: Option<&[(String, String, String)]>,
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
                Some(list) => match list.iter().find(|(n, _, _)| *n == name) {
                    None => (serde_json::Value::Bool(false), serde_json::Value::Null),
                    Some((_, _, ccm_sid)) => (
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

pub(crate) fn list_for_inbound() -> Result<serde_json::Value, (String, String)> {
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
    Ok(serde_json::json!({
        "agents": join_identity(parse_list(&text), sessions.as_deref())
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
    ///（两处固定位置的形状 + [`NOT_INSTALLED_HINT`] 这句尾巴），而 `e2e/daemon-cc-bus.sh` 的
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
