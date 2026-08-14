//! `S6`：**最小假 agent** —— 本区的验收件把 `G1` 成功标准②「加一个新 agent 只需新增
//! `agents/<名>/`，通用层零改动」变成一个**跑得起来、会红**的东西。
//!
//! # ⚠ 先说本件裁掉了什么（`S6#§2` 的第 1 问，B 阶段必须裁）
//!
//! 两条路二选一：
//!
//! - **先立接口再造假 agent** ⇒ `ADAPTER_CALL_SITES` 当场降到个位数，`S6` 就"配叫零改动"；
//! - **先造假 agent、让它反推接口** ⇒ 更合 `D4`，但第一版的"零改动"是**假的**。
//!
//! **裁定：走第二条 —— 本件一处调用点都不收进接口，`ADAPTER_CALL_SITES` 仍是 8 文件 / 27 处。**
//!
//! 三条理由，第一条是决定性的：
//!
//! 1. **验收件不许移动自己的靶子。** `S6` 报的那个数是用来评价 `S1`–`S5` 的。
//!    先立接口的话，`S6` 报的就是**它自己刚做的事**，而 `S1`–`S5` 到底把地基打成什么样
//!    就再也没人量得出来了。`F+` 方向体检刚点过名的风险是「acceptor 全绿而主线为零」，
//!    验收件自己动主线是它的**镜像** —— 主线动了，而没有人在验。
//! 2. **`D2` 逐字排除了这种做法**：「先立判据，让它当场红出一份真实清单，**再逐处搬**」，
//!    并且明写排除「先把文件挪到 `agents/` 目录再说」。那 27 处**就是**那份清单，
//!    一件之内全收 = 一次性大挪动。
//! 3. **`L2` 的钉法逐字**：「`S6` 的最小假 agent —— 它只实现这组接口，**若接口漏了什么，
//!    `S6` 走不通就会红**」。先设计接口、再让假 agent 照着实现，假 agent 就**按构造**
//!    满足接口，那条"会红"永远不可能红。反推的方向被弄反了。
//!
//! **它排除了什么（代价如实写）**：
//!
//! - 排除了「`S6` 交付一个降到个位数的读数」。**本件不会让那个数下降一处**；
//! - 排除了「`S6` 宣布成功标准②成立」。本件的产出是一个**诚实的差距读数**加一副会红的夹具，
//!   不是一张通过证书；
//! - 也放弃了第一条路的真实好处：12 种能力一次收进接口之后，假 agent 会**全程走通**。
//!   那个好处本件拿不到，留给后续（收接口那件）。而它落地的**验收手段**恰恰是本件造出来的。
//!
//! # ⚠ 第 2 问：假 agent 也算一家吗 —— **算半家：进文件树与 `HOMES`，不进 `REGISTRY`**
//!
//! `agents/mod.rs` 里它的模块声明带 `#[cfg(test)]` ⇒ **生产二进制里一个字节都没有它**。
//! 与真加一个 agent 的差别**只有那一行 cfg**（外加它不在 `REGISTRY` 里）。
//! 判据 `the_fixture_agent_never_ships` 双向钉住这两件事；
//! `agent_locality_guard::FIXTURE_HOMES` 登记「谁是夹具家」并**带天花板**——
//! 没有天花板的话，「夹具家」就成了往 `agents/` 里塞东西躲判据①的逃生舱。
//!
//! # 12 种能力（不是 9 —— 本件订正的读数之一）
//!
//! `S6#§0` 与 PM 交底都写「去重之后只是 **9 种能力**」。**实测那 9 种只覆盖 27 处里的 21 处**：
//! 它们漏掉的正是 `control/resolve_query.rs` 的 6 处 —— 也就是 PM 那个 21/27 计数错误的**残留**
//! （`PR-S5 §3.5` 已经点名要求补，件文件的能力清单没跟着补）。resume 那 3 件事
//! （默认命令 · 命令形 · 会话名前缀）**各是一种能力**，与 `会话文件命名` 同一个粒度。
//! ⇒ 反推出来的接口面是 **12 种能力 / 27 处**，逐条对账住
//! `agent_locality_guard::tests::NEW_AGENT_BLOCKERS`。
//!
//! # 这个假 agent 的每一种能力都**刻意与 Claude 不同形**
//!
//! | 能力 | Claude | 本假 agent |
//! |---|---|---|
//! | 会话记录根 | `<home>/projects` | `<home>/convos` |
//! | 会话文件判定 | `.jsonl` | `.ndjson` |
//! | 会话文件命名 | `<sid>.jsonl` | `sess-<sid>.ndjson` |
//! | pidfile 目录 | `<home>/sessions` | `<home>/live` |
//! | 账号环境变量名 | `CLAUDE_CONFIG_DIR` | `CCM_FAKE_ACCOUNT_DIR` |
//! | 账号信任判定 | `.claude.json` → `projects[cwd].hasTrustDialogAccepted` | `fake-config.json` → `trusted[cwd]` |
//! | 判活 cmdline | 含 `claude` / `node` | 含 `fakeagent` |
//! | 解析本机 home | `$CLAUDE_CONFIG_DIR` 否则 `$HOME/.claude`（**恒有值**） | `$CCM_FAKE_AGENT_HOME`，**没有默认** |
//! | 用量聚合 | `usage_core` 扫 `projects/**/*.jsonl` | 扫 `convos/**/*.ndjson` |
//! | resume 默认命令 | `claude` | `fakeagent` |
//! | resume 命令形 | `<base> --resume <sid>`（flag） | `<base> revive <sid>`（子命令） |
//! | resume 会话名前缀 | `cc` | `fk` |
//!
//! ⚠ **"刻意不同"是本件最要紧的一条设计决定**，不是趣味。假 agent 若照抄 Claude 的布局，
//! 通用层拿 Claude 的知识去解释它的 home **恰好也能读出东西** —— 那时"走通全流程"证明的是
//! 「Claude 的布局被施加到了另一个目录上」，不是「通用层能容纳第二种布局」。
//! 那正是一个**粉饰的通过**。
//!
//! # 走全流程的两半，**边界写在明处**
//!
//! [`walk`] 把假 agent 推过 7 段。**前两段真的过通用层的机器**（`agents::visible_among`
//! 的发现判准 · `wire::Frame::Hello` 的序列化），**后五段没有** ——
//! 它们是假 agent 拿自己的知识**自问自答**，因为通用层今天**没有任何接口**收第二种布局。
//! 这不是本件偷懒，这**就是本件要报的那个差距**：
//!
//! - 前两段 = 今天已经成立的那部分成功标准②；
//! - 后五段 = 还差的 27 处；`walk` 走得通只证明「**这 12 种能力足以走完全流程**」
//!   （`L2` 那组接口的形状够不够用），**不证明通用层能用它们**。

