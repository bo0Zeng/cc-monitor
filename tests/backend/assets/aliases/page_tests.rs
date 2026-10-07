//! 设置窗那一页的五条命令（`profiles-read` · `-resolve` · `-impact` · `-bases` · `-write`）：
//! 合并只问 `profile::resolve`，界面拿到的是成品（每格标签 · 值的说法 · 来自哪一段 · 被谁盖掉 · 「等于」那一行）。

use super::*;
use crate::assets::aliases::tests::HomeDoor;
use std::path::PathBuf;

struct Tmp(PathBuf);
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tmp(tag: &str, profiles: Option<&str>) -> Tmp {
    let d = std::env::temp_dir().join(format!(
        "ccm-page-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|x| x.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(d.join(super::super::links::bin_rel())).unwrap();
    std::fs::write(d.join(super::super::links::bin_rel()).join("ccm"), "").unwrap();
    if let Some(t) = profiles {
        std::fs::write(d.join(relay_route_core::PROFILES_REL), t).unwrap();
    }
    Tmp(d)
}

impl Tmp {
    fn door(&self) -> HomeDoor {
        HomeDoor(self.0.clone())
    }
    fn text(&self) -> String {
        std::fs::read_to_string(self.0.join(relay_route_core::PROFILES_REL)).unwrap()
    }
}

const BOOK: &str = "\
# 我的配置
[cc]
cwd-if = [[\"~\", \"~/projects/notes\"]]

[cct]
from = \"cc\"   # tmux 版
ccm-tmux = true

[betacct]
from = \"cct\"
account = \"b\"

[alphacct]
from = \"cct\"
account = \"z\"

[pcc]
base = true
cwd = \"/srv/p\"
";

fn read(t: &Tmp) -> Value {
    answer_read(&t.door(), &json!({})).expect("profiles-read")
}

fn prof<'a>(r: &'a Value, name: &str) -> &'a Value {
    r["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("{name} 不在清单里"))
}

#[test]
fn read_gives_each_section_its_own_items_with_lines_and_the_shared_labels() {
    let t = tmp("read", Some(BOOK));
    let r = read(&t);
    assert_eq!(r["exists"], true);
    assert!(r["fingerprint"].is_string());
    assert!(r["modified"].is_u64(), "修改时间（秒）：{}", r["modified"]);
    let b = prof(&r, "betacct");
    assert_eq!(b["from"], "cct");
    assert_eq!(b["usable"], true);
    assert!(b["problem"].is_null());
    assert_eq!(b["own"][0]["key"], "account");
    assert_eq!(b["own"][0]["slot"], "account");
    assert_eq!(b["own"][0]["vals"], json!(["b"]));
    assert_eq!(b["own"][0]["line"], 11);
    // 树里那一行的摘要与表单、合并表同一套词：标签「账号」＋ 值。
    let label = copy_text("beProfile.slot.account", &[]);
    assert_eq!(
        b["said"],
        copy_text("beProfile.said.slot", &[("label", &label), ("val", "b")])
    );
    // 表单回填：自己写的那几格有值，没写的继承（null）。
    assert_eq!(b["form"]["account"], json!({"kind": "named", "name": "b"}));
    assert!(b["form"]["tmux"].is_null());
    let p = prof(&r, "pcc");
    assert_eq!(p["form"]["account"], json!({"kind": "base"}));
    assert_eq!(p["form"]["cwd"], "/srv/p");
}

#[test]
fn a_broken_section_and_those_based_on_it_are_unusable_with_the_line_others_stay_usable() {
    let broken = BOOK.replace("account = \"b\"", "tmux-sise = \"200x50\"");
    let t = tmp("broken", Some(&broken));
    let r = read(&t);
    let b = prof(&r, "betacct");
    assert_eq!(b["usable"], false);
    let said = b["problem"]["message"].as_str().unwrap();
    assert!(said.contains("tmux-sise"), "{said}");
    assert_eq!(b["problem"]["line"], 11);
    // 终端里敲 betacct 得到的是同一句（ccm 也走 resolve）。
    let book = profile::parse_book(&broken);
    let ccm_said = profile::resolve(&book, "betacct", &[]).unwrap_err();
    assert_eq!(said, ccm_said);
    assert_eq!(prof(&r, "alphacct")["usable"], true);
    assert!(r["fileProblem"].is_null());
}

#[test]
fn a_toml_syntax_error_is_one_file_problem_with_its_line_and_no_sections() {
    let t = tmp("syntax", Some("[cc]\ncwd = \"/x\"\n[cct\nfrom = \"cc\"\n"));
    let r = read(&t);
    assert_eq!(r["profiles"], json!([]));
    assert_eq!(r["fileProblem"]["line"], 3);
    assert!(r["fileProblem"]["message"].is_string());
}

#[test]
fn with_no_file_read_offers_the_first_two_as_a_seed_and_writes_nothing() {
    let t = tmp("empty", None);
    let r = read(&t);
    assert_eq!(r["exists"], false);
    assert!(r["fingerprint"].is_null());
    let names: Vec<&str> = r["seed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["cc", "cct"]);
    assert_eq!(r["seed"][1]["from"], "cc");
    assert!(!t.0.join(relay_route_core::PROFILES_REL).exists());
}

#[test]
fn a_name_that_clashes_with_a_program_on_path_is_a_function_and_says_why() {
    let t = tmp("kind", Some(BOOK));
    let r = read(&t);
    for p in r["profiles"].as_array().unwrap() {
        let name = p["name"].as_str().unwrap();
        let fun = super::super::wants_function(name, Shell::Posix);
        assert_eq!(p["kind"], if fun { "function" } else { "link" }, "{name}");
        assert_eq!(p["functionWhy"].is_string(), fun, "{name}");
    }
}

fn row<'a>(r: &'a Value, key: &str, from: &str) -> &'a Value {
    r["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["key"] == key && x["from"] == from)
        .unwrap_or_else(|| panic!("没有 {key} 来自 {from} 的那一行：{}", r["rows"]))
}

#[test]
fn resolve_lists_every_item_with_label_origin_and_the_overridden_ones_struck() {
    let book = BOOK.replace(
        "[betacct]\nfrom = \"cct\"\n",
        "[betacct]\nfrom = \"cct\"\nccm-tmux = \"work\"\n",
    );
    let t = tmp("resolve", Some(&book));
    let r = answer_resolve(&t.door(), &json!({"name": "betacct"})).unwrap();
    assert_eq!(r["chain"], json!(["cc", "cct", "betacct"]));
    assert!(row(&r, "cwd-if", "cc")["overriddenBy"].is_null());
    assert_eq!(row(&r, "ccm-tmux", "cct")["overriddenBy"], "betacct");
    assert!(row(&r, "ccm-tmux", "betacct")["overriddenBy"].is_null());
    let acct = row(&r, "account", "betacct");
    assert_eq!(acct["label"], copy_text("beProfile.slot.account", &[]));
    assert_eq!(acct["vals"], json!(["b"]));
    // 「等于」那一行是这台后端算的（同 `ccm @betacct -- --ccm-print`）：这台没有 b 号 ⇒ 不给行、给 ccm 那一句。
    assert!(r["line"].is_null());
    assert!(r["lineError"].as_str().unwrap().contains("'b'"), "{r}");
    let p = answer_resolve(&t.door(), &json!({"name": "pcc"})).unwrap();
    let line = p["line"].as_str().unwrap_or_else(|| panic!("{p}"));
    assert!(line.contains("/srv/p"), "{line}");
}

#[test]
fn resolve_takes_the_directory_you_pretend_to_type_in() {
    let t = tmp("at", Some(BOOK));
    let home = answer_resolve(&t.door(), &json!({"name": "cc", "at": "~"})).unwrap();
    let elsewhere = answer_resolve(&t.door(), &json!({"name": "cc", "at": "/tmp"})).unwrap();
    let (a, b) = (
        home["line"].as_str().unwrap(),
        elsewhere["line"].as_str().unwrap(),
    );
    assert!(a.contains("projects/notes"), "{a}");
    assert!(!b.contains("projects/notes"), "{b}");
}

#[test]
fn resolve_with_an_unsaved_form_answers_for_the_form_and_writes_nothing() {
    let t = tmp("edit", Some(BOOK));
    let before = t.text();
    let r = answer_resolve(
        &t.door(),
        &json!({"name": "bcct2", "at": "/tmp", "edit": {"name": "bcct2", "from": "cct", "account": {"kind": "base"}, "cwd": "/srv/q"}}),
    )
    .unwrap();
    assert_eq!(r["chain"], json!(["cc", "cct", "bcct2"]));
    assert_eq!(
        row(&r, "base", "bcct2")["said"],
        copy_text("beProfile.val.base", &[])
    );
    let line = r["line"].as_str().unwrap_or_else(|| panic!("{r}"));
    assert!(line.contains("/srv/q"), "{line}");
    assert_eq!(t.text(), before);
}

#[test]
fn impact_names_only_the_children_whose_merged_result_changes_with_before_and_after() {
    let t = tmp("impact", Some(BOOK));
    let r = answer_impact(
        &t.door(),
        &json!({"changes": [{"op": "set", "was": "cct", "form": {"name": "cct", "from": "cc", "tmux": {"mode": "named", "name": "work"}}}]}),
    )
    .unwrap();
    let names: Vec<&str> = r["affected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["betacct", "alphacct"]);
    let c = &r["affected"][0]["changes"][0];
    assert_eq!(c["slot"], "tmux");
    assert_eq!(c["before"], copy_text("beProfile.val.tmuxAuto", &[]));
    assert_eq!(
        c["after"],
        copy_text("beProfile.val.tmuxNamed", &[("name", "work")])
    );
}

#[test]
fn bases_lists_only_what_cannot_loop_and_greys_itself() {
    let t = tmp("bases", Some(BOOK));
    let r = answer_bases(&t.door(), &json!({"name": "cct"})).unwrap();
    let pick: Vec<(&str, bool)> = r["bases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["name"].as_str().unwrap(),
                b["selectable"].as_bool().unwrap(),
            )
        })
        .collect();
    // betacct / alphacct 基于 cct ⇒ 选了就成圈，不列；cct 自己灰着。
    assert_eq!(pick, [("cc", true), ("cct", false), ("pcc", true)]);
}

fn write(t: &Tmp, changes: Value) -> Result<Value, (&'static str, String)> {
    let fp = read(t)["fingerprint"].clone();
    answer_write(&t.door(), &json!({"changes": changes, "fingerprint": fp}))
}

#[test]
fn write_set_changes_only_that_section_and_keeps_hand_comments() {
    let t = tmp("set", Some(BOOK));
    write(
        &t,
        json!([{"op": "set", "was": "betacct", "form": {"name": "betacct", "from": "cct", "account": {"kind": "named", "name": "q"}}}]),
    )
    .unwrap();
    let now = t.text();
    assert!(
        now.contains("# 我的配置") && now.contains("# tmux 版"),
        "{now}"
    );
    assert_eq!(now, BOOK.replace("account = \"b\"", "account = \"q\""));
}

#[test]
fn write_rename_moves_the_section_in_place_and_children_follow() {
    let t = tmp("rename", Some(BOOK));
    write(
        &t,
        json!([{"op": "set", "was": "cct", "form": {"name": "tt", "from": "cc", "tmux": {"mode": "auto"}}}]),
    )
    .unwrap();
    let now = t.text();
    let book = profile::parse_book(&now);
    let order: Vec<&str> = book.profiles.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(order, ["cc", "tt", "betacct", "alphacct", "pcc"]);
    assert_eq!(book.find("betacct").unwrap().from.as_deref(), Some("tt"));
    assert!(now.contains("# tmux 版"), "{now}");
}

#[test]
fn removing_a_base_needs_a_choice_and_each_choice_does_what_it_says() {
    let t = tmp("remove", Some(BOOK));
    let (code, _) = write(&t, json!([{"op": "remove", "name": "cct"}])).unwrap_err();
    assert_eq!(code, "refused");
    assert_eq!(t.text(), BOOK);

    write(
        &t,
        json!([{"op": "remove", "name": "cct", "children": "reparent"}]),
    )
    .unwrap();
    let b = profile::parse_book(&t.text());
    assert!(b.find("cct").is_none());
    assert_eq!(b.find("betacct").unwrap().from.as_deref(), Some("cc"));

    let t = tmp("cascade", Some(BOOK));
    write(
        &t,
        json!([{"op": "remove", "name": "cct", "children": "cascade"}]),
    )
    .unwrap();
    let names: Vec<String> = profile::parse_book(&t.text())
        .profiles
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, ["cc", "pcc"]);
}

#[test]
fn a_stale_fingerprint_writes_nothing() {
    let t = tmp("stale", Some(BOOK));
    let r = answer_write(
        &t.door(),
        &json!({"changes": [{"op": "remove", "name": "pcc"}], "fingerprint": "0-0000000000000000"}),
    );
    assert_eq!(r.unwrap_err().0, "stale");
    assert_eq!(t.text(), BOOK);
}

#[test]
fn a_write_may_fix_the_broken_section_but_may_not_break_another() {
    let broken = BOOK.replace("account = \"b\"", "tmux-sise = \"200x50\"");
    let t = tmp("fix", Some(&broken));
    // 别的段照样能改（betacct 坏着不挡）。
    write(
        &t,
        json!([{"op": "set", "was": "pcc", "form": {"name": "pcc", "cwd": "/srv/q"}}]),
    )
    .unwrap();
    // 改坏的那一段：按条目改会覆盖那一项 ⇒ 存了就修好。
    write(
        &t,
        json!([{"op": "set", "was": "betacct", "form": {"name": "betacct", "from": "cct", "account": {"kind": "named", "name": "b"}}}]),
    )
    .unwrap();
    assert!(
        profile::problems_after(&t.text()).is_empty(),
        "{}",
        t.text()
    );
    // 改出新的坏处（基于一个不存在的）⇒ 拒、一个字节不写。
    let before = t.text();
    let (code, _) = write(
        &t,
        json!([{"op": "set", "was": "pcc", "form": {"name": "pcc", "from": "nope"}}]),
    )
    .unwrap_err();
    assert_eq!(code, "refused");
    assert_eq!(t.text(), before);
}

#[test]
fn init_writes_the_seed_or_an_empty_file() {
    let t = tmp("init", None);
    write(&t, json!([{"op": "init", "seed": true}])).unwrap();
    let names: Vec<String> = profile::parse_book(&t.text())
        .profiles
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, ["cc", "cct"]);
    let t = tmp("init-empty", None);
    write(&t, json!([{"op": "init", "seed": false}])).unwrap();
    assert!(profile::parse_book(&t.text()).profiles.is_empty());
}

