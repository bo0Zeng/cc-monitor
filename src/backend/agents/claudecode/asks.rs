//! Claude Code 的答题口（注册表 `Adapter.asks` 那一格）：会话级 `PermissionRequest` hook 的输入怎么读、答案怎么写、起会话时怎么挂上；
//! 同一处多挂一轮的那几种 hook（提交了一句 · 一轮结束 · 闲着的通知），翻成统一事件（[`heard`]：会话在干什么先照它，pidfile 只兜底）。
//!
//! 实测（2.1.296，真机上交互会话里答批准、不送键）：交互会话里这个 hook 与终端框并行、谁先答算谁的；
//! 晚到的答案被丢、不会落到下一个框上；提问框要带 `updatedInput.answers` 才算答了；批计划要带原样 `updatedInput`，
//! 「批准并切档」经 `updatedPermissions` 的 `setMode`；子 agent 的请求多 `agent_id` · `agent_type`，后台那种是 hook 先跑完框才出。
//! exec 形必须写 `command`（只写 `args` 整份设置被跳过、开局还弹一个框）。
//!
//! 这一家的字段名只住这里；待答登记表、选项写字、谁先答算谁的住通用层（`control::asks`）。

use crate::agents::{
    AgentEvent, AskFace, AskIn, AskKind, AskQuestion, AskSuggestion, PlanMode, SuggestRule,
    SuggestScope, Verdict,
};
use serde_json::{json, Map, Value};

/// 注册表 `Adapter.asks` 那一格。
pub(crate) const FACE: AskFace = AskFace {
    launch_args,
    launch_flag: SETTINGS_FLAG,
    merge: Some(merge),
    gate: None,
    // 用户自己的 `--settings` 并进去（`merge`），盖不掉我们那一组：不诊断。
    overridden: None,
    read,
    decide,
    // 提问也经钩子答（`updatedInput.answers`）：哪一种都答得了。
    cannot: crate::agents::answers_every_kind,
    // 拒绝带 `interrupt` ⇒ 那一轮停下。
    stop_on_deny: true,
    heard,
    plugin: Some(PLUGIN),
    on_screen: Some(SCREEN),
};

/// 挡着终端的那一条（后台子 agent 的批准）在终端里的那个框：认法与键位照 2.1.322 在私有 tmux 里起的测试会话实测（宽 160 · 60 · 40 列 · 80×14 矮窗）。
pub(crate) const SCREEN: crate::agents::OnScreen = crate::agents::OnScreen {
    covers: screen_covers,
    seen: box_on_screen,
    queued: queued_on_screen,
    allow: crate::agents::Keystroke::Char("1"),
    deny: crate::agents::Keystroke::Key("esc"),
};

/// 框里那几行（实测 2.1.322）：标题以它起头 · 问句 · 底行以它起头 · 夹着命令的那两道虚线 · 框的上边框。
const BOX_TITLE: &str = "Bash command";
const BOX_ASK: &str = "Do you want to proceed?";
const BOX_FOOT: &str = "Esc to cancel";
const BOX_DASH: char = '╌';
const BOX_EDGE: char = '─';
const BOX_CURSOR: char = '❯';

/// 标题行右边的 `k of n`（排着的不止这一条时画）唯一的读法：字间空白并成一个的那一行 ⇒（去掉它之后的标题, `Some((k, n))`）；不带 ⇒（原样, `None`）。
fn split_count(flat: &str) -> (&str, Option<(u32, u32)>) {
    let mut tail = flat.rsplitn(4, ' ');
    if let (Some(n), Some("of"), Some(k), Some(rest)) =
        (tail.next(), tail.next(), tail.next(), tail.next())
    {
        if let (Ok(k), Ok(n)) = (k.parse::<u32>(), n.parse::<u32>()) {
            return (rest, Some((k, n)));
        }
    }
    (flat, None)
}

