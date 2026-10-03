/**
 * 文件窗口（egui）那一半：私有 Xvfb ＋ `workspace_tests.rs` 末尾那一格截图测试，一张图一个进程
 * （winit 一个进程只许建一个事件循环）。测试二进制先 `cargo test --no-run` 编出来，再逐场景按名字拉起。
 */
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import path from "node:path";
import { sleep } from "./cdp.mjs";

/** 截图测试的全名（`cargo test` 的过滤串；改名要同拍改这里）。 */
const TEST = "workspace::tests::screenshot_for_the_shots_tool";

export const FILEWIN_SCENES = [
  { id: "filewin-main", scene: "main", title: "文件窗口 · 单栏 ＋ 预览", desc: "远端 devbox 上的一个项目目录，选中 main.rs、右侧预览（代码高亮）；左栏书签。左栏「家目录」灰着、预览头上是沙箱路径：都是合成后端的痕迹" },
  { id: "filewin-split", scene: "split", title: "文件窗口 · 双栏", desc: "开了双栏：两栏同一个目录" },
  { id: "filewin-empty", scene: "empty", title: "文件窗口 · 空目录", desc: "进到一个空目录" },
  { id: "filewin-missing", scene: "missing", title: "文件窗口 · 目录不存在", desc: "进到一个不存在的目录：列表那里报错" },
  { id: "filewin-search", scene: "search", title: "文件窗口 · 按名字搜", desc: "工具条搜索框里输「retry」：全家目录里名字带它的文件。索引那一行的「扫描间隔 4242 秒」是合成后端故意给的怪数、命中路径是沙箱里的长路径" },
].map((s) => ({ ...s, dir: "文件窗口", width: 1280, height: 800 }));

export async function shootFilewin({ repo, sandbox, out, scenes, env, buildEnv, children }) {
  const results = [];
  const problems = [];
  const crate = path.join(repo, "src/frontend/filewin");
  console.log("文件窗口：编测试二进制（第一次要几分钟）…");
  const build = spawnSync("cargo", ["test", "--lib", "--no-run", "--message-format=json"], {
    cwd: crate,
    // 编译要真 HOME（rustup / cargo 的家），别的照摘
    env: { ...buildEnv, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "3" },
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
    stdio: ["ignore", "pipe", "inherit"],
  });
  const exe = build.stdout
    .split("\n")
    .filter((l) => l.startsWith("{"))
    .map((l) => JSON.parse(l))
    .find((m) => m.reason === "compiler-artifact" && m.profile?.test && m.target?.name === "cc_monitor_filewin" && m.executable)?.executable;
  if (build.status !== 0 || !exe) {
    problems.push(`文件窗口：测试二进制没编出来（cargo 退出码 ${build.status}）`);
    return { results, problems };
  }

  const display = freeDisplay();
  if (display === null) {
    problems.push("文件窗口：找不到空闲的 X 显示号");
    return { results, problems };
  }
  const xvfb = spawn("Xvfb", [`:${display}`, "-screen", "0", "1600x1000x24", "-nolisten", "tcp", "-noreset"], { env, stdio: "ignore" });
  children.push(xvfb);
  for (let i = 0; i < 100 && !existsSync(`/tmp/.X11-unix/X${display}`); i++) await sleep(100);

  for (const s of scenes) {
    const file = path.join(out, s.dir, `${s.id}.png`);
    mkdirSync(path.dirname(file), { recursive: true });
    // 合成的家目录：地址栏末几段读起来像一台真机器（…/home/user/work/orders-service）
    const home = path.join(sandbox, "filewin", s.scene, "home", "user");
    layFixture(home);
    const runEnv = {
      ...env,
      DISPLAY: `:${display}`,
      CCM_SHOTS_FILEWIN_SCENE: s.scene,
      CCM_SHOTS_FILEWIN_OUT: file,
      CCM_SHOTS_FILEWIN_DIR: home,
      CCM_SHOTS_FILEWIN_PREVIEW: CODE,
    };
    delete runEnv.WAYLAND_DISPLAY;
    const r = spawnSync(exe, [TEST, "--exact", "--ignored", "--nocapture", "--test-threads=1"], { env: runEnv, encoding: "utf8", timeout: 120_000 });
    const ok = r.status === 0 && existsSync(file);
    if (!ok) {
      const why = `${r.stderr ?? ""}\n${r.stdout ?? ""}`.split("\n").filter((l) => /panicked|error/i.test(l)).slice(0, 2).join(" | ");
      problems.push(`${s.id}：没截成（退出码 ${r.status}）${why}`);
    }
    results.push({ ...s, file: path.relative(out, file), ok });
    console.log(`${ok ? "✓" : "✗"} ${s.dir}/${s.id}`);
  }
  try {
    process.kill(xvfb.pid, "SIGTERM");
  } catch {
    // 已经退了
  }
  return { results, problems };
}

