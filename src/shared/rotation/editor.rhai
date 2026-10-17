// 每号的用量上限、切入、切出照设置；用满或被拒才换号，几个号都能用时照号牌顺序。
// 切入：默认 ＝ 轮到它才用；强制切入 ＝ 有额度就切到它，几个强制切入的照号牌顺序、靠前的有额度就切回；延迟切入 ＝ 其余账号都用完才切到它，延迟时长内有账号恢复就先停发、到点用那个账号。
// 切出：默认 ＝ 到上限或被拒才换走；强制切出 ＝ 用着它时，其余账号余量合计达到切出条件（或一有额度）即换走，换走后排在其余账号后面。
// 智能切换：提前换号的那份算法还没接进来，开着也照号牌顺序。

/*--- rotation
{
  "format": 5,
  "settings": { "accounts": [], "smart": true },
  "form": [
    { "key": "caps", "kind": "daySegments", "scope": "each", "windows": ["5h", "7d"], "follow": true,
      "min": 0, "max": 100, "unit": "percent", "default": 90,
      "label": { "zh": "用量上限" }, "help": { "zh": "账号用量的上限，可按时段分段；0% 表示该时段停用；7 天未另设时同 5 小时" } },
    { "key": "in", "kind": "choice", "scope": "each", "default": "default",
      "choices": [
        { "v": "default", "label": { "zh": "默认" } },
        { "v": "force", "label": { "zh": "强制切入" }, "mark": { "glyph": "↺" } },
        { "v": "delay", "label": { "zh": "延迟切入" }, "mark": { "dashed": true } } ],
      "label": { "zh": "切入" },
      "help": { "zh": "默认：轮到它才用。强制切入：有额度就切到它；几个强制切入的账号都有额度时照号牌顺序，靠前的有额度就切回。延迟切入：其余账号都用完以后才切到它" } },
    { "key": "wait", "kind": "minutes", "scope": "each", "min": 0, "max": 300, "default": 0, "showIf": { "key": "in", "v": "delay" },
      "label": { "zh": "延迟" },
      "help": { "zh": "其余账号都用完时，这一时长内有账号恢复额度就先停发、到点用那个账号；没有就马上切入。0 表示用完就切" } },
    { "key": "out", "kind": "choice", "scope": "each", "default": "default",
      "choices": [
        { "v": "default", "label": { "zh": "默认" } },
        { "v": "force", "label": { "zh": "强制切出" }, "mark": { "glyph": "⇥" } } ],
      "label": { "zh": "切出" },
      "help": { "zh": "默认：用着它时，到上限或被拒才换走。强制切出：用着它时，满足切出条件即换走" } },
    { "key": "outTotal", "kind": "percent", "scope": "each", "min": 0, "max": 1000, "default": 400, "showIf": { "key": "out", "v": "force" },
      "nullable": { "zh": "一有额度" }, "lead": { "zh": "其余号余量合计达到" },
      "label": { "zh": "切出条件" },
      "help": { "zh": "其余账号余量之和，单位为百分点；余量 ＝ 上限 − 用量，两窗取小，此刻不可用的账号按 0 计" } },
    { "key": "smart", "kind": "toggle", "scope": "all", "default": true,
      "label": { "zh": "智能切换" }, "help": { "zh": "按用速算会作废的，值得才换" } }
  ]
}
---*/

// 一组分段在某个钟点的上限；落空 ⇒ 90（声明里的默认）。
fn cap_in(segs, clock) {
  for g in segs { if clock >= g.from && clock < g.to { return g.v.to_float(); } }
  90.0
}

// 一个号某一窗此刻的上限：那一窗没设 ⇒ 同 5 小时；都没设 ⇒ 90。
fn cap_of(r, w, clock) {
  let c = r.caps;
  if c != () && c[w] != () { return cap_in(c[w], clock); }
  if c != () && c["5h"] != () { return cap_in(c["5h"], clock); }
  90.0
}

// 一窗此刻的用量：读数缺 ⇒ 0；过了重置时刻 ⇒ 0。
fn used_of(a, w, now) {
  let u = a.used[w];
  let r = a.resets[w];
  if u == () || (r != () && r <= now) { 0.0 } else { u.to_float() }
}

// 用不了的号几点恢复（被拒到期 · 到上限那几窗的重置，取晚的）；说不出 ⇒ ()。
fn back_at(a, r, now, clock) {
  let t = now;
  if a.refusedUntil != () && a.refusedUntil > now { t = a.refusedUntil; }
  for w in [["5h", "w5h"], ["7d", "w7d"]] {
    let c = cap_of(r, w[0], clock);
    if used_of(a, w[1], now) < c { continue; }
    let at = a.resets[w[1]];
    if c <= 0.0 || at == () { return (); }
    if at > t { t = at; }
  }
  if t > now { t } else { () }
}

