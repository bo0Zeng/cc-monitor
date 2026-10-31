/**
 * vitest 两组的判据（`vitest.config.ts` 的 `test.projects`）：不碰 DOM 的那批在 `node` 组跑，余下的在 `jsdom` 组。
 *
 * jsdom 环境每份测试文件要另花半秒上下建一份 DOM；不碰 DOM 的文件放进 node 组就省掉这一截。
 * node 组收一份明列的名单（逐份在 node 环境里跑过、条数与 jsdom 下一样才进），jsdom 组收全部、减掉那份名单
 * ⇒ 新文件默认进 jsdom（慢些，但不会因为没有 `window` 红）。
 *
 * 判的是 vitest 自己挑文件那一步的结果（`globTestSpecifications`），不是另抄一份 glob：
 * ① 两组恰好叫 node · jsdom，环境各是 node · jsdom；jsdom 组的 include 是 `tests/**\/*.vitest.ts` 一整条。
 * ② node 组名单每一条都在盘上、不重复、照字典序排；vitest 真挑进 node 组的恰好是这份名单。
 * ③ 两组合起来恰好是全部 `tests/**\/*.vitest.ts`，没有一份两组都跑。
 * ④ 文件头的 `@vitest-environment` 与它所在那一组的环境一致（文件头盖过组的设定，不一致就是两处说法）。
 */
import { existsSync, readFileSync } from "node:fs";
import { relative, resolve } from "node:path";
import { createVitest } from "vitest/node";
import { beforeAll, describe, expect, it } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { filePaths } from "../../test-support/production-sources.ts";

const CONFIG = resolve(REPO_ROOT, "vitest.config.ts");

interface Project {
  extends?: boolean;
  test?: { name?: string; environment?: string; include?: string[]; exclude?: string[] };
}

/** 配置里写下的两组（读配置对象本身，不读字面）。 */
async function declaredProjects(): Promise<Project[]> {
  // 路径现拼、不写成字面量：写成字面量 tsc 会顺着它把 `vite.config.ts` 拉进程序。
  const cfg = ((await import(/* @vite-ignore */ CONFIG)) as { default: { test?: { projects?: Project[] } } }).default;
  return cfg.test?.projects ?? [];
}

/** vitest 真挑出来的文件：组名 → 相对仓根的路径（排好序）。 */
async function picked(): Promise<Map<string, string[]>> {
  const v = await createVitest("test", { config: CONFIG, root: REPO_ROOT, watch: false });
  try {
    const out = new Map<string, string[]>();
    for (const s of await v.globTestSpecifications()) {
      const name = s.project.name;
      out.set(name, [...(out.get(name) ?? []), relative(REPO_ROOT, s.moduleId).split("\\").join("/")]);
    }
    for (const files of out.values()) files.sort((a, b) => a.localeCompare(b));
    return out;
  } finally {
    await v.close();
  }
}

let projects: Project[];
let byGroup: Map<string, string[]>;
beforeAll(async () => {
  projects = await declaredProjects();
  byGroup = await picked();
});

const ALL = (): string[] => filePaths("tests", { ext: ".vitest.ts", tests: true });

describe("vitest 两组：node · jsdom", () => {
  it("① 恰好两组，各自的环境对；jsdom 组收全部 `tests/**/*.vitest.ts`", () => {
    const shape = projects.map((p) => ({ extends: p.extends, name: p.test?.name, environment: p.test?.environment }));
    expect(shape).toEqual([
      { extends: true, name: "node", environment: "node" },
      { extends: true, name: "jsdom", environment: "jsdom" },
    ]);
    expect(projects[1].test?.include, "jsdom 组要收全部，新文件默认落在这一组").toEqual(["tests/**/*.vitest.ts"]);
  });

  it("② node 组名单：每一条都在盘上、不重复、照字典序；vitest 挑进 node 组的恰好是它", () => {
    const list = projects[0]?.test?.include ?? [];
    expect(list.length, "node 组名单是空的 —— 下面几条零命中地绿").toBeGreaterThan(0);
    const missing = list.filter((f) => !existsSync(resolve(REPO_ROOT, f)));
    expect(missing, "名单里这几份不在盘上（改名 / 删了没跟着改名单）").toEqual([]);
    expect(list.filter((f) => !/^tests\/.+\.vitest\.ts$/.test(f)), "名单只收 `tests/` 下的 `*.vitest.ts` 具体路径，不写 glob").toEqual([]);
    expect(new Set(list).size, "名单里有重复的").toBe(list.length);
    expect(list, "名单照字典序排（好查、好合）").toEqual([...list].sort((a, b) => a.localeCompare(b)));
    expect(byGroup.get("node")).toEqual(list);
  });

  it("③ 两组合起来恰好是全部 `tests/**/*.vitest.ts`，没有一份两组都跑", () => {
    const node = byGroup.get("node") ?? [];
    const jsdom = byGroup.get("jsdom") ?? [];
    expect([...byGroup.keys()].sort()).toEqual(["jsdom", "node"]);
    expect(node.filter((f) => jsdom.includes(f)), "两组都跑的").toEqual([]);
    expect([...node, ...jsdom].sort((a, b) => a.localeCompare(b))).toEqual(ALL());
  });

  it("④ 文件头的 `@vitest-environment` 与所在那一组的环境一致", () => {
    const envOf = new Map<string, string>();
    for (const [group, files] of byGroup) for (const f of files) envOf.set(f, group);
    const clash: string[] = [];
    for (const f of ALL()) {
      const m = /^\s*(?:\/\/|\/?\*+)\s*@vitest-environment\s+(\S+)/m.exec(readFileSync(resolve(REPO_ROOT, f), "utf8"));
      if (m && m[1] !== envOf.get(f)) clash.push(`${f}：文件头写 ${m[1]}，所在组是 ${envOf.get(f) ?? "（没有）"}`);
    }
    expect(clash, "文件头的环境与组对不上：要 node 就进 node 组名单，文件头删掉").toEqual([]);
  });
});
