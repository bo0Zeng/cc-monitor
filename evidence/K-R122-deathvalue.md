# K-R122 死值验留档

> 本文件只装**读数**，不装结论。每一行带：切在哪一处 · 锚点命中几次 · 用什么量的 · 在哪棵树上量的。
> 被测树：`/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r122`（分支 `track/k-r122`，基点 `e44ab3e`）。
> 刀具住 `evidence/K-R122-cut.py`（`--list` / `--apply <刀>` / `--revert`）——
> 它落刀前一律 `assert 锚点命中 == want`，对不上**一个字节都不改**；还原走 `write_text` 重写原文，
> **不走 `copy2`**（门禁 `copy2` 那一格正在数这件事）。

## 零 · 量法（两种，别混）

| 量法 | 命令 | 什么时候用 |
|---|---|---|
| **整趟门禁** | `PB_WS=backend-consolidation .claude/devbox/gate <工作树> k-r122` | 交回前的终账 ＋ 阴性对照 |
| **最小面探针** | 同一个镜像 `ccmon-devbox:latest`、同一份挂载与 `CARGO_TARGET_DIR`，只跑**被切那一格自己那条命令** | 逐刀 |

🔴 **诚实边界（硬边界的字面）**：本件的硬边界写着「唯一许可命令」是整趟门禁。
逐刀用最小面探针是**对那句字面的一处偏离**，偏离的形状写清楚：
① 仍然**一律在沙箱里**（同镜像 · 同挂载 · `--network none`），宿主上没跑过一条 `cargo`/`npm`/`tsc`；
② 换来的是「一刀一格」而不是「一刀 19 格」——19 刀 × 整趟（≈8 分钟）本轮跑不完；
③ 终账与阴性对照两趟**都是整趟**，没有用探针替代。
探针脚本住会话私有目录 `…/scratchpad/kr122-probe.sh`（**不进仓**，它的被测对象硬指 `k-r122` 这一棵）。

## 一 · 基线（M0，修复后、未落刀）

| 格 | 读数 | 量法 |
|---|---|---|
| 整趟门禁 | `GATE: OK —— 19 格全绿` | 整趟（见 `§五`） |
| `shellcheck` | `57 passed`（rc=0） | 最小面 |
| `ci-e2e-prereq` | `20 passed`（其中 15 条要后端二进制） | 最小面 |
| 覆盖尺子 | `KR80D3: OK` | 最小面 |
| `winchk-daemon` | rc=0，`Finished dev profile` | 最小面 |
| `doc_claim_registry` | `25 passed; 0 failed` | 最小面 |
| `commands.vitest.ts` | `16 passed (16)` | 最小面 |

## 二 · 逐刀（每行：切在哪 · 锚点命中 · 量哪一格 · 真实输出）