use std::path::{Path, PathBuf};

/// 本假 agent 在 wire 上的 `agent_kind` 值。
///
/// ⚠ 它**永远不会**出现在真 `hello.homes` 里 —— 本层不进 `agents::REGISTRY`
/// （`the_fixture_agent_never_ships` 钉住）。
pub(crate) const AGENT_KIND: &str = "fake";

/// 解析本机 home 的环境变量。⚠ **刻意没有默认值**（Claude 那家恒有值）——
/// 少了这个差别，`visible_among` 里 `None` 那条分支就永远不会被这家走一遍。
const HOME_ENV: &str = "CCM_FAKE_AGENT_HOME";

/// 能力 8：解析本机 home。`None` = 连候选路径都说不出。
pub(crate) fn home() -> Option<PathBuf> {
    std::env::var_os(HOME_ENV).map(PathBuf::from)
}

/// 能力 1：会话记录根。**`convos` 不是 `projects`**。
pub(crate) fn records_root(home: &Path) -> PathBuf {
    home.join("convos")
}

/// 能力 2：这个路径是不是一份会话记录。**`.ndjson` 不是 `.jsonl`**。
pub(crate) fn is_session_file(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "ndjson")
        && p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("sess-"))
}

/// 能力 3：会话 id → 记录文件名。**带前缀**，与 Claude 的裸 `<sid>.<ext>` 不同形。
pub(crate) fn session_file_name(sid: &str) -> String {
    format!("sess-{sid}.ndjson")
}

/// 能力 4：pidfile 目录。**`live` 不是 `sessions`**。
pub(crate) fn pidfile_root(home: &Path) -> PathBuf {
    home.join("live")
}

/// 能力 5：账号（配置根）环境变量名。
pub(crate) const ACCOUNT_DIR_ENV: &str = "CCM_FAKE_ACCOUNT_DIR";

/// 能力 6：某个配置根下、对某个 cwd 的信任判定。
///
/// 形状与 Claude 那家**不同**：文件名不同、字段路径不同（`trusted[cwd]` 而不是
/// `projects[cwd].hasTrustDialogAccepted`）。
pub(crate) fn trust_of_config(root: &Path, cwd: &str) -> Result<bool, String> {
    let p = root.join("fake-config.json");
    let bytes = std::fs::read(&p).map_err(|e| format!("fake_config_unreadable: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("fake_config_invalid: {e}"))?;
    Ok(v.get("trusted")
        .and_then(|t| t.get(cwd))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false))
}

/// 能力 7：判活时"这个 cmdline 看起来像不像本 agent"。
///
/// ⚠ 与 Claude 那家**方向相同但词表不同**（那边是「明显不像才判冒名」，含 `claude`/`node`）。
pub(crate) fn cmdline_may_be_agent(lower: &str) -> bool {
    lower.trim().is_empty() || lower.contains("fakeagent")
}

/// 能力 9：用量聚合 —— 扫会话记录根，每有 token 的会话出一行。
///
/// 刻意只做**最小**的一件事（把两个 token 字段加起来），因为本件要证的是
/// 「通用层有没有地方收这个能力」，不是「聚合算得对不对」。
pub(crate) fn usage_rows(home: &Path) -> Vec<String> {
    let root = records_root(home);
    let mut out = Vec::new();
    let Ok(projects) = std::fs::read_dir(&root) else {
        return out;
    };
    let mut dirs: Vec<PathBuf> = projects.flatten().map(|e| e.path()).collect();
    dirs.sort();
    for dir in dirs {
        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = files.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            if !p.is_file() || !is_session_file(&p) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let mut total = 0i64;
            for line in text.lines() {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                if let Some(u) = v.pointer("/usage") {
                    total += u.get("in").and_then(serde_json::Value::as_i64).unwrap_or(0);
                    total += u.get("out").and_then(serde_json::Value::as_i64).unwrap_or(0);
                }
            }
            if total > 0 {
                let sid = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|n| n.strip_prefix("sess-"))
                    .and_then(|n| n.strip_suffix(".ndjson"))
                    .unwrap_or_default();
                out.push(format!(
                    "{}",
                    serde_json::json!({"sessionId": sid, "agentKind": AGENT_KIND, "tokens": total})
                ));
            }
        }
    }
    out
}

/// 能力 10：无 `launchCandidate` 时的默认命令基底。
pub(crate) const DEFAULT_COMMAND: &str = "fakeagent";

/// 能力 11：resume 命令 —— **子命令形，且动词与 Codex 也不同**。
pub(crate) fn resume_command(base: &str, session_id: &str) -> String {
    format!("{base} revive {session_id}")
}

/// 能力 12：resume 会话名前缀（Claude `cc` / Codex `cx`）。
pub(crate) const SESSION_NAME_PREFIX: &str = "fk";

// ─────────────────────────────────────────────────────────────────────────────
// 能力表 + 走全流程的 driver
// ─────────────────────────────────────────────────────────────────────────────

