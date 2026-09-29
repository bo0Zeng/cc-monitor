//! 上游选择从哪儿拿 key，以及**拿之前先查一次它的权限**。中转手里没有 key。
//!
//! # `KS9`：这条路上**没有前端**
//!
//! 本模块只做三件事：算出路径 · 读那个文件 · 解析。
//! 它不依赖任何 IPC / 界面 / 帧 —— 所以「**只放一份文件进去、一次界面都不开**」
//! 这句话在这里是**结构上成立**的，不是靠一条测试证的。
//! 判据 `upstream_selection_loads_the_key_from_a_hand_written_file_alone` 走的就是这条真实的路。
//!
//! # `KS11`：读之前查权限，**过宽出声、不拒绝**
//!
//! 出声还是拒绝是一条**产品取舍**，件计划定的是「出声」（拒绝会把人卡死在一个他不知道
//! 怎么修的地方），先例是 OpenSSH 私钥权限过宽直接拒绝。⇒ 本模块出声：
//! stderr 一行「宽在哪」+ 一行「怎么修」，**然后照常把 key 交出去**。
//! ⚠ 这条取舍**实现方无权改**（件计划 `§0b` 第 5 条逐字：「实现方若判断该改成拒绝，交回，不许自批」）。
//!
//! # ⚠ 「路径文档化」这一格落在哪（如实记）
//!
//! `KS9` 逐字要求「**路径必须文档化**（写进 README 或 `--help`）：一个『能手编但没人知道在哪』
//! 的文件等于不能手编」。今天两个字面落点**都不在本轮写区**（`README.md` 不在；backend **没有**
//! `--help`，现打：`main.rs` 里 `"--help"` 零命中）。
//! ⇒ 落点改成**启动日志**：中转进程起来、本层装表时把**算出来的那条绝对路径**印在 stderr 上，
//! 文件不在时连**模板**一起印。它比 README 更强（在你需要它的那一刻告诉你），
//! 但**它不是 README** —— 这一格已抬进上报口。

use super::table::{Note, Rejected, WHY_AUTH_STYLE_UNKNOWN};
use copy_core::copy_text;
use creds_core::perm::{self, Verdict};
use creds_core::store::{self, AccountEntry, AuthStyle};
use std::path::{Path, PathBuf};

/// 覆盖那份文件的位置。给判据与「一台机器上跑两个中转」用。
pub(crate) const ENV_CREDENTIALS: &str = "CCM_APIKEY_CREDENTIALS";

/// 读一次的结果。**三样都要带出去**，因为调用方要把它们分别印出来。
pub(crate) struct Loaded {
    /// 算出来的那条绝对路径 —— **一定要印**（`KS9` 的「路径文档化」落在这儿）。
    pub(crate) path: PathBuf,
    /// 文件里写着的那些账号。**没配 ⇒ 空** —— 空与「读坏了」是两回事，后者走 `problem`。
    ///
    /// ⚠ **它还不是路由表**：这里的每条还带着一个**没解析过**的 `base_url` 字符串，
    /// 而「id 当不当得了路由段 / `base_url` 解析不解析得了」要后端那两个谓词才判得了。
    /// 装成表那一步在 `super::table::build`，两条判断都在那里，都出声。
    pub(crate) accounts: Vec<AccountEntry>,
    /// 权限判断（`KS11`）。`OwnerOnly` 之外都要出声。
    pub(crate) verdict: Verdict,
    /// 文件读不动 / 解析不了时的说法。`None` = 没问题。
    pub(crate) problem: Option<String>,
}

/// 算出那份文件在哪。`env` 覆盖优先，其次这台 monitor 数据目录（默认 `~/.cc-monitor`，V160）根上那一份。
///
/// 数据目录与 monitor 那一侧**同一条规矩**：`store::monitor_data_dir(CCM_DATA_DIR, HOME)`（家目录取法同中转钥匙
/// `relay::door::key_path`），不按 agent 家（`CLAUDE_CONFIG_DIR` 换号不许把凭据换到另一份）。
/// 推不出（`CCM_DATA_DIR` 设了却不是绝对路径 / 没有家目录）⇒ `Err`（那句话），**不退回真 profile**。
///
/// **纯函数**：取值器是注入的 ⇒ 判据打得到这条接线，而不必去改进程环境
/// （`std::env::set_var` 与并行跑的别的判据是竞态 —— 隔壁 `server::run_reading` 的头注
/// 逐字记着这一课）。
pub(crate) fn resolve_path(get: &dyn Fn(&str) -> Option<String>) -> Result<PathBuf, String> {
    if let Some(p) = get(ENV_CREDENTIALS).filter(|p| !p.trim().is_empty()) {
        return Ok(PathBuf::from(p));
    }
    let home = crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into));
    store::monitor_data_dir(get(store::DATA_DIR_ENV).as_deref(), home)
        .map(|d| store::credentials_path(&d))
        .ok_or_else(|| {
            copy_text(
                "beUpstreamCreds.path.noDataDir",
                &[("env", store::DATA_DIR_ENV)],
            )
        })
}