/// 屏上说还排着几条（连正显示的这一条）：框的标题行右边的 `k of n` ⇒ n − k + 1（n 连答过的也算）；没认出框 · 不画 ⇒ `None`。
fn queued_on_screen(lines: &[String]) -> Option<u32> {
    let (_, Some((k, n))) = split_count(&frame_of(lines)?.title) else {
        return None;
    };
    (k >= 1 && k <= n).then(|| n - k + 1)
}

/// 只认过 Bash 批准那一形（别的工具标题不同、排法没实测过）。
fn screen_covers(a: &AskIn) -> bool {
    a.kind == AskKind::Approve && a.tool == "Bash" && str_of(&a.input, "command").is_some()
}

/// 屏上那个批准框的几块（[`frame_of`] 找）：标题（字间空白并成一个、`k of n` 还在）· 两道虚线之间那几行（命令）。
struct Frame<'a> {
    title: String,
    body: &'a [String],
}

/// 找屏上那个批准框（自下而上，全部满足才算；认框 · 读排着几条都经它，只一处）：
/// 1. 一行是问句，下面紧跟编号选项（[`options_ok`]），再下面是底行；
/// 2. 问句上面的两道虚线夹着命令；
/// 3. 再往上到上边框之间头一行是标题（40 列时拆成两栏两行，拼起来认，[`title_of`]）。上边框看不见（矮窗，标题被顶出屏外）⇒ 找不到。
fn frame_of(lines: &[String]) -> Option<Frame<'_>> {
    let ask = lines.iter().rposition(|l| l.trim() == BOX_ASK)?;
    if !options_ok(&lines[ask + 1..]) {
        return None;
    }
    let rule = |l: &String, c: char| {
        let t = l.trim();
        !t.is_empty() && t.chars().all(|x| x == c)
    };
    let d2 = lines[..ask].iter().rposition(|l| rule(l, BOX_DASH))?;
    let d1 = lines[..d2].iter().rposition(|l| rule(l, BOX_DASH))?;
    let top = lines[..d1].iter().rposition(|l| rule(l, BOX_EDGE))?;
    Some(Frame {
        title: title_of(&lines[top + 1..d1])?,
        body: &lines[d1 + 1..d2],
    })
}

/// 屏上正显示的是这一条的框：找得到框（[`frame_of`]），两道虚线之间那几行逐行拼回这一条的命令
/// （[`folds_into`]：窄窗按词折 · 硬折都认，别处空白差一个都不认），标题去掉右边的 `k of n` 之后
/// 子 agent 的 ⇒ 正是 `Bash command · from the <agent_type> agent`，主会话的 ⇒ 只有 `Bash command`；子 agent 发的却没带种类 ⇒ 不认。
fn box_on_screen(lines: &[String], a: &AskIn) -> bool {
    let Some(cmd) = screen_covers(a)
        .then(|| str_of(&a.input, "command"))
        .flatten()
    else {
        return false;
    };
    let Some(frame) = frame_of(lines) else {
        return false;
    };
    let pieces: Vec<&str> = frame
        .body
        .iter()
        .map(|l| {
            let t = l.trim_start();
            t.strip_prefix('│').unwrap_or(t).trim()
        })
        .collect();
    if !folds_into(&pieces, cmd) {
        return false;
    }
    let want = match (a.agent_id.as_deref(), a.agent_type.as_deref()) {
        (_, Some(t)) => format!("{BOX_TITLE} · from the {t} agent"),
        (None, None) => BOX_TITLE.to_string(),
        // 子 agent 发的、没带种类：标题里写的是谁对不上，认不准。
        (Some(_), None) => return false,
    };
    split_count(&frame.title).0 == copy_core::collapse_ws(&want)
}