const CODE = `//! 库存客户端：统一的重试与超时。
use std::time::Duration;

/// 一次调用最多重试几次、首次退避多久；之后每次翻倍。
pub struct Retry {
    pub times: u32,
    pub backoff: Duration,
}

impl Retry {
    pub fn delay(&self, attempt: u32) -> Duration {
        // 第 n 次重试前等 backoff × 2^n
        self.backoff * 2u32.pow(attempt)
    }
}

fn main() {
    let r = Retry { times: 3, backoff: Duration::from_millis(200) };
    for i in 0..r.times {
        println!("第 {} 次重试前等 {:?}", i + 1, r.delay(i));
    }
}
`;

/** 铺一个合成的项目目录（占位文件；修改时间钉死，每次截出来一样）。 */
function layFixture(home) {
  rmSync(home, { recursive: true, force: true });
  const dir = path.join(home, "work", "orders-service");
  const at = (iso) => new Date(iso);
  const dirs = [["src", "2026-10-01T09:12:00"], ["docs", "2026-09-28T17:40:00"], ["tests", "2026-10-01T09:20:00"], [".git", "2026-10-01T09:31:00"], ["node_modules", "2026-09-02T11:05:00"], ["empty-dir", "2026-09-30T08:00:00"]];
  for (const [d] of dirs) mkdirSync(path.join(dir, d), { recursive: true });
  const files = [
    ["README.md", "# orders-service\n\n订单服务。\n", "2026-09-15T10:00:00"],
    [".env", "A=1\n", "2026-08-01T09:00:00"],
    [".gitignore", "target\n", "2026-08-01T09:00:00"],
    ["Cargo.toml", '[package]\nname = "orders"\nversion = "0.1.0"\n', "2026-09-20T14:22:00"],
    ["notes.txt", "todo\n".repeat(200), "2026-09-29T18:03:00"],
    ["report.pdf", Buffer.alloc(182_000), "2025-12-31T23:59:00"],
    ["release.tar.gz", Buffer.alloc(3_400_000), "2026-09-30T20:15:00"],
    ["logo.png", Buffer.alloc(24_000), "2026-07-11T08:30:00"],
    ["main.rs", CODE, "2026-10-01T09:18:00"],
    ["src/retry.rs", "pub fn backoff() {}\n", "2026-10-01T09:12:00"],
    ["src/clients.rs", "pub struct InventoryClient;\n", "2026-10-01T09:11:00"],
    ["tests/retry_test.rs", "#[test]\nfn retries() {}\n", "2026-10-01T09:20:00"],
    ["docs/retry.md", "# 重试\n", "2026-09-28T17:40:00"],
  ];
  for (const [name, body, t] of files) {
    writeFileSync(path.join(dir, name), body);
    utimesSync(path.join(dir, name), at(t), at(t));
  }
  // 目录的修改时间最后钉（往里写文件会把它刷成现在）
  for (const [d, t] of dirs) utimesSync(path.join(dir, d), at(t), at(t));
}

/** 挑一个没人用的显示号（没有套接字、也没有锁文件）。 */
function freeDisplay() {
  for (let n = 150; n < 200; n++) {
    if (!existsSync(`/tmp/.X11-unix/X${n}`) && !existsSync(`/tmp/.X${n}-lock`)) return n;
  }
  return null;
}
