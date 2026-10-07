#!/usr/bin/env node
// GitHub Release 的正文生成器：正文取自 `CHANGELOG.md` 里本版那一段，不另立第二份（两份必漂）。
// `src/doc/RELEASING.md` §5「Release Notes」那条 SOP 由本文件自动化；`release.yml` 两处发布步骤都吃它的输出。
// 刻意不做：不抄下载清单（资产名的权威是 `release.yml` 两处 `files:`，Release 页本来会逐个列出）；不判正文写得对不对，
//   只保证这一版在 CHANGELOG 里有一段、那段不短于地板。
//
// 跑法：
//     node tests/scripts/release-notes.mjs <输出文件>   // 写正文，`release.yml` 两个 job 各跑一次
//     node tests/scripts/release-notes.mjs --check      // 只验 + 印读数（本地门禁那一格用）
// 退出码 0 = 过；1 = 这一版在 CHANGELOG 里没有段 / 段太短 / 读不到文件。拿不到正文就红，不回落到 GitHub 自动生成那份。

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

// 段落地板：少于这么多非空行就判「这一版没有真正写过发版说明」。
// 它挡的是空段和只剩一两句的占位；小版本照实写本来就短（只修一两件事的版本约十几行），地板不逼人凑行数。
const MIN_LINES = 10;

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
  if (!target) die("没给输出文件 —— 用法 `node tests/scripts/release-notes.mjs <输出文件>`");
  writeFileSync(target, out, "utf8");
  console.log(
    `release-notes: 写出 ${target}（v${version}，${out.split("\n").length} 行 / ${solid} 个非空行）`,
  );
}

main(process.argv.slice(2));
