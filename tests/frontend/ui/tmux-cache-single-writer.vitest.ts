/**
 * audit-0805 F14 第五刀（报告 I9′）：**tmux 会话列表的取数点登记表**。
 *
 * # 报告说的是「3 写 1 读」，但那不是毛病所在
 *
 * 一个缓存有多个写者、少数读者，本身没什么不对。实测下来真正的毛病是：
 * `TabManager` 里**四处**各自 `invoke("list_remote_tmux")`，其中三处顺手写了缓存、
 * **一处没写** —— 而没写的那处（`awaitExitFor` 的轮询 tick）恰好是**唯一会反复取数的**：
 * 它每秒查一遍同一个 origin，一次都不喂缓存。
 *
 * ⇒ 病根是「**取数**」与「**写缓存**」是两件可以分开做的事。
 * 只要还能「取而不写」，下一个新增的取数点就会再漏一次 —— 这正是定框 **E3** 说的
 * 「权威源恰好一个」：不是「把三处都改对」，是让它**只剩一处**。
 *
 * 现在 `TabManager.fetchTmuxFresh` 是类内唯一取数点，取数与写缓存在语法上是同一件事。
 *
 * # 这张表为什么带例外列
 *
 * 全仓还有三处取数点**够不到那个缓存**（它是 `TabManager` 私有）。
 * 与其假装没有，不如**逐条登记 + 写清为什么**：新增第四处没登记的 ⇒ 本条红。
 * ⚠ 登记表不是豁免清单 —— 每条例外都得说清「为什么它没法走唯一取数点」。
 */
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve, join } from "node:path";

import { srcDirOf } from "../../test-support/repo-root";
const SRC = resolve(srcDirOf(__dirname));

/**
 * 取数的写法：那两条 Tauri 命令退役，今天唯一的取法是读口 `tmux-reads.ts::listTmux(`（经通道问那台后端 `tmux-list`）。
 * 旧的两形（裸 `invoke("list_remote_tmux")`〔散文墓碑〕 · 经 `commands.` 包装）一并留着认 —— 哪天长回来也数得到。
 */
const PATTERNS = [
  /\blistTmux\(/g,
  /invoke<[^>]*>\(\s*"list_(?:local|remote)_tmux"/g,
  /commands\.list_(?:local|remote)_tmux\(/g,
];

/**
 * 够不到 `TabManager` 私有缓存、因而**允许取而不写**的取数点。
 *
 * 每条必须写清「为什么」。想加第四条之前先问：它是不是本来就该走 `fetchTmuxFresh`。
 */
const EXEMPT: ReadonlyArray<readonly [file: string, why: string]> = [
  [
    "tmux-reads.ts",
    "读口本体（原来这一格是 IPC 包装本体 `ipc/commands.ts`）—— 它就是那一问，不是调用方。缓存策略不该住在读口" +
      "（住进去等于让每个调用方都被动吃 8s 陈旧数据，包括 attach/kill 这些对新鲜度最敏感的）。",
  ],
  [
    "tmux-name-mint.ts",
    "另一个模块，够不到 `TabManager` 的私有 `tmuxCache`。它是起会话铸名的唯一家：" +
      "先前登记在这里的三条（`fork-flow.ts` · `settings/machine-card.ts` · `remote-launch-run.ts`）" +
      "都是「列名单 → 铸名」的副本，三条里两条列不出就拿空集铸名（#76 的形状），一条 `.catch(() => null)` " +
      "把「没问到」压成「没有会话」。收进这里之后三态保留（`TmuxListing`：known / unknown），" +
      "**这一次取数的意义就是要最新的**（拿 8s 前的快照去避让，正好会让到一个刚被占掉的名字）。",
  ],
  // 〔墓碑〕`tabs.ts::explainBringFrontFailure` 这一条删了：那个函数整个删了 ——
  //   它是点名要收的「四套有没有终端的判断」之一（E73 那次远端 RPC），
  //   ↗ 的归因从此只住后端一处（`bind.rs::resolve_remote_front`），tmux 不在 ↗ 的前提链上。
] as const;

/**
 * 唯一取数点住哪个文件。`TabManager` 拆开之后，会话动作（连同 `tmuxCache` 与
 * `fetchTmuxFresh`）搬进了 `tab-session-actions.ts` —— **住址换了，性质一字不变**：
 * tab 层仍然恰好一个取数点，且它就是写缓存的那一个。原先这里写死的是 `"tabs.ts"`。
 */
const HOME = "tab-session-actions.ts";

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      walk(p, out);
      continue;
    }
    if (!name.endsWith(".ts")) continue;
    if (name.includes(".vitest.") || name.includes(".test.") || name.endsWith(".d.ts")) continue;
    out.push(p);
  }
  return out;
}

