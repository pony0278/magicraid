// 魔法基地突襲 — web 殼(表現層)。玩法走 Rust sim(WASM);此檔只做渲染/輸入/動畫接點。
// view metadata(法術 icon/名稱/瞄準型別)留在這層(B0 §C-3)。

// ── 法術表(view metadata;code 對齊 sim SPELL_ORDER)──
const SPELLS = [
  { code: 0, id: "bolt", name: "Bolt", icon: "✨", target: "enemy", baseline: true },
  { code: 1, id: "push", name: "Push", icon: "👐", target: "adjEnemy", baseline: true },
  { code: 2, id: "fire", name: "Fireball", icon: "🔥", target: "cell" },
  { code: 3, id: "heavy", name: "Inferno", icon: "☄", target: "cell", channel: true },
  { code: 4, id: "oilflask", name: "Oil Flask", icon: "🛢️", target: "cell" },
  { code: 5, id: "hook", name: "Hook", icon: "🪝", target: "enemy" },
  { code: 6, id: "haste", name: "Haste", icon: "⚡", target: "self" },
];
const ROOM_NAMES = [
  "Room 1 · Order & Sight", "Room 2 · Spikes", "Room 3 · Oil & Wood",
  "Room 4 · Open Field", "Room 5 · Oil Field", "Boss · Rune Golem",
];
const TILE = { FLOOR: 0, WALL: 1, WOOD: 2, WOODBURN: 3, OIL: 4, SPIKE: 5, RUNE: 6 };
const ST = { INPUT: 0, RELEASE: 1, PICK: 2, COMPLETE: 3, DEFEAT: 4 };
const KIND_ICON = { mage: "🧙", imp: "👹", eye: "👁", boss: "🗿" };
// reject 原因碼 → 訊息(對齊 web/src/lib.rs reject_code)。
const REJECT_MSG = {
  1: "No enemy there",
  2: "Out of range",
  3: "No line of sight (a wall blocks it)",
  4: "Not adjacent — Push only hits enemies right next to you (incl. diagonals)",
  5: "It's already next to you",
  6: "That's a wall",
  7: "Floor tiles only",
  8: "Out of bounds",
  9: "No potion, or HP is full",
  10: "Can't move there (occupied / same tile)",
  11: "No path",
  12: "Nothing to release",
};

// ── Poki SDK 薄殼:有就用,沒有就 no-op(離線/本機開發)──
const Poki = (() => {
  const sdk = window.PokiSDK;
  const noop = () => {};
  // 無 SDK(本機 / GitHub Pages):廣告直接放行,只呼叫 onDone。
  if (!sdk) {
    return { loadingFinished: noop, gameplayStart: noop, gameplayStop: noop,
      commercialBreak: (onDone) => (typeof onDone === "function" ? onDone : noop)() };
  }
  return {
    loadingFinished: () => sdk.gameLoadingFinished && sdk.gameLoadingFinished(),
    gameplayStart: () => sdk.gameplayStart && sdk.gameplayStart(),
    gameplayStop: () => sdk.gameplayStop && sdk.gameplayStop(),
    // 插頁廣告:先 gameplayStop(Poki 規範:廣告期間暫停/靜音),播完(或無廣告/失敗)再 onDone。
    // gameplayStart 由下一場的 firstInput 觸發。Poki 自帶頻率上限,故每個自然斷點都呼叫即可。
    commercialBreak: (onDone) => {
      const done = typeof onDone === "function" ? onDone : noop;
      sdk.gameplayStop && sdk.gameplayStop();
      if (!sdk.commercialBreak) { done(); return; }
      Promise.resolve(sdk.commercialBreak()).then(done, done);
    },
  };
})();

let wasm, mem; // wasm exports + helper
let state = null;
let selSpell = null; // 目前選取的法術(target 需瞄準時)
let pending = null; // 觸控預覽:第一次點的瞄準格 {x,y};點同格才提交(防誤觸)
let pendingDrop = null; // 丟牌 UI:欄位滿時待換上的法術 code(等玩家選丟哪張;null = 非丟牌中)
let gameplayStarted = false;
let anim = null; // 進行中的動畫(events→重建);null = 靜態
let mode = "wild"; // "wild" 野區 run | "editor" 基地編輯器 | "raid" 自己試打基地(Demo 2)

const $ = (id) => document.getElementById(id);
const cv = $("cv"), ctx = cv.getContext("2d");

// ── 讀 wasm 記憶體中的 JSON(每次重抓 buffer,避免成長後 detach)──
function readJson(ptr) {
  const len = wasm.mr_buf_len();
  const bytes = new Uint8Array(wasm.memory.buffer, ptr, len);
  return JSON.parse(new TextDecoder().decode(bytes));
}
function refresh() { state = readJson(wasm.mr_render()); }
function readEvents() { return readJson(wasm.mr_events()); }
// 取目前畫面上各實體的位置/狀態(id → {...}),供動畫補間 before→after。
function entMap(st) {
  const m = {};
  for (const e of st.ents) m[e.id] = e;
  return m;
}

