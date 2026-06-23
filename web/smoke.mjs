// Node 煙霧測試:透過 WASM 邊界(同 JS 殼的呼叫)驅動一場,驗證 step/pick/room 流程。
import { readFileSync } from "fs";
const bytes = readFileSync(new URL("./www/magicraid_web.wasm", import.meta.url));
const { instance } = await WebAssembly.instantiate(bytes, {});
const ex = instance.exports;
const readJson = (ptr) => JSON.parse(new TextDecoder().decode(
  new Uint8Array(ex.memory.buffer, ptr, ex.mr_buf_len())));
const render = () => readJson(ex.mr_render());

function play(seed, budget = 4000) {
  ex.mr_new(seed);
  let picks = 0, steps = 0;
  for (; steps < budget; ) {
    const st = render();
    if (st.status === 3) return { res: "complete", picks, steps };
    if (st.status === 4) return { res: "defeat", picks, steps };
    if (st.status === 2) { // PickOffered
      const offers = readJson(ex.mr_offers());
      if (offers.length) { ex.mr_pick(offers[0]); picks++; }
      ex.mr_next_room();
      continue;
    }
    if (st.status === 1) { ex.mr_step(4, 0, 0, 0); steps++; continue; } // release
    // AwaitingInput: 簡單策略 — 射程內轟最近敵,否則朝它走一步。
    const mage = st.ents.find((e) => e.k === "mage");
    const foes = st.ents.filter((e) => e.k !== "mage");
    if (!foes.length) { ex.mr_step(0, 0, 0, 0); steps++; continue; }
    foes.sort((a, b) => Math.max(Math.abs(a.x-mage.x),Math.abs(a.y-mage.y)) - Math.max(Math.abs(b.x-mage.x),Math.abs(b.y-mage.y)));
    const e = foes[0];
    const d = Math.max(Math.abs(e.x-mage.x), Math.abs(e.y-mage.y));
    if (d <= 5) ex.mr_step(3, e.x, e.y, 0); // bolt
    else ex.mr_step(2, mage.x + Math.sign(e.x-mage.x), mage.y + Math.sign(e.y-mage.y), 0); // step toward
    steps++;
  }
  return { res: "timeout", picks, steps };
}

let reachedPick = 0, completes = 0;
for (let s = 0; s < 30; s++) {
  const r = play(s);
  if (r.picks > 0) reachedPick++;
  if (r.res === "complete") completes++;
}
console.log(`30 場:抵達三選一 ${reachedPick} 場,通關 ${completes} 場`);
console.log(reachedPick > 0 ? "✓ WASM 邊界 step/pick/room 流程正常" : "✗ 未觸發三選一");
process.exit(reachedPick > 0 ? 0 : 1);
