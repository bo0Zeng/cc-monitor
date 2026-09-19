
/// ★★ **启动入口真的过了规格校验吗**〔audit-0805 08-07，Phase G 第 49 件〕。
///
/// 下面那几条判的是 `validate_spec` **这个函数本身**（端口 0 / 空 host …）。
/// 08-07 实测：把 `start_forward` 里那行 `validate_spec(&spec)?;` 删掉，
/// **全仓 980 条判据一条不红** —— 与删除路、建分支路同一族的第三例。
///
/// # 这条能跑真路
///
/// 围栏在**任何 I/O 之前**，紧接着是查远端配置。于是：
/// 传一个非法 spec + 一个不存在的 origin ——
/// · 围栏在 ⇒ 报围栏的话（「本地端口必须 > 0」）；
/// · 围栏没了 ⇒ 往下走一步，报「未找到远端配置」。
/// 两句话分得开，而**整个过程零网络**（配置查不到就返回了）。
#[tokio::test]
async fn the_start_entry_point_actually_validates_before_anything_else() {
    let spec = ForwardSpec {
        origin: "这个 origin 一定不存在-audit0805".to_string(),
        local_port: 0, // 非法：围栏第一条就该拒
        remote_host: "127.0.0.1".to_string(),
        remote_port: 5432,
    };
    let err = start_forward(spec)
        .await
        .expect_err("非法 spec 竟然没被拒 —— 围栏没接上");
    // 红要红对成因：必须是**围栏**拒的，不是后面查配置那步。
    assert!(
        err.contains("本地端口"),
        "拒绝了，但不是规格校验拒的（错误：{err}）—— \
             说明 `start_forward` 已经越过围栏往下走了一步。\
             那一步今天只是查配置；哪天它先做点别的（连接、绑端口），\
             非法 spec 就会真的落到 I/O 上。"
    );
}
use super::*;

fn spec(lp: u16, rh: &str, rp: u16) -> ForwardSpec {
    ForwardSpec {
        origin: "o".into(),
        local_port: lp,
        remote_host: rh.into(),
        remote_port: rp,
    }
}

#[test]
fn validate_spec_guards() {
    assert!(validate_spec(&spec(15432, "localhost", 5432)).is_ok());
    assert!(validate_spec(&spec(0, "h", 80)).is_err()); // 本地端口 0
    assert!(validate_spec(&spec(8080, "", 80)).is_err()); // 空 remote host
    assert!(validate_spec(&spec(8080, "  ", 80)).is_err()); // 纯空白 remote host
    assert!(validate_spec(&spec(8080, "h", 0)).is_err()); // 远端端口 0
}

#[test]
fn next_id_monotonic_and_prefixed() {
    let a = next_id();
    let b = next_id();
    assert_ne!(a, b);
    assert!(a.starts_with("fwd-"));
    assert!(b.starts_with("fwd-"));
}
