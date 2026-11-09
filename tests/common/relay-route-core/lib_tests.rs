//! 中转门牌（`relay-route-core`）的判据：拼与拆互逆 · 段闸 · 两个前缀闭集。
//! 守的要求：「端口 · 路由渲染 · 段的字符闸收进共享 crate，一份实现两侧 use」·
//! `§3.3`「值 fail-closed 校验：必须是构造口产得出的形状」。

use super::*;

/// 拼出来的每一形都被 [`base_url_shape_ok`] 收、被 [`parse_target`] 还原成同样的三格（两个前缀 × 若干段）。
#[test]
fn what_base_url_builds_the_inverse_accepts_and_parse_restores() {
    let segs = [("claude-code", "acct-a"), ("claude-code", "0"), ("a", "_")];
    for m in RouteMode::ALL {
        for (a, b) in segs {
            let url = base_url(PORT, m, a, b).expect("合法段拼不出来");
            assert_eq!(url, format!("http://127.0.0.1:8788{}{a}/{b}", m.prefix()));
            assert!(base_url_shape_ok(&url), "构造口的产物被逆拒了：{url}");
            let path = route_path(m, a, b).unwrap();
            let target = format!("{path}/v1/messages?beta=true");
            assert_eq!(
                parse_target(&target),
                Some(Parsed {
                    mode: m,
                    seg1: a,
                    seg2: b,
                    origin: None,
                    rest: "v1/messages?beta=true"
                })
            );
        }
    }
}

/// 注入的地址**不随会话变**：恰好两段，带第 3 段（会话 id / 启动器铸的 nonce）的那一形逆一侧拒。
/// 守的要求：「启动器不造会话 id、不往环境变量 / 中转地址里塞任何会话身份」。
#[test]
fn the_injected_address_carries_no_session_segment() {
    let two = base_url(PORT, RouteMode::Passthrough, "claude-code", "_").unwrap();
    assert!(base_url_shape_ok(&two), "正控：两段那一形该收");
    assert!(
        !base_url_shape_ok(&format!("{two}/3f2a9c1e-7d44-4c3b-9a55-0e6b2f1d8c77")),
        "带会话段的老三段形被当成注入地址收了"
    );
}

/// 坏形一律拒（逆那一侧）；过不了闸的段一律拼不出来（构造那一侧）。
#[test]
fn bad_shapes_and_bad_segments_are_refused_on_both_sides() {
    for bad in [
        "http://localhost:8788/s/a/b",
        "https://127.0.0.1:8788/s/a/b",
        "http://127.0.0.1:8788/s/a/b/",
        "http://127.0.0.1:8788/s/a",
        "http://127.0.0.1:8788/s/a/b/c",
        "http://127.0.0.1:8788/x/a/b",
        "http://127.0.0.1:0/s/a/b",
        "http://127.0.0.1:70000/s/a/b",
        "http://127.0.0.1:/s/a/b",
        "http://127.0.0.1:8788/s/a/b?x=1",
        "http://127.0.0.1:8788/s/a/..",
        "http://127.0.0.1:8788",
    ] {
        assert!(!base_url_shape_ok(bad), "坏形被收了：{bad}");
    }
    for seg in ["", "a.b", "a/b", "..", "a b", "é", &"x".repeat(129)] {
        assert!(!segment_is_safe(seg), "{seg:?} 过了闸");
        for m in RouteMode::ALL {
            assert_eq!(route_path(m, seg, "b"), None);
            assert_eq!(route_path(m, "a", seg), None);
        }
    }
    assert!(
        segment_is_safe(&"x".repeat(128)),
        "正好 128 字节该过（正控）"
    );
    assert_eq!(base_url(0, RouteMode::Substitute, "a", "b"), None);
    for bad in ["/s/a", "/q/a/b/x", "/s/a/b", "/s/a/b./x", "s/a/b/x"] {
        assert_eq!(parse_target(bad), None, "{bad} 被切出来了");
    }
}

/// 两个前缀两两不同、各自以 `/` 起止；闭集就这两个。
#[test]
fn the_two_prefixes_are_a_closed_distinct_set() {
    let p: Vec<&str> = RouteMode::ALL.iter().map(|m| m.prefix()).collect();
    assert_eq!(p, vec!["/s/", "/t/"]);
    assert_ne!(RouteMode::ALL[0], RouteMode::ALL[1]);
}

