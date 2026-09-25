//! 〔AS2 · 第四波 4B〕`asset_sync.rs` 的判据：一趟「拉 · 并 · 推」做对了、只推对面缺的、变了才扇出。
//!
//! 守的要求（住址）：用户裁决 **V113** 逐字「本机后端在本机看见一个skill并记录下来, 就会和远端后端同步」
//! ＋「目录自动同步，装要你点」· `设计/01 §3.5`「观测方沿它本来就拥有的那条连接去拉被观测方」（零新通道）。
//!
//! # 买到的
//!
//! - 两台各自真目录文件（临时目录）上跑完整一趟：本机与对面并完之后**机器集合与各台代数两向相等**；推的恰好是对面缺 / 旧的那几台。
//! - 第二趟零移动（幂等）；本机因一趟变了 ⇒ 对可达表里其余每台各一趟（恰好一层）。
//! - 推的命令形状只此一处拼；**真 `sh`** 把管进去的载荷原样读回（带单引号 / 换行 / 非 ASCII —— 异源：真 shell 对我们的引号）。
//! - 切块：每块不超上限、单台超了就说出来不推（纯函数，逐格相等）。
//!
//! # 买不到的
//!
//! - 🔴 **真远端**：对面是替身（解析命令、在另一个临时目录上跑同一个写口）；`DialRemote` 那条 capture 没对真 sshd 跑过。
//! - 远端登录 shell 不是 POSIX（fish 之类）⇒ 管道 ＋ 单引号那一形不成立 —— 与 monitor 起远端后端那条命令同一个假设。
//! - 🔴 真 Windows 远端：不在承诺面（`01 §6.7a`）。

use super::*;
use crate::asset_catalog::{self as cat, Asset, KIND_SKILL};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-assetsync-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

fn skills(names: &[&str]) -> (Vec<Asset>, Vec<String>) {
    (
        names
            .iter()
            .map(|n| Asset {
                kind: KIND_SKILL.into(),
                name: (*n).into(),
                project: None,
                digest: format!("d-{n}"),
                summary: json!({}),
            })
            .collect(),
        vec![],
    )
}

/// 一台机器 = 一份目录文件 ＋ 一份固定的「现扫」结果。
#[derive(Clone)]
struct Machine {
    path: PathBuf,
    scan: Vec<&'static str>,
    label: &'static str,
}

impl Machine {
    fn new(dir: &Path, name: &'static str, scan: Vec<&'static str>) -> Self {
        Machine {
            path: dir.join(name).join(cat::FILE_NAME),
            scan,
            label: name,
        }
    }
    fn update(&self, incoming: Option<&Value>) -> Result<Value, (&'static str, String)> {
        let inc = incoming.map(|c| cat::machines_from_wire(c).expect("载荷形状"));
        cat::update_at(&self.path, skills(&self.scan), self.label, inc)
    }
    fn fold(&self) -> Fold {
        let me = self.clone();
        Arc::new(move |args: &Value| me.update(Some(&args["catalog"])))
    }
    fn gens(&self) -> BTreeMap<String, u64> {
        let v = self.update(None).unwrap();
        v["machines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                (
                    m["id"].as_str().unwrap().to_string(),
                    m["gen"].as_u64().unwrap(),
                )
            })
            .collect()
    }
}

/// POSIX 单引号词的反解（替身对面用；真 shell 那一向由 `a_real_posix_shell…` 验）。
fn unquote_word(s: &str) -> (String, &str) {
    let mut out = String::new();
    let mut rest = s;
    loop {
        if let Some(r) = rest.strip_prefix("\\'") {
            out.push('\'');
            rest = r;
            continue;
        }
        let Some(r) = rest.strip_prefix('\'') else {
            return (out, rest);
        };
        let end = r.find('\'').expect("引号没闭合");
        out.push_str(&r[..end]);
        rest = &r[end + 1..];
    }
}

