//! A2：多账号（账号库：每个号一个配置目录，共享项链回同一个配置根）的**只读**消费侧（写侧是 `accounts/manage/`）。
//!
//! - `--list-accounts [--accts-dir <p>]`
//!   → 第 1 行 `{"kind":"accounts-meta",…}`，其后每账号一行 JSON。
//! - `--session-accounts [--accts-dir <p>]`
//!   → 每条运行中会话一行：它的 `CLAUDE_CONFIG_DIR` 属于哪个账号。
//! - `--account-trust <configDir> <cwd> [--accts-dir <p>]`
//!   → 单行 `{"trusted":bool,"known":bool}`：目标账号是否已信任该目录
//!   （换号 resume 前的预检——首次用某账号进某目录，CC 会弹信任确认，会卡住编排）。
//! - `--account-trust-zero <cwd>`
//!   → 同上，但问的是**账号 0**（见下）。它没有 configDir，`.claude.json` 在 `$HOME`。
//!
//! # Z01：账号 0
//! manifest 里 `configDir` **键缺席**的那一条 = 账号 0 =「不设 `CLAUDE_CONFIG_DIR`」
//! 这个状态本身。本模块**结构性**地认它（看键在不在），**不认名字**——不在 Rust 里
//! 硬编码 "0"。它的 config dir 是共享库（`sharedStore`）、`.claude.json` 在 `$HOME`。
//! **空串不算缺席**：`is_safe_config_dir("")` 会挡掉它（空值 ≠ 未设）。
//!
//! # ⚠ 谓词的作用面**不对称**，这里如实写下〔audit-0805 08-06 抽样核实〕
//!
//! `is_safe_config_dir` 今天有三处调用点，全部在**清单侧**（`--list-accounts` /
//! `--account-trust` 那几条路：源是 accounts manifest）。而 `--session-accounts`
//! 那条路的 `configDir` 来自 **`/proc/<pid>/environ`**（`proc_claude_config_dir`），
//! **没有过谓词**就作为帧字段发给 monitor。
//!
//! ## 为什么**不**顺手给它套上谓词
//!
//! 逐条量过后果，套上去**更糟**：
//! - **归属那一半已经是安全的** —— 进程侧的值只拿去与 `by_dir` 比对，
//!   而 `by_dir` 只装清单侧、已过谓词的目录 ⇒ 不安全的值匹配不上，`account` 恒 `None`；
//! - 而若把不安全值直接丢弃（置 `None`），那条会话就会变成 `bare: true` ——
//!   **语义上等同于「账号 0」**，于是一个可疑会话反而被贴成默认账号。
//!   那不是收紧，是把一种坏结果换成另一种更坏的。
//! - 真要处理，得给帧加一个「configDir 不可信」的状态位 ——
//!   那是**改上线契约**（D6：暴露给第三方 = 契约冻结成本），不属本区范围（只修缺陷，不加能力）。
//!
//! ⇒ 结论：**归属安全、展示未净化**。登记在，
//! 解锁条件 = 帧契约允许新增状态位时，把「不可信的 configDir」表达成一个显式状态，
//! 而不是让它退化成 `bare`。

//!
//! 输出协议同 `history_query`：每行一个 JSON 对象（**不是** wire::Frame）。
//! 成功 exit 0；`--account-trust` 的硬错误 exit 2 + stderr 纯 `{code,message}` JSON
//! （照 `resolve_query` 的结构化错误约定，客户端可整段 parse）。
//!
//! # 只读铁律（src/doc/INVARIANTS.md §1）
//! 本模块只 `read` / `read_dir` / `metadata`，**零写入**，且**不 shell out**
//! （backend 是非登录 shell、PATH 很瘦；直接读 manifest 文件即可，省掉 PATH 依赖
//! 与"让只读组件去跑写工具"的争议面）。
//!
//! # 凭据边界（本模块最重要的约束）
//! - `.credentials.json` **只 stat 存在性，绝不读内容**。
//! - `.claude.json` 只取 `projects[<cwd>].hasTrustDialogAccepted` 一个布尔；
//!   **绝不回传文件内容**——那里面有 `mcpServers` 的环境变量（可能含 API key）。
//! - `/proc/<pid>/environ` 只抠**三个写死的键**（`CLAUDE_CONFIG_DIR` · `CCM_LAUNCH_ID` ·`ANTHROPIC_BASE_URL`），
//!   **不回传整个环境快照**。`ANTHROPIC_BASE_URL` 的值带中转钥匙 ⇒ **只折成一个布尔**（`viaRelay`：是不是本机中转那一形地址），
//!   值本身不出参、不进日志。
//!   ⚠ `K-P5f` 加第二个键那一拍要求把「两个键」与「整个快照」的界说清楚，界在这里：
//!   **键名是本文件里的两个常量**（`paths::CONFIG_DIR_ENV` 与 [`LAUNCH_ID_ENV`]），
//!   **不接受任何调用方传进来的键名**。一旦键名成为一维参数，这条查询就退化成
//!   「任意环境变量读」原语 —— 与 `--account-trust` 那条「`configDir` 必须 ∈ manifest，
//!   否则就是任意文件读」是同一形的退化。⇒ 判据 `the_only_env_keys_this_module_reads_are_the_two_named_constants`
//!   数着本文件生产段里 `proc_env_var(` 的调用点，**多一处 ⇒ 红**。
//! - `--account-trust` 的 `configDir` 必须逐字等于 manifest 里某个账号的 `configDir`，
//!   否则拒绝——避免它退化成"任意文件读"原语。`--account-trust-zero` 不收路径参数
//!   （路径是 `$HOME/.claude.json`，写死在代码里），所以它连这个面都没有。

use acct_core::{
    auth_kind_from_manifest, auth_kind_with_apikey_table, auth_ready, ACCTS_DIR_NAME,
    CREDENTIALS_NAME, MANIFEST_NAME, SUPPORTED_SCHEMA,
};
use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// manifest 读取上限。账号数有限，几 MB 足矣，此处宽松给 8MB 兜底。
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
/// 单个 `sessions/<PID>.json` 读取上限（正常几百字节）。
const MAX_SESSION_FILE_BYTES: u64 = 1024 * 1024;
/// `sessions/*.json` 扫描上限，防病态目录拖垮一次性查询。
const MAX_SESSION_FILES: usize = 500;

// ---------------------------------------------------------------- manifest

