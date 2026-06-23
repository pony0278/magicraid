# 魔法基地突襲 — 專案進度待辦(master backlog)

> 用途:跨設計/技術/工程的單一追蹤表。詳細設計見《設計統整》《速度制規格》§10;技術選型見《技術棧定案》;產品閘門見《Demo 階段計畫》;sim 拆法見《B0 遷移清單》。
> 圖例:✅ 完成　🟡 進行中　⚪ 未開始　🔒 被前置卡住

---

## 0. 現況快照

| 軸 | 狀態 | 備註 |
|---|---|---|
| 設計定型(階段 A) | ✅ 近完成 | 速度制、§10 元素=機制、set-piece 詞彙都已拆 |
| 技術棧定案 | ✅ | Poki-first:一顆 Rust sim 核 + 輕量 web + AUDS;深度殼放棄 |
| JS 原型 `demo1.html` | ✅ 已驗證好玩 | **就是 Rust port 的規格**;已含 Demo 0 + 大半 Demo 1 玩法 |
| B0 遷移清單 | ✅ | engine 核 / view / 待拆 三類已分,確定性風險已列 |
| 階段 B(headless Rust sim) | ⚪ 未開工 | **當前焦點**,見下 |
| Poki-ready 客戶端 | ⚪ | 觸控/版面/字型/SDK 都要改 |
| Demo 0 / 1 產品閘門 | ⚪ | 玩法已具備,卡在「port 完 + Poki 化 + 真人測」 |

**關鍵 reframe**:原型已把 Demo 0 與 Demo 1 多數機制跑出來,所以這兩個 Demo 閘門**不是再設計玩法**,而是「把已驗證的玩法搬成確定性 Rust + Poki 可上架版,丟給 Poki 真人測」。近期 backlog 因此偏工程。

---

## 1. 關鍵路徑(一句話)

**B-0 拍決定 → port sim 核 → harness 驗 bit 一致 → WASM 接客戶端 → Poki 化 → 真人測(= Demo 0/1 閘門)→ 才往 Demo 2 蓋基地爬。**

中途沒有伺服器:整段到 Demo 3、多半連 Demo 4 都不用 Go/Nakama。

---

## 階段 B — Headless Rust sim（當前焦點）

### B-0　先拍的決定(coding blocker,來自《遷移清單》G 區)
- [ ] 時間單位:**1/6 整數累加**(建議)vs 自訂 Rational
- [ ] action 粒度:**`MoveTo{x,y}`**(A\* 留 sim、回放只存目的地,建議)vs `Move{dir}` 單步
- [ ] events 顆粒度:先「狀態正確 + 可回放」、動畫保真排後(建議)
- [ ] `validate` 角色:sim 端保留、非法回 `Rejected`(harness 跑 baseline 會送非法,建議保留)
- [ ] 行為計數器:B0 是否一起補(從 event 流數幾乎免費,建議補)
- [ ] `mageHurt`:留 step 區域變數,不進持久狀態(建議)

### B-1　port sim 核(按模組,逐顆對《遷移清單》A 區)
- [ ] `state.rs` — GameState / Entity / Channel / enums
- [ ] `config.rs` — CFG 常數 + ROOMS 資料
- [ ] `grid.rs` — inB/cheb/walkable/slamArea/heavyArea + **整數 LoS(D-2)**
- [ ] `time_chain.rs` — nextActor/advance/effSpeed/endMageAction + **1/6 整數時間(D-1)**
- [ ] `damage.rs` — dealDamage + **event emit 取代 pendingHits(C-4)**
- [ ] `terrain.rs` — lightFire/makeWet/igniteOil/fireTick + **複製 DFS push/pop 序(D-4)**
- [ ] `movement.rs` — doPush/doPull/shoveDir/moveMageTo/walkBrake + **整數成本 A\*(D-3)**
- [ ] `spells.rs` — spell 表(validate/cast/initiate)+ CHAIN_SCAN/chainCandidates
- [ ] `ai.rs` — stepToward/enemyAct(imp/eye/boss 過熱循環)
- [ ] `roguelite.rs` — hash32/mulberry32/gen_offers/apply_pick… + **種子外傳(D-5)**
- [ ] `events.rs` — Event enum + Status enum(《遷移清單》E 區)
- [ ] `lib.rs` — `step(state,action,seed)→{state,events,status}` + `project_chain` query
- [ ] **顯式排序總檢(D-6)**:禁 HashMap 迭代序;nextActor 三段 tiebreak 寫死

