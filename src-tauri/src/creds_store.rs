//! 第三方 API key 那份文件的**写侧**（monitor 独占）与**读侧掩码**。
//!
//! # 为什么写盘留在 `src-tauri/src`，而不是搬进 `crates/creds-core`
//!
//! 决策（格式 / 判断 / 平台原语）全在 `creds-core`——那是 `K-H2a` 裁三要的
//! 「一个安全性质**一个实现**」。**但真正的写盘不能搬进去**，理由是一次现打的读数（08-27）：
//! `write_site_registry.rs` 与 `atomic_replace_registry.rs` 的 `src_root()` **逐字都是**
//! `Path::new(env!("CARGO_MANIFEST_DIR")).join("src")` ⇒ **`src-tauri/crates/` 整个不在它们的人群里**。
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
/// 而那个目录的**清单等号对拍**住 `src/generated-boundary-guard.vitest.ts`
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
pub(crate) fn write_key(plain: &str) -> Result<(), String> {
    write_key_at(
        &resolve_path().ok_or_else(|| "no home dir".to_string())?,
        plain,
    )
}

/// `write_key` 剥掉「路径从哪来」之后的那一半 —— **`KS10` 的行为判据打的就是它**。
///
/// ⚠ 不抽的话它只能对着**真实 home 目录下那份文件**跑，而判据绝不许碰用户的真东西
/// ⇒ 「未知键一个不吃 / 顺序稳定 / 原子替换 / 交错」这四条**一条都量不到**。
pub(crate) fn write_key_at(path: &std::path::Path, plain: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    // ★ 在**写的这一刻**读盘（不是收一份调用方缓存的副本）。
    let current = match std::fs::read_to_string(path) {
        Ok(s) => store::parse(&s).map_err(|e| e.to_string())?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::Map::new(),
        Err(e) => return Err(format!("读不动 {}：{e}", path.display())),
    };
    let merged = store::merge_key(&current, &SecretKey::new(plain));
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
mod tests {
    use super::*;

    /// ★★ **跨 crate 契约对拍**：monitor 与 daemon 算出来的是**同一份文件**。
    ///
    /// 两边各写一份路径字面量的话，漂开的那天没有任何东西会说，
    /// 而症状是「界面上配好了，中转说没配」——查不出来的那一类。
    #[test]
    fn the_two_sides_resolve_the_same_file() {
        let home = dirs::home_dir().expect("这台机器得有 home");
        let mine = resolve_path().expect("monitor 侧算得出来");
        // daemon 侧算法：`store::path_under_claude_home(<claude 家目录>)`。
        let daemons = store::path_under_claude_home(&home.join(".claude"));
        assert_eq!(mine, daemons, "两侧算出来的凭据文件路径不一样 —— 契约漂了");
        // 非空对照：这把尺子分得出不同的路径（不是恒相等）。
        assert_ne!(
            mine,
            store::path_under_claude_home(&home.join(".claude-other"))
        );
    }

    /// `KS7`：它**不是**前端整份读写的那份配置。
    #[test]
    fn the_key_never_lands_in_the_config_file_the_frontend_rewrites_wholesale() {
        let cfg = crate::paths::resolve_config_path().expect("config path");
        let creds = resolve_path().expect("creds path");
        assert_ne!(cfg, creds, "凭据落在了前端『读—改—写』整份的那个文件上");
        // 同一个目录是**可以**的（`§0a` 要的是「不进那份配置」，不是「不同目录」）。
        assert_eq!(cfg.parent(), creds.parent());
        // ★ 机检：`config.rs` 的生产段里不许出现那个字段名 ——
        //   它一旦出现，就说明有人把 key 塞进 `load_config`/`save_config` 那条路了。
        let cfg_src = guard_core::production_code(include_str!("config.rs"));
        assert!(
            !cfg_src.contains(store::KEY_FIELD),
            "`config.rs` 的生产段里出现了 `{}` —— key 进了前端整份读写的那份配置",
            store::KEY_FIELD
        );
        // 非空对照：这把尺子**认得出**那个字段名（不是恒不含）。
        assert!(store::TEMPLATE.contains(store::KEY_FIELD));
    }

    /// 从 `at` 之后的第一个 `{` 起按花括号配平切一整块。替掉 `.find("\n}")` 那种找收尾的写法
    /// （它撞 `needle_anchor_registry` 的递减棘轮，而且会在块里第一个顶格 `}` 上停住）。
    fn brace_block(src: &str, at: usize) -> Option<&str> {
        let open = src[at..].find('{')? + at;
        let b = src.as_bytes();
        let (mut depth, mut i) = (0i32, open);
        while i < src.len() {
            match b[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&src[open..=i]);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// 一个只属于本判据的临时目录。**名字中性**（不含任何被断言的字面）——
    /// 诊断常把路径原样印进输出，那时「输出里含某句话」会靠路径恒真。
    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "ccm-cs-{}-{}-{tag}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|x| x.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).expect("建临时目录");
        d
    }

    /// ★★ **`KS10` 的行为那一半**：程序回写不许吃掉人手写的东西。
    ///
    /// PM 08-27 点名：**别把它测成「写完能读回来」——那测不到覆盖。**
    /// 这里测的是**交错**：人改了 A，程序写 B，A 还在不在。
    #[test]
    fn a_program_write_keeps_everything_the_human_put_there() {
        let dir = tmpdir("interleave");
        let p = dir.join("relay-credentials.json");

        // ① 人先手写了一份（裸 `fs::write` = 拿编辑器写的）。
        std::fs::write(
            &p,
            b"{\n  \"_note\": \"first\",\n  \"my_own\": \"keep me\",\n  \"api_key\": \"OLD\"\n}\n",
        )
        .expect("写夹具");
        // ② 界面读了一次（这一份马上就会**过期**）。
        let seen_earlier = read_status_at(&p).expect("读");
        assert!(seen_earlier.configured);

        // ③ 人又在编辑器里改了 —— 就在界面「保存」之前的那一刻。
        std::fs::write(
            &p,
            b"{\n  \"_note\": \"human changed this\",\n  \"my_own\": \"keep me\",\n  \"brand_new\": 7,\n  \"api_key\": \"OLD\"\n}\n",
        )
        .expect("改夹具");

        // ④ 程序这时候才写。
        write_key_at(&p, "NEW-KEY").expect("写");

        let back: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&p).expect("读回")).expect("解析");
        assert_eq!(
            back["_note"], "human changed this",
            "人的编辑被陈旧副本盖掉了"
        );
        assert_eq!(back["brand_new"], 7, "人新加的键被吃掉了");
        assert_eq!(back["my_own"], "keep me");
        assert_eq!(back["api_key"], "NEW-KEY");
        // 非空对照：界面那一份**确实**是旧的（不是「它碰巧一样」让上面恒真）。
        assert!(!seen_earlier.masked.is_empty());

        // `KS10②` 顺序稳定：整份文本按键名排序。
        let text = std::fs::read_to_string(&p).expect("读回");
        let ia = guard_core::find_pinned(&text, "_note").expect("_note 应当恰好出现一处");
        let ib = guard_core::find_pinned(&text, "api_key").expect("api_key 应当恰好出现一处");
        let ic = guard_core::find_pinned(&text, "brand_new").expect("brand_new 应当恰好出现一处");
        assert!(ia < ib && ib < ic, "落盘不是按键名排序的：{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `KS5` 调用点 + `KS11` 门①那半：写完那份文件**立刻只给本人**，
    /// 而读入口在它被放宽时**出声**。
    #[cfg(unix)]
    #[test]
    fn a_written_file_is_owner_only_and_a_widened_one_is_called_out() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmpdir("perm");
        let p = dir.join("relay-credentials.json");

        write_key_at(&p, "sk-ant-JUST-WRITTEN").expect("写");
        let mode = std::fs::metadata(&p).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "写完没有收窄成只给本人（实测 {mode:04o}）");
        assert!(
            read_status_at(&p).expect("读").notice.is_none(),
            "刚写完就报权限问题 —— 那条提醒会变成噪音"
        );

        // ★ 非空对照：**放宽它，读入口必须出声**（否则上面那条 `is_none` 证不了什么）。
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).expect("放宽");
        let notice = read_status_at(&p)
            .expect("读")
            .notice
            .expect("过宽了必须出声");
        assert!(notice.contains("chmod 600"), "没说清怎么修：{notice}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 三态：没配 · 配了 · 文件读坏了。**「读坏了」不许退化成「没配」**。
    #[test]
    fn a_broken_file_is_surfaced_instead_of_looking_unconfigured() {
        let dir = tmpdir("three-states");
        let p = dir.join("relay-credentials.json");

        // ① 文件不存在 ⇒ 没配、无问题、无提醒。
        let s0 = read_status_at(&p).expect("读");
        assert!(!s0.configured && s0.problem.is_none() && s0.notice.is_none());

        // ② 配了 ⇒ 只回掩码。
        write_key_at(&p, "sk-ant-0123456789ABCDEF").expect("写");
        let s1 = read_status_at(&p).expect("读");
        assert!(s1.configured);
        assert!(!s1.masked.contains("0123456789"), "回了明文：{}", s1.masked);
        assert!(s1.masked.contains('*'), "掩码里没有遮蔽符：{}", s1.masked);

        // ③ 人手编打错一个逗号 ⇒ **出声**，不是「没配」。
        std::fs::write(&p, b"{\"api_key\": }").expect("改坏");
        let s2 = read_status_at(&p).expect("读");
        assert!(!s2.configured);
        assert!(
            s2.problem.expect("读坏了必须有说法").contains("手编"),
            "没告诉人这是一份手编的文件"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 明文两个出口，各自**只许出现在哪棵树的哪个文件里**。
    ///
    /// `(方法名, 期望总处数, 期望它住在哪个文件的路径尾巴)`。**默认拒绝**：对不上就红。
    const PLAINTEXT_EXIT_SITES: &[(&str, usize, &str)] = &[
        (
            "expose_for_auth_header(",
            1,
            "remote-daemon-proto/src/relay/server.rs",
        ),
        (
            "expose_for_persisting(",
            1,
            "crates/creds-core/src/store.rs",
        ),
    ];

    /// 本判据扫哪几棵树。**这就是「取明文恰好 N 处」那句全称的分母。**
    const PLAINTEXT_SCAN_TREES: &[&str] = &[
        "src-tauri/src",
        "src-tauri/crates",
        "remote-daemon-proto/src",
    ];

    /// ★★★ **`KS2` 的人群那一格〔D1 阻-1 回修，08-27〕：三棵树全扫，不是一个文件、也不是一个 crate。**
    ///
    /// # 它替掉的是一个**按 crate 边界画的人群**
    ///
    /// 回修前，「取明文恰好 N 处」这条性质由两处判据分管，而它们的人群加起来**盖不住产品**：
    /// · `creds-core/src/lib.rs` 只扫 `include_str!("lib.rs")`——**它自己这一个文件**；
    /// · `relay/creds_guard.rs` 扫 daemon 那个 crate；
    /// ⇒ **`src-tauri` 整个不在任何人的人群里**，而 monitor 恰恰是明文**第一次进程序**的地方
    ///   （`write_relay_credentials_key(key: String)`）。
    /// D1 审计刀 B 实打：在 monitor 生产段取一次明文 `eprintln!` 出去
    /// ⇒ **8 包合计 1284 passed，一条都没红**。
    ///
    /// 件计划 `§0` 逐字警告过这一形：「**判据守的是前门，key 从后门进**」，
    /// 而它「同时骗过了 PM 与一路审计」。这次它在**同一件里**又长了一次，只是换了个边界。
    ///
    /// # 分母（写清它算了什么、没算什么）
    ///
    /// 人群 = [`PLAINTEXT_SCAN_TREES`] 那三棵树下**所有 `.rs` 的生产段**（剥掉 `#[cfg(test)]`），
    /// **外加本文件自己**（见下面那段：`scan_tree!` 按构造摘掉调用者，那正好会把本文件摘出人群）。
    /// 它数的是**两个具名方法的调用点**，不是「明文」这个概念 ——
    /// 有人把明文经别的路径带出去（自定义类型、`Deref`），本条看不见；
    /// 那一格由 `creds-core` 的 `every_string_returning_exit_is_registered_by_name`
    /// 与 `the_type_has_no_second_impl_block_that_hands_the_inner_string_out` 两条在**定义面**兜。
    /// **三格一起才成立**：定义面（有几个出口）· 调用面（每个出口被调几次、在哪）· 本条（人群覆盖三棵树）。
    #[test]
    fn the_two_plaintext_exits_are_called_from_exactly_one_place_each_across_all_three_trees() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级 = 仓根")
            .to_path_buf();

        let mut files: Vec<(String, String)> = Vec::new();
        for sub in PLAINTEXT_SCAN_TREES {
            for (path, raw) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
                files.push((
                    path.display().to_string().replace('\\', "/"),
                    guard_core::production_code(&raw),
                ));
            }
        }
        // ⚠ `scan_tree!` **按构造摘掉调用者自己那一份** —— 那正好会把本文件摘出人群，
        //   而本文件是 monitor 侧碰 key 最多的一个。⇒ 单独补回来。
        files.push((
            "src-tauri/src/creds_store.rs".to_string(),
            guard_core::production_code(include_str!("creds_store.rs")),
        ));

        // 采集面自检：三棵树都要扫到东西，且总量不能小得离谱。
        assert!(
            files.len() >= 150,
            "只扫到 {} 个 .rs —— 遍历坏了，下面全是空真",
            files.len()
        );
        for sub in PLAINTEXT_SCAN_TREES {
            let leaf = sub.rsplit('/').next().unwrap_or(sub);
            assert!(
                files
                    .iter()
                    .any(|(p, _)| p.contains(sub) || p.contains(leaf)),
                "`{sub}` 这棵树一个文件都没扫到 —— 分母缺了一块"
            );
        }

        for (needle, want, home) in PLAINTEXT_EXIT_SITES {
            let mut hits: Vec<String> = Vec::new();
            for (path, prod) in &files {
                // ⚠ **只数调用点，不数定义** —— needle 第一版没剥定义，实测当场红：
                //   `expose_for_auth_header(` 在三棵树里 **2 次**，多出来的那次是
                //   `creds-core/src/lib.rs` 里的 `pub fn expose_for_auth_header(`。
                //   「定义」不是一个出口，「调用」才是；两者混在一个数里，
                //   那个数就同时装了两件事（本区最贵的那族病）。
                for (i, _) in prod.match_indices(needle) {
                    if prod[..i].trim_end().ends_with("fn") {
                        continue; // 这是定义
                    }
                    hits.push(path.clone());
                }
            }
            assert_eq!(
                hits.len(),
                *want,
                "`{needle}` 在三棵树的生产段里出现 {} 次，应当 **{want}** 次：{hits:?}\n\
                 ⚠ `KS2` 逐字：加行是收紧、动断言是放宽 —— 真要多一处，\n\
                 **必须先在件计划里说清那一处是什么**，不许在实现里顺手把这个数改大。",
                hits.len()
            );
            assert!(
                hits[0].ends_with(home),
                "`{needle}` 唯一那处不在 `{home}`，而在 `{}` —— 靶子挪了",
                hits[0]
            );
        }
    }

    /// ★★ **两张表必须说同一件事**〔D2 回修，08-27〕：
    /// 定义面（`creds-core` 里哪几个 fn 标着 `HandsOut`）与调用面（本文件里哪几个 needle
    /// 被数调用点）**必须是同一组名字**。
    ///
    /// # 它买的是什么
    ///
    /// `D2` 点名要「给一条判据钉住 `HandsOut` / `ReadsOnly` 这个区分」。
    /// 光在定义面钉「`HandsOut` 恰好 2 条」还不够 —— 有人把一个**新出口**标成 `HandsOut`
    /// 并同时把定义面那条相等断言改大，两边就都绿了。
    /// ⇒ 本条把它接到**另一个 crate 里的另一张表**上：新出口要绿，得**同时**改两张表，
    /// 而调用面那张表一改，`the_two_plaintext_exits_are_called_from_exactly_one_place_each_across_all_three_trees`
    /// 立刻要求它「恰好 1 处调用、住在指定文件」。**三张表互相钉住，改一张不够。**
    ///
    /// ⚠ 它**不判**分类对不对（那要判语义）—— 它判的是**两张表有没有说同一件事**。
    #[test]
    fn the_definition_table_and_the_call_site_table_name_the_same_exits() {
        let core = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/creds-core/src/lib.rs"),
        )
        .expect("读不到 creds-core 的源码 —— 抽取器坏了，本条会零命中地绿");

        // 从 `INNER_FIELD_USERS` 里挑出标着 `HandsOut` 的行，取它的名字。
        let at = guard_core::find_pinned(&core, "const INNER_FIELD_USERS:")
            .expect("切不出定义面那张表 —— 本条按红处理");
        // ⚠ 切法**不是** `brace_block` —— 这张表是 `&[ … ]`，它的第一个 `{` 可能落在很远的地方
        //   （实测第一版就是这么切歪的，被下面那条反空真自检当场逮住：「一条 HandsOut 都没抽到」）。
        //   按它自己的收尾 `];` 切才是这张表的边界。
        let table = core[at..]
            .find("\n    ];")
            .map(|i| &core[at..at + i])
            .expect("切不出表体（找不到 `];` 收尾）—— 按红处理");
        assert!(table.len() > 200, "表体只有 {} 字节 —— 切歪了", table.len());
        let mut hands_out: Vec<String> = Vec::new();
        for (i, _) in table.match_indices("Handling::HandsOut") {
            // 往回找最近的一个 `"名字"`。
            let before = &table[..i];
            let Some(q_end) = before.rfind('"') else {
                continue;
            };
            let Some(q_start) = before[..q_end].rfind('"') else {
                continue;
            };
            hands_out.push(before[q_start + 1..q_end].to_string());
        }
        hands_out.sort();
        assert!(
            !hands_out.is_empty(),
            "定义面表里一条 `HandsOut` 都没抽到 —— 抽取器坏了，本条在空转"
        );

        // 调用面那张表的 needle 去掉尾巴那个左括号，就是方法名。
        let mut call_side: Vec<String> = PLAINTEXT_EXIT_SITES
            .iter()
            .map(|(n, _, _)| n.trim_end_matches('(').to_string())
            .collect();
        call_side.sort();

        assert_eq!(
            hands_out, call_side,
            "两张表点的**不是同一组出口**：\n\
             · 定义面（creds-core `INNER_FIELD_USERS` 里标 `HandsOut` 的）= {hands_out:?}\n\
             · 调用面（本文件 `PLAINTEXT_EXIT_SITES`）= {call_side:?}\n\
             ⚠ 两边说的不是一件事时，**各自都绿**，而中间那道缝就是明文出去的地方。"
        );
    }

    /// ★ `KS5` 调用点的机检：`write_key_at` 里收窄**恰好两次**（tmp 一次、目标一次）。
    ///
    /// # 它为什么必须存在（`MU12` 实测：行为判据看不见这一刀）
    ///
    /// 08-27 变异台：只把 **tmp 那一次** `make_private` 去掉、留下目标那一次
    /// ⇒ `a_written_file_is_owner_only_and_a_widened_one_is_called_out` **照样绿**
    /// （它量的是**改名之后**那份文件的 mode，而 tmp 那一刻的宽窗口已经过去了、观测不到）。
    /// ⇒ 这一格只能靠机检钉，且**必须钉次数**，不是钉「有没有」。
    ///
    /// 两次各守什么：
    /// · **tmp 那次**：临时文件那一刻就是明文，中间那一段不许是宽的。
    ///   Windows 上更要紧 —— `MoveFileExW` 会把 tmp 的 ACL 覆盖到目标上，tmp 的 ACL 就是最终的 ACL。
    /// · **目标那次**：非 Windows 上 rename 保留源的位，这一句是**纵深**；
    ///   而目标此前若已存在且是宽的，只靠 tmp 那次收不到它。
    #[test]
    fn the_write_path_narrows_both_the_temp_file_and_the_final_one() {
        let src = guard_core::production_code(include_str!("creds_store.rs"));
        let at = guard_core::find_pinned(&src, "pub(crate) fn write_key_at(")
            .expect("切不出 `write_key_at` —— 本条按红处理，不是绿");
        let body = brace_block(&src, at).expect("`write_key_at` 的花括号没配平 —— 按红处理");
        // 反空真自检：窗口不许跨进下一个 item。
        assert!(
            !body.contains("\nfn ") && !body.contains("\npub"),
            "窗口跨进了下一个 item —— 窗口无界，下面的断言不算数"
        );
        assert!(body.len() > 300, "窗口只有 {} 字节 —— 切法坏了", body.len());

        let n = body.matches("make_private(").count();
        assert_eq!(
            n, 2,
            "`write_key_at` 里收窄了 {n} 次，应当**恰好 2** 次（tmp 一次 + 目标一次）。\n\
             ⚠ 少一次是**真缺口而行为判据看不见**：`MU12` 实测只去掉 tmp 那次，\n\
             `a_written_file_is_owner_only_and_a_widened_one_is_called_out` 照样绿。"
        );
        // 顺序也要对：tmp 那次必须在原子替换**之前**。
        let i_tmp =
            guard_core::find_pinned(body, "make_private(&tmp)").expect("tmp 那次应当恰好一处");
        let i_rep = guard_core::find_pinned(body, "atomic_replace(").expect("原子替换应当恰好一处");
        assert!(
            i_tmp < i_rep,
            "tmp 的收窄排在原子替换之后了 —— 那时 tmp 已经变成目标，中间那段宽窗口白留了"
        );
    }

    /// ★★★ **`KS5` 阻-2 回修〔D1，08-27〕：临时文件必须在**出生那一刻**就只给本人。**
    ///
    /// # 它替掉的不是一条判据，是一个**站错位置的观测点**
    ///
    /// 回修前 `write_key_at` 的顺序是
    /// `fs::write(&tmp, text)` → `make_private(&tmp)` → `atomic_replace` → `make_private(path)`。
    /// **第一步就把明文写进了一个按 umask 建出来的文件。**
    /// D1 审计探针实打：`tmp 刚建出来那一刻 mode=0664，里面已经有明文 = true`
    /// （那台机器 umask 是 `0002`；**常见的 `0022` 下就是 `0644` —— 全机可读**）。
    /// 而 `§0a` 逐字承诺的正是这一条：「**保**：同机器上别的用户读不到」。
    ///
    /// ★ **它是 `MU12` 那个形状的第二次**：`MU12` 的补法钉住了「**有没有收窄**」，
    /// 把窗口从「写完到 rename」缩短到「写完到 `make_private`」，**但没有消掉那个窗口**。
    /// 三条既有判据全部量在窗口之外（源码面数次数 · rename 之后的 mode · `make_private` 自己）。
    /// ⇒ 修法不是再加一次收窄，是**让它出生时就不宽**：建文件那一步自己带上权限。
    ///
    /// # 本条钉的是「**怎么建**」，行为那一半由 `perm::tests::a_file_created_through_create_private_is_born_owner_only` 钉
    ///
    /// ⚠ 这个名字**改过一次**〔D2，08-27〕：先前写的是 `a_temp_file_is_born_owner_only`，
    /// 而盘上**没有这个判据** —— 真名住 `crates/creds-core/src/perm.rs`。
    /// **指向一个不存在的判据，比不指更坏**：读的人会以为那一格有人守着。
    ///
    /// 两条各管各的：本条管**过程**（生产段里 tmp 只许经 `create_private` 出生，
    /// 且不许再出现「先写后收」那个形状），那条管**终态**（真建一个出来，立刻 stat）。
    #[test]
    fn the_temp_file_is_created_narrow_not_widened_afterwards() {
        let src = guard_core::production_code(include_str!("creds_store.rs"));
        let at = guard_core::find_pinned(&src, "pub(crate) fn write_key_at(")
            .expect("切不出 `write_key_at` —— 本条按红处理，不是绿");
        let body = brace_block(&src, at).expect("`write_key_at` 的花括号没配平 —— 按红处理");
        assert!(body.len() > 300, "窗口只有 {} 字节 —— 切法坏了", body.len());
        assert!(
            !body.contains("\nfn ") && !body.contains("\npub"),
            "窗口跨进了下一个 item —— 窗口无界，下面的断言不算数"
        );

        // ① tmp **必须**经 `create_private` 出生，恰好一次。
        let born = body.matches("create_private(&tmp)").count();
        assert_eq!(
            born, 1,
            "tmp 经 `create_private` 出生的次数是 {born}，应当恰好 1 —— \n\
             0 次 = 它是被 umask 建出来的，出生那一刻就是宽的，而那一刻里已经有明文。"
        );
        // ② **不许**再出现「先按 umask 建、建完再收」那个形状。
        assert_eq!(
            body.matches("fs::write(&tmp").count(),
            0,
            "`write_key_at` 里还有 `fs::write(&tmp…)` —— 那是按 umask 建文件，\n\
             D1 审计探针实打：那一刻 mode=0664（umask 0002）/ 0644（umask 0022），里面已经有明文。"
        );
        // ③ 出生必须排在**写内容之前**（否则「出生时窄」买到的是空文件窄，没意义）。
        let i_born =
            guard_core::find_pinned(body, "create_private(&tmp)").expect("上一条已断言它恰好一处");
        let i_write =
            guard_core::find_pinned(body, "write_all(").expect("写内容那一步应当恰好一处");
        assert!(
            i_born < i_write,
            "tmp 的创建排在写内容之后了 —— 那顺序上不成立"
        );
    }

    /// ★ 手写 TS 类型与本结构体**双向**对拍（`KS6` 前端那一半的地基）。
    ///
    /// # 它比 `SkillView` 那条现成先例强在哪（如实写，不是贬低那条）
    ///
    /// `skill_host::the_ts_view_type_matches_this_struct` 的字段人群是一张**手写清单**
    /// （`for field in ["id", "label", …]`），而它的注释写着「人群从 Rust 这一侧派生」——
    /// **那句话与它的实现对不上**：往 Rust 结构体加一个字段，那条判据不会红。
    /// ⇒ 这里两边都**真派生**，而且**条数相等**：Rust 多一个字段 ⇒ 红；TS 多一个 ⇒ 也红。
    /// 后者是承重的：TS 侧偷偷多一个 `plaintext` 字段，正是 `KS6` 要挡的那一形。
    #[test]
    fn the_ts_status_type_matches_this_struct() {
        // Rust 侧：从本文件的**生产段**里切出结构体体，派生字段名。
        let rust_src = guard_core::production_code(include_str!("creds_store.rs"));
        // ⚠ 用 `find_pinned` 而不是裸 `.find("…")`：本仓 `needle_anchor_registry` 立着一条递减棘轮
        //   （语料变量上的裸匹配「与 `contains` 同族同险：needle 被撑大时照样绿」）。
        //   它额外买两样：**恰好一处** + 两侧有边界。〔08-27 我第一版写裸 `.find` 撞红过它。〕
        let at = guard_core::find_pinned(&rust_src, "pub struct RelayCredentialsStatus {")
            .expect("切不出结构体 —— 本条按红处理，不是绿");
        let body = brace_block(&rust_src, at).expect("结构体没闭合 —— 按红处理");
        let rust_fields: Vec<String> = body
            .lines()
            .filter_map(|l| l.trim().strip_suffix(','))
            .filter_map(|l| l.split_once(':'))
            .map(|(n, _)| n.trim().trim_start_matches("pub ").to_string())
            .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
            .collect();
        assert!(
            rust_fields.len() >= 4,
            "只派生出 {} 个 Rust 字段 —— 抽取器坏了，本条会零命中地绿：{rust_fields:?}",
            rust_fields.len()
        );

        // TS 侧：从 `src/ipc/commands.ts` 里切出接口体，派生字段名。
        let ts = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/ipc/commands.ts"),
        )
        .expect("读不到 `src/ipc/commands.ts` —— 抽取器坏了，本条会零命中地绿");
        let tat = guard_core::find_pinned(&ts, "export interface RelayCredentialsStatus {")
            .expect("TS 侧找不到那个接口 —— 它被改名或删了");
        let tbody = brace_block(&ts, tat).expect("接口没闭合 —— 按红处理");
        assert!(
            tbody.len() > 120,
            "切出来的 TS 接口体只有 {} 字节 —— 切歪了，本条会零命中地绿",
            tbody.len()
        );
        let ts_fields: Vec<String> = tbody
            .lines()
            .filter_map(|l| l.trim().strip_suffix(';'))
            .filter_map(|l| l.split_once(':'))
            .map(|(n, _)| n.trim().to_string())
            .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
            .collect();

        // ★ **双向**：两边各自缺什么都点名，再加一条条数相等。
        for f in &rust_fields {
            assert!(
                ts_fields.contains(f),
                "TS 的 `RelayCredentialsStatus` 缺字段 `{f}` —— 它是手写类型，没有编译器管：\nRust={rust_fields:?}\nTS={ts_fields:?}"
            );
        }
        for f in &ts_fields {
            assert!(
                rust_fields.contains(f),
                "TS 的 `RelayCredentialsStatus` 多了字段 `{f}`，Rust 侧没有它。\n\
                 ⚠ 这一格是承重的：TS 侧偷偷多一个装明文的字段，正是 `KS6` 要挡的那一形。\nRust={rust_fields:?}\nTS={ts_fields:?}"
            );
        }
        assert_eq!(rust_fields.len(), ts_fields.len(), "两侧字段条数不等");
    }

    /// `KS6` 的后端那一半：**回帧里装不下明文**。
    #[test]
    fn the_status_type_cannot_carry_the_plaintext() {
        let s = RelayCredentialsStatus {
            configured: true,
            masked: SecretKey::new("sk-ant-PLAINTEXT-NEVER-ECHOED").masked(),
            path: "/somewhere/relay-credentials.json".to_string(),
            notice: None,
            problem: None,
        };
        let json = serde_json::to_string(&s).expect("序列化");
        assert!(
            !json.contains("sk-ant-PLAINTEXT-NEVER-ECHOED"),
            "回给前端的帧里出现了明文：{json}"
        );
        // 非空对照：它确实带了掩码（不是把整条抹成空串就算过）。
        assert!(json.contains("sk-a"), "掩码里连前缀都没有：{json}");
        assert!(json.contains('*'), "掩码里没有遮蔽符：{json}");
        // `Debug` 也不许漏（错误路径最爱 `{:?}`）。
        assert!(!format!("{s:?}").contains("sk-ant-PLAINTEXT-NEVER-ECHOED"));
    }
}
