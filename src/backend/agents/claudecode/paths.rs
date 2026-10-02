//! Claude 的目录布局：配置根怎么解析、根下面有哪两个子目录。

use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// 覆盖配置根的环境变量名。**账号隔离（后端 `accounts/manage/` 建的账号库）就是靠切它**，
/// 所以它不只是"一个环境变量"，是账号这个概念在 Claude 侧的载体。
pub(crate) const CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";

/// Claude 找上游的环境变量名。起会话时中转地址经它注入（`http://127.0.0.1:<口>/<钥匙>/<前缀>/…`，钥匙段在 pane shell 里展开）；
/// `--session-accounts` 读它**只为答一个布尔**（这条会话走不走本机中转），值本身带钥匙、**绝不出参、绝不进日志**。
pub(crate) const BASE_URL_ENV: &str = "ANTHROPIC_BASE_URL";

/// `--session-accounts` 从会话进程环境里读的那两个**适配层的**键，收成一处交出去：账号（配置根）· 上游地址。
/// 通用层只问适配层**一次**「该读哪两个键」（`agent_locality_guard` 那一格仍是一处：加第三家 agent 时这两个键一起换）。
pub(crate) struct SessionEnvKeys {
    pub(crate) config_dir: &'static str,
    pub(crate) base_url: &'static str,
    /// 这条会话自己的设置文件会不会压过进程环境里的上游地址（见 [`settings_may_set_base_url`]）。
    pub(crate) settings_may_set_base_url: fn(Option<&Path>, &Path, Option<&Path>) -> bool,
}

/// 见 [`SessionEnvKeys`]。
pub(crate) const SESSION_ENV_KEYS: SessionEnvKeys = SessionEnvKeys {
    config_dir: CONFIG_DIR_ENV,
    base_url: BASE_URL_ENV,
    settings_may_set_base_url,
};

/// claude 的设置文件里 `env.ANTHROPIC_BASE_URL` **压过**进程环境（GAP1 件 3 取证：真跑一次，settings 那个口收到请求、
/// 进程环境那个口零次）⇒ 进程环境里是我们的中转地址，不等于它真走中转。这里答「可能被压过」：
/// 配置根（`config_dir`，缺席 = 默认根）下的 `settings.json` · 会话 cwd 下 `.claude/settings.json` / `settings.local.json`
/// 任一份设了非空的 `env.ANTHROPIC_BASE_URL`，或在却读不了 / 解析不了 ⇒ `true`（说不清）。
/// 买不到：系统级 managed settings 与 `--settings` 命令行那一形不看（那两形今天没人用；看到了也只会更「说不清」）。
pub(crate) fn settings_may_set_base_url(
    config_dir: Option<&Path>,
    default_root: &Path,
    cwd: Option<&Path>,
) -> bool {
    let root = config_dir.unwrap_or(default_root);
    let mut files = vec![root.join("settings.json")];
    if let Some(c) = cwd {
        files.push(c.join(HOME_DIR_NAME).join("settings.json"));
        files.push(c.join(HOME_DIR_NAME).join("settings.local.json"));
    }
    files.iter().any(|f| match std::fs::read_to_string(f) {
        Err(e) => e.kind() != std::io::ErrorKind::NotFound,
        Ok(t) => match serde_json::from_str::<serde_json::Value>(&t) {
            Err(_) => true,
            Ok(v) => v["env"][BASE_URL_ENV]
                .as_str()
                .is_some_and(|u| !u.is_empty()),
        },
    })
}

/// 直接敲的 claude 也走中转（注册表 `DefaultUpstream.settings_env` 那一格）：`~/.claude/settings.json` 的 `env.ANTHROPIC_BASE_URL`。
/// 各号的设置文件都链回这一份 ⇒ 写进去对这台所有号同时生效；它**压过**进程环境（见 [`settings_may_set_base_url`]）。
pub(crate) const SETTINGS_ENV: super::super::SettingsEnvFace = super::super::SettingsEnvFace {
    read: read_settings_base_url,
    snippet: settings_env_snippet,
};

/// 读那份设置文件的上限（几 KB 的配置；超了按「读不了」说，不当没写）。
const SETTINGS_CAP_BYTES: u64 = 1 << 20;