#[derive(serde::Deserialize)]
struct RawAccount {
    name: String,
    #[serde(default)]
    email: Option<String>,
    /// **Z01：可以缺席。** 缺席 = 账号 0 =「不设 `CLAUDE_CONFIG_DIR`」这个状态本身，
    /// 它的 config dir 就是共享库。**判据是结构性的（这个键在不在），不认名字**——
    /// 不在这里硬编码 "0"，manifest 想叫它什么都行。
    /// 空串**不算缺席**：`is_safe_config_dir("")` 会把它挡掉（空值 ≠ 未设）。
    #[serde(rename = "configDir", default)]
    config_dir: Option<String>,
    #[serde(rename = "isDefault", default)]
    is_default: bool,
    #[serde(default)]
    mode: Option<String>,
    /// **K-A1：鉴权方式。可以缺席** —— 缺席 = 旧 manifest = 订阅号
    /// （裁决与排除见件计划 `KA6d`；分类规则的唯一住址是
    /// `acct_core::auth_kind_from_manifest`，**这里不许再写一份 match**）。
    #[serde(rename = "authKind", default)]
    auth_kind: Option<String>,
}

struct Manifest {
    updated_at: Option<String>,
    shared_store: Option<String>,
    accounts: Vec<RawAccount>,
}

/// 路径是否可安全地交给下游（cc-monitor 会把 configDir 拼进 `export CLAUDE_CONFIG_DIR='…'`）。
/// ⚠ `N-F1c`（09-05）之后**不再是整套同一**：`\` 从本函数的拒绝集里拿出来了（Windows 的路径
/// 分隔符就是它），而上面那一侧照旧拒。这不是漂移，是**分层校验** ——
/// monitor 把 `configDir` 拼进 POSIX 命令之前要过 `config_dir_command_safe`，那个函数明确拒 `\`。
/// 〔旧文逐字，留作来历：「同一套字符集——两端对齐」——「整套同一」今天不成立。〕
/// ⚠⚠ 上面那个名字**在本文件里只许出现一次** —— `structural_scan::INVENTORY` 按**处数**钉着它
/// （PM 09-05 改这段注释时多写了一次，当场红：「盘上 2 处，登记表写 1 处」）。
/// 允许普通空格与常规非 ASCII（如中文；单引号内无害且常见），拒绝引号/命令替换/
/// 重定向/通配/控制字符 + 视觉欺骗类 Unicode。
pub(crate) fn is_safe_config_dir(p: &str) -> bool {
    // 规则整份搬进 `acct_core::config_dir_ok`（全仓唯一一份）：
    // 后端 `control/ccm` 起会话也要这张全表，而 `control → observe` 是禁止方向 —— 住共享 crate 两边都够得着。
    // 原先这里的拆法（`N-F1c`：「平台无关的安全性质（拒绝集）＋ 平台相关的形式」，以及为什么要认 Windows 形 ——
    // monitor 的本机账号清单也来问这个二进制，Windows 的账号目录是 `C:\Users\…`）随规则一起搬过去了，理由原样写在那边。
    acct_core::config_dir_ok(p)
}

/// 去掉尾部 `/`，让 manifest 里的路径与 `/proc` 环境变量里的写法能对上。
fn norm_dir(p: &str) -> &str {
    let t = p.trim_end_matches('/');
    if t.is_empty() {
        "/"
    } else {
        t
    }
}

fn home_dir() -> Option<PathBuf> {
    crate::platform::paths::home_dir()
}

/// 账号库目录：`--accts-dir <p>` > `$HOME/.claude-alt`。
fn resolve_accts_dir(args: &[String]) -> PathBuf {
    if let Some(i) = args.iter().position(|a| a == "--accts-dir") {
        if let Some(p) = args.get(i + 1) {
            if !p.is_empty() {
                return PathBuf::from(p);
            }
        }
    }
    match home_dir() {
        Some(h) => h.join(ACCTS_DIR_NAME),
        None => PathBuf::from(ACCTS_DIR_NAME),
    }
}

fn manifest_path(accts_dir: &Path) -> PathBuf {
    accts_dir.join(MANIFEST_NAME)
}

/// **这台机器上**那份账号 manifest 在哪（没有 `--accts-dir` 时的那条解析，
/// 与 `--list-accounts` 读的是同一份）。watcher 盯着它、变了发 `accounts_changed`。
pub(crate) fn default_manifest_path() -> PathBuf {
    manifest_path(&resolve_accts_dir(&[]))
}

/// 读 + 解析 manifest。缺文件/坏 JSON/不支持的 schema 都是 `Err(人话原因)`——
/// 调用方据此输出 `enabled:false` 而**不是**失败退出（"没启用多账号"是正常状态）。
///
/// 账号数组**逐条**解析：单个坏账号（缺 name/configDir 等）被跳过而非拖垮整份
/// manifest（避免手改 manifest 时一坏全灭；写侧 `accounts/manage/model.rs` 对这种条目也是原样留着、不去动它）。
fn load_manifest(accts_dir: &Path) -> Result<Manifest, String> {
    let p = manifest_path(accts_dir);
    let bytes = read_regular_capped(&p, MAX_MANIFEST_BYTES).map_err(|e| {
        copy_text(
            "beAccountsQuery.loadManifest.unreadable",
            &[("path", &(p.display()).to_string()), ("e", &e.to_string())],
        )
    })?;
    // UTF-8 BOM 剥掉再解析：PowerShell 5.1 `-Encoding UTF8` 与记事本默认写 BOM，
    //   `serde_json` 不吃它 ⇒ 不剥就是整份「不是合法 JSON」、账号页整块空（`control/ccm/plan.rs::AccountTable::load`
    //   09-21 修过同一份文件的另一个读者；两个读者读出同一张表由 `tests::both_readers_of_the_manifest_see_the_same_accounts` 钉）。
    let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    let root: serde_json::Value = serde_json::from_slice(body).map_err(|e| {
        copy_text(
            "beAccountsQuery.loadManifest.badJson",
            &[("e", &e.to_string())],
        )
    })?;
    match root.get("version").and_then(|v| v.as_u64()) {
        Some(SUPPORTED_SCHEMA) => {}
        Some(v) => {
            return Err(copy_text(
                "beAccountsQuery.loadManifest.badVersion",
                &[("v", &v.to_string())],
            ))
        }
        None => return Err(copy_text("beAccountsQuery.loadManifest.noVersion", &[]).into()),
    }
    let mut accounts = Vec::new();
    if let Some(arr) = root.get("accounts").and_then(|a| a.as_array()) {
        for (i, item) in arr.iter().enumerate() {
            match serde_json::from_value::<RawAccount>(item.clone()) {
                Ok(a) => accounts.push(a),
                Err(e) => tracing::warn!("manifest 第 {i} 个账号解析失败，已跳过：{e}"),
            }
        }
    }
    Ok(Manifest {
        updated_at: root
            .get("updatedAt")
            .and_then(|v| v.as_str())
            .map(String::from),
        shared_store: root
            .get("sharedStore")
            .and_then(|v| v.as_str())
            .map(String::from),
        accounts,
    })
}

