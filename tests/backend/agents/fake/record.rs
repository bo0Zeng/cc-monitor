//! 假 agent 的记录面（`RecordFace`）：记录根 · 会话文件判定与命名 · 按 sid 找记录 · 链事实。
//! 与 Claude 那家每一格都不同形（见 `fake/mod.rs` 头注那张表）。

use crate::platform::fs::Ro;
use std::path::{Path, PathBuf};

/// 能力 1：会话记录根。**`convos` 不是 `projects`**。
pub fn records_root(home: &Path) -> PathBuf {
    home.join("convos")
}

/// 能力 2：这个路径是不是一份会话记录。**`.ndjson` 不是 `.jsonl`**。
pub fn is_session_file(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "ndjson")
        && p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("sess-"))
}

/// 能力 3：会话 id → 记录文件名。**带前缀**，与 Claude 的裸 `<sid>.<ext>` 不同形。
pub fn session_file_name(sid: &str) -> String {
    format!("sess-{sid}.ndjson")
}

/// 按 sid 找记录：`<记录根>/<项目>/sess-<sid>.ndjson`（项目目录按名字次序问）。
fn find_session(root: &Path, sid: &str) -> Result<PathBuf, String> {
    if !shell_quote_core::session_id_ok(sid) {
        return Err(format!("fake: invalid session id {sid:?}"));
    }
    let name = session_file_name(sid);
    let mut projects: Vec<PathBuf> = Ro::read_dir(root)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .collect();
    projects.sort();
    projects
        .into_iter()
        .map(|p| p.join(&name))
        .find(|f| f.is_file())
        .ok_or_else(|| format!("fake: no {name}"))
}

thread_local! {
    /// 自己叉那一下被叫时收到的 `(源 sid, 消息 id)`，依次记（判据在同一条线程上读）。
    static OWN_FORK_CALLS: std::cell::RefCell<Vec<(String, String)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// 取走这条线程上记下的自己叉的调用（取完即空）。
pub fn own_fork_calls() -> Vec<(String, String)> {
    OWN_FORK_CALLS.with(|c| std::mem::take(&mut *c.borrow_mut()))
}

/// 分叉面：自己叉（与 Claude 的改写记录不同形）。先记下收到的源 sid 与消息 id；那条消息在源记录的链上（`node`）⇒ 交回固定的 `new-1`，
/// 不在 ⇒ `NoTurn`；源记录找不着 ⇒ `Failed`。一个文件都不写。
fn own_fork(at: &crate::agents::OwnFork<'_>) -> Result<String, crate::agents::ForkFail> {
    use crate::agents::ForkFail;
    OWN_FORK_CALLS.with(|c| {
        c.borrow_mut()
            .push((at.sid.to_string(), at.message.to_string()))
    });
    let file = find_session(&records_root(at.home), at.sid).map_err(|_| ForkFail::Failed)?;
    let bytes = Ro::read_capped(&file, super::FAKE_FILE_CAP).map_err(|_| ForkFail::Failed)?;
    let known = String::from_utf8_lossy(&bytes).lines().any(|l| {
        serde_json::from_str::<serde_json::Value>(l)
            .ok()
            .and_then(|v| v.get("node").and_then(|n| n.as_str()).map(str::to_string))
            .is_some_and(|n| n == at.message)
    });
    if known {
        Ok("new-1".to_string())
    } else {
        Err(ForkFail::NoTurn {
            message: at.message.to_string(),
        })
    }
}

/// 记录解释面：只带记录树与文件判定（本假 agent 的记录内容通用判据用不到 ⇒ 解析恒「这一行不出成品」）。
pub(super) fn records_face(
    root: fn(&Path) -> PathBuf,
    is_session_file: fn(&Path) -> bool,
    file_name: fn(&str) -> String,
) -> crate::agents::RecordFace {
    crate::agents::RecordFace {
        parse: parse_line,
        sid: session_id_of,
        is_session_file,
        tree: Some(crate::agents::RecordTree { root, file_name }),
        activity: None,
        chain: Some(chain_fact),
        find_session,
        branch: Some(crate::agents::BranchFace::Own(own_fork)),
        drift: None,
        text: None,
        delete: None,
        archive: Some(ARCHIVE),
        response_id: None,
        run_of: None,
        child_link: None,
        background: None,
        children: None,
        project_dir: None,
        mcp_said: None,
        quiet_unread: None,
        skim: Some(skim),
        beats: Some(BEATS),
    }
}

/// 记录面的 `skim`：簿记行（`"role":"note"`）不进界面 ⇒ 只留链事实那几格（`role` · `node` · `up` · `when` · `cut`），话不留。
/// 别的行 · 连 JSON 都不是 ⇒ 整行读。
fn skim(line: &crate::agents::Line<'_>) -> Option<serde_json::Value> {
    let v: serde_json::Map<String, serde_json::Value> = line.read().ok()?;
    if v.get("role").and_then(serde_json::Value::as_str) != Some("note") {
        return None;
    }
    let kept = v
        .into_iter()
        .filter(|(k, _)| ["role", "node", "up", "when", "cut"].contains(&k.as_str()))
        .collect();
    Some(serde_json::Value::Object(kept))
}

/// 记录面的 `beats`：`human` ⇒ 用户那侧 · `bot` ⇒ 代理那侧 · 别的 ⇒ 簿记；`node` 是 id、`when` 是时刻，
/// `from` 写着就是从那个会话分叉来的（与 Claude 的格名都不同）。快路一律放过（只许放过、不许误拦）。
const BEATS: crate::agents::beat::BeatFace = crate::agents::beat::BeatFace {
    beat: beat_of,
    hint: |_, _| true,
    ask: |_| crate::agents::beat::CallAsk::Plain,
};

fn beat_of(v: &serde_json::Value) -> Option<crate::agents::beat::Beat<'_>> {
    use crate::agents::beat::{Beat, Side};
    use std::borrow::Cow;
    let s = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_str)
            .map(Cow::Borrowed)
    };
    let side = match v.get("role").and_then(serde_json::Value::as_str)? {
        "human" => Side::Input,
        "bot" => Side::Reply,
        _ => Side::Note,
    };
    Some(Beat {
        id: s("node"),
        at: s("when"),
        side,
        injected: false,
        acts: Vec::new(),
        usage: None,
        model: None,
        mode: None,
        forked_from: s("from").map(Cow::into_owned),
    })
}