/// 12 种能力的**名字**，与 `agent_locality_guard::tests::NEW_AGENT_BLOCKERS` 逐条对账。
///
/// ⚠ 顺序即 [`FakeCaps`] 字段序，[`FakeCaps::without`] 按名字挖洞时靠它。
pub(crate) const CAPABILITIES: &[&str] = &[
    "会话记录根",
    "会话文件判定",
    "会话文件命名",
    "pidfile 目录",
    "账号环境变量名",
    "账号信任判定",
    "判活 cmdline",
    "解析本机 home",
    "用量聚合",
    "resume 默认命令",
    "resume 命令形",
    "resume 会话名前缀",
];

/// 假 agent 交出来的一整套能力。
///
/// ⚠ **每一项都是 `Option`，这是本结构存在的全部理由**：`S6` 的反向夹具要「把某一种能力
/// **删掉**」，而删掉之后流程必须在一个**说得出话**的地方停下来。用 `Option` 而不是真去
/// 注释掉一个函数，是为了让那个反向夹具成为**常驻判据**而不是一次性的手工变异
/// —— 手工变异证明的是"那天它会红"，常驻判据证明的是"以后它一直会红"。
#[derive(Clone)]
pub(crate) struct FakeCaps {
    pub(crate) records_root: Option<fn(&Path) -> PathBuf>,
    pub(crate) is_session_file: Option<fn(&Path) -> bool>,
    pub(crate) session_file_name: Option<fn(&str) -> String>,
    pub(crate) pidfile_root: Option<fn(&Path) -> PathBuf>,
    pub(crate) account_dir_env: Option<&'static str>,
    pub(crate) trust_of_config: Option<fn(&Path, &str) -> Result<bool, String>>,
    pub(crate) cmdline_may_be_agent: Option<fn(&str) -> bool>,
    pub(crate) home: Option<fn() -> Option<PathBuf>>,
    pub(crate) usage_rows: Option<fn(&Path) -> Vec<String>>,
    pub(crate) default_command: Option<&'static str>,
    pub(crate) resume_command: Option<fn(&str, &str) -> String>,
    pub(crate) session_name_prefix: Option<&'static str>,
}

impl FakeCaps {
    /// 还剩几种能力在（[`without`](Self::without) 的自检用）。
    pub(crate) fn present(&self) -> usize {
        [
            self.records_root.is_some(),
            self.is_session_file.is_some(),
            self.session_file_name.is_some(),
            self.pidfile_root.is_some(),
            self.account_dir_env.is_some(),
            self.trust_of_config.is_some(),
            self.cmdline_may_be_agent.is_some(),
            self.home.is_some(),
            self.usage_rows.is_some(),
            self.default_command.is_some(),
            self.resume_command.is_some(),
            self.session_name_prefix.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count()
    }

    /// 完整的一套（正题用）。
    pub(crate) fn full() -> Self {
        Self {
            records_root: Some(records_root),
            is_session_file: Some(is_session_file),
            session_file_name: Some(session_file_name),
            pidfile_root: Some(pidfile_root),
            account_dir_env: Some(ACCOUNT_DIR_ENV),
            trust_of_config: Some(trust_of_config),
            cmdline_may_be_agent: Some(cmdline_may_be_agent),
            home: Some(home),
            usage_rows: Some(usage_rows),
            default_command: Some(DEFAULT_COMMAND),
            resume_command: Some(resume_command),
            session_name_prefix: Some(SESSION_NAME_PREFIX),
        }
    }

    /// 挖掉**一种**能力（反向夹具用）。名字必须是 [`CAPABILITIES`] 里的一条 —— 写错就 panic，
    /// 不静默返回原样（静默的话反向夹具会**变成正题**再跑一遍，而它会绿）。
    pub(crate) fn without(&self, capability: &str) -> Self {
        assert!(
            CAPABILITIES.contains(&capability),
            "`{capability}` 不是一种登记过的能力 —— 反向夹具挖了个不存在的洞，\
             这一格此刻在把正题当反向跑（而正题是绿的）。已登记的：{CAPABILITIES:?}"
        );
        let mut c = self.clone();
        match capability {
            "会话记录根" => c.records_root = None,
            "会话文件判定" => c.is_session_file = None,
            "会话文件命名" => c.session_file_name = None,
            "pidfile 目录" => c.pidfile_root = None,
            "账号环境变量名" => c.account_dir_env = None,
            "账号信任判定" => c.trust_of_config = None,
            "判活 cmdline" => c.cmdline_may_be_agent = None,
            "解析本机 home" => c.home = None,
            "用量聚合" => c.usage_rows = None,
            "resume 默认命令" => c.default_command = None,
            "resume 命令形" => c.resume_command = None,
            "resume 会话名前缀" => c.session_name_prefix = None,
            other => unreachable!("`{other}` 在 CAPABILITIES 里却没有对应字段 —— 两处漂开了"),
        }
        c
    }
}

/// 全流程的 7 段。名字进错误信息，所以是常量而不是字面量。
pub(crate) const STAGES: &[&str] = &[
    "发现",
    "宣告（hello.homes）",
    "读会话",
    "判活",
    "账号",
    "用量",
    "resume",
];

/// 流程停下来的原因 —— **两种都必须说得出话**。
///
/// ⚠ 这个枚举本身就是 `S6` 的正题：件里逐字要求「不许静默当成"这个 agent 没有会话"」。
/// 而通用层今天对同一情形的反应恰恰是**静默**（`usage_query::run` 在一个布局不同的 home 上
/// rc=0、零输出），实测读数见 `PR-S6.md`。两者的差别就是 `L2` 那组接口要补上的东西。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    /// 假 agent 自己少了一种能力 —— 点名是**哪一段**缺了**哪一种**。
    MissingCapability {
        stage: &'static str,
        capability: &'static str,
    },
}

