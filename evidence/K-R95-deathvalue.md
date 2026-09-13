# `K-R95` 死值验读数（实打）

**量于**：工作树 `.claude/worktrees/k-r95`，分支 `track/k-r95`，基点 `08ed17f`。
**跑法**：唯一许可的那条 —— `PB_WS=backend-consolidation .claude/devbox/gate <工作树> k-r95`（沙箱，`K31`）。
每一刀都是**真改一处、真跑一趟门禁**，下面每条都带它红在哪一格、哪一条判据、原文一句。

## 终盘（未变异）

```
GATE: OK —— 13 格全绿（hooks · fmt · fmt-daemon · winchk · cargo · generated · daemon · npm · 四套 ccm e2e · pb check）
cargo 1574 passed（8 个包合计）· 本树**未铺** src-tauri/embedded-daemons/
npm 1707 passed · daemon 699 · hooks 11 · e2e 12 / 8 / 46 / 45
```

### 预登记对账（押错照实报）

| 格 | 预登记 | 实得 | 判 |
|---|---|---|---|
| `cargo` | 1556 起，押 +4~+12 | **1574** | 我加了 **7 条**判据（`launch_wire.rs` 3 · `launch_cli_parity.rs` 4）⇒ 增量 **+7**，落在押的区间里；**对不上的是起点** —— 1574 − 7 = **1567**。⚠ 这个 1567 是**推出来的，不是量出来的**（`K31` 只许跑门禁，我没为量基点单跑一趟干净树） |
| `npm` | 1698 起，押 +6~+16 | **1707** | 🔴 **押错**：我**一条 vitest 都没加**（只改了两条既有判据的期望值：生成物清单 +1 项、运行期 import 的生成物 1→2）⇒ 增量 **0**。若起点真是 1698，那 9 条不是我加的 |
| `daemon` / `hooks` / e2e | 699 / 11 / 12·8·46·45 ±0 | 同 | ✅ 押中 |
| `fmt`/`fmt-daemon`/`winchk`/`generated` | 绿 | 绿 | ✅（`generated` 那一格的**瞎点**见 §3-1 刀①） |

## §3-1 `KR95D1` —— 改后端能改变前端手里的载荷／调用行

### 刀① 后端改一处而前端产出不跟 ⇒ 必须红

**两路各打一次，都红。**

**(a) 走生成物那条主渠道**（`mutB.log`）：只改 `launch_wire.rs` 里
`PROBE_UNKNOWN_REASON` 那**一个字符串**（`不等于没装` → `这次只改了后端一处`），
其余一个字节不动、**不跑 `npm run gen:types`**：

```
FAIL generated      src/generated/ 与 Rust 源不一致：
 src/generated/launch-render-facts.ts | 2 +-
GATE: FAIL —— generated（改了带 ts_rs::TS 的类型 ⇒ 跑 npm run gen:types 并把 src/generated/ 一起提交）
```

⚠ **这一格此前是瞎的**：新生成物一开始没 `git add`，而第六格判的是
`git diff --quiet --exit-code -- src/generated/` —— **未跟踪文件它看不见**。
第一趟 `GATE: OK` 里这一格对本件的产物零覆盖。发现后已入库，上面这条红是入库之后打的。

**(b) 走「前端还留着那一份」那条**（`mutA.log`）：把
`ccm_invocation::CLI_REQUIRED_CAPS` **换序**（`"new","resume"` → `"resume","new"`，集合不变，
所以 `ccm-contract-parity` 的 ⊇ 判据不受干扰）：

```
thread '…launch_cli_parity::tests::the_capability_list_the_frontend_still_spells_out_is_pinned_to_the_backend' panicked
```

### 刀② 跟了 ⇒ 绿

同一次 (a) 改动之后，门禁的 cargo 格**现算**出来的生成物逐字是：

```ts
probeUnknown: "探测没得出答案（这次只改了后端一处）：{error}",
```

—— **TS 侧一个字节都没改**，`launch-render-cli.ts::tryRenderCli` 那句降级理由就跟着变了
（它读的是 `CLI_REFUSAL_REASON.probeUnknown`）。把生成物一起提交后 13 格全绿。

### 刀③ 前端退回自己拼 ⇒ 必须红

把 `accounts.ts` 里那三处**计算键**换回 `{ kind: "named", configDir: …, name }` 三个字面量
（产出**逐字节相同**）：