/// 链事实（与 Claude 每一格都不同名）：`{"node", "up", "when", "role": human | bot | note, "cut": bool, "words"}`；
/// 排队那句是 `{"queued": "…"}`。只有 `human` / `bot` 进界面。
pub fn chain_fact(line: &crate::agents::Line<'_>) -> Option<crate::agents::mainline::ChainFact> {
    use crate::agents::mainline::{ChainFact, Link, Words};
    let v = line.value()?;
    let s = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    if let Some(q) = s("queued") {
        return Some(ChainFact::Queued(Words::of(&q)));
    }
    let role = s("role")?;
    Some(ChainFact::Node(Link {
        id: s("node")?,
        parent: s("up"),
        at: s("when")?,
        said: role == "human",
        reply: role == "bot",
        interrupt: v.get("cut").and_then(serde_json::Value::as_bool) == Some(true),
        text: (role == "human").then(|| Words::of(&s("words").unwrap_or_default())),
        shown: role != "note",
    }))
}

/// `sess-<sid>.ndjson`（已归档 ⇒ `sess-<sid>.shelved.ndjson`）⇒ sid。
fn session_id_of(p: &Path) -> Option<String> {
    let name = p.file_name()?.to_str()?;
    let stem = name.strip_prefix("sess-")?.strip_suffix(".ndjson")?;
    Some(stem.strip_suffix(SHELVED).unwrap_or(stem).to_string())
}

/// 已归档的记录文件名上的记号。
const SHELVED: &str = ".shelved";

/// 归档面：归档 ＝ `fakeagent shelve <sid>`，取消归档 ＝ `fakeagent unshelve <sid>`，都带 `FAKE_LOG` 与会话所在那个家 `FAKE_HOME`；
/// 程序是这台 `PATH` 上的 `sh`，跑 [`CLI`]：把收到的 argv 一行一个、连 `FAKE_LOG` 那一格写进 `<FAKE_LOG>/<动词>-<sid>`；
/// sid 以 `bad` 打头 ⇒ 往 stderr 说一句、退出码 3。归档与否照文件名上的 [`SHELVED`] 认。替身自己一个文件都不写。
const ARCHIVE: crate::agents::ArchiveFace = crate::agents::ArchiveFace {
    archive: |home, sid| Some(cli_command("shelve", home, sid)),
    unarchive: |home, sid| Some(cli_command("unshelve", home, sid)),
    archived: |p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".ndjson"))
            .is_some_and(|stem| stem.ends_with(SHELVED))
    },
};

