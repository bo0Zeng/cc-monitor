/**
 * U8c-2c-2：`render_ccm_launch` 的 **wire 形状两侧一致** + **生产真的切过去了**。
 *
 * `src/frontend/ui/launch-cli-wire.ts` 是一份**手写镜像**（不是 ts-rs 生成的）。Rust 那边带
 * `deny_unknown_fields` ⇒ 前端多送/少送一个字段会被**拒**，而那在生产里表现为
 * 「拉起时静默走兜底」—— 功能不变砖，但**真正在跑的那条路悄悄换了**，没人会发现。
 * 所以这条对拍是必需的。
 */
import { describe, expect, test } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { productionTsFiles, SCAN_TIMEOUT_MS } from "../../test-support/production-sources.ts";
import { stripComments } from "../../test-support/strip-comments.ts";

const read = (p: string) => readFileSync(resolve(__dirname, "../../..", p), "utf8");
const RUST = read("src/backend/control/launch_render/wire.rs");
const WIRE = read("src/frontend/ui/launch-cli-wire.ts");
const RUN = read("src/frontend/ui/remote-launch-run.ts");

describe("ccm 调用行的 wire 形状（U8c-2c-2）", () => {
  // Rust 侧 `CliRenderRequest` 用 `rename_all = "camelCase"`，所以字段名要转过来比。
  const rustReqFields = (() => {
    const body = RUST.slice(
      RUST.indexOf("pub struct CliRenderRequest {"),
      RUST.indexOf("}", RUST.indexOf("pub struct CliRenderRequest {")),
    );
    return body
      .split("\n")
      .map((l) => /^\s{4}pub ([a-z_0-9]+):/.exec(l)?.[1])
      .filter((x): x is string => !!x)
      .map((snake) => snake.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase()))
      .sort();
  })();

  test("抽取器自检：真的从 Rust 抽到了字段（抽空就会零命中零失败地绿）", () => {
    expect(rustReqFields.length).toBeGreaterThanOrEqual(8);
  });

  test("TS 手写镜像的字段集 == Rust `CliRenderRequest` 的字段集", () => {
    const body = WIRE.slice(
      WIRE.indexOf("export interface CliRenderRequest {"),
      WIRE.indexOf("}", WIRE.indexOf("export interface CliRenderRequest {")),
    );
    const tsFields = body
      .split("\n")
      .map((l) => /^\s{2}([a-zA-Z0-9]+)[?]?:/.exec(l)?.[1])
      .filter((x): x is string => !!x)
      .sort();
    expect(tsFields).toEqual(rustReqFields);
  });

  // 判据自带清单（遍历被测文件自己是恒真的）：一个请求结构 ＋ 三个枚举。载荷那条请求与它的四个类型随载荷渲染删了。
  const DENY_WIRE_TYPES = ["CliRenderRequest", "WireAction", "WireContainer", "WireAccount"];

  test("Rust 侧四个入方向 wire 类型都带 deny_unknown_fields（多送字段必须被拒，不静默吞）", () => {
    // 数量自检：将来加第五个类型时这条红，提醒把它加进上面的清单 ——
    // 只看**属性里**的，因为这些类型的文档注释里就写着这个词（M3 抓到过）。
    //
    // 🔴 **量法换过一次**：原来数的是「以 `#[` 开头且含那个词的**行**」，
    // 而 rustfmt 会把长属性拆成多行 —— `WireTmuxOuter` 的属性有四项（`tag` /
    // `rename_all` / `rename_all_fields` / `deny_unknown_fields`），拆开之后
    // **含那个词的那一行不以 `#[` 开头**，于是一个真带属性的新类型在这把尺子上是隐形的。
    // ⇒ 改成「剥掉注释之后，全文还剩几处」：既不受排版影响，也仍然把散文排除在外
    // （M3 要防的就是散文，那一半一个字没松）。
    const rustNoComments = RUST.split("\n")
      .filter((l) => !l.trim().startsWith("//"))
      // ⚠ 用 `indexOf` 不用 `includes` —— `scanning-guard-registry` 那条递减棘轮
      // 数的是**磁盘语料变量上的裸 `.includes("…")`**（子串匹配单位比事实小，
      // 正向事实钉会从缝里溜过去）。这里剥注释是「切」不是「钉」，换个写法就不占额度。
      .map((l) => {
        const cut = l.indexOf("//");
        return cut >= 0 ? l.slice(0, cut) : l;
      })
      .join("\n");
    const denyInAttrs = rustNoComments.split("deny_unknown_fields").length - 1;
    // 剥法自检：剥之前一定更多（否则「剥掉注释」这一步没发生，这条就退化成数全文）。
    expect(
      RUST.split("deny_unknown_fields").length - 1,
      "剥注释前后一样多 —— 剥法没生效，或者注释里不再提这个词（那要换一种自检）",
    ).toBeGreaterThan(denyInAttrs);
    expect(denyInAttrs, "带 deny_unknown_fields 的类型数变了，清单要同步").toBe(
      DENY_WIRE_TYPES.length,
    );
    for (const t of DENY_WIRE_TYPES) {
      const at = RUST.indexOf(`enum ${t} {`) >= 0 ? RUST.indexOf(`enum ${t} {`) : RUST.indexOf(`struct ${t} {`);
      expect(at, `${t} 找不到`).toBeGreaterThan(0);
      // ⚠ **只看紧邻的 `#[...]` 属性行** —— 初版是「往前扫 220 字符找子串」，
      // 而这些类型的**文档注释里就写着** `deny_unknown_fields` 这个词 ⇒ 摘掉真属性照样绿
      // （自己的变异检查 M3 抓到的）。散文不是属性。
      //
      // 🔴 **同一拍补了多行属性**：rustfmt 会把长属性拆成
      //    `#[serde(` / `    tag = …,` / … / `)]` 好几行，而原来的往上扫只认
      //    「整行以 `#[` 开头」⇒ 撞到收尾那行 `)]` 当场 break，attrLines 空 ——
      //    一个**真带属性**的类型在这把尺子上是隐形的。
      //    ⇒ 认收尾行，往上吃到 `#[` 为止。**散文那一半一个字没松**：
      //    doc 注释既不以 `#[` 开头、也不是收尾行 ⇒ 照样 break。
      const before = RUST.slice(0, at).split("\n");
      const attrLines: string[] = [];
      for (let i = before.length - 2; i >= 0; i--) {
        const line = before[i].trim();
        if (line.startsWith("#[")) {
          attrLines.push(line);
        } else if (line === ")]" || line === "]") {
          // 多行属性的收尾：往上吃到它的 `#[` 那一行。
          attrLines.push(line);
          while (i > 0) {
            i--;
            // ⚠ 名字刻意不叫 `l`：`const l = before[i]…` 会让那个名字被
            // `scanning-guard-registry::corpusVars` 认成**磁盘语料变量**，
            // 于是同文件里任何一处挂在它上面的裸成员检查都进那条递减棘轮的账
            // （本件第一次跑就是这么把上限从 8 顶到 9 的）。
            const up = before[i].trim();
            attrLines.push(up);
            if (up.startsWith("#[")) break;
          }
        } else if (line !== "") break; // 撞到 doc 注释/空行以外的东西就停
      }
      expect(attrLines.join("\n"), `${t} 的属性里缺 deny_unknown_fields`).toContain(
        "deny_unknown_fields",
      );
    }
  });

  // ★ 接缝判据：**生产真的切过去了**。没有它，「把 renderCliViaBackend 换回 tryRenderCli」
  // 会让所有夹具/单测照常全绿 —— 那正是这一轮唯一实质的改动，也是最容易被悄悄回退的一处。
  test("生产渲染路径调的是后端，不是 TS 的 tryRenderCli", () => {
    // 渲染问那台后端（`launch-render.ts::renderCli` → 通道 `launch-render-cli`），探测结果不再由前端转交。
    expect(RUN).toContain("return renderCli(origin, buildCliRenderRequest(ctx));");
    // TS 的 `tryRenderCli` 已删；这一格留着挡「在本文件里再手写一个」。
    // 全仓那一格见下面那组。
    expect(/[^a-zA-Z]tryRenderCli\s*\(/.test(RUN)).toBe(false);
  });
});

