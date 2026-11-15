//! 计划读面的帧面宿主：本子只起一次 pb · 交出去的那几句（一格的 agent 视角点到才起 pb：`dump_tests` 里真起假 pb 判）。
//! 走登记表问的参数形状那几条住 `on_upper_tests`（读命令表 ⇒ 挂主 crate）。

use super::*;
use serde_json::json;

/// `plan-list` 那一条照 [`reads_in`] 读、再拼成清单：判据换成替身数起了几次 pb。
fn list_in(
    b: &book::Book,
    dirs: Vec<String>,
    fresh: bool,
    read: &dyn Fn(&Path) -> Result<wire::PlanRead, book::Miss>,
    tz: &crate::common::time::Tz,
) -> wire::PlanList {
    let (pb, docs) = reads_in(b, dirs, fresh, read, tz);
    list_of(pb, &docs)
}

/// ★ 不重问时（`plan-list {}` · `sessions-needs` 计划那一截）同一个目录只起一次 pb：认出是工作区 · 不是工作区 · 读不成（pb 失败 · 不认得）
/// 都记在本子里，连问两次第二次照记下的那个结局；读不成的那一种照旧说 pb 不认得。`fresh` ⇒ 重起。
#[test]
fn an_unfresh_list_runs_pb_once_per_dir_whatever_it_said() {
    use crate::plan::dump::Ran;
    let tz = crate::common::time::Tz::default();
    type Case = (&'static str, fn() -> Ran);
    let cases: [Case; 3] = [
        ("不是工作区", || Ran::NotWorkspace("x".into())),
        ("读不成", || Ran::Failed {
            said: "坏了".into(),
            raw: None,
        }),
        ("不认得", || Ran::Unsupported("老 pb".into())),
    ];
    for (name, ran) in cases {
        let b = crate::plan::book::Book::default();
        let runs = std::cell::Cell::new(0);
        let read = |dir: &Path| {
            runs.set(runs.get() + 1);
            b.ingest(ran(), dir, &crate::plan::fixture::who, 1)
        };
        let first = list_in(&b, vec!["/d".into()], false, &read, &tz);
        let again = list_in(&b, vec!["/d".into()], false, &read, &tz);
        assert_eq!(runs.get(), 1, "{name}：同一个目录连问两次只起一次 pb");
        assert_eq!(
            serde_json::to_value(&first).unwrap(),
            serde_json::to_value(&again).unwrap(),
            "{name}：第二次照记下的结局"
        );
        list_in(&b, vec!["/d".into()], true, &read, &tz);
        assert_eq!(runs.get(), 2, "{name}：fresh ⇒ 重起");
        if name == "不认得" {
            // 那一句（要更新）与为什么（pb 说的那一半）分两格，同 `plan-new` 那一份。
            assert_eq!(
                first.pb,
                crate::plan::locate::pb_unsupported("老 pb".into())
            );
            assert_eq!(first.pb.hint.as_deref(), Some("老 pb"));
        }
    }
}

/// ★ `plan-read` 照盯盘那一路给：已在盯、追上了这期间的动静 ⇒ 本子里那一份，不起 pb；没在盯 ⇒ 起 pb；
/// 计划审面送退回那一下要此刻的接手（现读）⇒ 一定起 pb。三种都经同一个函数。
#[test]
fn a_read_takes_the_watched_copy_unless_asked_to_read_now() {
    use crate::plan::dump::Ran;
    use crate::plan::fixture::{dump, who};
    let tz = crate::common::time::Tz::default();
    let b = book::Book::default();
    let runs = std::cell::Cell::new(0);
    let read = |dir: &Path| -> Result<wire::PlanRead, Said> {
        runs.set(runs.get() + 1);
        let doc = dump(&dir.to_string_lossy());
        let raw = serde_json::to_vec(&doc).unwrap();
        b.ingest(Ran::Dump { doc, raw }, dir, &who, 1)
            .map(|d| annotated(d, &tz))
            .map_err(miss)
    };
    let ws = Path::new("/w");
    let watched = |_: &Path| true;
    let unwatched = |_: &Path| false;
    // 没读过、没在盯 ⇒ 起 pb。
    read_in(&b, ws, Fresh::Kept, &unwatched, &read, &tz).unwrap();
    assert_eq!(runs.get(), 1);
    // 盯着、追上了 ⇒ 本子里那一份。
    let kept = read_in(&b, ws, Fresh::Kept, &watched, &read, &tz).unwrap();
    assert_eq!(runs.get(), 1, "已在盯又没动静：不该再起 pb");
    assert_eq!(kept.workspace, "/w");
    assert!(
        kept.slices.iter().all(|sl| sl.said.read_at.is_some()),
        "本子那一份也照常标时刻的字"
    );
    // 没在盯 ⇒ 起 pb。
    read_in(&b, ws, Fresh::Kept, &unwatched, &read, &tz).unwrap();
    assert_eq!(runs.get(), 2);
    // 现读 ⇒ 盯着也起 pb。
    read_in(&b, ws, Fresh::Now, &watched, &read, &tz).unwrap();
    assert_eq!(runs.get(), 3, "送退回要此刻的接手：一定现读");
}

/// ★ 手加的目录住这台的偏好：`prefs-set {planDirs:[X]}` 打上之后，`plan-list` 与需手动的计划那一截问的那一组（[`ask_dirs`]）都含 X，
/// 活会话的工作目录照旧在。
#[test]
fn hand_added_dirs_live_in_this_machines_prefs() {
    use crate::control::machine_prefs::{patched, PrefsSet};
    let set: PrefsSet =
        serde_json::from_value(json!({ "planDirs": ["/hand/x"] })).expect("prefs-set 认 planDirs");
    let p = patched(crate::common::machine_prefs::Prefs::default(), &set);
    let dirs = ask_dirs(vec!["/live/a".into()], &p);
    assert!(dirs.iter().any(|d| d == "/hand/x"), "{dirs:?}");
    assert!(dirs.iter().any(|d| d == "/live/a"), "{dirs:?}");
}

/// 「谁」那一步认一个 id 的判法（[`whose_in`]）：喂假名单 · 假的活会话表与记录树。
mod whose {
    use super::*;
    use crate::agents::SessionActivity;
    use crate::host::{Housing, Input, TermRow};
    use crate::observe::accounts_query::Doing;
    use crate::wire::{SessionContainer, TerminalHost};

    fn row(sid: &str, handle: &str) -> TermRow {
        TermRow {
            host: TerminalHost::Managed,
            id: handle.into(),
            pane: None,
            shared: false,
            name: format!("n-{handle}"),
            windows: 1,
            activity: 0,
            sid: sid.into(),
            agent: "fake".into(),
            client: String::new(),
            program: "fakeagent".into(),
            cwd: "/".into(),
            title: String::new(),
            input: Input::Held(None),
        }
    }

    fn at(handle: &str) -> Option<SessionContainer> {
        Some(SessionContainer::Managed {
            terminal: Some(handle.into()),
        })
    }

    /// 活着的只有 `s-live`；`agent-1` 是它底下的子 agent（名单里另挂一行，认错 id 就住到 `h-child`）；`s-old` 有记录、不活。
    fn ask(id: &str, list: &dyn Fn() -> Vec<TermRow>) -> Whose {
        let doing = BTreeMap::from([(
            "s-live".to_string(),
            Doing {
                activity: Some(SessionActivity::Working),
                needs: None,
            },
        )]);
        let recorded = |sid: &str| sid == "s-live" || sid == "s-old";
        let parent = |child: &str| (child == "agent-1").then(|| "s-live".to_string());
        let title = |sid: &str| format!("t-{sid}");
        let facts = WhoFacts {
            doing: &doing,
            recorded: &recorded,
            parent: &parent,
            title: &title,
        };
        whose_in(id, &facts, &Housing::with(list))
    }

    fn list() -> Vec<TermRow> {
        vec![
            row("s-other", "h-other"),
            row("s-live", "h-live"),
            row("agent-1", "h-child"),
        ]
    }

    fn live_at(handle: &str) -> Option<Live> {
        Some(Live {
            activity: Some(SessionActivity::Working),
            needs: None,
            housed: at(handle),
        })
    }

    /// ★ 活会话住哪照名单里挂着它的那一行；名单里不挂 ⇒ 不知道。
    #[test]
    fn a_live_session_is_housed_where_the_list_carries_it() {
        assert_eq!(
            ask("s-live", &list),
            Whose::Session {
                sid: "s-live".into(),
                title: "t-s-live".into(),
                live: live_at("h-live"),
            }
        );
        let elsewhere = || vec![row("s-other", "h-other")];
        assert_eq!(
            ask("s-live", &elsewhere),
            Whose::Session {
                sid: "s-live".into(),
                title: "t-s-live".into(),
                live: Some(Live {
                    activity: Some(SessionActivity::Working),
                    needs: None,
                    housed: None,
                }),
            }
        );
    }

    /// ★ 子 agent 活不活 · 住哪都照父会话问（不拿它自己的 id 去查名单）。
    #[test]
    fn a_subagent_is_housed_where_its_parent_is() {
        assert_eq!(
            ask("agent-1", &list),
            Whose::Subagent {
                parent: "s-live".into(),
                parent_title: "t-s-live".into(),
                live: live_at("h-live"),
            }
        );
    }

    /// 有记录、不活 ⇒ 会话，不问住哪（名单一次都不列）；都不中 ⇒ 认不出。
    #[test]
    fn a_dead_session_never_lists_and_a_stranger_is_unknown() {
        let calls = std::cell::Cell::new(0u32);
        let counted = || {
            calls.set(calls.get() + 1);
            list()
        };
        assert_eq!(
            ask("s-old", &counted),
            Whose::Session {
                sid: "s-old".into(),
                title: "t-s-old".into(),
                live: None,
            }
        );
        assert_eq!(ask("nobody", &counted), Whose::Unknown);
        assert_eq!(calls.get(), 0, "没有活会话要问 ⇒ 不列名单");
    }
}

/// 手加的目录照其它几张表的值域：相对路径 · 空串 · 超过上限条数都不收（`prefs-set` 回 `bad_args`，盘上的当读不懂）。
#[test]
fn hand_added_dirs_must_be_absolute_and_bounded() {
    use crate::common::machine_prefs::{check, Prefs, PREFS_ROWS_MAX};
    let with = |dirs: Vec<String>| Prefs {
        plan_dirs: dirs,
        ..Prefs::default()
    };
    assert!(check(&with(vec!["/a".into()])).is_ok());
    assert!(check(&with(vec!["rel/a".into()])).is_err());
    assert!(check(&with(vec![" ".into()])).is_err());
    let many = (0..=PREFS_ROWS_MAX).map(|i| format!("/d{i}")).collect();
    assert!(check(&with(many)).is_err());
}

/// 交出去的那一份：随数据变的那几句后端照文案表写好（`said`），两端不再拼。
mod said {
    use super::*;
    use crate::plan::fixture::{dump, dump_broken, who, MAIN, STRANGER};
    use crate::plan::product::make;

    fn out(d: &serde_json::Value, stale: bool) -> wire::PlanRead {
        let mut doc = make(d, &who, "r0".into(), 0);
        if stale {
            doc.stale = Some(wire::PlanStale {
                said: Some("pb 那一句".into()),
                raw: None,
                since: 0,
            });
        }
        annotated(doc, &Default::default())
    }

    fn t(key: &str, args: &[(&str, &str)]) -> String {
        copy_text(key, args)
    }

    fn sep() -> String {
        t("kit.text.sep", &[])
    }

    #[test]
    fn the_slice_lines_are_written_from_its_numbers_and_time() {
        let doc = out(&dump("/w"), false);
        let sl = &doc.slices[0];
        let p = sl.progress.as_ref().expect("有收的值");
        let (d, o) = (p.done.to_string(), p.of.to_string());
        assert_eq!(
            sl.said.top.as_deref(),
            Some(t("plan.overview.top", &[("done", &d), ("of", &o)]).as_str())
        );
        assert_eq!(
            sl.said.root.as_deref(),
            Some(t("plan.outline.rootProgress", &[("done", &d), ("of", &o)]).as_str())
        );
        // 读到的时刻（夹具读在 0 那一刻）照这台此刻写成字，与成品那一句同一把钟。
        let at = crate::common::time::fmt_at(
            0,
            i64::try_from(crate::common::time::now_ms() / 1000).unwrap_or(i64::MAX),
            &Default::default(),
        );
        assert_eq!(
            sl.said.read_at.as_deref(),
            Some(t("plan.overview.readAt", &[("time", &at)]).as_str())
        );
        assert_eq!(sl.said.strip, None);
        assert_eq!(sl.said.bare, None);
    }

    #[test]
    fn a_stale_read_says_unreadable_then_since_when_in_one_line() {
        let doc = out(&dump("/w"), true);
        // 读不成那一份在 1970 年读到：照别的年那一形写（带年），与此刻几点无关。
        let since = crate::common::time::fmt_at(
            0,
            crate::common::time::now_secs() as i64,
            &Default::default(),
        );
        let want = [
            t("plan.strip.sliceUnreadable", &[("reason", "pb 那一句")]),
            t("plan.strip.stale", &[("time", &since)]),
        ]
        .join(&sep());
        assert_eq!(doc.slices[0].said.strip.as_deref(), Some(want.as_str()));
        let broken = out(&dump_broken("/w"), false);
        let sl = &broken.slices[0];
        let e = sl.error.clone().expect("读不成");
        assert_eq!(
            sl.said.bare.as_deref(),
            Some(t("plan.strip.sliceUnreadable", &[("reason", &e)]).as_str())
        );
    }

    #[test]
    fn blocks_cells_and_sessions_carry_their_written_lines() {
        let doc = out(&dump("/w"), false);
        let sl = &doc.slices[0];
        let a2 = sl.blocks.iter().find(|b| b.id == "A2").expect("A2 那一块");
        let k = a2.inside.as_ref().expect("底下有算活的");
        assert_eq!(
            a2.said.inside.as_deref(),
            Some(
                t(
                    "plan.whyShort.inside",
                    &[("done", &k.done.to_string()), ("of", &k.of.to_string())]
                )
                .as_str()
            )
        );
        let cell = |id: &str| sl.cells.iter().find(|c| c.id == id).expect("那一格");
        assert_eq!(
            Some(a2.said.name.as_str()),
            cell("A2").title.as_deref(),
            "块名是根节点的标题"
        );
        assert_eq!(
            sl.blocks[0].said.name,
            t("plan.head.top", &[]),
            "最上面那一块"
        );
        assert!(
            cell("A2").said.block_root.as_deref().is_some_and(
                |l| l.starts_with(&t("plan.cell.blockRoot", &[("block", &sl.labels["A2"])]))
            ),
            "插的是「编号 标题」"
        );
        assert_eq!(cell("A2").said.trail, None, "顶层没有上面几层");
        let kid = sl
            .cells
            .iter()
            .find(|c| c.parent.as_deref() == Some("A2"))
            .expect("A2 底下一格");
        assert_eq!(kid.said.trail.as_deref(), Some(sl.labels["A2"].as_str()));
        assert_eq!(kid.said.block_root, None);
        let mine = &doc.by_session[MAIN].said;
        assert_eq!(mine.name, t("plan.head.top", &[]));
        assert_eq!(
            mine.mark,
            [
                "alpha".to_string(),
                t("plan.head.top", &[]),
                "开发".to_string()
            ]
            .join(&sep())
        );
        let mut d = dump("/w");
        d["repos"][0]["scopes"][0]["assignee"] = json!(STRANGER);
        let doc = out(&d, false);
        let owner = doc.slices[0].blocks[0].owner.as_ref().expect("负责人");
        assert_eq!(owner.label, t("plan.who.unknown", &[("id", "999999…0000")]));
    }

    /// 列表那一行的 `said` 照抄那一片成品上的那一份（不另写一遍）。
    #[test]
    fn a_list_row_copies_the_slices_said() {
        let doc = out(&dump("/w"), false);
        let row = crate::plan::product::list_row(&doc);
        assert_eq!(row.slices[0].said, doc.slices[0].said);
        assert_eq!(row.by_session[MAIN].said, doc.by_session[MAIN].said);
    }
}

/// ★ 新计划框提交（`plan-command repo`）前照 `plan-new` 那一份拦：pb 太旧与读面同一个码 `pb_unsupported`、同一行；
/// 起 pb 没成 ⇒ `failed` 带那一句；都好 ⇒ 不拦。
#[test]
fn a_repo_is_refused_with_the_same_code_as_every_other_plan_command() {
    let form = |pb: wire::PlanPb, failed: Option<&str>| wire::PlanNewForm {
        pb,
        workspace: None,
        root: "/w".into(),
        slice: None,
        domains: Vec::new(),
        existing: None,
        repo_dir: None,
        note: None,
        line: None,
        failed: failed.map(str::to_string),
    };
    let old = repo_refused(&form(locate::pb_unsupported("缺 dump".into()), None)).unwrap();
    assert_eq!(old, locate::too_old("缺 dump".into()));
    assert_eq!(old.code.as_deref(), Some("pb_unsupported"));
    let failed = repo_refused(&form(locate::pb_ok(), Some("读计划失败"))).unwrap();
    assert_eq!(
        (failed.code.as_deref(), failed.said.as_str()),
        (Some("failed"), "读计划失败")
    );
    assert_eq!(repo_refused(&form(locate::pb_ok(), None)), None);
    let gone = locate::pb_missing(&locate::Missing::SetDir("/synthetic/pb".into()));
    let none = repo_refused(&form(gone.clone(), None)).unwrap();
    assert_eq!(none.code.as_deref(), Some("no_pb"));
    assert_eq!(
        none.said,
        format!(
            "{}{}{}",
            gone.said.unwrap(),
            copy_text("kit.text.sep", &[]),
            gone.hint.unwrap()
        )
    );
}

/// ★ 读面（`plan-read` · `plan-cell-view` …）遇上 pb 太旧交的那一条：码 `pb_unsupported`，那一行 ＝「要更新」＋ 分隔 ＋ pb 那一半。
#[test]
fn a_read_on_an_old_pb_says_update_then_why_in_one_line() {
    let got = miss(book::Miss::Unsupported("缺 dump".into()));
    assert_eq!(got.code.as_deref(), Some("pb_unsupported"));
    assert_eq!(
        got.said,
        format!(
            "{}{}缺 dump",
            copy_text("bePlan.dump.tooOld", &[]),
            copy_text("kit.text.sep", &[])
        )
    );
}