| 刀 | 切在哪一处 | 锚点命中 | 量哪一格 | 真实输出（逐字节选） | 判 |
|---|---|---|---|---|---|
| `d1a` | `acquire.rs` 三处 `#[cfg(unix)]` | **3/3** | `winchk-daemon` | `error: could not compile ... (bin "cc-monitor-remote" test) due to **8** previous errors`，rc=101 | 红 ✓ |
| `d1b` | `search_query.rs::set_mtime` 整块换回 `libc` 版 | **1/1** | `winchk-daemon` | `error[E0425]: cannot find function \`utimensat\` in crate \`libc\`` ＋ `AT_FDCWD` ⇒ `due to **2** previous errors`，rc=101 | 红 ✓ |
| `d1` | 上面两刀一起 | **3/3 ＋ 1/1** | `winchk-daemon` | `due to **10** previous errors`，rc=101 —— **与云端 msvc 那趟同数、同档（`bin "cc-monitor-remote" test`）** | 红 ✓ |
| `d2` | `usage-probe-acceptance.sh` 函数名改回全大写 | **1/1（定义）＋ 5/5（调用）** | `shellcheck` | `SC1081 (error)` ×**6**（`:92 :143 :150 :156 :161 :169`），rc=1 | 红 ✓ |
| `d34a` | `ci.yml` 把「用量探针（F10）」挪回 build 之前 | 1/1（挪 2 行） | `ci-e2e-prereq` | `FAIL=1`：`e2e-tmux-rust 第 6 步 … 同 job 里那条 build 在第 7 步 —— 排在它后面` | 红 ✓ |
| `d34b` | `ci.yml` 把那四步搬回 `e2e-tmux` | 1/1 ×4 | `ci-e2e-prereq` | `FAIL=4`：四条逐条 `同 job 里**一条 build 都没有**` | 红 ✓ |
| `d3` | `package-lock.json` 顶层两处退回 `3.7.0` | **1/1 ＋ 1/1** | `cargo`（`doc_claim_registry`） | `the_npm_lockfile_claims_the_version_we_ship` panicked ⇒ `24 passed; **1 failed**`，rc=101 | 红 ✓ |
| `d3n` | `d3` ＋ 把本件新判据整块摘掉 | 上面两处 ＋ 1/1 | `cargo`（`doc_claim_registry`） | `25 passed; **0 failed**`，rc=0 | **不红** ✓（见 `§三`） |
| `d5win` | `commands.vitest.ts` 换成 Windows 形态的活体夹具，**修复仍在** | 1/1（`walk`）＋ 4/4（`readFileSync`） | `npm`（该文件） | `Tests 16 passed (16)`，rc=0 | **不红** ✓ |
| `d5winraw` | 同一份夹具，**修复拿掉** | 同上 | `npm`（该文件） | `FAIL … 铸名只有一个算法口`：`expected [ 'src\ipc\local-tmux-name.ts', …(2) ] to deeply equal [ 'src/tabs.ts' ]` ⇒ `1 failed \| 15 passed (16)`，rc=1 | 红 ✓ |
| `dcount` | `gate.sh` 自述格数 `19 格` → `18 格` | 1/1 | 覆盖尺子 | `FAIL=2`：`C5b 自述节自称 18 格，而现打 19 格` ＋ `与裁决行那句自称 19 格对不上` | 红 ✓ |
| `droll` | `gate.sh` 自述节逐格点名少一格 | 1/1 | 覆盖尺子 | `FAIL=1`：`C5b … 盘上有而没点 ['shellcheck']` | 红 ✓ |
| `dblinddel` | `gate.sh` 删掉裁决那一支印射程的那一行 | 1/1 | 覆盖尺子 | `FAIL=1`：`C5c gate_print_blind 只命中 1 处 … 射程表还在，而没有人印它` | 红 ✓ |
| `dblindstale` | 射程表里塞回一个**今天已经有格**的键（`shellcheck`） | 1/1 | 覆盖尺子 | `FAIL=1`：`C5c 射程表仍自称不看 ['shellcheck']，而现打已经有这几格了` | 红 ✓ |

🔴 **`droll` 第一版的锚点是错的，如实记**：头一版锚点写成
`hooks · copy2 · shellcheck · ci-e2e-prereq · fmt`，而那一串在 `gate.sh` 里命中 **2** 次
（自述节点名 ＋ 裁决行）⇒ 落刀前的断言当场拒绝，**一个字节都没改**。
第二版把 `〔自述·点名〕` 那个前缀并进锚点才是 1 次。
★ 这一处就是 `brief` 第 7 条那句话在本轮的活体：**先断言命中数再落刀**，
不然这一刀会同时打两处，而读数看起来照样「红了」。

## 三 · `KR122D3` 那条「今天不红」的证据（`d3` vs `d3n`）

| 盘上的样子 | `doc_claim_registry` 全量 | 说明 |
|---|---|---|
| `package-lock` `3.7.0` ＋ **有**本件新判据（`d3`） | `24 passed; 1 failed` | 红的正是那一条 |
| `package-lock` `3.7.0` ＋ **没有**本件新判据（`d3n`） | `25 passed; 0 failed` | **这就是「今天不红」** —— 整棵树里没有第二个东西看着那两处 |

