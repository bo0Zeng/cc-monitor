use super::*;

#[test]
fn parses_the_two_schemes_and_their_default_ports() {
    assert_eq!(
        Base::parse("https://api.example.com"),
        Ok(Base {
            tls: true,
            host: "api.example.com".to_string(),
            port: 443,
            path: String::new()
        })
    );
    assert_eq!(
        Base::parse("http://127.0.0.1:18789"),
        Ok(Base {
            tls: false,
            host: "127.0.0.1".to_string(),
            port: 18789,
            path: String::new()
        })
    );
}

#[test]
fn rejects_shapes_it_does_not_understand() {
    // 分母 = 我列出的这 5 形。
    for bad in [
        "ftp://x",
        "api.example.com",
        "https://",
        "://x",
        "https://:443",
    ] {
        assert!(Base::parse(bad).is_err(), "这一形不该被接受：{bad}");
    }
    // ★ 非空对照排在后面也够（上面几形都是 `Err`，尺子不可能恒 `Err` 还让这一句过）。
    assert!(Base::parse("https://ok.example").is_ok(), "这把尺子是瞎的");
}

/// ★★★ **`K-R1` 的正主之一**：`base_url` 里那一段路径**装得下了**，
/// 而先前它被 `rest.split('/').next()` 整个丢掉、且照样回 `Some`。
///
/// # 死值验就在这条判据的第一格上
///
/// 第一格逐字是 PM 补充里点名的那个读数：`Base::parse("https://h:443/v1")`。
/// **改前**它回 `Some(Base{host:"h",port:443})`，`/v1` 静默消失、没有任何东西出声；
/// **改后**那一段留在 `path` 里。⇒ 把 [`normalize_prefix`] 的返回改成
/// 恒 `Ok(String::new())`（形状对、恒答「没有前缀」那张脸），本条当场红。
#[test]
fn the_path_part_of_a_base_url_is_kept_instead_of_being_silently_dropped() {
    // 分母 = 我列出的这 7 形，逐形手写期望值。
    let cases: &[(&str, &str)] = &[
        ("https://h:443/v1", "/v1"),
        ("https://vendor.example/anthropic", "/anthropic"),
        ("http://127.0.0.1:11434/v1", "/v1"),
        ("https://h/openai/v1", "/openai/v1"),
        // 尾随的 `/` 在一个前缀里不携带信息 ⇒ 去掉。
        ("https://h/v1/", "/v1"),
        // 整段就是一个 `/` ⇒ 说的是「根」，等价于没有前缀。
        ("https://h/", ""),
        ("https://h", ""),
    ];
    for (url, want) in cases {
        let b = Base::parse(url).unwrap_or_else(|e| panic!("{url} 该解析得了：{e:?}"));
        assert_eq!(&b.path, want, "这一形的前缀取错了：{url}");
    }
    // ★ 非空对照承重：这把尺子**分得出**「有前缀」与「没前缀」
    //   （没有这一格，上面那两条 `""` 可能只是因为它恒回空串）。
    assert_ne!(
        Base::parse("https://h/v1").expect("有前缀那一形").path,
        Base::parse("https://h").expect("没前缀那一形").path
    );
}

/// **`base_url` 里丢东西要出声** —— 三形各自的理由**不许挤进同一个 `None`**。
#[test]
fn a_base_url_that_carries_things_a_prefix_cannot_carry_says_why() {
    // 分母 = 我列出的这 4 形。
    for bad in [
        "https://h/v1?beta=true",
        "https://h/v1#frag",
        "https://h//v1",
        "https://h:99999/v1",
    ] {
        assert!(Base::parse(bad).is_err(), "这一形不该被接受：{bad}");
    }
    // ★★ 理由**逐形不同**：四个理由串放进集合去重之后必须还是 4 个。
    //    这一格才是「一个 None 装了几件事」被治掉的读数 —— 只断 `is_err()` 的话，
    //    把每一支的理由都换成同一句，本条照样全绿。
    let mut whys: Vec<&'static str> = ["ftp://x", "https://", "https://h/v1?x=1", "https://h//v1"]
        .iter()
        .map(|u| Base::parse(u).expect_err("这几形都该是 Err").0)
        .collect();
    let n = whys.len();
    whys.sort();
    whys.dedup();
    assert_eq!(
        whys.len(),
        n,
        "有两形共用了同一句理由 —— 那句话在其中一形上是假的指引：{whys:?}"
    );
}

