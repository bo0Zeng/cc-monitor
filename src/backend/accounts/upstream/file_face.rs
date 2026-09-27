//! 〔RM1a · 第四波〕**这台机器上那份凭据文件的帧面读写口** —— 上游选择自己的状态，不是用户文件。
//!
//! # 它补的是哪一格
//!
//! 远端账号页建的 apikey 号，key 一直写进的是**本机**那份表（`设计/70 §13.2` 第三条）——
//! 远端会话用不上。那份文件要落在**会话跑的那台机器**上，而那台机器上唯一住着的是它的后端。
//! ⇒ 本模块是那台机器上这份文件**唯一的程序写者**：`apikey-key-set` 写一条账号的 key，
//! `apikey-read` 回文件级的状态与「表里有哪几行」。两条都只从 `inbound.rs` 登记进来。
//!
//! # 🔴 归属：上游选择自己的状态，**不是用户文件**（判清的全文住 `调研/第四波记录/RM1a.md §1`）
//!
//! 文件名、格式、落点都是本仓定的（`creds_core::store`），只有中转进程里的上游选择读它。
//! ⇒ 它**不走**文件管理那一面（第三层 · 路径解析 · 暂存区 —— 那是给用户文件的；〔AR1 · V119〕上一版写「Claude 会话数据围栏」，
//!   FN1 之后那一面已不设会话文件围栏），
//! 走 `readonly_guard` **第四层**（后端自有状态文件：按文件登记、动词闭集、只从一扇门进来）。
//!
//! # 每台机器上的写者恰好一个
//!
//! monitor 所在那台：monitor 自己（`src/bridge/src/creds_store.rs`，本路一个字节没动）。
//! 其余每台：那台的后端，就是本模块。monitor **从不**把 `apikey-key-set` 发给本机那条连接。
//!
//! # 路径：与 `--relay` 那一臂**同一个出处**
//!
//! 中转进程里的上游选择按 [`super::creds::resolve_path`] 找那份文件，家目录是 `main.rs` 的 `agent_home`。
//! 本模块用**同一个函数、同一个家目录出处**（[`machine_path`]）。远端那台的中转由后端起
//! （`relay-ensure`），环境从后端继承 ⇒ 写的这一份与读的那一份是同一个路径 —— 构造上的事，
//! 不靠一个跨层传递的环境变量（中转那一层因此不必认识凭据文件的任何名字）。
//!
//! # 明文走哪、不走哪
//!
//! key 在 `args.key` 里进来（帧面：那条长连接的入方向；派生的 CLI 面：stdin），**一进来就包成
//! `SecretKey`**，落盘那一步由 `creds_core::store` 的纯函数拼（明文出口仍只在 `merge_key` 那一处）。
//! **不进 argv、不进 env、不进任何一行日志**；应答只回掩码。本模块一行 `tracing` 都没有。
//!
//! # 写法（与 monitor 那一侧 `KS10` 同一组性质）
//!
//! 写的那一刻读盘（不收调用方缓存的副本）· 只改 `accounts.<id>` 那一格、未知键一个不吃
//! （`store::merge_account_key`）· 顺序稳定（`store::to_pretty_json`）· 临时文件**出生即只给本人**
//! （`creds_core::perm::create_private`，O_EXCL）· 写满 · 落盘 · 原子改名 · 失败删自己的临时文件。
//! ⚠ 现有文件**解析不了 ⇒ 拒绝、不覆盖**（`bad_file`）：人手编打错一个逗号时，覆盖等于把他写的东西抹掉。