#[test]
fn the_form_slots_cover_exactly_the_profile_keys() {
    let mut keys: Vec<String> = SLOTS
        .iter()
        .flat_map(|s| s.keys())
        .map(str::to_string)
        .collect();
    keys.sort();
    let mut want: Vec<String> = profile::PROFILE_KEYS
        .iter()
        .map(|k| k.to_string())
        .chain([profile::ARGS_KEY.to_string()])
        .collect();
    want.sort();
    assert_eq!(keys, want);
}

#[test]
fn every_form_round_trips_through_the_file() {
    let t = tmp("roundtrip", Some(BOOK));
    let r = read(&t);
    for p in r["profiles"].as_array().unwrap() {
        let f: ProfileForm = serde_json::from_value(p["form"].clone()).unwrap();
        let e = edit_of_form(&f).unwrap();
        let back = profile::parse_book(&BOOK.to_string());
        let orig = profile::edit_of(back.find(&f.name).unwrap());
        assert_eq!(e, orig, "{}", f.name);
    }
}

#[test]
fn a_migration_is_told_once_with_what_could_not_move_and_ack_removes_it() {
    let t = tmp("migrated", None);
    std::fs::write(
        t.0.join(relay_route_core::POSIX_ALIASES_REL),
        "alphacc() { ccm \"$@\" -- --account 'z'; }\ncca() { ccm -- --attach \"$@\"; }\n",
    )
    .unwrap();
    let r = read(&t);
    assert_eq!(r["migrated"]["count"], 1, "{r}");
    let skipped = r["migrated"]["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 1, "{r}");
    assert!(skipped[0].as_str().unwrap().contains("cca"));
    write(&t, json!([{"op": "ackMigrated"}])).unwrap();
    assert!(read(&t)["migrated"].is_null());
}
