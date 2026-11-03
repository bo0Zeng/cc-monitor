//! 那一行 `ccm …` 的字节表（远端那一形）：本包的渲染判据与拨号那一侧「流进 `terminal-ssh` 的每一条远端命令」那条判据共用。

use std::collections::BTreeSet;

/// 那台 ccm 会的那几样（整份；拒的那几格各缺一样）。
const ALL_CAPS: &[&str] = &[
    "new",
    "resume",
    "attach",
    "tmux",
    "account",
    "model",
    "cwd",
    "agent",
    "launcher",
    "ccm-sid",
    "print",
    "detach",
    "tmux-size",
];

/// 起什么（那一家的启动命令照缺省的偏好）；各格只改要改的那几样。
pub fn launch(agent: &str, act: super::super::Act) -> super::super::Launch {
    let (launcher, default_launcher) =
        super::super::Launch::launchers(agent, &crate::common::machine_prefs::Prefs::default());
    super::super::Launch {
        agent: agent.into(),
        act,
        tmux: None,
        cwd: None,
        ccm_sid: None,
        model: None,
        launcher,
        default_launcher,
        preset_args: Vec::new(),
        alias: None,
    }
}

/// 那台判好的号：具名那一形带这台模型偏好里那个号的那一条。
pub fn named(name: &str, model: Option<&str>) -> crate::control::launch_account::Settled {
    crate::control::launch_account::Settled::Account(
        crate::control::launch_account::LaunchedAccount {
            name: name.into(),
            config_dir: format!("/h/.cc/{name}"),
            model: model.map(str::to_string),
        },
    )
}

/// 一格：哪一形 · 起什么 · 那台判好的号 · 那台 ccm 缺哪一样（`None` ＝ 整份）· 那一行或拒的那一句。
pub type Cell = (
    &'static str,
    super::super::Launch,
    crate::control::launch_account::Settled,
    Option<&'static str>,
    Result<&'static str, &'static str>,
);