/** 剥掉整行注释 —— 本文件自己的头注里就写着那几个取数的名字。 */
function stripLineComments(src: string): string {
  return src
    .split("\n")
    .filter((l) => {
      const t = l.trimStart();
      return !(t.startsWith("//") || t.startsWith("*") || t.startsWith("/*"));
    })
    .join("\n");
}

function fetchSites(): { file: string; count: number }[] {
  const out: { file: string; count: number }[] = [];
  for (const p of walk(SRC).sort()) {
    const src = stripLineComments(readFileSync(p, "utf8"));
    let n = 0;
    for (const re of PATTERNS) n += (src.match(re) ?? []).length;
    if (n > 0) out.push({ file: p.slice(SRC.length + 1).replace(/\\/g, "/"), count: n });
  }
  return out;
}

describe("tmux 会话列表的取数点（audit-0805 F14 第五刀，E3）", () => {
  it("★ 取数点一个都不许漏登记", () => {
    const sites = fetchSites();
    const total = sites.reduce((a, s) => a + s.count, 0);
    // 抽取器自检：扫不到东西时下面的对拍会两边都空、静默变绿。
    // 地板 `≥ 4` 换成相等：包装层 1（今天是读口 `listTmux` 的定义那一处）＋ `tab-session-actions.ts::fetchTmuxFresh` 1 ＋
    //   `tmux-name-mint.ts::readTmuxListing` 1 = 3（少掉的是 fork-flow · machine-card · remote-launch-run 三份副本）。
    //   抽取器坏了会少、有人新长一处会多，两个方向都红。
    expect(
      total,
      `全仓扫到 ${total} 个 tmux 名单取数点（现打应为 3）—— 抽取器坏了，或新长了一处`,
    ).toBe(3);

    const exemptFiles = new Set(EXEMPT.map(([f]) => f.split("::")[0]));
    const unregistered = sites.filter(
      (s) => s.file !== HOME && !exemptFiles.has(s.file),
    );
    expect(
      unregistered.map((s) => `${s.file}（${s.count} 处）`),
      "有 tmux 名单取数点没登记。**这张表不是豁免清单** —— " +
        "先问它能不能走 `TabManager.fetchTmuxFresh`（取数与写缓存是同一件事）；" +
        "真够不到就在 EXEMPT 里写清为什么",
    ).toEqual([]);
  });

  it("★ TabManager 类内只剩一个取数点，且它就是写缓存的那一个", () => {
    const src = stripLineComments(readFileSync(join(SRC, HOME), "utf8"));
    // 原先只数裸 `invoke` 那一形（`PATTERNS[0]`）：那一处收进了包装层，这里改数两形之和
    //   —— 只数一形的话「取数点换了写法」就会被读成「取数点没了」。
    const fetches = PATTERNS.reduce((a, re) => a + (src.match(re) ?? []).length, 0);
    const writes = (src.match(/this\.tmuxCache\.set\(/g) ?? []).length;

    expect(
      fetches,
      `${HOME} 里有 ${fetches} 个取数点。只允许一个：类内唯一的 \`fetchTmuxFresh\`。` +
        "〔2 → 1〕少掉的那一个是模块级 `explainBringFrontFailure`（↗ 失败后按 tmux 实况猜归因），" +
        " 把它收进后端那一个布尔之后整个删了。" +
        "多出来的那个 —— 它是不是本来就该走 `fetchTmuxFresh`？" +
        "★ 报告 I9′ 那条毛病（取而不写）就是这么来的：取数与写缓存是两件能分开做的事。",
    ).toBe(1);

    expect(
      writes,
      `${HOME} 里有 ${writes} 处写缓存。收成一处之后就该恒为 1 —— ` +
        "多出一处意味着又有人在取数点之外单独写缓存了，「取数即写缓存」这条就不再成立",
    ).toBe(1);
  });

  it("每条例外都得说清为什么，不许只登记文件名", () => {
    for (const [file, why] of EXEMPT) {
      expect(why.length, `${file} 的例外理由太短，像是占位`).toBeGreaterThan(30);
      expect(
        /够不到|包装本体|读口本体|另一个模块|模块级/.test(why),
        `${file} 的例外理由没说清它为什么走不了唯一取数点：「${why}」`,
      ).toBe(true);
    }
  });

  it("例外表不许长草 —— 登记的文件必须真的还在取数", () => {
    const sites = new Map(fetchSites().map((s) => [s.file, s.count]));
    for (const [entry] of EXEMPT) {
      const file = entry.split("::")[0];
      expect(sites.has(file), `例外表里的 ${file} 已经不取数了 —— 删掉这条，别留僵尸账`).toBe(
        true,
      );
    }
  });
});