/// `K-R1`：**本地部署那一格的谓词** —— 回环认得出来，别的一律说「不是」。
///
/// ⚠ 它只是**谓词**；「明文非回环要不要拒」是 `table::build` 的决定，判据在那边。
#[test]
fn the_loopback_predicate_says_yes_only_to_the_local_machine() {
    // 分母 = 我列出的这 4 形回环写法。
    for yes in [
        "http://127.0.0.1:11434",
        "http://127.0.0.1",
        "http://localhost:8000/v1",
        "http://[::1]:11434/v1",
    ] {
        assert!(
            upstream_url_core::upstream_is_loopback(&Base::parse(yes).expect("该解析得了").host),
            "这一形是回环，却被说成不是：{yes}"
        );
    }
    // ★ 非空对照 + 单断：分母 = 我列出的这 4 形非回环写法。
    for no in [
        "http://1.2.3.4/v1",
        "https://api.example.com",
        "http://192.0.2.1:8000",
        // ⚠ 一个**名字**里含 localhost 不算 —— 它解到哪儿本条判不了。
        "http://localhost.evil.example",
    ] {
        assert!(
            !upstream_url_core::upstream_is_loopback(&Base::parse(no).expect("该解析得了").host),
            "这一形不是回环，却被说成是：{no}"
        );
    }
}

#[test]
fn host_header_omits_the_default_port_only() {
    let d = Base::parse("https://api.example.com").expect("d");
    assert_eq!(d.host_header(), "api.example.com");
    let n = Base::parse("http://127.0.0.1:18789").expect("n");
    assert_eq!(n.host_header(), "127.0.0.1:18789");
    // ★ `K-R1`：`Host:` 头里**没有**路径前缀那一段（它属于请求行，不属于这里）。
    let p = Base::parse("https://api.example.com/anthropic").expect("p");
    assert_eq!(p.host_header(), "api.example.com");
}

/// TLS 那条路本轮**没有任何行为验证**（不许打真 API，本机也没有 HTTPS 夹具）。
///
/// # 名字只承诺它证得了的那一半（回修轮 08-25 改名，D1 `重要-2`）
///
/// 旧名 `tls_client_config_builds_and_carries_roots` 里的 **carries roots** 是**空真**：
/// 它断的是 crate 常量 `webpki_roots::TLS_SERVER_ROOTS` 非空，**不是** `tls_config()`
/// 真把根装了进去 ⇒ 把装配点的根证书集换成空 `Vec::new()`，它照样全绿（审计 `CS`）。
///
/// 今天它断的是**生产段的装配函数** `root_store()` 的**根证书条数 > 0**，
/// 而 `tls_config()` 里那一份就是它返回的那一份（唯一调用点，就在上面几行）。
///
/// **它仍然不证明**：握手成功 · 证书校验真的按这套根做 · 逐块透传在 TLS 上成立。
/// 那三样要真 TLS 行为验，本轮禁打真 API ⇒ 登记为 `判不了`，别再让名字替它们背书。
#[test]
fn tls_client_config_builds_from_a_non_empty_root_store() {
    // ★ 真查数量（>0），而且查的是**装进去的那一份**，不是 crate 常量。
    let n = root_store().roots.len();
    assert!(
        n > 0,
        "装进 ClientConfig 的根证书集不该是空的（实测 {n} 条）"
    );
    // 非空对照：空的根证书集在 rustls 自己看来连服务端校验器都建不起来
    //（`VerifierBuilderError::NoRootAnchors`）⇒ 这一条把「非空」与「它真能当校验根用」连起来。
    assert!(
        rustls::client::WebPkiServerVerifier::builder_with_provider(
            Arc::new(root_store()),
            Arc::new(rustls::crypto::ring::default_provider()),
        )
        .build()
        .is_ok(),
        "非空的根证书集应当建得起服务端校验器"
    );
    let name = rustls::pki_types::ServerName::try_from("api.example.com".to_string())
        .expect("域名应当合法");
    let conn = rustls::ClientConnection::new(tls_config(), name);
    assert!(conn.is_ok(), "TLS 客户端连接对象应当建得起来");
}
