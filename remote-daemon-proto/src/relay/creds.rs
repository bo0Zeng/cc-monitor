//! 中转从哪儿拿 key，以及**拿之前先查一次它的权限**。
//!
//! # `KS9`：这条路上**没有前端**
//!
//! 本模块只做三件事：算出路径 · 读那个文件 · 解析。
//! 它不依赖任何 IPC / 界面 / 帧 —— 所以「**只放一份文件进去、一次界面都不开**」
//! 这句话在这里是**结构上成立**的，不是靠一条测试证的。
//! 判据 `a_relay_started_with_only_a_file_on_disk_gets_the_key` 走的就是这条真实的路。
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
//! 的文件等于不能手编」。今天两个字面落点**都不在本轮写区**（`README.md` 不在；daemon **没有**
//! `--help`，现打：`main.rs` 里 `"--help"` 零命中）。
//! ⇒ 落点改成**启动日志**：中转起来时把**算出来的那条绝对路径**印在 stderr 上，
//! 文件不在时连**模板**一起印。它比 README 更强（在你需要它的那一刻告诉你），
//! 但**它不是 README** —— 这一格已抬进上报口。

use creds_core::perm::{self, Verdict};
use creds_core::store;
use creds_core::SecretKey;
use std::path::{Path, PathBuf};

/// 覆盖那份文件的位置。给判据与「一台机器上跑两个中转」用。
pub(crate) const ENV_CREDENTIALS: &str = "CCM_RELAY_CREDENTIALS";

/// 读一次的结果。**三样都要带出去**，因为调用方要把它们分别印出来。
pub(crate) struct Loaded {
    /// 算出来的那条绝对路径 —— **一定要印**（`KS9` 的「路径文档化」落在这儿）。
    pub(crate) path: PathBuf,
    /// 没配 ⇒ `None`。**「没配」与「读坏了」是两回事**，后者走 `problem`。
    pub(crate) key: Option<SecretKey>,
    /// 权限判断（`KS11`）。`OwnerOnly` 之外都要出声。
    pub(crate) verdict: Verdict,
    /// 文件读不动 / 解析不了时的说法。`None` = 没问题。
    pub(crate) problem: Option<String>,
}

/// 算出那份文件在哪。`env` 覆盖优先，其次 `<claude 家目录>/claudecode-frontend/…`。
///
/// **纯函数**：取值器与家目录都是注入的 ⇒ 判据打得到这条接线，而不必去改进程环境
/// （`std::env::set_var` 与并行跑的别的判据是竞态 —— 隔壁 `server::run_reading` 的头注
/// 逐字记着这一课）。
pub(crate) fn resolve_path(get: &dyn Fn(&str) -> Option<String>, home: &Path) -> PathBuf {
    match get(ENV_CREDENTIALS) {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => store::path_under_claude_home(home),
    }
}

/// 读一次。**只读** —— 本模块一个文件系统变更调用都没有（`K-H2a` 裁四；
/// daemon 的 `readonly_guard` 扫的就是这件事）。
pub(crate) fn load(path: &Path) -> Loaded {
    // ★ 顺序是承重的：**先查权限，再读内容**。
    //   反过来的话，一份过宽的文件已经被读进内存了才开始出声 —— 那时提醒的意义少一半。
    let verdict = perm::judge(&perm::probe(path));

    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Loaded {
                path: path.to_path_buf(),
                key: None,
                // 文件不存在不是「权限有问题」，把那个判断清掉，免得日志里出现
                // 「读不到它的元数据」这种误导性的一行。
                verdict: Verdict::OwnerOnly,
                problem: None,
            };
        }
        Err(e) => {
            return Loaded {
                path: path.to_path_buf(),
                key: None,
                verdict,
                problem: Some(format!("读不动凭据文件：{e}")),
            };
        }
    };

    match store::parse(&raw) {
        Ok(doc) => Loaded {
            path: path.to_path_buf(),
            key: store::read_key(&doc),
            verdict,
            problem: None,
        },
        // ⚠ **解析失败不许退化成「没配」** —— 人手编时打错一个逗号，
        //   如果这里静默当成「没配」，症状是一条查不出来的 401。
        Err(e) => Loaded {
            path: path.to_path_buf(),
            key: None,
            verdict,
            problem: Some(e.to_string()),
        },
    }
}