use copy_core::copy_text;
use creds_core::perm::{self, Verdict};
use creds_core::store;
use creds_core::SecretKey;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// 本族的应答：`data` 或 `(code, message)`（形状同 `read_face::Answer`）。
pub(crate) type FileFaceAnswer = Result<Value, (&'static str, String)>;

/// 这台机器上那份文件在哪 —— **与 `--relay` 那一臂同一个出处**（见模块头注）。
///
/// ⚠ 家目录借 `observe::history_query::agent_home` 那一句：它是 lib 这一侧**唯一**那处「解析本机 home」
/// （与 `main.rs::resolve_agent_home` 逐字同一个适配层函数，`agent_locality_guard` 两处都登记着）。
/// 本模块自己再直呼一次适配层 = 那张「加一个 agent 要回来改的地方」的表**长一格**，而那张表只许降；
/// 自己拼一条路径 = 长出第二份规则。两样都不做。
pub(crate) fn machine_path() -> PathBuf {
    super::creds::resolve_path(
        &|k| std::env::var(k).ok(),
        &crate::observe::history_query::agent_home(),
    )
}

/// `apikey-key-set`：给**一个账号**写 key，写完读回，回掩码。
pub(crate) fn answer_set(args: &Value) -> FileFaceAnswer {
    answer_set_at(&machine_path(), args)
}

/// [`answer_set`] 的本体，路径是参数（判据拿临时目录喂它，不碰真家目录）。
pub(crate) fn answer_set_at(path: &Path, args: &Value) -> FileFaceAnswer {
    // 〔HX2 · 第四波 4D〕入参从 `account` 换成 `configDir`：账号 id 由**这台后端**按全仓唯一那份规则推
    //   （`acct_core::apikey_account_id_of_dir`，起会话那一侧 `endpoint.rs` 调的同一个）。从前是 monitor 推好了交过来 ——
    //   那一跳随写 key 改走 `chan.call` 一起退了（前端一个字都不推账号 id：`KH2C1`）。不为旧形状留兼容：还给 `account` ⇒ 拒。
    if args.get("account").is_some() {
        return Err((
            "bad_args",
            crate::common::contract::malformed(
                "`account` is not accepted; the account id is derived from `configDir`",
            ),
        ));
    }
    let config_dir = args.get("configDir").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `configDir` (string)"),
    ))?;
    let account_id = acct_core::apikey_account_id_of_dir(config_dir).ok_or((
        "bad_args",
        copy_text(
            "beUpstreamFileFace.keySet.noAccount",
            &[("dir", &format!("{config_dir:?}"))],
        ),
    ))?;
    let account = account_id.as_str();
    // ★ 与装表那一步**同一个谓词**：写得进去、却装不进表 ⇒ 那一行的请求永远 404，而文件里明明有它。
    if !crate::relay::segment_is_safe(account) {
        return Err((
            "bad_args",
            copy_text(
                "beUpstreamFileFace.keySet.badId",
                &[("account", &format!("{account:?}"))],
            ),
        ));
    }
    let plain = args.get("key").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `key` (string)"),
    ))?;
    let key = SecretKey::new(plain);
    if !key.is_configured() {
        return Err((
            "bad_args",
            copy_text("beUpstreamFileFace.keySet.emptyKey", &[]),
        ));
    }
    // 〔ST2 × RM1a〕Base URL（加账号表单 apikey 那一支的第二格）：缺席 / null / 空串 = **不碰那一格**
    //   （只配 key 时已有端点原样留着）；给了就先过**与装表同一个谓词**（〔DUP3 · J9〕`upstream_url_core::usable`：
    //   写得进去、却装不进表 ⇒ 那一行永远用不了），不对 ⇒ 整次不写（key 也不落）。
    let base_url = match args.get("baseUrl") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.trim().is_empty() => None,
        Some(Value::String(s)) => Some(s.trim()),
        Some(_) => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("`baseUrl` must be a string when given"),
            ))
        }
    };
    if let Some(url) = base_url {
        super::table::base_if_usable(url).map_err(|why| ("bad_args", why.to_string()))?;
    }
    write_at(path, account, &key, base_url)?;
    // 回的是**盘上的事实**：写完再读一遍，取这一行的掩码与端点。
    let row = read_doc(path)?.and_then(|doc| {
        store::read_accounts(&doc)
            .into_iter()
            .find(|e| e.id == account)
    });
    let base_url_now = row.as_ref().and_then(|e| e.base_url.clone());
    let masked = row.and_then(|e| e.key.map(|k| k.masked())).ok_or((
        "io_failed",
        copy_text(
            "beUpstreamFileFace.keySet.notReadBack",
            &[
                ("path", &path.display().to_string()),
                ("account", &format!("{account:?}")),
            ],
        ),
    ))?;
    Ok(json!({
        "account": account,
        "path": path.display().to_string(),
        "masked": masked,
        "baseUrl": base_url_now,
    }))
}

