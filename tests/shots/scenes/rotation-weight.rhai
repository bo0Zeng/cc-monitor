// 按权重排序，权重相差在权重容差以内算同权，同权按默认优先级（号牌顺序）。
// 权重 ＝ 余量 ÷（距 5 小时重置的分钟数 ＋ 15），在用账号的余量按 100% 计，未开窗为 0。
// 在用账号未达用量上限时，权重超出它强制切换阈值的账号立即切入。
// 耗尽后启用的账号：其余账号均无可用额度、满启用延迟才启用，每次用到单次用量上限即停用。

/*--- rotation
{
  "format": 5,
  "tz": "UTC",
  "settings": {
    "accounts": [
      { "id": "work" },
      { "id": "team", "caps": { "5h": [{ "from": 0, "to": 1440, "v": 99 }] } },
      { "id": "lab",
        "caps": { "5h": [{ "from": 0, "to": 120, "v": 80 }, { "from": 120, "to": 1020, "v": 99 }, { "from": 1020, "to": 1440, "v": 80 }] } },
      { "id": "personal" },
      { "id": "api", "fallback": true, "delay": 30, "stint": 20 }
    ],
    "caps": { "5h": [{ "from": 0, "to": 1440, "v": 90 }], "7d": [{ "from": 0, "to": 1440, "v": 90 }] },
    "close": 25,
    "force": 25
  },
  "form": [
    { "key": "caps", "kind": "daySegments", "scope": "each", "inherit": true, "windows": ["5h", "7d"],
      "min": 0, "max": 100, "unit": "percent", "default": 90,
      "label": { "zh": "用量上限" }, "help": { "zh": "账号用量的上限，可按时段分段；0% 表示该时段停用" } },
    { "key": "close", "kind": "percent", "scope": "all", "min": 0, "max": 100, "default": 25,
      "label": { "zh": "权重容差" },
      "help": { "zh": "权重 ＝ 余量 ÷（距 5 小时重置的分钟数 ＋ 15）；在用账号的余量按 100% 计；未开窗为 0。较高权重超出较低权重不到这一比例时，两账号视为同权，按默认优先级排序" } },
    { "key": "force", "kind": "percent", "scope": "all", "min": 0, "max": 100, "default": 25, "nullable": { "zh": "不强制切换" },
      "label": { "zh": "强制切换阈值" },
      "help": { "zh": "权重超出在用账号这一比例的账号立即切入" } },
    { "key": "fallback", "kind": "toggle", "scope": "each", "default": false,
      "label": { "zh": "耗尽后启用" }, "help": { "zh": "其余账号均无可用额度时才启用；其余账号恢复额度即切出" } },
    { "key": "delay", "kind": "minutes", "scope": "each", "min": 0, "max": 1440, "default": 0,
      "label": { "zh": "启用延迟" }, "help": { "zh": "其余账号均无可用额度持续这一时长后才启用；仅对耗尽后启用的账号生效" } },
    { "key": "stint", "kind": "percent", "scope": "each", "min": 0, "max": 100, "nullable": { "zh": "不限" },
      "label": { "zh": "单次用量上限" },
      "help": { "zh": "每次启用后，5 小时用量增幅达到这一比例即停用，直到其余账号恢复额度；仅对耗尽后启用的账号生效" } }
  ]
}
---*/

// 一组分段（分钟 0–1440）在某个钟点的上限；落空 ＝ 100。
fn cap_at(segs, clock) {
  for g in segs {
    if clock >= g.from && clock < g.to { return g.v.to_float(); }
  }
  100.0
}

// 这个号这个窗口用哪组分段：号自己设了用自己的，否则用总体的。
fn segs_for(s, row, w) {
  if row != () && row.caps != () && row.caps[w] != () { return row.caps[w]; }
  if s.caps != () && s.caps[w] != () { return s.caps[w]; }
  []
}

