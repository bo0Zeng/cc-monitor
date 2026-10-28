// 用户态延迟限速代理：每个方向每块数据晚 oneway 毫秒交出（保序），可限速（kbps，0 = 不限）。只延迟与限速，不丢包。
// node delay.mjs <听口> <目标口> <oneway_ms> [kbps]   起好印 `READY <听口>`。
import net from "node:net";

const [listen, target, oneway, kbps = 0] = process.argv.slice(2).map(Number);

function pipe(from, to) {
  let busyUntil = 0;
  from.on("data", (d) => {
    const now = Date.now();
    busyUntil = Math.max(busyUntil, now) + (kbps > 0 ? (d.length * 8) / kbps : 0);
    setTimeout(() => to.write(d), Math.max(0, busyUntil + oneway - now));
  });
  from.on("end", () => setTimeout(() => to.end(), oneway + 5));
  from.on("error", () => to.destroy());
}

net
  .createServer((c) => {
    c.setNoDelay(true);
    const u = net.connect(target, "127.0.0.1");
    u.setNoDelay(true);
    pipe(c, u);
    pipe(u, c);
  })
  .listen(listen, "127.0.0.1", () => console.log("READY", listen));
