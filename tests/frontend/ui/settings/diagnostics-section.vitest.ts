/**
 * 设置窗「日志」页（`diagnostics-section.ts`）：
 * - 「日志写入文件」改了 ⇒ 行内「重启后生效」＋ 顶上那条；拨回这次运行起来时的值 ⇒ 两处一起消。
 * - 会写回壳的三格读回来之前不可交互；读不到 ⇒ 原因落在这一页上。
 * - 存失败 ⇒ 拇指退回 ＋ 行下一句。
 * - 本机 cc-monitor 输出：有 ⇒ 路径 ＋［打开］打开恰是它；没有 ⇒ 只说没有，不摆按钮。
 * - 复制诊断信息：读到那一份之前置灰（悬停说为什么）；读到 ⇒ 复制的是壳出的那一整段；剪贴板不可用 ⇒ 只读文本框全选好。
 * - 「未识别数据」那一行读同一份答复里的数（读不到的那台照实说读不到）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { setDiag, getDiag, logInfo, opened, report } = vi.hoisted(() => ({
  setDiag: { fail: null as Error | null, calls: [] as unknown[] },
  getDiag: { fail: null as Error | null, logEnabled: true },
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
  report: {
    fail: null as Error | null,
    value: {
      text: "cc-monitor 诊断信息\n版本 4.1.1",
      unknown: [
        { machine: "本机", records: 0 },
        { machine: "devbox", records: 3 },
        { machine: "gpu-01", records: null },
      ],
      configUnknown: 2,
    },
  },
}));

vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: {
    set_diagnostics_config: (a: unknown) => {
      setDiag.calls.push(a);
      return setDiag.fail ? Promise.reject(setDiag.fail) : Promise.resolve("none");
    },
    get_diagnostics_config: () =>
      getDiag.fail
        ? Promise.reject(getDiag.fail)
        : Promise.resolve({ log_enabled: getDiag.logEnabled, log_level: "info", error_toast: true, max_files: 3 }),
    get_log_file_info: () => Promise.resolve(logInfo.value),
    diagnostics_report: () => (report.fail ? Promise.reject(report.fail) : Promise.resolve(report.value)),
    open_log_dir: () => Promise.resolve(),
  },
}));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: opened }));
vi.mock("@tauri-apps/api/path", () => ({ homeDir: () => Promise.resolve("/d") }));

import { DiagnosticsSection } from "../../../../src/frontend/ui/settings/diagnostics-section";
import { restartReasons, __resetRestartNoticeForTests } from "../../../../src/frontend/ui/settings/restart-notice";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const settle = () => new Promise((r) => setTimeout(r, 0));

async function loaded(): Promise<DiagnosticsSection> {
  const sec = new DiagnosticsSection();
  document.body.appendChild(sec.element);
  sec.loadNow();
  await settle();
  await settle();
  return sec;
}

/** 名字是 `label` 的那个开关。 */
function sw(el: HTMLElement, label: string): HTMLButtonElement {
  const row = [...el.querySelectorAll("label")].find((l) => l.querySelector("span")?.firstChild?.textContent === label);
  expect(row, `找不到开关「${label}」—— 下面是空真`).toBeTruthy();
  return row!.querySelector<HTMLButtonElement>("[role=switch]")!;
}

const fileSwitch = (el: HTMLElement) => sw(el, copyText("diagnostics.file.enable"));
const restartLine = (el: HTMLElement) => el.querySelector<HTMLElement>(".diag-restart")!;

beforeEach(() => {
  __resetRestartNoticeForTests();
  setDiag.fail = null;
  setDiag.calls = [];
  getDiag.fail = null;
  getDiag.logEnabled = true;
  report.fail = null;
  logInfo.value.backend_stderr = [];
  opened.mockClear();
  document.body.replaceChildren();
});

describe("日志写入文件：改了要重启、拨回原值两处一起消", () => {
  it("★ 拨离这次运行起来时的值 ⇒ 行内一句 ＋ 顶上那条；拨回 ⇒ 两处都消", async () => {
    const sec = await loaded();
    expect(restartLine(sec.element).hidden, "前提：没改过时不说要重启").toBe(true);
    fileSwitch(sec.element).click();
    await settle();
    expect(setDiag.calls, "先确认真的交出去了（否则下面断言是空转）").toHaveLength(1);
    expect(restartLine(sec.element).hidden).toBe(false);
    expect(restartReasons()).toContain(copyText("diagnostics.save.fileSwitch"));
    fileSwitch(sec.element).click();
    await settle();
    expect(restartLine(sec.element).hidden, "拨回原值了还说要重启").toBe(true);
    expect(restartReasons(), "拨回原值了顶上那条还挂着").toEqual([]);
  });

  it("★ 存失败 ⇒ 拇指退回 ＋ 行下一句，不说要重启", async () => {
    const sec = await loaded();
    // 壳命令失败带复制详情（`SaidError` 那一形）⇒ 那一句原样上屏。
    setDiag.fail = Object.assign(new Error("写不进去-xyz"), { detail: "d-xyz" });
    fileSwitch(sec.element).click();
    await settle();
    expect(fileSwitch(sec.element).getAttribute("aria-checked"), "存失败了拇指还停在新位置").toBe("true");
    expect(sec.element.textContent).toContain("写不进去-xyz");
    expect(restartReasons()).toEqual([]);
  });
});