/// 推不出那份文件在哪时要说的那一行（中转装表那一刻；形状同 [`announce`] 里「读不动」那一行）。
pub(crate) fn announce_unresolved(p: &str, out: &mut dyn std::io::Write) {
    let _ = writeln!(out, "[apikey] credentials problem: {p}");
}

/// 读一次。**只读** —— 本模块一个文件系统变更调用都没有（`K-H2a` 裁四；〔RM1a〕裁四今天收窄成
/// 「写只在 `file_face` 那一份、只从帧面进来」，本模块照旧一个写都没有；
/// backend 的 `readonly_guard` 扫的就是这件事）。
pub(crate) fn load(path: &Path) -> Loaded {
    // ★ 顺序是承重的：**先查权限，再读内容**。
    //   反过来的话，一份过宽的文件已经被读进内存了才开始出声 —— 那时提醒的意义少一半。
    let verdict = perm::judge(&perm::probe(path));

    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Loaded {
                path: path.to_path_buf(),
                accounts: Vec::new(),
                // 文件不存在不是「权限有问题」，把那个判断清掉，免得日志里出现
                // 「读不到它的元数据」这种误导性的一行。
                verdict: Verdict::OwnerOnly,
                problem: None,
            };
        }
        Err(e) => {
            return Loaded {
                path: path.to_path_buf(),
                accounts: Vec::new(),
                verdict,
                problem: Some(copy_text(
                    "beUpstreamCreds.load.unreadable",
                    &[("e", &e.to_string())],
                )),
            };
        }
    };

    match store::parse(&raw) {
        Ok(doc) => Loaded {
            path: path.to_path_buf(),
            accounts: store::read_accounts(&doc),
            verdict,
            problem: None,
        },
        // ⚠ **解析失败不许退化成「没配」** —— 人手编时打错一个逗号，
        //   如果这里静默当成「没配」，症状是一条查不出来的 401。
        Err(e) => Loaded {
            path: path.to_path_buf(),
            accounts: Vec::new(),
            verdict,
            problem: Some(e.said(path)),
        },
    }
}

