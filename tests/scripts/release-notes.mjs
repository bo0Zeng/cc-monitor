#!/usr/bin/env node
// GitHub Release 的**正文生成器** —— 一份正文，一个住址。
//
// # 它治的是什么（`K-R124` `KR124D2`，2026-09-15）
//
// `release.yml` 两处「往 Release 上写」此前一处写着 `generate_release_notes: true`、
// 另一处连 `body` 都没有 ⇒ **真发出去的正文是 GitHub 自动生成的那份提交列表**，
// 不是我们写的任何一个字。读数住 `tests/evidence/K-R123-发版读数.md § 4.5`。
//
// # 正文取自哪儿 —— `CHANGELOG.md` 里本版那一段，不另立第二份
//
// `src/doc/RELEASING.md § 5「Release Notes」`**早就写着**这条 SOP，逐字：
// 「GitHub Releases 描述用 CHANGELOG.md 对应版本段的复制 + 加：…」。
// 它一直是**手工**的一步，而手工的那一步从 v3.6.0 起一次都没人做 ⇒ 本文件把它自动化。
// ⇒ **不新开一份发版说明文件**：正文与 `CHANGELOG.md` 同源，两份必漂
//   （`K-R119-发版说明-v3.8.0.md` 那四行数在 24 小时内就馊了，就是这一形的现打例子）。
//
// # ⚠ 它刻意不做的事
//
// · **不抄那张下载清单**。资产名今天已经有两个住址（`release.yml` 两处 `files:` ＝ 权威，
//   `src/doc/RELEASING.md § 2.2` ＝ 它的快照，那份文件自己逐字警告过「改一处要两处一起改」）。
//   再抄第三份就是再立一处会漂的副本 —— 而 Release 页本来就会把资产逐个列出来。
// · **不判正文写得对不对**。它只保证「这一版在 CHANGELOG 里有一段、那段非空」。
//
// # 跑法
//
//     node scripts/release-notes.mjs <输出文件>   // 写正文，`release.yml` 两个 job 各跑一次
//     node scripts/release-notes.mjs --check      // 只验 + 印读数（本地门禁那一格用）
//
// 退出码 0 = 过；1 = 这一版在 CHANGELOG 里没有段 / 段太短 / 读不到文件。
// **不回落**：拿不到正文就红，不许静默让 Release 去用自动生成那份。

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

// 段落地板：少于这么多非空行就判「这一版没有真正写过发版说明」。
// 现打（2026-09-15，`CHANGELOG.md` 的 `[3.8.0]` 段）：98 非空行。
const MIN_LINES = 20;

const REPO_URL = "https://github.com/bo0Zeng/cc-monitor";

function die(msg) {
  console.error(`release-notes: ${msg}`);
  process.exit(1);
}

function read(rel) {
  try {
    return readFileSync(join(ROOT, rel), "utf8");
  } catch (e) {
    die(`读不到 ${rel} —— ${e.message}`);
  }
}

/** `CHANGELOG.md` 里 `## [<version>]` 那一段（不含标题行本身）。 */
function section(changelog, version) {
  const lines = changelog.split("\n");
  const head = (l) => /^## \[/.test(l);
  const mine = (l) => l.startsWith(`## [${version}]`);
  const hits = lines.map((l, i) => (mine(l) ? i : -1)).filter((i) => i >= 0);
  if (hits.length !== 1) {
    die(
      `CHANGELOG.md 里 \`## [${version}]\` 命中 ${hits.length} 处（应当恰好 1 处）——` +
        ` 这一版没写发版说明，或者写了两段。**不回落到自动生成**，按红记`,
    );
  }
  const start = hits[0];
  let end = lines.length;
  for (let i = start + 1; i < lines.length; i++) {
    if (head(lines[i])) {
      end = i;
      break;
    }
  }
  return lines.slice(start + 1, end);
}

function main(argv) {
  const version = JSON.parse(read("package.json")).version;
  if (!version) die("package.json 里没有 version");
  const body = section(read("CHANGELOG.md"), version);
  const solid = body.filter((l) => l.trim() !== "").length;
  if (solid < MIN_LINES) {
    die(
      `\`## [${version}]\` 那一段只有 ${solid} 个非空行（地板 ${MIN_LINES}）——` +
        ` 这不像一份写过的发版说明。**不回落到自动生成**，按红记`,
    );
  }

  const out = [
    `# cc-monitor v${version}`,
    "",
    ...body,
    "",
    "---",
    "",
    `完整 CHANGELOG 见 [CHANGELOG.md](${REPO_URL}/blob/main/CHANGELOG.md)。`,
    "",
  ].join("\n");

  const target = argv.find((a) => !a.startsWith("--"));
  if (argv.includes("--check")) {
    console.log(
      `release-notes: v${version} 的正文 ${out.split("\n").length} 行 / ${solid} 个非空行` +
        `（取自 CHANGELOG.md 的 [${version}] 段，地板 ${MIN_LINES}）`,
    );
    return;
  }
  if (!target) die("没给输出文件 —— 用法 `node scripts/release-notes.mjs <输出文件>`");
  writeFileSync(target, out, "utf8");
  console.log(
    `release-notes: 写出 ${target}（v${version}，${out.split("\n").length} 行 / ${solid} 个非空行）`,
  );
}

main(process.argv.slice(2));