fn json_str(v: Option<&str>) -> serde_json::Value {
    match v {
        Some(s) => serde_json::Value::String(s.to_string()),
        None => serde_json::Value::Null,
    }
}

// ---------------------------------------------------------------- /proc

// U2：**这里原本有第二份 `proc_starttime`**（`/proc/<pid>/stat` 的 field 22）。
// 与 `platform::proc::proc_starttime` 逐字同语义（都返回 boot 起的 jiffies，解析都是
// `nth(22-3)`），只是这一份把解析内联了、那一份走 `parse_starttime_from_stat`。
// 合并前**逐条核过单位**：单位不同的话它们就不是重复，合并就是引 bug。
use crate::agents::claudecode::accounts as cc_accounts;
use crate::common::fs::read_regular_capped;
use crate::platform::proc::{proc_env_var, proc_starttime, EnvRead};

/// 从 pidfile 字节里取 `procStart`（CC 写的是 starttime ticks 的十进制字符串；容忍裸数字）。
fn parse_procstart_ticks(v: &serde_json::Value) -> Option<u64> {
    let field = v.get("procStart")?;
    if let Some(s) = field.as_str() {
        return s.trim().parse::<u64>().ok();
    }
    field.as_u64()
}

/// 该 pidfile 记录的进程**当前是否仍是同一个进程**（防 PID 复用 → 误归属账号）。
/// 严于 watcher 的判活：这里的结果直接喂给"按会话切账号"，**错标签比缺标签危害大**，
/// 所以要求 pidfile 的 `procStart` 与当前 `/proc/<pid>` 的 starttime **精确相等**才认。
/// 缺 `procStart`（老 pidfile）或读不到 starttime（进程已死）→ 不认（宁缺毋错）。
fn session_process_identity_ok(pid: u32, pidfile: &serde_json::Value) -> bool {
    match (parse_procstart_ticks(pidfile), proc_starttime(pid)) {
        (Some(recorded), Some(current)) => recorded == current,
        _ => false,
    }
}

// ------------------------------------------------- K-P5f：身份 token 读回来那一侧

/// cc-monitor 起会话时铸进下一跳进程环境的**身份 token**〔`K-P5b` 写侧，`K-P5f` 读侧〕。
///
/// # 🔴 双写点，且**共享不了常量** —— 界在这里说清楚
///
/// 写侧的家是 `src/backend/control/launch_render/local.rs::LAUNCH_ID_VAR`（原在 monitor），而
/// `src/backend` 是**另一个 crate、另一份 `Cargo.lock`**（`src/frontend/shell/Cargo.toml`
/// 的 workspace members 里逐字没有它）⇒ 两侧不可能 `use` 同一个 `const`。
/// 与 `CREDENTIALS_NAME` 那个双写点（Rust ↔ bash）同形，处置也照它：
/// **由测试对拍**（[`tests::the_launch_id_env_var_matches_the_monitor_side_home`]， 〔散文墓碑〕
/// `include_str!` 直接读 monitor 那份源码）。**改这里必须改那边，反之亦然。**
///
/// # ⚠ 它**不住** `agents/claudecode/paths.rs`，这不是疏忽
///
/// 那一层装的是「**Claude** 的目录布局与环境变量」（`CONFIG_DIR_ENV` 住那儿是对的：
/// 那是 Claude 认的变量）。而本变量是 **cc-monitor 自己**铸的 token，Claude 一个字都不认
/// ⇒ 把它塞进 `agents/claudecode/` 会让那一层多出一件不属于它的知识。
/// 今天读它的只有本文件这一处，家就设在这里。
const LAUNCH_ID_ENV: &str = "CCM_LAUNCH_ID";

