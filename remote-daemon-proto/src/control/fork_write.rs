//! `--fork-session`：在**远端本地**把一个会话从指定消息处分叉出一个新会话文件。
//!
//! # 为什么这一步必须在 daemon 里做
//!
//! 分叉要读整份 jsonl 的祖先链。真机会话动辄几十 MB —— 为了分叉把它拉过 ssh 再算完写回去，
//! 是两趟大文件传输。daemon 就跑在会话所在那台机器上，读写都是本地。
//!
//! # ★ 本模块是 daemon **唯一**被允许写文件系统的地方
//!
//! `readonly_guard` 对它另有一套**更严**的断言（见该文件的白名单层）：
//!
//! - **只准 `create_new`（`O_EXCL`）新建**：目标已存在直接失败。
//! - **不得**删除、改名、复制、建硬链软链。
//! - **不得**用截断或追加打开 —— 那两样能改到既有文件。
//! - **不得**整文件覆盖写、也不建目录（projects 目录本来就在）。
//!
//! ⚠ **上面这几条刻意不写出那些函数的字面名字**。护栏是**子串扫描、不剥注释**，
//! 把 `fs`+`::`+`write` 这种词原样写进注释，会让这个模块被自己的文档判成违规
//! —— 本仓「把注释当代码」已栽过四次（见 `test-support/strip-comments.ts` 头注）。
//! **要改就改措辞，别去放宽护栏。**
//!
//! 这条边界背后的判据不是「daemon 不许碰文件系统」，而是
//! **「daemon 不许改动用户既有数据」**。`O_EXCL` 新建一个此前不存在的文件不违反后者
//! —— 详见 `doc/INVARIANTS.md` I7 与 `.claude/planned-build/branch-anywhere/MASTERPLAN.md §4`。
//!
//! # 变换逻辑不在这里
//!
//! 记录变换走共享 crate `branch-core`（monitor 与 daemon **同一份实现**，G1）。
//! 本模块只负责：定位源文件 → 路径守卫 → 调变换 → `O_EXCL` 落盘。

// U2：合并进 `common/paths.rs`（原来这里各有一份逐字相同的副本）。
use crate::common::paths::projects_root;
use std::path::{Path, PathBuf};

/// 成功时 stdout 输出的一行 JSON（camelCase，与 monitor 侧 `BranchResult` 同形）。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ForkResult {
    session_id: String,
    jsonl_path: String,
}

/// 在 `<claude_dir>/projects/**/` 里按 sid 找那份 `<sid>.jsonl`。
///
/// **不接受调用方传路径**（monitor 侧那个 `validate_branch_source` 收的是路径，
/// 这里刻意只收 sid）：daemon 是被 ssh 远程调起来的，少一个可被构造的路径入参，
/// 就少一条路径穿越的攻击面。sid 先过格式校验，再只在 projects 下按文件名匹配。
fn find_session_file(claude_dir: &Path, sid: &str) -> Result<PathBuf, String> {
    if !is_plain_sid(sid) {
        return Err(format!("refuse fork: invalid session id {sid:?}"));
    }
    let root = projects_root(claude_dir);
    let want = format!("{sid}.jsonl");
    for entry in walkdir::WalkDir::new(&root)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() && entry.file_name().to_str() == Some(want.as_str()) {
            return Ok(entry.into_path());
        }
    }
    Err(format!(
        "refuse fork: session {sid} not found under projects/"
    ))
}