/// 问句下面那几项：编号连着从 1 起、头一项 `Yes`、末一项 `No`，之后是底行。哪一行是一项照缩进认：
/// 编号起头的那一列（光标 `❯` 那一行里编号所在的那一列）上起头的才是一项，更靠右的是上一项折下来的续行（续行开头恰好是「N. 」也不算）。
fn options_ok(below: &[String]) -> bool {
    let lead = |l: &str| l.chars().take_while(|c| c.is_whitespace()).count();
    let Some(col) = below.iter().find_map(|l| {
        let at = l.find(BOX_CURSOR)?;
        let after = &l[at + BOX_CURSOR.len_utf8()..];
        Some(l[..at].chars().count() + 1 + lead(after))
    }) else {
        return false;
    };
    let mut opts: Vec<(u32, &str)> = Vec::new();
    for l in below {
        let t = l.trim_start();
        if t.starts_with(BOX_FOOT) {
            return opts.first() == Some(&(1, "Yes"))
                && opts.last().is_some_and(|(_, x)| *x == "No")
                && opts.iter().zip(1..).all(|((n, _), i)| *n == i);
        }
        let body = match t.strip_prefix(BOX_CURSOR) {
            Some(rest) => rest.trim_start(),
            None if lead(l) == col => t,
            None => continue,
        };
        if let Some((n, text)) = body.split_once(". ") {
            if let Ok(n) = n.parse::<u32>() {
                opts.push((n, text.trim()));
            }
        }
    }
    false
}

/// 上边框与头一道虚线之间那几行 ⇒ 标题（字间空白并成一个，右边的 `k of n` 留着、由 [`split_count`] 读）；拆成两栏两行的拼回来。
fn title_of(head: &[String]) -> Option<String> {
    let first = head.first()?.trim();
    let joined = if first.starts_with(BOX_TITLE) {
        first.to_string()
    } else {
        let (w1, w2) = BOX_TITLE.split_once(' ')?;
        let left = first.strip_prefix(w1)?;
        let right = head.get(1)?.trim().strip_prefix(w2)?;
        format!("{BOX_TITLE} {left} {right}")
    };
    Some(copy_core::collapse_ws(&joined))
}

/// 屏上两道虚线之间那几行（每行去掉开头的空白与 `│`、去掉首尾空白）依次拼回命令原文：每一行得逐字接上，
/// 行与行之间只许原文本来有空白（按词折行丢了它）或本来就连着（硬折）；原文里别处的空白一个都不能多、不能少。
fn folds_into(pieces: &[&str], cmd: &str) -> bool {
    let mut rest = cmd.trim();
    for p in pieces.iter().filter(|p| !p.is_empty()) {
        let Some(r) = rest.strip_prefix(*p) else {
            return false;
        };
        rest = r.trim_start();
    }
    rest.is_empty()
}

/// 换了启动器那一支（`AskFace.plugin`）：同一组 hook 做成一个小插件，经 `CLAUDE_CODE_PLUGIN_DIRS` 交下去（2.1.280 起认；
/// 文档 env-vars：「each loaded the way a `--plugin-dir` flag loads it」，id 是 `<名>@inline`、只管这一个会话）。
const PLUGIN: crate::agents::PluginFace = crate::agents::PluginFace {
    env: PLUGIN_ENV,
    gate: PLUGIN_GATE,
    files: plugin_files,
    policy_files,
    policy_blocks,
    child_marker: CHILD_MARKER,
};

/// 认 [`PLUGIN_ENV`] 的那道门：最低版本 ＋ `PATH` 上的 `claude`、npm 包名（读法是通用的 `agents::installed_version`）。
pub(crate) const PLUGIN_GATE: crate::agents::HookGate = crate::agents::HookGate {
    min_version: [2, 1, 280],
    command: super::resume::DEFAULT_COMMAND,
    package: "@anthropic-ai/claude-code",
};

/// 插件目录清单那个变量。
pub const PLUGIN_ENV: &str = "CLAUDE_CODE_PLUGIN_DIRS";
/// 插件名（清单里那一格；用户在 `enabledPlugins` 里写 `"<名>@inline": false` 就不加载，那一家自己照办）。
pub(crate) const PLUGIN_NAME: &str = "cc-monitor-answer-hook";
/// claude 导给子进程的嵌套标记（`resume::NESTED_ENV` 的头一个；ccm 起会话前清掉它）。
const CHILD_MARKER: &str = super::resume::NESTED_ENV[0];

