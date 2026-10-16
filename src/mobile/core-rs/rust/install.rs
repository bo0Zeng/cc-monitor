//! **手机给一台装 / 升级后端**的宿主：判定与 ①–④ 是 `deploy` crate 那一份（桌面后端答 `deploy-plan` 用的同一份），
//! ⑤ 换常驻是 `resident-client` 那一条（`Feed::replace`：`--resident-ensure --replace` 再整份重接）。这里只接线、写话。
//!
//! 一个入口（`Core.deploy(handle, slots)`），事经那台常驻流同一个口交 Kotlin（种类 `deploy`，一行 JSON）：
//! - `{"step": "plan" | "place" | "restart", "text", "bar"}`：走到哪一步（话是写好的；`bar` ⇒ 放字节那一步，屏上走一条进度条）；
//! - `{"done": "skip" | "deploy" | "keep", "text"}`：做完了（已是这一版 · 装上了这一版 · 那台不比这一版旧、没动它）；
//! - `{"failed": <码>, "text", "detail"}`：没做成（那一句 ＋ 复制详情）。
//!
//! 字节：App 把包里那两格解到自己的私有目录（每版一次），交路径；这里从文件读，不经 JNI 传数组。

use std::sync::{Arc, Mutex, OnceLock};

use copy_core::copy_text;
use deploy_contract::DeployAction;

use crate::resident::{self, OnEvent};

/// 种类：交 Kotlin 时用的那个词。
pub const KIND: &str = "deploy";

/// 正在装的那几台（一台同时只装一趟）。
fn busy() -> &'static Mutex<std::collections::HashSet<i64>> {
    static B: OnceLock<Mutex<std::collections::HashSet<i64>>> = OnceLock::new();
    B.get_or_init(Default::default)
}

/// 走到哪一步了。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Plan,
    Place { updating: bool },
    Restart,
}

/// 一步 ⇒ 交 Kotlin 的那一行（话同桌面机器页那几句）。
pub fn step_json(s: Step, machine: &str) -> String {
    let connecting = |stage: &str| {
        copy_text(
            "machineState.busy.connecting",
            &[("machine", machine), ("stage", &copy_text(stage, &[]))],
        )
    };
    let (step, text, bar) = match s {
        Step::Plan => ("plan", connecting("machineState.stage.deploy"), false),
        Step::Place { updating: true } => (
            "place",
            copy_text("machineState.busy.updating", &[("machine", machine)]),
            true,
        ),
        Step::Place { updating: false } => (
            "place",
            copy_text("machineState.busy.installing", &[("machine", machine)]),
            true,
        ),
        Step::Restart => ("restart", connecting("machineState.stage.attach"), false),
    };
    serde_json::json!({ "step": step, "text": text, "bar": bar }).to_string()
}

/// 做完了 ⇒ 那一行（话同桌面［部署后端］那几句；那台不比这一版旧 ⇒ 计划里那一句）。
pub fn done_json(plan: &deploy::Plan) -> String {
    let path = format!("~/{}", relay_route_core::BACKEND_LANDING_REL);
    let machine = plan.key.label();
    let (done, text) = match &plan.action {
        DeployAction::Skip => (
            "skip",
            copy_text(
                "rsSftp.deploy.upToDate",
                &[
                    ("buildId", &plan.expected),
                    ("machine", &machine),
                    ("path", &path),
                ],
            ),
        ),
        DeployAction::Deploy(reason) => (
            "deploy",
            copy_text(
                "rsSftp.deploy.done",
                &[
                    ("buildId", &plan.expected),
                    ("machine", &machine),
                    ("path", &path),
                    ("reason", reason),
                ],
            ),
        ),
        DeployAction::Keep { why, .. } => ("keep", why.clone()),
    };
    serde_json::json!({ "done": done, "text": text }).to_string()
}

/// 没做成 ⇒ 那一行（详情：命令 · 码 · 原话，钟是这台手机的）。
pub fn failed_json(code: &str, text: &str, raw: Option<&str>) -> String {
    let detail = ssh_dial::env::get().facts.detail("deploy", Some(code), raw);
    serde_json::json!({ "failed": code, "text": text, "detail": detail }).to_string()
}