/// `apikey-read`：文件级的状态 ＋ 表里有哪几行。**从不回明文**。
pub(crate) fn answer_read() -> FileFaceAnswer {
    Ok(read_at(&machine_path()))
}

/// [`answer_read`] 的本体。**从不报错** —— 读不动 / 解析不了是**状态**（`problem`），不是一次失败：
/// 界面要的正是那一句「读坏了」，而不是一次「命令失败」。
///
/// `configured` / `masked` 说的是**顶层那一把**（`KH2C3`）。界面经 `chan.call` 直接问它、按形状收
/// （〔US1〕monitor 那一份状态读者 `creds_store::read_status`〔散文墓碑〕与转发的 Tauri 命令退役）。
/// 〔US1〕先前还回一格 `rows`（表里有哪几行，只给 monitor 起会话那一侧用）：「表里有哪几行」从此只有 [`rows_at`] 一份、
/// 只在这台后端里用（`launch-endpoint` · `apikey-routing` · `accounts-list`），不再出线。
pub(crate) fn read_at(path: &Path) -> Value {
    let verdict = perm::judge(&perm::probe(path));
    let (doc, problem) = match read_doc(path) {
        Ok(Some(doc)) => (Some(doc), None),
        Ok(None) => (None, None),
        Err((_, why)) => (None, Some(why)),
    };
    let exists = doc.is_some() || problem.is_some();
    let (configured, masked) = match doc.as_ref().and_then(store::read_key) {
        Some(k) => (true, k.masked()),
        None => (false, String::new()),
    };
    json!({
        "configured": configured,
        "masked": masked,
        "path": path.display().to_string(),
        // 文件不存在时不报权限问题（那时的「查不出来」不是一条有用的提醒）—— 同 monitor 那一侧。
        "notice": if exists { notice_of(&verdict) } else { None },
        "problem": problem,
    })
}

/// ★★〔US1 · 第四波 4D〕**「表里有哪几行」的唯一住址** = 上游选择装表那一步（`table::build`，中转装表同一个函数、
/// 同一张每 agent 默认上游）**真收进表**的那几行的 id。
///
/// 先前这里（与 monitor `history::apikey_rows_at`〔散文墓碑〕）只筛「id 当不当得了路由段」，而装表还会因为
/// `base_url` 解析不了 · 明文非回环 · `auth_style` 认不出 · 「不发头」却配了 key 把一行丢出表 ⇒
/// 那一行界面说「经本机中转」、起会话注入 `/s/`，中转却 404（头注自认的残留）。今天三处读者
/// （`accounts-list` 并表 · `apikey-routing` · `launch-endpoint`）都读这一份 ⇒ 与中转同答。
///
/// **读不动 / 解析不了 ⇒ 零条**：零条的正确行为就是「谁都不按 apikey 号算」，把一份坏文件变成一次清单失败，
/// 是拿一个能用的状态去换一条报错。坏文件自己的那句话由 `apikey-read` 的 `problem` 说。
/// ⚠ 与中转的一处差别（照实写）：中转遇到坏文件**保留上一张表**（`Accounts::refresh_if_changed`），这里答零条。
/// 默认上游的环境旋钮认不出（那一刻中转也起不来）⇒ 同样零条。
pub(crate) fn rows_at(path: &Path) -> Vec<String> {
    rows_at_with(path, &|k| std::env::var(k).ok())
}

/// [`rows_at`] 的本体：环境取值器注入（判据不改进程环境）。
pub(crate) fn rows_at_with(path: &Path, get: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let mut loaded = super::creds::load(path);
    if loaded.problem.is_some() {
        return Vec::new();
    }
    let Some(upstreams) = super::Upstreams::from_env(get) else {
        return Vec::new();
    };
    let (table, _, _) = super::table::build(
        std::mem::take(&mut loaded.accounts),
        super::CREDENTIALS_FILE_AGENT,
        upstreams.of_credentials_file(),
    );
    table.ids_of(super::CREDENTIALS_FILE_AGENT)
}