/// 替身程序的本体（`sh -c` 跑，`$0` ＝ `fakeagent`）。
const CLI: &str = "mkdir -p \"$FAKE_LOG\"\n\
     out=\"$FAKE_LOG/$1-$2\"\n\
     for a in \"$@\"; do printf '%s\\n' \"$a\"; done > \"$out\"\n\
     printf 'FAKE_LOG=%s\\n' \"$FAKE_LOG\" >> \"$out\"\n\
     case \"$2\" in bad*) echo 'no such thread' >&2; exit 3;; esac\n";

/// 替身程序的一条命令：`home` ＝ 会话所在那个号的家（照 `FAKE_HOME` 带上，判据看它是不是那个家）。
pub(crate) fn cli_command(verb: &str, home: &Path, sid: &str) -> crate::agents::ArchiveCommand {
    // 这次运行独占的记录目录（只算路径；建它是替身程序的事）：判据读 `<它>/<动词>-<sid>` 看跑过的 argv。
    let log = std::env::temp_dir().join(format!("ccm-fake-cli-{}", std::process::id()));
    crate::agents::ArchiveCommand {
        program: "sh".to_string(),
        args: ["-c", CLI, "fakeagent", verb, sid]
            .map(str::to_string)
            .to_vec(),
        env: vec![
            ("FAKE_LOG", log.into_os_string()),
            ("FAKE_HOME", home.as_os_str().to_os_string()),
        ],
    }
}

/// 解析面：只认自报额度那一种行 `{"meter": {"<窗>": [<已用比例>, <几点重置 unix 秒>], …}}`（与 Claude · Codex 都不同形），
/// 读成不进界面、只带自报额度的那一形；别的行不出成品。
fn parse_line(line: &crate::agents::Line<'_>) -> Result<Option<crate::agents::Translated>, String> {
    let Some(meter) = line
        .value()
        .and_then(|v| v.get("meter"))
        .and_then(serde_json::Value::as_object)
    else {
        return Ok(None);
    };
    let windows = meter
        .iter()
        .map(|(name, cell)| crate::agents::QuotaWindow {
            name: name.clone(),
            used: cell.get(0).and_then(serde_json::Value::as_f64),
            resets_at: cell.get(1).and_then(serde_json::Value::as_u64),
            warned_at: None,
        })
        .collect();
    Ok(Some(crate::agents::Translated {
        record: None,
        cwd: None,
        quota_said: Some(windows),
        unsent: Default::default(),
    }))
}

/// 会话落在另一家记录根下、照路径认不出是哪一家的那一条（认家那几处判据共用）：两家替身（[`super::family_row`]），
/// 都认 `sess-*.ndjson`、都不报自己的家；默认那一家是 `dflt`，会话住在 `other` 的记录根下，`other` 那一行的起会话事实先过 `shape`。
/// `tag` 分开各条判据的沙箱。回 注册表 ＋ 按 sid 找到的那一条（`kind` ＝ `other`）。
#[expect(
    clippy::disallowed_methods,
    reason = "判据夹具：往这条判据自己的临时目录里铺一份会话记录（只在测试档与 test-support 下编）"
)]
pub fn found_in_the_other(
    tag: &str,
    shape: impl FnOnce(crate::agents::LaunchFace) -> crate::agents::LaunchFace,
) -> (Vec<crate::agents::Adapter>, crate::agents::SessionAt) {
    fn no_home() -> Option<PathBuf> {
        None
    }
    let mut other = super::family_row("other", "CCM_OTHER_DIR", "other-w", false, no_home);
    other.launch = other.launch.map(shape);
    let reg = vec![
        super::family_row("dflt", "CCM_DFLT_DIR", "dflt-w", true, no_home),
        other,
    ];
    let home = std::env::temp_dir().join(format!(
        "ccm-found-other-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let sid = "00000000-0000-4000-8000-0000000000c7";
    let dir = records_root(&home).join("p");
    std::fs::create_dir_all(&dir).expect("sandbox");
    std::fs::write(dir.join(session_file_name(sid)), "{}\n").expect("写夹具");
    let homes = [crate::agents::AgentHome {
        kind: "other",
        home,
    }];
    let at = crate::agents::find_session_in_among(&reg, homes.as_slice(), sid).expect("找得到");
    assert_eq!(at.kind, "other", "找到它的是 other 那一根");
    assert_eq!(
        crate::agents::record_kind_among(&reg, &at.file),
        None,
        "夹具要的是照路径认不出那一家"
    );
    (reg, at)
}