/// 身份 token 的字符集 —— **fail closed**，形状不对就不往下游递。
///
/// 与铸法那一侧同一条：段闸 `relay_route_core::segment_is_safe`（从 monitor `payload.rs` 搬进共享 crate）逐字是
/// 「只许字母数字与 `-` `_`，1..=128 字节」，而 `route_key_for_session` 铸出来的
/// 要么是 UUID v4（`[0-9a-f-]`，36 字节）、要么是过了那条白名单的 sid ⇒ 两种都在集内。
///
/// # 为什么读回来还要再核一次（"来源可信"不是放行的理由）
///
/// 这个值来自 `/proc/<pid>/environ`，而**谁都能 `export CCM_LAUNCH_ID=…` 再起 claude** ——
/// 它是本模块唯一一个**任意用户可控**的出参。同 `identity_tag::sid_is_safe` 那条头注
/// 逐字记的纪律：「本仓栽过的那些坑里，最贵的一类就是『这个值不可能有问题』」。
fn launch_id_is_safe(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 128
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 一条会话在出参里的全部字段（`--session-accounts` 每行一个）。
///
/// `K-P5f` 之前这里没有中间结构、边算边 `json!` —— 现在要有，理由是**防冒名那一格
/// 只能在看完整批之后才判得出来**（见 [`suppress_inherited_launch_ids`]）。
struct SessionRow {
    pid: u32,
    sid: Option<String>,
    cwd: Option<String>,
    config_dir: Option<String>,
    account: Option<String>,
    alive: bool,
    /// 🔴 `K-R21`：读 `CLAUDE_CONFIG_DIR` 的**那一次**，`/proc/<pid>/environ`
    /// **这一刻取不到**（读失败 / 读回 0 字节）。
    ///
    /// 它与 `config_dir: None` 是**两件事**：`None` 说的是「读到了、这个键没设」，
    /// 本格说的是「这一刻我读不出来」。⇒ 本格为真时**不归属账号、也不算裸起**。
    ///
    /// ⚠ **不进出参**：出参形状一个字节都没动（`configDir:null` + `account:null`
    /// + `bare:false` 今天就表达得了「不知道」）。要把「为什么不知道」也发出去，
    /// 那是给出参加状态位、是改上线契约 —— 同 [`suppress_inherited_launch_ids`]
    /// 头注里那条已被前人裁死的口径。
    cfg_env_unreadable: bool,
    /// 从 `/proc/<pid>/environ` 抠到、且过了 [`launch_id_is_safe`] 的原值。
    /// 还没过防冒名那一格 —— **别直接往出参里填这一格**。
    launch_id: Option<String>,
    /// 这条会话的 `ANTHROPIC_BASE_URL` 是不是**本机中转那一形**（回环 ＋ 钥匙段 ＋ 路由，
    /// `relay_route_core::split_keyed_base_url` 认得出）。`None` = 不知道（进程已死 / 环境这一刻取不到）。
    /// 用途：机器页「停」本机后端之前数一数有几条会话会断（主会话 D-f）。
    via_relay: Option<bool>,
}

/// 🔴🔴 **防冒名：不唯一的身份 token 一律不作数**。
///
/// # 它防的是什么（与本文件里另一道身份检查**不是同一件事**）
///
/// 同一个文件里已经有一道 [`session_process_identity_ok`]，它防的是 **PID 复用**：
/// pidfile 记的 `procStart` 必须与 `/proc/<pid>` 当前的 starttime 精确相等，
/// 否则这个 pid 已经是别人的了。⇒ 它买到的是「**这个 pid 就是写那份 pidfile 的那个进程**」。
///
/// **它一个字都不管环境变量是从哪继承来的。** 而 `CCM_LAUNCH_ID` 是**继承型**变量：
/// `export CCM_LAUNCH_ID=…; claude …` 之后，claude 再 spawn 的**子进程原样继承它**
/// （SDK 起的、claude 自己起的 claude）。那些子进程会写**自己的** `sessions/<PID>.json`
/// ⇒ 它们过得了 `procStart` 对拍（pid 与 pidfile 确实是同一个进程），
/// **但按 pid 读回来的 token 是父会话的**。
/// ⚠ 这两件事很容易被当成一件 —— 看见那一行 `session_process_identity_ok` 就以为
/// 「防冒名」已经打过勾了。**那正是本工作区最贵的那族病。**
///
/// # 为什么不照抄盘上那两条已上线的防法（`KP5FD5` 要求说清选的是哪条、为什么）
///
/// | 盘上的防法 | 它防的 | 能不能用在这里 |
/// |---|---|---|
/// | ① `@ccm_sid` 那一侧的 `procStart` 冒名检查（`identity_tag.rs` 头注逐字：backend 打标前已过 `pid_alive` + `add_time_verdict`）| **PID 复用** | ❌ **威胁模型不对** —— 与上面那道是同一族，继承一格都不防 |
/// | ② `CC_BUS_ID` 那一侧的「无条件覆盖继承值」（`shared/ccm:1128`–`:1136`，记着一次**有可复现反例**的事故）| 继承 | ⚠ **原则可用、实现抄不了**：它成立靠「会话名是这个会话身份的唯一事实来源」——`derive_bus_id` 在**本地**就算得出真值，所以敢无条件覆盖。`CCM_LAUNCH_ID` **没有这样的本地真值**（token 是起会话方现铸的 nonce，被起的那一方无从复算）⇒ 写侧无法分辨「监视器刚给我的」与「我从父进程继承的」 |
///
/// ⇒ 本函数落的是 **② 的原则在读侧的兑现**：`CC_BUS_ID` 那条头注最后一句逐字是
/// 「**继承来的值一律不作数**」。读侧能独立判出来的「不作数」只有一条 ——
/// **一个 launch token 只对应一次拉起，因而只该落在一条活会话上**；
/// 落在两条以上，其中至少一条是继承来的，而**谁是原主判不出来**
/// ⇒ 照 `account: null` 那条「查不到就是查不到，**不猜**」，涉事的**全部**置 `None`。
///
/// # ⚠ 它买不到什么（如实写，别读宽）
///
/// - **父会话已经死了**的那一格买不到：死进程过不了 `procStart` 对拍 ⇒ `alive:false`
///   ⇒ 根本不读它的 environ ⇒ 撞不出重复，活着的那个子进程会带着继承来的 token 出现。
///   ⇒ 这条读回路的诚实边界是「**同一批里唯一**」，不是「**确实是它的**」。
/// - **跨批次**不判：本查询是一次性的（`exec` 一次、`ssh` 一次），没有跨调用的记忆。
/// - `launchId: null` **不区分原因**（没设 / 形状不对 / 不唯一 / 进程已死 /
///   **读那一刻环境取不到**）。要区分就得给出参加状态位，那是**改上线契约**——
///   与本文件头注给 `configDir` 那一格写下的裁决同一条（「不属本区范围」），此处照办。
///   ⚠ 第五种是 `K-R21`（09-03）现打出来的：它**一直都在**，只是从前混在「没设」里
///   数不出来（`platform/proc.rs` 那个 `None` 装着四件事）。**这条不是新增的行为，
///   是把「四种」这句旧话订正成实话** —— 而 `configDir` 那一半已经把它拆出来了
///   （`SessionRow::cfg_env_unreadable`），只有身份这一半仍按上面那条裁定合并着。
fn suppress_inherited_launch_ids(rows: &mut [SessionRow]) {
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for r in rows.iter() {
        if let Some(v) = r.launch_id.as_deref() {
            *seen.entry(v).or_insert(0) += 1;
        }
    }
    let dup: std::collections::HashSet<String> = seen
        .into_iter()
        .filter(|(_, n)| *n > 1)
        .map(|(v, _)| v.to_string())
        .collect();
    for r in rows.iter_mut() {
        if r.launch_id.as_deref().is_some_and(|v| dup.contains(v)) {
            tracing::warn!(
                "会话 pid={} 的 {LAUNCH_ID_ENV} 与别的活会话撞了 —— 至少有一条是继承来的，\
                 判不出谁是原主 ⇒ 两边都不作数（launchId: null）",
                r.pid
            );
            r.launch_id = None;
        }
    }
}

// ---------------------------------------------------------------- 命令

/// `--list-accounts`：meta 行 + 每账号一行。永远 exit 0（"未启用"是正常状态，不是错误）。
///
/// 扫描本体挪进 [`scan_accounts`]（帧面 `accounts-list` 出成品那一臂共用同一个扫描）；
/// 本函数只把它摊成 CLI 那几行（meta 行多两格分帧用的 `kind` 与 `accountZeroAware`，逐字节同旧形状）。
/// CLI 这一臂**不并 apikey 表**（`in_table` 恒假 ⇒ 分类逐格是 manifest 那一份）。
fn list_accounts(accts_dir: &Path) -> Vec<String> {
    let (meta, rows) = scan_accounts(accts_dir, &|_| false);
    let enabled = meta.get("enabled") == Some(&serde_json::Value::Bool(true));
    let mut line = serde_json::Map::new();
    line.insert("kind".into(), "accounts-meta".into());
    line.extend(meta);
    if enabled {
        // Z01 能力标记：本后端认识「configDir 缺席 = 账号 0」。
        // **旧后端不会出这个键**（它把账号 0 当坏数据跳过了）⇒ 读这几行的人 default=false ⇒ 能**明说**
        // 「后端太旧，列表里少了账号 0」，而不是让用户看着一个静默少一行的列表。
        line.insert("accountZeroAware".into(), true.into());
    }
    let mut out = vec![serde_json::Value::Object(line).to_string()];
    out.extend(rows.iter().map(serde_json::Value::to_string));
    out
}

/// 清单的**扫描本体**：`(meta, 账号们)`。CLI 那一臂（[`list_accounts`]）与帧面成品
/// （[`list_product_at`]）两个出口共用它 —— 「一份扫描、两个出口」，读 manifest / 判安全 / 判鉴权方式一行不重写。
///
/// `in_table(configDir)`：这个号在**这台机器**的 apikey 表里有没有行（帧面那一臂由调用方按
/// `acct_core::apikey_routed_subset` 答；CLI 那一臂恒答没有）。它是鉴权方式的**第二个输入**，
/// 进的是同一条计算路径（`auth_kind_with_apikey_table` → `auth_ready`，各只一处）。
fn scan_accounts(
    accts_dir: &Path,
    in_table: &dyn Fn(&str) -> bool,
) -> (
    serde_json::Map<String, serde_json::Value>,
    Vec<serde_json::Value>,
) {
    let mpath = manifest_path(accts_dir);
    let meta = |enabled: bool,
                updated_at: serde_json::Value,
                shared_store: serde_json::Value,
                count: usize,
                err_text: serde_json::Value| {
        let mut m = serde_json::Map::new();
        m.insert("enabled".into(), enabled.into());
        m.insert("acctsDir".into(), accts_dir.to_string_lossy().into());
        m.insert("manifestPath".into(), mpath.to_string_lossy().into());
        m.insert("updatedAt".into(), updated_at);
        m.insert("sharedStore".into(), shared_store);
        m.insert("count".into(), count.into());
        m.insert("error".into(), err_text);
        m
    };
    match load_manifest(accts_dir) {
        Err(e) => (
            meta(
                false,
                serde_json::Value::Null,
                serde_json::Value::Null,
                0,
                e.into(),
            ),
            Vec::new(),
        ),
        Ok(m) => {
            let mut lines = Vec::new();
            for a in &m.accounts {
                // Z01：configDir 缺席 = 账号 0。它的 config dir 就是共享库 ⇒ 登录态查那儿，
                // 而 `configDir` 在帧里出 **null**（下游据此「不注入 CLAUDE_CONFIG_DIR」）。
                let (cfg_out, probe_dir) = match a.config_dir.as_deref() {
                    None => (
                        serde_json::Value::Null,
                        m.shared_store.as_deref().map(PathBuf::from),
                    ),
                    Some(c) => {
                        if !is_safe_config_dir(c) {
                            tracing::warn!("账号 {} 的 configDir 不安全，已丢弃：{}", a.name, c);
                            continue;
                        }
                        let n = norm_dir(c);
                        (
                            serde_json::Value::String(n.to_string()),
                            Some(PathBuf::from(n)),
                        )
                    }
                };
                // 只 stat 存在性，绝不读内容。这个文件名就是身份表
                // （`agents::claudecode::accounts::NATIVE_IDENTITY`）里凭据那一项的同一个常量。
                // 探不到 config dir（账号 0 且 manifest 没写 sharedStore）⇒ false，
                // 那是「不知道」，不假装已登录。
                let credentials_present = probe_dir
                    .as_ref()
                    .is_some_and(|d| d.join(CREDENTIALS_NAME).exists());
                // K-A1：鉴权方式这一维。分类与就绪**各只有一处实现**，都住 `acct-core`
                // ——本文件与 `local_accounts.rs` 都调它，所以「两个生产者各填一个不同的
                // 默认值」在结构上不可表示（`KAY1` 那条 acceptor 的失效模式就是这个）。
                // 第二个输入：这台机器的 apikey 表里有没有它（帧面那一臂才问；CLI 那一臂恒没有）。
                let in_apikey_table = cfg_out.as_str().is_some_and(in_table);
                let auth_kind = auth_kind_with_apikey_table(
                    auth_kind_from_manifest(a.auth_kind.as_deref()),
                    in_apikey_table,
                );
                lines.push(serde_json::json!({
                        "name": a.name,
                        "email": a.email.clone().unwrap_or_default(),
                        "configDir": cfg_out,
                        "isDefault": a.is_default,
                        "mode": a.mode.clone().unwrap_or_else(|| "isolated".into()),
                        // 账号 0 恒 exists（「裸起」这个状态永远可达）；有 configDir 的看目录在不在。
                        "exists": match &probe_dir {
                            Some(d) if a.config_dir.is_some() => d.is_dir(),
                            _ => a.config_dir.is_none(),
                        },
                        // 逐字节旧语义：仅 stat `.credentials.json` 存在性，**不代表凭据有效**
                        // （`KA6b` 今天之后仍然成立）。可用性**不再**直接读它，走 `authReady`。
                        "loggedIn": credentials_present,
                        // K-A1：订阅 / api-key。旧 manifest 缺这个键 ⇒ 订阅（`KA6d`）。
                        "authKind": auth_kind,
                        // K-A1：「鉴权方式这一维不再阻塞它被选中」。**不等于真能连上**
                        // （api-key 号还没有配端点的路 ⇒ `KA6a`，UI 必须把这个状态说出来）。
                        "authReady": auth_ready(auth_kind, credentials_present),
                }));
            }
            let count = lines.len();
            (
                meta(
                    true,
                    json_str(m.updated_at.as_deref()),
                    json_str(m.shared_store.as_deref()),
                    count,
                    serde_json::Value::Null,
                ),
                lines,
            )
        }
    }
}

/// 帧面 `accounts-list` 的**成品**（主会话裁：账号域读自己那台的 apikey 表，agent 随请求带）。
///
/// `{meta, accounts, notice}`：清单同 CLI 那一臂同一个扫描（[`scan_accounts`]），并上**这台机器自己**那份 apikey 表
/// （`rows`：表里有哪几条账号 id，调用方从 `accounts::upstream_select::file_face` 读来 —— 与中转里的上游选择同一个出处）；
/// 「哪几个号在表里有行」只问 `acct_core::apikey_routed_subset`（`table_agent`：这台机器上那份文件属于哪一家）。
///
/// `notice`：「能用但有缺」—— manifest 启用了、却一个账号 0 都没有（写它的那一侧旧到不认账号 0）。
/// 措辞不说「远端」：本机远端同一条路（此前那句写着「远端」，本机那条路因此刻意不出它）。
/// 〔旧那一句「后端太旧、不认账号 0」删了：出成品的后端按构造认得账号 0；老后端回的是旧形状，界面当场认出。〕
pub(crate) fn list_product(rows: &[String], agent: &str, table_agent: &str) -> serde_json::Value {
    list_product_at(&resolve_accts_dir(&[]), rows, agent, table_agent)
}

/// [`list_product`] 的本体，账号库目录是参数（判据拿夹具喂它，不碰真家目录）。
pub(crate) fn list_product_at(
    accts_dir: &Path,
    rows: &[String],
    agent: &str,
    table_agent: &str,
) -> serde_json::Value {
    let routed = |dir: &str| {
        !acct_core::apikey_routed_subset(&[dir.to_string()], rows, agent, table_agent).is_empty()
    };
    let (meta, accounts) = scan_accounts(accts_dir, &routed);
    let enabled = meta.get("enabled") == Some(&serde_json::Value::Bool(true));
    let notice = (enabled && !accounts.iter().any(|a| a["configDir"].is_null()))
        .then(|| copy_text("beAccountsQuery.listProductAt.noDefault", &[]));
    serde_json::json!({ "meta": meta, "accounts": accounts, "notice": notice })
}

/// pidfile 目录里每一份**读得出来**的 `(pid, 内容)` —— 上限、跳过要说清，逐字搬自 [`session_accounts`] 那一段循环头
/// （抽出来是为了让「这台机器上哪几个会话活着」（[`live_session_ids`]）与账号归属读**同一批** pidfile，
/// 而 pidfile 目录这件 agent 知识的调用点仍然只有这一处 —— `agent_locality_guard` 的 `ADAPTER_CALL_SITES` 不涨）。
fn pidfiles(agent_home: &Path) -> Vec<(u32, serde_json::Value)> {
    let mut out = Vec::new();
    let dir = crate::agents::claudecode::paths::sessions_root(agent_home);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new(); // 没有 sessions/ → 零行（exit 0）
    };
    let mut seen = 0usize;
    for ent in rd.flatten() {
        if seen >= MAX_SESSION_FILES {
            tracing::warn!("sessions/ 条目超过 {MAX_SESSION_FILES}，其余跳过");
            break;
        }
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(pid) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        seen += 1;
        let bytes = match read_regular_capped(&path, MAX_SESSION_FILE_BYTES) {
            Ok(b) => b,
            Err(e) => {
                // ★〔audit-0805 §5 1x〕**跳过要说清是谁**。
                //
                // 这里原来是 `Err(_) => continue` —— 一个超过 `MAX_SESSION_FILE_BYTES`
                // 的 pidfile 会被**静默丢掉**，那个会话就永远不归属到任何账号，
                // 而没有任何东西说得出是哪一个。
                // ⚠ 而**同一个函数里** `MAX_SESSION_FILES` 超限是会 warn 的
                // （上面几行）—— 同一个函数里两种上限、两种态度。
                //
                // 登记表把它记成「硬报错」，那是**假的**：真实处置是跳过。
                // 定框 **E4**：静默失败一律给身份。⇒ 给它身份，并把登记改成实话
                // （新语义「跳过+说清」，与被刻意排除的「静默截断」的分界就在这个 warn）。
                tracing::warn!(
                    "会话文件 {} 读不了，跳过（不归属该会话）：{e}",
                    path.display()
                );
                continue;
            }
        };
        let v: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => continue,
        };
        out.push((pid, v));
    }
    out
}

