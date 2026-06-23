# B0 遷移清單 — engine 核 vs view

> 來源:`demo1.html`(882 行,已驗證好玩 = 本次 Rust port 的規格)。
> 目標契約:`step(state, action) → {state, events, status}`,無 DOM、種子外傳、確定性(《技術棧定案》§8 / 《速度制規格》§3)。
> 行號對應 `demo1.html`,可逐條照搬。

---

## 切割總原則

`step()` 是純函式:吃 `(state, action, seed)`,吐 `(new_state, events[], status)`。據此倒推三條界線:

1. **凡是會改變 `G` 狀態的邏輯 → sim 核**(時間鏈、AI、傷害、火水、法術結算、撿取)。
2. **凡是 `log()` 字串、`pendingHits`、`snap()`/frames、DOM、輸入、動畫 → view**。sim 不吐字串、不吐動畫幀,只吐**結構化 events**,字串與動畫由殼從 events 重建。
3. **`SPELLS` / `loadRoom` / `checkClear` / 撿取流程是混的 → 必須拆**(下方 C 區逐項給拆法)。

---

## A. Sim 核(port 成 Rust,一份實作,確定性綁死)

| 子系統 | JS 符號 | 行號 | 備註 |
|---|---|---|---|
| **設定資料** | `CFG` | 156–174 | 直接搬成 Rust struct / const。`fireEvaporatesWet` 等 hook 旗標一起帶。 |
| **房間資料** | `ROOMS`(map 字串)+ 字元→tile/entity 映射 | 176–223 | 字串地圖是資料;**解析邏輯**(loadRoom 內的雙重迴圈)是 sim,見 C-1。 |
| **狀態容器** | `G = {w,h,tiles,fire,burnT,wet,entities,roomIdx}` | 224 | Rust `struct GameState`。`tiles/fire/burnT/wet` 都是 `Vec<Vec<_>>` grid。 |
| **實體模型** | entity 欄位:`id,kind,x,y,hp,maxhp,speed,time,hasteTurns,channel,stun,pendingSlam,exhausted,slam,path` | 218–221 | 用 `enum Kind` + struct。`channel`/`slam`/`path` 是 `Option<_>`。 |
| **格子謂詞** | `inB,cheb,entAt,blocksMove,blocksSight,walkable,aliveEnemies,effSpeed` | 241–252 | 全純函式。`effSpeed` 含 haste/wet/clamp,是時間鏈關鍵,見 D-1。 |
| **視線 LoS** | `los` | 256–261 | 用 `Math.round` → **改整數 LoS**,見 D-2。 |
| **砸擊範圍** | `slamArea` | 254 | 純形狀函式;輸出存進 `boss.slam`(狀態)。 |
| **傷害/死亡** | `dealDamage` | 264–275 | 含過熱雙倍、打斷 channel、擊殺續加速。**`pendingHits.push` 那行要抽掉改成 emit event**,見 C-4。 |
| **火/水** | `lightFire,makeWet,igniteOil,fireTick` | 278–300 | `igniteOil` 用 stack DFS + `seen`,順序影響擊殺先後 → 見 D-4。 |
| **連鎖閃電接口** | `CHAIN_SCAN,chainCandidates` | 427–433 | 已顯式排序(方向掃描序→id),照搬即確定性。 |
| **法術結算** | `SPELLS[*].validate / cast / initiate` | 307–414 | 只搬這三個函式 + `element/baseline/channel/target/maxTier`。`icon/name/desc/up/preview/noTarget` 是 view,見 C-3。 |
| **位移招式** | `doPush,shoveDir,doPull,moveMageTo,walkBrake` | 463–508 | 撞牆/尖刺/火結算、ZOC 煞車。`walkBrake` 讀 `mageHurt`(sim 旗標)。 |
| **A\* 尋路** | `findPath` | 510–531 | 對角成本 1.4 + 1e-9 epsilon → **改整數成本**,見 D-3。為回放保真**留在 sim**(理由見 C-2)。 |
| **烈焰術 AoE** | `heavyArea,resolveHeavy,releaseHeavy` | 532–557 | `heavyArea` 形狀函式;`releaseHeavy` 推進 mage time。 |
| **推進時間** | `endMageAction` | 559–566 | `time += 1/effSpeed`,見 D-1。 |
| **★時間鏈核心** | `nextActor,advance` | 569–608 | 整個迴圈。`advance` 裡的 auto-walk(587–598)是 sim(消耗 mage 的手),不是 view。 |
| **敵人 AI** | `stepToward,enemyAct` | 611–635 | 確定性。imp/eye/boss 三套行為 + 過熱循環。 |
| **撿取 PRNG** | `hash32,mulberry32,rngFor` | 809–811 | u32 wrapping 算術,Rust `wrapping_mul` 可 bit 級對齊。**RUN_SEED 外傳**,見 D-5。 |
| **撿取狀態變更** | `genOffers`(抽牌)、`choosePick`/`finishPick` 的**狀態變更部分**、`acquired,tier,SPELL_CAP,tierOf,opLog` | 199–205, 813–863 | 抽三張、升級、丟牌、push opLog 都是 sim。UI 呈現部分是 view,見 C-5。 |
| **順序鏈投影**(查詢) | `projectChain` | 638–650 | 純確定性函式,但**不是 step()**,是唯讀 query:`project_chain(state,n)→Vec<Slot>`。view 拿去畫鏈。 |

