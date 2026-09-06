# K-W4b 读数 —— 取样层四态的判据从 0 到 7

> 量于 **2026-09-06**（命令逐条写在下面，可重跑）。
> 被测对象：工作树 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-w4b`
> （分支 `track/k-w4b`，基点 `77220e6`，**未铺** `src-tauri/embedded-daemons/`）。
> ⚠ 这棵树未铺 embedded-daemons ⇒ cargo 那个合计里少「本地后端真的能起来吗」那族 4 条；
> 下面所有 cargo 数都在**同一个未铺状态**下量的，别拿去跟铺了的树比。

## 一 · 门禁两趟（九格逐格）

沙箱命令（宿主上一条 cargo/vitest/e2e 都没跑）：

```
cd /home/zbl/文档/claudecode-frontend
PB_WS=backend-consolidation .claude/devbox/gate \
  /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-w4b k-w4b
```

| 格 | 入场趟①（到手即跑） | 入场趟②（补了 node_modules） | 交回趟 |
|---|---|---|---|
| cargo | ok 1438 | ok 1438 | ok 1445 |
| generated | ok | ok | ok |
| daemon | ok 587 | ok 587 | ok 587 |
| npm | **FAIL 退出码 127** | ok 1590 | ok 1590 |
| e2e ccm-print-parity | **FAIL PASS=0**（地板 12） | ok 12 | ok 12 |
| e2e ccm-rbind-title | ok 8 | ok 8 | ok 8 |
| e2e ccm-cli | **FAIL PASS=259**（地板 264） | ok 264 | ok 264 |
| e2e ccm-contract-parity | ok 72 | ok 72 | ok 72 |
| pb check | ok FAIL=0 BROKEN=0 | ok FAIL=0 BROKEN=0 | **FAIL=1 BROKEN=0** `[J3 陈账]` |
| 裁决 | GATE: FAIL | **GATE: OK** | GATE: FAIL（只此一格） |

交回趟跑了两趟，前八格两趟逐格相同（cargo 1445 · generated ok · daemon 587 · npm 1590 ·
e2e 12/8/264/72），差别只在第九格的**成因**：

- 第一趟（03:0x，代码改完、件文件 `§8` **还没写**）：`FAIL   [J3 陈账] INDEX.md 比源文件旧` ——
  那一刻 `INDEX.md` 比 PM 02:49 落的 `.dispatch.json` 旧，**不是本实现拍造成的**（PM 03:02 已重跑过一次）。
- 第二趟（提交 `94a4540` 之后、`§8` 写完之后）：**同一条** `[J3 陈账]`，这一次是本节自己写进件文件造成的。

⚠ `pb index` 归 PM（生成命令），**本实现拍一次都没跑**。这一格如实报红，不自行修。

**入场趟① 那三格红与本件无关，是这棵树的环境缺件**：`k-w4b` 是 21 棵工作树里**唯一
没有 `node_modules` 符号链接**的一棵（其余 20 棵都指向
`/home/zbl/文档/claudecode-frontend/cc-monitor/node_modules`）⇒ 容器里 `tsx: not found`
（原文 6 处），npm 那条 `&&` 链在第一个套件就断，两套需要 `tsx` 的 e2e 同因。
处置：照其余 20 棵的样子补一条符号链接（`node_modules` 在 `.gitignore` 里，
`git status` 不受影响），然后**在零代码改动的状态下重跑一趟**，那一趟才是本件的基线。

## 二 · KW4bD1① 四个读数自己重打

命令都在 `.claude/worktrees/k-w4b` 里跑，文件 `src-tauri/src/sftp.rs`（2245 行，基点态）。

1. 接线判据住址：`grep -n both_daemon_deploy_paths_ask_the_file_itself_not_only_the_marker`
   ⇒ **1812 行**（`#[test]` 在 **1811**，`§0a` 写的就是这个行号）。它断的四件事逐字：
   `marker_path(` 有 · `probe_target_binary(` 有 · `deploy_decision_at(` 有 · 裸 `deploy_decision(` 没有。
   射程 = `ensure_daemon_deployed` / `deploy_remote_daemon` 两个函数体的**源码文本**。
2. `grep -c deploy_decision_at src-tauri/src/sftp.rs` ⇒ **18**（`§0a` 的 18 处，对上）。
3. `grep -n "async fn probe_target_binary"` ⇒ **479 行**，签名逐字
   `async fn probe_target_binary(sftp: &SftpSession, path: &str) -> TargetBinary`。
