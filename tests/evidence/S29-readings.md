# S29 读数：兼容债与卸载残留四档普查（2026-09-19 现打）

量具：`tests/evidence/S29-legacy-compat-census.py`（定义与射程边界写死在它头注里）
裁定依据：用户 2026-09-19 逐字（条 80，**通则**）

⚠ 本件**只读**。一行 `src/**` 与都没有动，也没有 commit 任何代码。

---

## 1. 复算

```bash
cd ~/work/cc-monitor
python3 tests/evidence/S29-legacy-compat-census.py              # 主报告（四档）
python3 tests/evidence/S29-legacy-compat-census.py --addresses  # 逐处住址（代码/散文分列）
python3 tests/evidence/S29-legacy-compat-census.py --tier 甲    # 只看一档
python3 tests/evidence/S29-legacy-compat-census.py --negatives  # 🔴 负向断言逐条
python3 tests/evidence/S29-legacy-compat-census.py --unpaired   # 🔴 丙档：无对拍的字面量族
python3 tests/evidence/S29-legacy-compat-census.py --json
python3 tests/evidence/S29-legacy-compat-census.py --selftest   # 反空真自检
```

退出码：`0` 每处切口都切到东西 · `3` **有切口空转（失败，不是 0 命中的绿）** · `4` 自检失败。

---

## 2. 主报告（2026-09-19 现打，原样贴）

```
人群：98 处切口 · 代码 360 行 · 散文 285 行 （散文占 44%）· 空转 0 处

── 甲 · 兼容债（删） ── 代码 51 行 · 散文 119 行 · 42 处
    A1-daemonless      代码   45 · 散文  108 · 36 处
    A2-single-obj      代码    6 · 散文   11 ·  6 处

── 乙 · 卸载残留清理（🔴 交用户裁） ── 代码 229 行 · 散文 100 行 · 31 处
    B1-ps-misplaced    代码   67 · 散文   11 ·  9 处
    B2-rc-bare         代码  138 · 散文   50 · 11 处
    B3-ccm-config      代码    8 · 散文    6 ·  2 处
    B4-old-tmux        代码   16 · 散文   33 ·  9 处

── 丙 · 缺判据（补判据，不是删） ── 代码 24 行 · 散文 12 行 · 7 处
    C1-fence-4th       代码   12 · 散文    1 ·  4 处
    C2-template-top    代码    7 · 散文   11 ·  2 处
    C3-ts-scan         代码    5 · 散文    0 ·  1 处

── 丁 · 别的原因（不动） ── 代码 56 行 · 散文 54 行 · 18 处
    D1-force-legacy    代码   19 · 散文   11 ·  9 处
    D2-legacy-acct     代码   19 · 散文   28 ·  3 处
    D3-proto-neg       代码    7 · 散文   11 ·  4 处
    D4-build-id        代码   11 · 散文    4 ·  2 处

🔴 负向断言：11 条登记 · 其中 **7 条今天已恒绿**（被守的词在生产代码里已不存在）

🔴 丙档无对拍的字面量族：4 族
```

退出 `0`。

---

## 3. 反空真自检（两格，原样贴）

```
$ python3 tests/evidence/S29-legacy-compat-census.py --selftest
自检：空树上空转 98/98 处 · 代码行合计 0
剥法自检：6 份语料里，纯注释行被算成 code 的有 0 行（必须是 0）
自检通过：真树上切到 360 行代码，空树上 0 行。
```

**第一格（扫空树）**：对着一棵空临时目录跑，要求 **98/98 处全空转、代码行合计 0**，
且真树上必须切得到 ⇒ 否则退 4。这挡的是「尺子坏了却一片绿」。

**第二格（剥法自检）** —— 🔴 **这一格是实测咬出来的，不是预设的**：

> 第一版的剥法在行末只清 `"` / `'` 的「在串里」状态，**没清反引号**。
> 中文注释行里一个没配对的引号就会把状态挂到文件末尾，后面每一行都被当成「在串里」
> ⇒ `//` 不再被认成注释。
> **实测 `src/launcher-diagnostics.ts` 上 53 行纯注释被算进了代码数**，
> 而报告只会显示一个更大的「待删行数」，**不会红**。
>
> 修法：行末一律清掉 `in_str`（`in_block` 与 Rust 的 `r#"…"#` 除外）。
> 修完 6 份语料上误判 **53 → 0**。

⇒ 这一格现在是**常驻自检**：6 份语料里但凡有一行 `//` 开头的被算成 code 就退 4。

---

## 4. 本机现打的三条硬读数

> 它们把主产物「④ 谁受影响」那一列从推测变成事实。

### 4.1 `~/.claude/work/config.json` —— **不存在**

```
$ ls ~/.claude/work/
auto-launch.json  logs  ps-await  ps-registry
```

⇒ 盘上**没有** `remote.hosts[].daemonless`、**没有**旧单对象 `remote`、**没有** `forceLegacyLaunchRenderer`。
⇒ **甲① ＋ 甲② 删掉零人受影响**（用户只有一个，就是这台）。

### 4.2 `~/.bashrc` —— 围栏之外**有 2 行真命中**，且两行都 shadow

