//! 第三方 API key 那份文件的**写侧**（monitor 独占）与**读侧掩码**。
//!
//! # 为什么写盘留在 `src/bridge/src`，而不是搬进 `crates/creds-core`
//!
//! 决策（格式 / 判断 / 平台原语）全在 `creds-core`——那是 `K-H2a` 裁三要的
//! 「一个安全性质**一个实现**」。**但真正的写盘不能搬进去**，理由是一次现打的读数（08-27）：
//! `write_site_registry.rs` 与 `atomic_replace_registry.rs` 的 `src_root()` **逐字都是**
//! `Path::new(env!("CARGO_MANIFEST_DIR")).join("src")` ⇒ **`src/bridge/crates/` 整个不在它们的人群里**。
//! 把写盘搬进共享 crate，等于**让它从两张登记表底下溜出去**——
//! 而那正是本工作区在治的那族病（守卫的人群对不上它守的性质）。
//! ⇒ 写盘留在这里：在人群里、要申报。〔洞本身另立跟进件 `己1-f8`，本件不修。〕
//!
//! # `K-H2a` 裁四：**daemon 只读，写只有这一侧**
//!
//! 本模块整个不存在于 daemon crate 里，而 `creds-core` 那半「把文件收窄」的平台原语
//! 挂在 `harden` feature 上、**只有 monitor 开** ⇒ 「daemon 写不了这份文件」是**编译器**兜的。
//!
//! # 这一档保什么、不保什么
//!
//! 整段逐字住 `creds_core` 的 crate 头注（保：同机器上别的用户读不到 · 顺手打开配置文件不会看见 ·
//! 前端每次读写整份配置时它不在里面；**不保**：已经能以你的身份运行程序的人）。
//! ⚠ 那里还记着两条别在这里重复、但**必须一起读**的：DPAPI 那条「拷走也解不开」的性质**今天没有**，
//! 以及**远端那一侧不许从 SFTP 的 mode 参数拿机密性**。

use creds_core::perm::{self, Verdict};
use creds_core::store;
use creds_core::SecretKey;
use std::path::PathBuf;

/// 那份文件在本机的位置。
///
/// ★ **与 daemon 那侧是同一个契约**：相对路径住 `creds_core::store`，两边各自 join 自己的家目录。
/// 由 `the_two_sides_resolve_the_same_file` 对拍 —— 两边各写一份字面量，
/// 漂开的那天没有任何东西会说，而症状是「界面上配好了，中转说没配」这种查不出来的形状。
///
/// ⚠ 它**不跟随** `claudeDir` 覆盖：`config.rs` 头注逐字「monitor 自己的设置永远在默认
/// `~/.claude/claudecode-frontend/` 下，不跟随 `claudeDir` 字段变化」。
pub(crate) fn resolve_path() -> Option<PathBuf> {
    Some(crate::paths::resolve_monitor_data_dir()?.join(store::FILE_NAME))
}

/// 回给前端的东西。**永远只有掩码**（`KS6`）。
///
/// ⚠ 这个结构体**装不下明文**——不是「我们记得不填」，是**类型里没有那个字段**。
/// `KS6` 逐字：一旦回显，key 就从「只住在后端」变成「每次打开那个界面都往前端传一遍」
/// ⇒ 泄漏面从一次变成无数次，每一次都新增前端日志 / 崩溃报告 / 截图 / 录屏四个出口。
/// ⚠ **本类型是手写对拍的，不是 `ts-rs` 生成的** —— 照本仓 `skill_host::SkillView` 的先例
/// （`src/ipc/commands.ts` 头注逐字记着那条：手写、字段名与 Rust 侧必须手动同步、
/// 由 Rust 侧一条判据读那个文件的源码逐个字段对拍，漏一个就红）。
///
/// **为什么不走 `ts-rs`**（现打 08-27）：`#[ts(export)]` 会在 `src/generated/` 新增一个文件，
/// 而那个目录的**清单等号对拍**住 `tests/generated-boundary-guard.vitest.ts`
/// （`readdirSync` + 逐项比对，新增文件必然让它红一次）——**那个文件不在本轮写区**。
/// ⇒ 走手写 + 对拍，等价的牙由 `the_ts_status_type_matches_this_struct` 买。
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct RelayCredentialsStatus {
    /// 配了没配。
    pub configured: bool,
    /// 掩码形（前后各留几位；短到看不出前后缀的整条遮掉）。没配 = 空串。
    pub masked: String,
    /// 那份文件在哪 —— 给「我想自己拿编辑器改」的人看（`KS9` 的路径要能被找到）。
    pub path: String,
    /// 权限过宽 / 查不出来时的提醒（`KS11`：**在界面上显出来**）。没问题 = `None`。
    pub notice: Option<String>,
    /// 文件读坏了时的说法（人手编打错一个逗号）。`None` = 没问题。
    pub problem: Option<String>,
}

