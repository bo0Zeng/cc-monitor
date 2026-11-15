//! 换号那一族帧面宿主自己接的几处：`quota-read` 照额度账交显示态（有账 · 没账 · 读不懂各一形）· 交给换号的 history 那两口
//! （`HistoryPorts` 的 `live` · `ring`）各接到历史那一侧 · 交给中转的号表读法（`books`）照给的家读、接上「账号 0 排第一」（那一判本体钉在 `acct-core` 的 `lib_tests.rs`）。
//! 登录盯防那一份号目录读法（`account_dirs`）钉在 `tests/backend/accounts/on_upper_tests.rs`（号的登录文件一变就来一件），这里不另钉；
//! 显示态逐格（快满 · 被拒 · 没出过数的号 · 候选）钉在 `rotation_view_face_tests.rs` 的 `quota_read_*` 那两条。

use super::*;
use crate::accounts::quota::ledger;
use crate::faces::rotation_view_face::tests::{now, seen, Home};
use cmd_wire::quota::ReadState;

fn read(ctx: &Ctx) -> QuotaRead {
    quota_read_with(ctx, now(), &crate::common::time::Tz::default())
}

fn unseen_ids(q: &QuotaRead) -> Vec<String> {
    q.unseen.iter().map(|u| u.head.account.clone()).collect()
}

/// ★ 额度账三形各交一形：有账 ⇒ `present`、出过数的号各一行；还没有那份账 ⇒ `absent`、账号库里的号全在「没出过数」里；
/// 账读不懂 ⇒ `unreadable` ＋ 为什么，号照样列在「没出过数」里，不当一次失败。
#[test]
fn quota_read_hands_each_ledger_state_as_its_own_shape() {
    let home = Home::new("qr-absent");
    let ctx = home.ctx();
    let q = read(&ctx);
    assert_eq!(q.state, ReadState::Absent, "还没有那份账");
    assert!(q.accounts.is_empty() && q.reason.is_none(), "{q:?}");
    assert_eq!(
        unseen_ids(&q),
        ["a", "b", "c", "api"],
        "账号库里的号都没出过数"
    );

    let home = Home::new("qr-present");
    let ctx = home.ctx();
    seen(&ctx, "b", 0.3, None);
    let q = read(&ctx);
    assert_eq!(q.state, ReadState::Present);
    let rows: Vec<&str> = q.accounts.iter().map(|a| a.head.account.as_str()).collect();
    assert_eq!(rows, ["b"], "出过数的那个号一行");
    assert_eq!(unseen_ids(&q), ["a", "c", "api"]);
    assert!(q.reason.is_none());

    let home = Home::new("qr-unreadable");
    let ctx = home.ctx();
    std::fs::write(home.root.join(ledger::FILE_NAME), b"{ not json").expect("写坏账");
    let q = read(&ctx);
    assert_eq!(q.state, ReadState::Unreadable);
    assert!(
        q.reason.as_deref().is_some_and(|r| !r.is_empty()),
        "读不懂要说为什么：{q:?}"
    );
    assert!(q.accounts.is_empty());
    assert_eq!(unseen_ids(&q), ["a", "b", "c", "api"]);
    assert_eq!(
        q.path.as_deref(),
        Some(
            home.root
                .join(ledger::FILE_NAME)
                .display()
                .to_string()
                .as_str()
        ),
        "说的是哪一份"
    );
}

/// ★ 交给换号的 `ring` 就是历史那一侧需手动的铃：敲一下，订那一声的收到一件整份重问（不带会话 id）。
#[test]
fn the_ring_port_rings_the_needs_bell() {
    let mut rx = crate::history::needs_changed::moved_rx();
    (history_ports().ring)();
    let mut got = Vec::new();
    while let Ok(m) = rx.try_recv() {
        got.push(m);
    }
    assert!(got.contains(&None), "敲了没响：{got:?}");
}