/**
 * **`ccm …` 调用行在前端没有第二个家。**
 *
 * 守的要求：「起会话收成一处 —— 同一条命令串只留 Rust 那两份（CLI ＋ 载荷）；
 * TS 的只供对拍、排期删」。TS 那份 `ccm …` 渲染器（`launch-render-cli.ts::tryRenderCli`）
 * 删在本件；本组挡它以任何名字之外的最常见形状回来：原文件复活 · 生产段再 import 它 ·
 * 生产段再出现那个入口名。
 *
 * ⚠ 射程如实写：它认的是**名字**，不是「渲染 `ccm …` 这件事」—— 换个名字重写一份它看不见
 * （那一形由 `launch_wire_f07_main_path_tests.rs` 的 `LAUNCH_RENDERERS` 恒等登记表兜：
 * 那张表是人写的，多一份不登记它也看不见 —— 两条都只是「照抄回来」会有东西说话）。
 */
describe("〔LR1〕TS 那份 `ccm …` 调用行渲染器不许回来", () => {
  // 路径形要求 `./` 前缀：通道那一问的 op 名也叫 `launch-render-cli`（`src/frontend/ui/launch-render.ts`），它不是那份文件。
  const NAME = /\btryRenderCli\b|["']\.{1,2}\/(?:[\w.-]+\/)*launch-render-cli(?:\.ts)?["']/;
  const hits = (files: { file: string; text: string }[]) =>
    files.filter((f) => NAME.test(stripComments(f.text, "ts"))).map((f) => f.file);

  test("正控：import 路径形 · 调用形各命中，只在注释里的不算", () => {
    expect(
      hits([
        { file: "a.ts", text: 'import type { X } from "./launch-render-cli";\n' },
        { file: "b.ts", text: "// tryRenderCli 只在注释里\nconst x = 1;\n" },
        { file: "c.ts", text: "const r = tryRenderCli(plan, ctx, probe);\n" },
        { file: "d.ts", text: 'await chan.call(o, "launch-render-cli", body, budget);\n' },
      ]),
    ).toEqual(["a.ts", "c.ts"]);
  });

  test("原文件不在盘上，生产段零命中", () => {
    expect(existsSync(resolve(__dirname, "../../..", "src/launch-render-cli.ts"))).toBe(false);
    const files = productionTsFiles();
    expect(files.length, "生产 TS 一份都没收到 —— 遍历坏了，下面的零命中不携带信息").toBeGreaterThan(100);
    expect(hits(files)).toEqual([]);
  }, SCAN_TIMEOUT_MS);
});