async function boot() {
  // 用 arrayBuffer 路徑:不依賴伺服器把 .wasm 標成 application/wasm。
  const buf = await (await fetch("magicraid_web.wasm")).arrayBuffer();
  const { instance } = await WebAssembly.instantiate(buf, {});
  wasm = instance.exports;
  Poki.loadingFinished();
  newRun((Math.random() * 0xffffffff) >>> 0);
  window.addEventListener("resize", () => { if (mode === "editor") drawEditorStatic(); else if (!anim) drawStatic(); });
  const mb = $("modeBtn");
  if (mb) { mb.addEventListener("click", toggleMode); syncModeBtn(); }
}

function newRun(seed) {
  wasm.mr_new(seed >>> 0);
  selSpell = null;
  pending = null;
  pendingDrop = null;
  gameplayStarted = false;
  refresh();
  render();
}

// 玩家第一次輸入時才觸發 gameplayStart(Poki 規範)。
function firstInput() {
  if (!gameplayStarted) { gameplayStarted = true; Poki.gameplayStart(); }
}

// ── 套用一手 ──
function doStep(act, x = 0, y = 0, spell = 0) {
  if (anim) finishAnim(); // 連點:先把上一段動畫收尾
  pending = null; // 提交即清掉瞄準預覽
  firstInput();
  const before = entMap(state); // 動畫起點(這手之前的位置/血量)
  const fireBefore = state.fire.map((r) => r.slice()); // 火蔓延動畫:step 前的火格快照
  wasm.mr_step(act, x, y, spell);
  const rejected = wasm.mr_rejected() === 1;
  selSpell = null;
  const events = readEvents();
  const reason = wasm.mr_reject_reason();
  refresh(); // state = 新狀態
  logStep(act, x, y, spell, before, events, rejected, reason);
  if (rejected) {
    flash("✋ " + (REJECT_MSG[reason] || "Invalid move"));
    render();
    return;
  }
  $("feedback").textContent = "";
  startAnim(before, events, fireBefore);
}

// ── 火蔓延動畫(view-diff,A 案;不碰 sim)──
// 比對 step 前後的 fire 格,對「新點燃」的格子按「離既有火源的 BFS 距離」排圈,漣漪式逐圈亮起。
// 拿不到 sim 內真實點燃順序,用距離近似(Demo 0 夠看);純表現層,確定性不受影響。
const FIRE_RING_MS = 55;     // 每一圈漣漪間隔
const FIRE_FADE_MS = 90;     // 單格亮起的淡入
const FIRE_REVEAL_CAP = 450; // 整段火蔓延最長(避免長油帶拖太久)

function computeFireRings(fireBefore) {
  const W = state.w, H = state.h, ring = new Map(), now = state.fire;
  const newly = [];
  for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) {
    if (now[y][x] > 0 && !(fireBefore[y] && fireBefore[y][x] > 0)) newly.push([x, y]);
  }
  if (newly.length === 0) return { ring, maxRing: 0 };
  const newlySet = new Set(newly.map(([x, y]) => y * W + x));
  // 火源 = step 前就在燒的格子;從它們 BFS 出去給新格排圈。
  const q = [];
  for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) {
    if (fireBefore[y] && fireBefore[y][x] > 0) q.push([x, y, 0]);
  }
  if (q.length === 0) { // 無前火源(全新點燃,如火球炸乾油)→ 同圈一起亮
    for (const [x, y] of newly) ring.set(y * W + x, 0);
    return { ring, maxRing: 0 };
  }
  let head = 0, maxRing = 0;
  while (head < q.length) {
    const [x, y, d] = q[head++];
    for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) {
      if (!dx && !dy) continue;
      const nx = x + dx, ny = y + dy;
      if (nx < 0 || ny < 0 || nx >= W || ny >= H) continue;
      const k = ny * W + nx;
      if (newlySet.has(k) && !ring.has(k)) { ring.set(k, d + 1); maxRing = Math.max(maxRing, d + 1); q.push([nx, ny, d + 1]); }
    }
  }
  for (const [x, y] of newly) { const k = y * W + x; if (!ring.has(k)) { ring.set(k, 1); maxRing = Math.max(maxRing, 1); } } // 與火源斷開的孤格
  return { ring, maxRing };
}

