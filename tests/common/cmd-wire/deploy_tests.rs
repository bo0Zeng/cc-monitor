//! 装 · 卸那两条的线上类型对着金样（`tests/__fixtures__/deploy-install.golden.json`，后端 `deploy_plan_tests` 核真产出的键集 == 它）：
//! 解得开、照类型写回逐字节同；多一格 · 缺那一句都解不开。
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
    assert_eq!(got.said, "placeholder");
    assert_eq!(serde_json::to_value(&got).unwrap(), product);
    let with = |k: &str, v: serde_json::Value| {
        let mut x = product.clone();
        x[k] = v;
        x
    };
    let mut missing = product.clone();
    missing.as_object_mut().unwrap().remove("said");
    // 没人读的格不上线：带着旧回包那几格（做了哪一样 · 哪一版）⇒ 解不开。
    for bad in [missing, with("action", serde_json::json!("keep"))] {
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
    assert_eq!(got.said, "placeholder-uninstall");
    assert_eq!(serde_json::to_value(&got).unwrap(), product);
}

/// 装 · 卸各是一条命令：入参类型各一个，名字与回包挂在它上面。
#[test]
fn install_and_uninstall_are_two_calls() {
    assert_eq!(DeployInstallReq::NAME, "deploy-install");
    assert_eq!(DeployUninstallReq::NAME, "deploy-uninstall");
    // 入参只有那台的名字：带着哪几个槽是本机后端自己的事实，调用方多交一格 ⇒ 解不开。
    let req = DeployInstallReq {
        machine: "box".into(),
    };
    assert_eq!(
        serde_json::to_value(&req).unwrap(),
        serde_json::json!({"machine": "box"})
    );
    for extra in [
        serde_json::json!({"slots": [], "machine": "box"}),
        serde_json::json!({"machine": "box", "extra": 1}),
    ] {
        assert!(
            serde_json::from_value::<DeployUninstallReq>(extra.clone()).is_err(),
            "{extra}"
        );
    }
}
