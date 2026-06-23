// 魔法基地突襲 — web 殼(表現層)。玩法走 Rust sim(WASM);此檔只做渲染/輸入/動畫接點。
// view metadata(法術 icon/名稱/瞄準型別)留在這層(B0 §C-3)。

// ── 法術表(view metadata;code 對齊 sim SPELL_ORDER)──
const SPELLS = [
  { code: 0, id: "bolt", name: "魔法彈", icon: "✨", target: "enemy", baseline: true },
  { code: 1, id: "push", name: "推", icon: "👐", target: "adjEnemy", baseline: true },
  { code: 2, id: "fire", name: "火球", icon: "🔥", target: "cell" },
  { code: 3, id: "heavy", name: "烈焰術", icon: "☄", target: "cell", channel: true },
  { code: 4, id: "oilflask", name: "澆油", icon: "🛢️", target: "cell" },
  { code: 5, id: "hook", name: "勾索", icon: "🪝", target: "enemy" },
  { code: 6, id: "haste", name: "加速術", icon: "⚡", target: "self" },
];
const ROOM_NAMES = [
  "房間 1 · 順序鏈 + 視線", "房間 2 · 尖刺場", "房間 3 · 油 + 木牆",
  "房間 4 · 開闊場", "房間 5 · 油陣", "關主 · 符文魔像",
];
const TILE = { FLOOR: 0, WALL: 1, WOOD: 2, WOODBURN: 3, OIL: 4, SPIKE: 5, RUNE: 6 };
const ST = { INPUT: 0, RELEASE: 1, PICK: 2, COMPLETE: 3, DEFEAT: 4 };
const KIND_ICON = { mage: "🧙", imp: "👹", eye: "👁", boss: "🗿" };
// reject 原因碼 → 訊息(對齊 web/src/lib.rs reject_code)。
const REJECT_MSG = {
  1: "那裡沒有敵人(沒點到敵人格?)",
  2: "超出射程",
  3: "沒有視線(被牆擋住)",
  4: "不是相鄰敵人 — 推只能推緊鄰你的敵人(含斜角)",
  5: "它已經貼著你了",
  6: "目標是牆",
  7: "只能用在空地板",
  8: "超出邊界",
  9: "沒血瓶或血已滿",
  10: "那格不能走(有東西/原地)",
  11: "找不到路",
  12: "沒有可釋放的蓄力",
};

// ── Poki SDK 薄殼:有就用,沒有就 no-op(離線/本機開發)──
const Poki = (() => {
  const sdk = window.PokiSDK;
  const noop = () => {};
  if (!sdk) return { loadingFinished: noop, gameplayStart: noop, gameplayStop: noop };
  return {
    loadingFinished: () => sdk.gameLoadingFinished && sdk.gameLoadingFinished(),
    gameplayStart: () => sdk.gameplayStart && sdk.gameplayStart(),
    gameplayStop: () => sdk.gameplayStop && sdk.gameplayStop(),
  };
})();

let wasm, mem; // wasm exports + helper
let state = null;
let selSpell = null; // 目前選取的法術(target 需瞄準時)
let pending = null; // 觸控預覽:第一次點的瞄準格 {x,y};點同格才提交(防誤觸)
let pendingDrop = null; // 丟牌 UI:欄位滿時待換上的法術 code(等玩家選丟哪張;null = 非丟牌中)
let gameplayStarted = false;
let anim = null; // 進行中的動畫(events→重建);null = 靜態

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
  window.addEventListener("resize", () => { if (!anim) drawStatic(); });
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
    flash("✋ " + (REJECT_MSG[reason] || "那一手不行"));
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
const ACT_NAME = { 0: "待機", 1: "回血瓶", 2: "移動", 4: "釋放" };
// 把一手記成一行:動作→目標 [法師位置 / 切比雪夫距離]:結果(移動/傷害 或 拒絕原因)。
function logStep(act, x, y, spell, before, events, rejected, reason) {
  const name = act === 3 ? `施法:${SPELLS[spell].name}` : ACT_NAME[act] || `act${act}`;
  const m = before[0]; // 法師(id 0)行動前位置
  let head = name;
  if (act === 2 || act === 3) {
    const d = m ? Math.max(Math.abs(x - m.x), Math.abs(y - m.y)) : "?";
    head += `→(${x},${y}) [法(${m ? m.x : "?"},${m ? m.y : "?"}) d=${d}]`;
  }
  let out;
  if (rejected) {
    out = "✗ " + (REJECT_MSG[reason] || `拒絕#${reason}`);
  } else {
    const mv = events.filter((e) => e.t === "mv").map((e) => `#${e.id}(${e.fx},${e.fy})→(${e.tx},${e.ty})`);
    const dmg = events.filter((e) => e.t === "dmg").map((e) => `#${e.id}-${e.amt}`);
    const die = events.filter((e) => e.t === "die").map((e) => `☠#${e.id}`);
    out = "✓ " + [mv.length ? "移:" + mv.join(" ") : "", dmg.length ? "傷:" + dmg.join(" ") : "", die.join(" ")]
      .filter(Boolean).join(" | ") || "✓(無事件)";
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
    flash(selSpell !== null ? "再點一次確認施法 ✓(點別處改目標)" : "再點一次確認移動 ✓(點別處改目標)");
    drawStatic();
  }
}
cv.addEventListener("click", onCanvasTap);