/// 展开过钥匙的地址：切得开的恰是「构造口产物 ＋ 一段形状对的钥匙」；别的都切不开。
#[test]
fn a_keyed_url_splits_only_when_the_key_and_the_route_both_have_our_shape() {
    let key = "0123456789abcdef".repeat(4);
    let url = base_url(PORT, RouteMode::Passthrough, "claude-code", "_").unwrap();
    let (head, tail) = url.split_at("http://127.0.0.1:8788/".len());
    let keyed = format!("{head}{key}/{tail}");
    assert_eq!(
        split_keyed_base_url(&keyed),
        Some((head, &format!("/{tail}")[..]))
    );
    assert_eq!(
        format!("{head}{key}/{tail}"),
        keyed,
        "两半拼回去（插回钥匙）== 原串"
    );
    for bad in [
        url.clone(),                                    // 没有钥匙段
        format!("{head}{}/{tail}", &key[..63]),         // 短一个字符
        format!("{head}{}/{tail}", key.to_uppercase()), // 大写
        format!("{head}{key}/x/a/b"),                   // 前缀不认得
        format!("{head}{key}/{tail}/k1"),               // 退役的会话段
        format!("http://localhost:8788/{key}/{tail}"),  // 非字面回环
        format!("https://127.0.0.1:8788/{key}/{tail}"),
        "https://api.example.com/v1".to_string(), // 用户自己的端点
    ] {
        assert_eq!(split_keyed_base_url(&bad), None, "{bad}");
    }
    assert!(key_shape_ok(&key) && !key_shape_ok(&key[..63]) && !key_shape_ok(&key.to_uppercase()));
}

/// 插钥匙是切钥匙的逆：插出来的切得回原来两半、钥匙段恰是那一把；地址或钥匙形状不对 ⇒ 不插。
#[test]
fn keying_a_url_is_the_inverse_of_splitting_it() {
    let key = "0123456789abcdef".repeat(4);
    for (m, a, b) in [
        (RouteMode::Passthrough, "claude-code", "_"),
        (RouteMode::Substitute, "claude-code", "work"),
    ] {
        let url = base_url(PORT, m, a, b).unwrap();
        let keyed = keyed_base_url(&url, &key).expect("合法地址 ＋ 合法钥匙插不进去");
        let (head, tail) = split_keyed_base_url(&keyed).expect("插出来的切不开");
        assert_eq!(format!("{head}{}", &tail[1..]), url, "去掉钥匙段 == 原地址");
        assert_eq!(
            &keyed[head.len()..head.len() + key.len()],
            key,
            "钥匙段就是那一把"
        );
    }
    let url = base_url(PORT, RouteMode::Passthrough, "claude-code", "_").unwrap();
    assert_eq!(keyed_base_url(&url, &key[..63]), None, "短钥匙");
    assert_eq!(keyed_base_url(&url, &key.to_uppercase()), None, "大写钥匙");
    assert_eq!(
        keyed_base_url("https://api.example.com/v1", &key),
        None,
        "别人的地址"
    );
    let already = keyed_base_url(&url, &key).unwrap();
    assert_eq!(
        keyed_base_url(&already, &key),
        None,
        "已经带钥匙的不再插一次"
    );
}

/// 来处段（会话血缘）：拼上去的 `~<来处>[~<父>]` 被逆一侧收、被切的一侧还原；钥匙插拔前后尾段原样。
#[test]
fn an_origin_tail_round_trips_through_build_shape_split_and_parse() {
    let key = "0123456789abcdef".repeat(4);
    let parent = "3f2a9c1e-7d44-4c3b-9a55-0e6b2f1d8c77";
    for m in RouteMode::ALL {
        let url = base_url(PORT, m, "claude-code", "work").unwrap();
        for (p, tail) in [
            (None, "~0a1b2c3d4e5f6071".to_string()),
            (Some(parent), format!("~0a1b2c3d4e5f6071~{parent}")),
        ] {
            let with = with_origin(&url, "0a1b2c3d4e5f6071", p).expect("合法来处拼不上");
            assert_eq!(with, format!("{url}/{tail}"));
            assert!(base_url_shape_ok(&with), "带来处的被逆拒了：{with}");
            let keyed = keyed_base_url(&with, &key).expect("带来处的插不进钥匙");
            let (head, rest) = split_keyed_base_url(&keyed).expect("带来处的切不开");
            assert_eq!(
                format!("{head}{}", &rest[1..]),
                with,
                "去掉钥匙段 == 原地址"
            );
            let target = format!("{}/v1/messages", &rest);
            let got = parse_target(&target).expect("带来处的请求目标切不出");
            assert_eq!(
                got,
                Parsed {
                    mode: m,
                    seg1: "claude-code",
                    seg2: "work",
                    origin: Some(Origin {
                        token: "0a1b2c3d4e5f6071",
                        parent: p,
                    }),
                    rest: "v1/messages",
                }
            );
            assert_eq!(
                with_origin(&with, "0a1b2c3d4e5f6071", None),
                None,
                "已经带来处的不再拼一次"
            );
        }
    }
}