---

## B. View / 殼(**不 port**;每個殼各寫一套,只碰渲染/輸入/juice)

| 子系統 | JS 符號 | 行號 | 為什麼是 view |
|---|---|---|---|
| HTML / CSS | 全 `<style>` + `<body>` | 10–151 | 表現層。 |
| DOM 工具 | `$` | 207 | — |
| 主渲染 | `render` | 701–767 | 純畫面;讀狀態畫格子/HUD/preview。 |
| 精靈同步 | `syncSprites,spriteMap` | 768–788 | DOM 節點管理。 |
| 順序鏈繪製 | `paintChain` | 692–700 | 畫 `projectChain` 的輸出。 |
| **逐格播放引擎** | `frames,pendingHits,anim,animTimer,simRunning,wantOverlay,curActor,STEP_MS,snap,turn,applyFrame,present,skipAnim,revealOverlay` | 653–687 | 程式碼自註「純視覺,不碰決定論」。**整段丟掉**,改由 events 重建動畫,見 C-4。 |
| 文字日誌 | `log,logLines` | 789, 198 | 中文字串 = 在地化呈現。sim 改 emit event,殼再對應字串。 |
| 提示/回饋 | `feedback,hint`(及 ROOMS.hint) | 198, 50, 178… | 純文案。 |
| 輸入處理 | `selectAction,cellClick,sel,hover`、actions 綁定 | 436–462, 878 | hover/tap → 觸控改造在這層(《技術棧定案》§4.2)。 |
| 彈窗 UI | `showOverlay,showPick,showDrop` 的 **DOM 部分**、`pending` | 803–805, 823–858 | 三選一/勝敗畫面。 |
| 法術按鈕 | `buildSpellButtons` | 867–877 | 從 registry 生按鈕。 |
| 法術 metadata | `SPELLS[*].icon/name/cost/desc/up/preview/noTarget`、`SPELL_ORDER`、`ELNAME`、`ICON` | 各 SPELL 條目, 416, 206, 690 | 顯示用;`preview`/`noTarget` 是瞄準 UI 輔助。 |

---

## C. 混在一起的(**必須拆**,逐項給拆法)

**C-1 `loadRoom`(209–238)** — 一半 sim 一半 view。
- → sim:`init_room(room_idx, seed) → GameState`(176–230 的 map 解析、實體生成、`boss.slam` 初始化、idx0 時重置 acquired/tier/potions/seed/opLog)。
- → view:`$("roomName")/$("hint")/buildSpellButtons/render` 那幾行(231–237)。
- 注意:`RUN_SEED=(Date.now()^Math.random())`(233)**搬出 sim**,改由殼產生、外傳給 `init_room`。

**C-2 `findPath` 歸屬決策(已建議)** — 它同時被「點遠處走過去」(view 輸入)與「auto-walk」(sim 迴圈)用。
- 建議:**A\* 留在 sim**。理由:回放只存「目的地」這個 op,replay 時 sim 內部用同一份 A\* 重算路徑 + live `walkBrake` 重走,才能 bit 級一致。若把路徑算在 view 再餵 N 個單步,客戶端/harness/伺服器三套 A\* 會漂移 → 違反護欄 A。
- 契約:`action = MoveTo{x,y}`;sim 內 `step` 算一次路徑存進 `mage.path`,之後每個 mage 時間片消耗一步直到 ZOC 煞車或抵達。