/// `$HOME/.claude/settings.json`（不看 `CLAUDE_CONFIG_DIR`：直接敲的 claude 没设它时读的就是这一份）＋ 里面那一格。**只读**。
/// 不在 ⇒ 没写；不是普通文件 / 超上限 / 读不动 / 读不懂 ⇒ 读不了（带为什么，不当没写）。
fn read_settings_base_url(home: &Path) -> (PathBuf, super::super::SettingsBaseUrl) {
    use super::super::{SettingsBaseUrl as S, SettingsUnreadable as Why};
    let file = home.join(HOME_DIR_NAME).join("settings.json");
    let raw = match std::fs::metadata(&file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (file, S::Unset),
        Err(e) => Err(Why::Io(e.to_string())),
        Ok(m) if !m.is_file() => Err(Why::NotFile),
        Ok(m) if m.len() > SETTINGS_CAP_BYTES => {
            let error = Why::TooLarge(SETTINGS_CAP_BYTES);
            Err(error)
        }
        Ok(_) => std::fs::read_to_string(&file).map_err(|e| Why::Io(e.to_string())),
    };
    let found = match raw {
        Ok(text) => settings_base_url(&text),
        Err(error) => S::Unreadable(error),
    };
    (file, found)
}

/// 原文 → `env.ANTHROPIC_BASE_URL`。BOM 容忍；空串当没写；顶层 / `env` 不是对象、值不是串 ⇒ 读不懂（不猜成没写）。
fn settings_base_url(raw: &str) -> super::super::SettingsBaseUrl {
    use super::super::{SettingsBaseUrl as S, SettingsUnreadable as Why};
    let bad = || S::Unreadable(Why::BadShape);
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw.trim_start_matches('\u{feff}'))
    else {
        return bad();
    };
    let Some(top) = v.as_object() else {
        return bad();
    };
    match top.get("env") {
        None | Some(serde_json::Value::Null) => S::Unset,
        Some(serde_json::Value::Object(env)) => match env.get(BASE_URL_ENV) {
            None | Some(serde_json::Value::Null) => S::Unset,
            Some(serde_json::Value::String(u)) if u.is_empty() => S::Unset,
            Some(serde_json::Value::String(u)) => S::Set(u.clone()),
            Some(_) => bad(),
        },
        Some(_) => bad(),
    }
}