```
thread '…launch_cli_parity::tests::the_frontend_reads_the_account_wire_table_instead_of_writing_the_keys_out_again' panicked
```

🔴 **这一条是判写法，登记在案**：产出逐字节相同 ⇒ 任何按产出判的东西都看不见它。
理由与边界逐字写在那条判据自己的头注里，**它不是 `KR95D1` 的主判据**（主判据是刀①那两路）。

## §3-2 `KR95D2` —— 那 16 例夹具活下来，且对着唯一那份跑

### 刀① 夹具数变少 ⇒ 必须红

从 `fixtures/cli-golden.json` 删掉第 16 例：

```
thread '…launch_cli_parity::tests::the_fixture_covers_both_ok_and_refusal' panicked at …:183:9:
assertion `left == right` failed: 夹具用例数变了（加/删用例请一起改 EXPECT_CASES）
```

### 刀② 16 例对着唯一那份跑且全过 ⇒ **只兑现了前半句**

终盘 `EXPECT_CASES = 16` **一例没删**（纪律 ⑱），
`rust_cli_rendering_matches_the_typescript_golden_byte_for_byte` 仍逐字节跑**生产命令本体**
`launch_wire::render_ccm_launch`，16 例全过。

🔴 **后半句没兑现，如实报**：它**仍对着 2 份跑**，不是 1 份。
消费这份夹具的今天仍是两个：① 上面那条 Rust 对拍 · ② `src/launch-cli-golden.ts`
用 `tryRenderCli` **生成**它。⇒ TS 那个渲染器整个还在，删它要动 6 个写区没给的文件，
而且删掉会让这份夹具变成「没有生成者的冻结文件」（`ccm_invocation.rs` 的 P1 头注逐字预告过）。
逐条住件文件 `§8` 的 `〔R95e〕`。

⚙ 本件顺手把这一族**补厚了一格**：夹具里的 `defaultLauncher` 此前挂 `#[allow(dead_code)]`、
头注逐字写着「只为『让夹具能被解析』」—— 而它是「`--launcher` 吐不吐」那条分支的唯一输入。
现在由 `the_fixture_default_launcher_is_the_one_the_backend_says` 接到 `adapter::active()` 那一份上。

## §3-3 `KR95D3` —— 「问不到后端」说得出话

### 刀① 静默回落 ⇒ 必须红

把 `PROBE_UNKNOWN_REASON` 改成 `"远端未装 ccm"`（= 把「不知道」伪装成「知道」）：

```
thread '…launch_wire::k_r95_launch_render_facts::not_knowing_is_never_spelled_the_same_way_as_knowing_it_is_absent' panicked
assertion `left != right` failed: 「没探出来」被写成了「远端未装 ccm」—— 那是把「不知道」伪装成「知道」
```

### 刀② 明说不知道且结构上带不出命令 ⇒ 绿

- **措辞侧**：那一句 ≠「远端未装 ccm」，且必须带 `{error}` 占位（那条唯一线索），同上判据。
- **结构侧**：生成器**问不到后端就 panic，不出表** —— `local_launch_account_wire()` 每次生成都跑
  一遍 `history.rs::LaunchAccount` 的**生产反序列化器**验键名，并配阴性对照
  （`{"kind":"named","cfgDir":"/d"}` 必须被拒）。验不过 ⇒ `npm run gen:types` 当场 panic
  ⇒ **不会写出一份旧的/默认的表**给前端拿去拼命令。

### 🔴 未闭合的一格（`〔R95b〕`，交回里报了）

线上那份 `CliRenderRequest.caps: Option<Vec<String>>` **只有两态**，而它上游
（`ccm-probe.ts`，`K-R53` 起）是三态。⇒ 走**生产主路**（`remote-launch-run.ts::renderCliViaBackend`
→ 后端渲染）时，「没探出来」仍被压成 `Refusal::NotInstalled`「远端未装 ccm」。
补第三态要动 `src/launch-cli-wire.ts`（TS 那份手写镜像，由 `launch-cli-wire.vitest.ts`
的「字段集相等」钉着）＋ `remote-launch-run.ts::buildCliRenderRequest` —— **两个都不在本件写区**。
本件试过加字段，`launch-cli-wire.vitest.ts` 当场红（读数：
`expected [ …(7) ] to deeply equal [ …(8) ]`，差的就是 `probeError`），已按写区撤回。
**缺的是线，不是措辞** —— 那句话今天已经只有一处了。
