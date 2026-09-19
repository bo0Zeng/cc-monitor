use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// 整棵 `tests/evidence/*.md` 的 `daemon` 合计地板 —— **量于 `897afec`，2026-09-14，69 份**。
///
/// 🔴 **只许涨，不许把这个数调下来让今天好过。** 调下来 = 把「有人抹掉了历史读数」
/// 这件事直接注销掉，而那正是本闸唯一要接的东西。
const EVIDENCE_DAEMON_FLOOR: usize = 1388;

/// 同上，`ccm` 那一侧（`R64` 逐字「**ccm 不改**」，它在这棵树里同样是历史读数）。
const EVIDENCE_CCM_FLOOR: usize = 442;

/// 登记过的那几份 —— `(住址, daemon 处数下界, ccm 处数下界)`，量于 `897afec`。
///
/// 🔴 表名起成 `REGISTERED` 是 `scanning_guard_registry::TABLE_DECLS` 那条纪律要的。
const REGISTERED: &[(&str, usize, usize)] = &[
    ("tests/evidence/K-P6-readings.md", 26, 2),
    ("tests/evidence/K-P6b-readings.md", 29, 0),
    ("tests/evidence/K-P7-readings.md", 82, 1),
    ("tests/evidence/K-R100-deathvalue.md", 19, 1),
    ("tests/evidence/K-R101-deathvalue.md", 20, 1),
    ("tests/evidence/K-R102-deathvalue.md", 37, 6),
    ("tests/evidence/K-R103-deathvalue.md", 7, 5),
    ("tests/evidence/K-R104-deathvalue.md", 43, 4),
    ("tests/evidence/K-R105-deathvalue.md", 10, 2),
    ("tests/evidence/K-R106-deathvalue.md", 13, 16),
    ("tests/evidence/K-R109-deathvalue.md", 5, 4),
    ("tests/evidence/K-R110-census.md", 10, 1),
    ("tests/evidence/K-R110-deathvalue.md", 16, 8),
    ("tests/evidence/K-R111-census.md", 39, 31),
    ("tests/evidence/K-R112-deathvalue.md", 41, 3),
    ("tests/evidence/K-R113-deathvalue.md", 43, 0),
    ("tests/evidence/K-R114-deathvalue.md", 16, 20),
    ("tests/evidence/K-R114-真机清单.md", 31, 37),
    ("tests/evidence/K-R115-deathvalue.md", 16, 9),
    ("tests/evidence/K-R118-deathvalue.md", 13, 14),
    ("tests/evidence/K-R12-deathvalue.md", 0, 9),
    ("tests/evidence/K-R12-locale-lab.md", 0, 7),
    ("tests/evidence/K-R24-D1-premise-census.md", 5, 3),
    ("tests/evidence/K-R24-D5-home-axis-census.md", 4, 3),
    ("tests/evidence/K-R24-D6-locale-axis-census.md", 5, 8),
    ("tests/evidence/K-R24-D7-etxtbsy.md", 8, 2),
    ("tests/evidence/K-R24-D8-load-axis.md", 12, 1),
    ("tests/evidence/K-R25-D2-unit-alignment.md", 44, 1),
    ("tests/evidence/K-R25-D4-nine-uncovered-files.md", 6, 0),
    ("tests/evidence/K-R26-readings.md", 17, 5),
    ("tests/evidence/K-R27-parked-tense-audit.md", 38, 1),
    ("tests/evidence/K-R56-deathvalue.md", 10, 1),
    ("tests/evidence/K-R67-依赖脊柱与顺序.md", 14, 24),
    ("tests/evidence/K-R68-三种载体摸底.md", 79, 21),
    ("tests/evidence/K-R70-身份从字节里读得出.md", 29, 11),
    ("tests/evidence/K-R71-observe归位.md", 5, 3),
    ("tests/evidence/K-R72-deathvalue.md", 53, 2),
    ("tests/evidence/K-R73-monitor侧层间方向判据.md", 9, 5),
    ("tests/evidence/K-R74-拨号住址与递减棘轮.md", 5, 2),
    ("tests/evidence/K-R75-剥法认形状与真静默读数.md", 27, 5),
    ("tests/evidence/K-R76-三处假话与裸行号的刀.md", 21, 0),
    ("tests/evidence/K-R77-拆最后那道绕道与恒零棘轮.md", 30, 0),
    ("tests/evidence/K-R78-readings.md", 22, 5),
    ("tests/evidence/K-R79-远端写那一层与判档第四档.md", 22, 2),
    ("tests/evidence/K-R80-gate-daemon-fmt.md", 66, 25),
    ("tests/evidence/K-R81-一个后端两处使用.md", 37, 12),
    ("tests/evidence/K-R82-hooks-gate.md", 10, 5),
    ("tests/evidence/K-R83-deathvalue.md", 11, 1),
    ("tests/evidence/K-R85-摸底.md", 20, 2),
    ("tests/evidence/K-R86-deathvalue.md", 33, 6),
    ("tests/evidence/K-R87-deathvalue.md", 43, 10),
    ("tests/evidence/K-R88-deathvalue.md", 11, 0),
    ("tests/evidence/K-R89-deathvalue.md", 28, 26),
    ("tests/evidence/K-R9-R2-fallback-watch-scope.md", 16, 1),
    ("tests/evidence/K-R92-deathvalue.md", 2, 0),
    ("tests/evidence/K-R93-deathvalue.md", 2, 1),
    ("tests/evidence/K-R94-deathvalue.md", 0, 2),
    ("tests/evidence/K-R95-deathvalue.md", 6, 10),
    ("tests/evidence/K-R96-deathvalue.md", 12, 39),
    ("tests/evidence/K-R97-deathvalue.md", 2, 0),
    ("tests/evidence/K-R98-deathvalue.md", 15, 0),
    ("tests/evidence/K-W1B-D1-agent-coupling-census.md", 12, 1),
    ("tests/evidence/K-W1C-D1-edges.md", 1, 0),
    ("tests/evidence/K-W1C-D3D4-deathvalue.md", 0, 0),
    ("tests/evidence/K-W1C-D4-reachability.md", 0, 5),
    ("tests/evidence/K-W2E-readings.md", 17, 2),
    ("tests/evidence/K-W4-D1-rename-surface.md", 5, 1),
    ("tests/evidence/K-W4-D4-build-id-split.md", 50, 1),
    ("tests/evidence/K-W4b-readings.md", 8, 6),
    ("CHANGELOG.md", 90, 79),
];

