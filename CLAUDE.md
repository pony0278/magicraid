# CLAUDE.md — 魔法基地突襲(Magic Raid)

專案總覽,給後續 session 快速進入用。詳細設計一律以 `docs/` 為準。

## 這是什麼

可愛、低暴力、節奏快的魔法冒險休閒遊戲:經營法師聖所、布置防禦、突襲別人的聖所。
**Poki-first**:先用輕量 web 版驗證市場,再用同一顆核心長成深度版。

## 倉庫結構

```
docs/        所有設計／技術定案文件(見 docs/README.md 的閱讀順序與關係圖)
prototype/   demo1.html — 已驗證好玩的 JS 原型 = Rust port 的規格基準
sim/         確定性 Rust sim 核(headless,WASM + native 共用一份)← 開發中
```

## 文件地圖(真相來源)

- **玩法**:`docs/01-design-overview.md`(§10 元素=機制)
- **戰鬥行動經濟**:`docs/02-speed-combat-spec.md`(速度/時間鏈,取代 §2 行動點)
- **收集/組裝**:`docs/03-wand-parts-spec.md`(4 槽平排法杖)
- **技術選型**:`docs/04-tech-stack.md`(Rust sim + 輕量 web + Poki AUDS)
- **port 施工清單**:`docs/05-b0-migration.md`(demo1.html → sim 核 vs view + 確定性風險)
- **進度**:`docs/06-demo-roadmap.md`(閘門)、`docs/07-master-backlog.md`(當前焦點)

## 不可違反的核心紀律

1. **一個 sim,不能兩套**:sim 核唯一一份 Rust 實作,客戶端(WASM)/harness(native)/未來伺服器(cgo)全跑同一份。表現層(渲染/輸入/動畫)才允許多套。
2. **確定性綁死**:`step(state, action, seed) → {state, events, status}` 純函式;無 DOM、無計時器、無 `Math.random`/`Date.now`(種子外傳)。同種子 + 同操作 = bit 級相同回放。
3. **確定性風險(B0 必修)**:時間用 **1/6 整數**累加(不用 float)、整數 LoS / 整數成本 A\*、PRNG 種子外傳、**一切影響狀態的迭代顯式排序**(禁 HashMap 迭代序)。
4. **port 不是重寫**:玩法規格 = `prototype/demo1.html` 的行為,只是搬語言。
5. **每一階只加一樣、過關才往上爬**;不要兩個殼並行開發。

## sim 核模組切法(對齊 docs/05-b0-migration.md §F)

```
sim/src/
  state.rs      GameState / Entity / Kind / Channel / enums
  config.rs     CFG 常數、ROOMS 資料、1/6 整數時間換算
  time_chain.rs next_actor / advance / eff_speed / end_mage_action(時間=1/6 整數)
  events.rs     Event enum + Status enum
  lib.rs        step(...) + project_chain query
  (待補) grid.rs / damage.rs / terrain.rs / spells.rs / movement.rs / ai.rs / roguelite.rs
```

## 確定性關鍵數字(來自 prototype/demo1.html CFG + 速度規格)

- 時間單位 = **1/6**(6 = 速度 {0.5,1,1.5} 的 LCD)。`time += 6/速度`:速度 0.5→+12、1.0→+6、1.5→+4、2.0→+3,全整數。
- 速度夾在 **[0.5, 2.0]**;基礎 1.0,加速 1.5(`hasteSpeed`)。
- nextActor tiebreak(寫死):**time 升 → mage 優先 → id 升**。

## 開發慣例

- Rust sim 在 `sim/`:`cargo test` 必須過,**determinism 測試是地基**(同輸入兩次跑 bit 一致)。
- 改 sim 邏輯前先想:會不會引入非確定性?(float / 迭代序 / 外部隨機)
- 設計數字多為**佔位**,playtest 才轉 → 集中放 `config.rs`,別散寫死。

## 當前焦點

階段 B(headless Rust sim)。已起 `state.rs` + `time_chain.rs` 骨架並驗時間鏈確定性;
接著按 `docs/05-b0-migration.md` A 區逐模組往 damage / terrain / spells 長。
