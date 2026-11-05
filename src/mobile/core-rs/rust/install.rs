//! **手机给一台装 / 升级后端**的宿主：判定与 ①–④ 是 `deploy` crate 那一份（桌面后端答 `deploy-install` 用的同一份），
//! ⑤ 换常驻是 `resident-client` 那一条（`Feed::replace`：`--resident-ensure --replace`，换不换那台自己判，换了就整份重接）。这里只接线、写话。
//!
//! 一个入口（`Core.deploy(handle, slots)`）。走到哪一步收进那台的状态成品（`deploy::machine`，同桌面那一份），经 `state` 那一行交 Kotlin
//! （正在装 / 更新那一句与进度条照成品画）；结局经常驻流同一个口交（种类 `deploy`，一行 JSON）：
//! - `{"end": "done", "act": "skip" | "deploy" | "keep", "text"}`：做完了（已是这一版 · 装上了这一版 · 那台不比这一版旧、没动它）；
//! - `{"end": "failed", "code", "said", "detail"}`：没做成（交出去的那一形失败：码 · 那一句 · 复制详情）。
//! 形状住契约（`cmd_wire::phone::DeployEnd`），Kotlin 读同一份生成的类。
//!
//! 字节：App 把包里那两格解到自己的私有目录（每版一次），交路径；这里从文件读，不经 JNI 传数组。

use std::sync::{Arc, Mutex, OnceLock};

use cmd_wire::phone::{DeployAct, DeployEnd};
use copy_core::copy_text;
use copy_core::said::Said;
use deploy_contract::DeployAction;
use relay_route_core::{Cap, Over, Regular};

use crate::resident::{self, OnEvent};

/// 种类：交 Kotlin 时用的那个词。
pub const KIND: &str = "deploy";

/// 正在装的那几台（一台同时只装一趟）。
fn busy() -> &'static Mutex<std::collections::HashSet<i64>> {
    static B: OnceLock<Mutex<std::collections::HashSet<i64>>> = OnceLock::new();
    B.get_or_init(Default::default)
}

/// 做完了 ⇒ 那一行（那一句是 `deploy` crate 写的）。
pub fn done_json(plan: &deploy::Plan) -> String {
    let act = match &plan.action {
        DeployAction::Skip => DeployAct::Skip,
        DeployAction::Deploy(_) => DeployAct::Deploy,
        DeployAction::Keep { .. } => DeployAct::Keep,
    };
    serde_json::to_string(&DeployEnd::Done {
        act,
        text: plan.said(),
    })
    .unwrap_or_default()
}

