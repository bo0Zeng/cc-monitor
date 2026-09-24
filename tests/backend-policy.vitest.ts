/**
 * 〔B2 · 条 66 · `设计/01 §3.3b`〕「退出行为」那个值**搬走了** —— 这一份钉「搬干净了」。
 *
 * ## 搬家前这里钉的是什么（留档，别当成今天的形状）
 *
 * P2s-Y4：策略**存进 monitor 的 config.json、每台机各记各的、先推后存、写盘串行**，
 * 外加一条接线钉「启动时真的推了一次」。那一整套的前提是「值住 monitor 这一侧」，
 * 而 `§3.3b ①` 逐字给过它真会犯的错（C 拿自己那份默认值悄悄改掉 B 的行为）⇒ 条 66 把值搬到了
 * **后端所在那台机器上**，只有后端写、决定那一刻现读。⇒ 那几条判据的对象整个不存在了，随之退役。
 *
 * ## 今天钉什么
 *
 * - **推送链与本地持久化零命中**（带正控）：前端源码里再也叫不出那条推送命令、
 *   也不再有「把这个值存进 config.json」的那几个名字；而新的两条命令**真的在**包装层里。
 * - 三态 / 壳 / 那几句话的逐格等号住 `settings/backend-section.vitest.ts`（`E3` / `E4`）。
 */
import { describe, it, expect } from "vitest";

import { productionTsFiles } from "./test-support/production-sources";
import { stripComments } from "./test-support/strip-comments";
import {
  describeBackendHealth,
  HEALTH_CLEAN,
  HEALTH_CRASHED,
  HEALTH_LAST_MISSING,
  HEALTH_UNKNOWN,
  type BackendHealth,
} from "../src/backend-policy";

/** `src/` 下全部生产 `.ts`，剥掉注释 —— 只看代码（遍历住 `test-support`，本文件不另写一份）。 */
function frontendCode(): { rel: string; code: string }[] {
  return productionTsFiles("src").map((f) => ({
    rel: f.file,
    code: stripComments(f.text, "ts"),
  }));
}

describe("B2 · 「退出行为」那个值不住前端", () => {
  it("★★ 推送链与本地持久化零命中 —— 两向带正控", () => {
    const files = frontendCode();
    expect(
      files.length,
      "只扫到这么几份前端源码 —— 遍历坏了，零命中在空人群上恒绿",
    ).toBeGreaterThan(100);
    // 名字**运行时拼**：本文件自己不在人群里（它住 tests/），但这样读者一眼看得出它们是「已退役的名字」。
    const retired = [
      ["set_backend_", "kill_on_exit"],
      ["init", "BackendPolicy"],
      ["push", "PolicyToBackend"],
      ["set", "KillOnExit"],
      ["backend", "Policy:"],
    ].map((p) => p.join(""));
    for (const name of retired) {
      const hits = files.filter((f) => f.code.includes(name)).map((f) => f.rel);
      expect(
        hits,
        `退役的名字 \`${name}\` 还在前端代码里：${hits.join(" / ")}\n` +
          "`设计/01 §3.3b ②④`：monitor 的 config 里不许再留一份、那条推送随之退役 —— 留着就是第二个真相源。",
      ).toEqual([]);
    }
    // ★ 正控：新的两条命令**真的**在包装层里（否则上面那几条零命中可能只是「整片都没了」）。
    const cmds = files.find((f) => f.rel === "src/ipc/commands.ts");
    expect(cmds, "找不到 ipc/commands.ts —— 人群坏了").toBeDefined();
    for (const name of ["backend_exit_policy", "set_backend_exit_policy"]) {
      expect(
        cmds!.code.includes(`invoke<Record<string, unknown>>("${name}"`),
        `包装层里没有 ${name}`,
      ).toBe(true);
    }
  });
});

/**
 * K-P3 KP3C 的 TS 那一半 ——〔`K-P3` `§3-5` 第四行登记的欠账，`K-P3b` 补上〕。
 *
 * 交付第一档时这份文件不在那件的写区 ⇒ TS 侧只有 Rust 那侧的**源码文本**判据
 * （`the_health_reading_branches_are_wired_into_the_typescript`，它证的是「那三支写在那儿」）。
 * 这一组是**运行时**那一半：三档逐格等号。
 */
describe("K-P3 那句读数 —— 三档逐格钉死", () => {
  const NONE: BackendHealth = {
    crashed: 0,
    refused: 0,
    neverStarted: 0,
    misread: 0,
    last: null,
  };

  it("★★ 第一档：账上一条都没有 ⇒「答不出来」，**即使 crashed === 0**", () => {
    expect(
      describeBackendHealth({ ...NONE }),
      "一条记录都没有的机器被说成了别的 —— §0-1 逐字：\n" +
        "「今天不是『它没崩过』，是『没有任何东西在记它崩没崩』…… 这两句话差得很远，不许混用」。",
    ).toBe(HEALTH_UNKNOWN);
    // ★ 这一格是**判准本身**：把 `seen === 0` 换成 `crashed === 0`，上面那格照样绿，
    //   而这一格当场红 —— 一台记到过 2 次读坏了的机器会被说成「答不出来」，
    //   可它明明有账。**两格一起才钉住那个判准。**
    expect(
      describeBackendHealth({ ...NONE, misread: 2 }),
      "账上记到过事（读坏了 2 次），却仍说「答不出来」——\n" +
        "那说明第一档的判准是 `crashed === 0` 而不是「这本账上一条记录都没有」。",
    ).not.toBe(HEALTH_UNKNOWN);
  });

  it("★ 第二档：记到过事、一次崩溃都没有 ⇒「没崩过」，且带得出读坏了几次", () => {
    expect(describeBackendHealth({ ...NONE, misread: 2 })).toBe(
      HEALTH_CLEAN.replace("{misread}", "2"),
    );
    // 「读坏了」不算它崩 —— 那是我们这一侧的读端（B1 那条错误诊断的全部内容）。
    expect(describeBackendHealth({ ...NONE, refused: 1, misread: 0 })).toBe(
      HEALTH_CLEAN.replace("{misread}", "0"),
    );
  });

  it("★ 第三档：崩过 ⇒ 带次数与最后那一行；那一行没留住也要说出口", () => {
    expect(describeBackendHealth({ ...NONE, crashed: 3, last: "甲那一行" })).toBe(
      HEALTH_CRASHED.replace("{crashed}", "3").replace("{last}", "甲那一行"),
    );
    expect(
      describeBackendHealth({ ...NONE, crashed: 1, last: null }),
      "崩过、而那一行没留住 —— 这一格不许拿空串糊过去",
    ).toBe(HEALTH_CRASHED.replace("{crashed}", "1").replace("{last}", HEALTH_LAST_MISSING));
  });

  it("★ 占位符必须真的被填掉 —— 漏一个 replace 就把 `{crashed}` 端到用户眼前", () => {
    for (const h of [
      { ...NONE, misread: 5 },
      { ...NONE, crashed: 2, last: "乙" },
      { ...NONE, crashed: 2, last: null },
    ]) {
      const said = describeBackendHealth(h);
      for (const ph of ["{crashed}", "{last}", "{misread}"]) {
        expect(said.includes(ph), `读数里还留着占位符 ${ph}：${said}`).toBe(false);
      }
    }
  });
});