// 開始動畫:把這手的事件**依序逐格重放**(看得到推→敵人走回來、命中閃光、死亡淡出),
// 火蔓延則沿整段時間漣漪亮起。對應 B0 §C-4 events→動畫;不再只補間 net。
function startAnim(before, events, fireBefore) {
  const disp = {};
  for (const id in before) disp[id] = { ...before[id] };
  const segs = [];
  for (const e of events) {
    if (e.t === "mv") segs.push({ k: "mv", id: e.id, from: [e.fx, e.fy], to: [e.tx, e.ty] });
    else if (e.t === "dmg") segs.push({ k: "dmg", id: e.id, amt: e.amt });
    else if (e.t === "heal") segs.push({ k: "heal", id: e.id, amt: e.amt });
    else if (e.t === "die") segs.push({ k: "die", id: e.id });
  }
  const { ring: fireRing, maxRing } = computeFireRings(fireBefore);
  const ringMs = maxRing > 0 ? Math.min(FIRE_RING_MS, FIRE_REVEAL_CAP / maxRing) : FIRE_RING_MS;
  const fireDur = fireRing.size > 0 ? maxRing * ringMs + FIRE_FADE_MS : 0;
  if (segs.length === 0 && fireDur === 0) { render(); return; } // 無事可演
  const dur = segs.length > 0 ? Math.max(70, Math.min(140, Math.floor(700 / segs.length))) : 0;
  anim = {
    disp, segs, dur, applied: 0,
    tStart: performance.now(),
    totalDur: Math.max(segs.length * dur, fireDur),
    fireRing, ringMs, fireFade: FIRE_FADE_MS,
  };
  requestAnimationFrame(tick);
}

function finishAnim() {
  anim = null;
  render(); // 落定到最終狀態 + 更新面板(火格也回到完整態)
}

// 套用一個片段的「結果」到顯示狀態(片段播完時)。
function applySeg(seg) {
  const e = anim.disp[seg.id];
  if (seg.k === "mv" && e) { e.x = seg.to[0]; e.y = seg.to[1]; }
  else if (seg.k === "dmg" && e) { e.hp -= seg.amt; }
  else if (seg.k === "heal" && e) { e.hp += seg.amt; }
  else if (seg.k === "die") { delete anim.disp[seg.id]; }
}

// 全段以 tStart 為基準的全域時間:實體 seg 逐格推進,火蔓延沿同一時鐘漣漪。
function tick(now) {
  if (!anim) return;
  const el = now - anim.tStart;
  const segDone = anim.dur > 0 ? Math.min(anim.segs.length, Math.floor(el / anim.dur)) : anim.segs.length;
  while (anim.applied < segDone) { applySeg(anim.segs[anim.applied]); anim.applied++; }
  if (el >= anim.totalDur) { finishAnim(); return; }
  drawAnimFrame(el);
  requestAnimationFrame(tick);
}

// 火格透明度(依漣漪圈到達時間);非新點燃格回 1(前火源/最終態照常)。
function fireAlphaAt(el) {
  const W = state.w;
  return (x, y) => {
    const r = anim.fireRing.get(y * W + x);
    if (r === undefined) return 1;
    return Math.max(0, Math.min(1, (el - r * anim.ringMs) / anim.fireFade));
  };
}

function flash(msg) { $("feedback").textContent = msg; }

// ── 動作紀錄(調查用)──
const logLines = [];
function logMsg(s) {
  logLines.push(s);
  if (logLines.length > 30) logLines.shift();
  const el = $("log");
  el.textContent = logLines.join("\n");
  el.scrollTop = el.scrollHeight;
}
const ACT_NAME = { 0: "Wait", 1: "Potion", 2: "Move", 4: "Release" };
// 把一手記成一行:動作→目標 [法師位置 / 切比雪夫距離]:結果(移動/傷害 或 拒絕原因)。
function logStep(act, x, y, spell, before, events, rejected, reason) {
  const name = act === 3 ? `Cast: ${SPELLS[spell].name}` : ACT_NAME[act] || `act${act}`;
  const m = before[0]; // 法師(id 0)行動前位置
  let head = name;
  if (act === 2 || act === 3) {
    const d = m ? Math.max(Math.abs(x - m.x), Math.abs(y - m.y)) : "?";
    head += `→(${x},${y}) [mage(${m ? m.x : "?"},${m ? m.y : "?"}) d=${d}]`;
  }
  let out;
  if (rejected) {
    out = "✗ " + (REJECT_MSG[reason] || `Rejected #${reason}`);
  } else {
    const mv = events.filter((e) => e.t === "mv").map((e) => `#${e.id}(${e.fx},${e.fy})→(${e.tx},${e.ty})`);
    const dmg = events.filter((e) => e.t === "dmg").map((e) => `#${e.id}-${e.amt}`);
    const die = events.filter((e) => e.t === "die").map((e) => `☠#${e.id}`);
    out = "✓ " + [mv.length ? "mv:" + mv.join(" ") : "", dmg.length ? "dmg:" + dmg.join(" ") : "", die.join(" ")]
      .filter(Boolean).join(" | ") || "✓ (no events)";
  }
  logMsg(`${head}: ${out}`);
}

// ── 點擊格子 ──
function cellFromEvent(ev) {
  const r = cv.getBoundingClientRect();
  const px = (ev.touches ? ev.touches[0].clientX : ev.clientX) - r.left;
  const py = (ev.touches ? ev.touches[0].clientY : ev.clientY) - r.top;
  const cell = cv._cell;
  const x = Math.floor((px / r.width) * cv.width / cell);
  const y = Math.floor((py / r.height) * cv.height / cell);
  return { x, y };
}

