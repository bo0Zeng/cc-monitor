/**
 * 截图台架换了 HOME 照样找得到工具链与浏览器（`host-tools.mjs`）。
 *
 * 起因：照规矩换成沙箱 HOME 跑 `npm run shots`，rustup 的家跟着 HOME 走进空目录 ⇒ 编后端那一步「no default is configured」；
 * 浏览器只在 `$HOME/.cache/ms-playwright` 里找 ⇒「找不到 Chromium」。
 *
 * 人群：`toolchainEnv` · `findChrome` 两个口（`run.mjs` 编译那份环境、起浏览器都经它们）。
 * 买不到：真 rustup / 真 Chromium 起不起得来不在这里试（造目录与空的可执行文件）。
 */
import { chmodSync, mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

// 台架那一侧是不带类型的 .mjs：按路径取。
const MOD = "./host-tools.mjs";
const { toolchainEnv, findChrome } = (await import(MOD)) as {
  toolchainEnv: (env: Record<string, string | undefined>, home: string) => Record<string, string>;
  findChrome: (env: Record<string, string | undefined>, home: string) => string;
};

let root = "";
let real = "";
let sandboxHome = "";

function chromeIn(cacheDir: string, build: string, mtimeSec: number): string {
  const dir = path.join(cacheDir, build, "chrome-linux64");
  mkdirSync(dir, { recursive: true });
  const exe = path.join(dir, "chrome");
  writeFileSync(exe, "");
  chmodSync(exe, 0o755);
  utimesSync(exe, mtimeSec, mtimeSec);
  return exe;
}

beforeEach(() => {
  root = mkdtempSync(path.join(os.tmpdir(), "shots-host-"));
  real = path.join(root, "real");
  sandboxHome = path.join(root, "sandbox");
  mkdirSync(real, { recursive: true });
  mkdirSync(sandboxHome, { recursive: true });
});
afterEach(() => rmSync(root, { recursive: true, force: true }));

describe("工具链的家", () => {
  it("HOME 是沙箱、外面没给 ⇒ 落在登录用户的家目录", () => {
    const got = toolchainEnv({ HOME: sandboxHome }, real);
    expect(got).toEqual({ CARGO_HOME: path.join(real, ".cargo"), RUSTUP_HOME: path.join(real, ".rustup") });
  });
  it("外面给了 ⇒ 照用", () => {
    const got = toolchainEnv({ HOME: sandboxHome, CARGO_HOME: "/opt/c", RUSTUP_HOME: "/opt/r" }, real);
    expect(got).toEqual({ CARGO_HOME: "/opt/c", RUSTUP_HOME: "/opt/r" });
  });
});

describe("浏览器", () => {
  it("HOME 是沙箱 ⇒ 在登录用户家目录的 Playwright 缓存里找，取最新那一份", () => {
    const cache = path.join(real, ".cache/ms-playwright");
    chromeIn(cache, "chromium-1100", 1_000_000);
    const newer = chromeIn(cache, "chromium-1200", 2_000_000);
    chromeIn(cache, "chromium_headless_shell-1300", 3_000_000);
    expect(findChrome({ HOME: sandboxHome }, real)).toBe(newer);
  });
  it("给了 PLAYWRIGHT_BROWSERS_PATH ⇒ 在那里找", () => {
    const exe = chromeIn(path.join(root, "pw"), "chromium-1243", 1_000_000);
    expect(findChrome({ HOME: sandboxHome, PLAYWRIGHT_BROWSERS_PATH: path.join(root, "pw") }, real)).toBe(exe);
  });
  it("给了 CCM_SHOTS_CHROME ⇒ 照用", () => {
    chromeIn(path.join(real, ".cache/ms-playwright"), "chromium-1200", 1_000_000);
    expect(findChrome({ HOME: sandboxHome, CCM_SHOTS_CHROME: "/opt/chrome" }, real)).toBe("/opt/chrome");
  });
  it("哪里都没有 ⇒ 抛，说清找过哪几处", () => {
    expect(() => findChrome({ HOME: sandboxHome }, real)).toThrow(path.join(real, ".cache/ms-playwright"));
  });
});
