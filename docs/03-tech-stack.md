# 魔法基地突襲 — 技術棧定案(Poki-first)

> 用途:把這次討論收斂的技術選型固定下來——回答「客戶端 / sim 核 / 後端各用什麼、每塊的角色與界線、什麼時候才升級」。
> 設計內容見《設計統整》《速度制規格》;進度標準見《Demo 階段計畫》。**本文件只管技術棧,不重述玩法。**
> Poki 相關事實於 2026-06 查證 Poki 開發者文件(`sdk.poki.com`),AUDS/Netlib 屬會變動的早期 API,正式上線前需再確認。

---

## 0. 一句話定位

Poki-first 的輕量 web 遊戲。**一顆確定性 Rust sim 核**,客戶端盡量輕,後端能不做就不做。深度殼(Steam/手機原生)**已放棄**——最重要的是在 Poki 上順跑,且可能有人用手機瀏覽器玩。

---

## 1. 心智模型:三層,只有一層被確定性綁死

把「引擎/客戶端/後端」拆成三層,糾結就解開了:

1. **確定性 sim 核** — `step(state, action) → {state, events}`。受《速度制規格》§3 確定性合約綁死,**必須一份實作**。它的語言被「客戶端 + 單一來源 + 確定性」夾住。
2. **表現層(渲染/輸入/juice)** — **不被確定性綁**,所以**可以有不只一套**;這是《設計統整》§1「一核兩殼」裡唯一允許有兩套的東西。
3. **後端服務 / 儲存** — 跟確定性無關,正常後端,愛用什麼用什麼。

真正要拍的只有「第 1 層的語言」,其餘都跟著它走。

---

## 2. 鐵律:一個 sim,不能兩套

> **確定性 = 回放 + 非同步 PvP + harness 的地基。兩套 sim 實作(例:客戶端一套、伺服器一套)一定會漂移(float 順序、整數溢位、迭代順序、RNG)→ 回放對不上 → 突襲與 harness 同時垮。**

由此推出兩條不可違反的護欄:

- **護欄 A**:sim 核是**唯一一份 Rust 實作**,客戶端(WASM)、harness(native)、未來伺服器(cgo)全跑同一份。
- **護欄 B**:**外部工具/樣板只碰外圍(儲存、渲染、後端),永不擁有 sim 邏輯。** 例:Godot grid 套件、Pixi、任何 GDScript/JS tactics 模板,只拿來做畫面,戰鬥規則一律走 Rust 核。

---

## 3. 定案技術棧(總表)

| 層 | 用什麼 | 為什麼 | 界線 / 何時升級 |
|---|---|---|---|
| **sim 核** | **Rust → WASM(客戶端)+ native(harness/伺服器)** | 一份確定性核同時餵 web 客戶端、harness、未來伺服器;WASM 瘦,合 Poki | 不升級——這是定海神針 |
| **客戶端/表現層** | **輕量 web**(沿用/演化現有 HTML-JS,Pixi.js 之後按需上) | Poki-first 要輕、要快、吃低階手機;Godot-web 太肥 | 想要更多 juice/效能再上 Pixi;**不引 Godot** |
| **儲存 / 非同步突襲** | **Poki AUDS**(+ Poki Accounts 作身分) | 確定性讓回放=幾位元組且任何客戶端可重現,笨儲存就夠;省掉自架伺服器 | 需要伺服器權威防作弊時 → 接 Nakama/Go(見下) |
| **即時網路** | **Netlib — 不用** | 設計刻意非同步(《設計統整》§5),不需即時連線 | 只有做即時「法術對轟」小模式時才碰 |
| **後端服務** | **Go / Nakama — 砍到「以後才接」** | Poki 階段 AUDS 全包了,不需要 | 防作弊 / 離開 Poki / 競技經濟成熟時才接,且跑同一顆 Rust sim(cgo),單一來源不破 |

**結論:整段 Poki 旅程(到 Demo 3、多半連 Demo 4)不需架任何伺服器、不用 Go、不用 Nakama。**