// 觸控預覽→確認:第一次點 = 設瞄準(畫高亮 + 虛線),點同格 = 提交,點別格 = 改目標。
// 回合制每手流時間,點錯一格白送一拳 → 確認步驟擋誤觸(對齊 docs/02 I-go-you-go)。
function onCanvasTap(ev) {
  ev.preventDefault();
  if (mode === "editor") { editorTap(ev); return; } // Demo 2:編輯器點塗
  if (!state) return;
  if (anim) { finishAnim(); return; } // 動畫中點一下 = 跳過
  if (state.status === ST.RELEASE) { pending = null; doStep(4); return; } // 釋放蓄力:任意點擊
  if (state.status !== ST.INPUT) return;
  const { x, y } = cellFromEvent(ev);
  if (x < 0 || y < 0 || x >= state.w || y >= state.h) { pending = null; drawStatic(); return; }
  if (pending && pending.x === x && pending.y === y) {
    const p = pending; pending = null;
    if (selSpell !== null) doStep(3, p.x, p.y, selSpell); // 施法
    else doStep(2, p.x, p.y, 0);                          // 移動
  } else {
    pending = { x, y };
    flash(selSpell !== null ? "Tap again to cast ✓ (tap elsewhere to re-aim)" : "Tap again to move ✓ (tap elsewhere to re-aim)");
    drawStatic();
  }
}
cv.addEventListener("click", onCanvasTap);

// ── UI:動作列 + 順序鏈 + overlay ──
function render() {
  if (mode === "editor") { renderEditor(); return; } // Demo 2 編輯器自有渲染
  $("hud").textContent = mode === "raid" ? "Test raid 🛡" : (ROOM_NAMES[state.room] || `Room ${state.room}`);
  renderChain();
  renderBar();
  renderOverlay();
  drawStatic();
}

function renderChain() {
  const el = $("chain"); el.innerHTML = "";
  for (const s of state.chain) {
    const d = document.createElement("div");
    d.className = "slot" + (s.k === "mage" ? " mage" : "") + (s.rel ? " rel" : "");
    d.textContent = (KIND_ICON[s.k] || "?") + (s.rel ? " ⚡" : "");
    el.appendChild(d);
  }
}

function renderBar() {
  const bar = $("bar"); bar.innerHTML = "";
  const playable = state.status === ST.INPUT;
  // 可用法術 = 基礎包(bolt + push)+ 已撿
  const baseCodes = SPELLS.filter((s) => s.baseline).map((s) => s.code);
  const codes = [...baseCodes, ...state.acquired];
  for (const code of codes) {
    const sp = SPELLS[code];
    const b = document.createElement("button");
    b.className = "act" + (selSpell === code ? " sel" : "");
    const tier = state.tiers && state.tiers[code] >= 2 ? ' <span class="star">★</span>' : "";
    b.innerHTML = `${sp.icon} ${sp.name}${tier}`;
    b.disabled = !playable;
    b.onclick = () => {
      firstInput();
      if (sp.target === "self") { doStep(3, 0, 0, code); return; }
      selSpell = selSpell === code ? null : code;
      pending = null; // 換法術 → 清掉舊瞄準
      flash(selSpell !== null ? `${sp.name} selected — tap a target, then tap again to confirm.` : "");
      renderBar();
      drawStatic();
    };
    bar.appendChild(b);
  }
  // 待機 / 回血瓶 / 重來
  const wait = mkBtn("⏳ Wait", playable, () => doStep(0));
  bar.appendChild(wait);
  const pot = mkBtn(`🧪 Potion ×${state.potions}`, playable && state.potions > 0, () => doStep(1));
  bar.appendChild(pot);
}

function mkBtn(label, enabled, fn) {
  const b = document.createElement("button");
  b.className = "act"; b.innerHTML = label; b.disabled = !enabled;
  b.onclick = fn; return b;
}