/// sid 只许 `[A-Za-z0-9-]`。挡掉 `..`、`/`、`\` 与任何能拼出别处路径的字符。
fn is_plain_sid(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// 单份会话 jsonl 的读取上限。
///
/// **Phase G 审计补的**：原来是裸 `read_to_string`（无上限）。同一个 crate 里
/// `common::fs::read_regular_capped` 早就为这件事写好了函数**和**一句结论
///（「`read_to_string` 无上限 → 远端 OOM」，还实测过 symlink→/dev/zero 六秒涨 11GB），
/// 分叉这条新路却没用它。真机上会话 jsonl 到几十 MB 是常态（本仓注释里记过 37MB 一份），
/// daemon 常跑在树莓派/SBC 上，原文 String + 全量 `Value` 双份驻留很容易把它按死。
///
/// 次生危害更隐蔽：进程被 OOM-killer 杀掉时 sshd 送的是 `exit-signal` 而不是 `exit-status`，
/// monitor 侧 `interpret_fork_exec` 会看到 `exit_status: None` ⇒ 报「没收到退出码，连接可能中断」
/// ⇒ 把排查方向带到网络上去。
///
/// 256MB：与 monitor 侧 `remote_history::MAX_SESSION_BYTES` 同一量级，正常会话远够不到。
const MAX_SESSION_JSONL_BYTES: u64 = 256 * 1024 * 1024;

/// 读一个 jsonl 为逐行 `Value`（剥 BOM、跳空行、坏行忽略——口径同 monitor 侧）。
fn read_jsonl(path: &Path) -> Result<Vec<serde_json::Value>, String> {
    // 走共享的**安全读**（先确认是常规文件，挡掉 FIFO/设备，再 take(cap) 限量）——
    // 不是自己再写一遍 `read_to_string`。
    let bytes = crate::common::fs::read_regular_capped(path, MAX_SESSION_JSONL_BYTES)
        .map_err(|e| format!("refuse fork: read {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(text
        .lines()
        .map(|l| l.trim_start_matches('\u{feff}').trim())
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .collect())
}

/// 生成一个新 sid。**不引 uuid crate**（daemon 依赖表刻意极简，见 Cargo.toml 抬头）：
/// 用「时间 + 进程 id + 源 sid 的哈希」拼一个 v4 形状的串。
///
/// 唯一性不靠这个串本身保证 —— **靠 `O_EXCL`**：撞了就直接失败，绝不覆盖。
fn new_session_id(source_sid: &str) -> String {
    use std::hash::{Hash, Hasher};
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source_sid.hash(&mut h);
    std::process::id().hash(&mut h);
    nanos.hash(&mut h);
    let a = h.finish();
    let mut h2 = std::collections::hash_map::DefaultHasher::new();
    a.hash(&mut h2);
    nanos.hash(&mut h2);
    let b = h2.finish();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (a >> 32) as u32,
        (a >> 16) as u16,
        (a & 0x0fff) as u16,
        ((b >> 48) as u16 & 0x3fff) | 0x8000,
        b & 0xffff_ffff_ffff
    )
}

/// `--fork-session <source-sid> <message-uuid>`：stdout 出一行 `ForkResult` JSON、exit 0；
/// 出错 exit 2 + stderr 出 `{code,message}`（与 `--resolve` 同一个错误信封约定）。
pub fn run(claude_dir: &Path, args: &[String]) -> i32 {
    match run_inner(claude_dir, args) {
        Ok(res) => {
            match serde_json::to_string(&res) {
                Ok(s) => println!("{s}"),
                Err(e) => return fail("serialize", &e.to_string()),
            }
            0
        }
        Err(msg) => fail("fork_failed", &msg),
    }
}

fn fail(code: &str, message: &str) -> i32 {
    let env = serde_json::json!({ "code": code, "message": message });
    eprintln!("{env}");
    2
}

fn run_inner(claude_dir: &Path, args: &[String]) -> Result<ForkResult, String> {
    let source_sid = args
        .get(1)
        .ok_or("usage: --fork-session <source-sid> <message-uuid>")?;
    let message_uuid = args
        .get(2)
        .ok_or("usage: --fork-session <source-sid> <message-uuid>")?;

    let source = find_session_file(claude_dir, source_sid)?;
    let lines = read_jsonl(&source)?;
    let new_sid = new_session_id(source_sid);
    let records = branch_core::build_branch_records(&lines, message_uuid, source_sid, &new_sid)?;

    // 落点 = 源文件同目录（那已是 projects 下某个项目目录），文件名 = 新 sid。
    let dir = source
        .parent()
        .ok_or("refuse fork: source has no parent dir")?;
    let out_path = dir.join(format!("{new_sid}.jsonl"));
    write_new_file(&out_path, &records)?;

    Ok(ForkResult {
        session_id: new_sid,
        jsonl_path: out_path.to_string_lossy().into_owned(),
    })
}

/// **唯一的写盘处**。`create_new(true)` = `O_EXCL`：
/// 目标已存在直接失败，既消掉 `exists()→write` 的 TOCTOU 窗口，
/// 也自证「绝不覆盖任何现存会话」——两个 monitor 同时分叉同一会话时，
/// 后到的那个会拿到错误而不是把先到的那份盖掉。
fn write_new_file(out_path: &Path, records: &[serde_json::Value]) -> Result<(), String> {
    use std::io::Write as _;
    let mut body = String::new();
    for rec in records {
        body.push_str(&serde_json::to_string(rec).map_err(|e| format!("serialize: {e}"))?);
        body.push('\n');
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out_path)
        .map_err(|e| format!("refuse fork: create {}: {e}", out_path.display()))?;
    // 写失败（磁盘满等）也要把原因带回 UI —— 静默半截文件比报错糟得多。
    f.write_all(body.as_bytes())
        .map_err(|e| format!("refuse fork: write {}: {e}", out_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("ccm-fork-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(p.join("projects").join("proj")).unwrap();
        p
    }

    fn seed(dir: &Path, sid: &str) {
        let rows = [
            serde_json::json!({"type":"user","uuid":"u1","parentUuid":null,"timestamp":"t1","sessionId":sid}),
            serde_json::json!({"type":"assistant","uuid":"u2","parentUuid":"u1","timestamp":"t2","sessionId":sid}),
            serde_json::json!({"type":"user","uuid":"u3","parentUuid":"u2","timestamp":"t3","sessionId":sid}),
        ];
        let body: String = rows
            .iter()
            .map(|r| format!("{r}\n"))
            .collect::<Vec<_>>()
            .concat();
        std::fs::write(
            dir.join("projects")
                .join("proj")
                .join(format!("{sid}.jsonl")),
            body,
        )
        .unwrap();
    }

    #[test]
    fn fork_writes_new_file_and_leaves_source_untouched() {
        let root = tmp("ok");
        seed(&root, "srcsid");
        let src = root.join("projects").join("proj").join("srcsid.jsonl");
        let before = std::fs::read(&src).unwrap();

        let res = run_inner(&root, &sargs(&["--fork-session", "srcsid", "u2"])).unwrap();

        assert_eq!(std::fs::read(&src).unwrap(), before, "源文件被改动了");
        let out = PathBuf::from(&res.jsonl_path);
        assert!(out.exists());
        assert_eq!(
            out.parent().unwrap(),
            src.parent().unwrap(),
            "应落在源同目录"
        );
        let rows: Vec<serde_json::Value> = std::fs::read_to_string(&out)
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        // 走的是共享变换：祖先链 u1→u2，sessionId 换新
        let uuids: Vec<&str> = rows.iter().map(|r| r["uuid"].as_str().unwrap()).collect();
        assert_eq!(uuids, vec!["u1", "u2"]);
        assert_eq!(rows[0]["sessionId"].as_str().unwrap(), res.session_id);
        std::fs::remove_dir_all(&root).ok();
    }

    /// ★ 并发/覆盖：`O_EXCL` 必须让第二次写落到错误上，而不是盖掉第一份。
    #[test]
    fn write_new_file_refuses_existing_target() {
        let root = tmp("excl");
        let p = root.join("projects").join("proj").join("dup.jsonl");
        std::fs::write(&p, "PREEXISTING\n").unwrap();
        let err = write_new_file(&p, &[serde_json::json!({"x":1})]).unwrap_err();
        assert!(err.contains("create"), "got: {err}");
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "PREEXISTING\n",
            "已存在的文件被覆盖了"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_path_traversal_sid() {
        let root = tmp("trav");
        for bad in ["../../etc/passwd", "a/b", "..", "a\\b", ""] {
            assert!(find_session_file(&root, bad).is_err(), "sid {bad:?} 应被拒");
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_session_is_an_error_not_a_panic() {
        let root = tmp("missing");
        let err = find_session_file(&root, "nope").unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
        std::fs::remove_dir_all(&root).ok();
    }

    /// 子 agent 记录不可分叉 —— 这条判据在共享 crate 里，这里钉住 daemon 真的走了它。
    #[test]
    fn sidechain_reject_reaches_daemon_path() {
        let root = tmp("side");
        seed(&root, "sc");
        let f = root.join("projects").join("proj").join("sc.jsonl");
        let mut body = std::fs::read_to_string(&f).unwrap();
        body.push_str(
            &serde_json::json!({"type":"assistant","uuid":"s1","parentUuid":"u2",
                "timestamp":"t9","sessionId":"sc","isSidechain":true})
            .to_string(),
        );
        body.push('\n');
        std::fs::write(&f, body).unwrap();
        let err = run_inner(&root, &sargs(&["--fork-session", "sc", "s1"])).unwrap_err();
        assert!(err.contains("sidechain"), "got: {err}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn new_session_id_has_uuid_shape_and_varies() {
        let a = new_session_id("x");
        assert_eq!(a.len(), 36, "{a}");
        assert_eq!(a.split('-').count(), 5, "{a}");
        // 同一输入连续两次也应不同（含 nanos）——但唯一性真正靠 O_EXCL，不靠这个。
        assert_ne!(a, new_session_id("x"));
    }

    fn sargs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
}
