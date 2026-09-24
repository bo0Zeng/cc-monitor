//! Claude 的目录布局：配置根怎么解析、根下面有哪两个子目录。

use std::path::{Path, PathBuf};

/// 覆盖配置根的环境变量名。**账号隔离（cc-acct-iso）就是靠切它**，
/// 所以它不只是"一个环境变量"，是账号这个概念在 Claude 侧的载体。
pub(crate) const CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";

/// 默认配置根在 `$HOME` 下的名字。
const HOME_DIR_NAME: &str = ".claude";

/// 解析配置根：`$CLAUDE_CONFIG_DIR` 优先，否则 `$HOME/.claude`。
///
/// Windows（只为编译/冒烟，真实目标是 Linux）：`$HOME` 缺失时退 `%USERPROFILE%\.claude`，
/// 再退 cwd 下的 `.claude`，让二进制至少起得来。
///
/// `S3` 从 `main.rs::resolve_claude_dir` 原样搬来（逻辑一字未改）。
pub fn resolve_home() -> PathBuf {
    if let Some(dir) = std::env::var_os(CONFIG_DIR_ENV) {
        return PathBuf::from(dir);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(HOME_DIR_NAME);
    }
    #[cfg(windows)]
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(profile).join(HOME_DIR_NAME);
    }
    PathBuf::from(HOME_DIR_NAME)
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
/// 〔步 23b · 2026-09-19〕`设计/60 §6.5.2 A` 给新的写模块定的围栏逐字是
/// 「写点……**不许**落进 `~/.claude*` 那几棵树」。这句话里的**布局知识**
/// （根叫什么、星号包含哪些）归本层 —— `control/` 是通用层，
/// 它不该知道这个目录叫什么（`agent_locality_guard` 的针就钉在这上面）。
///
/// 两条各治一形，任一命中即真：
///
/// 1. **在配置根之下**（含它自己）。根由 [`resolve_home`] 现打解析，所以账号隔离
///    把根切到一个**不带这个名字**的地方时（`cc-acct-iso` 每天在做的事），
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