// ── UI:動作列 + 順序鏈 + overlay ──
function render() {
  $("hud").textContent = ROOM_NAMES[state.room] || `房 ${state.room}`;
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
    d.textContent = (KIND_ICON[s.k] || "?") + (s.rel ? " 釋放" : "");
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
      flash(selSpell !== null ? `已選「${sp.name}」— 點目標瞄準、再點一次確認。` : "");
      renderBar();
      drawStatic();
    };
    bar.appendChild(b);
  }
  // 待機 / 回血瓶 / 重來
  const wait = mkBtn("⏳ 待機", playable, () => doStep(0));
  bar.appendChild(wait);
  const pot = mkBtn(`🧪 回血瓶 ×${state.potions}`, playable && state.potions > 0, () => doStep(1));
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
    show(`法術已滿(3)— 換上 ${np.icon} ${np.name}`, "丟一張舊的換上它。滿槽取捨,就是這一刻。");
    for (const oid of state.acquired) {
      const s = SPELLS[oid];
      const star = state.tiers && state.tiers[oid] >= 2 ? " ★" : "";
      btns.appendChild(mkBtn(`丟掉 ${s.icon} ${s.name}${star}`, true, () => {
        wasm.mr_drop(pendingDrop, oid); pendingDrop = null; wasm.mr_next_room(); selSpell = null; refresh(); render();
      }));
    }
    btns.appendChild(mkBtn("← 取消(改挑別張)", true, () => { pendingDrop = null; renderOverlay(); }));
  } else if (state.status === ST.PICK) {
    const offers = readJson(wasm.mr_offers());
    if (offers.length === 0) {
      show("房間清空 ✨", "可撿的都拿過了,直接前進。");
      btns.appendChild(mkBtn("繼續 →", true, () => { wasm.mr_next_room(); selSpell = null; refresh(); render(); }));
    } else {
      show("清關!三選一", `挑一張帶走(欄位 ${state.acquired.length}/3)。撿不同法術會長出不同打法。`);
      for (const code of offers) {
        const sp = SPELLS[code];
        const owned = state.acquired.includes(code);
        const b = mkBtn(`${sp.icon} ${owned ? "升級 " : ""}${sp.name}${owned ? " ★→★★" : ""}`, true, () => {
          if (wasm.mr_pick(code) === 1) { pendingDrop = code; renderOverlay(); } // 欄位滿 → 丟牌 UI
          else { wasm.mr_next_room(); selSpell = null; refresh(); render(); }     // 撿到/升級 → 下一房
        });
        btns.appendChild(b);
      }
    }
  } else if (state.status === ST.COMPLETE) {
    show("通關 🎉", "一場 run 完成 — 這就是『再來一局』。");
    btns.appendChild(mkBtn("再玩一次", true, () => { Poki.gameplayStop(); newRun((Math.random() * 0xffffffff) >>> 0); }));
  } else if (state.status === ST.DEFEAT) {
    show("你被擊倒了 💀", "重來一場 — roguelite 的『再來一局』。");
    btns.appendChild(mkBtn("重新開始", true, () => { Poki.gameplayStop(); newRun((Math.random() * 0xffffffff) >>> 0); }));
  }
}

// ── Canvas 繪製 ──
const COLORS = {
  [TILE.FLOOR]: "#3b3157", [TILE.WALL]: "#15101f", [TILE.WOOD]: "#5a4326",
  [TILE.WOODBURN]: "#7a4326", [TILE.OIL]: "#3a4a35", [TILE.SPIKE]: "#4a2f3a", [TILE.RUNE]: "#3a3a6a",
};

function fitCanvas() {
  const stage = $("stage").getBoundingClientRect();
  const cell = Math.max(28, Math.floor(Math.min(stage.width / state.w, stage.height / state.h)));
  cv.width = state.w * cell; cv.height = state.h * cell; cv._cell = cell;
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

boot().catch((err) => { document.body.innerHTML = "<p style='padding:20px'>載入失敗:" + err + "</p>"; });
