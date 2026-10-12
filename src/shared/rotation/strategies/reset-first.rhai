/*--- rotation
{
  "format": 5,
  "name": "先用快重置的",
  "line": "先用快要重置的号，赶在它重置前把剩下的额度用掉",
  "settings": {
    "caps": {},
    "capKind": "soft",
    "accounts": [
      {"start": true}
    ]
  }
}
---*/

// 5 小时窗口离重置越近，分越高；没开窗的、按量号不给分（按 0 分、照列表）。
// 次序只在某个号重置时变，后端本来就在重置时重判，所以不交 next_change_at。
fn score(ctx) {
  let out = #{};
  for a in ctx.accounts {
    let r = a.resets.w5h;
    if a.kind == "api" || r == () { continue; }
    let mins = (r - ctx.now) / 60;
    out[a.id] = #{ score: 100000 - mins, why: "离重置还有 " + mins + " 分钟" };
  }
  #{ scores: out }
}
