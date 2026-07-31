/**
 * S9：本机 OS 判定。
 *
 * 最要紧的两条不是「能不能认出 Windows」，而是：
 * ① **判定顺序** —— `Linux` 这个词会出现在别的平台的 UA 里，把它放最后才对；
 * ② **测不出时往显示的方向倒** —— 藏错了 Windows 用户就找不到安装入口。
 */
import { describe, it, expect, afterEach } from "vitest";
import {
  detectHostOs,
  hostOs,
  hostOsAllows,
  __setHostOsForTests,
} from "./host-os";

afterEach(() => __setHostOsForTests(null));

// 三个平台上 Tauri webview 的真实 UA 形态（WebView2 / WKWebView / WebKitGTK）。
const UA = {
  windows:
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0",
  macos:
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15",
  linux:
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15",
};

describe("detectHostOs", () => {
  it("认得三个平台的真实 UA", () => {
    expect(detectHostOs(UA.windows)).toBe("windows");
    expect(detectHostOs(UA.macos)).toBe("macos");
    expect(detectHostOs(UA.linux)).toBe("linux");
  });

  // **这里刻意没有「判定顺序」测试，理由如实记一笔**：
  //
  // 我原本打算钉「Linux 那条必须排最后」，写之前先算了一遍变异 —— 把顺序反过来，
  // 三条真实 UA 的结果**一个都不变**（Windows 串不含 `Linux`/`X11`，macOS 串也不含）。
  // 也就是说「顺序反过来」在今天是个**等价变异**，为它写的测试必然是安慰剂。
  //
  // ⇒ 顺序照旧从窄到宽排（将来加平台时不被最宽那条吞掉），但**不假装有测试在守它**。
  // 真被顺序影响的只有 `Linux; Android` 这类串，而桌面 Tauri 应用遇不上。

  it("认不出的串 → unknown（空串也是）", () => {
    expect(detectHostOs("")).toBe("unknown");
    expect(detectHostOs("SomeEmbeddedThing/1.0")).toBe("unknown");
  });
});

describe("hostOsAllows —— 失败方向", () => {
  it("不填 allowed = 与 OS 无关，一律显示", () => {
    __setHostOsForTests("linux");
    expect(hostOsAllows(undefined)).toBe(true);
  });

  it("★ unknown 时**照常显示** —— 藏错了 Windows 用户就找不到入口", () => {
    __setHostOsForTests("unknown");
    expect(hostOsAllows(["windows"])).toBe(true);
  });

  it("在名单里 → 显示；不在名单里 → 藏", () => {
    __setHostOsForTests("windows");
    expect(hostOsAllows(["windows"])).toBe(true);
    __setHostOsForTests("linux");
    expect(hostOsAllows(["windows"])).toBe(false);
    __setHostOsForTests("macos");
    expect(hostOsAllows(["windows"])).toBe(false);
  });
});

describe("hostOs 覆盖值", () => {
  it("置了就用置的；清掉后回到真实探测（jsdom 下是 linux）", () => {
    __setHostOsForTests("windows");
    expect(hostOs()).toBe("windows");
    __setHostOsForTests(null);
    // jsdom 的 UA 含 `linux` —— 这条同时说明了为什么面板类测试必须显式置成 windows。
    expect(hostOs()).toBe(detectHostOs(navigator.userAgent));
  });
});
