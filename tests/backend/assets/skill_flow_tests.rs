//! 设计/96 §3.5：「写：一趟，不重算」·「`stale` ⇒ 停、说清停在哪 / 前面写了哪几个，**不重读**」—— skill 装 / 卸的写那一半进了被写那台（MIG-3a）。
use super::*;
use crate::assets::mcp_sync::There;
use crate::stream::inbound::LocalFiles;

struct NoFacts;
impl Facts for NoFacts {
    fn path(&self, _p: &str) -> There {
        There::Absent
    }
    fn command(&self, _name: &str) -> Option<bool> {
        None
    }
}

fn fixture(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("mig3a-skill-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("skills")).unwrap();
    d
}

fn golden() -> Value {
    serde_json::from_str(include_str!("../../__fixtures__/skill-flow.golden.json")).unwrap()
}

fn sub(v: &Value, root: &str) -> Value {
    serde_json::from_str(&v.to_string().replace("<ROOT>", root)).unwrap()
}

/// 跨语言金样：装两个（一个新建、一个说了盖）→ 记进装记录 → 卸一个 == 金样；码集合 == 登记表。
#[test]
fn the_skill_flow_matches_the_cross_language_golden() {
    let g = golden();
    let base = fixture("golden");
    let root = base.join("skills");
    let ledger = base.join("skill-installs.json");
    std::fs::create_dir_all(root.join("demo/lib")).unwrap();
    std::fs::write(root.join("demo/lib/a.txt"), "old a\n").unwrap();
    let record = |a: &Value| crate::assets::skill_ledger::record_at(&ledger, Some(&root), a);
    let plan = crate::assets::skill_install::answer_plan_with(
        &NoFacts,
        Some(&root),
        &json!({ "name": "demo", "source": g["source"] }),
    )
    .unwrap();
    let installed = answer_install(
        &LocalFiles,
        &NoFacts,
        Some(&root),
        &record,
        &json!({ "name": "demo", "source": g["source"], "target": plan["target"], "take": g["take"], "overwrite": g["overwrite"] }),
    )
    .expect("装不上");
    let r = root.display().to_string();
    assert_eq!(installed, sub(&g["installReply"], &r));
    assert_eq!(
        std::fs::read_to_string(root.join("demo/lib/a.txt")).unwrap(),
        "new a\n"
    );
    let seen = crate::assets::skill_install::answer_uninstall_plan_at(
        &ledger,
        &json!({ "dir": root.join("demo").display().to_string() }),
    )
    .unwrap();
    let removed = answer_uninstall(
        &LocalFiles,
        Some(&ledger),
        &record,
        &json!({ "dir": root.join("demo").display().to_string(), "seen": seen["seen"], "take": g["uninstallTake"] }),
    )
    .expect("卸不掉");
    assert_eq!(removed, sub(&g["uninstallReply"], &r));
    assert!(!root.join("demo/SKILL.md").exists() && root.join("demo/lib/a.txt").exists());
    let _ = std::fs::remove_dir_all(&base);
    for (name, want) in g["codes"].as_object().unwrap() {
        let spec = crate::stream::inbound::REGISTRY
            .iter()
            .find(|s| s.name == name)
            .expect("登记表里没有");
        let mut codes: Vec<&str> = spec.codes.to_vec();
        codes.sort_unstable();
        let want: Vec<&str> = want
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        assert_eq!(codes, want, "`{name}` 的拒绝码与金样不相等");
    }
}

/// 看过之后那一份变了 ⇒ `stale`、停在那一个，说清前面写了哪几个，不重读重算；写了的照记。
#[test]
fn a_file_changed_after_the_preview_stops_the_install_there() {
    let g = golden();
    let base = fixture("stale");
    let root = base.join("skills");
    let ledger = base.join("skill-installs.json");
    std::fs::create_dir_all(root.join("demo/lib")).unwrap();
    std::fs::write(root.join("demo/lib/a.txt"), "changed since\n").unwrap();
    let record = |a: &Value| crate::assets::skill_ledger::record_at(&ledger, Some(&root), a);
    let (code, why) = answer_install(
        &LocalFiles,
        &NoFacts,
        Some(&root),
        &record,
        &json!({ "name": "demo", "source": g["source"], "target": [{ "path": "lib/a.txt", "text": "old a\n" }], "take": g["take"], "overwrite": g["overwrite"] }),
    )
    .expect_err("看过之后变了还写了");
    assert_eq!(code, "stale");
    assert!(why.contains("SKILL.md"), "没说前面写了哪几个：{why}");
    assert_eq!(
        std::fs::read_to_string(root.join("demo/lib/a.txt")).unwrap(),
        "changed since\n"
    );
    let installs = crate::assets::skill_install::answer_installs_at(&ledger).unwrap();
    assert_eq!(
        installs["installs"][0]["files"],
        json!(1),
        "写了的那一个没记：{installs}"
    );
    let _ = std::fs::remove_dir_all(&base);
}