// 从此刻起到下一个上限会变的钟点还有几分钟；没有 ⇒ ()。
fn next_edge(segs, clock) {
  if segs.len() < 2 { return (); }
  let here = cap_at(segs, clock);
  let best = ();
  for g in segs {
    for edge in [g.from, g.to] {
      let e = edge % 1440;
      let d = (e - clock + 1440) % 1440;
      if d == 0 { continue; }
      if cap_at(segs, e) == here { continue; }
      if best == () || d < best { best = d; }
    }
  }
  best
}

// 权重：余量（0–100）÷（离 5 小时重置的分钟数 ＋ pad()）。
//   离重置越近越高；剩得越多越高，剩得少就低。
//   加 pad()（15 分钟）：离重置只剩几分钟时，分母不会趋近 0，只剩一点余量的号也就抬不上来。
//   在用的号不用换过去，余量按 100 算：它的权重只随时间变，不随自己的用量掉，
//   所以重置得比它晚的号永远超不过它，用着用着也不会被换走又换回来。
fn pad() { 15.0 }
fn weight(left, mins) { if left <= 0.0 { 0.0 } else { (left / 100.0) / (mins + pad()) } }

fn decide(ctx) {
  let s = ctx.settings;
  let now = ctx.now;
  let clock = ctx.clock;
  let cur = ctx.current;

  // 事实按号查。权重就是优先级；号牌的先后是默认优先级（默认权重）：权重同权时照它，总能分出先后
  let fact = #{};
  for a in ctx.accounts { fact[a.id] = a; }
  let rows = #{};
  let ids = [];
  for r in s.accounts {
    if fact[r.id] == () { continue; }
    ids.push(r.id);
    rows[r.id] = r;
  }
  let is_fb = |id| rows[id] != () && rows[id].fallback == true;

  // 每个号：余量（两窗「此刻上限 − 用量」取小）、有没有额度、离 5 小时重置几分钟
  let left = #{};
  let has = #{};
  let mins = #{};
  for id in ids {
    let a = fact[id];
    let r = rows[id];
    let u5 = if a.used.w5h != () { a.used.w5h } else { 0.0 };
    let u7 = if a.used.w7d != () { a.used.w7d } else { 0.0 };
    let l = cap_at(segs_for(s, r, "5h"), clock) - u5;
    let l7 = cap_at(segs_for(s, r, "7d"), clock) - u7;
    if l7 < l { l = l7; }
    left[id] = l;
    has[id] = a.sendable && l > 0.0;
    mins[id] = if a.resets.w5h != () { (a.resets.w5h - now) / 60 } else { () };
  }

  // 只在别的号都没额度才用的号：别的号全都没额度满它的启用延迟，才算有额度
  let memo = if ctx.memo != () { ctx.memo } else { #{} };
  let others_have = false;
  for id in ids { if !is_fb.call(id) && has[id] { others_have = true; } }
  if others_have {
    memo.allOutSince = ();
    memo.stint = ();
  } else if memo.allOutSince == () {
    memo.allOutSince = now;
  }
  let wait_until = ();
  for id in ids {
    if !is_fb.call(id) || !has[id] || others_have { continue; }
    let d = if rows[id].delay != () { rows[id].delay } else { 0 };
    let t = memo.allOutSince + d * 60;
    if now < t {
      has[id] = false;
      if wait_until == () || t < wait_until { wait_until = t; }
    }
  }

  // 只在别的号都没额度才用的号，每次换过来最多用多少：从换过来那一刻起，记它的 5 小时用量涨了多少。
  //   读数是这个号的，别的会话用的也算在里面。这一段里窗口重置了 ⇒ 新窗口从 0 接着记。
  //   涨够了 ⇒ 这一段里不再用它，等别的号恢复（别的号一有额度，这本账就清掉）。
  let stint = if memo.stint != () { memo.stint } else { #{} };
  let spent = #{};
  for id in ids {
    if !is_fb.call(id) || rows[id].stint == () { continue; }
    let e = stint[id];
    if e == () { continue; }
    let a = fact[id];
    let u = if a.used.w5h != () { a.used.w5h } else { 0.0 };
    if a.resets.w5h == e.reset { if u > e.last { e.rose += u - e.last; } } else { e.rose += u; }
    e.last = u;
    e.reset = a.resets.w5h;
    stint[id] = e;
    if e.rose >= rows[id].stint.to_float() {
      has[id] = false;
      spent[id] = true;
    }
  }

  // 权重（见文件头 weight）；窗口还没开 ⇒ 0：还没开始计时，剩的不会作废，不急。
  let pri = #{};   // 权重
  for id in ids {
    if !has[id] { continue; }
    if mins[id] == () { pri[id] = 0.0; continue; }
    pri[id] = weight(left[id], mins[id].to_float());
  }
  // 排先后：每次在剩下的号里找权重最高的，比它低不到「相差不到多少」那么多的都算一样，取号牌最靠前的
  let close = if s.close != () { s.close.to_float() } else { 0.0 };   // 较高的超出较低的不到这一比例 ⇒ 同权
  let sorted = |list| {
    let rest = list;
    let out = [];
    while rest.len() > 0 {
      let best = ();
      for id in rest { if best == () || pri[id] > best { best = pri[id]; } }
      let floor = best / (1.0 + close / 100.0);
      for id in ids {
        if rest.contains(id) && pri[id] >= floor {
          out.push(id);
          rest.remove(rest.index_of(id));
          break;
        }
      }
    }
    out
  };

  // 用满换号时挑谁：照上面排先后；「别的号都没额度才用」的号排最后
  let main = [];
  let fb = [];
  for id in ids {
    if pri[id] == () { continue; }
    if is_fb.call(id) { fb.push(id); } else { main.push(id); }
  }
  let next = sorted.call(main);
  for id in sorted.call(fb) { next.push(id); }

  let why = #{};
  for id in next { why[id] = if is_fb.call(id) { "其余账号无可用额度" } else { "权重最高" }; }

  // 换不换：
  // · 在用的号没额度，或者在用的是「别的号都没额度才用」的号 ⇒ 照上面那一串；
  // · 否则设了「强制切换阈值」时，权重超出在用的号这一比例的号直接换过去（在用的号余量按 100 算）；
  //   其余照旧等在用的号用满。
  //   在用的号还没读到重置时刻 ⇒ 不比。
  let k_force = if s.force != () { 1.0 + s.force.to_float() / 100.0 } else { () };   // 强制切换阈值
  let beats = |id| {
    if k_force == () || is_fb.call(id) || mins[cur] == () || mins[id] == () { return false; }
    pri[id] > weight(100.0, mins[cur].to_float()) * k_force
  };
  let order = [];
  // 在用的号不在这份策略的号列表里（会话是用别的号开的）⇒ 当它没额度，照那一串换
  if has[cur] != true || is_fb.call(cur) {
    order = next;
  } else {
    let take = ();
    for id in next { if id != cur && beats.call(id) { take = id; break; } }
    if take != () {
      why[take] = "强制切换：重置更早，权重超出阈值";
      order.push(take);
    } else {
      why[cur] = "未达用量上限，继续使用";
      order.push(cur);
    }
    for id in next { if !order.contains(id) { order.push(id); } }
  }

  // 要用的是只在别的号都没额度才用的号、它的账还没开 ⇒ 从此刻的读数开始记
  let first = if order.len() > 0 { order[0] } else { () };
  if first != () && is_fb.call(first) && rows[first].stint != () && stint[first] == () {
    let a = fact[first];
    stint[first] = #{ rose: 0.0, last: if a.used.w5h != () { a.used.w5h } else { 0.0 }, reset: a.resets.w5h };
  }
  memo.stint = if stint.len() > 0 { stint } else { () };

  // 一个都不能用：停发，停到「最早有号恢复」和「启用延迟到点」里早的那个
  let hold = ();
  if order.len() == 0 {
    for id in ids {
      if spent[id] == true { continue; }   // 这一段里用够了：它重置也不算恢复
      let a = fact[id];
      let r = rows[id];
      let t = ();
      if a.refusedUntil != () && a.refusedUntil > now { t = a.refusedUntil; }
      let u5 = if a.used.w5h != () { a.used.w5h } else { 0.0 };
      let u7 = if a.used.w7d != () { a.used.w7d } else { 0.0 };
      if u5 >= cap_at(segs_for(s, r, "5h"), clock) && a.resets.w5h != () && (t == () || a.resets.w5h > t) { t = a.resets.w5h; }
      if u7 >= cap_at(segs_for(s, r, "7d"), clock) && a.resets.w7d != () && (t == () || a.resets.w7d > t) { t = a.resets.w7d; }
      if t != () && (hold == () || t < hold) { hold = t; }
    }
    if wait_until != () && (hold == () || wait_until < hold) { hold = wait_until; }
  }

  // 约下次再来问。照接下来要用的那个号约（这一判换了号 ⇒ 是换过去的那个，不是刚才在用的）
  let nxt = if order.len() > 0 { order[0] } else { () };
  let when = [];
  let at = ();
  let soonest = |t| { if t != () && t > now && (at == () || t < at) { at = t; } };
  for id in ids {
    let r = rows[id];
    if id == nxt {
      for w in ["5h", "7d"] { when.push(#{ acct: id, w: w, used_at_least: cap_at(segs_for(s, r, w), clock) }); }
    }
    for w in ["5h", "7d"] {
      let e = next_edge(segs_for(s, r, w), clock);
      if e != () { soonest.call(now + e * 60); }
    }
  }
  soonest.call(wait_until);
  // 在用只在别的号都没额度才用的号、记着账：涨够的那一读数来问；它的窗口要重置 ⇒ 重置前一分钟来记一笔，免得那一截没记上
  if nxt != () && stint[nxt] != () && spent[nxt] != true {
    let e = stint[nxt];
    when.push(#{ acct: nxt, w: "5h", used_at_least: e.last + rows[nxt].stint.to_float() - e.rose });
    if fact[nxt].resets.w5h != () { soonest.call(fact[nxt].resets.w5h - 60); }
  }
  // 有额度的号：它的权重随时间涨（离重置越来越近）；要用的号的权重按余量 100 算，也只随时间涨。
  // 解出它超过那条线的那一刻来问；它的余量只会被别的会话用掉、变小，那一刻只会往后推，先来问不会漏。
  // · 按此刻两边的余量，算它高过要用的号的那一刻来问（两边离重置的分钟数都照走）；
  // · 要用的号用到会被它高过的那一读数（时间先不走）之前，每走一半再来问；最多每多用 1% 问一次。
  // 窗口没开的号不用看：它一开窗就是 5 小时后才重置，不会比要用的号先。
  if nxt != () && !is_fb.call(nxt) && k_force != () && (mins[nxt] == () || pri[nxt] == ()) {
    // 要用的号窗口还没开：一用就开，开了再算
    let u = if fact[nxt].used.w5h != () { fact[nxt].used.w5h } else { 0.0 };
    when.push(#{ acct: nxt, w: "5h", used_at_least: u + 0.01 });
  } else if nxt != () && !is_fb.call(nxt) && k_force != () {
    let tc = mins[nxt].to_float();
    for id in ids {
      if id == nxt || is_fb.call(id) || pri[id] == () || mins[id] == () { continue; }
      let sx = left[id] / 100.0;
      let ti = mins[id].to_float();
      // sx/(ti ＋ pad − x) > k/(tc ＋ pad − x)  ⇔  x > (k·(ti ＋ pad) − sx·(tc ＋ pad))/(k − sx)，x 是从此刻起的分钟数
      let x = (k_force * (ti + pad()) - sx * (tc + pad())) / (k_force - sx);
      if x > 0.0 && x < ti { soonest.call(now + (x * 60.0).to_int() + 1); }
    }
  }

  #{ order: order, why: why, hold_until: hold, wake: #{ at: at, when: when }, memo: memo }
}
