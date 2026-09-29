//! 设计/99 §2.1 ⑬「`list_local_tmux` / `list_remote_tmux`：`tmux-list` 出成品、`parse_tmux_ls`〔散文墓碑〕 进后端」—— `observe/tmux_list.rs` 的解析判据
//! （从 monitor `tmux_tests.rs` 那四条原样搬来：多会话 · 残行 / 边角 · windows 回退 · `@ccm_sid` 字符集）＋ 成品形状（跨语言金样）。
use super::*;

#[test]
fn parse_multi_session() {
    // 真 TAB 分隔(Rust "\t" = 0x09)。6 列,末列 @ccm_sid。
    let s = rows("cc-abc12345\t/home/pi/proj\tclaude\t1\t2\tsess-42\nweb\t/srv/web\tzsh\t0\t1\t\n");
    assert_eq!(s.len(), 2);
    assert_eq!(
        (
            s[0].name.as_str(),
            s[0].path.as_str(),
            s[0].command.as_str()
        ),
        ("cc-abc12345", "/home/pi/proj", "claude")
    );
    assert!(s[0].attached);
    assert_eq!(s[0].windows, 2);
    // @ccm_sid 有值 → Some;空串 → None(老会话)。
    assert_eq!(s[0].sid.as_deref(), Some("sess-42"));
    assert!(!s[1].attached);
    assert_eq!(s[1].sid, None);
}

#[test]
fn parse_skips_malformed_and_handles_edges() {
    assert!(rows("").is_empty());
    assert!(rows("\n\n").is_empty());
    // 字段数不符(无 TAB / 少字段 / 旧 5 列)→ 跳过;name 空 → 跳过。
    let s = rows("no tabs here\nn\t/p\tsh\t0\n\t/p\tclaude\t1\t1\told5\t/p\tclaude\t1\t2\ngood\t/home/a b\tclaude\t1\t3\t");
    assert_eq!(s.len(), 1, "只有最后一行(6 列)合法");
    assert_eq!(
        (s[0].name.as_str(), s[0].path.as_str(), s[0].windows),
        ("good", "/home/a b", 3)
    );
    assert_eq!(s[0].sid, None);
}

#[test]
fn parse_windows_nonnumeric_falls_back_zero() {
    let s = rows("n\t/p\tclaude\t1\tNaN\tsid-x");
    assert_eq!((s[0].windows, s[0].sid.as_deref()), (0, Some("sid-x")));
}

#[test]
fn parse_sid_rejects_unexpanded_format_and_garbage() {
    // 极老 tmux 不展开 `#{@ccm_sid}` → 原样字面串(含 `#{}`)→ 当 None（`INVARIANTS §30`）。
    assert_eq!(
        rows("n\t/p\tclaude\t1\t1\t#{@ccm_sid}")[0].sid,
        None,
        "未展开格式串不当 sid"
    );
    assert_eq!(
        rows("n\t/p\tclaude\t1\t1\tab_c-12")[0].sid.as_deref(),
        Some("ab_c-12")
    );
}

/// 成品的线上形状 == 跨语言金样（界面 `src/frontend/ui/tmux-reads.ts::decodeTmuxList` 严格收的就是这一份）。
#[test]
fn the_product_matches_the_cross_language_golden() {
    let golden: Value =
        serde_json::from_str(include_str!("../../__fixtures__/tmux-list.golden.json")).unwrap();
    assert_eq!(
        product(
            true,
            "proj-cc\t/home/u/proj\tclaude\t1\t2\tsid-1\nweb\t/srv\tzsh\t0\t1\t"
        ),
        golden["installed"]
    );
    assert_eq!(product(false, ""), golden["notInstalled"]);
}

/// ★★ **K-R12 `J1` 死值（从 monitor 那一侧搬来）：段数下溢的行不许当好数据**；过溢那格照旧整行丢（`§5.4` 点名的误伤，刻意没改）。
/// 死值取自 `tests/evidence/K-R12-deathvalue.md` ①/S5：真 tmux 3.4 ＋ POSIX 客户端，六列塌成 1 段。
///
/// 〔IV1 · V121〕要求住址：`INVARIANTS §49`（tmux 打印通道必须是 UTF-8，段数下溢出声）。
#[test]
fn a_dirty_line_underflows_and_an_overflowing_line_is_still_dropped_today() {
    const DIRTY: &str = "kr12_/tmp/kr12dv/____/proj_bash_0_1_cc-deadval1";
    const CLEAN: &str = "kr12\t/tmp/kr12dv/文档/proj\tbash\t0\t1\tcc-deadval1";
    const OVERFLOW: &str = "kr12\t/tmp/a\tb\tbash\t0\t1\tcc-deadval1";
    assert!(rows(DIRTY).is_empty(), "脏行不许进结果");
    let ok = rows(CLEAN);
    assert_eq!(ok.len(), 1, "干净行必须解析出来（正对照）");
    assert_eq!(
        (ok[0].path.as_str(), ok[0].sid.as_deref()),
        ("/tmp/kr12dv/文档/proj", Some("cc-deadval1"))
    );
    assert!(
        rows(OVERFLOW).is_empty(),
        "⚠ 现状：过溢的行今天照样被整行丢掉 —— K-R12 §5.4 点名的误伤；修它的那一拍会让本条红，那是对的"
    );
}