function renderOverlay() {
  const ov = $("overlay"), btns = $("ovBtns");
  btns.innerHTML = "";
  const show = (title, text) => { $("ovTitle").textContent = title; $("ovText").textContent = text; ov.classList.add("on"); };
  ov.classList.remove("on");

  if (state.status === ST.PICK && pendingDrop !== null) {
    // 丟牌 UI:欄位滿(3/3),選一張丟掉換上 pendingDrop(§10「最濃的取捨」)。
    const np = SPELLS[pendingDrop];
    show(`Spells full (3) — take ${np.icon} ${np.name}`, "Drop one to make room. This is the choice that matters.");
    for (const oid of state.acquired) {
      const s = SPELLS[oid];
      const star = state.tiers && state.tiers[oid] >= 2 ? " ★" : "";
      btns.appendChild(mkBtn(`Drop ${s.icon} ${s.name}${star}`, true, () => {
        wasm.mr_drop(pendingDrop, oid); pendingDrop = null; wasm.mr_next_room(); selSpell = null; refresh(); render();
      }));
    }
    btns.appendChild(mkBtn("← Cancel (pick another)", true, () => { pendingDrop = null; renderOverlay(); }));
  } else if (state.status === ST.PICK) {
    const offers = readJson(wasm.mr_offers());
    if (offers.length === 0) {
      show("Room cleared ✨", "Nothing left to grab — press on.");
      btns.appendChild(mkBtn("Continue →", true, () => { wasm.mr_next_room(); selSpell = null; refresh(); render(); }));
    } else {
      show("Cleared! Pick one", `Take one (slots ${state.acquired.length}/3). Different spells grow different playstyles.`);
      for (const code of offers) {
        const sp = SPELLS[code];
        const owned = state.acquired.includes(code);
        const b = mkBtn(`${sp.icon} ${owned ? "Upgrade " : ""}${sp.name}${owned ? " ★→★★" : ""}`, true, () => {
          if (wasm.mr_pick(code) === 1) { pendingDrop = code; renderOverlay(); } // 欄位滿 → 丟牌 UI
          else { wasm.mr_next_room(); selSpell = null; refresh(); render(); }     // 撿到/升級 → 下一房
        });
        btns.appendChild(b);
      }
    }
  } else if (state.status === ST.COMPLETE) {
    if (mode === "raid") { // 自己試打:突襲者(你)贏 = 基地被攻破
      show("Base cracked 🗝", "The raider reached the core / cleared it. Make it nastier?");
      btns.appendChild(mkBtn("↻ Retry raid", true, () => startRaid()));
      btns.appendChild(mkBtn("← Back to editor", true, () => enterEditor()));
    } else {
      show("Victory 🎉", "Run complete — that's the 'one more run'.");
      btns.appendChild(mkBtn("Play Again", true, () => { Poki.commercialBreak(() => newRun((Math.random() * 0xffffffff) >>> 0)); }));
    }
  } else if (state.status === ST.DEFEAT) {
    if (mode === "raid") { // 自己試打:突襲者(你)死 = 基地守住
      show("Base held 🛡", "The raider fell before the core. Maybe too hard?");
      btns.appendChild(mkBtn("↻ Retry raid", true, () => startRaid()));
      btns.appendChild(mkBtn("← Back to editor", true, () => enterEditor()));
    } else {
      show("Defeated 💀", "Try again — roguelite's 'one more run'.");
      btns.appendChild(mkBtn("Restart", true, () => { Poki.commercialBreak(() => newRun((Math.random() * 0xffffffff) >>> 0)); }));
    }
  }
}

// ── Canvas 繪製 ──
const COLORS = {
  [TILE.FLOOR]: "#3b3157", [TILE.WALL]: "#15101f", [TILE.WOOD]: "#5a4326",
  [TILE.WOODBURN]: "#7a4326", [TILE.OIL]: "#3a4a35", [TILE.SPIKE]: "#4a2f3a", [TILE.RUNE]: "#3a3a6a",
};

function fitCanvas(w, h) {
  w = w || state.w; h = h || state.h; // 省略 = 用 state 尺寸(野區/試打);編輯器傳基地尺寸
  const stage = $("stage").getBoundingClientRect();
  const cell = Math.max(28, Math.floor(Math.min(stage.width / w, stage.height / h)));
  cv.width = w * cell; cv.height = h * cell; cv._cell = cell;
}

// 地形/火/預告。fireAlpha(x,y)→0..1 供火蔓延漣漪;省略 = 全亮(靜態/最終態)。
function drawBackground(cell, fireAlpha) {
  for (let y = 0; y < state.h; y++) for (let x = 0; x < state.w; x++) {
    const t = state.tiles[y][x];
    ctx.fillStyle = COLORS[t] || "#3b3157";
    ctx.fillRect(x * cell, y * cell, cell - 1, cell - 1);
    if (t === TILE.SPIKE) { ctx.fillStyle = "#c45"; glyph("▲", x, y, cell, 0.5); }
    if (t === TILE.RUNE) { glyph("⚡", x, y, cell, 0.6); }
    if (t === TILE.WOOD || t === TILE.WOODBURN) { ctx.strokeStyle = "#0004"; ctx.strokeRect(x*cell+2, y*cell+2, cell-5, cell-5); }
    if (state.fire[y][x] > 0) {
      const fa = fireAlpha ? fireAlpha(x, y) : 1;
      if (fa > 0) {
        ctx.globalAlpha = fa;
        ctx.fillStyle = "rgba(255,122,60,.55)"; ctx.fillRect(x*cell, y*cell, cell-1, cell-1); glyph("🔥", x, y, cell, 0.6);
        ctx.globalAlpha = 1;
      }
    }
  }
  for (const [cx, cy] of state.slamCells) {
    ctx.fillStyle = "rgba(226,87,76,.4)";
    ctx.fillRect(cx * cell, cy * cell, cell - 1, cell - 1);
    ctx.strokeStyle = "#e2574c"; ctx.lineWidth = 2;
    ctx.strokeRect(cx*cell+1, cy*cell+1, cell-3, cell-3);
  }
  if (state.core) { // base-raid:核心目標格(突襲者要踩到的)
    const [cx, cy] = state.core;
    ctx.fillStyle = "rgba(245,200,74,.18)"; ctx.fillRect(cx*cell, cy*cell, cell-1, cell-1);
    ctx.strokeStyle = "#f5c84a"; ctx.lineWidth = 2; ctx.strokeRect(cx*cell+1, cy*cell+1, cell-3, cell-3);
    glyph("💎", cx, cy, cell, 0.6);
  }
}