/// 读一份登记在案的历史留档。读不到 ⇒ 当场 panic 并说清为什么。
///
/// ⚠ **刻意包成函数，不在 `let` 右边直接写 `read_to_string`**：
/// `needle_anchor_registry::corpus_vars` 按「`let X = …read_to_string(…)`」播种语料变量，
/// 而它的传递闭包**按名字**跑一层 —— 在本文件里播一个 `body` 出去，
/// 会把同文件别处 `name.starts_with("README")` 这类**早就存在**的匹配一起卷进人群，
/// 那条递减棘轮当场从 33 涨到 34。〔09-14 实打过一次，读数在 `tests/evidence/K-R116-deathvalue.md`〕
fn read_frozen(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| {
        panic!(
            "{rel} 读不到：{e}\n\
                 ★ 它是登记在案的历史读数 / 墓碑（`K-R116` `KR116D2`）——\n\
                 删掉它 = 把那一刀的证据整份销毁。真要删，先在这张表里删行并说清为什么。"
        )
    })
}

/// 一份文本里某个词的处数。`needle` 走**变量**（针不写成字面量：
/// `needle_anchor_registry` 的棘轮按「拿磁盘语料做裸字面量匹配」计数）。
fn count(text: &str, needle: &str, fold_case: bool) -> usize {
    let hay = if fold_case {
        text.to_ascii_lowercase()
    } else {
        text.to_string()
    };
    hay.matches(needle).count()
}

/// ★★ 正题：登记过的每一份，两个词的处数都不许掉。
#[test]
fn the_frozen_history_never_loses_a_daemon() {
    const DAEMON: &str = "daemon";
    const CCM: &str = "ccm";
    let root = repo_root();

    // 抽取器自检：登记表不许被掏空。
    assert!(
        REGISTERED.len() >= 60,
        "登记表只剩 {} 行（09-14 现打 70）—— 被掏空了，本条在空转",
        REGISTERED.len()
    );

    let mut shrunk: Vec<String> = Vec::new();
    for (rel, floor_d, floor_c) in REGISTERED {
        let frozen_text = read_frozen(&root, rel);
        let d = count(&frozen_text, DAEMON, true);
        let c = count(&frozen_text, CCM, false);
        if d < *floor_d {
            shrunk.push(format!("  {rel}：daemon {d} < 登记 {floor_d}"));
        }
        if c < *floor_c {
            shrunk.push(format!("  {rel}：ccm {c} < 登记 {floor_c}"));
        }
    }

    // ── 整棵树那一档：新长出来的 `tests/evidence/*.md` 由它兜 ──
    //
    // ⚠ 用 `scan_tree!` 而不是裸 `read_dir`：`scanning_guard_registry` 那条元判据
    // 逐字禁裸遍历（判据在自己那份里找到自己 ⇒ 恒绿）。
    let mut n_files = 0usize;
    let mut sum_d = 0usize;
    let mut sum_c = 0usize;
    for (_, evidence_text) in guard_core::scan_tree!(&root.join("tests/evidence"), &["md"]) {
        n_files += 1;
        sum_d += count(&evidence_text, DAEMON, true);
        sum_c += count(&evidence_text, CCM, false);
    }
    assert!(
        n_files >= 60,
        "`evidence/` 只扫到 {n_files} 份 `.md`（09-14 现打 69）—— 遍历坏了，下面两条在空转"
    );
    if sum_d < EVIDENCE_DAEMON_FLOOR {
        shrunk.push(format!(
            "  evidence/ 整棵树：daemon 合计 {sum_d} < 地板 {EVIDENCE_DAEMON_FLOOR}"
        ));
    }
    if sum_c < EVIDENCE_CCM_FLOOR {
        shrunk.push(format!(
            "  evidence/ 整棵树：ccm 合计 {sum_c} < 地板 {EVIDENCE_CCM_FLOOR}"
        ));
    }

    assert!(
        shrunk.is_empty(),
        "有人把历史读数里的 `daemon` / `ccm` 抹掉了：\n{}\n\n\
             ★ `tests/evidence/**` 是死值验留档、`CHANGELOG.md` 是发版墓碑，两者装的都是\n\
             「**某年某月现打是多少**」—— 改它 = **伪造一次读数**（`brief` 第 12 条）。\n\
             `K-R116` 那一轮把散文里的 `daemon` 全换成了「后端」，**这两档刻意不在射程里**。\n\
             ⚠ 真要动（比如一份留档整个作废）：先在 `REGISTERED` 里改行并写清为什么，\n\
             别反过来把地板调下去 —— 那等于把这道闸注销掉。",
        shrunk.join("\n")
    );
}