/// **这台机器上此刻活着的会话**（sid 集合）—— 历史跨机 join 的本机判活源头。
///
/// 从前本机历史清单的「活没活」由 monitor 的 `SessionMap` 答（它由本后端 watcher 的起停帧喂）；join 进了本机后端之后
/// 由这台后端自己答：pidfile 里的会话 id ＋ 进程还是不是**同一个**（`platform::proc::session_alive`：存在性 ＋ 有 `procStart`
/// 时对拍启动时刻 —— 与 watcher 加会话那一道闸同一个平台原语，**不是**账号归属那道更严的「缺 `procStart` 就不认」）。
pub(crate) fn live_session_ids(agent_home: &Path) -> std::collections::BTreeSet<String> {
    pidfiles(agent_home)
        .into_iter()
        .filter(|(pid, v)| crate::platform::proc::session_alive(*pid, parse_procstart_ticks(v)))
        .filter_map(|(_, v)| {
            v.get("sessionId")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .collect()
}

/// `--session-accounts`：扫 `<claude_dir>/sessions/<PID>.json`，每条一行。
fn session_accounts(agent_home: &Path, accts_dir: &Path) -> Vec<String> {
    // 向适配层要「会话进程环境里该读哪两个键」只问这一次（账号 · 上游地址）。
    let env_keys = crate::agents::claudecode::paths::SESSION_ENV_KEYS;
    // Z01：`None` 这个 key 是账号 0（configDir 缺席）。裸起会话过去归属不到任何账号
    // （`account: null` + `bare: true`），现在它有名字了。
    let by_dir: Vec<(Option<String>, String)> = load_manifest(accts_dir)
        .map(|m| {
            m.accounts
                .into_iter()
                .filter_map(|a| match a.config_dir.as_deref() {
                    None => Some((None, a.name)),
                    Some(c) if is_safe_config_dir(c) => {
                        Some((Some(norm_dir(c).to_string()), a.name))
                    }
                    Some(_) => None,
                })
                .collect()
        })
        .unwrap_or_default();

    let mut out: Vec<SessionRow> = Vec::new();
    for (pid, v) in pidfiles(agent_home) {
        let sid = v.get("sessionId").and_then(|x| x.as_str());
        let cwd = v.get("cwd").and_then(|x| x.as_str());
        // 判活必须过 procStart 身份对拍：PID 会被复用，陈旧 pidfile 的 PID 可能已被
        // 别的进程占用。只按 /proc/<pid> 存在性判活会把已死会话误贴成"活着 + 别人的账号"
        // （审计 R1，已在沙盒复现）。身份不符 → 当作该会话已死，不读 environ、不归属。
        let alive = session_process_identity_ok(pid, &v);
        // 🔴 `K-R21`（09-03）：**这一处从前在说一句斩钉截铁的假话。**
        //
        // 从前它是 `let cfg = if alive { proc_env_var(...) } else { None };` ——
        // 而 `proc_env_var` 那个 `None` 装着四件事，其中「**环境这一刻取不到**」
        // （读失败，或读回 0 字节：exec 窗口 / 僵尸进程）会一路走成
        // `cfg_norm = None` ⇒ 在下面的 `by_dir` 里**正好撞上账号 0 那个 `None` 键**
        // （`:568` 逐字 `None => Some((None, a.name))`）
        // ⇒ 出参不是「不知道」，是 `account: "<账号0>"` + `bare: true`。
        // ⇒ **一条真跑在账号 Z 下的会话，会被报成账号 0 的**，而且无声无息：
        // `alive` 仍是 `true`（它读 `/proc/<pid>/stat`，与 `environ` 不是同一次读）。
        //
        // 现在：「取不到」单独一支，**不归属、不算裸起**（`configDir:null` + `account:null`
        // + `bare:false` 今天就是一个可表达的状态 ⇒ 出参形状一个字节都没改）。
        // ⚠ 另两支（键不在 / 值是空串）**仍然合并**，理由在 `EnvRead` 的类型头注。
        let (cfg, cfg_env_unreadable) = if alive {
            match proc_env_var(pid, env_keys.config_dir) {
                EnvRead::Value(v) => (Some(v), false),
                EnvRead::Unset => (None, false),
                EnvRead::Unreadable => (None, true),
            }
        } else {
            // 进程已死时一个字节都不读 ⇒ 谈不上「取不到」，那一格照旧是「没读」。
            (None, false)
        };
        // `K-P5f`：**第二个键**。进程已死时一个字节都不读（同 `configDir` 那一格的理由：
        // `/proc/<pid>/environ` 在进程消失那一刻整个不存在，读了也只是 `None`；
        // 而万一 pid 被复用，读到的就是**别人的**环境）。
        // 形状不对 ⇒ 直接当没有（fail closed，见 `launch_id_is_safe`）。
        // ⚠ `K-R21` **刻意没动这一处**：它是 fail-closed 的（「取不到」与「没设」
        // 都得同一个保守答案 `None`），而 `:425` 那条裁定写着要区分就得给出参加状态位、
        // 那是改上线契约。⇒ 这里用 `.value()`，等于声明「这两件事对我等价」。
        // 🔴 代价如实登记：那条 flaky 判据
        // （`an_inherited_launch_id_is_never_reported_as_the_childs_own_identity`）
        // 红在**这条**读回路上，不是上面那条 ⇒ **本拍没有让它变绿，也不该被读成让它变绿**。
        let launch_id = if alive {
            proc_env_var(pid, LAUNCH_ID_ENV)
                .value()
                .filter(|v| launch_id_is_safe(v))
        } else {
            None
        };
        // 第三个键：只折成「走不走本机中转」一个布尔；值带钥匙，这一行之后就丢掉。
        let via_relay = if alive {
            match proc_env_var(pid, env_keys.base_url) {
                // 环境里是中转地址 ≠ 真走中转：agent 自己的设置文件可能压过它（`settings_may_set_base_url`）⇒ 那时说不清。
                EnvRead::Value(v) if relay_route_core::split_keyed_base_url(&v).is_some() => {
                    let settings_win = (env_keys.settings_may_set_base_url)(
                        cfg.as_deref().map(Path::new),
                        agent_home,
                        cwd.map(Path::new),
                    );
                    (!settings_win).then_some(true)
                }
                EnvRead::Value(_) => Some(false),
                EnvRead::Unset => Some(false),
                EnvRead::Unreadable => None,
            }
        } else {
            None
        };
        let cfg_norm = cfg.as_deref().map(|c| norm_dir(c).to_string());
        // 归属：有 configDir 就逐字匹配；没有、**进程确实活着**、**且环境这一刻读得到**
        // 才是账号 0。
        // 进程已死时不归属（cfg 恒 None，归给账号 0 会把死会话贴成账号 0 的）。
        // 🔴 `cfg_env_unreadable` 时也不归属（`K-R21`）：那一刻我们**不知道**它设没设，
        // 而「不知道」与「确实没设」在这里的差别就是一句假话的差别。
        let account = if alive && !cfg_env_unreadable {
            by_dir
                .iter()
                .find(|(d, _)| d.as_deref() == cfg_norm.as_deref())
                .map(|(_, n)| n.clone())
        } else {
            None
        };
        out.push(SessionRow {
            pid,
            sid: sid.map(str::to_string),
            cwd: cwd.map(str::to_string),
            config_dir: cfg_norm,
            account,
            alive,
            cfg_env_unreadable,
            launch_id,
            via_relay,
        });
    }
    // 🔴 防冒名那一格**只能在这里判**：它要看完整批才知道有没有撞（`KP5FD5`）。
    suppress_inherited_launch_ids(&mut out);
    out.into_iter()
        .map(|r| {
            serde_json::json!({
                "pid": r.pid,
                "sessionId": json_str(r.sid.as_deref()),
                "cwd": json_str(r.cwd.as_deref()),
                "configDir": json_str(r.config_dir.as_deref()),
                "account": json_str(r.account.as_deref()),
                // 🔴 **`bare` 的语义钉死在 `CLAUDE_CONFIG_DIR` 上，加第二个键没有把它拓宽。**
                // 〔`KP5FD4`，`K-P5f` 第二拍现打的一格〕它的全部含义是「进程活着（身份已确认）
                // 但没设 `CLAUDE_CONFIG_DIR`」，**Z01 起这不再是异常**：它就是账号 0
                // （上面的 `account` 会给出名字）。字段保留是因为下游要用它区分
                // 「账号 0」与「设了 configDir 的账号」——语义从「告警」变成「事实」。
                // ⚠ **没设 `CCM_LAUNCH_ID` 不进这一格**，理由是两件事：`bare` 答的是
                // **账号**这一维（它与 `account` 是一对），`launchId` 答的是**身份**这一维。
                // 让一个布尔同时表示两个变量的缺席，正是本工作区最贵的那族病
                // （「一个值装了两件事」）；`launchId: null` 自己就说得清「没有」。
                // 🔴 **`K-R21`（09-03）给它补了第三个合取项，而语义没有拓宽、是收窄**：
                // 「没设」这句话只有在**环境读得到**的时候才说得出口。环境这一刻取不到时
                // （exec 窗口 / 僵尸进程）从前这里会斩钉截铁地报 `bare:true` ——
                // 那不是「裸起」，那是「不知道」。⇒ 加 `!r.cfg_env_unreadable`。
                // ⚠ 布尔仍然只答**账号**这一维，一格都没多装（那正是上一段在防的病）。
                "bare": r.alive && !r.cfg_env_unreadable && r.config_dir.is_none(),
                "alive": r.alive,
                // `K-P5f`：起会话方铸的身份 token（`CCM_LAUNCH_ID`），过了形状核与防冒名两道。
                // **`null` = 不作数**（没设 / 形状不对 / 与别的活会话撞了 / 进程已死 /
                // **读那一刻环境取不到**），五种原因**刻意不区分** —— 同 `account: null` 那条「不猜」。
                // ⚠ 第五种是 `K-R21`（09-03）现打出来的，**它一直都在、只是没人写出来**：
                // 从前它混在「没设」里数不出来。⇒ 这里不是新增了一种行为，是把一句
                // 「四种」的旧话订正成实话。
                // ⚠ 老后端不出这个键，下游读成 `None`（additive）。
                "launchId": json_str(r.launch_id.as_deref()),
                // 走不走本机中转：`true` / `false` / `null`（不知道：进程已死或环境这一刻取不到）。
                // ⚠ 老后端不出这个键，下游读成 `null`（additive）。
                "viaRelay": r.via_relay,
            })
            .to_string()
        })
        .collect()
}

/// `--account-trust <configDir> <cwd>`：目标账号是否已信任该目录。
/// `configDir` 必须 ∈ manifest（否则这就成了任意文件读原语）。
fn account_trust(
    accts_dir: &Path,
    config_dir: &str,
    cwd: &str,
) -> Result<String, (String, String)> {
    if !is_safe_config_dir(config_dir) {
        return Err((
            "unsafe_config_dir".into(),
            copy_text("beAccountsQuery.accountTrust.unsafeDir", &[]).into(),
        ));
    }
    let m = load_manifest(accts_dir).map_err(|e| ("manifest_unavailable".to_string(), e))?;
    let want = norm_dir(config_dir);
    if !m.accounts.iter().any(|a| {
        a.config_dir
            .as_deref()
            .is_some_and(|c| is_safe_config_dir(c) && norm_dir(c) == want)
    }) {
        return Err((
            "unknown_config_dir".into(),
            copy_text("beAccountsQuery.accountTrust.notListed", &[]).into(),
        ));
    }
    cc_accounts::trust_of_config(&cc_accounts::config_path_in(Path::new(want)), cwd)
}

/// `--account-trust-zero <cwd>`：**账号 0** 的信任预检。
///
/// 为什么要单独一个入口而不是给 `--account-trust` 传个空 `configDir`：账号 0 **没有**
/// config dir，空串是被明令禁止的拼法（空值 ≠ 未设，见 `RawAccount::config_dir`）。
/// 而且它的 `.claude.json` 也不在共享库里 —— 原生根是 `$HOME`（身份表 `NATIVE_IDENTITY` 里它的原生根是家目录）⇒ 路径来源本就不同，
/// 用同一个入口只能靠哨兵值区分，那比多一个动词更容易出错。
fn account_trust_zero(cwd: &str) -> Result<String, (String, String)> {
    let home = home_dir().ok_or_else(|| {
        (
            "no_home".to_string(),
            copy_text("beAccountsQuery.accountTrustZero.noHome", &[]),
        )
    })?;
    cc_accounts::trust_of_config(&cc_accounts::config_path_in(&home), cwd)
}

/// 帧面那两条（`accounts-list` / `accounts-sessions`）的入口。
///
/// 跑的是 CLI 那两臂**同一个函数**（[`list_accounts`] / [`session_accounts`]），账号库目录
/// 走同一个解析（帧面不收 `--accts-dir` 覆盖 —— monitor 从不发它，那是给人手调的旋钮）。
pub(crate) fn lines_for_frame(agent_home: &Path, which: FrameAccounts) -> Vec<String> {
    let accts_dir = resolve_accts_dir(&[]);
    match which {
        FrameAccounts::List => list_accounts(&accts_dir),
        FrameAccounts::BySession => session_accounts(agent_home, &accts_dir),
    }
}

/// 帧面 `accounts-trust`：换号前的信任预检 —— 替掉仍在逐次拨号的
/// `--account-trust` / `--account-trust-zero`（此前远端每问一次经本机后端开一条链路、在那台 exec 一次本二进制）。
///
/// `config_dir == None` ⇒ 账号 0（同 `--account-trust-zero`：路径写死在 `$HOME`，不收路径参数）；
/// 否则同 `--account-trust`（必须 ∈ manifest，否则就成了任意文件读原语）。两形各调 CLI 那一臂**同一个函数**，
/// 拒绝码原样上交。成品 `{trusted, known}`。
pub(crate) fn trust_product(
    config_dir: Option<&str>,
    cwd: &str,
) -> Result<serde_json::Value, (String, String)> {
    trust_product_at(&resolve_accts_dir(&[]), config_dir, cwd)
}

/// [`trust_product`] 的本体，账号库目录是参数（判据拿夹具喂它）。账号 0 那一形仍读真 `$HOME`（它不收路径）。
pub(crate) fn trust_product_at(
    accts_dir: &Path,
    config_dir: Option<&str>,
    cwd: &str,
) -> Result<serde_json::Value, (String, String)> {
    let line = match config_dir {
        None => account_trust_zero(cwd)?,
        Some(c) => account_trust(accts_dir, c, cwd)?,
    };
    // CLI 那一臂的出参是一行 JSON（`trust_of_config` 造的）；帧面只取那两格。缺一格 ⇒ 契约坏了，不猜。
    let v: serde_json::Value = serde_json::from_str(&line).map_err(|e| {
        (
            "failed".to_string(),
            copy_text(
                "beAccountsQuery.trustProductAt.unparsable",
                &[("e", &e.to_string())],
            ),
        )
    })?;
    match (v["trusted"].as_bool(), v["known"].as_bool()) {
        (Some(trusted), Some(known)) => {
            Ok(serde_json::json!({ "trusted": trusted, "known": known }))
        }
        _ => Err((
            "failed".to_string(),
            crate::common::contract::malformed("trust line lacks `trusted` / `known`"),
        )),
    }
}

/// [`lines_for_frame`] 问的是哪一条。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameAccounts {
    /// `--list-accounts` 那一臂。
    List,
    /// `--session-accounts` 那一臂。
    BySession,
}

/// 查询模式入口。返回进程退出码（0 ok / 2 err），同 `history_query::run` 约定。
pub fn run(agent_home: &Path, args: &[String]) -> i32 {
    let accts_dir = resolve_accts_dir(args);
    match args.first().map(String::as_str) {
        Some("--list-accounts") => {
            for l in list_accounts(&accts_dir) {
                println!("{l}");
            }
            0
        }
        Some("--session-accounts") => {
            for l in session_accounts(agent_home, &accts_dir) {
                println!("{l}");
            }
            0
        }
        Some("--account-trust") => match (args.get(1), args.get(2)) {
            (Some(cfg), Some(cwd)) => match account_trust(&accts_dir, cfg, cwd) {
                Ok(line) => {
                    println!("{line}");
                    0
                }
                Err((code, message)) => {
                    // 结构化错误：stderr 纯 JSON，客户端可整段 parse（同 --resolve 约定）
                    eprintln!("{}", serde_json::json!({"code": code, "message": message}));
                    2
                }
            },
            _ => {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "code": "bad_args",
                        "message": "--account-trust requires <configDir> <cwd>"
                    })
                );
                2
            }
        },
        Some("--account-trust-zero") => match args.get(1) {
            Some(cwd) => match account_trust_zero(cwd) {
                Ok(line) => {
                    println!("{line}");
                    0
                }
                Err((code, message)) => {
                    eprintln!("{}", serde_json::json!({"code": code, "message": message}));
                    2
                }
            },
            None => {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "code": "bad_args",
                        "message": "--account-trust-zero requires <cwd>"
                    })
                );
                2
            }
        },
        other => {
            eprintln!("cc-monitor-backend accounts error: unknown argument: {other:?}");
            2
        }
    }
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
#[path = "../../../tests/backend/observe/accounts_query_tests.rs"]
mod tests;
