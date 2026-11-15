//! cc-bus 钩子诊断 —— 帧命令 `hooks-diag` 的本体（只读）。
//!
//! 足迹面声明了收信钩子的每一家各判一份：这台后端读那一家家目录下的钩子文件（足迹面 `user_settings` 头一份）、就地 stat 钩子点名的程序，
//! 出那一家的诊断 ＋ 要加的内容 ＋ 读的是哪份文件；那一家声明了信任（足迹面 `hook_trust`）⇒ 再判那一条的信任对没对上，
//! 没对上给出要并进信任那份的那一段（键与值由那一家的算法算好）。要加的内容只有一形：一条 Stop 钩子直接指向这台装好的 cc-bus 的入口、带动词 `stop-hook`
//! （脚本靠 `readlink -f` 认自己的目录；不依赖 `PATH`）；cc-bus 没装 ⇒ 不给。本机远端同一条命令（`chan.call(origin, "hooks-diag")`），
//! 帧面宿主在顶层 `feature_face`。
//!
//! 判据不是字符串等值：`"$HOME/.local/bin/cc-bus" stop-hook` 与规范片段 `cc-bus stop-hook` 功能等价、字符串不等
//! ⇒ 按「被执行的程序 ＋ 它的第一个参数」判。看不懂的形态答「无法判断」，不猜「未装」。
//! 老版那条 `cc-bus-stop-hook`（独立的脚本）不算：那个程序不在了（老名字不留）。
//! 只读：钩子那份与信任那份都不写（用户的文件，由用户自己合并），只生成要加的内容。

use crate::platform::fs::Ro;
use crate::platform::shell::posix;
use cap_core::{Cap, Over};
use copy_core::copy_text;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// 钩子命令里那个程序（`Stop` → `cc-bus stop-hook`：唯一的入口 ＋ 动词）。会话起来时不用登记：会话 id 由 cc-monitor 认。
pub const PROGRAM: &str = "cc-bus";
/// 钩子命令里程序后面紧跟的那个词。
pub(crate) const VERB: &str = "stop-hook";

/// 读 `settings.json` 的上限（它是几 KB 的配置；超了按「读不到」降级，诊断里说出来）。
const SETTINGS_CAP_BYTES: Cap = Cap {
    bytes: 1 << 20,
    over: Over::DegradeAndSay,
};

/// 一个钩子的诊断结论。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum HookState {
    /// 没有任何一条 command 调到这个程序。
    NotInstalled,
    /// 裸命令形态（走 PATH）。
    InstalledViaPath { command: String },
    /// 显式路径且**该路径存在**（用户当前的实际状态，不是问题）。
    InstalledAtPath { command: String, path: String },
    /// 显式路径但**该路径不存在** —— 看着像装了，其实指不到东西。
    PathMissing { command: String, path: String },
    /// 无法判断：命令里出现了目标程序名，但它不是被直接执行的那个（`sh -c` / `env` / `timeout` 包着）。猜「未装」和猜「已装」一样是猜。
    Unknown { command: String },
}

impl HookState {
    /// 只有这两种算「能用」；`PathMissing` 刻意不算。
    pub fn is_working(&self) -> bool {
        matches!(
            self,
            HookState::InstalledViaPath { .. } | HookState::InstalledAtPath { .. }
        )
    }
}

/// 那一态的色调：能用 ⇒ `ok`；未装 · 指不到 ⇒ `bad`；看不懂 ⇒ `unknown`（中性）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "lowercase")]
pub enum HookTone {
    Ok,
    Bad,
    Unknown,
}

/// 整份诊断。`said` · `tone` 是停下钩子那一态写好的那一句与色调（界面照抄）；`note` 装「为什么没读到」（文件缺失 / 坏 JSON），不为空即应展示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct HooksDiagnosis {
    pub stop: HookState,
    pub(crate) said: crate::common::cells::Words,
    pub(crate) tone: HookTone,
    pub(crate) note: String,
}