/// 要合并进那份文件的那一段：`{"env": {"ANTHROPIC_BASE_URL": "<地址>"}}`（两格缩进）。
fn settings_env_snippet(url: &str) -> String {
    let v = serde_json::json!({ "env": { BASE_URL_ENV: url } });
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

/// 默认配置根在 `$HOME` 下的名字。
pub(crate) const HOME_DIR_NAME: &str = ".claude";

/// 解析配置根：`$CLAUDE_CONFIG_DIR` 优先，否则 `$HOME/.claude`。
///
/// Windows（只为编译/冒烟，真实目标是 Linux）：`$HOME` 缺失时退 `%USERPROFILE%\.claude`，
/// 再退 cwd 下的 `.claude`，让二进制至少起得来。
///
/// `S3` 从 `main.rs::resolve_claude_dir` 原样搬来（逻辑一字未改）。
pub fn resolve_home() -> PathBuf {
    match std::env::var_os(CONFIG_DIR_ENV) {
        Some(dir) => PathBuf::from(dir),
        None => default_home(),
    }
}

/// 不看 `CLAUDE_CONFIG_DIR` 的那一格：`$HOME/.claude`（同 [`resolve_home`] 的退路）—— `ccm --base` 与「没选账号」时的家目录。
pub fn default_home() -> PathBuf {
    crate::platform::paths::home_dir()
        .map(|h| h.join(HOME_DIR_NAME))
        .unwrap_or_else(|| PathBuf::from(HOME_DIR_NAME))
}

/// 给了账号配置目录 ⇒ 就是它；没给 ⇒ `from_env` 时 [`resolve_home`]（看 `CLAUDE_CONFIG_DIR`），否则 [`default_home`]。
pub fn home_of(config_dir: Option<&Path>, from_env: bool) -> PathBuf {
    match (config_dir, from_env) {
        (Some(d), _) => d.to_path_buf(),
        (None, true) => resolve_home(),
        (None, false) => default_home(),
    }
}

/// `<home>/projects` —— 会话记录树的根。
///
/// `S3` 从 `common/paths.rs` 搬来。⚠ 它当初被建出来是为了**去重**（U2 实测五处副本，
/// 第五处是内联的、grep 函数名找不到），那个性质本件**原样保留** ——
/// 变的只是它住哪一层：目录名 `projects` 是 **Claude 的布局知识**，
/// 而 `common/` 的三条门槛第③条逐字写着「无域知识」。它当初就不该在那儿。
pub(crate) fn projects_root(home: &Path) -> PathBuf {
    home.join("projects")
}

/// `<home>/sessions` —— **pidfile 目录**（`<PID>.json`，判活用）。
///
/// ⚠ 与 Codex 的 `sessions/`（会话记录根）**同名不同物**。`S2` 就是因为这个
/// 把 `sessions/` 从 codex 的针里剔了出去。
pub(crate) fn sessions_root(home: &Path) -> PathBuf {
    home.join("sessions")
}

/// 一个路径**在不在 Claude 的那几棵树里** —— `~/.claude*` 那个星号的**唯一住址**。
///
/// 给新的写模块定的围栏逐字是
/// 「写点……**不许**落进 `~/.claude*` 那几棵树」。这句话里的**布局知识**
/// （根叫什么、星号包含哪些）归本层 —— `control/` 是通用层，
/// 它不该知道这个目录叫什么（`agent_locality_guard` 的针就钉在这上面）。
///
/// 两条各治一形，任一命中即真：
///
/// 1. **在配置根之下**（含它自己）。根由 [`resolve_home`] 现打解析，所以账号隔离
///    把根切到一个**不带这个名字**的地方时（每个具名账号都是这样），
///    这一条仍然认得出来。
/// 2. **任何一段以 [`HOME_DIR_NAME`] 开头**。它兜的是第 1 条够不着的那些树：
///    此刻**没有被选中**的那几个账号目录、同名的备份文件、工程里的那一份。
///
/// ⚠ [`Path::starts_with`] 是**按段**比的 ⇒ 同前缀的兄弟目录不会被第 1 条误判；
/// 而它会被第 2 条拦下 —— 这是刻意的，星号逐字包含它。
///
/// ⚠ **它不解 symlink**：入参是什么就判什么。要挡「目录里藏一条指过去的链接」，
/// 得由调用方先把路径解成真路径再来问（`control/files_write.rs` 的围栏② 就是那么做的）。
pub fn is_inside_tree(home: &Path, target: &Path) -> bool {
    if target.starts_with(home) {
        return true;
    }
    target.components().any(|c| match c {
        std::path::Component::Normal(seg) => {
            seg.to_str().is_some_and(|s| s.starts_with(HOME_DIR_NAME))
        }
        _ => false,
    })
}

/// 🔴 **一条路径是不是 Claude 的「会话记录」那几份具体文件的形状**（`projects/<proj>/<sid>.jsonl` 恰 2 段 ·
/// `sessions/<x>.json` 恰 1 段）。
///
/// # 〔用户〕它**不再是写侧围栏**
///
/// 用户原话「**文件管理器全部都可以改. 不需要任何围栏**」⇒ 文件管理写面（`control/files_write.rs`）
/// 不再问它。后端里它今天**只有删历史会话那一条**在问（[`session_file_for_delete_in`] 与
/// `files_write::fenced_session_file`：「要删的必须**是**一份会话记录」—— 方向与从前那道围栏相反）。
/// 旧名 `is_protected_session_file` / `is_protected_session_path`〔散文墓碑〕：「protected」在后端从此是假的，名字改成它真在答的那一问。
/// 桥那一份逐字副本（`claude_data_fence`〔散文墓碑〕）随它最后一个用户搬进后端，全仓只剩这一份；
/// 那个用户（skill 收件箱编辑面）09-30 整块删了。下面几节是它当写侧围栏那一段的历史。
///
/// 〔波 5 ㈢ · 2026-09-23 · 用户 2026-09-23 逐字裁「文件管理器该不该能改 `~/.claude`
/// 里的东西. **可以.**」〕
///
/// # 它换掉了什么，以及为什么
///
/// 现打过一件事：同一次「往 `~/.claude/skills/` 里写」的操作，
/// 在两条路上会得到**两种结果** —— 后端那条问 [`is_inside_tree`]（拒**整棵树**），
/// 桥／SFTP 那条问桥那一份会话形状判定〔散文墓碑〕
/// （只拒**那几份具体的会话文件**）。那一节自陈「丙（统一成同一个判定）才是真正的解，
/// 但它是一道产品题不是工程题」。**用户 09-23 把那道产品题裁了**：往窄的那一档统一
/// ⇒ skills / 配置 / 账号库**改得动**，正在跑的那场会话的记录**照旧改不动**。
///
/// # 🔴 它是那个判定的**逐字副本**，而不是第二个判定
///
/// 两个 crate 之间没有共享落点：`src/backend` **刻意不在** monitor 那个 workspace 里
/// （它有自己的 `Cargo.lock`，那条隔离是真架构约束，见它 `Cargo.toml` 头注），
/// 而新立一个共享 crate 会动门禁那句 `run_gate_sum cargo 9`（本刀写区之外），
/// 并且记着上一次「把围栏搬成共享 crate」当天就被撤回。
/// ⇒ 处置：**函数体逐字节相同**，并由判据把这件事钉成相等断言 ——
/// 两侧任何一处改动、另一处不跟，当场红。桥那一份删了，那条相等判据随之退役（全仓只剩这一份）。
///
/// ⚠ **方向相反的那一道不在这儿，也不许合并**：从前是桥那一侧的 `is_safe_remote_jsonl`〔散文墓碑〕，
/// 今天是本文件的 [`session_file_for_delete_in`]，正题恰恰是
/// 「**只许**删恰是 `projects/<proj>/<sid>.jsonl` 的那一份」（`INVARIANTS §1` 例外 3，历史浏览器删会话）。
/// 两道都读 Claude 的目录结构、方向相反，合成一个之后「哪些不许写」与「哪些才许删」
/// 会共用一个真相，而它们要的恰好是补集。
///
/// # ⚠ 它**不**管什么（如实登记，别读宽）
///
/// - **只看路径的形状**，不看那场会话是不是真的活着（要那个得问 pidfile / session_map，
///   而那在写路径上会变成一次多余的 IO）。
/// - **不认非会话的那些东西**：`settings.json` · `skills/**` · 账号库 · `.credentials.json`
///   —— 全部**放行**，那正是用户这一裁要买的东西。谁要收回这一格，那是下一道产品题。
/// - 它管不着的那几类路径登记成一张读数表
///   （今天住 `paths_tests.rs::every_uncovered_shape_is_still_uncovered_today`）。
pub fn is_session_record_file(path: &str) -> bool {
    let p = path.replace('\\', "/");
    // batch20 审计修：**结构判定**，不靠 `/.claude/` 字面——Claude 数据文件结构为 `<任意>/projects/<proj>/<sid>.jsonl`
    // （projects 下恰 2 段）或 `<任意>/sessions/<x>.json`（sessions 下 1 段）。**闭 `CLAUDE_CONFIG_DIR` 重定位缺口**：
    // 重定位后路径成 `<CFGDIR>/projects/.../*.jsonl`，原字面 `/.claude/` 判定会漏、SFTP 面板可覆写 live jsonl。
    let jsonl_protected = p.rfind("/projects/").is_some_and(|i| {
        let parts: Vec<&str> = p[i + "/projects/".len()..].split('/').collect();
        parts.len() == 2
            && !parts[0].is_empty()
            && parts[1].len() > ".jsonl".len()
            && parts[1].ends_with(".jsonl")
    });
    let json_protected = p.rfind("/sessions/").is_some_and(|i| {
        let rest = &p[i + "/sessions/".len()..];
        !rest.contains('/') && rest.len() > ".json".len() && rest.ends_with(".json")
    });
    jsonl_protected || json_protected
}

/// [`is_session_record_file`] 的 `&Path` 门面。
///
/// ⚠ **有损转换如实登记**：非 UTF-8 的路径字节经 `to_string_lossy` 变成 U+FFFD。
/// 那**不会**造成漏判（受保护那两形要的是段数 ＋ `.jsonl` / `.json` 后缀，
/// 而替换字符只出现在段**内部**，段数与后缀都活着），但它确实让「那一段原本是什么字节」
/// 在这一问里不可知。⇒ 判定的语料是**字符串**，这条围栏从此与那一事实对齐。
pub fn is_session_record_path(target: &Path) -> bool {
    is_session_record_file(&target.to_string_lossy())
}

/// 「删除历史会话」那一条**要删的那一份在哪** —— 只收 sid。
///
/// 只有后端的文件管理部分写文件（本机也算）⇒ 删历史会话（本机那一支此前是 monitor
/// 进程直删，远端那一支是 SFTP 直删）改成后端**一条明确的命令**。它**不收路径**：
/// 落点由这里按 sid 在本机记录树里找，调用方连表达「另一份文件」的办法都没有。
///
/// 三关，各治一形：
///
/// 1. 找：[`super::branch::find_session_file`]（与分叉那条同一份；sid 形状不合法**先于任何 IO** 就拒，
///    符号链接不算命中）。
/// 2. **解到底再判一次**：真路径必须在记录树（也解到底）底下、恰好 `<proj>/<sid>.jsonl` 两段 ——
///    子代理那种更深的文件、一条指出去的链接，都在这一关被拒。
/// 3. 解完的那一份**必须是**会话记录的形状（[`is_session_record_path`] 答真）——
///    它删的恰恰是那一类，别的一样都删不到。
///
/// `home` 由调用方给：生产侧是 [`resolve_home`]（见 [`session_file_for_delete`]），
/// 判据拿临时目录当 home，不碰真实配置根，也不改测试进程的环境变量。
pub fn session_file_for_delete_in(home: &Path, sid: &str) -> Result<PathBuf, String> {
    let root = projects_root(home);
    let found = super::branch::find_session_file(&root, sid).map_err(|e| {
        copy_text(
            "beClaudePaths.delete.notFound",
            &[("id", sid), ("e", &e.to_string())],
        )
    })?;
    let real_root = std::fs::canonicalize(&root).map_err(|e| {
        copy_text(
            "beClaudePaths.delete.rootUnresolved",
            &[
                ("id", sid),
                ("path", &root.display().to_string()),
                ("e", &e.to_string()),
            ],
        )
    })?;
    let real = std::fs::canonicalize(&found).map_err(|e| {
        copy_text(
            "beClaudePaths.delete.fileUnresolved",
            &[
                ("id", sid),
                ("path", &found.display().to_string()),
                ("e", &e.to_string()),
            ],
        )
    })?;
    let rel = real.strip_prefix(&real_root).map_err(|_| {
        copy_text(
            "beClaudePaths.delete.outsideRoot",
            &[
                ("id", sid),
                ("path", &real.display().to_string()),
                ("root", &real_root.display().to_string()),
            ],
        )
    })?;
    let want = format!("{sid}.jsonl");
    let segs: Vec<&std::ffi::OsStr> = rel.iter().collect();
    if segs.len() != 2 || segs[1] != std::ffi::OsStr::new(&want) {
        return Err(copy_text(
            "beClaudePaths.delete.wrongShape",
            &[
                ("id", sid),
                ("want", &want.to_string()),
                ("path", &rel.display().to_string()),
            ],
        ));
    }
    if !is_session_record_path(&real) {
        return Err(copy_text(
            "beClaudePaths.delete.notRecord",
            &[("id", sid), ("path", &real.display().to_string())],
        ));
    }
    Ok(real)
}

/// [`session_file_for_delete_in`] 的生产入口：根取本机后端此刻的配置根（[`resolve_home`]）。
pub fn session_file_for_delete(sid: &str) -> Result<PathBuf, String> {
    session_file_for_delete_in(&resolve_home(), sid)
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/paths_tests.rs"]
mod tests;