/// 只含 ccm 那几组 hook 的那一份（答题那一种 ＋ 一轮的那几种 [`TURN_HOOKS`]）：`--settings` 的 JSON 串与插件的 `hooks/hooks.json` 都是它（实现只一处）。
fn hooks_doc(entry: &str) -> Value {
    let mut hooks = Map::new();
    for (event, group) in groups(entry) {
        hooks.insert(event.to_string(), json!([group]));
    }
    json!({ "hooks": hooks })
}

/// 每一种 hook 挂的那一组：答题那一种等人答（[`HOOK_TIMEOUT_SECS`]）；一轮的那几种交了就走（[`EVENT_HOOK_TIMEOUT_SECS`]）。
fn groups(entry: &str) -> Vec<(&'static str, Value)> {
    let mut out = vec![(EVENT, hook_group(entry, HOOK_TIMEOUT_SECS, None))];
    out.extend(
        TURN_HOOKS
            .iter()
            .map(|(e, m)| (*e, hook_group(entry, EVENT_HOOK_TIMEOUT_SECS, *m))),
    );
    out
}

/// 一轮的那几种 hook（＋ matcher）：用户提交了一句 · 一轮结束 · 闲着的通知（Esc 打断不发 Stop，闲了一会儿那一家发这一种）。
const TURN_HOOKS: [(&str, Option<&str>); 3] = [
    (PROMPT_EVENT, None),
    (STOP_EVENT, None),
    (NOTE_EVENT, Some(IDLE_NOTE)),
];
const PROMPT_EVENT: &str = "UserPromptSubmit";
const STOP_EVENT: &str = "Stop";
const NOTE_EVENT: &str = "Notification";
/// 闲着的那一种通知（`notification_type`）。
const IDLE_NOTE: &str = "idle_prompt";
/// 一轮的那几种 hook 的期限（秒）：交给常驻就走；常驻卡住也不拖住那一家（`UserPromptSubmit` · `Stop` 等它跑完）。
pub const EVENT_HOOK_TIMEOUT_SECS: u64 = 10;

/// hook 的 stdin ⇒（会话 id, 统一事件）（注册表 `AskFace.heard`）。要人答的那一种 · 别的通知 · 缺会话 id ⇒ `None`。
pub fn heard(v: &Value) -> Option<(String, AgentEvent)> {
    let event = match v.get("hook_event_name").and_then(Value::as_str)? {
        PROMPT_EVENT => AgentEvent::PromptSubmitted,
        STOP_EVENT => AgentEvent::TurnEnded,
        NOTE_EVENT if v.get("notification_type").and_then(Value::as_str) == Some(IDLE_NOTE) => {
            AgentEvent::TurnEnded
        }
        _ => return None,
    };
    Some((nonempty(v.get("session_id"))?, event))
}

/// 插件目录里的两份：清单 ＋ hook。
pub(crate) fn plugin_files(entry: &str) -> Vec<(&'static str, String)> {
    let manifest = json!({
        "name": PLUGIN_NAME,
        "description": "cc-monitor: answer permission prompts from the message stream",
    });
    vec![
        (".claude-plugin/plugin.json", manifest.to_string()),
        ("hooks/hooks.json", hooks_doc(entry).to_string()),
    ]
}

/// 这台的 managed 策略目录（docs/en/settings 的落点表）。
#[cfg(target_os = "linux")]
const POLICY_DIR: Option<&str> = Some("/etc/claude-code");
#[cfg(target_os = "macos")]
const POLICY_DIR: Option<&str> = Some("/Library/Application Support/ClaudeCode");
#[cfg(windows)]
const POLICY_DIR: Option<&str> = Some("C:\\Program Files\\ClaudeCode");
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
const POLICY_DIR: Option<&str> = None;

