// JS↔Rust 對拍 — 比對器。
//
// 跑 `harness trace <seed>` 拿 Rust 逐手狀態,把同一串 op 重放進 demo1.html 的 JS sim,
// 逐手比對「可觀察狀態」(存活實體 id/kind/x/y/hp + fire + tiles + status + room + potions
// + acquired)。**不比 time**:JS float、Rust 整數 1/6,表示法本就不同;順序漂掉會反映在位置上。
//
// 用法:node diff/compare.mjs [seed] [--full]
//   預設只比到房 0 結束(第一個 nextroom)。--full 比整場(需 tier 同步,見 README)。

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { loadJsSim } from "./jsim.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, "..");
const seed = Number(process.argv[2] ?? 0) >>> 0;
const full = process.argv.includes("--full");

// ── 取 Rust trace ──
function rustTrace(seed) {
  const bin = join(ROOT, "target", "release", "harness");
  const out = execFileSync(bin, ["trace", String(seed)], { encoding: "utf8", maxBuffer: 64 << 20 });
  return JSON.parse(out);
}

const SELF_SPELLS = new Set(["haste"]);
function applyOp(api, op) {
  switch (op.op) {
    case "move": api.setSel("move"); api.cellClick(op.x, op.y); break;
    case "cast":
      if (SELF_SPELLS.has(op.spell)) api.selectAction(op.spell);
      else { api.setSel(op.spell); api.cellClick(op.x, op.y); }
      break;
    case "wait": case "release": api.selectAction("wait"); break;
    case "potion": api.selectAction("potion"); break;
    default: break; // init / nextroom 由主迴圈處理
  }
}

// 把一筆 step 正規化成可比對字串(只取 Rust trace 也有的欄位)。
function norm(s) {
  return JSON.stringify({
    status: s.status, room: s.room, potions: s.potions, acquired: s.acquired,
    ents: s.ents, fire: s.fire, tiles: s.tiles,
  });
}

function diffReport(rust, js) {
  const lines = [];
  if (rust.status !== js.status) lines.push(`  status: rust=${rust.status} js=${js.status}`);
  if (rust.room !== js.room) lines.push(`  room: rust=${rust.room} js=${js.room}`);
  if (rust.potions !== js.potions) lines.push(`  potions: rust=${rust.potions} js=${js.potions}`);
  if (JSON.stringify(rust.acquired) !== JSON.stringify(js.acquired))
    lines.push(`  acquired: rust=${JSON.stringify(rust.acquired)} js=${JSON.stringify(js.acquired)}`);
  const re = new Map(rust.ents.map((e) => [e.id, e]));
  const je = new Map(js.ents.map((e) => [e.id, e]));
  for (const id of new Set([...re.keys(), ...je.keys()])) {
    const a = re.get(id), b = je.get(id);
    if (!a) { lines.push(`  ent#${id}: rust 無 / js=${JSON.stringify(b)}`); continue; }
    if (!b) { lines.push(`  ent#${id}: rust=${JSON.stringify(a)} / js 無`); continue; }
    if (a.x !== b.x || a.y !== b.y || a.hp !== b.hp || a.k !== b.k)
      lines.push(`  ent#${id}(${a.k}): rust=(${a.x},${a.y})hp${a.hp} js=(${b.x},${b.y})hp${b.hp}`);
  }
  for (let y = 0; y < rust.fire.length; y++)
    for (let x = 0; x < rust.fire[y].length; x++)
      if (rust.fire[y][x] !== js.fire[y][x])
        lines.push(`  fire[${y}][${x}]: rust=${rust.fire[y][x]} js=${js.fire[y][x]}`);
  for (let y = 0; y < rust.tiles.length; y++)
    for (let x = 0; x < rust.tiles[y].length; x++)
      if (rust.tiles[y][x] !== js.tiles[y][x])
        lines.push(`  tile[${y}][${x}]: rust=${rust.tiles[y][x]} js=${js.tiles[y][x]}`);
  return lines.join("\n");
}

// ── 跑 ──
const trace = rustTrace(seed);
const api = loadJsSim();
api.newRun(seed);

let compared = 0, mismatch = null;
for (let i = 0; i < trace.steps.length; i++) {
  const step = trace.steps[i];
  if (step.op.op === "nextroom") {
    if (!full) { console.log(`(房 0 結束於第 ${i} 筆;--full 才繼續跨房)`); break; }
    api.setAcquired(step.acquired);
    api.loadRoomIdx(step.room);
  } else if (step.op.op !== "init") {
    applyOp(api, step.op);
  }
  const js = api.state();
  compared++;
  if (norm(step) !== norm(js)) {
    mismatch = { i, op: step.op, rust: step, js };
    break;
  }
}

if (mismatch) {
  console.log(`❌ seed ${seed}:第 ${mismatch.i} 手漂移(op=${JSON.stringify(mismatch.op)})`);
  console.log(diffReport(mismatch.rust, mismatch.js));
  process.exit(1);
} else {
  console.log(`✅ seed ${seed}:${compared} 手全對(JS 與 Rust 可觀察狀態逐手一致)`);
}