/// 把假 agent 推过全流程。成功 = 走完的段名。
///
/// # ⚠ 边界（本函数最容易被读错的一句，写在这里而不是只写在计划里）
///
/// 前两段（`发现` / `宣告`）**真的调用通用层的机器**：
/// [`crate::agents::visible_among`] 的判准与 [`crate::wire::Frame::Hello`] 的序列化。
/// 后五段（`读会话`/`判活`/`账号`/`用量`/`resume`）**没有过通用层** ——
/// 通用层今天在这五段上直呼 `crate::agents::claudecode::…`（27 处里的 22 处），
/// 没有任何入口收第二种布局。⇒ 这五段是假 agent 拿自己的知识自问自答。
///
/// 因此本函数走得通，**只证明这 12 种能力凑得出一条完整的路**（`D4`：接口面够不够用），
/// **不证明**通用层能用它们。后者今天不成立，差距逐条登记在
/// `agent_locality_guard::tests::NEW_AGENT_BLOCKERS`（27 处）。
pub(crate) fn walk(caps: &FakeCaps, fixture_home: &Path) -> Result<Vec<&'static str>, Stop> {
    let mut done: Vec<&'static str> = Vec::new();

    // ── ① 发现：**真的走通用层**（注册表 × 判准）。
    let home_fn = caps.home.ok_or(Stop::MissingCapability {
        stage: STAGES[0],
        capability: "解析本机 home",
    })?;
    // `Adapter.home` 是裸函数指针，所以"加一个 agent"在这一层真的只是多一条记录。
    let adapter = crate::agents::Adapter {
        kind: AGENT_KIND,
        home: home_fn,
    };
    let discovered = crate::agents::visible_among(std::slice::from_ref(&adapter));
    if discovered.len() != 1 {
        return Err(Stop::MissingCapability {
            stage: STAGES[0],
            capability: "解析本机 home",
        });
    }
    done.push(STAGES[0]);

    // ── ② 宣告：**真的走通用层**（塞进 `Hello.homes` 并序列化成一行）。
    let line = crate::wire::to_line(&crate::wire::Frame::Hello {
        v: 1,
        build_id: "s6".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/c".into(),
        homes: discovered.clone(),
        capabilities: vec![],
        emits: vec![],
        commands: vec![],
    })
    .map_err(|_| Stop::MissingCapability {
        stage: STAGES[1],
        capability: "解析本机 home",
    })?;
    if !line.contains(&format!("\"agent_kind\":\"{AGENT_KIND}\"")) {
        return Err(Stop::MissingCapability {
            stage: STAGES[1],
            capability: "解析本机 home",
        });
    }
    done.push(STAGES[1]);

    // ── ③ 读会话：⚠ 以下各段**没有过通用层**（见头注边界）。
    let records_root = caps.records_root.ok_or(Stop::MissingCapability {
        stage: STAGES[2],
        capability: "会话记录根",
    })?;
    let is_session = caps.is_session_file.ok_or(Stop::MissingCapability {
        stage: STAGES[2],
        capability: "会话文件判定",
    })?;
    let file_name = caps.session_file_name.ok_or(Stop::MissingCapability {
        stage: STAGES[2],
        capability: "会话文件命名",
    })?;
    let mut found: Vec<PathBuf> = Vec::new();
    if let Ok(projects) = std::fs::read_dir(records_root(fixture_home)) {
        let mut dirs: Vec<PathBuf> = projects.flatten().map(|e| e.path()).collect();
        dirs.sort();
        for d in dirs {
            if let Ok(files) = std::fs::read_dir(&d) {
                let mut ps: Vec<PathBuf> = files.flatten().map(|e| e.path()).collect();
                ps.sort();
                found.extend(ps.into_iter().filter(|p| p.is_file() && is_session(p)));
            }
        }
    }
    if found.is_empty() {
        return Err(Stop::MissingCapability {
            stage: STAGES[2],
            capability: "会话记录根",
        });
    }
    // 命名口径与判定口径必须互洽：拿 sid 算出来的名字得等于扫出来的那个文件名。
    let sid = FIXTURE_SESSION_ID;
    if !found
        .iter()
        .any(|p| p.file_name().and_then(|n| n.to_str()) == Some(file_name(sid).as_str()))
    {
        return Err(Stop::MissingCapability {
            stage: STAGES[2],
            capability: "会话文件命名",
        });
    }
    done.push(STAGES[2]);

    // ── ④ 判活：pidfile 目录 + cmdline 判定。
    let pid_root = caps.pidfile_root.ok_or(Stop::MissingCapability {
        stage: STAGES[3],
        capability: "pidfile 目录",
    })?;
    let cmdline = caps.cmdline_may_be_agent.ok_or(Stop::MissingCapability {
        stage: STAGES[3],
        capability: "判活 cmdline",
    })?;
    let pidfiles = std::fs::read_dir(pid_root(fixture_home))
        .map(|rd| rd.flatten().count())
        .unwrap_or(0);
    if pidfiles == 0 || !cmdline("fakeagent revive x") || cmdline("/usr/bin/vim") {
        return Err(Stop::MissingCapability {
            stage: STAGES[3],
            capability: "判活 cmdline",
        });
    }
    done.push(STAGES[3]);

    // ── ⑤ 账号：环境变量名 + 信任判定。
    let env_name = caps.account_dir_env.ok_or(Stop::MissingCapability {
        stage: STAGES[4],
        capability: "账号环境变量名",
    })?;
    let trust = caps.trust_of_config.ok_or(Stop::MissingCapability {
        stage: STAGES[4],
        capability: "账号信任判定",
    })?;
    if env_name.trim().is_empty() || trust(fixture_home, FIXTURE_CWD) != Ok(true) {
        return Err(Stop::MissingCapability {
            stage: STAGES[4],
            capability: "账号信任判定",
        });
    }
    done.push(STAGES[4]);

    // ── ⑥ 用量。
    let usage = caps.usage_rows.ok_or(Stop::MissingCapability {
        stage: STAGES[5],
        capability: "用量聚合",
    })?;
    if usage(fixture_home).is_empty() {
        return Err(Stop::MissingCapability {
            stage: STAGES[5],
            capability: "用量聚合",
        });
    }
    done.push(STAGES[5]);

    // ── ⑦ resume：默认命令 + 命令形 + 会话名前缀。
    let base = caps.default_command.ok_or(Stop::MissingCapability {
        stage: STAGES[6],
        capability: "resume 默认命令",
    })?;
    let cmd = caps.resume_command.ok_or(Stop::MissingCapability {
        stage: STAGES[6],
        capability: "resume 命令形",
    })?;
    let prefix = caps.session_name_prefix.ok_or(Stop::MissingCapability {
        stage: STAGES[6],
        capability: "resume 会话名前缀",
    })?;
    if base.trim().is_empty() || prefix.trim().is_empty() || !cmd(base, sid).contains(sid) {
        return Err(Stop::MissingCapability {
            stage: STAGES[6],
            capability: "resume 命令形",
        });
    }
    done.push(STAGES[6]);

    Ok(done)
}