/// 替身对面：按拨号请求里的 `host` 找到那台，照命令跑同一个写口。记下它收过的每一条命令。
struct FakeRemotes {
    by_host: BTreeMap<String, Machine>,
    seen: Mutex<Vec<(String, String)>>,
}

impl Remote for FakeRemotes {
    fn run<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            let host = dial["host"].as_str().unwrap().to_string();
            self.seen
                .lock()
                .unwrap()
                .push((host.clone(), command.clone()));
            let m = self.by_host.get(&host).ok_or("没这台")?;
            let backend = "/opt/cc b/ccm";
            if command == pull_command(backend) {
                return m.update(None).map(|v| v.to_string()).map_err(|e| e.1);
            }
            let rest = command
                .strip_prefix("printf '%s\\n' ")
                .ok_or(format!("认不出的命令：{command}"))?;
            let (payload, tail) = unquote_word(rest);
            assert_eq!(
                tail,
                format!(" | {}", pull_command(backend).replace(PULL_FLAG, PUSH_FLAG))
            );
            let v: Value = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
            m.update(Some(&v["catalog"]))
                .map(|v| v.to_string())
                .map_err(|e| e.1)
        })
    }
}

fn dial(host: &str) -> Value {
    json!({ "host": host, "port": 22, "user": "u" })
}

fn sync_args(host: &str) -> Value {
    json!({ "origin": format!("o-{host}"), "dial": dial(host), "backend": "/opt/cc b/ccm" })
}

#[tokio::test]
async fn one_sync_pulls_merges_and_pushes_exactly_what_the_other_side_lacks() {
    let d = temp_dir("one");
    let local = Machine::new(&d, "local", vec!["a"]);
    let remote = Machine::new(&d, "remote", vec!["b"]);
    let lid = local.update(None).unwrap()["self"]
        .as_str()
        .unwrap()
        .to_string();
    let rid = remote.update(None).unwrap()["self"]
        .as_str()
        .unwrap()
        .to_string();
    // 本机早知道 X@3；对面知道 X@5 与 Y@1。
    let snap = |id: &str, gen: u64| json!({"machines":[{"id":id,"label":id,"gen":gen,"seenAt":0,"assets":[]}]});
    local.update(Some(&snap("X", 3))).unwrap();
    remote.update(Some(&snap("X", 5))).unwrap();
    remote.update(Some(&snap("Y", 1))).unwrap();

    let fakes = FakeRemotes {
        by_host: [("r".to_string(), remote.clone())].into(),
        seen: Mutex::new(vec![]),
    };
    let table = Table::default();
    let out = answer_with(&sync_args("r"), local.fold(), &fakes, &table)
        .await
        .expect("一趟");
    let row = &out["synced"][0];
    assert_eq!(out["self"], json!(lid), "回本机目录的 id");
    assert_eq!(row["error"], Value::Null, "{out}");
    assert_eq!(row["peer"], json!(rid));
    assert_eq!(row["changed"], json!(true));
    assert_eq!(
        row["pushed"],
        json!(1),
        "只该推本机自己那一格（X 对面更新、Y 本来就是对面给的）：{out}"
    );

    let want: BTreeMap<String, u64> = [
        (lid.clone(), 1),
        (rid.clone(), 1),
        ("X".into(), 5),
        ("Y".into(), 1),
    ]
    .into();
    assert_eq!(local.gens(), want, "本机并完");
    assert_eq!(remote.gens(), want, "对面并完");
    assert_eq!(
        out["reach"],
        json!([{ "origin": "o-r", "machine": rid }]),
        "可达表记下了对面的 id"
    );
    // 恰好两条命令：一拉、一推
    let seen = fakes.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert_eq!(seen[0].1, pull_command("/opt/cc b/ccm"));

    // 第二趟：零移动
    let again = answer_with(&sync_args("r"), local.fold(), &fakes, &table)
        .await
        .unwrap();
    assert_eq!(again["synced"][0]["changed"], json!(false));
    assert_eq!(again["synced"][0]["pushed"], json!(0));
    assert_eq!(local.gens(), want);
    let _ = std::fs::remove_dir_all(&d);
}

