//! 「待办」那几件（`机器配置-v2.md` §2–§3）：这台后端按事实判类 · 态、算好要贴的东西（行号 · diff · 合好的整份），界面只画。
//!
//! - 类：`must`（要做）· `install`（要装）· `decide`（待定）· `installOptional`（要装 · 可选）· `optional`（可选）。
//!   角标只数前三类里还没做完的（[`badge`]）。
//! - 态：`todo` · `done`（自己认出）· `expired`（贴过的失效了，升成要做）· `blocked`（先决没满足）· `declined`（可选的点过「不用了」）。
//!   「已复制」只住界面（点了复制的那一刻），不进这里。
//! - 判定只在这里：事实由 [`gather`] 从这台收（别名块 · PATH 上的 ccm · 设置文件 · 记下的选择），本模块是纯函数。

pub(crate) mod gather;
pub(crate) mod marks;
pub(crate) mod patch;

use copy_core::copy_text;
use patch::{DiffLine, Plan};
use serde_json::{json, Value};

/// PATH 上先找到的那个 `ccm` 不是 cc-monitor 放的。
pub(crate) struct StaleCcm {
    pub path: String,
    /// 它所在的目录这个用户写不了 ⇒ 命令前加 `sudo`。
    pub needs_root: bool,
}

/// 你选了「我自己贴」那 N 行。
pub(crate) struct SelfPaste {
    pub rc: String,
    pub lines: Vec<String>,
    /// 贴在第几行之后（那份文件现在的行数）。
    pub after_line: usize,
    /// 那份文件里已经有别名块了。
    pub present: bool,
}

/// 块外与清单同名的函数。
pub(crate) struct Clash {
    pub name: String,
    pub path: String,
    pub line: usize,
    /// `yours` · `list` · `unclear`。
    pub wins: String,
}

/// 一份文件里确证失效的几行（行号 · 原文）。
pub(crate) struct DeadLines {
    pub path: String,
    pub lines: Vec<(usize, String)>,
}

/// 直接敲的 agent 也走中转（实时显示）：一家一件。
pub(crate) struct Relay {
    /// 那一家的路由名（这一件的 id 按它分）。
    pub agent: String,
    /// 那一家给人看的名字。
    pub name: String,
    /// 地址住那份文件里哪一格（位置行）。
    pub slot: &'static str,
    /// （现在的内容, 地址）→ 合好的整份（那一家的格式，注册表 `SettingsEnvFace.merge`）。
    pub merge: fn(&str, &str) -> Option<String>,
    /// `installed` · `stale` · `absent`（同 `relay-optin`）。
    pub state: String,
    pub path: String,
    /// 那份设置文件现在的内容（不在 ⇒ `None`）。
    pub text: Option<String>,
    /// 要写进去的地址（带钥匙）；生成不了 ⇒ `None`。
    pub url: Option<String>,
    /// 地址里那把钥匙（界面显示时遮住）。
    pub secret: Option<String>,
}

impl Relay {
    /// 按现在的内容合上地址的那一份改法（合不出 ⇒ `None`）。
    fn plan(&self, url: &str) -> Option<Plan> {
        let now = self.text.as_deref().unwrap_or("");
        let whole = (self.merge)(now, url)?;
        let (diff, at_line) = patch::line_diff_of(now, &whole);
        Some(Plan {
            whole,
            diff,
            at_line,
        })
    }
}

/// cc-bus 自动收信的两条钩子。
pub(crate) struct Hooks {
    /// 这台装了 cc-bus（两个脚本都在）。
    pub ccbus: bool,
    /// 两条钩子都在、指向的脚本都在。
    pub installed: bool,
    pub path: String,
    pub text: Option<String>,
    /// 要加的：每个事件一项（事件名 · 那一项）。
    pub items: Vec<(String, Value)>,
}

/// 收齐的事实。
pub(crate) struct Facts {
    pub windows: bool,
    /// 足迹里这台确实缺的那几样（`data.rs::missing` 的形状）。
    pub needs_install: Vec<Value>,
    pub stale_ccm: Option<StaleCcm>,
    pub self_paste: Option<SelfPaste>,
    pub clashes: Vec<Clash>,
    pub dead: Vec<DeadLines>,
    pub relay: Vec<Relay>,
    pub hooks: Option<Hooks>,
    /// 用户自己配的（全局 · 本项目 · 项目文件里的）、要登录的 MCP（`mcp-read` 那一份判定）。
    pub mcp_login: Vec<McpLogin>,
    /// 点过「不用了」的那几件（`marks.rs`）。
    pub declined: Vec<String>,
}