/// 把该说的话说出去。**返回印了几行**，好让判据数得着（不是 `()` —— 那又是一个死值）。
///
/// ⚠ 这里印的每一样都在 `creds_guard::ALLOWED_LOG_FIELDS` 那张白名单里（`KS4`）。
/// **一个字节的 key 都不许进来**：印的是路径、是判断的说法、是「配了没配」这个布尔。
///
/// # ⚠ `K-H2`：多了两个参数，理由逐条
///
/// - `rows` = **真正进了表的行数**，不是文件里写了几条。两者不一样时说明有行被拒。
/// - `rejected` = 被拒的那些行 + 为什么。**静默丢掉一行的症状是「我明明配了，请求永远 404」**，
///   而那查起来要人去读源码 ⇒ 必须出声。
///
/// # ⚠ `K-R1`：又多了一个参数，理由与上面那两条同族
///
/// - `notes` = **进了表、但行为与默认不同**的那些行。它与 `rejected` 是两件事：
///   被拒的那一行的后果是 404（用户立刻看得见），而带 note 的那一行**照发**，
///   只是发出去的字节与默认不同 ⇒ 它错了的症状是**上游的 404 / 401**，
///   与「上游挂了」「key 打错了」同形。**这类才是必须在启动时说出来的。**
pub(crate) fn announce(
    loaded: &Loaded,
    rows: usize,
    rejected: &[Rejected],
    notes: &[Note],
    out: &mut dyn std::io::Write,
) -> usize {
    let mut n = 0usize;
    // ⚠ 〔`设计/90 §1.2` · `设计/20 §6` 命名推论〕本层每一行日志的前缀是 `[apikey]`，**不是** `[relay]`：
    //   常驻后端这一个进程同时承载两层，而先前两层的日志共用一个 `[relay]` —— 读日志的人
    //   分不出「中转（搬字节）出事了」还是「apikey 那张表出事了」，影响面就判不出来。
    //   `[relay]` 只留给中转自己那几行（监听 · 连上游 · 在途上界）。
    // ① 路径 —— `KS9` 的「文档化」就落在这一行。**总是印**，配没配都印。
    let _ = writeln!(out, "[apikey] credentials file: {}", loaded.path.display());
    n += 1;

    if let Some(p) = &loaded.problem {
        let _ = writeln!(out, "[apikey] credentials problem: {p}");
        n += 1;
    }

    // ② 权限（`KS11`）：宽在哪 + 怎么修，**两样都要**。
    match &loaded.verdict {
        Verdict::OwnerOnly => {}
        Verdict::TooWide { how, fix } => {
            let _ = writeln!(out, "[apikey] credentials permissions too wide: {how}");
            let _ = writeln!(out, "[apikey] how to fix: {fix}");
            n += 2;
        }
        Verdict::Undetermined { why } => {
            let _ = writeln!(out, "[apikey] credentials permissions unknown: {why}");
            n += 1;
        }
    }

    // ③ ⚠ **进不了表的那些行要说出去**（`K-H2`）——静默丢一行的症状是
    //    「我明明配了，请求永远 404」。印的是**账号 id**（它本来就要出现在 URL 路径里，
    //    不是秘密）与一句**固定文案**；id 走 `{:?}` ⇒ 控制字符被转义，
    //    不给「把换行塞进日志」留口子。
    for r in rejected {
        let _ = writeln!(
            out,
            "[apikey] credentials: this account cannot be used: {:?} - {}",
            r.id, r.why
        );
        n += 1;
    }

    // ③b ⚠ **`auth_style` 认不出的时候，把认得的那几个现算着印出来**〔`K-R1`〕。
    //
    //    ★ 为什么是**现算**而不是一句写死的清单（`brief` 13b）：那个闭集只有一个住址
    //      （`creds_core::store::AuthStyle::ALL`）。在这里再抄一份，加第四个成员的那天
    //      这一行会**静默变旧**，而它是给正在排错的人看的最后一句话。
    //    ⚠ 只在真有一条这么写错的时候印 —— 每次启动都印等于噪音。
    if rejected
        .iter()
        .any(|r| r.why == WHY_AUTH_STYLE_UNKNOWN.as_str())
    {
        let legal = AuthStyle::ALL
            .iter()
            .map(|s| s.field_value())
            .collect::<Vec<_>>()
            .join(" | ");
        let _ = writeln!(
            out,
            "[apikey] credentials: auth_style must be one of: {legal}"
        );
        n += 1;
    }

    // ③c ⚠ **进了表、但行为与默认不同的那些行**〔`K-R1`〕。
    //    印的与 ③ 同形：账号 id（`{:?}` 转义控制字符）+ 一句**固定文案**。
    //    ⚠ 刻意**不印**那个前缀本身 / 那个认不出的词：那是文件内容，而
    //      `creds_guard` 那张白名单买的正是「进日志的东西不含文件内容」这条性质，
    //      由 `Note::what` 的类型（`&'static str`）兜着，不由「记得别塞进来」兜着。
    for note in notes {
        let _ = writeln!(
            out,
            "[apikey] credentials: this account is not on the default path: {:?} - {}",
            note.id, note.what
        );
        n += 1;
    }

    // ④ 配了没配 —— **只印布尔与行数，不印长度、不印掩码、不印任何一个 key**。
    //    掩码是给界面看的；日志是给运维看的，运维不需要认出是哪一把。
    //    ⚠ 行数印的是**进了表的**那个数，不是文件里写了几条 —— 两者不一样时上面已经逐条说过了。
    if rows > 0 {
        let _ = writeln!(
            out,
            "[apikey] credentials: configured, {rows} account(s) routable"
        );
    } else {
        let _ = writeln!(out, "[apikey] credentials: not configured");
        // ⚠ 文件不在时**连模板一起印** —— 否则「导入」这条要靠猜（`KS9` 逐字）。
        if loaded.problem.is_none() {
            let _ = writeln!(
                out,
                "[apikey] create that file to configure one; it is plain JSON:\n{}",
                store::template()
            );
            n += 1;
        }
    }
    n + 1
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/upstream_select/creds_tests.rs"]
mod tests;