#[tokio::test]
async fn a_change_fans_out_once_to_every_other_reachable_machine() {
    let d = temp_dir("fan");
    let local = Machine::new(&d, "local", vec!["a"]);
    let r1 = Machine::new(&d, "r1", vec!["one"]);
    let r2 = Machine::new(&d, "r2", vec!["two"]);
    let id1 = r1.update(None).unwrap()["self"]
        .as_str()
        .unwrap()
        .to_string();
    let fakes = FakeRemotes {
        by_host: [
            ("r1".to_string(), r1.clone()),
            ("r2".to_string(), r2.clone()),
        ]
        .into(),
        seen: Mutex::new(vec![]),
    };
    let table = Table::default();
    answer_with(&sync_args("r2"), local.fold(), &fakes, &table)
        .await
        .unwrap();
    fakes.seen.lock().unwrap().clear();
    // r1 连上：带来新东西 ⇒ 本机变了 ⇒ r2 也要拿到 r1 那一格
    let out = answer_with(&sync_args("r1"), local.fold(), &fakes, &table)
        .await
        .unwrap();
    let origins: Vec<&str> = out["synced"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["origin"].as_str().unwrap())
        .collect();
    assert_eq!(
        origins,
        vec!["o-r1", "o-r2"],
        "先连上的那台，再恰好一层扇出"
    );
    assert!(r2.gens().contains_key(&id1), "r2 没拿到 r1 那一格");
    // 不变就不扇出
    fakes.seen.lock().unwrap().clear();
    let quiet = answer_with(&sync_args("r1"), local.fold(), &fakes, &table)
        .await
        .unwrap();
    assert_eq!(
        quiet["synced"].as_array().unwrap().len(),
        1,
        "没变却扇出了：{quiet}"
    );
    // 本机自己那份变了（刚装了一个 skill）⇒ 哪怕这一台没带来新东西，也要扇出到其余每一台
    let changed_local = Machine {
        scan: vec!["a", "b"],
        ..local.clone()
    };
    let fan = answer_with(&sync_args("r1"), changed_local.fold(), &fakes, &table)
        .await
        .unwrap();
    assert_eq!(
        fan["synced"].as_array().unwrap().len(),
        2,
        "本机变了却没扇出：{fan}"
    );
    // 什么都没给 ⇒ 表里每台各一趟
    let all = answer_with(&json!({}), local.fold(), &fakes, &table)
        .await
        .unwrap();
    assert_eq!(all["synced"].as_array().unwrap().len(), 2);
    let _ = std::fs::remove_dir_all(&d);
}

#[tokio::test]
async fn a_failing_remote_is_said_and_nothing_is_pushed_to_it() {
    let d = temp_dir("fail");
    let local = Machine::new(&d, "local", vec!["a"]);
    let fakes = FakeRemotes {
        by_host: BTreeMap::new(),
        seen: Mutex::new(vec![]),
    };
    let table = Table::default();
    let out = answer_with(&sync_args("gone"), local.fold(), &fakes, &table)
        .await
        .unwrap();
    assert_eq!(out["synced"][0]["error"], json!("没这台"));
    assert_eq!(out["synced"][0]["pushed"], json!(0));
    assert_eq!(fakes.seen.lock().unwrap().len(), 1, "拉失败了不许再推");
    let _ = std::fs::remove_dir_all(&d);
}