/// 没做成 ⇒ 那一行（详情：命令 · 码 · 原话，钟是这台手机的）。
pub fn failed_json(code: &str, said: &str, raw: Option<&str>) -> String {
    let detail = ssh_dial::env::get().facts.detail("deploy", Some(code), raw);
    serde_json::to_string(&DeployEnd::Failed(cmd_wire::failed::Failed {
        code: code.to_string(),
        said: said.to_string(),
        detail,
        data: None,
    }))
    .unwrap_or_default()
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

/// 整份读本版带的那份后端的上限（同后端整读后端字节那一个数：今天一份二十来 MB，留足十倍余量）。
/// 超了 ⇒ 拒收：一个字节不读进来，回超出上限那一句，这一趟不装、那台一个字节不动。
const BACKEND_BYTES_CAP: Cap = Cap {
    bytes: 256 << 20,
    over: Over::Reject,
};

/// 读一格字节（App 解到私有目录的那份后端）。超上限 ⇒ 拒收，那一句就是部署那一格的原因。
fn load(p: &std::path::Path) -> Result<Vec<u8>, Said> {
    read_capped(p, BACKEND_BYTES_CAP)
}

/// 带上限地整份读一份本机文件（跟链接），经 `relay_route_core::read_capped_regular`：只认普通文件、长度已过上限 ⇒ 不读。
/// 读不了的那一句只是原因，系统原话进 `raw`。
fn read_capped(p: &std::path::Path, cap: Cap) -> Result<Vec<u8>, Said> {
    let io = |e: std::io::Error| Said::with_raw(copy_core::io_reason(e.kind()), e);
    let meta = std::fs::metadata(p).map_err(io)?;
    let c = cap.bytes.to_string();
    match relay_route_core::read_capped_regular(&meta, || std::fs::File::open(p), cap)
        .map_err(io)?
    {
        Regular::Whole(buf) => Ok(buf),
        Regular::NotRegular => Err(Said::from(copy_text("reason.fs.notRegular", &[]))),
        Regular::OverCap(Some(size)) => Err(Said::from(copy_text(
            "beFs.readRegularCapped.tooBigSized",
            &[("cap", &c), ("size", &size.to_string())],
        ))),
        Regular::OverCap(None) => Err(Said::from(copy_text(
            "beFs.readRegularCapped.tooBig",
            &[("cap", &c)],
        ))),
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
                &comms_inward::chan::said::offline(&machine),
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
            let text = copy_text("reason.link.lost", &[]);
            return emit(failed_json("bad_request", &text, Some(&e.to_string())));
        }
    };
    // 一次性命令走常驻流那条链路的 capture（同一条池连接）。
    let link = Arc::new(resident::PhoneLink::new(base));
    let exec: deploy::Exec = Box::new(move |command| {
        let link = Arc::clone(&link);
        Box::pin(async move {
            use resident_client::feed::Transport as _;
            link.capture(command).await.map_err(|p| p.said)
        })
    });
    let facing = deploy::DialFacing::new(req, exec);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let step = |s: deploy::Step| {
        for (kind, body) in resident::note(h, |f| f.step(s, machine)) {
            on(h, kind, body);
        }
    };
    // 结局那一行 ＋ 没做成那一句（记进那台的成品，机器条照它说）。
    let (ended, failed) = match deploy::install(&facing, &slots, &load, machine, now, &step).await {
        Err(f) => (
            failed_json(f.code_or_fallback(), &f.said, f.raw.as_deref()),
            Some(f.said),
        ),
        Ok(plan) => {
            // 换不换常驻由那台判（`--resident-ensure --replace`：在听的比落点旧才换）。
            step(deploy::Step::Restart);
            match feed.replace().await {
                Ok(_) => (done_json(&plan), None),
                Err(p) => {
                    let raw = (!p.detail.is_empty()).then_some(p.detail.as_str());
                    (failed_json("restart_failed", &p.said, raw), Some(p.said))
                }
            }
        }
    };
    for (kind, body) in resident::note(h, |f| f.settle(machine, failed.as_deref())) {
        on(h, kind, body);
    }
    emit(ended);
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
        }
    }

    fn v(s: &str) -> serde_json::Value {
        serde_json::from_str(s).expect("JSON")
    }

    /// 结局三种各一句：装上了（带这一版与原因）· 已是这一版 · 那台更新（计划里那一句，不叫人去别处卸载）。
    #[test]
    fn the_outcome_names_what_was_done() {
        let d = v(&done_json(&plan(DeployAction::Deploy("该机无后端".into()))));
        assert_eq!(
            (d["end"].as_str(), d["act"].as_str()),
            (Some("done"), Some("deploy"))
        );
        assert!(
            copy_core::copy_matches("rsSftp.deploy.done", d["text"].as_str().unwrap()),
            "{d}"
        );
        assert!(d["text"].as_str().unwrap().contains("p9b-mine"));
        let s = v(&done_json(&plan(DeployAction::Skip)));
        assert_eq!(s["act"], "skip");
        assert!(copy_core::copy_matches(
            "rsSftp.deploy.upToDate",
            s["text"].as_str().unwrap()
        ));
        let k = v(&done_json(&plan(DeployAction::Keep {
            theirs: "p9c".into(),
            why: "那台更新".into(),
        })));
        assert_eq!(
            (k["act"].as_str(), k["text"].as_str()),
            (Some("keep"), Some("那台更新"))
        );
        let f = v(&failed_json("refused", "box 不装", Some("uname: x")));
        assert_eq!(
            (f["end"].as_str(), f["code"].as_str()),
            (Some("failed"), Some("refused"))
        );
        assert!(f["detail"].as_str().unwrap().contains("uname: x"), "{f}");
    }

    /// 本版带的那份后端比上限大一个字节 ⇒ 拒收，那一句是超出上限（带实际大小），一个字节不读进来。
    #[test]
    fn a_carried_backend_one_byte_over_the_cap_is_refused_as_over_the_cap() {
        let d = test_home::scratch("over");
        let p = d.join("backend");
        let f = std::fs::File::create(&p).expect("建假后端");
        f.set_len(BACKEND_BYTES_CAP.bytes + 1)
            .expect("稀疏扩到上限加一");
        let said = match load(&p) {
            Ok(b) => panic!("超上限却读进来了 {} 字节", b.len()),
            Err(s) => s.said,
        };
        let cap = BACKEND_BYTES_CAP.bytes.to_string();
        let size = (BACKEND_BYTES_CAP.bytes + 1).to_string();
        assert_eq!(
            said,
            copy_text(
                "beFs.readRegularCapped.tooBigSized",
                &[("cap", &cap), ("size", &size)]
            )
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 正好等于上限的照读，多一个字节就拒。用小上限量，不写几百 MB。
    #[test]
    fn exactly_at_the_cap_is_read_one_byte_more_is_refused() {
        let d = test_home::scratch("edge");
        let p = d.join("backend");
        std::fs::write(&p, b"0123456789").expect("写假后端");
        let at = |bytes| Cap {
            bytes,
            over: Over::Reject,
        };
        assert_eq!(
            read_capped(&p, at(10)).expect("正好等于上限"),
            b"0123456789"
        );
        let said = read_capped(&p, at(9)).expect_err("超上限却读进来了").said;
        assert!(
            copy_core::copy_matches("beFs.readRegularCapped.tooBigSized", &said),
            "{said}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 不是普通文件（目录 · FIFO · 设备）⇒ 不打开、回「不是普通文件」。
    #[test]
    fn a_carried_backend_that_is_not_a_regular_file_is_refused() {
        let d = test_home::scratch("dir");
        let said = load(&d).expect_err("目录却读成了").said;
        assert_eq!(said, copy_text("reason.fs.notRegular", &[]));
        let _ = std::fs::remove_dir_all(&d);
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
}