/// 来处段坏形：拼的一侧拒（段闸外 · 空），认与切的一侧拒（空 · 三截 · 段闸外 · 不在尾上）。
#[test]
fn bad_origin_tails_are_refused_on_every_side() {
    let url = base_url(PORT, RouteMode::Passthrough, "claude-code", "_").unwrap();
    for (t, p) in [
        ("", None),
        ("a.b", None),
        ("ok", Some("")),
        ("ok", Some("a b")),
    ] {
        assert_eq!(with_origin(&url, t, p), None, "{t:?} {p:?} 拼上了");
    }
    assert_eq!(with_origin("https://api.example.com/v1", "ok", None), None);
    for bad in ["~", "~~p", "~t~", "~t~p~q", "~t.x", "~t~p.q"] {
        assert!(!base_url_shape_ok(&format!("{url}/{bad}")), "{bad} 被收了");
        assert_eq!(
            parse_target(&format!("/t/claude-code/_/{bad}/v1/messages")),
            None,
            "{bad} 被切出来了"
        );
    }
    assert!(
        !base_url_shape_ok(&format!("{url}/~t/x")),
        "来处之后还有段的被收了"
    );
    assert_eq!(
        parse_target("/t/claude-code/_/v1/~t"),
        Some(Parsed {
            mode: RouteMode::Passthrough,
            seg1: "claude-code",
            seg2: "_",
            origin: None,
            rest: "v1/~t",
        }),
        "不紧跟在第 2 段之后的不是来处，原样交上游"
    );
}

/// 声明表每一行自洽：落点不重名 · 版本从 1 起 · 缓存不升级（下限等于版本）· 状态的下限不高于版本。
#[test]
fn every_declared_file_is_consistent_and_declared_once() {
    assert!(
        OWN_FILES.len() >= 15,
        "声明表只有 {} 行：登漏了",
        OWN_FILES.len()
    );
    let mut seen = std::collections::BTreeSet::new();
    for r in OWN_FILES {
        assert!(seen.insert(r.rel), "落点登了两行：{}", r.rel);
        if let Format::Json { version, floor } = r.format {
            assert!(
                floor >= 1 && floor <= version,
                "{}：下限 {floor} · 版本 {version}",
                r.rel
            );
            if r.kind() == Kind::Cache {
                assert_eq!(floor, version, "{}：缓存不升级，下限要等于版本", r.rel);
            }
        }
    }
}

/// 落点 · 主人类 · 等迁到哪一版：后端家里那几份（[`OWN_FILES`]）＋ 客户端那几处（[`CLIENT_SPOTS`]）排成一张。
fn every_spot() -> Vec<Spot> {
    OWN_FILES
        .iter()
        .map(|r| Spot {
            place: r.place,
            at: r.rel,
            owner: r.owner,
            until: None,
        })
        .chain(CLIENT_SPOTS.iter().copied())
        .collect()
}

/// 每行都有主人类：缓存就是 ⑤（状态 / 缓存由主人类推出，不另记一格）；声明成状态的不许是 ⑤。
#[test]
fn every_row_has_an_owner_and_the_cache_rows_are_exactly_the_fifth() {
    let mut owners = std::collections::BTreeSet::new();
    for r in OWN_FILES {
        assert_eq!(
            r.owner == Owner::Cache,
            r.kind() == Kind::Cache,
            "{}：主人类 {:?}",
            r.rel,
            r.owner
        );
        owners.insert(format!("{:?}", r.owner));
    }
    assert!(
        owners.len() >= 4,
        "表里的主人类只有 {owners:?}：没照 ①–⑤ 分"
    );
}