describe("会写回壳的三格：读回来之前不可交互", () => {
  const ready = (el: HTMLElement) => [
    !el.querySelector("select")!.disabled,
    fileSwitch(el).getAttribute("aria-disabled") !== "true",
    sw(el, copyText("diagnostics.toast.enable")).getAttribute("aria-disabled") !== "true",
  ];

  it("还没读：三个都灰着；读成功：三个都亮起", async () => {
    const sec = new DiagnosticsSection();
    expect(ready(sec.element)).toEqual([false, false, false]);
    sec.loadNow();
    await settle();
    expect(ready(sec.element)).toEqual([true, true, true]);
  });

  it("读失败：那一句落在这一页上（原话进［复制详情］，不上句子），三个继续灰着", async () => {
    getDiag.fail = Object.assign(new Error("raw-1"), { detail: "d-raw-1" });
    const sec = await loaded();
    expect(ready(sec.element)).toEqual([false, false, false]);
    expect(sec.element.textContent).toContain(copyText("diagnostics.refresh.settingsUnreadable"));
    expect(sec.element.textContent).not.toContain("raw-1");
    expect(sec.element.querySelector('[data-part="copy-detail"]'), "没出［复制详情］").not.toBeNull();
  });
});

describe("文件：本机 cc-monitor 输出", () => {
  const row = (el: HTMLElement) => el.querySelector<HTMLElement>("[data-role=local-output]")!;

  it("★ 有那份文件：显示路径（~ 缩写）与大小，［打开］打开的恰是它（新在前的第一份）", async () => {
    logInfo.value.backend_stderr = [
      { path: "/d/logs/backend/stderr.log", size_bytes: 2048, modified_ms: 2 },
      { path: "/d/logs/backend/stderr.old.log", size_bytes: 9, modified_ms: 1 },
    ];
    const sec = await loaded();
    expect(row(sec.element).textContent).toContain("~/logs/backend/stderr.log");
    expect(row(sec.element).textContent).not.toContain("stderr.old.log");
    row(sec.element).querySelector("button")!.click();
    await settle();
    expect(opened).toHaveBeenCalledWith("/d/logs/backend/stderr.log");
  });

  it("★ 另一向：没有那份文件 ⇒ 只说没有，不摆按钮", async () => {
    const sec = await loaded();
    expect(row(sec.element).textContent).toContain(copyText("diagnostics.files.outputNone"));
    expect(row(sec.element).querySelector("button")).toBeNull();
  });
});

describe("复制诊断信息 · 未识别数据那一行", () => {
  it("★ 读到之前置灰（悬停说为什么）；读到 ⇒ 亮起，复制的是壳出的那一整段", async () => {
    const write = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText: write } });
    const sec = new DiagnosticsSection();
    const head = sec.headButton();
    const btns = [head, ...sec.element.querySelectorAll<HTMLButtonElement>("[data-role=copy-diagnostics]")];
    expect(btns.length, "页头一颗 ＋ 那一行一颗").toBe(2);
    expect(btns.every((b) => b.getAttribute("aria-disabled") === "true")).toBe(true);
    expect(head.title).toBe(copyText("diagnostics.copy.notYet"));
    sec.loadNow();
    await settle();
    await settle();
    expect(btns.every((b) => b.getAttribute("aria-disabled") === null)).toBe(true);
    head.click();
    await settle();
    expect(write).toHaveBeenCalledWith(report.value.text);
  });

  it("★ 读不到 ⇒ 复制一直灰着、那一行说读不到（不拿空段冒充「都认得」）", async () => {
    report.fail = Object.assign(new Error("壳没答-xyz"), { detail: "d-xyz" });
    const sec = new DiagnosticsSection();
    const head = sec.headButton();
    sec.loadNow();
    await settle();
    await settle();
    expect(head.getAttribute("aria-disabled")).toBe("true");
    expect(sec.element.textContent).toContain("壳没答-xyz");
  });

  it("那一行读同一份答复：有数的那台 · 读不到的那台 · config.json 几项", async () => {
    const sec = await loaded();
    const line = sec.element.textContent ?? "";
    expect(line).toContain(copyText("diagnostics.unknown.machine", { machine: "devbox", n: 3 }));
    expect(line).toContain(copyText("diagnostics.unknown.machineUnread", { machine: "gpu-01" }));
    expect(line).toContain(copyText("diagnostics.unknown.config", { n: 2 }));
    expect(line, "0 条的那台不该占一格").not.toContain(copyText("diagnostics.unknown.machine", { machine: "本机", n: 0 }));
  });

  it("★ 剪贴板不可用 ⇒ 页内一个只读文本框、全选好，说按 Ctrl+C", async () => {
    Object.assign(navigator, { clipboard: { writeText: vi.fn().mockRejectedValue(new Error("no clipboard")) } });
    const sec = await loaded();
    const head = sec.headButton();
    head.click();
    await settle();
    const box = sec.element.querySelector<HTMLTextAreaElement>(".diag-fallback textarea");
    expect(box, "没给退路").toBeTruthy();
    expect(box!.value).toBe(report.value.text);
    expect(box!.readOnly).toBe(true);
    expect([box!.selectionStart, box!.selectionEnd], "没全选好").toEqual([0, report.value.text.length]);
    expect(sec.element.textContent).toContain(copyText("diagnostics.copy.fallback"));
  });
});
