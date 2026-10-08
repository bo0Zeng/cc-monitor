//! 首次运行「开始用 · 剩 N 步」那份数：三步各自的事实 ⇒ 打没打勾；剩几步 ＝ 没打勾的几步。

use super::*;
use serde_json::json;

#[test]
fn 三步照事实打勾_哪步必做由这里说_剩几步只数必做的() {
    assert_eq!(
        product(false, false, 0, false),
        json!({"steps": [
            {"id": "terminal", "done": false, "required": true},
            {"id": "named", "done": false, "required": false},
            {"id": "remote", "done": false, "required": false},
        ], "left": 1, "skipped": false})
    );
    assert_eq!(
        product(true, false, 0, false)["left"],
        json!(0),
        "可选的没做不算剩下的（状态栏不为它们一直催）"
    );
    assert_eq!(
        product(true, true, 1, false)["steps"][2],
        json!({"id": "remote", "done": true, "required": false})
    );
    assert_eq!(
        product(false, false, 0, true)["skipped"],
        json!(true),
        "点过「跳过」原样带回"
    );
}

#[test]
fn 机器表的台数由问的那一方带上_不给或不是非负整数就拒() {
    assert_eq!(remotes_arg(&json!({"remotes": 3})).unwrap(), 3);
    assert_eq!(remotes_arg(&json!({})).unwrap_err().0, "bad_args");
    assert_eq!(
        remotes_arg(&json!({"remotes": -1})).unwrap_err().0,
        "bad_args"
    );
    assert_eq!(
        remotes_arg(&json!({"remotes": "2"})).unwrap_err().0,
        "bad_args"
    );
}