/// 这台的 managed 策略文件（`managed-settings.d/` 里的各份一起读）。
pub(crate) fn policy_files() -> Vec<std::path::PathBuf> {
    let Some(base) = POLICY_DIR.map(std::path::Path::new) else {
        return Vec::new();
    };
    let mut out = vec![base.join("managed-settings.json")];
    if let Ok(rd) = crate::platform::fs::Ro::read_dir(base.join("managed-settings.d")) {
        let mut more: Vec<_> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();
        more.sort();
        out.extend(more);
    }
    out
}

/// 一份 managed 策略不许这一路：`disableSideloadFlags`（设了这个变量当场 exitWithError）· `allowManagedHooksOnly`（插件里的 hook 被拦）。
pub(crate) fn policy_blocks(v: &Value) -> bool {
    ["disableSideloadFlags", "allowManagedHooksOnly"]
        .iter()
        .any(|k| v.get(k).and_then(Value::as_bool) == Some(true))
}

/// 会话级设置那个旗标（住启动形状那一处，`resume.rs`）。
use super::resume::SETTINGS_FLAG;
/// 答题那一种 hook。
const EVENT: &str = "PermissionRequest";
/// hook 的期限（秒）：到点它被杀、不给决定 ⇒ 框照常在终端里（挡着终端的那种到点才出框）。
pub const HOOK_TIMEOUT_SECS: u64 = 3600;
/// hook 那一趟叫 ccm 的哪几个词（后端子命令 ＋ 这一家的 kind）。
pub(crate) fn hook_words() -> [&'static str; 3] {
    let [end, flag] = crate::agents::asks::ANSWER_HOOK_WORDS;
    [end, flag, super::AGENT_KIND]
}

/// ccm 那一组 hook（exec 形，不经 shell；`entry` ＝ ccm 绝对路径；几种都叫同一个入口，入口照 `hook_event_name` 分）。
fn hook_group(entry: &str, timeout: u64, matcher: Option<&str>) -> Value {
    let mut g = json!({ "hooks": [{
        "type": "command",
        "command": entry,
        "args": hook_words(),
        "timeout": timeout,
    }] });
    if let Some(m) = matcher {
        g["matcher"] = json!(m);
    }
    g
}

/// 起会话时垫的那两个词：`--settings` ＋ 只含 ccm 那几组 hook 的 JSON 串。
pub fn launch_args(entry: &str) -> Vec<String> {
    vec![SETTINGS_FLAG.to_string(), hooks_doc(entry).to_string()]
}

/// 用户自己的会话级设置并进 ccm 那几组 hook：别的一切原样，只往每一种的列表末尾加我们那一组（hooks 本来就是各层合并、不替换）。
/// 不是对象 · `hooks` 不是对象 · 答题那一种不是数组 ⇒ `None`（不挂）；一轮的哪一种不是数组 ⇒ 只那一种不挂（少的是一档，答题照挂）。
pub(crate) fn merge(user: &Value, entry: &str) -> Option<Value> {
    let mut v = user.as_object()?.clone();
    let hooks = v
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()?;
    for (event, group) in groups(entry) {
        match hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
        {
            Some(list) => list.push(group),
            None if event == EVENT => return None,
            None => {}
        }
    }
    Some(Value::Object(v))
}

pub(crate) const ASK_TOOL: &str = "AskUserQuestion";
pub(crate) const PLAN_TOOL: &str = "ExitPlanMode";
pub(crate) const INTERACTIVE_TOOLS: &[&str] = &[ASK_TOOL, PLAN_TOOL];