/// ★ 交给换号的 `live` 就是历史那一侧这台的活会话表：替身家（测试档的沙箱家）的 pidfile 目录里放一份「本进程持着某条会话」，
/// 读出来有它；挪走 ⇒ 没有。
#[cfg(unix)]
#[test]
fn the_live_port_reads_this_machines_live_table() {
    const SID: &str = "rotface-live-4e1d7c";
    let _tree = crate::history::history_list::test_support::MACHINE_TREE.blocking_lock();
    let dir = crate::agents::local_homes()[0].pidfile_dir().unwrap();
    std::fs::create_dir_all(&dir).expect("建 pidfile 目录");
    let me = std::process::id();
    let ticks = crate::platform::proc::proc_starttime(me).expect("读得到自己的启动时刻");
    let pf = dir.join(format!("{me}.json"));
    std::fs::write(
        &pf,
        format!(r#"{{"sessionId":"{SID}","cwd":"/w","procStart":"{ticks}"}}"#),
    )
    .expect("写 pidfile");
    let live = history_ports().live;
    let with = live();
    std::fs::remove_file(&pf).ok();
    let without = live();
    assert!(
        with.contains(SID),
        "活会话表里有它，换号那一口读不到：{with:?}"
    );
    assert!(
        !without.contains(SID),
        "pidfile 挪走了还读得到：{without:?}"
    );
}

/// ★ 交给中转的号表读法（`books`）：照给的家读账号库，换号那一侧账号 0 恒在、排第一个（清单里没写那一条 ⇒ 补上）；没建账号库 ⇒ 没有。
#[test]
fn books_reads_the_library_at_that_home_with_the_bare_account_first() {
    let home = test_home::scratch("rotbooks");
    let read = books();
    assert_eq!(read(&home), None, "没建账号库");
    crate::faces::launch_account_face::tests::library(&home, &[("work", None)]);
    let names: Vec<String> = read(&home)
        .expect("读得到账号库")
        .accounts
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert_eq!(names, [acct_core::BARE_ACCOUNT, "work"]);
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ `Ctx::here`：额度账与轮换账本都落在这台数据目录（`data_home`）底下、配置编辑层问的就是同一份账本。
/// key 表那一格怎么答钉在 [`at_takes_each_landing_it_was_handed`]（测试档这台的那份表是空的）。
#[test]
fn here_takes_the_data_dir_of_this_machine() {
    use crate::accounts::quota::rotation;
    let ctx = Ctx::here();
    let data = crate::platform::paths::data_home().expect("测试档有数据目录");
    assert_eq!(
        ctx.hop.quota.path(),
        Some(ledger::path_under(&data).as_path()),
        "额度账住这台数据目录"
    );
    assert_eq!(
        ctx.hop.store.path(),
        Some(rotation::path_under(&data).as_path()),
        "轮换账本住这台数据目录"
    );
    assert!(
        Arc::ptr_eq(&ctx.setup.book, &ctx.hop.store),
        "配置编辑层问的不是换号那一份账本"
    );
}

/// ★ `Ctx::row`：绑好那一家，问的就是 key 表那一家那一行 —— 同一个号名，另一家照另一家答。
#[test]
fn row_binds_the_family_it_was_given() {
    let home = crate::faces::rotation_view_face::tests::Home::new("ctx-row");
    let mut ctx = home.ctx();
    let claude = crate::agents::claudecode::AGENT_KIND;
    let codex = crate::agents::codex::AGENT_KIND;
    ctx.rows = Box::new(move |agent, a| match (agent, a) {
        (x, "k") if x == claude => Some(true),
        (x, "k") if x == codex => Some(false),
        _ => None,
    });
    assert_eq!(ctx.row(claude)("k"), Some(true));
    assert_eq!(ctx.row(codex)("k"), Some(false), "另一家照另一家答");
    assert_eq!(ctx.row(claude)("other"), None);
}

/// ★ `quota-read` 那条命令：读的就是这台此刻那一份额度账（这台数据目录底下那份 · 此刻的钟），不是别处的、也不是钉死的一刻。
#[test]
fn the_quota_read_command_reads_this_machines_ledger_now() {
    use crate::wire::typed::Handle;
    let before = crate::common::time::now_secs();
    let q = crate::wire::spec::finish(QuotaReadCmd::run(
        Default::default(),
        crate::wire::typed::Cx::default(),
    ))
    .expect("quota-read 没有失败那一形");
    let after = crate::common::time::now_secs();
    let data = crate::platform::paths::data_home().expect("测试档有数据目录");
    assert_eq!(
        q.path.as_deref(),
        Some(ledger::path_under(&data).display().to_string().as_str()),
        "读的是这台数据目录底下那一份"
    );
    assert!(
        (before..=after).contains(&q.now),
        "此刻的钟：{before}..={after}，交的是 {}",
        q.now
    );
}

/// key 表夹具：一份只有 `ids` 那几行的凭据文件（夹具 key 是结构样本、不是真 key；端点写 https，装表不丢这一行），只给本人读写。
pub(crate) fn key_table(path: &std::path::Path, ids: &[&str]) {
    let accounts: serde_json::Map<String, serde_json::Value> = ids
        .iter()
        .map(|id| {
            (
                id.to_string(),
                serde_json::json!({
                    "api_key": format!("sk-FIXTURE-{id}-NOT-A-REAL-KEY"),
                    "base_url": "https://up.example.invalid/v1",
                }),
            )
        })
        .collect();
    std::fs::create_dir_all(path.parent().expect("表所在的那一层")).expect("建表目录");
    std::fs::write(
        path,
        serde_json::json!({ "accounts": accounts }).to_string(),
    )
    .expect("写 key 表");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).expect("只给本人");
    }
}