---

## 4. 各層細節

### 4.1 sim 核 — Rust(WASM + native)

- 介面:`step(state, action) → {state, events, status}`,**無 DOM、無計時器、無 `Math.random`/`Date.now`**(種子由外傳入)。
- 客戶端用 `wasm-bindgen` 呼叫;harness 與(未來)伺服器跑 native。
- 確定性要點(沿用速度規格 §3):有理/整數時間累加、tiebreak 寫死(玩家優先→entity id)、**任何影響狀態的迭代一律顯式排序**(Rust 同樣要避免靠不穩定迭代序)。
- **現有 `demo1.html` 的 JS sim 已被證明好玩 → 它就是這次 Rust port 的規格。** 不是重寫玩法,是把已驗證邏輯搬語言。

### 4.2 客戶端 / 表現層 — 輕量 web

- 起手:**沿用並演化現有 DOM/Canvas 客戶端**(它已是合法的 mobile-web base),把「算結果」那段換成呼叫 Rust-WASM。
- 渲染器:先 DOM/Canvas;要更多 juice/效能再上 **Pixi.js**(Poki 明列支援);**Phaser 偏重,先不用**。
- **必做的觸控改造**:現在的 hover 預覽路徑在手機沒有 hover → 改成「**tap 目的地預覽 → 再 tap 確認**」。tap 目標 ≥44–48px。
- **版面**:改成 16:9 縮放填滿 + 手機直/橫式滿版(不再是固定窄欄)。

### 4.3 儲存 / 非同步突襲 — Poki AUDS(+ Accounts)

REST 儲存 API。`data` = freeform JSON,`values` = 可查詢的純量(字串/數字/布林)。

- **存基地** → 基地 IR(set-piece 組合 / tile 網格)放 `data`;難度/段位/type/作者放 `values`。(AUDS 官方範例就是存 `tiles:[...]` + `type`,等於關卡儲存器)
- **找基地打(非同步配對)** → 用 `values` 篩選 + 排序 + `limit`(≤100)撈一頁挑。
- **突襲** → 客戶端用 Rust-WASM 確定性打那份快照。
- **回放** → `{基地 id, seed, 操作序列}`,幾位元組;防守方撈回來用自己的 Rust-WASM **重跑就 bit 級一致**,**不需伺服器存狀態或驗證**。
- **炫耀指標** → `_increment` 計數器(被打次數)、`_vote` 投票(基地評分),皆免 secret。
- 身分/擁有權:secret(存 localStorage,**包 try/catch 過無痕**)或 **Poki Accounts JWT**(建議,避免清快取就失去基地)。
- 限制:TTL ~1 年(寫入續期,廢棄基地過期=正合意);CORS 僅 Poki + 開發網域;查詢 `q` 語法之後會變,別綁太死。

### 4.4 即時網路 — Netlib(擱置)

- Netlib = WebRTC datachannel 的 P2P 即時 UDP 庫(「web 版 Steam Networking」)。
- 你的 PvP 刻意非同步 → **核心迴圈用不到**。只有未來做即時對轟小模式才碰。現在放著。

### 4.5 後端服務 — Go / Nakama(砍到「以後才接」)

- 現階段不做。**升級觸發條件**:需要伺服器權威防作弊、或要離開 Poki、或競技/經濟成熟需要可信驗證。
- 接的方式:Nakama 的 Go 權威 handler 收到 `{基地, seed, ops}` → **用 cgo 呼叫同一顆 Rust sim 重跑驗證**。客戶端(WASM)與伺服器(cgo)跑同一份 → 護欄 A 不破。
- Nakama 也是 Go 後端,自架(Docker + Postgres),屆時順手接配對/排行/帳號/社交。

---

## 5. Poki 上架硬約束(直接影響客戶端做法)