**C-3 `SPELLS` registry 拆兩半**(307–414)。
- → sim 表:`{ element, baseline, channel, target, maxTier, validate, cast | initiate }`。
- → view 表:`{ icon, name, cost, desc, up, preview, noTarget }`。
- `target`(enemy/adjEnemy/cell/self)是**共用 schema**:sim 用來定 action 形狀,view 用來定瞄準 UI。
- 缺口:`SPELLS` 把 `Demo 0 基礎包` 的火球/烈焰術也含在內,但《§10》要它們改成撿取道具——port 時用 `baseline` 旗標控制即可,邏輯不動。

**C-4 `dealDamage`/`snap`/`pendingHits` → events**(這是最核心的一拆)。
- 現況:sim 同步改狀態,順手 `pendingHits.push(id)`(265)+ `snap()`(寫 frames + projectChain)餵動畫。
- 改法:`step()` 累積 `events: Vec<Event>`(`Damaged{id,amt,cause}`、`Died{id}`、`Moved{id,from,to}`、`FireSpread{..}`、`WoodIgnited{..}`、`Stunned{id}`、`HasteGained`、`ChannelInterrupted`、`SpellCast{id}`…)。
- 殼端:hit 閃光 = `Damaged` event;逐格播放 = 依 event 順序重放;`log()` 字串 = event → 在地化字串映射。**`snap/turn/present/applyFrame` 整組刪掉**。

**C-5 撿取流程拆**(813–863)。
- → sim:`gen_offers(seed,room_idx)`(抽三張)、`apply_pick(id)` / `apply_upgrade(id)` / `apply_drop(old,new)`(改 acquired/tier、push opLog)。
- → view:`showPick/showDrop` 的 DOM、按鈕、文案。

**C-6 `checkClear`(792–800)拆。**
- → sim:判定「boss 死 / 房清空 / 還有沒有下一房」→ 回傳 **status**(見 E)。
- → view:`showOverlay(...)` 那幾個呼叫。
- `buildSummary`(801)目前只列法術名,是佔位——《§10》要的是**行為計數器**(火燒擊殺/推進危險格擊殺/加速多打手數/鏈式串殺)。port 時順手在 sim 加這些 counter(從 event 流數最省事),見 G-4。

---

## D. Port 時必修的確定性風險(B0 的真正重點)

> JS float + JS 迭代序 = 漂移來源。一份實作不漂移的前提是這些全部處理掉。

**D-1 ★時間值用浮點累加 → 改有理/定點。** `time += 1/effSpeed`,而 `effSpeed ∈ {0.5, 1.0, 1.5}` → `1/speed ∈ {2, 1, 0.6667}`。`1/1.5` 在二進位非終止 → **加速一開就漂**。《速度制規格》§3 明寫「有理數累加」。
- 建議:時間以 **1/6 為單位的整數**累加(6 = {1,2,3} 的 LCD)。`1/0.5→12`、`1/1→6`、`1/1.5→4`。全整數,無 float,`nextActor` 排序變整數比較。

**D-2 `los` 用 `Math.round`(259)→ 改整數 LoS。** 座標皆 ≥0,JS `Math.round`(.5 進位向 +∞)與 Rust `.round()`(.5 遠離 0)在非負區**剛好一致**,但別賭邊界:改成整數 supercover / Bresenham,徹底去 float。並確認「轉角嚴格(角落算擋)」與 JS 行為一致。

**D-3 A\* 用浮點成本 1.4 + `1e-9` epsilon(526–527)→ 改整數成本。** 正交 10 / 對角 14,去掉 epsilon。**務必保留**:鄰居展開順序(dy,dx 各 −1..1)、open 選取 tiebreak(`f` 小優先 → `f` 同則 `g` 大優先 → 再同則插入序)。這三者決定路徑唯一性。

**D-4 `igniteOil` 的 stack DFS(280–287)順序敏感。** `st.pop()` + 物件 `seen` 的展開序決定多目標受傷/死亡先後,而死亡會觸發「加速★★ 續手」(273)→ 影響後續時間鏈。Rust 要**完全複製 push/pop 順序**(別用 HashSet 迭代)。`fireTick`(288–300)的 y,x 掃描序同理照搬。

