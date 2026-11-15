//! 换 agent 家目录存之前那一问：在不在 · 是不是目录 · 像不像（有没有那一家的记录树），一条命令判、回写好的那一句（能用 ⇒ `null`）。

use super::*;
use serde_json::json;

/// 判据仍用 JSON 写入参：经 `typed::take` 解成入参类型（同帧面那一跳），回包序列化回 JSON 比。
fn answer_check_with(
    a: &serde_json::Value,
    home: Option<std::path::PathBuf>,
) -> Result<serde_json::Value, crate::common::said::Said> {
    let req: AgentHomeCheckReq = crate::wire::typed::take(a)?;
    super::answer_check_with(&req, home, crate::agents::REGISTRY)
        .map(|x| serde_json::to_value(x).unwrap())
}
fn answer_check(a: &serde_json::Value) -> Result<serde_json::Value, crate::common::said::Said> {
    answer_check_with(a, crate::platform::paths::home_dir())
}

struct TmpDir(std::path::PathBuf);
impl TmpDir {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn tmp() -> TmpDir {
    TmpDir(test_home::scratch("agent-home"))
}

#[test]
fn 有记录树才算像() {
    let d = tmp();
    std::fs::create_dir_all(
        crate::agents::IntoHomes::into_homes(d.path())[0]
            .records_root()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(verdict(crate::agents::REGISTRY, d.path()), Verdict::Ok);
    assert_eq!(
        answer_check(&json!({"path": d.path()})).unwrap(),
        json!({"said": null})
    );
}

#[test]
fn 不在_不是目录_没有记录树_各一个码() {
    let d = tmp();
    assert_eq!(
        verdict(crate::agents::REGISTRY, &d.path().join("nope")),
        Verdict::Missing
    );
    std::fs::write(d.path().join("a.txt"), "x").unwrap();
    assert_eq!(
        verdict(crate::agents::REGISTRY, &d.path().join("a.txt")),
        Verdict::NotDir
    );
    assert_eq!(
        verdict(crate::agents::REGISTRY, d.path()),
        Verdict::NoRecords
    );
    // 记录树那个名字是个文件也不算
    std::fs::write(
        crate::agents::IntoHomes::into_homes(d.path())[0]
            .records_root()
            .unwrap(),
        "x",
    )
    .unwrap();
    assert_eq!(
        verdict(crate::agents::REGISTRY, d.path()),
        Verdict::NoRecords
    );
    // 每一态回写好的那一句，带用户交来的原样路径。
    let said = |p: &std::path::Path| answer_check(&json!({ "path": p })).unwrap()["said"].clone();
    let shown = |p: &std::path::Path| p.display().to_string();
    let nope = d.path().join("nope");
    assert_eq!(
        said(&nope),
        json!(copy_core::copy_text(
            "bePath.dir.missing",
            &[("path", &shown(&nope))]
        ))
    );
    let file = d.path().join("a.txt");
    assert_eq!(
        said(&file),
        json!(copy_core::copy_text(
            "bePath.dir.notDir",
            &[("path", &shown(&file))]
        ))
    );
    assert_eq!(
        said(d.path()),
        json!(copy_core::copy_text(
            "bePath.dir.noRecords",
            &[("path", &shown(d.path()))]
        ))
    );
}

#[test]
fn 路径缺席_不是串_相对路径_拒() {
    for a in [
        json!({}),
        json!({"path": 3}),
        json!({"path": ""}),
        json!({"path": "rel/x"}),
    ] {
        assert_eq!(
            answer_check(&a).unwrap_err().code.as_deref(),
            Some("bad_args"),
            "{a}"
        );
    }
}

#[test]
fn 波浪号按这台家目录展开() {
    let d = tmp();
    std::fs::create_dir_all(
        crate::agents::IntoHomes::into_homes(&d.path().join(".agent-x"))[0]
            .records_root()
            .unwrap(),
    )
    .unwrap();
    let home = d.path().to_path_buf();
    let got = answer_check_with(&json!({"path": "~/.agent-x"}), Some(home.clone())).unwrap();
    assert_eq!(got, json!({"said": null}));
    // Windows 的写法 `~\…` 同一份展开（`platform::paths::expand_tilde_from`）。
    let got = answer_check_with(&json!({"path": "~\\.agent-x"}), Some(home.clone())).unwrap();
    assert_eq!(got, json!({"said": null}));
    assert_eq!(
        answer_check_with(&json!({"path": "~"}), Some(home)).unwrap(),
        json!({ "said": copy_core::copy_text("bePath.dir.noRecords", &[("path", "~")]) }),
        "那一句里是用户敲的原样（`~` 不展开）"
    );
}

#[test]
fn 家目录说不出_波浪号路径拒() {
    assert_eq!(
        answer_check_with(&json!({"path": "~/.agent-x"}), None)
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_args")
    );
}

/// 家目录名不是 UTF-8：`~/…` 照字节展开、照实带，判的是盘上真在的那个目录（有损拼出来的那一串指不到它）。
#[cfg(unix)]
#[test]
fn 家目录名不是_utf8_波浪号照字节展开() {
    use std::os::unix::ffi::OsStrExt;
    let d = tmp();
    let home = d.path().join(std::ffi::OsStr::from_bytes(b"caf\xe9"));
    std::fs::create_dir_all(
        crate::agents::IntoHomes::into_homes(&home.join(".agent-x"))[0]
            .records_root()
            .unwrap(),
    )
    .unwrap();
    let got = answer_check_with(&json!({"path": "~/.agent-x"}), Some(home)).unwrap();
    assert_eq!(got, json!({"said": null}), "非 UTF-8 的家没照字节展开");
}

/// 判的是注入的那份注册表：替身家的记录树跟生产那几家的不同名 ⇒ 照替身答，生产那几家的树在也不算。
#[test]
fn 照注入的注册表判_替身家的记录树才算() {
    let fake = [crate::agents::fake::full_row(|| None)];
    let d = tmp();
    std::fs::create_dir_all(crate::agents::records_root_among(&fake, "fake", d.path()).unwrap())
        .unwrap();
    assert_eq!(verdict(&fake, d.path()), Verdict::Ok, "替身家的树在却没认");
    assert_eq!(
        verdict(crate::agents::REGISTRY, d.path()),
        Verdict::NoRecords,
        "替身家的树不该让生产那几家认"
    );
    // 生产那一家的树在、替身的不在 ⇒ 照替身答「没有记录树」，帧面那一问也一样。
    let e = tmp();
    std::fs::create_dir_all(
        crate::agents::IntoHomes::into_homes(e.path())[0]
            .records_root()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        verdict(&fake, e.path()),
        Verdict::NoRecords,
        "照生产注册表判了"
    );
    let shown = e.path().display().to_string();
    let req = AgentHomeCheckReq {
        path: shown.clone(),
    };
    assert_eq!(
        super::answer_check_with(&req, None, &fake)
            .unwrap()
            .said
            .map(|w| w.0),
        Some(copy_core::copy_text(
            "bePath.dir.noRecords",
            &[("path", &shown)]
        )),
        "帧面那一问没照注入的注册表"
    );
}
