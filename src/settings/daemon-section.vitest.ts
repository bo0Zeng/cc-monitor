/**
 * P2s-Y5：**开关文案不许承诺做不到的事** + 这一区的行为。
 *
 * ## 措辞判据为什么是机检而不是评审
 *
 * DoD 原写 acceptor 是「评审」。评审**留不下判据** —— 下一个人加一行
 * 「关掉后 daemon 会在后台常驻」不会有任何东西红。
 * 而这条性质恰好是可机检的：扫本区**用户可见的字符串字面量**，禁用词一个都不许出现。
 *
 * ⚠ 判据的边界：它扫的是**字面量**，逮不到「用一句意思相同但用词不同的话去承诺常驻」。
 * 那一层仍然只能靠人 —— 如实登记在件的诚实边界里，不假装机检覆盖了它。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const calls: { name: string; args: unknown }[] = [];
let status: Record<string, unknown> = { channel: true, pid: 42 };
/** 按顺序喂给 `daemon_status` 的前几次读数（用完退回 `status`）。 */
let statusQueue: Record<string, unknown>[] = [];
let stored: Record<string, unknown> = {};

vi.mock("../ipc/commands", () => ({
  commands: {
    daemon_status: (a: unknown) => {
      calls.push({ name: "daemon_status", args: a });
      // ⚠ 支持「前几次还没落定」：A4 那条判据要证明它**轮询到落定**，
      // 而不是命令一返回就画一张操作前的快照。
      const next = statusQueue.shift();
      return Promise.resolve(next ?? status);
    },
    daemon_start: (a: unknown) => {
      calls.push({ name: "daemon_start", args: a });
      return Promise.resolve("已起");
    },
    daemon_stop: (a: unknown) => {
      calls.push({ name: "daemon_stop", args: a });
      return Promise.resolve("已停");
    },
    daemon_machines: () => {
      calls.push({ name: "daemon_machines", args: null });
      return Promise.resolve(["<local>", "甲机"]);
    },
    set_daemon_kill_on_exit: (a: unknown) => {
      calls.push({ name: "set_daemon_kill_on_exit", args: a });
      return Promise.resolve();
    },
    load_config: () => Promise.resolve(stored),
    save_config: (a: { value: Record<string, unknown> }) => {
      stored = a.value;
      return Promise.resolve();
    },
  },
}));

vi.mock("../remote-config", () => ({
  readRemoteConfig: () => Promise.resolve({ hosts: [{ label: "甲机", host: "a" }] }),
  hostKey: (h: { label: string; host: string }) => h.label.trim() || h.host,
}));

vi.mock("../error-toast", () => ({ showActionFailureToast: () => {} }));

import { DaemonSection } from "./daemon-section";
import { LOCAL_ORIGIN } from "../daemon-policy";

const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  calls.length = 0;
  stored = {};
  status = { channel: true, pid: 42 };
  statusQueue = [];
});

describe("P2s daemon 开关区", () => {
  it("★ 文案不许承诺 daemon 会常驻——它做不到（153ms 内自行退出）", () => {
    const src = readFileSync(resolve(__dirname, "daemon-section.ts"), "utf8");
    // 只看**用户可见的字符串字面量**：剥掉整行注释（内部词汇与解释性散文不管）。
    const visible = src
      .split("\n")
      .filter((l) => !l.trim().startsWith("//") && !l.trim().startsWith("*"))
      .join("\n");
    const FORBIDDEN = ["后台常驻", "继续运行", "一直跑", "常驻后台", "保持运行"];
    for (const w of FORBIDDEN) {
      expect(
        visible.split(w).length - 1,
        `文案里出现了「${w}」——这是**做不到**的承诺：daemon 是纯 stdio 子进程，` +
          "monitor 一退读端就断，它 153ms 内自己 broken-pipe 退出（P2s §0a 实测）。\n" +
          "要真常驻得先给 daemon 一个监听口（待决 U7）。在那之前，界面不许替它背书。",
      ).toBe(0);
    }
  });

  it("本机永远在第一行——它不是另一种机器，只是不走 ssh 的那一台", async () => {
    const s = new DaemonSection({ headless: true });
    await flush();
    await flush();
    const rows = [...s.element.querySelectorAll<HTMLElement>(".daemon-row")];
    expect(rows.map((r) => r.dataset.origin)).toEqual([LOCAL_ORIGIN, "甲机"]);
  });

  it("每台机各查各的状态", async () => {
    const s = new DaemonSection({ headless: true });
    await flush();
    await flush();
    const asked = calls
      .filter((c) => c.name === "daemon_status")
      .map((c) => (c.args as { origin: string }).origin);
    expect(asked.sort()).toEqual([LOCAL_ORIGIN, "甲机"].sort());
    expect(s.element.querySelector(".daemon-row-state")?.textContent).toContain("已连上");
  });

  it("★ 机器清单问后端要，不自己算（A5：前端自己拼会与 Rust 的 origin 分叉四处）", async () => {
    new DaemonSection({ headless: true });
    await flush();
    await flush();
    expect(
      calls.some((c) => c.name === "daemon_machines"),
      "没调 daemon_machines —— 前端又在自己拼清单了。\n" +
        "Rust 侧会对重复 label 后缀化（pi → pi (#2)）、会按 enabled 过滤、启动后新增的不注册；\n" +
        "自己拼出来的名字对不上注册表，那些行的起/停恒回「没有这台机的把手」。",
    ).toBe(true);
    const { readFileSync } = await import("node:fs");
    const { resolve } = await import("node:path");
    const src = readFileSync(resolve(__dirname, "daemon-section.ts"), "utf8");
    expect(
      src.split("hostKey(").length - 1,
      "daemon-section.ts 里又出现了 hostKey( —— 那正是自己拼 origin 的做法",
    ).toBe(0);
  });

  it("★ 起完之后轮询到落定，不画一张操作前的快照（A4）", async () => {
    const s = new DaemonSection({ headless: true });
    await flush();
    await flush();
    // 起：命令返回时 daemon 还没起来（channel:false），第三次才连上。
    statusQueue = [
      { channel: false, pid: null },
      { channel: false, pid: null },
      { channel: true, pid: 7 },
    ];
    const btns = [...s.element.querySelectorAll<HTMLButtonElement>(".daemon-row button")];
    btns[0].click();
    await new Promise((r) => setTimeout(r, 400));
    expect(
      s.element.querySelector(".daemon-row-state")?.textContent,
      "起完只画了一次就停手 —— 那张是操作前的快照（daemon_start 只是 spawn 了监护线程就返回）",
    ).toContain("已连上");
  });

  it("★ 存不下就把勾回退——屏上写着 A 而实际是 B 比报错更坏", async () => {
    const s = new DaemonSection({ headless: true });
    await flush();
    await flush();
    const mod = await import("../ipc/commands");
    const spy = vi
      .spyOn(mod.commands, "set_daemon_kill_on_exit")
      .mockRejectedValueOnce(new Error("盘满了"));
    const box = s.element.querySelector<HTMLInputElement>(".daemon-row-kill input")!;
    expect(box.checked).toBe(false);
    box.checked = true;
    box.onchange?.(new Event("change"));
    await flush();
    await flush();
    expect(box.checked, "存失败了勾还留在新位置 —— 界面在骗人").toBe(false);
    spy.mockRestore();
  });
});
