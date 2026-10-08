// @vitest-environment node
/**
 * 门禁里「机器忙时谁让路」那两手的判据（`tests/scripts/gate.sh` 头上「机器忙时谁让路」一段）。
 *
 * ① **vitest 分核**：门禁里同时各起一整套 vitest 的那几格（今天 `npm` · `coverage`），并跑时每格拿到的 worker 数
 *    合起来**不超过核数**；不并跑时一格拿满。判法：把 gate.sh 里「vitest 分核」那一段原样抠出来，在 bash 里按几种核数真算一遍。
 *    人群两向：跑整套 vitest 的格（命令里有 `npm test` / 覆盖率那一步 / `vitest`）== 套了 `gate_vitest_share` 的格
 *    == `GATE_VITEST_CELLS` 登记的格。另判 `vitest.config.ts` 真把 `GATE_VITEST_WORKERS` 当 `maxWorkers`（设 / 不设 / 乱设三态）。
 * ② **让路表**：按墙钟判（`GATE_WALL_CELLS`）与让路（`GATE_YIELD_CELLS`）两张表不相交，并起来 == 盘上声明的格（两向）。
 *
 * ⚠ 买不到：worker 数只管 vitest 自己起的线程 / 进程，用例里再起的子进程（`node --check`、`vite build`）不在数里；
 *   `nice` 只排 CPU，不排内存与磁盘；门禁外面别的路起的负载两手都管不着（那归 `gate-clean.sh` 拿锁时那一看）。
 */
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

const GATE = readFileSync(resolve(REPO_ROOT, "tests/scripts/gate.sh"), "utf8");
const CONFIG = resolve(REPO_ROOT, "vitest.config.ts");

/** `run_gate <名> '<分母>' \` 之后续行拼起来的命令（到第一行不以 `\` 收尾为止）。顶格的那几行才是盘上的格。 */
function cellCommands(text: string): Map<string, string> {
  const lines = text.split("\n");
  const out = new Map<string, string>();
  for (let i = 0; i < lines.length; i++) {
    const m = /^run_gate(?:_sum)? ([A-Za-z0-9_-]+) /.exec(lines[i]);
    if (!m) continue;
    let cmd = lines[i];
    let j = i;
    while (/\\\s*$/.test(lines[j]) && j + 1 < lines.length) cmd += `\n${lines[++j]}`;
    out.set(m[1], cmd);
  }
  return out;
}

/** 盘上声明的格名：`run_gate` / `run_gate_sum` / 顶格 `gate_cell <名>`；`run_e2e` 各套记成一个 `e2e`。 */
function declaredCells(text: string): Set<string> {
  const out = new Set<string>(cellCommands(text).keys());
  for (const m of text.matchAll(/^\s*gate_cell ([A-Za-z0-9_-]+) /gm)) out.add(m[1]);
  if (/^run_e2e /m.test(text)) out.add("e2e");
  return out;
}

const listVar = (text: string, name: string): string[] => {
  const m = new RegExp(`^${name}="([^"]*)"`, "m").exec(text);
  return m ? m[1].trim().split(/\s+/) : [];
};

/** 跑整套 vitest 的格 / 套了分核的格 / 登记的格。 */
function vitestCells(text: string): { runsVitest: string[]; shared: string[]; registered: string[] } {
  const cmds = [...cellCommands(text)];
  const reg = /^GATE_VITEST_CELLS=\(([^)]*)\)/m.exec(text);
  return {
    runsVitest: cmds.filter(([, c]) => /\bnpm test\b|coverage floor|\bvitest\b/.test(c.split("\n").slice(1).join("\n"))).map(([n]) => n).sort(),
    shared: cmds.filter(([, c]) => /\bgate_vitest_share\b/.test(c)).map(([n]) => n).sort(),
    registered: reg ? reg[1].trim().split(/\s+/).sort() : [],
  };
}

/** 「vitest 分核」那一段，在 bash 里按给定核数与并不并跑真算：每格几个 worker、几格。 */
function share(text: string, cores: number, par: boolean): { each: number; cells: number } {
  const m = /# >>> vitest 分核\n([\s\S]*?)# <<< vitest 分核/.exec(text);
  if (!m) throw new Error("gate.sh 里找不到「# >>> vitest 分核 … # <<< vitest 分核」那一段");
  const script = `nproc() { echo ${cores}; }\nGATE_PAR=${par ? 1 : 0}\n${m[1]}\necho "$GATE_VITEST_EACH \${#GATE_VITEST_CELLS[@]}"`;
  const [each, cells] = execFileSync("bash", ["-c", script], { encoding: "utf8" }).trim().split(" ").map(Number);
  return { each, cells };
}