/// 夹具里那条会话的 id。**只出现在夹具里**，不是任何真实会话。
pub(crate) const FIXTURE_SESSION_ID: &str = "00000000-0000-4000-8000-0000000000f6";
/// 夹具里那条会话的 cwd。
pub(crate) const FIXTURE_CWD: &str = "/home/u/proj";


#[cfg(test)]
mod tests {
    use super::*;

    /// 夹具根：按 pid + tag 隔开。
    ///
    /// ⚠ 建在临时目录里：用户 08-14 明令**不要动生产** ——
    /// 这份夹具与 `~/.cc-monitor/`、`~/.claude/` 一个字节都不相干。
    fn fixture_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ccm-s6-{tag}-{}", std::process::id()))
    }

    // ⚠ 各格自己的 home 指针：**裸 fn，不读环境变量**。
    //
    // 为什么不用 `CCM_FAKE_AGENT_HOME`：`std::env::set_var` 在多线程测试进程里与
    // 别的线程的 `getenv` 竞争（同进程里 `claudecode::home()` / `codex::home()` 正在读
    // `HOME`/`CLAUDE_CONFIG_DIR`/`CODEX_HOME`）。`Adapter.home` 本来就是**裸函数指针**
    // ——`S5` 选函数指针的一个副产物就是「假 agent 不需要任何运行时装配」。⇒ 直接给指针。
    fn home_of_announce() -> Option<PathBuf> {
        Some(fixture_root("announce"))
    }
    fn home_of_walk() -> Option<PathBuf> {
        Some(fixture_root("walk"))
    }
    fn home_of_hole() -> Option<PathBuf> {
        Some(fixture_root("hole"))
    }

    /// 建一份**假 agent 自己布局**的 home（`convos/` + `live/` + `fake-config.json`）。
    fn build_fixture(tag: &str) -> PathBuf {
        let root = fixture_root(tag);
        let _ = std::fs::remove_dir_all(&root);
        let proj = records_root(&root).join("-home-u-proj");
        std::fs::create_dir_all(&proj).expect("建会话记录根");
        std::fs::create_dir_all(pidfile_root(&root)).expect("建 pidfile 目录");
        std::fs::write(
            proj.join(session_file_name(FIXTURE_SESSION_ID)),
            "{\"role\":\"user\",\"text\":\"hi\"}\n\
             {\"role\":\"agent\",\"usage\":{\"in\":11,\"out\":22}}\n",
        )
        .expect("写会话记录");
        std::fs::write(
            pidfile_root(&root).join("424242.json"),
            "{\"pid\":424242,\"cwd\":\"/home/u/proj\"}\n",
        )
        .expect("写 pidfile");
        std::fs::write(
            root.join("fake-config.json"),
            format!("{{\"trusted\":{{\"{FIXTURE_CWD}\":true}}}}"),
        )
        .expect("写账号配置");
        root
    }

    fn caps_with(home_ptr: fn() -> Option<PathBuf>) -> FakeCaps {
        FakeCaps {
            home: Some(home_ptr),
            ..FakeCaps::full()
        }
    }

    /// `S6-Z1`：12 种能力**全部实现**，且**每一种都与两家真 agent 不同形**。
    ///
    /// # 为什么"不同形"是本件的地基，而不是趣味
    ///
    /// 假 agent 若照抄 Claude 的布局（`projects/` + `.jsonl`），通用层拿 Claude 的知识
    /// 去解释它的 home **恰好也能读出东西** —— 那时"走通全流程"证明的是
    /// 「Claude 的布局被施加到了另一个目录上」，不是「通用层能容纳第二种布局」。
    /// ⇒ 少了这一格，`S6` 的正题随时可能退化成一个**粉饰的通过**。
    #[test]
    fn every_fake_capability_differs_in_shape_from_both_real_agents() {
        // ⚠ 不叫 `home`：本模块的能力 8 就叫 `home()`，遮住它这一格就验不了它。
        let h = Path::new("/h");
        use crate::agents::{claudecode as cc, codex as cx};

        // 1 会话记录根 / 4 pidfile 目录：与 Claude 的两个根都不同，彼此也不同。
        assert_ne!(
            records_root(h),
            cc::paths::projects_root(h),
            "会话记录根与 Claude 同形"
        );
        assert_ne!(
            records_root(h),
            cc::paths::sessions_root(h),
            "会话记录根撞上了 Claude 的 pidfile 目录"
        );
        assert_ne!(
            pidfile_root(h),
            cc::paths::sessions_root(h),
            "pidfile 目录与 Claude 同形"
        );
        assert_ne!(pidfile_root(h), records_root(h), "本层自己两个根撞了");

        // 2 会话文件判定：两边**互相认不出对方的记录**（双向，否则只证明了一半）。
        let mine = Path::new("/h/convos/p").join(session_file_name(FIXTURE_SESSION_ID));
        let theirs =
            Path::new("/h/projects/p").join(cc::records::session_file_name(FIXTURE_SESSION_ID));
        assert!(is_session_file(&mine), "本层认不出自己的记录：{mine:?}");
        assert!(
            !cc::records::is_session_file(&mine),
            "Claude 的判定认出了本层的记录 —— 两种布局同形，`S6` 的正题会变成粉饰的通过：{mine:?}"
        );
        assert!(!is_session_file(&theirs), "本层认出了 Claude 的记录：{theirs:?}");
        assert!(cc::records::is_session_file(&theirs), "对照坏了：Claude 认不出自己的记录");

        // 3 会话文件命名
        assert_ne!(
            session_file_name(FIXTURE_SESSION_ID),
            cc::records::session_file_name(FIXTURE_SESSION_ID),
            "会话文件命名与 Claude 同形"
        );

        // 5 账号环境变量名
        assert_ne!(
            ACCOUNT_DIR_ENV,
            cc::paths::CONFIG_DIR_ENV,
            "账号环境变量名与 Claude 同名"
        );

        // 6 账号信任判定：文件名与字段路径都不同 —— 拿 Claude 的形状喂本层要判成"不信任"。
        let root = build_fixture("trust");
        assert_eq!(trust_of_config(&root, FIXTURE_CWD), Ok(true));
        assert_eq!(trust_of_config(&root, "/somewhere/else"), Ok(false));
        std::fs::write(
            root.join("fake-config.json"),
            format!("{{\"projects\":{{\"{FIXTURE_CWD}\":{{\"hasTrustDialogAccepted\":true}}}}}}"),
        )
        .expect("写 Claude 形状的配置");
        assert_eq!(
            trust_of_config(&root, FIXTURE_CWD),
            Ok(false),
            "本层认得懂 Claude 形状的信任字段 —— 那两种「账号知识」其实是同一种"
        );
        let _ = std::fs::remove_dir_all(&root);

        // 7 判活 cmdline：两边的**放行集不同**，否则"判活"这一格没有独立内容。
        assert!(cmdline_may_be_agent("fakeagent revive x"));
        assert!(
            !cmdline_may_be_agent("/usr/bin/node /x/claude"),
            "本层放行了 Claude 的 cmdline"
        );
        assert!(
            cc::liveness::cmdline_may_be_agent("/usr/bin/node /x/claude"),
            "对照坏了：Claude 那家不再认自己的 cmdline"
        );

        // 8 解析本机 home：Claude 那家**恒有值**（有默认），本层**没有默认**。
        // 这条差别让 `visible_among` 里 `None` 那条分支第一次由一家真实的适配层走过。
        assert!(cc::home().is_some(), "Claude 那家不再恒 Some 了 —— 本条的对照失效");
        if std::env::var_os(HOME_ENV).is_none() {
            // 正常状态：这个变量只有本夹具会用，机器上不会有人设它。
            assert!(home().is_none(), "本层在没有 {HOME_ENV} 时不该答得出 home");
        }

        // 10/11/12 resume 三件事：与 Claude **和** Codex 都不同 ——
        // 与 Codex 也不同才排除了"它只是第三个 codex"。
        assert_ne!(DEFAULT_COMMAND, cc::resume::DEFAULT_COMMAND);
        assert_ne!(DEFAULT_COMMAND, cx::resume::DEFAULT_COMMAND);
        assert_ne!(SESSION_NAME_PREFIX, cc::resume::SESSION_NAME_PREFIX);
        assert_ne!(SESSION_NAME_PREFIX, cx::resume::SESSION_NAME_PREFIX);
        assert_ne!(resume_command("b", "s"), cc::resume::resume_command("b", "s"));
        assert_ne!(
            resume_command("b", "s"),
            cx::resume::resume_command("b", "s"),
            "resume 命令形与 Codex 同形 —— 那样它就只是第三个 codex"
        );

        // 能力清单与 `FakeCaps` 的字段**不许漂开**：挖每一个洞都要真的少一种能力。
        let full = FakeCaps::full();
        assert_eq!(
            full.present(),
            CAPABILITIES.len(),
            "`FakeCaps::full()` 交出来的能力数与 `CAPABILITIES` 对不上 —— 两处漂开了"
        );
        for cap in CAPABILITIES {
            assert_eq!(
                full.without(cap).present(),
                CAPABILITIES.len() - 1,
                "挖「{cap}」这个洞没有挖到任何东西"
            );
        }
    }

    /// `S6-Z2`（正题上半）：一个**全新的 agent 被发现、进得了 `hello.homes`** ——
    /// 而通用层**一行没改**。
    ///
    /// ⚠ 这一段是**真的过通用层**：`agents::visible_among` 的判准与 `wire::Frame::Hello`
    /// 的序列化都是通用层的机器，本件没动它们一个字节。
    /// 它是 `G1` 成功标准②今天**唯一真正成立**的那一格。
    #[test]
    fn a_brand_new_agent_is_discovered_and_announced_with_zero_general_layer_change() {
        let root = build_fixture("announce");
        let adapter = crate::agents::Adapter {
            kind: AGENT_KIND,
            home: home_of_announce,
        };
        let discovered = crate::agents::visible_among(std::slice::from_ref(&adapter));
        assert_eq!(
            discovered.len(),
            1,
            "通用层的发现判准没有认出这家新 agent —— 而它的 home 目录就在那儿：{root:?}"
        );
        assert_eq!(discovered[0].agent_kind, AGENT_KIND);
        assert_eq!(discovered[0].path, root.to_string_lossy());

        let line = crate::wire::to_line(&crate::wire::Frame::Hello {
            v: 1,
            build_id: "s6".into(),
            host_arch: "x86_64".into(),
            claude_dir: "/c".into(),
            homes: discovered,
            capabilities: vec![],
            emits: vec![],
            commands: vec![],
        })
        .expect("填了第三家的 hello 必须序列化得出来");
        assert_eq!(
            line,
            format!(
                "{{\"kind\":\"hello\",\"v\":1,\"build_id\":\"s6\",\"host_arch\":\"x86_64\",\
                 \"claude_dir\":\"/c\",\"homes\":[{{\"agent_kind\":\"fake\",\"path\":{}}}]}}\n",
                serde_json::to_string(&root.to_string_lossy().into_owned()).unwrap()
            ),
            "第三个 agent 塞进 `Hello.homes` 之后字节形状不对 —— \
             `S4` 立的通用形状（agent 维度落在**值**里）没有真的容下第三家"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `S6-Z2`（正题下半）：12 种能力**凑得出一条完整的路**。
    ///
    /// `L2` 的钉法逐字：「`S6` 的最小假 agent —— 它只实现这组接口，
    /// **若接口漏了什么，`S6` 走不通就会红**」。本格就是那条钉法。
    ///
    /// ⚠ **边界写在明处**：后五段**没有过通用层**（见 [`walk`] 头注）。
    /// 本条证明的是接口面**够用**（`D4` 反推出来的 12 种能力不缺），
    /// **不是**通用层**能用**它们 —— 后者今天不成立，差距 27 处逐条登记在
    /// `agent_locality_guard::tests::NEW_AGENT_BLOCKERS`。
    #[test]
    fn the_twelve_reverse_derived_capabilities_are_enough_to_finish_the_pipeline() {
        let root = build_fixture("walk");
        let done = walk(&caps_with(home_of_walk), &root);
        assert_eq!(
            done.as_deref(),
            Ok(STAGES),
            "12 种能力走不完全流程 —— 那说明 `L2` 反推出来的接口面**漏了东西**"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `S6-Z4`（反向夹具）：**删掉任何一种能力，流程都在一个说得出话的地方停**。
    ///
    /// # 它守的是件里逐字禁掉的那种坏法
    ///
    /// 「不许静默当成"这个 agent 没有会话"」。本条对 12 种能力**逐一**挖洞，
    /// 每次都要求停下来的地方**点得出段名与能力名**，且点的是**挖掉的那一种**。
    ///
    /// ⚠ **两向都验**（铁律：只验"逮得住"会引进假阳）：
    /// ① 正向对照是上面那条 `…are_enough_to_finish_the_pipeline`（不挖洞就走得完）；
    /// ② 本格内部再验一次「停的位置**随挖的洞变**」—— 一个在第一段恒停的 driver
    ///    对每个洞都会"红"，那种红是假的。
    ///
    /// ⚠ 对照着看的是**通用层今天的反应**：同样是"这家的布局它不认识"，
    /// `usage_query::run` / `search_query::run` / `--session-accounts` 一律 **rc=0 + 零输出**
    /// （实测读数见 `PR-S6.md`）—— 那正是**静默**。两者的差就是 `L2` 要补的东西。
    #[test]
    fn removing_any_one_capability_stops_the_flow_somewhere_that_can_name_it() {
        let root = build_fixture("hole");
        let full = caps_with(home_of_hole);
        let mut stopped_at: Vec<(&str, &'static str, &'static str)> = Vec::new();
        for cap in CAPABILITIES {
            match walk(&full.without(cap), &root) {
                Ok(done) => panic!(
                    "挖掉能力「{cap}」之后流程仍然走完了 {done:?} —— \
                     少一种能力却一路绿灯，那正是件里禁掉的「静默当成这个 agent 没有会话」"
                ),
                Err(Stop::MissingCapability { stage, capability }) => {
                    assert!(
                        STAGES.contains(&stage),
                        "停在一个不认识的段名 `{stage}`（挖的是「{cap}」）"
                    );
                    assert!(
                        CAPABILITIES.contains(&capability),
                        "停下来时报的能力名 `{capability}` 不在登记表里（挖的是「{cap}」）"
                    );
                    stopped_at.push((cap, stage, capability));
                }
            }
        }
        // 停的位置必须**随挖的洞变**：全停在同一段 = driver 在第一段就恒停，
        // 那种"会红"对挖了哪个洞根本不敏感 —— 是假的。
        let distinct_stages: std::collections::BTreeSet<&str> =
            stopped_at.iter().map(|(_, s, _)| *s).collect();
        assert!(
            distinct_stages.len() >= 5,
            "12 个洞只停出了 {} 个不同的段：{stopped_at:?}\n\
             ⇒ driver 对「挖了哪个洞」不敏感，本格的「会红」是假的",
            distinct_stages.len()
        );
        // 点名点错了人，运维照着去修会修错地方 —— 说得出话但说错了话，比说不出话更坏。
        for (holed, stage, named) in &stopped_at {
            assert_eq!(
                holed, named,
                "挖的是「{holed}」，停下来却点名「{named}」（段 `{stage}`）"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `S6-Z3`（本件最要紧的一格）：**通用层今天怎么回答一个不是 Claude 的 agent** —— 实测。
    ///
    /// # 它逮的是件里逐字禁掉的那种坏法，而且逮到了
    ///
    /// 件里写着反向夹具「不许**静默**当成"这个 agent 没有会话"」。
    /// 本格把一个装好的、有会话有 pidfile 有账号配置的新 agent 的 home 直接喂给通用层的
    /// 四条一次性入口，读它们的实际反应 —— 实测三种，**只有一种算说得出话**：
    ///
    /// | 入口 | 今天的反应 |
    /// |---|---|
    /// | `history_query --list-projects` | rc=2 + 报错，但措辞是 **Claude 的布局**（`<home>/projects`） |
    /// | `search_query --search` | **rc=0、零输出** ⇒ 静默 |
    /// | `accounts_query --session-accounts` | **rc=0、零行** ⇒ 静默 |
    /// | `resolve_query`（`agentKind:"fake"`） | **rc=0，返回 `claude --resume <sid>`** ⇒ 静默**跑错命令** |
    ///
    /// 最后一条最坏：它不是「这个 agent 没有会话」，是「**当成 Claude 去跑**」。
    /// `resolve` 的分支写的是 `agent_kind == "codex"`，**其它一律落 Claude 路**——
    /// 对第三个 agent 来说那不是默认值，是**误路由**。
    ///
    /// ⚠ 本格**不是**在说这些入口有 bug —— 它们今天的契约就是「只服务一种 agent」。
    /// 它记的是：`G1` 成功标准②今天差的那 27 处，**每一处的失败长什么样**。
    /// 收接口那轮要补的错误出口，规格就在这里和 `NEW_AGENT_BLOCKERS` 那一列里。
    #[test]
    fn the_general_layer_answers_a_non_claude_agent_silently_or_with_claudes_words() {
        let root = build_fixture("probe");
        // 前提：这家的数据是**真的在那儿**，而 Claude 的布局在这个 home 下**不存在**。
        // 少了这两句，下面的"零输出"可能只是因为夹具是空的（台架空转）。
        assert!(
            records_root(&root).join("-home-u-proj").is_dir(),
            "夹具没建起来，本格在空转"
        );
        assert!(
            !crate::agents::claudecode::paths::projects_root(&root).exists(),
            "夹具里出现了 Claude 的 `projects/` —— 那样下面测的就不是「第二种布局」了"
        );

        // ① 会话读：说得出话，但说的是**别人的话**（措辞是 Claude 的目录布局）。
        let rc = crate::observe::history_query::run(&root, &["--list-projects".to_string()]);
        assert_eq!(
            rc, 2,
            "`--list-projects` 对一个布局不同的 agent 不再报错了 —— \
             那它就退化成了「静默零输出」，比现在更坏"
        );

        // ② 搜索：**静默**（rc=0）。
        let rc = crate::observe::search_query::run(
            &root,
            &["--search".to_string(), "hi".to_string()],
        );
        assert_eq!(
            rc, 0,
            "`--search` 的反应变了 —— 本格记的是「今天它 rc=0 零输出」这个事实，\
             变了就该同轮改 `NEW_AGENT_BLOCKERS` 的失败形态那一列"
        );

        // ③ 账号：**静默**（rc=0、零行）。
        // ⚠ `--accts-dir` 显式指到夹具里一个不存在的目录：不指的话它会去读**用户真实的**
        //   `~/.claude-accts`（用户 08-14 明令不碰生产）。
        let no_accts = root.join("no-such-accts");
        let rc = crate::observe::accounts_query::run(
            &root,
            &[
                "--session-accounts".to_string(),
                "--accts-dir".to_string(),
                no_accts.to_string_lossy().into_owned(),
            ],
        );
        assert_eq!(rc, 0, "`--session-accounts` 的反应变了");

        // ④ resume：**静默按 Claude 跑** —— 本件实测到的最坏一种。
        let spec = format!(
            "{{\"agentKind\":\"{AGENT_KIND}\",\"sessionId\":\"{FIXTURE_SESSION_ID}\"}}"
        );
        let plan = crate::control::resolve_query::resolve_json_for_inbound(&spec)
            .expect("`--resolve` 对未知 agentKind 今天不报错（这正是本格要记的）");
        assert_eq!(
            plan["command"],
            serde_json::json!(format!("claude --resume {FIXTURE_SESSION_ID}")),
            "\n`--resolve` 对 `agentKind:\"{AGENT_KIND}\"` 的返回变了。\n\
             本格记的事实是：**它不报错，它返回 Claude 的命令**（`agent_kind == \"codex\"` \
             之外一律落 Claude 路）。\n\
             ⚠ 这是 27 处里唯一一处**不是「读不出东西」而是「跑错东西」**的 —— \
             收接口那轮必须给它一个真正的错误出口（`unknown_agent_kind`），\n\
             而不是继续拿 Claude 当默认值。实得：{plan:?}"
        );
        assert_eq!(
            plan["sessionName"],
            serde_json::json!(format!("cc-{}", &FIXTURE_SESSION_ID[..8])),
            "会话名前缀也被**静默**给成了 Claude 的 `cc-`（这家自己的是 `{SESSION_NAME_PREFIX}-`）"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// `S6-Z5`：**夹具家永远不上生产** —— 判据两向。
    ///
    /// # 没有这一条，`agents/fake/` 就是一条捷径
    ///
    /// 本层进了 `agent_locality_guard::HOMES`（那是四条判据共用的**排除表**）——
    /// 也就是说它整层的格式知识都不被判据①扫。这是对的（它确实是一个 agent 家），
    /// 但代价必须付：它**不许**混进 `agents::REGISTRY`，也**不许**丢掉那行 `#[cfg(test)]`。
    /// 前者混进去 ⇒ 真 `hello.homes` 会声明一个根本不存在的 agent；
    /// 后者丢掉 ⇒ 生产二进制里凭空多出一个假 agent 的知识，而没有任何东西会说。
    #[test]
    fn the_fixture_agent_never_ships() {
        // ① 不在生产注册表里。
        let kinds: Vec<&str> = crate::agents::REGISTRY.iter().map(|a| a.kind).collect();
        assert!(
            !kinds.contains(&AGENT_KIND),
            "夹具 agent `{AGENT_KIND}` 混进了生产注册表 `agents::REGISTRY`：{kinds:?}\n\
             ⇒ 真填 `hello.homes` 那天，daemon 会向仓外消费方声明一个**不存在**的 agent。"
        );
        // ② 模块声明必须带 `#[cfg(test)]` —— 生产二进制里零字节。
        let mod_rs = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/agents/mod.rs"),
        )
        .expect("读 agents/mod.rs");
        let decl = format!("mod {AGENT_KIND};");
        let idx = mod_rs.find(&decl).unwrap_or_else(|| {
            panic!("`agents/mod.rs` 里找不到 `{decl}` —— 夹具家的模块声明改了形状，本条在空转")
        });
        let head = &mod_rs[..idx];
        let prev_line = head.lines().next_back().unwrap_or_default();
        let prev_prev = head.lines().nth_back(1).unwrap_or_default();
        assert!(
            prev_line.contains("cfg(test)") || prev_prev.contains("cfg(test)"),
            "`{decl}` 上面没有 `#[cfg(test)]` —— 夹具 agent 会被编进生产二进制。\n\
             上一行：{prev_line:?}\n上上行：{prev_prev:?}"
        );
    }
}