/// ①②③ 不许住客户端：会话 · 机器 · 跨机器集合的事实只住后端（问来的只当缓存）；等迁的旧行到期那一版前除外。
#[test]
fn session_machine_and_set_facts_never_live_on_a_client() {
    let now = current_release();
    for s in every_spot() {
        if !matches!(s.owner, Owner::Session | Owner::Machine | Owner::Set) {
            continue;
        }
        if s.place == Place::Backend {
            continue;
        }
        let at = s
            .until
            .unwrap_or_else(|| panic!("{}：{:?} 住在 {:?}（客户端）", s.at, s.owner, s.place));
        let due = release_of(at).unwrap_or_else(|| panic!("{}：到期 `{at}` 不是 X.Y.Z", s.at));
        assert!(now < due, "{}：等迁的旧行到期（{at}）还在：删掉它", s.at);
    }
}

/// ④ 不许经别台后端写：手机那一端的 ④ 只住手机（对手机，哪一台后端都是别台）；桌面那一端的 ④ 只住壳 · 界面 · 本机后端。
#[test]
fn an_ends_own_things_live_only_with_that_end() {
    let mut ends = std::collections::BTreeSet::new();
    for s in every_spot() {
        let Owner::End(end) = s.owner else { continue };
        let ok = match end {
            End::Phone => s.place == Place::Phone,
            End::Desktop => matches!(s.place, Place::Shell | Place::Ui | Place::Backend),
        };
        assert!(ok, "{}：{end:?} 那一端的 ④ 住在 {:?}", s.at, s.place);
        ends.insert(format!("{end:?}"));
    }
    assert_eq!(ends.len(), 2, "两端的 ④ 都该登着：{ends:?}");
}

/// 后端家里那几份缓存（⑤，别台东西的副本）住缓存目录；名单里那几份照别的东西现造、落点由读它的那一方定（只许变短）。
#[test]
fn backend_caches_live_under_the_cache_dir() {
    const AT_THE_READERS_PLACE: &[&str] = &[
        KEY_FILE_REL,
        SSH_CONFIG_REL,
        ".cc-monitor/launch-pending/<pid>.json",
        ".cc-monitor/hosts/.bin/<version>/ccm.exe",
        ".cc-monitor/answer-plugin/<file>",
    ];
    for r in OWN_FILES {
        if r.owner != Owner::Cache || r.place != Place::Backend {
            continue;
        }
        let under = r.rel.starts_with(".cc-monitor/cache/");
        assert_ne!(
            under,
            AT_THE_READERS_PLACE.contains(&r.rel),
            "{}：缓存住 `.cc-monitor/cache/` 底下（名单里的那几份除外，进了缓存目录就从名单删掉）",
            r.rel
        );
    }
}

/// 仓里一份源码（相对仓根）。
fn repo_text(rel: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel} 读不动：{e}"))
}

