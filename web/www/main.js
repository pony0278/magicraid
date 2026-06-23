// 魔法基地突襲 — web 殼(表現層)。玩法走 Rust sim(WASM);此檔只做渲染/輸入/動畫接點。
// view metadata(法術 icon/名稱/瞄準型別)留在這層(B0 §C-3)。

// ── 法術表(view metadata;code 對齊 sim SPELL_ORDER)──
const SPELLS = [
  { code: 0, id: "bolt", name: "魔法彈", icon: "✨", target: "enemy" },
  { code: 1, id: "push", name: "推", icon: "👐", target: "adjEnemy" },
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

const $ = (id) => document.getElementById(id);
const cv = $("cv"), ctx = cv.getContext("2d");

// ── 讀 wasm 記憶體中的 JSON(每次重抓 buffer,避免成長後 detach)──
function readJson(ptr) {
  const len = wasm.mr_buf_len();
  const bytes = new Uint8Array(wasm.memory.buffer, ptr, len);
  return JSON.parse(new TextDecoder().decode(bytes));
}
function refresh() { state = readJson(wasm.mr_render()); }

async function boot() {
  // 用 arrayBuffer 路徑:不依賴伺服器把 .wasm 標成 application/wasm。
  const buf = await (await fetch("magicraid_web.wasm")).arrayBuffer();
  const { instance } = await WebAssembly.instantiate(buf, {});
  wasm = instance.exports;
  Poki.loadingFinished();
  newRun((Math.random() * 0xffffffff) >>> 0);
  window.addEventListener("resize", draw);
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
  firstInput();
  wasm.mr_step(act, x, y, spell);
  const rejected = wasm.mr_rejected() === 1;
  selSpell = null;
  refresh();
  if (rejected) flash("✋ 那一手不行(超出射程/視線/沒目標)。");
  else $("feedback").textContent = "";
  render();
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
  draw();
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
  // 可用法術 = 基礎(bolt)+ 已撿
  const codes = [0, ...state.acquired];
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

function draw() {
  if (!state) return;
  fitCanvas();
  const cell = cv._cell;
  ctx.clearRect(0, 0, cv.width, cv.height);
  // tiles
  for (let y = 0; y < state.h; y++) for (let x = 0; x < state.w; x++) {
    const t = state.tiles[y][x];
    ctx.fillStyle = COLORS[t] || "#3b3157";
    ctx.fillRect(x * cell, y * cell, cell - 1, cell - 1);
    if (t === TILE.SPIKE) { ctx.fillStyle = "#c45"; glyph("▲", x, y, cell, 0.5); }
    if (t === TILE.RUNE) { glyph("⚡", x, y, cell, 0.6); }
    if (t === TILE.WOOD || t === TILE.WOODBURN) { ctx.strokeStyle = "#0004"; ctx.strokeRect(x*cell+2, y*cell+2, cell-5, cell-5); }
    if (state.fire[y][x] > 0) { ctx.fillStyle = "rgba(255,122,60,.55)"; ctx.fillRect(x*cell, y*cell, cell-1, cell-1); glyph("🔥", x, y, cell, 0.6); }
  }
  // boss 砸擊預告
  for (const [cx, cy] of state.slamCells) {
    ctx.fillStyle = "rgba(226,87,76,.4)";
    ctx.fillRect(cx * cell, cy * cell, cell - 1, cell - 1);
    ctx.strokeStyle = "#e2574c"; ctx.lineWidth = 2;
    ctx.strokeRect(cx*cell+1, cy*cell+1, cell-3, cell-3);
  }
  // 選法術時提示可點(簡單:整盤可點;sim 會回絕非法)
  // entities
  for (const e of state.ents) {
    glyph(KIND_ICON[e.k] || "?", e.x, e.y, cell, 0.62);
    if (e.k !== "mage" || true) drawHp(e, cell);
    if (e.ch) { ctx.strokeStyle = e.ready ? "#f5c84a" : "#74e0d4"; ctx.lineWidth = 2;
      ctx.strokeRect(e.x*cell+2, e.y*cell+2, cell-5, cell-5); }
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