4. 吃 `&SftpSession` 的函数 **6 个**（`grep -c "&SftpSession"` = 6，逐个点名：
   `upload_atomic` `upload_atomic_verified` `read_optional` `read_profile_text`
   `ensure_dir_all` `probe_target_binary`）。
   ⚠ 分母口径：**同一份 `sftp.rs` 里**的函数，不是全仓；单行 `grep "fn .*(sftp: &SftpSession"`
   只数得出 3 个（另 3 个签名跨行）—— 这就是「尺子的作用域」那条纪律的现打活体。

## 三 · KW4bD1② 那一刀：改前 0 红

改前（本件之前，基点态源码 + 这一刀）：把 `probe_target_binary` 的体整块换成
`let _ = (sftp, path); TargetBinary::Present`（形状对、恒答一张脸，不是让台子炸）。
沙箱里跑全量 cargo：

```
docker run --rm --network none \
  -v /home/zbl/文档/claudecode-frontend:/home/zbl/文档/claudecode-frontend \
  -v ccmon-cargo-registry:/opt/rust/cargo/registry \
  -e CARGO_TARGET_DIR=/home/zbl/文档/claudecode-frontend/.claude/pm-targets/k-w4b \
  -e HOME=/home/zbl -w /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-w4b \
  ccmon-devbox:latest bash -o pipefail -c \
  'mkdir -p "$HOME/.claude/projects" && cd src-tauri && cargo test --workspace --exclude code-picture-core --lib 2>&1'
```

读数：**EXIT=0**，8 行 `test result: ok.`（9 + 8 + 37 + 8 + 40 + 1324 + 1 + 11 = **1438 passed，0 failed**）。
⇒ **改前 0 红，PM 的预期兑现了；本件题面成立，没有作废。**
（判定行 8 条，与门禁那一趟同数 ⇒ 不是 CRASH 冒充的「新红 0」。）

改后（本件之后）同一刀 ⇒ **1 红**：`sftp::tests::the_probe_shell_really_goes_through_the_pure_interpreter`，
panic 逐字「取样壳没走那个纯解释函数 —— 四态映射的那几格全成了死代码，掏空它一条都不会红」。

## 四 · §3 变异表逐刀真跑（分母：`cargo test --workspace --exclude code-picture-core --lib`，本件之后 = 1445）

| id | 锚点（切在哪 · 命中几次） | 判定行 | 红几条 | 红的是谁 |
|---|---|---|---|---|
| W4bM1 | `let metadata_size = sftp`（`probe_target_binary` 体首行）· 1 次 | 8 行，1330+1 | **1** | `the_probe_shell_really_goes_through_the_pure_interpreter` |
| W4bM2 | `        Some(_) => TargetBinary::Present,`（纯函数第 2 臂）· 1 次；插一臂 `Some(None) => Empty` | 8 行，1329+2 | **2** | `probe_a_server_that_gives_no_size_is_not_the_empty_cell` · `probe_no_cell_answers_in_place_of_another` |
| W4bM3 | `            Some(false) => TargetBinary::Missing,`（纯函数 Err 那支）· 1 次；两臂并成 `_ => Unknown` | 8 行，1328+3 | **3** | `probe_stat_failed_and_try_exists_says_no_maps_to_missing` · `probe_stat_failed_and_try_exists_cannot_answer_maps_to_unknown` · `probe_no_cell_answers_in_place_of_another` |
| W4bM4 | `    interpret_target_probe(metadata_size, exists)`（壳的最后一行）· 1 次；逻辑塞回 async 体、纯函数留着 | 8 行，1330+1 | **1** | `the_probe_shell_really_goes_through_the_pure_interpreter` |
| W4bM4′ | 同上 + 纯函数**整块删掉**（表里那句的字面版）· 定义数 1→0 | **0 行** | **CRASH** | `error[E0425]: cannot find function interpret_target_probe` × 18，一条测试都没跑 |
| W4bM5 | `            .filter(\|l\| !l.trim_start().starts_with("//"))`（D3 判据体内）· 1 次；换成 `\|_\| false` ⇒ 喂空串 | 8 行，1330+1 | **1** | `the_probe_shell_really_goes_through_the_pure_interpreter`，panic 逐字「取到的不是 probe_target_binary 的体（拿到 0 字节）」 |