/// 手机 Room 那几张表的名字（core-data 每个 `@Entity` 的 `tableName`，写成常量的照 `const val` 解）。
fn room_tables() -> std::collections::BTreeSet<String> {
    let db = "src/mobile/core-data/src/main/kotlin/com/ccmonitor/mobile/core/data/db";
    // 两个库各自登的实体（`entities = [A::class, …]`）⇒ 实体住哪几份文件由下面那一核兜住：漏读一份就对不上。
    let listed: std::collections::BTreeSet<String> = ["AppDatabase.kt", "CacheDatabase.kt"]
        .iter()
        .flat_map(|f| {
            let t = repo_text(&format!("{db}/{f}"));
            let body = t
                .split_once("entities = [")
                .and_then(|(_, b)| b.split_once(']'))
                .expect("entities 那一段")
                .0
                .to_string();
            body.split(',')
                .filter_map(|c| c.trim().strip_suffix("::class").map(str::to_string))
                .collect::<Vec<_>>()
        })
        .collect();
    let text: String = ["Entities.kt", "CacheEntities.kt", "KeyEntities.kt"]
        .iter()
        .map(|f| repo_text(&format!("{db}/{f}")))
        .collect();
    let declared: std::collections::BTreeSet<String> = text
        .match_indices("@Entity(")
        .filter_map(|(i, _)| {
            let after = &text[i..];
            let name = after.split_once("data class ")?.1;
            Some(name[..name.find(['(', ' '])?].to_string())
        })
        .collect();
    assert_eq!(
        listed, declared,
        "两个库登的实体与那三份文件里的 @Entity 对不上（实体换了文件就把它加进上面那张）"
    );
    let consts: std::collections::BTreeMap<&str, &str> = text
        .lines()
        .filter_map(|l| {
            let (name, v) = l.trim().strip_prefix("const val ")?.split_once(" = ")?;
            Some((name.trim(), v.trim().trim_matches('"')))
        })
        .collect();
    text.match_indices("@Entity(tableName = ")
        .map(|(i, m)| {
            let rest = &text[i + m.len()..];
            let end = rest.find([',', ')']).expect("tableName 那一格有尾");
            let v = rest[..end].trim();
            match v.strip_prefix('"') {
                Some(lit) => lit.trim_end_matches('"').to_string(),
                None => consts
                    .get(v)
                    .unwrap_or_else(|| panic!("表名常量 {v} 没找到"))
                    .to_string(),
            }
        })
        .collect()
}

/// 界面 `LS_KEYS` 那几个键（`local-storage.ts`；带参数的那一形取它固定的前半）。
fn ls_keys() -> std::collections::BTreeSet<String> {
    let text = repo_text("src/frontend/ui/local-storage.ts");
    let body = text
        .split_once("export const LS_KEYS = {")
        .and_then(|(_, b)| b.split_once("} as const;"))
        .expect("LS_KEYS 那一段")
        .0;
    body.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('/') && !l.starts_with('*'))
        .filter_map(|l| {
            let q = l.find(['"', '`'])?;
            let rest = &l[q + 1..];
            let end = rest.find(['"', '`', '$'])?;
            Some(rest[..end].to_string())
        })
        .collect()
}

/// 客户端落点登全：Room 每张表 · 界面每个存储键在 [`CLIENT_SPOTS`] 里各一行，表里也没有多出来的。
#[test]
fn client_spots_match_the_room_tables_and_the_storage_keys() {
    let listed = |prefix: &str| -> std::collections::BTreeSet<String> {
        CLIENT_SPOTS
            .iter()
            .filter_map(|s| s.at.strip_prefix(prefix).map(str::to_string))
            .collect()
    };
    assert_eq!(
        listed(ROOM_SPOT),
        room_tables(),
        "手机 Room 的表与登的对不上"
    );
    assert_eq!(listed(STORAGE_SPOT), ls_keys(), "界面存储键与登的对不上");
    let mut seen = std::collections::BTreeSet::new();
    for s in CLIENT_SPOTS {
        assert!(seen.insert(s.at), "登了两行：{}", s.at);
        assert_ne!(s.place, Place::Backend, "{}：客户端那几处不住后端", s.at);
    }
}

/// 声明的「超了怎么办」与读口真做的对得上：自有文件全经后端 `own_state` 读，超了当读不出来 ——
/// 状态照实回错、不覆盖（拒收＋回错），缓存 `warn!` 带路径、当没有（跳过＋说清）。别的档在这张表里说的是假话。
#[test]
fn every_declared_cap_says_what_the_reader_does_on_overflow() {
    for r in OWN_FILES {
        let want = match r.kind() {
            Kind::State => Over::Reject,
            Kind::Cache => Over::SkipAndSay,
        };
        assert_eq!(r.cap.over, want, "{}：主人类 {:?}", r.rel, r.owner);
    }
}

/// `X.Y.Z` ⇒ 三个数；不是这一形 ⇒ 没有。
fn release_of(s: &str) -> Option<(u32, u32, u32)> {
    let mut it = s.trim().split('.').map(|p| p.parse::<u32>().ok());
    let v = (it.next()??, it.next()??, it.next()??);
    it.next().is_none().then_some(v)
}