### B-2　harness + baseline 參考 agent（`harness/` crate）
- [x] native build(同一份 sim,不走 WASM;workspace member)
- [x] baseline 參考 agent(貪婪:逃砸擊/撿符文/推勾進危險格/過熱爆發/普攻/接近)
- [x] harness 跑批次種子(500 seeds),蒐集崩潰/卡死/不可解 → **0 崩潰、0 卡死**
- [x] 可解性:**房 0–5(含 boss)全部由 baseline 攻克**(每房至少一條解,真種子驗證)
      - boss 攻略:過熱窗口節奏穩打魔法彈(普攻 3 / 過熱雙倍 6),~5 發內倒、只挨 1–2 下砸擊。
      - 端到端:500 場 **約 89% 全程通關**(每房皆 500/500 被攻克)。

> **playtest 修正(Demo 0 閘門回饋)**:房間 2 教不出「推進陷阱」——根因是「推」原本是撿取道具、玩家
> 常常沒有。已把**「推」設為基礎包**(對齊 docs/01 §10 base kit),並**重畫房間 2 佈局**(尖刺夾住法師、
> 小鬼逼近 → 卡角度一推撞刺,對齊 docs/02 §7)。harness 驗證房 0–5 全可解、通關率 22%→89%。

> **playtest 修正(bolt 單調)**:WASM 真人試玩發現「每手只點魔法彈、硬通關」——根因是 bolt **射程 5**
> outrange 敵人接近、又 **2 發秒小鬼**,走位/地形/撿到的法術全無誘因。**射程砍 5→3**(符文眼 range 4 反而
> outrange 你 → 逼近戰/找掩體;無法再從安全角落狙一整房)。也試過降傷害 3→2 讓 bolt 變磨血,但**拖垮 boss**
> (過熱窗口傷害砍半,harness 房 3+ 歸零),故**只留射程**這單一變數。harness 仍 **89.4% 通關、房 0–5 全可解**;
> IMP_HP 維持 5(否則尖刺 6 不再秒殺、砸掉房 2 教學);config.rs 與 demo1.html 同步。回饋:思考密度上升
> (對齊 docs/01 §2「每回合有張力取捨」)。**殘留**:單獨小鬼仍可能在貼臉前被 bolt 解決——若要逼出近戰壓力
> 需降傷害 + boss 補償(更大改動);長期解是 Demo 1 的**附能 combo**,非繼續削數值。

### B-3　確定性 / 回放驗證
- [x] 同 `(seed, op 序列)` → bit 級相同最終狀態(harness replay,500 場 **0 不一致**)
- [x] event 流可重放(`step` 吐 events;view 之後靠它重建動畫)
- [ ] **JS↔Rust 對拍**:同輸入跑兩份,逐 event 比對抓漂移(待客戶端接上 WASM 後做)

> **🚪 階段 B 閘門**:同種子同操作序列 bit 一致 ✅ + harness 全房可解(0–5 含 boss ✅)。
> **閘門達成**。剩 JS↔Rust 對拍(需 WASM)留待階段 C 接客戶端時做。

---

## 階段 C — Poki-ready web 客戶端(表現層,不進 sim crate)

- [ ] `wasm-bindgen` 包 Rust sim,JS 呼叫 `step`
- [ ] **沿用/演化現有 DOM/Canvas 殼**,把「算結果」換成呼叫 WASM
- [ ] **events → 動畫重建**(取代 snap/frames/applyFrame 整組,《遷移清單》C-4)
- [ ] **events → `log` 在地化字串映射**(中文文案搬到殼端)
- [ ] 法術 metadata 表(icon/name/cost/desc/up/preview/noTarget)留殼端
- [ ] **觸控改造**:hover 預覽 → **tap 目的地預覽 → 再 tap 確認**;目標 ≥44–48px
- [ ] **16:9 縮放填滿** + 手機直/橫滿版(取代固定窄欄)
- [ ] **移除外部請求**:Google Fonts(Fredoka)→ 打包本地字型
- [ ] Poki SDK 事件接點:`gameLoadingFinished` / `gameplayStart`(首次輸入)/ `gameplayStop` / 廣告點(死亡重來、兩局之間)