读法（别读宽）：

- M2 / M3 红的**不止**件文件预期的那一格：反向自检 `probe_no_cell_answers_in_place_of_another`
  也红，M3 还连带红了 ⑤ 那一格自己的 `assert_ne`。这是**设计如此**（它们就是拿来证明那五格不恒真的），
  但件文件 `§3` 写的是「别的四格不红」—— 现打的事实是：**五格映射判据里只有该红的那格红了**，
  另加反向自检那一格。两句话不是一句，这里分开写。
- M4′ 是**表里那句话的字面版**，它编译不过 ⇒ 判定行 0 ⇒ 按 CRASH 记，**不许当成「新红 0」**。
  真正能读出数的那一刀是 M4（把纯函数留着、只把壳的接线断掉），所以表里两行都留着。

## 五 · `7u`：把实现整个退掉，还有几条新断言仍绿

「退掉实现」= W4bM4（壳不再走纯函数，逻辑塞回 async 体；纯函数本身留着，
否则台子炸、拿不到读数）⇒ 7 条新判据里 **6 条仍绿、1 条红**。

- 仍绿的 6 条（五格映射 + 反向自检）**不是仪式**：它们的牙在 M2 / M3 上（各红 2 / 3 条），
  钉的是「四态映射规则」这件事本身；接线那件事本来就不归它们管。
- 红的那 1 条正是 D3 的函数体判据 —— 它是这一族里唯一够得着「壳有没有真的走纯函数」的。
- ⚠ 诚实边界：这 7 条**都够不着运行期**（本件禁真远端），买的是「映射规则有判据」，
  **不是**「真机上 stat 会怎么答」。

## 六 · KW4bD5② 四块函数体 md5

量具：`evidence/K-W4b-rs-fn-md5.py`（住址唯一，只此一份；被测对象由 argv 给）。

```
cd /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-w4b
python3 evidence/K-W4b-rs-fn-md5.py 77220e6 src-tauri/src/sftp.rs
```

读数（09-06，交回态）：30 个顶层函数 —— 相同 28 · 变了 1（`probe_target_binary`
`3e46c73e5a28` → `31ed41e3c58b`）· 没了 0 · 新增 1（`interpret_target_probe` `9ae765704382`）。
钉住的四块**逐个 md5 与基点相同**：
`deploy_decision` `9e75e8266590` · `deploy_decision_at` `1f76c24fc233` ·
`marker_path` `692023700ede` · `remote_parent` `747c66286e09`。

另两把尺子（那个量具的射程只到顶层函数，`mod tests` 里的判据它看不见，所以补这两条）：

- `git diff 77220e6 94a4540 -- src-tauri/src/sftp.rs | grep "^-"` ⇒ 删掉的行**全部落在
  `probe_target_binary` 那一段**（12 行体 + 1 行头注），别处一行没删。
- `:1811` 那条既有判据整块 md5（`awk` 从 `fn both_daemon_deploy_paths_…` 到它的收尾 `    }`）：
  基点 `c933fdc7a70832580d474789db6120a9` = 交回 `c933fdc7a70832580d474789db6120a9`
  ⇒ `§2.4`「不改那条既有判据的断言」有读数，不是口头保证。

## 七 · 收工自查：治的是「这一处」还是「所有同职的地方」

分母 = **这一份 `sftp.rs` 里吃 `&SftpSession` 的 6 个函数**（不是全仓）。逐个看「取样与解释
有没有焊在一起」：

| 函数 | 形状 | 还有没有同族的病 |
|---|---|---|
| `upload_atomic` | 动作（错误串向上传） | 没有；它的判定那一半早就住纯函数 `verify_uploaded_bytes` |
| `upload_atomic_verified` | 三行组装 | 没有 |
| `read_optional` | 一行 | 没有 |
| `read_profile_text` | 取样壳 | 没有（它就是本件照抄的先例） |
| `ensure_dir_all` | 动作，不映射任何状态 | 没有 |
| `probe_target_binary` | 取样壳 | **本件治的就是它** |

⇒ 在这个分母里**没有第二处漏网**。⚠ 这句话的分母只有 6 个函数、只在这一份文件里；
别的模块有没有同族的病，本件判不了。