/// 一个要登录的 MCP：名字 · 在哪几个号里（账号库里的名字；没设账号 ⇒ 空）。
pub(crate) struct McpLogin {
    pub name: String,
    pub who: Vec<String>,
}

/// 一件的成品（线上形状由判据按键集钉住）。
struct Chore {
    id: String,
    kind: &'static str,
    state: &'static str,
    name: String,
    loc: String,
    said: String,
    why: String,
    steps: Vec<String>,
    diff: Vec<DiffLine>,
    copy: Option<String>,
    whole: Option<String>,
    whole_covers: Vec<String>,
    file: Option<String>,
    go: Option<Value>,
    how_url: Option<String>,
    mask: Option<String>,
    action: &'static str,
}

impl Chore {
    fn new(id: impl Into<String>, kind: &'static str, action: &'static str) -> Chore {
        Chore {
            id: id.into(),
            kind,
            state: "todo",
            name: String::new(),
            loc: String::new(),
            said: String::new(),
            why: String::new(),
            steps: vec![],
            diff: vec![],
            copy: None,
            whole: None,
            whole_covers: vec![],
            file: None,
            go: None,
            how_url: None,
            mask: None,
            action,
        }
    }

    fn wire(self) -> Value {
        let diff: Vec<Value> = self
            .diff
            .into_iter()
            .map(|l| match l {
                DiffLine::Same(n, t) => json!({"n": n, "op": "same", "text": t}),
                DiffLine::Del(n, t) => json!({"n": n, "op": "del", "text": t}),
                DiffLine::Add(t) => json!({"n": null, "op": "add", "text": t}),
            })
            .collect();
        json!({
            "id": self.id, "kind": self.kind, "state": self.state, "name": self.name, "loc": self.loc,
            "said": self.said, "why": self.why, "steps": self.steps, "diff": diff, "copy": self.copy,
            "whole": self.whole, "wholeCovers": self.whole_covers, "file": self.file, "go": self.go,
            "howUrl": self.how_url, "mask": self.mask, "action": self.action,
        })
    }
}

/// 进角标的件数：要做 ＋ 要装 ＋ 待定，还没做完的（没做 · 过期）。
pub(crate) fn badge(chores: &[Value]) -> usize {
    chores
        .iter()
        .filter(|c| matches!(c["kind"].as_str(), Some("must" | "install" | "decide")))
        .filter(|c| matches!(c["state"].as_str(), Some("todo" | "expired")))
        .count()
}

