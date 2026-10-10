//! 上游选择从哪儿拿 key，以及拿之前先查一次它的权限。中转手里没有 key。
//!
//! 本模块只做三件事：算出路径 · 读那个文件 · 解析。不依赖任何 IPC / 界面 / 帧 ⇒「只放一份文件进去、一次界面都不开」结构上成立
//! （`upstream_selection_loads_the_key_from_a_hand_written_file_alone` 走这条真实的路）。
//!
//! 读之前查权限，过宽出声、不拒绝（拒绝会把人卡死在一个他不知道怎么修的地方）：stderr 一行「宽在哪」+ 一行「怎么修」，然后照常把 key 交出去。
//! 路径在启动日志里说：装表时把算出来的那条绝对路径印在 stderr 上，文件不在时连模板一起印（backend 没有 `--help`）。

use super::table::{Note, Rejected, WHY_AUTH_STYLE_UNKNOWN};
use copy_core::copy_text;
use creds_core::perm::{self, Verdict};
use creds_core::store::{self, AccountEntry, AuthStyle};
use std::path::{Path, PathBuf};

/// 读一次的结果。**三样都要带出去**，因为调用方要把它们分别印出来。
pub(crate) struct Loaded {
    /// 算出来的那条绝对路径 —— 一定要印（路径就靠这一行告诉人）。
    pub(crate) path: PathBuf,
    /// 文件里写着的那些账号。没配 ⇒ 空；空与「读坏了」是两回事，后者走 `problem`。
    /// 它还不是路由表：每条还带着没解析过的 `base_url`；id 当不当得了路由段、`base_url` 解析不解析得了在 `super::table::build` 判，都出声。
    pub(crate) accounts: Vec<AccountEntry>,
    /// 权限判断。`OwnerOnly` 之外都要出声。
    pub(crate) verdict: Verdict,
    /// 文件读不动 / 解析不了时的说法。`None` = 没问题。
    pub(crate) problem: Option<String>,
}

/// 算出那份文件在哪：这台的家（默认 `~/.cc-monitor`）根上那一份。位置只跟着家走，没有另指它的变量。
///
/// 数据目录与 monitor 那一侧同一条规矩：`store::monitor_data_dir(CCM_DATA_DIR, HOME)`，不按 agent 家（换号不许把凭据换到另一份）。
/// 推不出（`CCM_DATA_DIR` 设了却不是绝对路径 / 没有家目录）⇒ `Err`（那句话），不退回真 profile。
/// 纯函数：取值器是注入的，判据不必去改进程环境（`set_var` 与并行跑的别的判据是竞态）。
pub(crate) fn resolve_path(get: &dyn Fn(&str) -> Option<String>) -> Result<PathBuf, String> {
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

/// 读一次。只读 —— 写只在 `file_face` 那一份、只从帧面进来；backend 的 `readonly_guard` 扫的就是这件事。
pub(crate) fn load(path: &Path) -> Loaded {
    // 先查权限，再读内容：反过来的话，一份过宽的文件已经被读进内存了才开始出声。
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
                // 只进日志（`[apikey]` 那一行）：那一句 ＋ 原话一起。
                problem: Some(
                    crate::common::said::Said::with_raw(
                        copy_text(
                            "beUpstreamCreds.load.unreadable",
                            &[("why", &copy_core::io_reason(e.kind()))],
                        ),
                        e,
                    )
                    .logged(),
                ),
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
        // 解析失败不许退化成「没配」：手编时打错一个逗号，静默当成没配的症状是一条查不出来的 401。
        Err(e) => Loaded {
            path: path.to_path_buf(),
            accounts: Vec::new(),
            verdict,
            problem: Some(e.said(path).logged()),
        },
    }
}

/// 把该说的话说出去。返回印了几行（判据数得着）。
///
/// 印的每一样都在 `creds_guard::ALLOWED_LOG_FIELDS` 那张白名单里：路径、判断的说法、「配了没配」这个布尔；一个字节的 key 都不进来。
/// - `rows` = 真正进了表的行数（与文件里写了几条不一样时，说明有行被拒）。
/// - `rejected` = 被拒的那些行 + 为什么（静默丢一行的症状是「我明明配了，请求永远 404」）。
/// - `notes` = 进了表、但行为与默认不同的那些行：它们照发，错了的症状是上游的 404 / 401，与「上游挂了」「key 打错了」同形 ⇒ 启动时必须说。
pub(crate) fn announce(
    loaded: &Loaded,
    rows: usize,
    rejected: &[Rejected],
    notes: &[Note],
    out: &mut dyn std::io::Write,
) -> usize {
    let mut n = 0usize;
    // 本层每一行日志的前缀是 `[apikey]`，不是 `[relay]`：常驻后端一个进程承载两层，读日志的人要分得出是中转还是这张表出事了。
    // ① 路径：总是印，配没配都印。
    let _ = writeln!(out, "[apikey] credentials file: {}", loaded.path.display());
    n += 1;

    if let Some(p) = &loaded.problem {
        let _ = writeln!(out, "[apikey] credentials problem: {p}");
        n += 1;
    }

    // ② 权限：宽在哪 + 怎么修，两样都要。
    match &loaded.verdict {
        Verdict::OwnerOnly => {}
        Verdict::TooWide { how, fix } => {
            let _ = writeln!(out, "[apikey] credentials permissions too wide: {how}");
            let _ = writeln!(out, "[apikey] how to fix: {fix}");
            n += 2;
        }
        Verdict::Undetermined { why, .. } => {
            let _ = writeln!(out, "[apikey] credentials permissions unknown: {why}");
            n += 1;
        }
    }

    // ③ 进不了表的那些行：印账号 id（本来就出现在 URL 路径里，不是秘密）与一句固定文案；id 走 `{:?}` ⇒ 控制字符被转义。
    for r in rejected {
        let _ = writeln!(
            out,
            "[apikey] credentials: this account cannot be used: {:?} - {}",
            r.id, r.why
        );
        n += 1;
    }

    // ③b `auth_style` 认不出的时候，把认得的那几个现算着印出来（闭集只住 `creds_core::store::AuthStyle::ALL`）。只在真有一条写错时印。
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

    // ③c 进了表、但行为与默认不同的那些行：账号 id（`{:?}`）+ 一句固定文案。不印那个前缀本身 / 那个认不出的词：那是文件内容
    // （`Note::what` 的类型 `&'static str` 兜着「进日志的东西不含文件内容」）。
    for note in notes {
        let _ = writeln!(
            out,
            "[apikey] credentials: this account is not on the default path: {:?} - {}",
            note.id, note.what
        );
        n += 1;
    }

    // ④ 配了没配：只印布尔与行数（进了表的那个数），不印长度、不印掩码、不印任何一个 key。
    if rows > 0 {
        let _ = writeln!(
            out,
            "[apikey] credentials: configured, {rows} account(s) routable"
        );
    } else {
        let _ = writeln!(out, "[apikey] credentials: not configured");
        // 文件不在时连模板一起印，否则「导入」这条要靠猜。
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