按 `profile_installer::{fence_marker, mentions_ccm, function_name_of}` 的规则逐字复现：

```
/home/user/.bashrc → 围栏之外提到 ccm 的行：2
    (124, Function, 'cc',  'cc()  { ccm --cwd "$HOME/projects/notes" "$@"; }')
    (125, Function, 'cct', 'cct() { ccm --tmux --cwd "$HOME/projects/notes" "$@"; }')
/home/user/.profile → 0
```

围栏本体在 `~/.bashrc:127-138`（`# === cc-monitor remote ccm BEGIN/END ===`，
即 `sftp::CCM_PROFILE_BEGIN` 那一对）—— **它确实在这台机器上**。

`cc` 与 `cct` 两个名字**都在** `sftp::builtin_alias_names()` 里
（= `src/shared/ccm-aliases.sh:22 · 25` 那两行）⇒ 两行都会被标「**← 会赢过我们那一块**」，
且 `render_manual_cleanup_hint` 会多出那一段「不删掉它们，装了那一块也不生效」。

⚠ **诚实边界**：用户 `~/.bashrc:120-121` 自己写着「放在 cc-monitor 块之前」「先定义的赢」
—— 他是**故意**这么写的。这条提示今天对他说的是他已经知道的事。

### 4.3 `~/.claude/work/relay-credentials.json` —— **不存在**

⇒ 丁② `LEGACY_ACCOUNT_ID` 今天没有对象；
但 `creds-core::store::TEMPLATE`（首次使用时发出去的那一份）**仍在教用户填顶层 `api_key`**
⇒ 丙③ 那条「模板与读法没对拍」是活的。

---

## 5. 负向断言（`--negatives` 现打，摘要）

11 条登记，**7 条今天已恒绿**（被守的词在生产代码里已不存在）：

| 住址 | 分母 | 状态 |
|---|---|---|
| `launch_wire_f07_main_path_tests.rs:546` | `"daemonless",` @ `remote-config.ts` | 🔴 已恒绿 |
| `launch_wire_f07_main_path_tests.rs:553` | `daemonlessInput` @ `machine-card.ts` | 🔴 已恒绿 |
| `launch_wire_f07_main_path_tests.rs:558` | `daemonless_stream_loop` @ `ssh_source.rs` | 🔴 已恒绿 |
| `doc_claim_registry_tests.rs:441 · 445 · 449` | 同上三个（`carriers` 三格全 false） | 🔴 已恒绿 |
| `creds_store_tests.rs:443` | `store::merge_key(` @ `creds_store.rs` | 形状上恒绿，但**不是兼容债**（丁②） |
| `remote-section.vitest.ts:250 · 252 · 268` | `daemonless` / 夹具阴性对照 | 词还活着（`LEGACY_NO_BACKEND_KEY` 的值） |
| `creds_store_tests.rs:367` | `LEGACY_ACCOUNT_ID` | 词还活着 ⇒ **不动** |

🔴 **用户点的 8 条里，6 条今天就已经恒绿** —— 它们是**防回潮闸**，不是防删闸。
处置写在主产物 `§7`。

---

## 6. 丙档无对拍的字面量族（`--unpaired` 现打，摘要）

| 族 | 有几份独立字面量 | 缺的是什么 |
|---|---|---|
| U1 围栏前缀 `# === cc-monitor` | **5**（4 个常量 ＋ `fence_marker` 里 1 份裸串） | 没有任何断言把第 5 份与前 4 个绑起来 |
| U2 用户盘上那份围栏 | 产品侧 1 ＋ 用户盘上 1（**当年那一版**） | 🔴 常量一改名，盘上那块**永远对不上** ⇒ 卸载按钮消失、残留无人提 |
| U3 `TEMPLATE` 顶层 `api_key` ↔ `read_accounts` | 2（同文件两处，无对拍） | 今天唯一那条判据 `TEMPLATE.contains(KEY_FIELD)` **恒真**（说明文字里就有那个词） |
| U4 `forceLegacyLaunchRenderer` 存盘键名 | 1（好），但**是盘上的键名** | 改名 ⇒ 用户写下的 `true` 被静默当未知键忽略，逃生口无声关闭 |

---

## 7. 这把尺子**不**回答什么

- **不判「唯一的存在理由」** —— 四档归属是人写进 `CUTS` 表里的，不是算出来的。
  改归属必须改那张表并在主产物里写清理由。
- **不含任何改动动作**，也不输出「建议改成 X」。
- **丁档与丙档的行数含少量邻行**（切口按「删这一族时会一起走的行」画，不按语法块画）——
  它们**不进待删合计**，所以不影响结论。甲档的四处邻行已逐个收紧（`readiness.ts` 三处 ＋ `main.ts` 一处
  ＋ `lib.rs::parse_remote_hosts` 签名那六行）。
- **不读** `src/bridge/vendor/**` 与 `node_modules/**`。
- 「旧版 Claude Code / 旧版 tmux 自己写的数据少个字段」这一族**不在人群里** —— 那是第三方格式，
  理由写在主产物「丁⑤」。