/// `slots` 那一段：`[{os, arch, id, path}]`（Kotlin 交：包里带的那几格、各自构建时扫出来的身份、解到哪个文件）。
/// 认不出的行 ⇒ 丢掉（计划那一步照实说「这一版没带」）。
pub fn slots_of(json: &str) -> Vec<deploy::Slot> {
    let rows: Vec<serde_json::Value> = serde_json::from_str(json).unwrap_or_default();
    rows.iter()
        .filter_map(|r| {
            let s = |k: &str| r.get(k).and_then(serde_json::Value::as_str);
            let key = deploy_contract::key_of(s("os")?, s("arch")?).ok()?;
            Some(deploy::Slot {
                key,
                id: s("id").filter(|i| !i.is_empty())?.to_string(),
                path: s("path")?.into(),
            })
        })
        .collect()
}

/// 装完之后要不要换常驻：放上了这一版 ⇒ 换；已是这一版 ⇒ 接着的那一位旧（或根本没接着）才换；那台更新 ⇒ 不动。
pub fn restart_after(plan: &deploy::Plan, theirs: Option<&str>) -> bool {
    match &plan.action {
        DeployAction::Deploy(_) => true,
        DeployAction::Skip => theirs.is_none_or(|t| {
            deploy_contract::build_relation(t, &plan.expected) == deploy_contract::Relation::Older
        }),
        DeployAction::Keep { .. } => false,
    }
}

/// 给编号 `h` 那台装一趟（在当前运行时上起）。那台不在 · 正在装 ⇒ `false`（不起第二趟）。
pub fn start(h: i64, slots_json: &str, on: OnEvent) -> bool {
    let Some(feed) = resident::feed(h) else {
        return false;
    };
    let machine = feed.machine().to_string();
    let Some(base) = resident::base(h) else {
        on(
            h,
            KIND,
            failed_json(
                "offline",
                &copy_text("front.body.offline", &[("machine", &machine)]),
                None,
            ),
        );
        return true;
    };
    if !busy().lock().unwrap_or_else(|e| e.into_inner()).insert(h) {
        return false;
    }
    let slots = slots_of(slots_json);
    tokio::spawn(async move {
        run(h, base, slots, feed, &machine, &on).await;
        busy().lock().unwrap_or_else(|e| e.into_inner()).remove(&h);
    });
    true
}