/// 读一次，**只回掩码**。
///
/// # ⚠⚠ `K-H2c` `KH2C3`：它读的是**顶层那一把**，也就是 `LEGACY_ACCOUNT_ID` 那一行
///
/// 那一格今天仍然**读得出来**（老用户手上那份文件、以及 `K-H2a` 期界面写下的那一把），
/// 而 `K-H2c` 之后它**不再是写进去的地方** —— 写侧落的是 `accounts.<id>`
/// （见 [`write_key`] 头注）。**这两句话必须一起读**：
/// 只读前半会以为它被废了，只读后半会以为它被删了，而**两件事都没有发生**。
///
/// ⇒ 「某个**账号**配没配」不由本函数答，由 `history::relay_rows_at` 那一族答
/// （那正是起会话那一侧用的同一个取值口，`KH2B7` 已经把它端给了界面）。
/// 本函数答的三样是**文件级**的：那份文件在哪 · 权限过不过宽 · 是不是被手编坏了。
pub(crate) fn read_status() -> Result<RelayCredentialsStatus, String> {
    read_status_at(&resolve_path().ok_or_else(|| "no home dir".to_string())?)
}

/// `read_status` 剥掉「路径从哪来」之后的那一半。
///
/// ★ 抽出来的理由与本仓 `relay::server::resolve_config` 那次逐字同一条：
/// 不抽的话，这段逻辑只能对着**真实 home 目录下那份文件**跑 —— 而判据不许碰用户的真东西，
/// 于是它会变成一格**永远没人量过**的代码。
pub(crate) fn read_status_at(path: &std::path::Path) -> Result<RelayCredentialsStatus, String> {
    let verdict = perm::judge(&perm::probe(path));
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Ok(RelayCredentialsStatus {
                configured: false,
                masked: String::new(),
                path: path.display().to_string(),
                notice: notice_of(&verdict),
                problem: Some(format!("读不动这份文件：{e}")),
            })
        }
    };
    // ⚠ **解析失败不许退化成「没配」** —— 那会让界面说「还没配」而文件里其实有东西，
    //   用户一按「保存」就把自己手编的内容盖掉了。
    let (configured, masked, problem) = match store::parse(&raw) {
        Ok(doc) => match store::read_key(&doc) {
            Some(k) => (true, k.masked(), None),
            None => (false, String::new(), None),
        },
        Err(e) => (false, String::new(), Some(e.to_string())),
    };
    Ok(RelayCredentialsStatus {
        configured,
        masked,
        path: path.display().to_string(),
        // 文件不存在时不报权限问题（`probe` 那时返回「查不出来」，那不是一条有用的提醒）。
        notice: if raw.is_empty() {
            None
        } else {
            notice_of(&verdict)
        },
        problem,
    })
}

/// 把判断变成一句给人看的话。`OwnerOnly` ⇒ `None`（不出声）。
fn notice_of(v: &Verdict) -> Option<String> {
    match v {
        Verdict::OwnerOnly => None,
        Verdict::TooWide { how, fix } => Some(format!("{how}。怎么修：{fix}")),
        Verdict::Undetermined { why } => Some(why.clone()),
    }
}