/// hook 的 stdin ⇒ 通用待答请求。不是 `PermissionRequest` · 缺会话 id / 工具名 ⇒ `None`。
pub fn read(v: &Value) -> Option<AskIn> {
    if v.get("hook_event_name").and_then(Value::as_str) != Some(EVENT) {
        return None;
    }
    let sid = nonempty(v.get("session_id"))?;
    let tool = nonempty(v.get("tool_name"))?;
    let input = v.get("tool_input").cloned().unwrap_or(Value::Null);
    let kind = match tool.as_str() {
        ASK_TOOL => AskKind::Answer,
        PLAN_TOOL => AskKind::Plan,
        _ => AskKind::Approve,
    };
    let questions = if kind == AskKind::Answer {
        questions_of(&input)
    } else {
        Vec::new()
    };
    let suggestions = v
        .get("permission_suggestions")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(suggestion_of).collect())
        .unwrap_or_default();
    let agent_id = nonempty(v.get("agent_id"));
    Some(AskIn {
        sid,
        kind,
        what: what_of(&tool, &input, &questions),
        detail: detail_of(&tool, &input, &questions),
        tool,
        suggestions,
        questions,
        // 子 agent 的请求：这一版里我们起的会话子 agent 全在后台（fork 开关默认开着，Agent 一律后台），
        // 后台那种是 hook 先跑完框才出 ⇒ 挡着终端。hook 输入分不出前后台，带 `agent_id` 的一律按挡着算。
        blocks_terminal: agent_id.is_some(),
        agent_id,
        agent_type: nonempty(v.get("agent_type")),
        input,
    })
}

fn nonempty(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn questions_of(input: &Value) -> Vec<AskQuestion> {
    // 与过程里那张提问卡同一处读（形状不对 ⇒ 没有题，答不了，通用层只给「拒绝」）。
    match super::steps::ask_of(ASK_TOOL, input) {
        Some(crate::agents::StepAsk::Questions { questions }) => questions,
        _ => Vec::new(),
    }
}

fn str_of<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}

/// 钉条里那一段：命令里说得出用意的那一段（[`super::steps::main_text`]）的头一行 · 文件路径 · 第一道题；说不出 ⇒ `None`。
fn what_of(tool: &str, input: &Value, questions: &[AskQuestion]) -> Option<String> {
    let first_line = |s: &str| s.lines().next().map(str::trim).map(str::to_string);
    match tool {
        ASK_TOOL => questions.first().map(|q| q.question.clone()),
        PLAN_TOOL => None,
        _ => super::steps::main_arg(tool)
            .map(|(k, _)| k)
            .into_iter()
            .chain([
                "command",
                "file_path",
                "notebook_path",
                "url",
                "pattern",
                "path",
            ])
            .find_map(|k| str_of(input, k))
            .and_then(|s| first_line(&super::steps::main_text(tool, s)))
            .filter(|s| !s.is_empty()),
    }
}

/// 要批的东西的全文（不截）：命令全文 · 写哪个文件写成什么 · 改哪里改成什么 · 计划原文；认不出的工具 ⇒ 参数原样排开。
fn detail_of(tool: &str, input: &Value, questions: &[AskQuestion]) -> String {
    let path = str_of(input, "file_path").or_else(|| str_of(input, "notebook_path"));
    match tool {
        PLAN_TOOL => str_of(input, "plan").unwrap_or_default().to_string(),
        ASK_TOOL => questions
            .iter()
            .map(|q| q.question.clone())
            .collect::<Vec<_>>()
            .join("\n"),
        "Bash" if str_of(input, "command").is_some() => {
            str_of(input, "command").unwrap_or_default().to_string()
        }
        "Write" if path.is_some() => format!(
            "{}\n\n{}",
            path.unwrap_or_default(),
            str_of(input, "content").unwrap_or_default()
        ),
        "Edit" if path.is_some() => format!(
            "{}\n\n- {}\n+ {}",
            path.unwrap_or_default(),
            str_of(input, "old_string").unwrap_or_default(),
            str_of(input, "new_string").unwrap_or_default()
        ),
        _ => serde_json::to_string_pretty(input).unwrap_or_default(),
    }
}

