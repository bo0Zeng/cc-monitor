//! 渲那一行的两条（`launch-local` · `launch-render-cli`）交回之前预标哪一格：只认成品里实际用的号与请求里的工作目录。

use super::*;

#[test]
fn only_a_library_account_with_a_cwd_is_marked() {
    let named = Rendered {
        cmd: "ccm …".into(),
        account: Some(la::LaunchedAccount {
            name: "work".into(),
            config_dir: "/h/.cc/work".into(),
            model: None,
        }),
    };
    let base = Rendered {
        cmd: "ccm …".into(),
        account: None,
    };
    let with_cwd = Some("/w/proj");
    assert_eq!(
        rendered_target(with_cwd, &named),
        Some(("/h/.cc/work", "/w/proj"))
    );
    assert_eq!(
        rendered_target(with_cwd, &base),
        None,
        "账号 0 / 不表态不标"
    );
    assert_eq!(rendered_target(None, &named), None, "没给工作目录不标");
    assert_eq!(rendered_target(Some(""), &named), None);
}