impl HooksDiagnosis {
    /// 一态 ⇒ 带上那一句与色调。**`path-missing` 绝不能说成「已装」**；`unknown` 要中性（既不说已装也不说未装）。
    fn of(stop: HookState, note: String) -> Self {
        let (said, tone) = match &stop {
            HookState::NotInstalled => (copy_text("reason.spawn.notInstalled", &[]), HookTone::Bad),
            HookState::InstalledViaPath { .. } => {
                (copy_text("ccBusHooks.state.onPath", &[]), HookTone::Ok)
            }
            HookState::InstalledAtPath { path, .. } => (
                copy_text("ccBusHooks.state.explicit", &[("path", path)]),
                HookTone::Ok,
            ),
            HookState::PathMissing { path, .. } => (
                copy_text("ccBusHooks.state.brokenPath", &[("path", path)]),
                HookTone::Bad,
            ),
            HookState::Unknown { command } => (
                copy_text("ccBusHooks.state.indirect", &[("command", command)]),
                HookTone::Unknown,
            ),
        };
        HooksDiagnosis {
            stop,
            said: crate::common::cells::Words(said),
            tone,
            note,
        }
    }
}

/// 成品（形状与界面 `HooksReport` 逐字同；跨语言金样 `tests/__fixtures__/hooks-diag.golden.json`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct HooksReport {
    /// 这台跑得了 cc-bus（它要 tmux）；跑不了 ⇒ 不给要加的内容，界面只说这台不支持自动收信。
    pub supported: bool,
    /// 足迹面声明了收信钩子的每一家各一份（照注册表次序）。
    pub families: Vec<FamilyHooks>,
}

/// 一家的那一份：钩子装没装 · 要加的钩子那一段 · 读的哪份 · 声明了信任的再带信任那一格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct FamilyHooks {
    /// 那一家（注册表 `kind`）。
    pub kind: String,
    /// 那一家给人看的名字。
    pub name: String,
    pub diagnosis: HooksDiagnosis,
    /// 要合并进那份钩子文件的内容；这台的 cc-bus 没装（入口不在）· 跑不了 ⇒ `None`。
    pub snippet: Option<String>,
    /// 读的是哪份钩子文件（这台后端看到的路径）。
    pub source: String,
    /// 那一家声明了信任（足迹面 `hook_trust`）且有一条要判的命令 ⇒ 那一条的信任；不声明 ⇒ `None`。
    pub trust: Option<HookTrustDiagnosis>,
}

/// 一条收信钩子的信任：记着的值与照那一家声明算好的值对没对上。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct HookTrustDiagnosis {
    pub trusted: bool,
    pub(crate) said: crate::common::cells::Words,
    pub(crate) tone: HookTone,
    /// 信任记在哪份文件（这台后端看到的路径）。
    pub file: String,
    /// 没对上 ⇒ 要并进那份文件的那一段（键与值照那一家的算法算好）；对上 ⇒ `None`。
    pub line: Option<String>,
    /// 那一条的键 · 照声明算好的值（待办照它们经那一家的 `merge` 并进那份文件；不上线）。
    #[serde(skip)]
    #[cfg_attr(any(test, feature = "ts"), ts(skip))]
    pub key: String,
    #[serde(skip)]
    #[cfg_attr(any(test, feature = "ts"), ts(skip))]
    pub value: String,
}

/// 剥掉**配对的**一层包裹引号；不配对原样返回（不替用户猜本意）。
fn unquote_once(tok: &str) -> &str {
    let b = tok.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        &tok[1..tok.len() - 1]
    } else {
        tok
    }
}

/// 一条 shell 命令里**被执行的程序**：剥前导 `VAR=x` → 第一个词 → 剥一层配对引号 → (basename, 原词)。
/// 看不懂 ⇒ `None`（不猜）。
pub(crate) fn program_of(cmd: &str) -> Option<(String, String)> {
    let mut rest = cmd.trim();
    loop {
        let tok = rest.split_whitespace().next()?;
        if !shell_quote_core::is_assignment(tok) {
            break;
        }
        rest = rest[tok.len()..].trim_start();
    }
    let unq = unquote_once(rest.split_whitespace().next()?);
    if unq.is_empty() {
        return None;
    }
    let base = unq.rsplit('/').next().unwrap_or(unq);
    if base.is_empty() {
        return None;
    }
    Some((base.to_string(), unq.to_string()))
}

/// 被执行的程序后面紧跟的那个词（剥一层配对引号）；没有 ⇒ `None`。
pub(crate) fn first_arg_of(cmd: &str) -> Option<String> {
    let mut words = cmd
        .split_whitespace()
        .skip_while(|tok| shell_quote_core::is_assignment(tok));
    words.next()?;
    words.next().map(|w| unquote_once(w).to_string())
}