// 畫一隻實體(x,y 為格座標,可為小數以做補間;flash 0–1 閃光;alpha 死亡淡出;
// flashKind "heal" = 綠色回血閃光,其餘 = 紅色命中閃光)。
function drawEntity(e, cell, alpha, flash, flashKind) {
  ctx.globalAlpha = alpha;
  if (flash > 0) {
    ctx.fillStyle = flashKind === "heal" ? `rgba(120,230,140,${0.6 * flash})` : `rgba(255,90,80,${0.6 * flash})`;
    ctx.fillRect(e.x * cell, e.y * cell, cell - 1, cell - 1);
  }
  glyph(KIND_ICON[e.k] || "?", e.x, e.y, cell, 0.62);
  drawHp(e, cell);
  if (e.ch) {
    ctx.strokeStyle = e.ready ? "#f5c84a" : "#74e0d4"; ctx.lineWidth = 2;
    ctx.strokeRect(e.x*cell+2, e.y*cell+2, cell-5, cell-5);
  }
  ctx.globalAlpha = 1;
}

function drawStatic() {
  if (!state) return;
  fitCanvas();
  const cell = cv._cell;
  ctx.clearRect(0, 0, cv.width, cv.height);
  drawBackground(cell);
  for (const e of state.ents) drawEntity(e, cell, 1, 0);
  drawAim(cell);
}

// 觸控瞄準預覽:法師→目標的虛線 + 目標格高亮(純指示;合法性由 sim 在提交時裁決)。
function drawAim(cell) {
  if (!pending || !state) return;
  const { x, y } = pending;
  const m = state.ents.find((e) => e.k === "mage");
  if (m) {
    ctx.strokeStyle = "rgba(116,224,212,.5)"; ctx.lineWidth = 2; ctx.setLineDash([5, 5]);
    ctx.beginPath();
    ctx.moveTo(m.x * cell + cell / 2, m.y * cell + cell / 2);
    ctx.lineTo(x * cell + cell / 2, y * cell + cell / 2);
    ctx.stroke(); ctx.setLineDash([]);
  }
  ctx.fillStyle = "rgba(116,224,212,.18)"; ctx.fillRect(x * cell, y * cell, cell - 1, cell - 1);
  ctx.strokeStyle = "#74e0d4"; ctx.lineWidth = 2.5; ctx.strokeRect(x * cell + 2, y * cell + 2, cell - 5, cell - 5);
}

// 動畫幀(el = 自 tStart 起的毫秒):畫進行中的 seg(移動補間 / 閃光 + 飄字 / 死亡淡出),
// 其餘實體靜止於 disp;火格依漣漪時間亮起。seg 全播完後仍可能在等火蔓延收尾。
function drawAnimFrame(el) {
  if (!state) return;
  fitCanvas();
  const cell = cv._cell;
  ctx.clearRect(0, 0, cv.width, cv.height);
  drawBackground(cell, fireAlphaAt(el));
  const ai = anim.applied;
  const seg = ai < anim.segs.length ? anim.segs[ai] : null;
  const p = seg && anim.dur > 0 ? Math.min(1, (el - ai * anim.dur) / anim.dur) : 0;
  const ease = 1 - (1 - p) * (1 - p);
  for (const id in anim.disp) {
    const e = anim.disp[id];
    let x = e.x, y = e.y, alpha = 1, flash = 0;
    if (seg && +id === seg.id) {
      if (seg.k === "mv") { x = seg.from[0] + (seg.to[0] - seg.from[0]) * ease; y = seg.from[1] + (seg.to[1] - seg.from[1]) * ease; }
      else if (seg.k === "dmg" || seg.k === "heal") flash = 1 - p;
      else if (seg.k === "die") alpha = 1 - p;
    }
    const fk = seg && +id === seg.id && seg.k === "heal" ? "heal" : "hit";
    drawEntity({ ...e, x, y }, cell, alpha, flash, fk);
  }
  // 傷害/回血飄字
  if (seg && (seg.k === "dmg" || seg.k === "heal") && anim.disp[seg.id]) {
    const e = anim.disp[seg.id];
    ctx.globalAlpha = 1 - p;
    ctx.fillStyle = seg.k === "heal" ? "#bff5c0" : "#ffd0c0";
    ctx.font = `${Math.floor(cell * 0.42)}px system-ui, sans-serif`;
    ctx.textAlign = "center"; ctx.textBaseline = "middle";
    ctx.fillText(`${seg.k === "heal" ? "+" : "-"}${seg.amt}`, e.x * cell + cell / 2, e.y * cell + cell / 2 - p * cell * 0.7);
    ctx.globalAlpha = 1;
  }
}