/// ★ `Ctx::at`：三处落点都照交进来的 —— 额度账 · 轮换账本住交进来的数据目录（配置编辑层问同一份账本）；账号库照交进来的家读；
/// key 表照交进来的那一份答：表里那一家那一行 ⇒ 接得上，表里没有的号 · 别家同名 ⇒ 没有那一行；没交 key 表 ⇒ 一行都没有。
#[test]
fn at_takes_each_landing_it_was_handed() {
    use crate::accounts::quota::rotation;
    let root = test_home::scratch("ctxat");
    let (data, home, keys) = (root.join("data"), root.join("home"), root.join("keys.json"));
    crate::faces::launch_account_face::tests::library(&home, &[("api1", None)]);
    key_table(&keys, &["api1"]);
    let table = crate::accounts::upstream_select::file_face::rows_at(&keys);
    assert_eq!(table.ids, ["api1"], "夹具那份表装得上");

    let ctx = Ctx::at(Some(&data), Some(home.clone()), Some(&keys));
    assert_eq!(
        ctx.hop.quota.path(),
        Some(ledger::path_under(&data).as_path()),
        "额度账住交进来的数据目录"
    );
    assert_eq!(
        ctx.hop.store.path(),
        Some(rotation::path_under(&data).as_path()),
        "轮换账本住交进来的数据目录"
    );
    assert!(Arc::ptr_eq(&ctx.setup.book, &ctx.hop.store));
    let names: Vec<String> = ctx
        .hop
        .book()
        .accounts
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert_eq!(
        names,
        [acct_core::BARE_ACCOUNT, "api1"],
        "账号库照交进来的家读"
    );
    assert_eq!((ctx.rows)(table.agent, "api1"), Some(true), "表里那一行");
    assert_eq!((ctx.rows)(table.agent, "no-such-row"), None, "表里没有的号");
    assert_eq!((ctx.rows)("nobody-family", "api1"), None, "别家同名");

    let bare = Ctx::at(Some(&data), Some(home), None);
    assert_eq!(
        (bare.rows)(table.agent, "api1"),
        None,
        "没交 key 表 ⇒ 一行都没有"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// ★ `quota_read_now`（`quota-read` 那条命令交给它的就是请求信封）：时刻字照信封的时区写、钟是此刻的 ——
/// 与照同一刻 · 同一个时区问 [`quota_read_with`] 同一份，与照 UTC 问的不是同一份（这份账里有带时刻的字）。
#[test]
fn quota_read_now_writes_in_the_envelopes_zone_at_this_moment() {
    let home = Home::new("qr-now");
    let ctx = home.ctx();
    seen(&ctx, "b", 0.3, None);
    let tz = crate::common::time::Tz::named("Asia/Shanghai").expect("时区");
    let before = crate::common::time::now_secs();
    let q = quota_read_now(
        &ctx,
        &crate::wire::typed::Cx {
            tz: tz.clone(),
            ..Default::default()
        },
    );
    let after = crate::common::time::now_secs();
    let at = q.now;
    assert!((before..=after).contains(&at), "此刻的钟：{at}");
    let v = |q: QuotaRead| serde_json::to_value(q).expect("成品");
    let shanghai = v(quota_read_with(&ctx, at, &tz));
    assert_eq!(v(q), shanghai, "照信封的时区");
    assert_ne!(
        shanghai,
        v(quota_read_with(
            &ctx,
            at,
            &crate::common::time::Tz::default()
        )),
        "这份账里没有带时刻的字，钉不住时区"
    );
}