/// `package.json` 的 version（打版本号改的就是它）。
fn current_release() -> (u32, u32, u32) {
    let pkg = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../package.json");
    let text = std::fs::read_to_string(&pkg).expect("package.json 读不动");
    let line = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("\"version\":"))
        .expect("package.json 里没有 version");
    release_of(line.trim().trim_end_matches(',').trim_matches('"')).expect("version 不是 X.Y.Z")
}

/// 升级步都有到期那一版：下限低于版本的每一行写着「下限抬于 X.Y.Z」；`package.json` 的 version 到了那一版还没抬 ⇒ 红
/// （红了的那一路删这份的升级步与它们的测试、下限抬成版本、去掉这一格）。
#[test]
fn every_upgrade_step_has_a_release_that_drops_it() {
    let now = current_release();
    for r in OWN_FILES {
        let Format::Json { version, floor } = r.format else {
            assert_eq!(
                r.floor_raised_in, None,
                "{}：不带版本的行没有下限可抬",
                r.rel
            );
            continue;
        };
        if floor == version {
            assert_eq!(
                r.floor_raised_in, None,
                "{}：下限已等于版本，抬于那一格要去掉",
                r.rel
            );
            continue;
        }
        let Some(at) = r.floor_raised_in else {
            panic!("{}：下限 {floor} 低于版本 {version}，没写下限抬于哪一版（`.floor_raised_in(\"X.Y.Z\")`）", r.rel);
        };
        let due = release_of(at).unwrap_or_else(|| panic!("{}：抬于 `{at}` 不是 X.Y.Z", r.rel));
        assert!(
            now < due,
            "{}：版本已到 {}.{}.{}，下限抬于 {at} 到期：删第 {floor} 版起的升级步与它们的测试，下限抬成 {version}",
            r.rel,
            now.0,
            now.1,
            now.2
        );
    }
}

