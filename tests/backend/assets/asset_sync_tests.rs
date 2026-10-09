//! `asset_sync.rs` 的判据：一趟「拉 · 并 · 推」做对了、只推对面缺的、变了才扇出。
//!
//! 守的要求：用户原话「本机后端在本机看见一个skill并记录下来, 就会和远端后端同步」
//! ＋「目录自动同步，装要你点」· 「观测方沿它本来就拥有的那条连接去拉被观测方」（零新通道）。
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
//! - 🔴 真 Windows 远端：不在承诺面。

use super::*;
// 这三样原先由 `asset_sync.rs` 顺带引入（`Remote` trait 的签名要它们）；那一跳搬去 `remote_ask` 之后测试自己引。
use crate::assets::asset_catalog::{self as cat, Asset, KIND_SKILL};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-assetsync-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

fn skills(names: &[&str]) -> cat::Scanned {
    cat::Scanned {
        assets: names
            .iter()
            .map(|n| Asset {
                kind: KIND_SKILL.into(),
                name: (*n).into(),
                project: None,
                dir: None,
                digest: format!("d-{n}"),
                summary: json!({}),
            })
            .collect(),
        ..cat::Scanned::default()
    }
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

/// 替身对面：按拨号请求里的 `host` 找到那台，照命令跑同一个写口。记下它收过的每一条命令（连同写进 stdin 的那一行）。
struct FakeRemotes {
    by_host: BTreeMap<String, Machine>,
    seen: Mutex<Vec<(String, String)>>,
    stdins: Mutex<Vec<Option<String>>>,
}

impl Remote for FakeRemotes {
    fn run<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            let host = dial["machine"]["host"].as_str().unwrap().to_string();
            self.seen
                .lock()
                .unwrap()
                .push((host.clone(), command.clone()));
            self.stdins.lock().unwrap().push(stdin.clone());
            let m = self.by_host.get(&host).ok_or("没这台")?;
            if command == pull_command() {
                assert_eq!(stdin, None, "拉那一趟不写 stdin");
                return m.update(None).map(|v| v.to_string()).map_err(|e| e.1);
            }
            // 推那一趟：命令行逐字 == `<落点> --assets-catalog-merge --stdin-line`（不含载荷），载荷恰好一行进 stdin。
            if command != push_command() {
                return Err(format!("认不出的命令：{command}"));
            }
            let line = stdin.ok_or("推那一趟没写 stdin")?;
            let payload = line
                .strip_suffix('\n')
                .ok_or("stdin 那一行没有换行收尾（对面只读一行，等不到换行就一直等）")?;
            assert!(!payload.contains('\n'), "载荷不止一行 ⇒ 对面只读得到第一行");
            let v: Value = serde_json::from_str(payload).map_err(|e| e.to_string())?;
            m.update(Some(&v["catalog"]))
                .map(|v| v.to_string())
                .map_err(|e| e.1)
        })
    }
}

fn dial(host: &str) -> Value {
    json!({ "machine": { "host": host, "port": 22, "user": "u" } })
}

fn sync_args(host: &str) -> Value {
    json!({ "origin": format!("o-{host}"), "dial": dial(host) })
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
        stdins: Mutex::new(vec![]),
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
    assert_eq!(seen[0].1, pull_command());

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
        stdins: Mutex::new(vec![]),
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
        stdins: Mutex::new(vec![]),
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
        stdins: Mutex::new(vec![]),
    };
    let table = Table::default();
    for bad in [json!({"origin": ""}), json!({"dial": dial("x")})] {
        let e = answer_with(&bad, local.fold(), &fakes, &table)
            .await
            .expect_err("收下了");
        assert_eq!(e.0, "bad_args", "{bad}");
    }
    // 只给 `origin`（界面直问）而可达表里没有那一台 ⇒ 明说够不到，一次都不拨。
    let e = answer_with(&json!({"origin": "o"}), local.fold(), &fakes, &table)
        .await
        .expect_err("可达表里没有也收了");
    assert_eq!(e.0, "unreachable");
    assert!(fakes.seen.lock().unwrap().is_empty());
    // 正控：握手那一刻登记过（`remote-reach`）⇒ 只给 `origin` 就对那一台做一趟。
    crate::dial::remote_ask::register(&table, &json!({"origin": "o", "dial": dial("x")})).unwrap();
    let ok = answer_with(&json!({"origin": "o"}), local.fold(), &fakes, &table)
        .await
        .expect("登记过的也拒了");
    // 跨语言金样（「成品的两侧对拍」）：形状 == `assets-sync.golden.json`（id 与那一句错换成占位）；界面读同一份。
    let g: Value =
        serde_json::from_str(include_str!("../../__fixtures__/assets-sync.golden.json")).unwrap();
    let mut shape = ok.clone();
    shape["self"] = json!("<SELF>");
    shape["synced"][0]["error"] = json!("<ERR>");
    assert_eq!(shape, g["reply"], "`assets-sync` 成品与金样不相等：{ok}");
    fakes.seen.lock().unwrap().clear();
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