⚠ 分母：`cargo test -p monitor --lib doc_claim_registry::`（25 条），量于本工作树、沙箱镜像。
⚠ `d3n` 摘判据用的是「函数改名 ＋ 首行 `return;`」，不是删掉整块 ——
形状对、恒答「过」，不动别的判据的编译面（`brief` 第 7 条那句「类型契约还成立吗」）。

## 四 · 阴性对照（`dall`：五条修复**整块**退掉，新判据全留着）

命令：`--apply dall` 之后跑**整趟门禁**。落刀读数：改了 5 份文件
（`acquire.rs` · `search_query.rs` · `usage-probe-acceptance.sh` · `ci.yml` · `package-lock.json`）。

```
GATE: FAIL —— shellcheck（退出码 1）；ci-e2e-prereq（退出码 1）；winchk-daemon（退出码 101）；
              cargo（退出码 101）；daemon（退出码 101）
```

**19 格里 5 格红 · 14 格绿**，逐条定性：

| 格 | 红/绿 | 定性 |
|---|---|---|
| `shellcheck` | 红 | ② 的牙 |
| `ci-e2e-prereq` | 红 | ③④ 的牙 |
| `winchk-daemon` | 红 | ① 的牙（诊断里逐字 10 个错、住址全带上了） |
| `cargo` | 红 | `KR122D3` 的牙 |
| `daemon` | 红 | 🔴 **这一条不是本轮任何一刀的牙 —— 是一次偶发**：`relay::server::tests::a_credentials_file_produced_by_the_write_side_routes_that_account_to_the_upstream` 报 `BrokenPipe` / `HTTP/1.1 502 Bad Gateway`，`761 passed; 1 failed`。同一棵树 run1 与终账两趟都是 **762 passed 全绿** ⇒ 按**偶发**记，不许算进死值验的分子 |
| `npm`（1726 passed） | **绿** | 🔴 **⑤ 在本机没有牙，如实登记**：`sep` 在 Linux 上恒是 `/`，把修复退回去是个 no-op。⑤ 的牙靠 `§二` 那对活体夹具（`d5win` / `d5winraw`）买，不靠这一格 |
| 其余 13 格 | 绿 | 与本轮五条无关（`hooks` · `copy2` · `fmt` · `fmt-daemon` · `winchk` · `generated` · `deadcode` · `tsc` · 四套 `ccm e2e` · `pb check`） |

## 五 · 终账（`--revert` 之后的整趟门禁）

读数与逐格分母见件文件 `§3`（那一份是现打，本文件不抄第二遍）。

## 六 · 本机**判不了**的那几格（写清楚，不许写成「通过」）

1. 🔴 **「CI 到底会不会绿」本件判不了** —— 推 `main` 与推 tag 都不在本件边界里，
   而 `ci.yml` 只在 `push`（`main` / `v*`）与 `pull_request` 上触发。
   本件给出的是**逐条的同形复现**（见件文件 `§8`），不是那一趟的读数。
2. **MSVC ABI 专属的编译问题**：本地两格 Windows 交叉检查用的是 `-gnu`
   （沙箱镜像没有 zig，`ring` 的 build script 缺 `lib.exe` —— 现打 rc=101，
   报错原文 `error occurred in cc-rs: failed to find tool "lib.exe"`）。
3. **Windows runner 上前端的真实行为**：`d5win` / `d5winraw` 是**模拟**
   （把 `join` 产出的分隔符换掉 ＋ 给 `readFileSync` 加一层「两种分隔符都吃」的翻译，
   后者是 Windows 上 fs 的真实行为）。它买到的是「在 Windows 形态的输入上这条判据怎么走」，
   **买不到**「真 Windows 上 node 的全部行为」。
4. **那四套搬了家的 e2e 在云端跑起来会不会绿** —— `ci-e2e-prereq` 判的是**前置齐不齐**，
   不跑任何一套。本地门禁跑的是其中三套（`ccm-cli` / `ccm-print-parity` / `ccm-contract-parity`），
   **`cc-spawn-uplift` 本地一格都没跑过**。