/// 🔴 **一条路径是不是 Claude 的「会话数据」那几份具体文件** —— 写侧围栏的判定。
///
/// 〔波 5 ㈢ · 2026-09-23 · 用户 2026-09-23 逐字裁「文件管理器该不该能改 `~/.claude`
/// 里的东西. **可以.**」〕
///
/// # 它换掉了什么，以及为什么
///
/// `设计/60 §8.7` 现打过一件事：同一次「往 `~/.claude/skills/` 里写」的操作，
/// 在两条路上会得到**两种结果** —— 后端那条问 [`is_inside_tree`]（拒**整棵树**），
/// 桥／SFTP 那条问 `claude_data_fence::is_protected_claude_data_path`
/// （只拒**那几份具体的会话文件**）。那一节自陈「丙（统一成同一个判定）才是真正的解，
/// 但它是一道产品题不是工程题」。**用户 09-23 把那道产品题裁了**：往窄的那一档统一
/// ⇒ skills / 配置 / 账号库**改得动**，正在跑的那场会话的记录**照旧改不动**。
///
/// # 🔴 它是那个判定的**逐字副本**，而不是第二个判定
///
/// 两个 crate 之间没有共享落点：`src/backend` **刻意不在** monitor 那个 workspace 里
/// （它有自己的 `Cargo.lock`，那条隔离是真架构约束，见它 `Cargo.toml` 头注），
/// 而新立一个共享 crate 会动门禁那句 `run_gate_sum cargo 9`（本刀写区之外），
/// 并且 `设计/60 §8.8` 记着上一次「把围栏搬成共享 crate」当天就被撤回。
/// ⇒ 处置：**函数体逐字节相同**，并由判据把这件事钉成相等断言 ——
/// 两侧任何一处改动、另一处不跟，当场红。判据两棵树各一份：
/// `tests/backend/agents_tests.rs` 与 `tests/bridge/claude_data_fence_tests.rs`。
///
/// ⚠ **方向相反的那一道不在这儿，也不许合并**：`sftp::is_safe_remote_jsonl` 的正题恰恰是
/// 「**只许**删 `projects/**/*.jsonl`」（`INVARIANTS §1` 例外 3，历史浏览器删远端会话）。
/// 两道都读 Claude 的目录结构、方向相反，合成一个之后「哪些不许写」与「哪些才许删」
/// 会共用一个真相，而它们要的恰好是补集。
///
/// # ⚠ 它**不**管什么（如实登记，别读宽）
///
/// - **只看路径的形状**，不看那场会话是不是真的活着（要那个得问 pidfile / session_map，
///   而那在写路径上会变成一次多余的 IO）。
/// - **不认非会话的那些东西**：`settings.json` · `skills/**` · 账号库 · `.credentials.json`
///   —— 全部**放行**，那正是用户这一裁要买的东西。谁要收回这一格，那是下一道产品题。
/// - 它管不着的那几类路径**与桥那一侧逐字同一张表**
///   （`claude_data_fence_tests::THE_SHAPES_THIS_FENCE_DOES_NOT_COVER`）——
///   本处刻意不抄第二份，抄了就是第二个会漂的住址。
pub fn is_protected_session_file(path: &str) -> bool {
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

/// [`is_protected_session_file`] 的 `&Path` 门面。
///
/// ⚠ **有损转换如实登记**：非 UTF-8 的路径字节经 `to_string_lossy` 变成 U+FFFD。
/// 那**不会**造成漏判（受保护那两形要的是段数 ＋ `.jsonl` / `.json` 后缀，
/// 而替换字符只出现在段**内部**，段数与后缀都活着），但它确实让「那一段原本是什么字节」
/// 在这一问里不可知。⇒ 判定的语料是**字符串**，这条围栏从此与那一事实对齐。
pub fn is_protected_session_path(target: &Path) -> bool {
    is_protected_session_file(&target.to_string_lossy())
}

/// 〔RW1 · 第四波 · 2026-09-24〕「删除历史会话」那一条**要删的那一份在哪** —— 只收 sid。
///
/// 用户裁「只允许后端的文件管理部分写文件」「也管本机」⇒ 删历史会话（本机那一支此前是 monitor
/// 进程直删，远端那一支是 SFTP 直删）改成后端**一条明确的命令**。那条命令是
/// 会话文件围栏**唯一的例外**（别的写一律不许碰这几份文件），所以它**不收路径**：
/// 落点由这里按 sid 在本机记录树里找，调用方连表达「另一份文件」的办法都没有。
///
/// 三关，各治一形：
///
/// 1. 找：`branch_core::find_session_file`（与分叉那条同一份；sid 形状不合法**先于任何 IO** 就拒，
///    符号链接不算命中）。
/// 2. **解到底再判一次**：真路径必须在记录树（也解到底）底下、恰好 `<proj>/<sid>.jsonl` 两段 ——
///    子代理那种更深的文件、一条指出去的链接，都在这一关被拒。
/// 3. 解完的那一份**必须是**会话文件围栏认得的形状（[`is_protected_session_path`] 答真）——
///    这是「例外」的定义：它删的恰恰是围栏保护的那一类，别的一样都删不到。
///
/// `home` 由调用方给：生产侧是 [`resolve_home`]（见 [`session_file_for_delete`]），
/// 判据拿临时目录当 home，不碰真实配置根，也不改测试进程的环境变量。
pub fn session_file_for_delete_in(home: &Path, sid: &str) -> Result<PathBuf, String> {
    let root = projects_root(home);
    let found =
        branch_core::find_session_file(&root, sid).map_err(|e| format!("删不了会话 {sid}：{e}"))?;
    let real_root = std::fs::canonicalize(&root).map_err(|e| {
        format!(
            "删不了会话 {sid}：记录树解析不了（{}：{e}）",
            root.display()
        )
    })?;
    let real = std::fs::canonicalize(&found).map_err(|e| {
        format!(
            "删不了会话 {sid}：那份文件解析不了（{}：{e}）",
            found.display()
        )
    })?;
    let rel = real.strip_prefix(&real_root).map_err(|_| {
        format!(
            "删不了会话 {sid}：解完链接之后那份文件不在记录树里（{} 不在 {} 里）",
            real.display(),
            real_root.display()
        )
    })?;
    let want = format!("{sid}.jsonl");
    let segs: Vec<&std::ffi::OsStr> = rel.iter().collect();
    if segs.len() != 2 || segs[1] != std::ffi::OsStr::new(&want) {
        return Err(format!(
            "删不了会话 {sid}：只删 `<项目>/{want}` 这一形（找到的是 {}）",
            rel.display()
        ));
    }
    if !is_protected_session_path(&real) {
        return Err(format!(
            "删不了会话 {sid}：{} 不是一份会话记录的形状",
            real.display()
        ));
    }
    Ok(real)
}

/// [`session_file_for_delete_in`] 的生产入口：根取本机后端此刻的配置根（[`resolve_home`]）。
pub fn session_file_for_delete(sid: &str) -> Result<PathBuf, String> {
    session_file_for_delete_in(&resolve_home(), sid)
}