/// 一条「以后不再问」建议认成码 ＋ 原样（写成人话在核心 `AskSuggestion::said`）；不是放宽的那几种（删规则 · 删目录）⇒ 不给。
fn suggestion_of(s: &Value) -> Option<AskSuggestion> {
    let scope = match s.get("destination").and_then(Value::as_str) {
        Some("session") => SuggestScope::Session,
        Some("localSettings") => SuggestScope::Local,
        Some("projectSettings") => SuggestScope::Project,
        Some("userSettings") => SuggestScope::User,
        _ => return None,
    };
    let rule = match s.get("type").and_then(Value::as_str)? {
        "addDirectories" => {
            let dirs: Vec<String> = s
                .get("directories")
                .and_then(Value::as_array)?
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            if dirs.is_empty() {
                return None;
            }
            SuggestRule::Dirs(dirs)
        }
        "setMode" => match s.get("mode").and_then(Value::as_str)? {
            "acceptEdits" => SuggestRule::AcceptEdits,
            m => SuggestRule::Mode(m.to_string()),
        },
        "addRules" | "replaceRules"
            if s.get("behavior").and_then(Value::as_str) == Some("allow") =>
        {
            let rules: Vec<String> = s
                .get("rules")
                .and_then(Value::as_array)?
                .iter()
                .filter_map(|r| {
                    let t = r.get("toolName").and_then(Value::as_str)?;
                    Some(match r.get("ruleContent").and_then(Value::as_str) {
                        Some(c) if !c.is_empty() => format!("{t}({c})"),
                        _ => t.to_string(),
                    })
                })
                .collect();
            if rules.is_empty() {
                return None;
            }
            SuggestRule::Allow(rules)
        }
        _ => return None,
    };
    Some(AskSuggestion {
        scope,
        rule,
        raw: s.clone(),
    })
}

/// 人的答案 ⇒ hook 打到 stdout 的那一整份。
pub(crate) fn decide(ask: &AskIn, verdict: &Verdict) -> Option<Value> {
    let mut d = Map::new();
    match verdict {
        Verdict::Allow {
            rule,
            mode,
            answers,
        } => {
            d.insert("behavior".into(), json!("allow"));
            match ask.kind {
                // 提问：原样题目 ＋ 每题的答案（多选已由通用层用「, 」连好）；不带 `updatedInput` 那一家当没答。
                AskKind::Answer => {
                    let mut input = ask.input.as_object().cloned().unwrap_or_default();
                    let a: Map<String, Value> =
                        answers.iter().map(|(q, v)| (q.clone(), json!(v))).collect();
                    input.insert("answers".into(), Value::Object(a));
                    d.insert("updatedInput".into(), Value::Object(input));
                }
                // 批计划：原样计划（同样不带就当没答）＋ 切到哪一档。
                AskKind::Plan => {
                    d.insert("updatedInput".into(), ask.input.clone());
                    let m = match mode.unwrap_or(PlanMode::Manual) {
                        PlanMode::Auto => "auto",
                        PlanMode::AcceptEdits => "acceptEdits",
                        PlanMode::Manual => "default",
                    };
                    d.insert(
                        "updatedPermissions".into(),
                        json!([{ "type": "setMode", "mode": m, "destination": "session" }]),
                    );
                }
                AskKind::Approve => {
                    if let Some(s) = rule.and_then(|i| ask.suggestions.get(i)) {
                        d.insert("updatedPermissions".into(), json!([s.raw]));
                    }
                }
            }
        }
        Verdict::Deny { note, stop } => {
            d.insert("behavior".into(), json!("deny"));
            if let Some(n) = note.as_deref().filter(|n| !n.trim().is_empty()) {
                d.insert("message".into(), json!(n));
            }
            if *stop {
                d.insert("interrupt".into(), json!(true));
            }
        }
    }
    Some(json!({ "hookSpecificOutput": { "hookEventName": EVENT, "decision": Value::Object(d) } }))
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/asks_tests.rs"]
mod tests;