/// 推那一趟的**命令行里没有载荷**：不论载荷里有什么（单引号 · 反斜杠 · `$` · 反引号 · 中文），
/// 命令行逐字 == `command_line(后端, [--assets-catalog-merge, --stdin-line])`；载荷只经 stdin 走、恰好一行。
///
/// 要求：「远端命令走 POSIX shell 管道：与 monitor 起远端后端同一个假设；远端登录 shell 是 fish 之类就不成立。
/// 根治要给 CLI 面一个『只读一行 stdin』的入口」。此前的 `printf '%s\n' '<json>' | …` 把载荷过 POSIX 单引号拼进命令行 ——
/// fish 的单引号里 `\\` 与 `\'` 是转义，JSON 里的反斜杠会被吃掉一个；这一形已退役，本条钉它不回来。
#[test]
fn the_push_command_line_carries_no_payload_and_the_payload_rides_stdin_as_one_line() {
    let nasty =
        "{\"catalog\":{\"machines\":[{\"label\":\"it's \\\"x\\\" — 中文 $HOME `id` \\\\n\"}]}}";
    let cmd = push_command();
    assert_eq!(
        cmd,
        crate::dial::remote_ask::command_line(&[PUSH_FLAG, crate::STDIN_LINE_FLAG]),
        "推那一趟的命令行不是「后端路径 ＋ 两个旗标」"
    );
    // 命令行以固定落点打头（它自己带 `"$HOME"`）⇒ 查的是落点之后那一段。
    let tail = cmd
        .strip_prefix(relay_route_core::BACKEND_LANDING_SHELL)
        .expect("推那一趟的命令行不以后端落点打头");
    for frag in ["printf", "|", "machines", "中文", "$HOME"] {
        assert!(
            !tail.contains(frag),
            "命令行里出现了 `{frag}` —— 载荷（或管道）又回到命令行里了：{cmd}"
        );
    }
    let line = push_stdin(nasty);
    assert_eq!(
        line,
        format!("{nasty}\n"),
        "stdin 那一行 == 载荷原样 ＋ 一个换行"
    );
    assert_eq!(
        line.matches('\n').count(),
        1,
        "stdin 不是恰好一行 ⇒ 对面「只读一行」读不全"
    );
    // 命令行本身交给真 `sh` 也跑得通（后端换成 `echo`，印出来的就是那两个旗标）。
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(push_command().replacen(relay_route_core::BACKEND_LANDING_SHELL, "echo", 1))
        .output()
        .expect("起 sh");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        // 打头的 `--` 让那台的 `ccm` 当后端用（替身 `echo` 把它原样印出来）。
        format!("-- {PUSH_FLAG} {}\n", crate::STDIN_LINE_FLAG)
    );
    assert_eq!(
        pull_command(),
        "\"$HOME\"/.cc-monitor/bin/ccm -- '--assets-catalog'"
    );
}

/// 一块载荷（连外面那层 `{"catalog":{"machines":[…]}}` 与换行）装得进远端 CLI 面 stdin 的上限 —— 否则切出来的块对面整块拒。
#[test]
fn one_push_chunk_fits_under_the_remote_cli_stdin_cap() {
    let overhead = "{\"catalog\":{\"machines\":[]}}\n".len() as u64;
    assert!(
        PUSH_MAX_BYTES as u64 + overhead <= crate::control::cli_args::MAX_CLI_STDIN,
        "PUSH_MAX_BYTES（{PUSH_MAX_BYTES}）＋ 外层 {overhead} 字节 > 远端 CLI 面 stdin 上限 —— 切块上限要跟着它"
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
        let spec = crate::stream::inbound::REGISTRY
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