**D-5 PRNG 種子外傳。** `RUN_SEED=(Date.now()^Math.random())`(233)是**非確定來源**,必須移出 sim,由殼產生後當參數傳入 `init_room`。`hash32`(FNV-1a 變體 + `Math.imul`)、`mulberry32` 用 Rust `u32::wrapping_*` 可 bit 對齊;`hash32` 吃字串 → 注意用 UTF-8 byte 還是 char code(JS 是 `charCodeAt` = UTF-16 code unit,Rust 要對應)。

**D-6 一切影響狀態的迭代「顯式排序」。** `entAt`(`Array.find`,243)、`aliveEnemies().some`、`nextActor` tiebreak(571)、`chainCandidates`(已 sort)。Rust 禁用 `HashMap` 迭代序;實體存 `Vec`(id 序)或 `BTreeMap`。`nextActor` 的 `time 升 → mage 優先 → id 升` 三段 tiebreak 要寫死。

---

## E. `step → {state, events, status}` 契約對照(view 旗標如何收斂)

現在散在 `waiting/anim/simRunning/wantOverlay` 的控制流,headless 收成一個 **status** 回傳:

| JS 現況 | headless status | 觸發點 |
|---|---|---|
| `waiting=true` 等玩家輸入 | `AwaitingInput` | 輪到 mage 且無 auto-path、無 channel.ready |
| `mage.channel.ready`(585) | `AwaitingRelease` | 前搖撐過、停在釋放手 |
| `checkClear` 房清空(797) | `RoomClear` → `PickOffered` | 非 boss 且敵人清空且有下一房 |
| `checkClear` boss 死(795) | `RunComplete` | boss.hp≤0 |
| `defeat`(802) | `Defeat` | mage.hp≤0 |

`step()` 內部跑完時間鏈(所有敵人手 + auto-walk),**直到控制權回到玩家或進入終局**才回傳——這正取代了 `advance()` 的 `waiting=true;render();return`。

---

## F. 建議 Rust 模組切法(對齊上表)

```
sim/
  state.rs      A: GameState / Entity / Channel / enums
  config.rs     A: CFG 常數、ROOMS 資料
  grid.rs       A: inB/cheb/walkable/los(整數)/slamArea/heavyArea
  time_chain.rs A: nextActor/advance/effSpeed/endMageAction(★時間=1/6 整數)
  damage.rs     A: dealDamage + Event emit(取代 pendingHits)
  terrain.rs    A: lightFire/makeWet/igniteOil/fireTick
  spells.rs     A: spell 表(validate/cast/initiate)+ chain/CHAIN_SCAN
  movement.rs   A: doPush/doPull/shoveDir/moveMageTo/walkBrake/findPath(整數)
  ai.rs         A: stepToward/enemyAct
  roguelite.rs  A: hash32/mulberry32/gen_offers/apply_pick…(種子外傳)
  events.rs     events enum + status enum(E 區)
  lib.rs        step(state,action,seed)→{state,events,status} + project_chain query
```
view(殼,不在 sim crate 內):渲染、輸入、動畫、彈窗、法術 metadata、`log` 字串映射。

---

## G. 動 code 前要先拍的決定

1. **時間單位**:1/6 整數(建議)vs 自訂 Rational 型?整數最省、夠用。
2. **action 粒度**:`MoveTo{x,y}`(路徑算在 sim,回放只存目的地,建議)vs `Move{dir}` 單步(view 拆,但 ZOC 重現要重算)。見 C-2。
3. **events 顆粒度**:要細到能 100% 重建現有動畫(逐 hit、逐 spread),還是先粗、動畫後補?B0 建議先把「狀態正確 + 可回放」做完,動畫保真度排後。
4. **行為計數器**:`buildSummary` 的 placeholder 是否在 B0 一起補成真 counter(《§10》流派儀表板)?建議補,因為從 event 流數幾乎免費。
5. **`validate` 在 headless 的角色**:非法 action 是 sim 回 `Err`/`Rejected` event,還是契約保證 view 只送合法 action?harness 跑 baseline agent 時會送非法 → 建議 sim 端 validate 要在,回 `Rejected`。
6. **`mageHurt`**:現在是 walk 期間「被打過就煞車」的旗標(450, 504),要併進 entity 狀態還是 step 區域變數?它只在一次 step 的 auto-walk 內有效 → step 區域變數即可,別進持久狀態。
