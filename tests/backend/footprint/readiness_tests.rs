//! 首次运行「开始用 · 剩 N 步」那份数：三步各自的事实 ⇒ 打没打勾；剩几步 ＝ 没打勾的几步。

use super::*;
use serde_json::json;

#[test]
fn 三步照事实打勾_剩几步是没打勾的几步() {
    assert_eq!(
        product(false, false, 0),
        json!({"steps": [{"id": "terminal", "done": false}, {"id": "named", "done": false}, {"id": "remote", "done": false}], "left": 3})
    );
    assert_eq!(product(true, false, 2)["left"], json!(1));
    assert_eq!(product(true, true, 1)["left"], json!(0));
    assert_eq!(
        product(true, true, 1)["steps"][2],
        json!({"id": "remote", "done": true})
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