#[tokio::test]
async fn half_given_arguments_are_refused() {
    let d = temp_dir("args");
    let local = Machine::new(&d, "local", vec![]);
    let fakes = FakeRemotes {
        by_host: BTreeMap::new(),
        seen: Mutex::new(vec![]),
    };
    let table = Table::default();
    for bad in [
        json!({"origin": ""}),
        json!({"origin": "o"}),
        json!({"origin": "o", "dial": dial("x")}),
        json!({"origin": "o", "backend": "/b"}),
        json!({"dial": dial("x"), "backend": "/b"}),
    ] {
        let e = answer_with(&bad, local.fold(), &fakes, &table)
            .await
            .expect_err("收下了");
        assert_eq!(e.0, "bad_args", "{bad}");
    }
    assert!(fakes.seen.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn push_plan_chunks_under_the_cap_and_refuses_a_single_oversized_machine() {
    let m = |id: &str, gen: u64, pad: usize| json!({"id": id, "label": "x".repeat(pad), "gen": gen, "seenAt": 0, "assets": []});
    let third = PUSH_MAX_BYTES / 3;
    let mine = json!({"self": "me", "machines": [
        m("me", 2, third), m("a", 1, third), m("b", 1, third), m("c", 4, 10),
        m("big", 1, PUSH_MAX_BYTES + 1), m("them", 9, 10), m("old", 1, 10),
    ]});
    let theirs = json!({"self": "them", "machines": [ m("c", 4, 10), m("old", 3, 10) ]});
    let (chunks, too_big) = push_plan(&mine, &theirs);
    let ids: Vec<Vec<&str>> = chunks
        .iter()
        .map(|c| c.iter().map(|x| x["id"].as_str().unwrap()).collect())
        .collect();
    assert_eq!(
        ids,
        vec![vec!["me", "a"], vec!["b"]],
        "对面自己 / 对面不旧的不推，按上限切块"
    );
    for c in &chunks {
        let len: usize = c.iter().map(|x| x.to_string().len() + 1).sum();
        assert!(len <= PUSH_MAX_BYTES + 1, "一块超了上限：{len}");
    }
    assert_eq!(too_big.len(), 1);
    assert!(too_big[0].contains("big"));
}

/// 推的那一形（`printf … | 后端 --assets-catalog-merge`）交给**真 `sh`**：管进去的字节与载荷逐字相等。
#[test]
fn a_real_posix_shell_reads_the_piped_payload_back_verbatim() {
    let payload =
        "{\"catalog\":{\"machines\":[{\"label\":\"it's \\\"x\\\" — 中文 $HOME `id` \\\\n\"}]}}";
    let cmd = push_command("cat", payload);
    assert_eq!(
        cmd,
        format!(
            "printf '%s\\n' {} | 'cat' {PUSH_FLAG}",
            shell_quote_core::posix_quote(payload)
        )
    );
    // `cat --assets-catalog-merge` 不是我们要的；换成真 cat 读 stdin，只验管道 ＋ 引号那一半。
    let probe = cmd.replace(&format!(" {PUSH_FLAG}"), "");
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(&probe)
        .output()
        .expect("起 sh");
    assert!(out.status.success(), "{:?}", out);
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("{payload}\n")
    );
    assert_eq!(
        pull_command("/opt/a'b/ccm"),
        "'/opt/a'\\''b/ccm' --assets-catalog"
    );
}

/// 两条一次性子命令与后端真登记的同名（跨半边：这里的常量 ↔ `lib.rs::SUBCOMMANDS` ↔ `inbound::REGISTRY`）。
#[test]
fn the_remote_flags_are_real_subcommands_of_the_catalog_commands() {
    for (flag, cmd) in [
        (PULL_FLAG, "assets-catalog"),
        (PUSH_FLAG, "assets-catalog-merge"),
    ] {
        assert!(
            crate::SUBCOMMANDS.contains(&flag),
            "{flag} 不在 SUBCOMMANDS"
        );
        assert_eq!(format!("--{cmd}"), flag);
        let spec = crate::inbound::REGISTRY
            .iter()
            .find(|s| s.name == cmd)
            .unwrap_or_else(|| panic!("{cmd} 不在 REGISTRY"));
        assert_eq!(
            spec.takes_input,
            flag == PUSH_FLAG,
            "{cmd} 收不收 stdin 与推 / 拉那一形对不上"
        );
    }
}