/// 读一次、解析一次。`Ok(None)` = 文件不在（还没配）；空文件 = 空对象。
fn read_doc(path: &Path) -> Result<Option<Map<String, Value>>, (&'static str, String)> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err((
                "io_failed",
                copy_text(
                    "beUpstreamFileFace.read.failed",
                    &[("path", &path.display().to_string()), ("e", &e.to_string())],
                ),
            ))
        }
    };
    store::parse(&raw)
        .map(Some)
        .map_err(|e| ("bad_file", e.said(path)))
}

/// 把权限判断变成一句给人看的话（与 monitor 那一侧 `creds_store::notice_of` 同一个口径）。
fn notice_of(v: &Verdict) -> Option<String> {
    match v {
        Verdict::OwnerOnly => None,
        Verdict::TooWide { how, fix } => Some(copy_text(
            "beUpstreamFileFace.perm.tooWide",
            &[("how", how), ("fix", fix)],
        )),
        Verdict::Undetermined { why } => Some(why.clone()),
    }
}

/// **这台机器上唯一的写者**（第四层：动词只有建那一层目录 · 原子改名 · 删自己的临时文件）。
fn write_at(
    path: &Path,
    id: &str,
    key: &SecretKey,
    base_url: Option<&str>,
) -> Result<(), (&'static str, String)> {
    use std::io::Write as _;
    let dir = path.parent().ok_or((
        "io_failed",
        copy_text(
            "beUpstreamFileFace.write.noParent",
            &[("path", &path.display().to_string())],
        ),
    ))?;
    // 只建**这一层**（`work/`）；它的父目录是 agent 的家目录，不在就说出来、不替它建。
    if let Err(e) = std::fs::create_dir(dir) {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            return Err((
                "io_failed",
                copy_text(
                    "beUpstreamFileFace.write.mkdirFailed",
                    &[("dir", &dir.display().to_string()), ("e", &e.to_string())],
                ),
            ));
        }
    }
    // 〔HX2〕读—改—写整段在那个目录的跨进程锁里（`platform/lock.rs`）：两个后端进程同时给两个号写 key，
    //   从前后写的那一份整份盖掉先写的那一格（这一份连进程内锁都没有）。
    let _lock = crate::platform::lock::hold(dir).map_err(|e| ("io_failed", e))?;
    // ★ 写的这一刻读盘。解析不了 ⇒ `bad_file`，**不覆盖**。
    let current = read_doc(path)?.unwrap_or_default();
    let merged = store::merge_account_key(&current, id, key);
    // 同一个合并函数、只改这一条的那一格。〔GP1 · 第四波〕本机那一份也由（本机）后端的这一处写，monitor 那侧的写口删了。
    let merged = match base_url {
        Some(url) => store::merge_account_base_url(&merged, id, url),
        None => merged,
    };
    let text = store::to_pretty_json(&merged);
    let tmp = dir.join(format!("{}.{}.tmp", store::FILE_NAME, std::process::id()));
    let result = (|| {
        // ★ 出生即只给本人（O_EXCL）：先按 umask 建出来再收窄，中间那一段里已经有明文了。
        let mut f = perm::create_private(&tmp).map_err(|e| {
            (
                "io_failed",
                copy_text(
                    "beUpstreamFileFace.write.tmpCreateFailed",
                    &[("tmp", &tmp.display().to_string()), ("e", &e.to_string())],
                ),
            )
        })?;
        f.write_all(text.as_bytes())
            .and_then(|()| f.sync_all())
            .map_err(|e| {
                (
                    "io_failed",
                    copy_text(
                        "beUpstreamFileFace.write.tmpWriteFailed",
                        &[("tmp", &tmp.display().to_string()), ("e", &e.to_string())],
                    ),
                )
            })?;
        drop(f);
        std::fs::rename(&tmp, path).map_err(|e| {
            (
                "io_failed",
                copy_text(
                    "beUpstreamFileFace.write.renameFailed",
                    &[
                        ("tmp", &tmp.display().to_string()),
                        ("path", &path.display().to_string()),
                        ("e", &e.to_string()),
                    ],
                ),
            )
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/upstream/file_face_tests.rs"]
mod tests;