describe("门禁 · 机器忙时谁让路", () => {
  it("① 跑整套 vitest 的格 == 套了分核的格 == 登记的格（两向）；量具正控", () => {
    const got = vitestCells(GATE);
    expect(got.runsVitest, "认不出跑整套 vitest 的格 —— 量具坏了，下面那条零命中地绿").toEqual(["coverage", "npm"]);
    expect(got.shared, "跑整套 vitest 的格没套 `gate_vitest_share`（并跑时它按默认核数 − 1 起 worker）").toEqual(got.runsVitest);
    expect(got.registered, "`GATE_VITEST_CELLS` 与真套了分核的格对不上 —— 每格分到的份数按登记的格数算").toEqual(got.shared);
    // 正控：npm 那格摘掉 `gate_vitest_share` ⇒ 量具认得出
    const broken = GATE.replace(/gate_vitest_share npm test/, "npm test");
    expect(broken).not.toBe(GATE);
    expect(vitestCells(broken).shared).toEqual(["coverage"]);
  });

  it("① 并跑时各格 worker 合起来不超过核数（每格至少 1）；不并跑时一格拿满；量具反控", () => {
    for (const cores of [2, 3, 8, 23, 24, 64]) {
      const p = share(GATE, cores, true);
      expect(p.cells, "登记的格数读不出来").toBeGreaterThanOrEqual(2);
      expect(p.each, `${cores} 核：每格 worker 不到 1`).toBeGreaterThanOrEqual(1);
      expect(p.each * p.cells, `${cores} 核：${p.cells} 格各 ${p.each} 个 worker，合起来超过核数`).toBeLessThanOrEqual(cores);
      expect(share(GATE, cores, false).each, `${cores} 核不并跑：一次只有一格在跑，该拿满核数`).toBe(cores);
    }
    // 反控：每格都拿满核数的写法必须红在「合起来超过核数」上
    const greedy = GATE.replace(/GATE_VITEST_EACH=\$\(\( \$\(nproc\) \/ \$\{#GATE_VITEST_CELLS\[@\]\} \)\)/, "GATE_VITEST_EACH=$(nproc)");
    expect(greedy).not.toBe(GATE);
    const g = share(greedy, 24, true);
    expect(g.each * g.cells).toBeGreaterThan(24);
  });

  it("① vitest.config.ts：设了 GATE_VITEST_WORKERS 就当 maxWorkers；不设照默认；不是正整数当场报错", async () => {
    const load = async (v: string | undefined): Promise<{ maxWorkers?: number }> => {
      vi.resetModules();
      vi.stubEnv("GATE_VITEST_WORKERS", v);
      try {
        // 路径现拼、不写成字面量：写成字面量 tsc 会顺着它把 `vite.config.ts` 拉进程序（那份不在 include 里，另有自己的类型约定）。
        const cfg = ((await import(/* @vite-ignore */ CONFIG)) as { default: { test?: { maxWorkers?: number } } }).default;
        return cfg.test ?? {};
      } finally {
        vi.unstubAllEnvs();
      }
    };
    expect((await load("7")).maxWorkers).toBe(7);
    expect((await load(undefined)).maxWorkers).toBeUndefined();
    await expect(load("half")).rejects.toThrow(/不是正整数/);
    await expect(load("0")).rejects.toThrow(/不是正整数/);
  });

  it("② 按墙钟判 / 让路两张表不相交，并起来 == 盘上声明的格（两向）；run_gate 真按让路表降调", () => {
    const wall = listVar(GATE, "GATE_WALL_CELLS");
    const yieldList = listVar(GATE, "GATE_YIELD_CELLS");
    const declared = declaredCells(GATE);
    expect(declared.size, "盘上的格一个都没认出来 —— 量具坏了").toBeGreaterThan(30);
    expect(wall.filter((c) => yieldList.includes(c)), "同一格既按墙钟判又让路").toEqual([]);
    expect([...declared].filter((c) => !wall.includes(c) && !yieldList.includes(c)).sort(), "这几格没归边：按墙钟判还是让路，回 gate.sh 头上那两张表写清").toEqual([]);
    expect([...wall, ...yieldList].filter((c) => !declared.has(c)).sort(), "表里这几格盘上没有 —— 删了 / 改名了就从表里摘").toEqual([]);
    // 降调真接在 run_gate 上（只写表不接线等于没做）；让路表里认不认得出格名，用表里一格真跑一遍 `gate_yields`。
    expect(/^run_gate\(\) \{[\s\S]*?gate_yields "\$name"[\s\S]*?gate_exec_cmd gate_yield/m.test(GATE), "run_gate 没按 gate_yields 套 gate_yield").toBe(true);
    const fn = /^gate_yields\(\) \{.*\}$/m.exec(GATE)?.[0];
    expect(fn, "gate.sh 里找不到单行的 gate_yields").toBeDefined();
    const ask = (cell: string): string =>
      execFileSync("bash", ["-c", `GATE_YIELD_CELLS=" ${yieldList.join(" ")} "\n${fn}\ngate_yields "$1" && echo y || echo n`, "_", cell], { encoding: "utf8" }).trim();
    expect(ask("clippy")).toBe("y");
    expect(ask("npm")).toBe("n");
    expect(ask("clip"), "前缀不许误中").toBe("n");
  });
});