fn decide(ctx) {
  let s = ctx.settings;
  let now = ctx.now;
  let clock = ctx.clock;
  let cur = ctx.current;
  let memo = if ctx.memo != () { ctx.memo } else { #{} };

  // 号列表（号牌顺序）里此刻有的号：have ＝ 号 ⇒ 事实，rows ＝ 号 ⇒ 它那一行。
  let have = #{};
  for a in ctx.accounts { have[a.id] = a; }
  let ids = [];
  let rows = #{};
  for r in s.accounts {
    let id = if r.start == true { ctx.start } else { r.id };
    if id == () || have[id] == () || rows[id] != () { continue; }
    ids.push(id);
    rows[id] = r;
  }

  // 余量（两窗「上限 − 用量」取小）· 有没有额度 · 切入。
  let amt = #{}; let ok = #{}; let tier = #{};
  for id in ids {
    let a = have[id];
    let r = rows[id];
    let l5 = cap_of(r, "5h", clock) - used_of(a, "w5h", now);
    let l7 = cap_of(r, "7d", clock) - used_of(a, "w7d", now);
    amt[id] = if l5 < l7 { l5 } else { l7 };
    ok[id] = a.sendable == true && amt[id] > 0.0;
    tier[id] = if r["in"] != () { r["in"] } else { "default" };
  }

  // 强制切出：其余（不是延迟切入的）号余量合计达到切出条件；没设 ⇒ 400（声明里的默认），空 ⇒ 一有额度。
  let out_now = |id| {
    let r = rows[id];
    if r["out"] != "force" { return false; }
    let total = 0.0;
    let any = false;
    for x in ids { if x != id && ok[x] && tier[x] != "delay" { total += amt[x]; any = true; } }
    if !("outTotal" in r) { return total >= 400.0; }
    if r.outTotal == () { any } else { total >= r.outTotal.to_float() }
  };

  // 能用的号照号牌：强制切入的在前，默认的其次，此刻该切出的排到它们后面；延迟切入的另放。
  let force = []; let norm = []; let outs = []; let late = [];
  for id in ids {
    if !ok[id] { continue; }
    if tier[id] == "delay" { late.push(id); }
    else if out_now.call(id) { outs.push(id); }
    else if tier[id] == "force" { force.push(id); }
    else { norm.push(id); }
  }
  let front = force + norm + outs;

  // 在用的号留不留。
  let why = #{};
  let keep = false;
  if rows[cur] != () && ok[cur] && !out_now.call(cur) {
    if tier[cur] == "delay" {
      if front.len() == 0 { keep = true; why[cur] = "其余账号无可用额度"; }
      else if memo.late == cur { keep = true; why[cur] = if rows[cur]["out"] == "force" { "未达切出条件，继续使用" } else { "切出为默认：到上限才换" }; }
    } else if force.len() == 0 || force[0] == cur {
      keep = true;
      why[cur] = "未达用量上限，继续使用";
    }
  }

  let order = [];
  if keep { order.push(cur); }
  for id in front { if !order.contains(id) { order.push(id); } }
  let in_late = keep && tier[cur] == "delay";
  let wait_at = ();
  if front.len() == 0 && late.len() > 0 && !in_late {
    // 其余账号都用完：延迟时长内有号恢复 ⇒ 停到那一刻。
    let w = rows[late[0]].wait;
    if w == () { w = 0; }
    for id in ids {
      if w <= 0 || tier[id] == "delay" { continue; }
      let t = back_at(have[id], rows[id], now, clock);
      if t != () && t - now <= w * 60 && (wait_at == () || t < wait_at) { wait_at = t; }
    }
    if wait_at == () { order += late; }
  }
  if in_late { for id in late { if !order.contains(id) { order.push(id); } } }
  for id in ids {
    if why[id] != () { continue; }
    if order.contains(id) {
      why[id] = if tier[id] == "delay" { "其余账号无可用额度" } else if tier[id] == "force" { "强制切入" } else { "号牌顺序" };
    } else if ok[id] {
      why[id] = if tier[id] == "delay" { "其余账号用完才切入" } else { "满足切出条件" };
    } else if have[id].sendable == true {
      why[id] = "到用量上限";
    }
  }

  // 记下延迟切入的号是不是其余都用完才切进来的（会话从它开、其余有额度 ⇒ 换走）。
  let first = if order.len() > 0 { order[0] } else { () };
  memo.late = if first != () && tier[first] == "delay" { first } else { () };

  // 一个都不能用：停到延迟里等的那一刻，或最早有号恢复。
  let hold = ();
  if order.len() == 0 {
    hold = wait_at;
    if hold == () {
      for id in ids {
        let t = back_at(have[id], rows[id], now, clock);
        if t != () && (hold == () || t < hold) { hold = t; }
      }
    }
  }

  // 几时再来问：要用的号到上限的读数 · 分时段的边界 · 延迟切入的号在用时其余号被拒到期。
  let when = [];
  let at = ();
  for id in ids {
    let r = rows[id];
    if id == first {
      when.push(#{ acct: id, w: "5h", used_at_least: cap_of(r, "5h", clock) });
      when.push(#{ acct: id, w: "7d", used_at_least: cap_of(r, "7d", clock) });
    }
    let t = ();
    if first != () && tier[first] == "delay" && tier[id] != "delay" { t = have[id].refusedUntil; }
    if r.caps != () {
      for w in ["5h", "7d"] {
        let e = lib::next_edge(r.caps[w], clock);
        if e != () && (t == () || now + e * 60 < t) { t = now + e * 60; }
      }
    }
    if t != () && t > now && (at == () || t < at) { at = t; }
  }

  #{ order: order, why: why, hold_until: hold, wake: #{ at: at, when: when }, memo: memo }
}