/// 家里那几样里有声明的那几份，「删了会丢」照声明的种类（一处事实）；定下的那两处：中转钥匙是缓存、资产目录是状态。
#[test]
fn home_entries_with_a_declaration_take_their_class_from_it() {
    let mut checked = 0;
    for &(id, rel, dir, kept) in OWN_HOME_ENTRIES {
        if let Some(r) = OWN_FILES.iter().find(|r| r.rel == rel) {
            assert!(!dir, "{id}：声明的是文件，家里那一格写成了目录");
            assert_eq!(
                kept,
                r.kind() == Kind::State,
                "{id}：家里那一格与声明的种类对不上"
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 15,
        "家里那几样只对上 {checked} 份声明：下面的比较是空真"
    );
    let kept_of = |id: &str| OWN_HOME_ENTRIES.iter().find(|e| e.0 == id).map(|e| e.3);
    assert_eq!(kept_of("relayKey"), Some(false));
    assert_eq!(kept_of("assetCatalog"), Some(true));
    assert_eq!(KEY_FILE.row.format, Format::Foreign);
    assert_eq!(KNOWN_HOSTS_FILE.row.format, Format::Foreign);
}

/// ★ 能当一个号的 id（quota-warm · quota-probe · 轮换同问这一处）：过段闸、又不是「没说是哪个号」那一段。
#[test]
fn an_account_id_passes_the_gate_and_is_not_the_undeclared_segment() {
    assert_eq!(UNDECLARED_ACCOUNT_SEGMENT, "_");
    assert!(
        segment_is_safe(UNDECLARED_ACCOUNT_SEGMENT),
        "`_` 本身是合法段"
    );
    assert!(!account_ok(UNDECLARED_ACCOUNT_SEGMENT));
    for bad in ["", "../x", "a/b", "a b"] {
        assert!(!account_ok(bad), "{bad:?}");
    }
    for ok in ["a", "0", "team-2", "a_b"] {
        assert!(account_ok(ok), "{ok:?}");
    }
}

// ═══════ 家目录 · 数据目录：哪个环境变量算家、数据目录怎么推，两侧一条规矩 ═══════════════════

/// 注入的环境（`(变量, 值)`；不在表里 ⇒ 没设）。
fn env_of(
    pairs: &'static [(&'static str, &'static str)],
) -> impl Fn(&str) -> Option<std::ffi::OsString> {
    move |k| {
        pairs
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| (*v).into())
    }
}

/// ★ 要求：「规则按平台惯例 —— **Windows：`USERPROFILE` → `HOME`；其余：`HOME` → `USERPROFILE`**…
/// 都空 ⇒ `None`、调用方明说」。两臂各喂同一组注入环境，期望手写；空串当没设。
#[test]
fn the_home_is_picked_by_the_platform_convention() {
    let p = |s: &str| Some(std::path::PathBuf::from(s));
    let both: &[(&str, &str)] = &[("USERPROFILE", r"C:\Users\u"), ("HOME", "/home/u")];
    let only_home: &[(&str, &str)] = &[("HOME", "/home/u")];
    let only_profile: &[(&str, &str)] = &[("USERPROFILE", r"C:\Users\u")];
    let empty_first_win: &[(&str, &str)] = &[("USERPROFILE", ""), ("HOME", "/home/u")];
    let empty_first_other: &[(&str, &str)] = &[("HOME", ""), ("USERPROFILE", r"C:\Users\u")];
    let blank: &[(&str, &str)] = &[("HOME", ""), ("USERPROFILE", "")];
    let cases: [(&'static [(&str, &str)], bool, Option<std::path::PathBuf>); 10] = [
        (both, true, p(r"C:\Users\u")),
        (only_home, true, p("/home/u")),
        (empty_first_win, true, p("/home/u")),
        (blank, true, None),
        (&[], true, None),
        (both, false, p("/home/u")),
        (only_profile, false, p(r"C:\Users\u")),
        (empty_first_other, false, p(r"C:\Users\u")),
        (blank, false, None),
        (&[], false, None),
    ];
    for (env, windows, want) in cases {
        assert_eq!(
            home_dir_on(&env_of(env), windows),
            want,
            "env {env:?} · windows={windows}"
        );
    }
    // 本进程这一臂：`home_dir_from` 取的是本平台那一格（门禁在 Linux 上跑 ⇒ 非 Windows 那一臂）。
    #[cfg(not(windows))]
    assert_eq!(home_dir_from(&env_of(both)), p("/home/u"));
}

/// 数据目录：`CCM_DATA_DIR`（非空且绝对）照用；空串当没设 ⇒ `<家>/.cc-monitor`；设了却不是绝对路径 ⇒ `None`（不退回真家）；没家又没设 ⇒ `None`。
#[test]
fn the_data_dir_follows_the_override_else_the_home() {
    use std::path::PathBuf;
    assert_eq!(DATA_DIR_ENV, "CCM_DATA_DIR");
    let home = || Some(PathBuf::from("/home/u"));
    assert_eq!(
        monitor_data_dir(None, home()),
        Some(PathBuf::from("/home/u/.cc-monitor"))
    );
    assert_eq!(
        monitor_data_dir(Some("  "), home()),
        Some(PathBuf::from("/home/u/.cc-monitor"))
    );
    // 绝对路径照这台平台的写法（Windows 上 `/iso/data` 没有盘符，不算绝对）。
    let abs = if cfg!(windows) {
        r"C:\iso\data"
    } else {
        "/iso/data"
    };
    assert_eq!(
        monitor_data_dir(Some(abs), home()),
        Some(PathBuf::from(abs))
    );
    assert_eq!(monitor_data_dir(Some("rel/data"), home()), None);
    assert_eq!(monitor_data_dir(None, None), None);
    // 默认那一形与布局表同一个根（`.cc-monitor/` 开头的每一个 `*_REL`）。
    assert!(LISTEN_DIR_REL.starts_with(".cc-monitor/"));
}

/// 「文件与数据」里后端列的家里那几样，含历史注解与 API key 表（落点照声明表那一行，后端逐样 stat，壳不另列）。
#[test]
fn home_entries_list_the_history_annotations_and_the_api_key_table() {
    let rel_of = |id: &str| OWN_HOME_ENTRIES.iter().find(|e| e.0 == id).map(|e| e.1);
    assert_eq!(
        rel_of("historyMetadata"),
        Some(HISTORY_ANNOTATIONS_FILE.row.rel)
    );
    assert_eq!(rel_of("apikey"), Some(APIKEY_CREDENTIALS_FILE.row.rel));
    assert_eq!(
        file_name_of(HISTORY_ANNOTATIONS_REL),
        "history-metadata.json"
    );
    assert_eq!(
        file_name_of(APIKEY_CREDENTIALS_REL),
        "apikey-credentials.json"
    );
}