> **🚪 Demo 0 閘門**:真人玩,房間 2 能自己悟出「推進陷阱 > 直接打」,且打完想再玩一次。
> **🚪 Demo 1 閘門**:自發連玩多場、不同場打法明顯不同、能說出自己走了什麼流派。

---

## Demo 1 收尾(玩法已在原型,補工程 + 儀表)

- [ ] **行為計數器 + run 總結**:取代 `buildSummary` 佔位 — 火燒擊殺 / 推進危險格擊殺 / 加速多打手數 / 鏈式串殺(§10 流派儀表板)
- [ ] 確認多地牢串接 + 死亡重洗佈局/選項 port 後保留(原型已有)
- [ ] 第二批法術按需加:火牆 / 震退 / 餘勢 / 閃現 / 生根 / 過載(維持「一次只加必要的」)
- [ ] 元素 hook 是否開啟(`fireEvaporatesWet`、濕格擋火蔓延)— playtest 後決定

---

## 階段 D+ — Demo 2 → 5(規劃中,後端逐步上)

- [ ] **Demo 2 蓋基地 + 守**:基地編輯器(門面 + 陷阱 + 召喚守軍),先打自己基地測。不做聯網/配對/經濟。
- [ ] **Demo 3 非同步突襲**:AUDS 存佈局 + 查詢配對 + 客戶端突襲 + 回放;Poki Accounts 身分。**第一個用後端的階段,但仍無伺服器權威**。
- [ ] **Demo 4 經濟/情緒迴圈**:損失/護盾、炫耀、段位、輕量養成;幾乎全調參數(資料驅動回本)。**約此處把 Poki 殼包成可上架版測市場**。
- [ ] **Demo 5 大地圖整合**:三節點串成一個世界;此處才靠手感拍「hub vs 策略層」。
- [ ] (觸發條件才做)Go/Nakama:防作弊 / 離開 Poki / 競技經濟成熟 → cgo 跑同一顆 Rust sim 驗證。

---

## 橫切:Poki 上架硬約束(隨客戶端持續勾)

- [ ] 初始下載 ≤ 8 MB
- [ ] 16:9 縮放填滿(參考 640×360 / 836×470 / 1031×580);手機滿版
- [ ] 自適應控制(手機/平板觸控、桌面鍵盤;平板強制手機控制)
- [ ] 🔴 預設封鎖外部請求 → 所有資產/字型打包進 build
- [ ] 無痕支援:localStorage 包 try/catch
- [ ] 存檔系統:有(AUDS + Accounts),或明確告知不存
- [ ] 只用 Poki 廣告、無內購、開 adblocker 仍能玩核心
- [ ] 先讀「Working With AI」政策(動 skill/loop 前)

---

## 風險登記(來自《技術棧定案》§7)

- 🔴 **AUDS 標「開發中、勿用於正式環境」**:用它快速做 Demo 3 驗好玩;上線前向 Poki 確認生產狀態;留「接 Nakama/Go 跑 Rust 驗證」後備。
- **AUDS 無伺服器權威 = 無防作弊**:Poki 階段「先驗好不好玩、不驗平不平衡」可接受。
- **觸控重設計 + 16:9 滿版**:客戶端必做,別拖到上架前才發現。
- **資產/字型打包**:Google Fonts 一定壞,先處理。
- **Poki「Working With AI」政策**:動 skill/loop 前先讀,影響流程。

---

## 建議下一步(動 code 前最後一關)

1. 先把 **B-0 六個決定**逐一拍掉(15 分鐘的事,但卡住整個 port)。
2. 從 **`state.rs` + `time_chain.rs`(含 1/6 整數時間)** 起手,立刻配一個最小 harness 重跑同種子驗 bit 一致——**讓確定性地基在第一步就被驗到**,再往 damage/terrain/spells 長。
3. 平行可先做的零工:把 `demo1.html` 的 Google Fonts 換本地字型(獨立、無依賴、隨時可做)。