/// 写一把 key 进去。**`KS10` 的正主。**
///
/// 三条硬要求逐条落在哪：
/// ① **保留未知键** —— 靠 `store::merge_key`（它 clone 传进来的那份文档，只换一个字段）；
/// ② **字段顺序稳定** —— 靠 `store::ordered_keys`（按名字排，不按到达先后）；
/// ③ **原子替换** —— 写 `.tmp` 再 `crate::config::atomic_replace`。
///
/// ★★ **最要紧的那一条不在上面三条里**：`current` 是在**写的这一刻**从盘上读的，
/// 不是界面打开时读的那一份。`daemon_policy.rs` 头注逐字记着本仓踩过的形状 ——
/// 「前端『读—改—写』整份的那一刻，会把 Rust 刚写进去的键按一份**陈旧副本**覆盖掉」。
/// **本件是同一个形状换了两个当事人（程序 vs 人手）。**
///
/// ⚠ **为什么复用 `config::atomic_replace` 而不是自己写一个 `fs::rename`**：
/// `atomic_replace_registry` 按「`rename` / `MoveFileExW` 的**出现次数**」逐文件登记，
/// 而那张表**不在本轮写区**。复用现成的原语 ⇒ 本文件里那两个字面量出现 **0** 次
/// ⇒ 不动那张表，也不给它挖洞。〔它自己头注逐字论证过为什么**刻意不建**统一写入器：
/// 两类文件的正确行为本来就不同。这里选它是因为凭据文件与 `config.json` 同类
/// ——**都是 monitor 自己的文件**，`INVARIANTS §4` 那条 ACL 保留只限定在**用户的**文件。〕
///
/// # ⚠⚠ `K-H2c`：它写的是 `accounts.<id>` 那一格，**不再是顶层那一把**
///
/// `<id>` **不是**调用方给的名字，而是由 `config_dir` 经**全仓唯一那份规则**
/// [`crate::history::relay_account_id_of_dir`] 推出来的 —— 起会话那一侧
/// （`history::relay_account_id`）调的是**同一个函数**，不是一份同形的第二实现。
/// ⚠ 两边各写一份「取末段名」的逻辑，漂开的那天症状是
/// 「设置里说走中转、起会话时没走」，而两边看起来都没错（那条头注自己就是这么写的）。
/// ⇒ 这一格由 `what_the_write_side_wrote_is_exactly_the_row_the_launch_side_looks_for`
/// （行为，跨两半）与 `the_account_id_rule_is_not_reimplemented_on_the_write_side`（机检）钉住。
///
/// ⚠ **顶层那一把（`LEGACY_ACCOUNT_ID` 那一条）一个字节都没被删** —— 它今天仍然读得回来
/// （`creds_core::store::read_accounts` 那一支），**但它不再是写进去的地方**〔`KH2C3`〕。
/// 机检住 `the_write_side_no_longer_targets_the_legacy_top_level_slot`。
///
/// ⚠ 说不出 id（`config_dir` 是空串 / 只有分隔符 / 账号 0 那一档根本没有目录名）⇒ **报错，
/// 不回落到顶层那一格**。回落等于「用户以为配给了 A，实际写进了 default」，
/// 而那一形与 `K-H2` `KH2` 逐字禁的「查不到就拿默认行顶上」是同一族。
///
/// # ⚠ 订正一句**盘上的假话**〔`K-H2c`，09-02；`K-R17` 纪律：点符号不点行号〕
///
/// 这一段先前逐字写着：改 IPC 那条命令的签名会连带动「`src/ipc/commands.ts` ·
/// `src/settings/accounts-section.ts`（含它的 vitest）· `parity_ledger.rs` **三处**的既有判据面」。
/// **`parity_ledger.rs` 那一项是错的，而它漏了 `lib.rs` 自己。** 现打的依据：
/// · `parity_ledger.rs` 对**签名**只有一条断言 `local_or_both_commands_take_no_remote_only_parameter`，
///   而它的 needle 是运行时现拼的 `origin:` 与 `RemoteConfig` 两个 —— `config_dir` 两个都不是；
/// · `checked` 那个数由 `EXPECTED_LOCAL_OR_BOTH` 钉，它数的是**命令条数**、不是参数；
/// · 命令**名**没变 ⇒ `every_tauri_command_is_declared_in_the_ledger` 与 `ledger_shape_is_pinned` 都不动。
/// ⇒ 按**文件**数，真正跟着动的是 **4** 个：`commands.ts` · `accounts-section.ts` · 它的 vitest ·
/// **`lib.rs` 自己**。〔本轮实测兑现：这四个都动了，`parity_ledger.rs` 一个字节没动。〕
///
/// ⚠ 而「明文入参那一跳」由 `the_plaintext_argument_is_only_ever_handed_one_hop_further`
/// 钉着：明文进来之后**只许被往下传一次**，一路到 `SecretKey::new`。
/// **IPC 那一跳本身仍然是 `判不了`**（原样延续 `K-H2a` 的登记）。
pub(crate) fn write_key(config_dir: &str, plain: &str) -> Result<(), String> {
    write_key_at(
        &resolve_path().ok_or_else(|| "no home dir".to_string())?,
        config_dir,
        plain,
    )
}