/// **那一行 `ccm …` 的字节只在这一张表里钉**（远端那一形；别处要「渲得出来的每一行」也取这一张）。
pub fn cells() -> Vec<Cell> {
    use super::super::{Act, Launch};
    use crate::control::launch_account::Settled;
    let resume = |sid: &str| Act::Resume(sid.into());
    let tmux = |mut l: Launch, name: &str| {
        l.tmux = Some(name.into());
        l
    };
    let cwd = |mut l: Launch, c: &str| {
        l.cwd = Some(c.into());
        l
    };
    let attach = |name: &str| tmux(launch("claude", Act::Attach(name.into())), name);
    let claude = |act| launch("claude", act);
    let mut ident = claude(Act::New);
    ident.ccm_sid = Some("sid-1".into());
    let mut own_launcher = claude(Act::New);
    own_launcher.launcher = "mycc".into();
    let mut same_launcher = claude(Act::New);
    same_launcher.launcher = "claude".into();
    let mut probe = cwd(tmux(claude(Act::New), "cc-proj"), "/home/pi/my proj");
    probe.launcher = "CCMPROBE".into();
    let mut p1 = cwd(tmux(claude(resume("p1")), "cc-p1"), "/tmp");
    p1.ccm_sid = Some("p1".into());
    let mut p1_opus = p1.clone();
    p1_opus.model = Some("opus".into());
    vec![
        ("new + base", claude(Act::New), Settled::Base, None, Ok("ccm -- new --ccm-agent claude --base")),
        ("new + 具名", claude(Act::New), named("z", None), None, Ok("ccm -- new --ccm-agent claude --account z")),
        (
            "resume + tmux + 具名",
            tmux(claude(resume("abc-123")), "cc-abc123"),
            named("z", None),
            None,
            Ok("ccm --resume abc-123 -- --ccm-tmux=cc-abc123 --ccm-agent claude --account z"),
        ),
        (
            "resume + cwd + 号带来的模型",
            cwd(claude(resume("s1")), "/w"),
            named("z", Some("opus")),
            None,
            Ok("ccm --resume s1 --model opus -- --ccm-agent claude --account z --cwd /w"),
        ),
        ("身份标记", ident, Settled::Base, None, Ok("ccm -- new --ccm-sid=sid-1 --ccm-agent claude --base")),
        ("自定义启动命令", own_launcher, Settled::Base, None, Ok("ccm -- new --ccm-agent claude --base --launcher mycc")),
        ("启动命令等于默认 ⇒ 不写 --launcher", same_launcher, Settled::Base, None, Ok("ccm -- new --ccm-agent claude --base")),
        ("接回（不起 agent，不收修饰）", attach("cc-foo"), Settled::Base, None, Ok("ccm -- --attach cc-foo")),
        (
            "要引号的目录",
            cwd(claude(Act::New), "/home/用户/带 空格"),
            Settled::Base,
            None,
            Ok("ccm -- new --ccm-agent claude --base --cwd '/home/用户/带 空格'"),
        ),
        ("缺无条件能力 cwd", claude(Act::New), Settled::Base, Some("cwd"), Err("此机 ccm 不支持这样启动会话 · 缺 cwd")),
        (
            "建 tmux 会话而这台没有 tmux",
            tmux(claude(Act::New), "cc-x"),
            Settled::Base,
            Some("tmux"),
            Err("此机 ccm 不支持这样启动会话 · 缺 tmux"),
        ),
        (
            "用到的模型维度缺能力",
            claude(Act::New),
            named("z", Some("opus")),
            Some("model"),
            Err("此机 ccm 不认设置项 model · 缺 model"),
        ),
        ("用到的账号维度缺能力", claude(Act::New), Settled::Base, Some("account"), Err("此机 ccm 不认设置项 account · 缺 account")),
        (
            "坏 sid",
            claude(resume("-x")),
            Settled::Base,
            None,
            Err("会话 ID \"-x\" 不合法 · 1–64 位 · 只许 A-Z a-z 0-9 与 - · 不以 - 开头"),
        ),
        (
            "Codex 恢复（直路，跟随落账号 0）",
            cwd(launch("codex", resume("s1")), "/p"),
            Settled::Unsaid,
            None,
            Ok("ccm resume s1 -- --ccm-agent codex --base --cwd /p"),
        ),
        ("Codex 选了具名账号", cwd(launch("codex", resume("s1")), "/p"), named("z", None), None, Err("Codex 会话不可选账号")),
        (
            "直路恢复 ＋ 具名",
            cwd(claude(resume("s1")), "/p"),
            named("z", None),
            None,
            Ok("ccm --resume s1 -- --ccm-agent claude --account z --cwd /p"),
        ),
        ("接回 cc-s1", attach("cc-s1"), Settled::Base, None, Ok("ccm -- --attach cc-s1")),
        (
            "恢复进 tmux ＋ 身份标记",
            p1,
            Settled::Base,
            None,
            Ok("ccm --resume p1 -- --ccm-tmux=cc-p1 --ccm-sid=p1 --ccm-agent claude --base --cwd /tmp"),
        ),
        (
            "新起进 tmux ＋ 自定义启动命令",
            probe,
            Settled::Base,
            None,
            Ok("ccm -- new --ccm-tmux=cc-proj --ccm-agent claude --base --cwd '/home/pi/my proj' --launcher CCMPROBE"),
        ),
        ("接回 cc-p1", attach("cc-p1"), Settled::Base, None, Ok("ccm -- --attach cc-p1")),
        (
            "恢复进 tmux ＋ 点名模型",
            p1_opus,
            Settled::Base,
            None,
            Ok("ccm --resume p1 --model opus -- --ccm-tmux=cc-p1 --ccm-sid=p1 --ccm-agent claude --base --cwd /tmp"),
        ),
    ]
}

/// 那台 ccm 会的那几样，缺 `missing` 那一样。
pub fn caps_without_one(missing: Option<&str>) -> BTreeSet<String> {
    ALL_CAPS
        .iter()
        .filter(|c| Some(**c) != missing)
        .map(|c| (*c).to_string())
        .collect()
}

/// 那一格渲出来的那一行（远端那一形）。
pub fn render_cell(c: &Cell) -> Result<String, String> {
    super::super::line_of(
        &c.1,
        &c.2,
        super::super::Here::Remote,
        &caps_without_one(c.3),
        "ccm",
    )
}

/// 按 POSIX 单引号规则切一行（渲染器只用 `posix_quote` 的单引号形，`'\''` 那一形也认）。
pub fn shell_words(line: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted, mut any) = (Vec::new(), String::new(), false, false);
    for c in line.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                any = true;
            }
            ' ' if !quoted => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                any = false;
            }
            '\\' if !quoted => {}
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}