/// 事实 ⇒ 各件（急的在前：要做 · 待定 · 要装 · 要装 · 可选 · 可选）。
pub(crate) fn chores(f: &Facts) -> Vec<Value> {
    let mut out: Vec<Chore> = Vec::new();
    if let Some(s) = &f.stale_ccm {
        let mut c = Chore::new("stale-ccm", "must", "copyCommand");
        c.name = copy_text("beChore.staleCcm.name", &[]);
        c.loc = s.path.clone();
        c.said = copy_text("beChore.staleCcm.said", &[]);
        c.why = copy_text("beChore.staleCcm.why", &[("path", &s.path)]);
        c.steps = vec![copy_text("beChore.staleCcm.step", &[])];
        let cmd = format!("rm {}", shell_quote_core::posix_quote(&s.path));
        c.copy = Some(if s.needs_root {
            format!("sudo {cmd}")
        } else {
            cmd
        });
        out.push(c);
    }
    if let Some(p) = &f.self_paste {
        let mut c = Chore::new("self-paste", "must", "copySnippet");
        let n = p.lines.len().to_string();
        let line = p.after_line.to_string();
        c.name = copy_text("beChore.selfPaste.name", &[]);
        c.loc = copy_text(
            "beChore.selfPaste.loc",
            &[("path", &p.rc), ("n", &n), ("line", &line)],
        );
        c.said = copy_text("beChore.selfPaste.said", &[("n", &n)]);
        c.why = copy_text("beChore.selfPaste.why", &[]);
        c.steps = vec![
            copy_text("beChore.step.open", &[("path", &p.rc)]),
            copy_text("beChore.selfPaste.step", &[("line", &line), ("n", &n)]),
            copy_text("beChore.selfPaste.stepSave", &[]),
        ];
        c.diff = p.lines.iter().map(|l| DiffLine::Add(l.clone())).collect();
        c.copy = Some(p.lines.join("\n"));
        c.file = Some(p.rc.clone());
        if p.present {
            c.state = "done";
        }
        out.push(c);
    }
    for k in &f.clashes {
        let mut c = Chore::new(format!("clash:{}", k.name), "decide", "decide");
        let line = k.line.to_string();
        c.name = copy_text("beChore.clash.name", &[("name", &k.name)]);
        c.loc = copy_text("beChore.clash.loc", &[("path", &k.path), ("line", &line)]);
        c.said = match k.wins.as_str() {
            "yours" => copy_text("beChore.clash.winsYours", &[]),
            "list" => copy_text("beChore.clash.winsList", &[]),
            _ => copy_text("beChore.clash.winsUnclear", &[]),
        };
        c.why = copy_text("beChore.clash.why", &[("name", &k.name)]);
        c.copy = Some(format!("{}:{}", k.path, k.line));
        c.file = Some(k.path.clone());
        c.go = Some(json!({"page": "machine", "tab": "config", "anchor": "clash"}));
        out.push(c);
    }
    for n in &f.needs_install {
        let required = n["required"] == json!(true);
        let id = n["id"].as_str().unwrap_or_default();
        let mut c = Chore::new(
            format!("install:{id}"),
            if required {
                "install"
            } else {
                "installOptional"
            },
            "how",
        );
        c.name = n["name"].as_str().unwrap_or_default().to_string();
        c.loc = n["what"].as_str().unwrap_or_default().to_string();
        c.said = if required {
            copy_text("beChore.install.saidRequired", &[])
        } else {
            copy_text("beChore.install.saidOptional", &[])
        };
        c.why = c.said.clone();
        c.how_url = n["howUrl"].as_str().map(str::to_string);
        out.push(c);
    }
    for d in &f.dead {
        let mut c = Chore::new(format!("dead:{}", d.path), "optional", "locate");
        let file = d
            .path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&d.path)
            .to_string();
        let nums: Vec<String> = d.lines.iter().map(|(n, _)| n.to_string()).collect();
        c.name = copy_text(
            "beChore.dead.name",
            &[("file", &file), ("n", &d.lines.len().to_string())],
        );
        c.loc = copy_text(
            "beChore.dead.loc",
            &[("path", &d.path), ("lines", &nums.join(", "))],
        );
        c.said = copy_text("beChore.dead.said", &[]);
        c.why = copy_text("beChore.dead.why", &[]);
        c.steps = d
            .lines
            .iter()
            .map(|(n, text)| {
                copy_text(
                    "beChore.dead.step",
                    &[("n", &n.to_string()), ("text", text)],
                )
            })
            .collect();
        c.copy = d.lines.first().map(|(n, _)| format!("{}:{}", d.path, n));
        c.file = Some(d.path.clone());
        out.push(c);
    }
    let mut same_file: Vec<(usize, Plan)> = Vec::new();
    for r in &f.relay {
        let mut c = Chore::new(&format!("relay:{}", r.agent), "optional", "copySnippet");
        c.name = copy_text("beChore.relay.name", &[("agent", &r.name)]);
        c.loc = copy_text("beChore.relay.loc", &[("path", &r.path), ("slot", r.slot)]);
        c.why = copy_text("beChore.relay.why", &[]);
        c.file = Some(r.path.clone());
        c.mask = r.secret.clone();
        match (r.state.as_str(), &r.url) {
            ("installed", _) => {
                c.state = "done";
                c.said = copy_text("beChore.relay.saidDone", &[]);
            }
            (_, None) => {
                c.state = "blocked";
                c.said = copy_text("beChore.relay.saidBlocked", &[]);
            }
            (state, Some(url)) => {
                if state == "stale" {
                    c.kind = "must";
                    c.state = "expired";
                    c.said = copy_text("beChore.relay.saidExpired", &[]);
                } else {
                    c.said = copy_text("beChore.relay.saidTodo", &[]);
                }
                if let Some(p) = r.plan(url) {
                    fill_plan(&mut c, &r.path, &p);
                    same_file.push((out.len(), p));
                }
            }
        }
        out.push(c);
    }
    if let Some(h) = f.hooks.as_ref().filter(|_| !f.windows) {
        let mut c = Chore::new("cc-bus-hooks", "optional", "copySnippet");
        c.name = copy_text("beChore.hooks.name", &[]);
        c.loc = copy_text("beChore.hooks.loc", &[("path", &h.path)]);
        c.why = copy_text("beChore.hooks.why", &[]);
        c.file = Some(h.path.clone());
        if !h.ccbus {
            c.state = "blocked";
            c.action = "installFirst";
            c.said = copy_text("beChore.hooks.saidBlocked", &[]);
            c.go = Some(json!({"page": "ext"}));
        } else if h.installed {
            c.state = "done";
            c.said = copy_text("beChore.hooks.said", &[]);
        } else {
            c.said = copy_text("beChore.hooks.said", &[]);
            let mut text = h.text.clone().unwrap_or_default();
            let mut first: Option<Plan> = None;
            for (event, item) in &h.items {
                match patch::push_item(&text, &["hooks", event], item) {
                    Some(p) => {
                        text = p.whole.clone();
                        first.get_or_insert(p);
                    }
                    None => {
                        first = None;
                        break;
                    }
                }
            }
            if let Some(mut p) = first {
                // diff 按现在的文件算（一次加齐的那几行），整份是加齐之后的。
                let all = patch::line_diff_of(h.text.as_deref().unwrap_or(""), &text);
                p.diff = all.0;
                p.at_line = all.1;
                p.whole = text;
                fill_plan(&mut c, &h.path, &p);
                same_file.push((out.len(), p));
            }
        }
        out.push(c);
    }
    for m in &f.mcp_login {
        let mut c = Chore::new(format!("mcp-login:{}", m.name), "optional", "copyCommand");
        c.name = copy_text("beChore.mcpLogin.name", &[("name", &m.name)]);
        c.loc = m.who.join(&copy_text("beChore.mcpLogin.whoSep", &[]));
        c.said = copy_text("beChore.mcpLogin.said", &[]);
        c.why = copy_text("beChore.mcpLogin.why", &[]);
        c.steps = vec![copy_text("beChore.mcpLogin.step", &[("name", &m.name)])];
        c.copy = Some("/mcp".to_string());
        out.push(c);
    }
    // 同一份文件里有两件都没做 ⇒ 各自的「整份」都给含两件的那一份（按现在的内容先合一件、再合另一件）。
    //   能落在同一份文件里的只有「实时显示」某一家 ＋ 收信那两件（收信那一件排在最后）。
    if let Some(((b, _), rest)) = same_file.split_last() {
        let b = *b;
        if let Some(a) = rest
            .iter()
            .map(|(a, _)| *a)
            .find(|a| out[*a].file == out[b].file)
        {
            let both = merge_both(f);
            if let Some(both) = both {
                let covers = vec![out[a].id.clone(), out[b].id.clone()];
                for k in [a, b] {
                    out[k].whole = Some(both.clone());
                    out[k].whole_covers = covers.clone();
                }
            }
        }
    }
    for c in out.iter_mut() {
        if c.state == "todo" && c.kind == "optional" && f.declined.contains(&c.id) {
            c.state = "declined";
        }
    }
    let rank = |k: &str| {
        ["must", "decide", "install", "installOptional", "optional"]
            .iter()
            .position(|x| *x == k)
            .unwrap_or(9)
    };
    out.sort_by_key(|c| rank(c.kind));
    out.into_iter().map(Chore::wire).collect()
}

