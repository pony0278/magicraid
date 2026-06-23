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
  firstInput();
  const before = entMap(state); // 動畫起點(這手之前的位置/血量)
  wasm.mr_step(act, x, y, spell);
  const rejected = wasm.mr_rejected() === 1;
  selSpell = null;
  const events = readEvents();
  refresh(); // state = 新狀態
  if (rejected) {
    const why = REJECT_MSG[wasm.mr_reject_reason()] || "那一手不行";
    flash("✋ " + why);
    render();
    return;
  }
  $("feedback").textContent = "";
  startAnim(before, entMap(state), events);
}

// 開始一段動畫:實體 before→after 補間 + 命中閃光 + 死亡淡出 + 飄字。
function startAnim(before, after, events) {
  const dmg = {};
  for (const e of events) if (e.t === "dmg") dmg[e.id] = (dmg[e.id] || 0) + e.amt;
  anim = {
    t0: performance.now(),
    dur: 260,
    before,
    after,
    dmgIds: new Set(Object.keys(dmg).map(Number)),
    floats: Object.entries(dmg).map(([id, amt]) => ({ id: +id, amt })),
  };
  requestAnimationFrame(tick);
}

function finishAnim() {
  anim = null;
  render(); // 落定到最終狀態 + 更新面板
}

function tick(now) {
  if (!anim) return;
  const p = Math.min(1, (now - anim.t0) / anim.dur);
  drawFrame(p);
  if (p < 1) requestAnimationFrame(tick);
  else finishAnim();
}

function flash(msg) { $("feedback").textContent = msg; }

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

function onCanvasTap(ev) {
  ev.preventDefault();
  if (!state) return;
  if (anim) { finishAnim(); return; } // 動畫中點一下 = 跳過
  if (state.status === ST.RELEASE) { doStep(4); return; } // 釋放蓄力:任意點擊
  if (state.status !== ST.INPUT) return;
  const { x, y } = cellFromEvent(ev);
  if (x < 0 || y < 0 || x >= state.w || y >= state.h) return;
  if (selSpell !== null) {
    doStep(3, x, y, selSpell); // 施法瞄準
  } else {
    doStep(2, x, y, 0); // 移動
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
      flash(selSpell !== null ? `已選「${sp.name}」— 點目標。` : "");
      renderBar();
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

  if (state.status === ST.PICK) {
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
          wasm.mr_pick(code); wasm.mr_next_room(); selSpell = null; refresh(); render();
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

// 地形/火/預告(永遠以最終狀態繪製)。
function drawBackground(cell) {
  for (let y = 0; y < state.h; y++) for (let x = 0; x < state.w; x++) {
    const t = state.tiles[y][x];
    ctx.fillStyle = COLORS[t] || "#3b3157";
    ctx.fillRect(x * cell, y * cell, cell - 1, cell - 1);
    if (t === TILE.SPIKE) { ctx.fillStyle = "#c45"; glyph("▲", x, y, cell, 0.5); }
    if (t === TILE.RUNE) { glyph("⚡", x, y, cell, 0.6); }
    if (t === TILE.WOOD || t === TILE.WOODBURN) { ctx.strokeStyle = "#0004"; ctx.strokeRect(x*cell+2, y*cell+2, cell-5, cell-5); }
    if (state.fire[y][x] > 0) { ctx.fillStyle = "rgba(255,122,60,.55)"; ctx.fillRect(x*cell, y*cell, cell-1, cell-1); glyph("🔥", x, y, cell, 0.6); }
  }
  for (const [cx, cy] of state.slamCells) {
    ctx.fillStyle = "rgba(226,87,76,.4)";
    ctx.fillRect(cx * cell, cy * cell, cell - 1, cell - 1);
    ctx.strokeStyle = "#e2574c"; ctx.lineWidth = 2;
    ctx.strokeRect(cx*cell+1, cy*cell+1, cell-3, cell-3);
  }
}

// 畫一隻實體(x,y 為格座標,可為小數以做補間;flash 0–1 命中閃光;alpha 死亡淡出)。
function drawEntity(e, cell, alpha, flash) {
  ctx.globalAlpha = alpha;
  if (flash > 0) {
    ctx.fillStyle = `rgba(255,90,80,${0.6 * flash})`;
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
}

// 動畫幀:實體 before→after 補間;命中閃光;死亡(在 before 有、after 無)淡出;傷害飄字。
function drawFrame(p) {
  if (!state) return;
  fitCanvas();
  const cell = cv._cell;
  const ease = 1 - (1 - p) * (1 - p);
  ctx.clearRect(0, 0, cv.width, cv.height);
  drawBackground(cell);
  const ids = new Set([...Object.keys(anim.before), ...Object.keys(anim.after)].map(Number));
  for (const id of ids) {
    const b = anim.before[id], a = anim.after[id];
    const flash = anim.dmgIds.has(id) ? 1 - p : 0;
    if (b && a) {
      drawEntity({ ...a, x: b.x + (a.x - b.x) * ease, y: b.y + (a.y - b.y) * ease }, cell, 1, flash);
    } else if (b) {
      drawEntity({ ...b, hp: 0 }, cell, 1 - p, flash); // 死亡淡出
    } else if (a) {
      drawEntity(a, cell, p, 0); // 新生(罕見)
    }
  }
  // 傷害飄字(往上飄、淡出)。
  ctx.textAlign = "center"; ctx.textBaseline = "middle";
  ctx.font = `${Math.floor(cell * 0.42)}px system-ui, sans-serif`;
  for (const f of anim.floats) {
    const src = anim.after[f.id] || anim.before[f.id];
    if (!src) continue;
    ctx.globalAlpha = 1 - p;
    ctx.fillStyle = "#ffd0c0";
    ctx.fillText(`-${f.amt}`, src.x * cell + cell / 2, src.y * cell + cell / 2 - p * cell * 0.9);
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