- **初始下載 ≤ 8 MB**:輕量 web + 小 Rust-WASM 穩穩達標(這也是砍 Godot 的硬理由)。
- **16:9 縮放填滿**,參考解析度 640×360 / 836×470 / 1031×580;手機滿版(直或橫)。
- **自適應控制**:手機/平板觸控、桌面鍵盤;平板強制手機控制。
- **🔴 預設封鎖所有外部請求**:所有資產/字型**打包進 build**。
  - ⚠️ 現有 `demo1.html` 用了 `fonts.googleapis.com` 的 Fredoka → **會壞,要改本地字型**。
  - 用外部多人伺服器才需要隱私聲明(僅當未來接 Nakama 時)。
- **無痕支援**:localStorage 包 try/catch。
- **存檔系統**:要有,或明確告知玩家不會存(AUDS + Accounts 即可)。
- **SDK 事件**:`gameLoadingFinished()`、`gameplayStart()`(玩家**第一次輸入**時,非載入時)、`gameplayStop()`(任何中斷)、`commercialBreak()` / `rewardedBreak()`。**roguelite 的天然廣告點 = 兩局之間 / 死亡重來**。
- **只能用 Poki 廣告、無內購、開 adblocker 也要能玩核心**。
- **先讀「Working With AI」章**:本專案用 skill/loop-work,需符合 Poki 對 AI 生成內容的政策。
- **Player Fit Test / soft release** = 真人測試funnel,幾小時出反應 → **這就是 Demo 閘門「好不好玩」的放大版**。skill/loop 出 build,Poki 真人測試當外部閘門。

---

## 6. 對到 Demo 階段(每塊何時上)

| 階段 | 客戶端 | sim | 儲存/後端 |
|---|---|---|---|
| **Demo 0 / 1** | 輕量 web | Rust-WASM | 無(野區=客戶端程序生成 + harness 驗) |
| **Demo 2** 基地編輯器 | web,組 set-piece | 同上 | 之後用 AUDS 存佈局 |
| **Demo 3** 非同步突襲 | web | 同上 | **AUDS**(存基地/查詢/客戶端突襲/回放)+ Accounts;**無伺服器** |
| **Demo 4** 經濟/排行 | web | 同上 | AUDS 計數器/查詢 + Accounts(粗糙但夠驗) |
| 之後 | (同) | 同上 | 防作弊/離開 Poki/即時模式 → Go/Nakama/Netlib 才登場 |

---

## 7. 待確認 / 風險

1. **🔴 AUDS 標示「開發中、勿用於正式環境」**。策略:**用它把 Demo 3 快速做出來驗證好不好玩**;上線前向 Poki(Discord / developersupport）確認生產狀態;心裡留「真要時接 Nakama/Go 跑 Rust sim 驗證」的後備。
2. **AUDS 無伺服器權威驗證 = 無防作弊**。Poki 階段「先驗好不好玩、不驗平不平衡」→ 可接受;防作弊是後話。
3. **觸控輸入重設計**(hover→tap 預覽)與 **16:9 滿版版面**:客戶端必做項。
4. **資產/字型打包**:移除所有外部請求(先處理 Google Fonts)。
5. **Poki「Working With AI」政策**:動 skill/loop 前先讀。

---

## 8. 下一步:B0(下一個對話討論)

抽出 **headless Rust sim**:把 `demo1.html` 已驗證的玩法,port 成 `step(state, action) → {state, events, status}` 的純函式 Rust 模組——**無 DOM、種子外傳、確定性**。

- 它一份同時餵:客戶端(WASM)、harness(native,跑 baseline 參考 agent 的可解性檢查)、未來伺服器(cgo)。
- 規格 = 那顆已被證明好玩的 JS 原型;不是重寫玩法。
- 這是**無論後端怎麼決定都不變、且現在就能動**的第一步。

> 對應《Demo 階段計畫》方法層:**階段 A**(設計定型:set-piece 詞彙已拆、IR schema 草案已出)已近完成;**階段 B 第 0 步 = 本節 B0**,排在 baseline 參考 agent + harness 之前。
