// JS↔Rust 對拍 — 比對器。
//
// 跑 `harness trace <seed>` 拿 Rust 逐手狀態,把同一串 op 重放進 demo1.html 的 JS sim,
// 逐手比對「可觀察狀態」(存活實體 id/kind/x/y/hp + fire + tiles + status + room + potions
// + acquired)。**不比 time**:JS float、Rust 整數 1/6,表示法本就不同;順序漂掉會反映在位置上。
//
// 用法:
//   node diff/compare.mjs [seed] [--full]   單一種子(預設 seed 0;不加 --full 只比到房 0)
//   node diff/compare.mjs --all [N] [--full] 批次 seed 0..N(預設 N=50);任一漂移→非零退出(CI 閘門)

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { loadJsSim } from "./jsim.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, "..");
const argv = process.argv.slice(2);
const full = argv.includes("--full") || argv.includes("--all"); // --all 一律比整場
const all = argv.includes("--all");
const numArg = argv.find((a) => /^\d+$/.test(a));

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

// 對拍一個種子,回 {compared, mismatch, stoppedRoom0}。共用一個 JS sim 實例(newRun 會重置)。
function compareSeed(api, seed, full) {
  const trace = rustTrace(seed);
  api.newRun(seed);
  let compared = 0;
  for (let i = 0; i < trace.steps.length; i++) {
    const step = trace.steps[i];
    if (step.op.op === "nextroom") {
      if (!full) return { compared, mismatch: null, stoppedAt: i };
      api.setAcquired(step.acquired);
      api.setTiers(step.tiers);
      api.loadRoomIdx(step.room);
    } else if (step.op.op !== "init") {
      applyOp(api, step.op);
    }
    compared++;
    if (norm(step) !== norm(api.state())) return { compared, mismatch: { i, op: step.op, rust: step, js: api.state() } };
  }
  return { compared, mismatch: null };
}

// ── 跑 ──
const api = loadJsSim();

if (all) {
  const N = numArg ? Number(numArg) : 50;
  let pass = 0, firstFail = null;
  for (let s = 0; s <= N; s++) {
    const r = compareSeed(api, s, true);
    if (r.mismatch) { firstFail = { seed: s, ...r.mismatch }; break; }
    pass++;
  }
  if (firstFail) {
    console.log(`❌ seed ${firstFail.seed}:第 ${firstFail.i} 手漂移(op=${JSON.stringify(firstFail.op)})`);
    console.log(diffReport(firstFail.rust, firstFail.js));
    console.log(`(先前 ${pass} 個種子全對)`);
    process.exit(1);
  }
  console.log(`✅ 對拍通過:seed 0..${N} 整場逐手一致(${pass} 個種子)`);
} else {
  const seed = numArg ? Number(numArg) : 0;
  const r = compareSeed(api, seed, full);
  if (r.mismatch) {
    console.log(`❌ seed ${seed}:第 ${r.mismatch.i} 手漂移(op=${JSON.stringify(r.mismatch.op)})`);
    console.log(diffReport(r.mismatch.rust, r.mismatch.js));
    process.exit(1);
  }
  if (r.stoppedAt != null) console.log(`(房 0 結束於第 ${r.stoppedAt} 筆;--full 才繼續跨房)`);
  console.log(`✅ seed ${seed}:${r.compared} 手全對(JS 與 Rust 可觀察狀態逐手一致)`);
}