/// 把该说的话说出去。**返回印了几行**，好让判据数得着（不是 `()` —— 那又是一个死值）。
///
/// ⚠ 这里印的每一样都在 `creds_guard::ALLOWED_LOG_FIELDS` 那张白名单里（`KS4`）。
/// **一个字节的 key 都不许进来**：印的是路径、是判断的说法、是「配了没配」这个布尔。
pub(crate) fn announce(loaded: &Loaded, out: &mut dyn std::io::Write) -> usize {
    let mut n = 0usize;
    // ① 路径 —— `KS9` 的「文档化」就落在这一行。**总是印**，配没配都印。
    let _ = writeln!(out, "[relay] credentials file: {}", loaded.path.display());
    n += 1;

    if let Some(p) = &loaded.problem {
        let _ = writeln!(out, "[relay] credentials problem: {p}");
        n += 1;
    }

    // ② 权限（`KS11`）：宽在哪 + 怎么修，**两样都要**。
    match &loaded.verdict {
        Verdict::OwnerOnly => {}
        Verdict::TooWide { how, fix } => {
            let _ = writeln!(out, "[relay] credentials permissions too wide: {how}");
            let _ = writeln!(out, "[relay] how to fix: {fix}");
            n += 2;
        }
        Verdict::Undetermined { why } => {
            let _ = writeln!(out, "[relay] credentials permissions unknown: {why}");
            n += 1;
        }
    }

    // ③ 配了没配 —— **只印布尔，不印长度、不印掩码**。
    //    掩码是给界面看的；日志是给运维看的，运维不需要认出是哪一把。
    if loaded.key.is_some() {
        let _ = writeln!(out, "[relay] credentials: configured");
    } else {
        let _ = writeln!(out, "[relay] credentials: not configured");
        // ⚠ 文件不在时**连模板一起印** —— 否则「导入」这条要靠猜（`KS9` 逐字）。
        if loaded.problem.is_none() {
            let _ = writeln!(
                out,
                "[relay] create that file to configure one; it is plain JSON:\n{}",
                store::TEMPLATE
            );
            n += 1;
        }
    }
    n + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个只属于本判据的临时目录。**名字中性**（不含被断言的字面），
    /// 免得诊断把路径原样印进输出、让「输出里含某句话」靠路径恒真
    /// （`brief` 12 逐字点名的那一形）。
    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "ccm-rc-{}-{}-{tag}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).expect("建临时目录");
        d
    }

    /// ★★ **`KS9②` 的正主**：只往盘上放一份文件，**一次界面都不开**，中转就拿到了 key。
    ///
    /// 走的是**生产段那条真实的路**（`resolve_path` → `load`），
    /// 文件是用**裸 `fs::write` 写的**（不是本仓的写入器）—— 那正是「人拿编辑器写了一份」。
    #[test]
    fn a_relay_started_with_only_a_file_on_disk_gets_the_key() {
        let home = tmpdir("only-file");
        let p = store::path_under_claude_home(&home);
        std::fs::create_dir_all(p.parent().expect("父目录")).expect("建父目录");
        // ← 这一行就是「人手写了一份 JSON 放进去」。没有任何界面、没有任何 IPC。
        std::fs::write(
            &p,
            b"{\n  \"_note\": \"my own note\",\n  \"api_key\": \"sk-ant-FROM-A-BARE-FILE\"\n}\n",
        )
        .expect("写夹具");

        let resolved = resolve_path(&|_| None, &home);
        assert_eq!(resolved, p, "路径解析没落在契约那条路上");
        let loaded = load(&resolved);
        assert!(loaded.problem.is_none(), "不该有问题：{:?}", loaded.problem);
        assert_eq!(
            loaded
                .key
                .as_ref()
                .expect("应当拿到 key")
                .expose_for_auth_header(),
            "sk-ant-FROM-A-BARE-FILE"
        );
        // 非空对照：同一条路，文件里没 key 时拿不到（不是恒返回一个值）。
        std::fs::write(&p, b"{\"_note\":\"nothing here\"}").expect("改夹具");
        assert!(load(&resolved).key.is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn env_overrides_the_default_location() {
        let home = tmpdir("env-override");
        let elsewhere = home.join("somewhere-else.json");
        std::fs::write(&elsewhere, b"{\"api_key\":\"sk-ant-ELSEWHERE\"}").expect("写夹具");
        let got = resolve_path(
            &|k| {
                if k == ENV_CREDENTIALS {
                    Some(elsewhere.display().to_string())
                } else {
                    None
                }
            },
            &home,
        );
        assert_eq!(got, elsewhere);
        assert_eq!(
            load(&got)
                .key
                .expect("应当拿到 key")
                .expose_for_auth_header(),
            "sk-ant-ELSEWHERE"
        );
        // 非空对照：空串的覆盖**不算覆盖**，回默认路径。
        let dflt = resolve_path(&|_| Some("   ".to_string()), &home);
        assert_eq!(dflt, store::path_under_claude_home(&home));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// **「读坏了」不许退化成「没配」**（人手编打错一个逗号时的症状差一个量级）。
    #[test]
    fn a_broken_file_is_a_problem_not_a_silent_not_configured() {
        let home = tmpdir("broken");
        let p = store::path_under_claude_home(&home);
        std::fs::create_dir_all(p.parent().expect("父目录")).expect("建父目录");
        std::fs::write(&p, b"{\"api_key\": }").expect("写夹具");
        let loaded = load(&p);
        assert!(loaded.key.is_none());
        let problem = loaded.problem.expect("读坏了必须有说法");
        assert!(problem.contains("手编"), "说法没告诉人这是手编的文件：{problem}");

        // 非空对照：文件**不存在**时 `problem` 是 `None`（「还没配」不是「坏了」）。
        let missing = load(&home.join("nope.json"));
        assert!(missing.problem.is_none());
        assert!(missing.key.is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// `KS11`：权限过宽 ⇒ **出声**（不是拒绝），而且说得出怎么修。
    #[cfg(unix)]
    #[test]
    fn a_world_readable_file_is_announced_with_a_fix_and_still_serves_the_key() {
        use std::os::unix::fs::PermissionsExt;
        let home = tmpdir("too-wide");
        let p = store::path_under_claude_home(&home);
        std::fs::create_dir_all(p.parent().expect("父目录")).expect("建父目录");
        std::fs::write(&p, b"{\"api_key\":\"sk-ant-WIDE\"}").expect("写夹具");
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).expect("放宽");

        let loaded = load(&p);
        let mut buf: Vec<u8> = Vec::new();
        let lines = announce(&loaded, &mut buf);
        let text = String::from_utf8(buf).expect("utf8");

        assert!(text.contains("permissions too wide"), "没出声：{text}");
        assert!(text.contains("how to fix"), "没说怎么修：{text}");
        assert!(text.contains("chmod 600"), "修法不具体：{text}");
        // ★ **出声而不是拒绝**：key 照样交出去（这是件计划定的产品取舍）。
        assert_eq!(
            loaded.key.expect("仍应拿到 key").expose_for_auth_header(),
            "sk-ant-WIDE"
        );
        assert!(lines >= 4, "印的行数 {lines} 太少 —— 量点坏了");

        // ★★ 非空对照：收紧之后**这几句就不该出现**（否则上面全是恒真）。
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).expect("收紧");
        let mut buf2: Vec<u8> = Vec::new();
        announce(&load(&p), &mut buf2);
        let text2 = String::from_utf8(buf2).expect("utf8");
        assert!(!text2.contains("permissions too wide"), "收紧后仍在出声：{text2}");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// ★ **日志里一个字节的 key 都不许有**（`KS4` 的行为那一半，`KS6` 的日志出口）。
    #[test]
    fn nothing_that_gets_announced_ever_carries_the_key() {
        let home = tmpdir("announce-clean");
        let p = store::path_under_claude_home(&home);
        std::fs::create_dir_all(p.parent().expect("父目录")).expect("建父目录");
        std::fs::write(&p, b"{\"api_key\":\"sk-ant-CANARY-IN-ANNOUNCE\"}").expect("写夹具");
        let loaded = load(&p);
        let mut buf: Vec<u8> = Vec::new();
        announce(&loaded, &mut buf);
        let text = String::from_utf8(buf).expect("utf8");
        assert!(
            !text.contains("sk-ant-CANARY-IN-ANNOUNCE"),
            "启动日志里出现了 key：{text}"
        );
        // ⚠ 这里**删掉过一条断言**，经过记下来（它是 `brief` 12 逐字点名那一形的活样本）：
        //   第一版写的是 `assert!(!text.contains("26"))`，意思是「key 的长度也不许漏」。
        //   实测当场红 —— 因为**临时目录名里恰好有 `26`**（`/tmp/ccm-rc-803016-1787867…`），
        //   而这一行输出里本来就带着那条路径。⇒ **路径混进了断言**，
        //   它判的根本不是「有没有印长度」，而是「这台机器这一刻的 pid/纳秒里有没有 26」。
        //   本模块的 `announce` 压根不印任何长度 ⇒ 那条断言既恒脆又证不了东西。
        //   「明文的长度不许漏」这条性质的正确住址是 `SecretKey` 手写的 `Debug`，
        //   由 `creds-core::a_debug_print_never_carries_the_plaintext_or_its_length` 钉着。
        // 非空对照：它确实印了东西，而且印了「配好了」。
        assert!(text.contains("configured"), "什么都没印：{text}");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// 文件不在时要给模板 —— 否则「导入」这条要靠猜。
    #[test]
    fn a_missing_file_prints_the_template_so_import_does_not_need_guessing() {
        let home = tmpdir("missing");
        let loaded = load(&store::path_under_claude_home(&home));
        let mut buf: Vec<u8> = Vec::new();
        announce(&loaded, &mut buf);
        let text = String::from_utf8(buf).expect("utf8");
        assert!(text.contains("not configured"));
        assert!(text.contains(store::KEY_FIELD), "模板里没点名那个字段：{text}");
        assert!(text.contains("plain JSON"), "没说清它是明文 JSON：{text}");
        // 路径**总是**印（`KS9` 的「文档化」）。
        assert!(text.contains(store::FILE_NAME), "没印出路径：{text}");
        let _ = std::fs::remove_dir_all(&home);
    }
}