/// `write_key` 剥掉「路径从哪来」之后的那一半 —— **`KS10` 的行为判据打的就是它**。
///
/// ⚠ 不抽的话它只能对着**真实 home 目录下那份文件**跑，而判据绝不许碰用户的真东西
/// ⇒ 「未知键一个不吃 / 顺序稳定 / 原子替换 / 交错」这四条**一条都量不到**。
///
/// ⚠⚠ **它仍然是本文件唯一那个写函数**〔`K-H2c` 的设计约束，不是巧合〕：
/// `write_site_registry.rs` 按 `(文件, 函数名)` 登记写盘落点，本文件那一行逐字是
/// `("creds_store.rs", "write_key_at", …)`。⇒ 本轮**只给它加一格入参**，
/// **不另起第二个写函数** —— 另起就得动那张登记表，而那正是本工作区在治的那族病
/// （写盘落点从登记表底下溜出去）。
pub(crate) fn write_key_at(
    path: &std::path::Path,
    config_dir: &str,
    plain: &str,
) -> Result<(), String> {
    // ★★ 「这是哪个账号」**全仓只有一份规则** —— 直接调起会话那一侧的那一个。
    //    这不是「两侧对拍」，是**共用一份实现**：漂开这件事在结构上不可表示。
    let id = crate::history::relay_account_id_of_dir(config_dir).ok_or_else(|| {
        format!(
            "说不出这是哪个账号（configDir 是 {config_dir:?}）—— 这一格不许回落到顶层那一把：\
             回落的症状是「以为配给了 A，其实写进了 default」，而 default 那一行谁都能命中。"
        )
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    // ★ 在**写的这一刻**读盘（不是收一份调用方缓存的副本）。
    let current = match std::fs::read_to_string(path) {
        Ok(s) => store::parse(&s).map_err(|e| e.to_string())?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::Map::new(),
        Err(e) => return Err(format!("读不动 {}：{e}", path.display())),
    };
    // ★★ `KH2C1`：落进 `accounts.<id>` 那一格，**不是顶层那一把**。
    //    `merge_account_key` 只改这一条，别的条与两层的未知键一个字节都不动。
    let merged = store::merge_account_key(&current, &id, &SecretKey::new(plain));
    let text = store::to_pretty_json(&merged);

    let tmp = path.with_extension("json.tmp");
    // ★★ **tmp 在出生那一刻就只给本人**〔D1 阻-2 回修，08-27〕。
    //
    // 先前这里是 `fs::write(&tmp, text)` + 建完再 `make_private` —— 那条路上
    // **文件出生到收窄之间有一个真实的宽窗口，而那个窗口里已经有明文**。
    // D1 审计探针实打：`tmp 刚建出来那一刻 mode=0664，里面已经有明文 = true`
    //（那台机器 umask `0002`；常见的 `0022` 下是 `0644` —— **全机可读**）。
    // ⇒ 改成由**创建调用自己带上权限**（Unix 的 `mode(0o600)` / Windows 的 `SECURITY_ATTRIBUTES`）。
    //
    // 残骸先清：`create_private` 用的是 `create_new`（`O_EXCL` / `CREATE_NEW`），
    // 上次崩溃留下的 tmp 会让它直接失败 —— 那是**故意的**：`O_EXCL` 同时挡掉
    // 「别人预置一个符号链接、我们跟随并截断它」那一形（隔壁 `sftp::upload_atomic`
    // 的头注为同一件事逐字论证过 `EXCLUDE` 标志）。
    let _ = std::fs::remove_file(&tmp);
    {
        use std::io::Write as _;
        let mut f = creds_core::perm::create_private(&tmp)
            .map_err(|e| format!("建 {} 失败: {e}", tmp.display()))?;
        f.write_all(text.as_bytes())
            .map_err(|e| format!("write {}: {e}", tmp.display()))?;
        f.sync_all()
            .map_err(|e| format!("落盘 {} 失败: {e}", tmp.display()))?;
    }
    // ★ 这一句今天是**纵深**，不是必需的那一道：上面已经保证了「出生即窄」。
    //   留着它的理由有两条：① 哪天有人把创建那步换回按 umask 建，这一句仍把窗口压到最短；
    //   ② `the_write_path_narrows_both_the_temp_file_and_the_final_one` 那条**既有断言**
    //      钉的是「收窄恰好 2 次 + tmp 那次排在原子替换之前」——**D1 回修不许动既有断言**。
    crate::platform_fs::make_private(&tmp)?;
    crate::config::atomic_replace(&tmp, path)
        .map_err(|e| format!("replace → {}: {e}", path.display()))?;
    // 目标上再收一次：非 Windows 上 `rename` 保留源的位，这一句是**纵深**不是重复；
    // 而目标此前若已存在且是宽的，只靠 tmp 那一次收不到它。
    crate::platform_fs::make_private(path)?;
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/bridge/creds_store_tests.rs"]
mod tests;
