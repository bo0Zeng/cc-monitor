//! 装 · 卸那两条的线上类型对着金样（`tests/__fixtures__/deploy-install.golden.json`，后端 `deploy_plan_tests` 核真产出的键集 == 它）：
//! 解得开、照类型写回逐字节同；认不出的动作 · 缺一格都解不开。
use super::*;
use crate::Call;

fn golden() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../tests/__fixtures__/deploy-install.golden.json"
    ))
    .unwrap()
}

#[test]
fn the_install_reply_decodes_the_golden_and_writes_it_back() {
    let product = golden()["product"].clone();
    let got: Installed = serde_json::from_value(product.clone()).expect("金样解不开");
    assert_eq!(
        (
            got.action,
            got.build.as_str(),
            got.os.as_str(),
            got.said.as_str()
        ),
        (ActionTag::Keep, "p0b-placeholder", "Linux", "placeholder")
    );
    assert_eq!(serde_json::to_value(&got).unwrap(), product);
    let with = |k: &str, v: serde_json::Value| {
        let mut x = product.clone();
        x[k] = v;
        x
    };
    let mut missing = product.clone();
    missing.as_object_mut().unwrap().remove("said");
    for bad in [missing, with("action", serde_json::json!("overwrite"))] {
        assert!(
            serde_json::from_value::<Installed>(bad.clone()).is_err(),
            "该拒却收了：{bad}"
        );
    }
}

#[test]
fn the_uninstall_reply_decodes_the_golden_and_writes_it_back() {
    let product = golden()["uninstalled"].clone();
    let got: Uninstalled = serde_json::from_value(product.clone()).expect("金样解不开");
    assert_eq!(
        (got.removed, got.said.as_str()),
        (true, "placeholder-uninstall")
    );
    assert_eq!(serde_json::to_value(&got).unwrap(), product);
}

/// 装 · 卸各是一条命令：入参类型各一个，名字与回包挂在它上面。
#[test]
fn install_and_uninstall_are_two_calls() {
    assert_eq!(DeployInstallReq::NAME, "deploy-install");
    assert_eq!(DeployUninstallReq::NAME, "deploy-uninstall");
    let row = SlotRow {
        os: "Linux".into(),
        arch: "x86_64".into(),
        id: "p9a-x".into(),
        path: "/s/linux-x86_64".into(),
    };
    let req = DeployInstallReq {
        slots: vec![row],
        machine: "box".into(),
    };
    assert_eq!(
        serde_json::to_value(&req).unwrap(),
        serde_json::json!({"slots": [{"os": "Linux", "arch": "x86_64", "id": "p9a-x", "path": "/s/linux-x86_64"}], "machine": "box"})
    );
    assert!(serde_json::from_value::<DeployUninstallReq>(
        serde_json::json!({"slots": [], "machine": "box", "extra": 1})
    )
    .is_err());
}