/// 命令里是否**出现过**目标程序（不一定是被直接执行的那个）—— 包装写法据此判「无法判断」。
fn mentions_program(cmd: &str, want: &str) -> bool {
    cmd.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ';' | '&' | '|' | '(' | ')'))
        .any(|tok| {
            let t = unquote_once(tok);
            !t.is_empty() && t.rsplit('/').next().unwrap_or(t) == want
        })
}

/// 判一条命令是不是在调 `want`（程序 ＋ 第一个参数）并定态；`exists` 问「这个路径在不在」（注入，纯函数好测）。
pub(crate) fn classify_command(
    cmd: &str,
    want: (&str, &str),
    exists: &dyn Fn(&str) -> bool,
) -> Option<HookState> {
    let (program, verb) = want;
    let unknown = || {
        (mentions_program(cmd, program) && mentions_program(cmd, verb)).then(|| {
            HookState::Unknown {
                command: cmd.trim().to_string(),
            }
        })
    };
    let Some((base, full)) = program_of(cmd) else {
        return unknown();
    };
    if base != program || first_arg_of(cmd).as_deref() != Some(verb) {
        return unknown();
    }
    let command = cmd.trim().to_string();
    Some(if !full.contains('/') {
        HookState::InstalledViaPath { command }
    } else if exists(&full) {
        HookState::InstalledAtPath {
            command,
            path: full,
        }
    } else {
        HookState::PathMissing {
            command,
            path: full,
        }
    })
}

/// 登记着的那几条命令里调 `want` 的那一条定态：多条里能用的优先，其次 `PathMissing` / `Unknown`。
pub(crate) fn diagnose_commands(
    commands: &[String],
    want: (&str, &str),
    exists: &dyn Fn(&str) -> bool,
) -> HookState {
    let mut fallback: Option<HookState> = None;
    for cmd in commands {
        if let Some(st) = classify_command(cmd, want, exists) {
            if st.is_working() {
                return st;
            }
            fallback.get_or_insert(st);
        }
    }
    fallback.unwrap_or(HookState::NotInstalled)
}

/// 一家声明了收信钩子：照它的拼法要并进的至少有一项（[`crate::agents::FootprintFace::stop_hook_slots`]）。
fn declares_stop_hook(face: &crate::agents::FootprintFace) -> bool {
    !(face.stop_hook_slots)(&format!("{PROGRAM} {VERB}")).is_empty()
}

/// 那一家钩子文件原文 ⇒ 解析出来的对象（读不到 · 坏 JSON · 顶层不是对象 ⇒ `None`）。
fn parsed(raw: Option<&str>) -> Option<Value> {
    serde_json::from_str::<Value>(raw?.trim_start_matches('\u{feff}'))
        .ok()
        .filter(Value::is_object)
}

/// 完整诊断。`raw` = 那一家钩子文件原文；读不到 / 坏 JSON / 顶层不是对象 ⇒ `NotInstalled` ＋ `note` 说原因。
/// 哪几条命令登记在「停下时」那一种下由那一家答（[`crate::agents::FootprintFace::stop_hook_commands`]）。
pub(crate) fn diagnose(
    face: &crate::agents::FootprintFace,
    raw: Option<&str>,
    exists: &dyn Fn(&str) -> bool,
) -> HooksDiagnosis {
    let noted = |note: String| HooksDiagnosis::of(HookState::NotInstalled, note);
    let Some(raw) = raw else {
        return noted(copy_text("rsHooksDiag.diagnose.noSettings", &[]));
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw.trim_start_matches('\u{feff}'))
    else {
        return noted(copy_text("rsHooksDiag.diagnose.badJson", &[]));
    };
    if !v.is_object() {
        return noted(copy_text("rsHooksDiag.diagnose.notObject", &[]));
    }
    let commands = (face.stop_hook_commands)(&v);
    HooksDiagnosis::of(
        diagnose_commands(&commands, (PROGRAM, VERB), exists),
        String::new(),
    )
}