function glyph(ch, x, y, cell, scale) {
  ctx.fillStyle = "#efe7d2";
  ctx.font = `${Math.floor(cell * scale)}px system-ui, sans-serif`;
  ctx.textAlign = "center"; ctx.textBaseline = "middle";
  ctx.fillText(ch, x * cell + cell / 2, y * cell + cell / 2 + 1);
}

function drawHp(e, cell) {
  const w = cell - 8, ratio = Math.max(0, e.hp / e.max);
  const bx = e.x * cell + 4, by = e.y * cell + cell - 6;
  ctx.fillStyle = "#0006"; ctx.fillRect(bx, by, w, 3);
  ctx.fillStyle = e.k === "mage" ? "#74e0d4" : "#e2574c";
  ctx.fillRect(bx, by, w * ratio, 3);
}

// ════════════════════ Demo 2:基地編輯器 + 試打 ════════════════════
// 基地 = 玩家畫的一間房,同一顆 sim 拿來打(§4)。離線:編輯 → bot/自己試打 → 調整。
const DEFAULT_BASE = [
  "###########",
  "#@........#",
  "#.........#",
  "#.........#",
  "#.........#",
  "#.........#",
  "#........C#",
  "###########",
];
const PALETTE = [
  { ch: ".", name: "Erase", icon: "·" },
  { ch: "#", name: "Wall", icon: "🧱" },
  { ch: "W", name: "Wood", icon: "🪵" },
  { ch: "~", name: "Oil", icon: "🛢️" },
  { ch: "s", name: "Spike", icon: "🔺" },
  { ch: "o", name: "Imp", icon: "👹" },
  { ch: "e", name: "Eye", icon: "👁" },
  { ch: "@", name: "Spawn", icon: "🚪" },
  { ch: "C", name: "Core", icon: "💎" },
];
// 突襲者可帶的威脅牌(基礎包 bolt/push 永遠有);bit = sim SPELL code。測「我的防守扛得住嗎」。
const LOADOUT = [{ bit: 1 << 2, name: "Fire" }, { bit: 1 << 5, name: "Hook" }, { bit: 1 << 6, name: "Haste" }];
const BASE_KEY = "magicraid_base_v1";
// 放置預算:擋「填滿地圖 → 無解」。成本是佔位,playtest 再轉。出生/核心/地板免費;邊界鎖牆不計。
const COST = { "#": 1, "W": 1, "~": 1, "s": 2, "o": 3, "e": 3, "@": 0, "C": 0, ".": 0 };
const BUDGET = 30;
let baseGrid = null, paintChar = "#", loadoutMask = 0;

function usedBudget() {
  let n = 0;
  for (let y = 1; y < baseGrid.length - 1; y++)
    for (let x = 1; x < baseGrid[y].length - 1; x++) n += COST[baseGrid[y][x]] || 0;
  return n;
}
function updateEditorHud() { $("hud").textContent = `Base Editor · 💰 ${usedBudget()}/${BUDGET}`; }

function enterEditor() {
  mode = "editor";
  if (!baseGrid) baseGrid = (loadSavedBase() || DEFAULT_BASE).map((r) => r.split(""));
  render(); syncModeBtn();
}
function enterWild() { mode = "wild"; newRun((Math.random() * 0xffffffff) >>> 0); syncModeBtn(); }
function toggleMode() { if (mode === "editor") enterWild(); else enterEditor(); }
function syncModeBtn() { const b = $("modeBtn"); if (b) b.textContent = mode === "editor" ? "🗺 Adventure" : "🏰 Base Editor"; }

function saveBase() { try { localStorage.setItem(BASE_KEY, serializeBase()); } catch (e) { /* 無痕 */ } }
function loadSavedBase() { try { const s = localStorage.getItem(BASE_KEY); return s ? s.split("\n") : null; } catch (e) { return null; } }
function serializeBase() { return baseGrid.map((r) => r.join("")).join("\n"); }
function writeBaseToInput() {
  const bytes = new TextEncoder().encode(serializeBase());
  const ptr = wasm.mr_input_reserve(bytes.length);
  new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
}
function validateBase() {
  const s = serializeBase();
  const sp = (s.match(/@/g) || []).length, co = (s.match(/C/g) || []).length;
  if (sp !== 1 || co !== 1) { flash(`Need exactly 1 spawn (have ${sp}) and 1 core (have ${co}).`); return false; }
  return true;
}

