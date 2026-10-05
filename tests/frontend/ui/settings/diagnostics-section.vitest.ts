/**
 * S7 收尾（Phase G 核账逮出来的）：**诊断分节要给「待生效」那条常驻条供货**。
 *
 * 病灶：后端对「启用 log 文件」这一项明确回 `RestartHint = "needs_restart"`，
 * 而前端此前只弹一个 6 秒 toast。S7 立的规矩是「有改动没生效」是**状态**、
 * 唯一去处是底部那条常驻条 —— 漏了这个供给方，用户切完 log 开关、错过 toast，
 * 再看条子是空的，会读成「没有待生效的改动」。
 * **条子的存在本身让它显得权威**，所以漏供比没有条子更误导。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { setDiag, restartHint, getDiag, logInfo, opened } = vi.hoisted(() => ({
  setDiag: vi.fn(),
  restartHint: { value: "none" as "none" | "needs_restart" },
  getDiag: { fail: null as Error | null },
  // `get_log_file_info` 的应答（生成物 `LogFileInfo` 的形状）。
  logInfo: {
    value: {
      dir: "/d/logs",
      current_file: "/d/logs/monitor.2026-09-25.log",
      current_size_bytes: 10,
      all_files: [] as unknown[],
      backend_stderr: [] as { path: string; size_bytes: number; modified_ms: number }[],
    },
  },
  opened: vi.fn(),
}));

vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: {
    set_diagnostics_config: (...a: unknown[]) => {
      setDiag(...a);
      return Promise.resolve(restartHint.value);
    },
    get_diagnostics_config: () =>
      getDiag.fail ? Promise.reject(getDiag.fail) : Promise.resolve({
        log_enabled: true,
        log_level: "info",
        error_toast: true,
        max_files: 3,
      }),
    get_log_file_info: () => Promise.resolve(logInfo.value),
  },
}));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: opened }));

import { DiagnosticsSection } from "../../../../src/frontend/ui/settings/diagnostics-section";
import {
  restartReasons,
  __resetRestartNoticeForTests,
} from "../../../../src/frontend/ui/settings/restart-notice";

/** 切一下「启用 log 文件」复选框，等异步 save 落地。 */
async function toggleLogEnabled(): Promise<void> {
  const sec = new DiagnosticsSection();
  sec.loadNow();
  await new Promise((r) => setTimeout(r, 0));
  const cb = sec.element.querySelector<HTMLInputElement>(
    'input[type="checkbox"]',
  )!;
  cb.checked = !cb.checked;
  cb.dispatchEvent(new Event("change"));
  // save() → invoke → then；两个微任务轮足够
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

describe("诊断分节 → 「需重启」常驻条", () => {
  beforeEach(() => {
    __resetRestartNoticeForTests();
    setDiag.mockClear();
    restartHint.value = "none";
    document.body.replaceChildren();
  });

  it("★ 后端说 needs_restart → 条子上必须列出这一项（不能只弹一个会消失的 toast）", async () => {
    restartHint.value = "needs_restart";
    await toggleLogEnabled();
    expect(setDiag, "先确认 save 真的发出去了（否则下面断言是空转）").toHaveBeenCalled();
    // 改名：块叫「日志」、那一项叫「日志文件」⇒ 条子上的理由跟着改。
    expect(restartReasons()).toContain("日志文件开关");
  });

  it("★ 反向自检：后端说 none 时**不许**点亮条子（恒亮 = 背景噪音）", async () => {
    restartHint.value = "none";
    await toggleLogEnabled();
    expect(setDiag).toHaveBeenCalled();
    expect(restartReasons()).toEqual([]);
  });
});

// 那处真缺陷 ＋ `§8` 判据 #2：读不到当前设置时，三个控件**不许**顶着构造期默认值给人点。
describe("日志分节：读不到当前设置 ⇒ 说出来，并且三个控件读回来之前不可交互", () => {
  beforeEach(() => {
    getDiag.fail = null;
    document.body.replaceChildren();
  });
  const controls = (el: HTMLElement) => [
    ...el.querySelectorAll<HTMLInputElement | HTMLSelectElement>("input[type=checkbox], select"),
  ];

  it("还没读：三个都灰着（构造期那几个默认值不是后端的真状态）", () => {
    const sec = new DiagnosticsSection();
    expect(controls(sec.element).length, "控件找不到 —— 下面是空真").toBe(3);
    expect(controls(sec.element).every((c) => c.disabled)).toBe(true);
  });

  it("读成功：三个都亮起、没有失败那一行", async () => {
    const sec = new DiagnosticsSection();
    sec.loadNow();
    await new Promise((r) => setTimeout(r, 0));
    expect(controls(sec.element).every((c) => !c.disabled)).toBe(true);
    expect(sec.element.querySelector(".settings-banner-show")).toBeNull();
  });

  it("读失败：原因落在这一块上，三个继续灰着", async () => {
    getDiag.fail = new Error("后端没起来");
    const sec = new DiagnosticsSection();
    sec.loadNow();
    await new Promise((r) => setTimeout(r, 0));
    expect(controls(sec.element).every((c) => c.disabled)).toBe(true);
    expect(sec.element.querySelector(".settings-banner-show")?.textContent).toContain("后端没起来");
  });

  it("先读成功、再读失败：三个重新灰掉（上一次的值此刻已经不能当真）", async () => {
    const sec = new DiagnosticsSection();
    sec.loadNow();
    await new Promise((r) => setTimeout(r, 0));
    expect(controls(sec.element).every((c) => !c.disabled), "前提：先得亮起来").toBe(true);
    getDiag.fail = new Error("第二次读挂了");
    [...sec.element.querySelectorAll("button")].find((b) => b.textContent === "刷新信息")!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(controls(sec.element).every((c) => c.disabled)).toBe(true);
  });
});

// 守的要求（住址，纪律 19）：「本机 · 脱离常驻载体 | null | **仍开**」·
//   要求：「脱离载体的常驻后端 stderr 落本机日志文件（有上限、滚动），设置页『日志』里看得到」。
describe("日志分节：本机后端的输出（NT2 · S1）", () => {
  beforeEach(() => {
    getDiag.fail = null;
    opened.mockClear();
    document.body.replaceChildren();
  });
  const row = (el: HTMLElement) =>
    [...el.querySelectorAll(".settings-row")].find((r) =>
      r.textContent?.startsWith("本机后端的输出"),
    )!;

  it("★ 有那份文件：显示路径与大小，「打开」打开的恰是它（新在前的第一份）", async () => {
    logInfo.value.backend_stderr = [
      { path: "/d/logs/backend/stderr.log", size_bytes: 2048, modified_ms: 2 },
      { path: "/d/logs/backend/stderr.old.log", size_bytes: 9, modified_ms: 1 },
    ];
    const sec = new DiagnosticsSection();
    sec.loadNow();
    await new Promise((r) => setTimeout(r, 0));
    const r = row(sec.element);
    expect(r, "找不到那一行 —— 下面是空真").toBeTruthy();
    expect(r.textContent).toContain("/d/logs/backend/stderr.log");
    expect(r.textContent).not.toContain("stderr.old.log");
    const btn = r.querySelector("button")!;
    expect(btn.disabled).toBe(false);
    btn.click();
    await new Promise((r2) => setTimeout(r2, 0));
    expect(opened).toHaveBeenCalledWith("/d/logs/backend/stderr.log");
  });

  it("★ 另一向：没有那份文件 ⇒ 说清为什么没有，「打开」灰着、点不出东西", async () => {
    logInfo.value.backend_stderr = [];
    const sec = new DiagnosticsSection();
    sec.loadNow();
    await new Promise((r) => setTimeout(r, 0));
    const r = row(sec.element);
    expect(r.textContent).toContain("还没有");
    const btn = r.querySelector("button")!;
    expect(btn.disabled).toBe(true);
    btn.click();
    await new Promise((r2) => setTimeout(r2, 0));
    expect(opened).not.toHaveBeenCalled();
  });
});