/// 照（键路径, 那一项）并进 `base`：路径上一层层是对象，最后一格是数组、那一项加在末尾（通用的 JSON 形状，不认哪一家的格名）。
/// 并进空对象的那一份就是要加的内容；并进现有那份，就是并好之后它的样子（信任键照它算那一条落的那一格）。
fn nest(base: Value, slots: &[(Vec<&str>, Value)]) -> Value {
    fn put(at: &mut Map<String, Value>, path: &[&str], item: &Value) {
        match path {
            [] => {}
            [last] => {
                if let Some(arr) = at
                    .entry(last.to_string())
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                {
                    arr.push(item.clone());
                }
            }
            [key, rest @ ..] => {
                if let Some(next) = at
                    .entry(key.to_string())
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                {
                    put(next, rest, item);
                }
            }
        }
    }
    let mut doc = match base {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    for (path, item) in slots {
        put(&mut doc, path, item);
    }
    Value::Object(doc)
}

/// [`nest`] 的反面：要加的内容 ⇒（键路径, 那一项），「待办」照它一项项并进那份文件。读不懂 ⇒ 空。
pub fn slots_of(snippet: &str) -> Vec<(Vec<String>, Value)> {
    fn walk(v: &Value, path: &mut Vec<String>, out: &mut Vec<(Vec<String>, Value)>) {
        match v {
            Value::Object(m) => {
                for (k, sub) in m {
                    path.push(k.clone());
                    walk(sub, path, out);
                    path.pop();
                }
            }
            Value::Array(items) => {
                out.extend(items.iter().map(|it| (path.clone(), it.clone())));
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    if let Ok(v) = serde_json::from_str::<Value>(snippet) {
        walk(&v, &mut Vec::new(), &mut out);
    }
    out
}

/// 要加的内容：一条「停下时」钩子直接指向 `<skills 根>/cc-bus/scripts/cc-bus`、带动词 `stop-hook`（家目录底下 ⇒ `"$HOME/…"`，否则绝对路径）；
/// 并进哪一格由那一家答（[`crate::agents::FootprintFace::stop_hook_slots`]）。入口不在 ⇒ `None`（cc-bus 没装，先装）。
pub(crate) fn snippet(
    face: &crate::agents::FootprintFace,
    skills: &Path,
    home: Option<&Path>,
) -> Option<String> {
    let command = entry_command(skills, home)?;
    serde_json::to_string_pretty(&nest(
        Value::Object(Map::new()),
        &(face.stop_hook_slots)(&command),
    ))
    .ok()
}

/// 要加的那条钩子的命令：指向 `<skills 根>/cc-bus/scripts/cc-bus`、带动词 `stop-hook`。入口不在 ⇒ `None`。
fn entry_command(skills: &Path, home: Option<&Path>) -> Option<String> {
    let script: PathBuf = skills
        .join(super::cc_bus_install::NAME)
        .join("scripts")
        .join(PROGRAM);
    if !script.is_file() {
        return None;
    }
    let word = match home.and_then(|h| script.strip_prefix(h).ok()) {
        Some(rel) => posix::home_path_word(&rel.to_string_lossy().replace('\\', "/")),
        None => shell_quote_core::posix_quote(&script.display().to_string()),
    };
    Some(format!("{word} {VERB}"))
}

/// 一家声明了信任：判哪一条 —— 装好能用的那一条照现有那份算；否则要加的那一条照并进之后的那份算；两样都没有 ⇒ `None`。
/// 信任那份文件只读（超上限 · 读不了 ⇒ 当空，算没对上）。
fn trust_of(
    trust: &crate::agents::HookTrust,
    agent_home: &Path,
    hooks_file: &Path,
    doc: Option<&Value>,
    working: Option<&str>,
    adding: Option<&str>,
    slots: fn(&str) -> Vec<(Vec<&'static str>, Value)>,
) -> Option<HookTrustDiagnosis> {
    let empty = Value::Object(Map::new());
    let doc = doc.unwrap_or(&empty);
    let (command, at) = match (working, adding) {
        (Some(c), _) => (c.to_string(), doc.clone()),
        (None, Some(c)) => (c.to_string(), nest(doc.clone(), &slots(c))),
        (None, None) => return None,
    };
    // 同一条命令登记了几次 ⇒ 要加的落在最后那一格；装着的取头一条。
    let rows = (trust.expected)(hooks_file, &at);
    // 判钩子时那条命令是修剪过的（[`classify_command`]），这里照修剪过的认那一条（值照那一家读回的原文算）。
    let mut same = rows.into_iter().filter(|(c, _, _)| c.trim() == command);
    let (_, key, value) = if working.is_some() {
        same.next()?
    } else {
        same.last()?
    };
    let file = (trust.file)(agent_home);
    let raw = Ro::read_string_capped(&file, SETTINGS_CAP_BYTES).unwrap_or_default();
    let trusted = (trust.recorded)(&raw, &key).as_deref() == Some(value.as_str());
    let file = file.display().to_string();
    let (said, tone) = if trusted {
        (copy_text("ccBusHooks.trust.ok", &[]), HookTone::Ok)
    } else {
        (
            copy_text("ccBusHooks.trust.mismatch", &[("file", &file)]),
            HookTone::Bad,
        )
    };
    Some(HookTrustDiagnosis {
        trusted,
        said: crate::common::cells::Words(said),
        tone,
        line: (!trusted)
            .then(|| (trust.merge)("", &key, &value))
            .flatten(),
        file,
        key,
        value,
    })
}

/// `$HOME/…` · `${HOME}/…` · `~/…` 按这台家目录展开；其余原样。
fn expand(s: &str, home: Option<&Path>) -> PathBuf {
    // 哪几种写法算「家目录底下」住 `platform::shell::posix`。
    match (posix::home_relative(s), home) {
        (Some(r), Some(h)) => h.join(r),
        _ => PathBuf::from(s),
    }
}

/// 帧面入口：生产注册表上的 [`answer_in`]。
pub fn answer() -> HooksReport {
    answer_in(crate::agents::REGISTRY)
}

/// `registry` 里各家（足迹那一趟交它取的那一份）：这台后端进程的家目录 · 各家的配置根 · skills 根 · 这份二进制所在平台有没有原生 tmux。
pub fn answer_in(registry: &[crate::agents::Adapter]) -> HooksReport {
    let home = crate::platform::paths::home_dir();
    answer_among(
        registry,
        home.as_deref(),
        // 各家照进程环境解的家（那一家的本机布局解）。
        &|k| {
            crate::agents::local_face_among(registry, k)
                .map(|f| (f.home_at)(None, true))
                .unwrap_or_default()
        },
        // 那一家的用户级 skill 根（取根照家只经 `skill_install` 那一处；不带家 ⇒ 照请求没写家的那一下规整（`skill_install::family_among`））。
        crate::assets::skill_install::skill_root_of(
            crate::assets::skill_install::Roots {
                registry,
                user: None,
            },
            None,
            None,
        )
        .ok()
        .flatten()
        .as_deref(),
        !matches!(
            crate::platform::tmux::TMUX_PLATFORM,
            crate::platform::tmux::TmuxPlatform::AbsentUnlessExeOnPath
        ),
    )
}

/// [`answer`] 的本体（注册表 · 家目录 · 各家的家 · skills 根 · 跑不跑得了 cc-bus 是参数，判据拿替身家与夹具喂）。
/// 足迹面声明了收信钩子的每一家各判一份；只读，两份文件都不写。
pub fn answer_among(
    registry: &[crate::agents::Adapter],
    home: Option<&Path>,
    agent_home: &dyn Fn(&str) -> PathBuf,
    skills: Option<&Path>,
    supported: bool,
) -> HooksReport {
    let exists = |s: &str| expand(s, home).exists();
    let skills = skills.filter(|_| supported);
    let families = registry
        .iter()
        .filter_map(|a| Some((a, a.footprint?)))
        .filter(|(_, face)| declares_stop_hook(face))
        .map(|(a, face)| {
            let dir = agent_home(a.kind);
            // 钩子写在那一家用户级设置的头一份里（足迹面 `user_settings`）。
            let [settings, _] = (face.user_settings)(&dir);
            // 超上限 / 不是普通文件 / 读不了 ⇒ 按「没读到」降级，`note` 说出来（`diagnose(None)`）。
            let raw = Ro::read_string_capped(&settings, SETTINGS_CAP_BYTES).ok();
            let diagnosis = diagnose(&face, raw.as_deref(), &exists);
            let working = match &diagnosis.stop {
                HookState::InstalledViaPath { command }
                | HookState::InstalledAtPath { command, .. } => Some(command.as_str()),
                _ => None,
            };
            let adding = skills.and_then(|k| entry_command(k, home));
            let trust = face.hook_trust.as_ref().and_then(|t| {
                trust_of(
                    t,
                    &dir,
                    &settings,
                    parsed(raw.as_deref()).as_ref(),
                    working,
                    adding.as_deref(),
                    face.stop_hook_slots,
                )
            });
            FamilyHooks {
                kind: a.kind.to_string(),
                name: crate::agents::family_said_among(registry, a.kind),
                snippet: skills.and_then(|k| snippet(&face, k, home)),
                source: settings.display().to_string(),
                diagnosis,
                trust,
            }
        })
        .collect();
    HooksReport {
        supported,
        families,
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/cc_bus_hooks_tests.rs"]
mod tests;
