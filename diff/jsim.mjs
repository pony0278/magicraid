// JS↔Rust 對拍 — demo1.html 的 JS sim 無頭載入器(Node)。
//
// 做法:抽出 demo1.html 的 <script>、剝掉尾端自動執行(事件綁定 + loadRoom(0)),
// 在一個 vm context 裡跑(配 DOM shim),再 stub 掉 view 函式(render/present/…),
// 最後吐一個 __api:newRun / 套用一手 / 讀可觀察狀態。玩法真相在 demo1.html,這層只驅動。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import vm from "node:vm";

const HERE = dirname(fileURLToPath(import.meta.url));
const HTML = join(HERE, "..", "prototype", "demo1.html");

// ── 萬用 DOM 元素 shim:任何屬性讀取回 no-op 函式(支援鏈式)/可寫;style·classList 特例。 ──
function elem() {
  const t = { style: {}, dataset: {} };
  return new Proxy(t, {
    get(o, k) {
      if (k in o) return o[k];
      if (k === "classList") return { add() {}, remove() {}, contains() { return false; }, toggle() {} };
      if (["offsetWidth", "offsetHeight", "width", "height", "scrollHeight", "clientWidth", "clientHeight"].includes(k)) return 0;
      if (["textContent", "innerHTML", "value", "className", "id"].includes(k)) return "";
      if (k === "getContext") return () => ctx2d();
      if (k === "closest" || k === "querySelector") return () => elem();
      if (k === "querySelectorAll") return () => [];
      if (typeof k === "symbol") return undefined;
      return () => elem(); // 其餘當方法,回新元素
    },
    set(o, k, v) { o[k] = v; return true; },
  });
}
function ctx2d() { return new Proxy({}, { get: () => () => {}, set: () => true }); }

export function loadJsSim() {
  const html = readFileSync(HTML, "utf8");
  const m = html.match(/<script>([\s\S]*?)<\/script>/);
  if (!m) throw new Error("demo1.html 找不到 <script>");
  let src = m[1];

  // 剝掉尾端自動執行:`$("actions").addEventListener(...)` 與 `loadRoom(0)`(改由我們控制)。
  src = src.replace(/\$\("actions"\)\.addEventListener[\s\S]*?\}\);\s*$/m, "");
  src = src.replace(/\bloadRoom\(0\);\s*$/m, "");

  // 附加:停用 view 層 + 匯出驅動 API(在同一詞法作用域,拿得到所有 const/let)。
  src += `
;(function(){ render=()=>{}; present=()=>{}; paintChain=()=>{}; syncSprites=()=>{};
  revealOverlay=()=>{}; buildSpellButtons=()=>{}; showOverlay=()=>{}; showPick=()=>{}; })();
globalThis.__api = {
  newRun(seed){ loadRoom(0); RUN_SEED=(seed>>>0); acquired.length=0; for(const k in tier)delete tier[k]; potions=CFG.potion.count; },
  loadRoomIdx(idx){ loadRoom(idx); },
  setAcquired(list){ acquired.length=0; for(const id of list) acquired.push(id); },
  setTiers(map){ for(const k in tier)delete tier[k]; for(const id in map) tier[id]=map[id]; },
  setSel(s){ sel=s; },
  selectAction(a){ selectAction(a); },
  cellClick(x,y){ cellClick(x,y); },
  state(){
    const ents = G.entities.filter(e=>e.hp>0).map(e=>({id:e.id,k:e.kind,x:e.x,y:e.y,hp:e.hp}))
      .sort((a,b)=>a.id-b.id);
    let status;
    if(mage.hp<=0) status="defeat";
    else if(aliveEnemies().length===0){
      const boss=G.entities.find(e=>e.kind==="boss");
      status = (boss||G.roomIdx>=ROOMS.length-1) ? "complete" : "pick";
    }
    else if(mage.channel&&mage.channel.ready) status="release";
    else status="input";
    return { status, room:G.roomIdx, potions, acquired:acquired.slice(),
      ents, fire:G.fire.map(r=>r.slice()), tiles:G.tiles.map(r=>r.slice()) };
  },
};`;

  const sandbox = {
    document: { getElementById: () => elem(), createElement: () => elem(), querySelector: () => elem(), querySelectorAll: () => [], addEventListener() {} },
    window: { addEventListener() {}, requestAnimationFrame: () => 0, devicePixelRatio: 1 },
    requestAnimationFrame: () => 0,
    setTimeout: () => 0, clearTimeout: () => {}, setInterval: () => 0, clearInterval: () => {},
    console, Math, JSON, Date, Object, Array, Number, String, Boolean,
  };
  sandbox.globalThis = sandbox;
  vm.createContext(sandbox);
  vm.runInContext(src, sandbox, { filename: "demo1-sim.js" });
  return sandbox.__api;
}