// ── 編輯器渲染 ──
function renderEditor() {
  updateEditorHud();
  $("chain").innerHTML = "";
  $("overlay").classList.remove("on");
  drawEditorStatic();
  renderEditorBar();
}
function drawEditorStatic() {
  if (!baseGrid) return;
  fitCanvas(baseGrid[0].length, baseGrid.length);
  const cell = cv._cell;
  ctx.clearRect(0, 0, cv.width, cv.height);
  for (let y = 0; y < baseGrid.length; y++) for (let x = 0; x < baseGrid[y].length; x++) {
    const ch = baseGrid[y][x];
    ctx.fillStyle = ch === "#" ? COLORS[TILE.WALL] : ch === "W" ? COLORS[TILE.WOOD]
      : ch === "~" ? COLORS[TILE.OIL] : ch === "s" ? COLORS[TILE.SPIKE] : COLORS[TILE.FLOOR];
    ctx.fillRect(x * cell, y * cell, cell - 1, cell - 1);
    if (ch === "s") glyph("🔺", x, y, cell, 0.5);
    else if (ch === "W") { ctx.strokeStyle = "#0004"; ctx.strokeRect(x * cell + 2, y * cell + 2, cell - 5, cell - 5); }
    else if (ch === "o") glyph("👹", x, y, cell, 0.6);
    else if (ch === "e") glyph("👁", x, y, cell, 0.6);
    else if (ch === "@") glyph("🚪", x, y, cell, 0.62);
    else if (ch === "C") glyph("💎", x, y, cell, 0.62);
    ctx.strokeStyle = "#ffffff12"; ctx.strokeRect(x * cell, y * cell, cell - 1, cell - 1); // 格線
  }
}
function renderEditorBar() {
  const bar = $("bar"); bar.innerHTML = "";
  for (const p of PALETTE) {
    const b = document.createElement("button");
    b.className = "act" + (paintChar === p.ch ? " sel" : "");
    b.innerHTML = COST[p.ch] ? `${p.icon}<sub style="font-size:9px;color:var(--gold)">${COST[p.ch]}</sub>` : p.icon;
    b.title = `${p.name}${COST[p.ch] ? ` · ${COST[p.ch]} pts` : ""}`;
    b.onclick = () => { paintChar = p.ch; renderEditorBar(); };
    bar.appendChild(b);
  }
  bar.appendChild(mkBtn("🤖 Bot test", true, botTest));
  bar.appendChild(mkBtn("🎮 Play test", true, () => { if (validateBase()) startRaid(); }));
  bar.appendChild(mkBtn("🗑 Reset", true, () => { baseGrid = DEFAULT_BASE.map((r) => r.split("")); saveBase(); drawEditorStatic(); updateEditorHud(); }));
  for (const l of LOADOUT) {
    const on = (loadoutMask & l.bit) !== 0;
    bar.appendChild(mkBtn(`${on ? "☑" : "☐"} ${l.name}`, true, () => { loadoutMask ^= l.bit; renderEditorBar(); }));
  }
}
function editorTap(ev) {
  if (!baseGrid) return;
  const { x, y } = cellFromEvent(ev);
  const h = baseGrid.length, w = baseGrid[0].length;
  if (x < 0 || y < 0 || x >= w || y >= h) return;
  if (x === 0 || y === 0 || x === w - 1 || y === h - 1) return; // 邊界牆鎖死,不畫開放邊
  // 預算檢查:換上新格的成本差;超預算就擋(防填滿地圖)。
  const delta = (COST[paintChar] || 0) - (COST[baseGrid[y][x]] || 0);
  if (usedBudget() + delta > BUDGET) {
    flash(`Out of budget (${usedBudget()}/${BUDGET}) — erase something first.`);
    return;
  }
  if (paintChar === "@" || paintChar === "C") { // 出生/核心唯一:先清舊的
    for (let yy = 0; yy < h; yy++) for (let xx = 0; xx < w; xx++) if (baseGrid[yy][xx] === paintChar) baseGrid[yy][xx] = ".";
  }
  baseGrid[y][x] = paintChar;
  saveBase();
  drawEditorStatic();
  updateEditorHud();
}

// ── 試打 ──
function botTest() {
  if (!validateBase()) return;
  writeBaseToInput();
  const res = readJson(wasm.mr_bot_raid(loadoutMask));
  flash(res.outcome === "cracked" ? `🤖 Bot cracked it in ${res.steps} moves — make it nastier?`
    : res.outcome === "held" ? "🛡 Bot died — base held. Maybe too hard?"
    : "⏳ Bot couldn't crack it (stuck / no path).");
}
function startRaid() {
  if (!validateBase()) { enterEditor(); return; }
  writeBaseToInput();
  wasm.mr_new_base(loadoutMask);
  mode = "raid"; selSpell = null; pending = null;
  refresh(); render(); syncModeBtn();
}

boot().catch((err) => { document.body.innerHTML = "<p style='padding:20px'>Failed to load: " + err + "</p>"; });