async fn run(
    h: i64,
    base: serde_json::Value,
    slots: Vec<deploy::Slot>,
    feed: Arc<resident_client::feed::Feed<resident::PhoneLink>>,
    machine: &str,
    on: &OnEvent,
) {
    let emit = |body: String| on(h, KIND, body);
    let req: ssh_dial::DialRequest = match serde_json::from_value(base.clone()) {
        Ok(r) => r,
        Err(e) => {
            let text = copy_text("mobSsh.conn.unavailable", &[]);
            return emit(failed_json("bad_request", &text, Some(&e.to_string())));
        }
    };
    // 一次性命令走常驻流那条链路的 capture（同一条池连接；手机的 host key 拨号时已经回调落库，ack 不用）。
    let link = Arc::new(resident::PhoneLink::new(base));
    let exec: deploy::Exec = Box::new(move |command| {
        let link = Arc::clone(&link);
        Box::pin(async move {
            use resident_client::feed::Transport as _;
            let got = link.capture(command).await.map_err(|p| p.text)?;
            let captured = ssh_dial::Captured {
                stdout: got.stdout,
                stderr: got.stderr,
                exit_status: got.exit_status,
            };
            Ok((captured, serde_json::Value::Null))
        })
    });
    let facing = deploy::DialFacing::new(req, exec);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let step = |s: deploy::Step| {
        emit(step_json(
            match s {
                deploy::Step::Plan => Step::Plan,
                deploy::Step::Place { updating } => Step::Place { updating },
            },
            machine,
        ))
    };
    let load = |p: &std::path::Path| std::fs::read(p);
    let plan = match deploy::install(&facing, &slots, &load, machine, now, &step).await {
        Ok(p) => p,
        Err(f) => return emit(failed_json(f.code, &f.said, f.raw.as_deref())),
    };
    if restart_after(&plan, feed.theirs().as_deref()) {
        emit(step_json(Step::Restart, machine));
        if let Err(p) = feed.replace().await {
            let raw = (!p.detail.is_empty()).then_some(p.detail.as_str());
            return emit(failed_json("restart_failed", &p.text, raw));
        }
    } else {
        feed.wake();
    }
    emit(done_json(&plan));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(action: DeployAction) -> deploy::Plan {
        deploy::Plan {
            key: deploy_contract::key_of("Linux", "x86_64").unwrap(),
            expected: "p9b-mine".into(),
            action,
            leftovers: vec![],
            ack: serde_json::Value::Null,
        }
    }

    fn v(s: &str) -> serde_json::Value {
        serde_json::from_str(s).expect("JSON")
    }

    /// 每一步交的话同桌面机器页那几句；只有放字节那一步走进度条。
    #[test]
    fn each_step_says_the_desktop_line_and_only_placing_runs_a_bar() {
        let plan = v(&step_json(Step::Plan, "box"));
        assert_eq!(plan["step"], "plan");
        assert_eq!(plan["bar"], false);
        assert!(copy_core::copy_matches(
            "machineState.busy.connecting",
            plan["text"].as_str().unwrap()
        ));
        let install = v(&step_json(Step::Place { updating: false }, "box"));
        assert_eq!(
            install["text"],
            copy_text("machineState.busy.installing", &[("machine", "box")])
        );
        assert_eq!(install["bar"], true);
        let update = v(&step_json(Step::Place { updating: true }, "box"));
        assert_eq!(
            update["text"],
            copy_text("machineState.busy.updating", &[("machine", "box")])
        );
        assert_eq!(v(&step_json(Step::Restart, "box"))["step"], "restart");
    }

    /// 结局三种各一句：装上了（带这一版与原因）· 已是这一版 · 那台更新（计划里那一句，不叫人去别处卸载）。
    #[test]
    fn the_outcome_names_what_was_done() {
        let d = v(&done_json(&plan(DeployAction::Deploy("该机无后端".into()))));
        assert_eq!(d["done"], "deploy");
        assert!(
            copy_core::copy_matches("rsSftp.deploy.done", d["text"].as_str().unwrap()),
            "{d}"
        );
        assert!(d["text"].as_str().unwrap().contains("p9b-mine"));
        let s = v(&done_json(&plan(DeployAction::Skip)));
        assert_eq!(s["done"], "skip");
        assert!(copy_core::copy_matches(
            "rsSftp.deploy.upToDate",
            s["text"].as_str().unwrap()
        ));
        let k = v(&done_json(&plan(DeployAction::Keep {
            theirs: "p9c".into(),
            why: "那台更新".into(),
        })));
        assert_eq!(
            (k["done"].as_str(), k["text"].as_str()),
            (Some("keep"), Some("那台更新"))
        );
        let f = v(&failed_json("refused", "box 不装", Some("uname: x")));
        assert_eq!(f["failed"], "refused");
        assert!(f["detail"].as_str().unwrap().contains("uname: x"), "{f}");
    }

    /// Kotlin 交的那几格：认得的照收，缺格 · 空身份 · 表 A 外的机器丢掉。
    #[test]
    fn slots_keep_only_rows_that_name_a_machine_an_identity_and_a_file() {
        let got = slots_of(
            r#"[{"os":"Linux","arch":"x86_64","id":"p9b","path":"/a/x"},
                {"os":"Linux","arch":"aarch64","id":"","path":"/a/y"},
                {"os":"Plan9","arch":"x86_64","id":"p9b","path":"/a/z"},
                {"os":"Linux","arch":"aarch64","id":"p9b"}]"#,
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, std::path::PathBuf::from("/a/x"));
        assert!(slots_of("not json").is_empty());
    }

    /// 换不换常驻：放上了 ⇒ 换；已是这一版 ⇒ 没接着 · 接着的那一位旧才换；那台更新 ⇒ 不动。
    #[test]
    fn the_resident_is_replaced_only_when_it_runs_an_older_one() {
        assert!(restart_after(
            &plan(DeployAction::Deploy(String::new())),
            Some("p9b-mine")
        ));
        assert!(restart_after(&plan(DeployAction::Skip), None));
        assert!(restart_after(&plan(DeployAction::Skip), Some("p9a-old")));
        assert!(!restart_after(&plan(DeployAction::Skip), Some("p9b-mine")));
        assert!(!restart_after(
            &plan(DeployAction::Keep {
                theirs: "p9c".into(),
                why: String::new()
            }),
            None
        ));
    }
}
