/**
 * 截图工具起的无头浏览器不碰用户的桌面会话：不问钥匙环、子进程环境里不带会话总线。
 *
 * 为什么有这一格：Chrome 在 Linux 上起网络进程时要向桌面钥匙环（D-Bus 上的 secret service）取 cookie 加密钥匙；
 * 钥匙环的登录集合锁着时它会去请解锁（桌面上弹框、没人点），网络进程就一直等 —— 每个 http 页面都挂着、load 永远不来，
 * 截图工具整趟超时（10-08 晚整机各路截图同时卡住就是这一形；同一页加 `--password-store=basic` 或去掉会话总线就秒开）。
 * 截图本来就该在沙箱里：钥匙用浏览器自己的明文存储（`--password-store=basic`），环境里不带 `DBUS_SESSION_BUS_ADDRESS`。
 *
 * 人群：`tests/shots/` 下每一份起无头浏览器的脚本（认 `--headless=new` 那一行）—— 新加一份台架也自动进来。
 * 买不到：读的是源码文本（脚本有顶层副作用，不能 import 起来真跑），参数 / 环境被拼得更绕时认不出。
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { describe, it, expect } from "vitest";

const root = path.resolve(__dirname);

function scripts(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const p = path.join(dir, name);
    if (statSync(p).isDirectory()) out.push(...scripts(p));
    else if (name.endsWith(".mjs")) out.push(p);
  }
  return out;
}

const launchers = scripts(root).filter((p) => readFileSync(p, "utf8").includes('"--headless=new"'));

describe("截图工具的无头浏览器不碰桌面会话", () => {
  it("人群不空（至少截图工具那一份）", () => {
    expect(launchers.map((p) => path.relative(root, p))).toContain("run.mjs");
  });

  for (const file of launchers) {
    const rel = path.relative(root, file);
    const src = readFileSync(file, "utf8");
    it(`${rel}：起浏览器的参数里有 --password-store=basic`, () => {
      // 与 --headless=new 同一张参数表（同一个数组字面量）
      const arr = src.slice(src.lastIndexOf("[", src.indexOf('"--headless=new"')), src.indexOf("]", src.indexOf('"--headless=new"')));
      expect(arr, "不给 ⇒ 网络进程去问桌面钥匙环，钥匙环锁着时每个页面都挂着").toContain('"--password-store=basic"');
    });
    it(`${rel}：子进程环境里摘掉 DBUS_SESSION_BUS_ADDRESS`, () => {
      expect(
        /\[[^\]]*"DBUS_SESSION_BUS_ADDRESS"[^\]]*\]\)\s*delete env\[k\]/.test(src),
        "环境里带着会话总线 ⇒ 测试碰得到用户的桌面会话（钥匙环 · 通知 · 屏保）",
      ).toBe(true);
    });
  }
});