/// 实时显示与收信落在同一份文件：先合实时显示、再在合好的那份上加两条钩子。
fn merge_both(f: &Facts) -> Option<String> {
    let h = f.hooks.as_ref()?;
    let r = f.relay.iter().find(|r| r.path == h.path)?;
    let mut text = r.plan(r.url.as_ref()?)?.whole;
    for (event, item) in &h.items {
        text = patch::push_item(&text, &["hooks", event], item)?.whole;
    }
    Some(text)
}

/// 一份改法填进那一件：怎么做的几步 · diff · 要复制的那几行 · 整份。
fn fill_plan(c: &mut Chore, path: &str, p: &Plan) {
    let added: Vec<String> = p
        .diff
        .iter()
        .filter_map(|l| {
            if let DiffLine::Add(t) = l {
                Some(t.clone())
            } else {
                None
            }
        })
        .collect();
    let replaces = p.diff.iter().any(|l| matches!(l, DiffLine::Del(..)));
    let n = added.len().to_string();
    let line = p.at_line.to_string();
    let first_del = p.diff.iter().find_map(|l| {
        if let DiffLine::Del(n, _) = l {
            Some(n.to_string())
        } else {
            None
        }
    });
    c.steps = vec![
        copy_text("beChore.step.open", &[("path", path)]),
        match first_del {
            Some(d) if replaces => copy_text("beChore.step.replace", &[("line", &d), ("n", &n)]),
            _ if p.at_line == 0 => copy_text("beChore.step.insertTop", &[("n", &n)]),
            _ => copy_text("beChore.step.insert", &[("line", &line), ("n", &n)]),
        },
        copy_text("beChore.step.save", &[]),
    ];
    c.diff = p.diff.clone();
    c.copy = Some(added.join("\n"));
    c.whole = Some(p.whole.clone());
    c.whole_covers = vec![c.id.clone()];
}

#[cfg(test)]
#[path = "../../../../tests/backend/footprint/chores_tests.rs"]
mod tests;
