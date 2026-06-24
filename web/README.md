# web/ — Poki-ready 客戶端(階段 C)

把確定性 Rust sim 編成 **WASM**,接一層輕量 DOM/Canvas 殼。**護欄 A**:跑同一份 `sim`,
這裡只做 C-ABI 邊界 + 渲染 + 輸入(玩法真相在 `sim/`)。

## 結構

```
web/
  src/lib.rs     WASM 綁定(手寫 C-ABI,免 wasm-bindgen):sim 狀態 → JSON 寫進線性記憶體
  www/
    index.html   殼(系統字型,無外部請求)
    main.js       渲染 / 輸入 / Poki SDK 接點;法術 view metadata 留這層(B0 §C-3)
    magicraid_web.wasm   構建產物(由 build.sh 產生)
  build.sh       cargo build wasm + 複製到 www/
  smoke.mjs      Node 邊界煙霧測試(驅動 step/pick/room 流程)
```

## 構建 & 試玩

```bash
rustup target add wasm32-unknown-unknown   # 一次性
web/build.sh
cd web/www && python3 -m http.server 8080  # → http://localhost:8080
```

## 線上試玩(GitHub Pages,自動部署)

`.github/workflows/ci.yml` 每次 push 到 `main` 會跑測試 + 構建 WASM,並把 `web/www/`
部署到 GitHub Pages → 一個公開網址,手機/電腦直接開來玩(免本機環境)。

**一次性設定**:GitHub repo → Settings → Pages → Source 選「**GitHub Actions**」。
(在啟用前,CI 的 `deploy` job 會失敗;`test` job 不受影響。)網址會出現在該次 Actions run 的
`deploy` job 摘要,通常是 `https://<user>.github.io/<repo>/`。

> 注意:Pages 是**公開**的(若 repo 公開)。這只是給你/測試者點開玩的 vertical slice,
> 與 Poki 正式上架無關。後端目前**不需要**(依 docs/04 技術棧:到 Demo 3 才接 Poki AUDS)。

## 邊界設計(免 wasm-bindgen)

只用已安裝的 `wasm32-unknown-unknown` target(無需 wasm-bindgen/wasm-pack CLI)。
匯出 C 函式;狀態以 JSON 字串寫進 wasm 記憶體,JS 讀 `ptr`(`mr_buf_len()` 取長度)後 `JSON.parse`。

| 匯出 | 作用 |
|---|---|
| `mr_new(seed)` | 開新一場(種子外傳) |
| `mr_step(act,x,y,spell)` | 套用一手(0 待機 /1 回血瓶 /2 移動 /3 施法 /4 釋放)→ status code |
| `mr_rejected()` | 上一手是否非法(無時間流逝) |
| `mr_status()` | 0 等輸入 /1 等釋放 /2 三選一 /3 通關 /4 陣亡 |
| `mr_offers()` / `mr_pick(code)` / `mr_drop(take,drop)` / `mr_next_room()` | 三選一與換房;`mr_pick` 回 0=已套用、1=欄位滿需丟牌 → 玩家選後 `mr_drop` |
| `mr_render()` + `mr_buf_len()` | 完整可渲染狀態 JSON(tiles/fire/ents/chain/slamCells/acquired/tiers) |

法術 `code` 對齊 sim `SPELL_ORDER`:0 bolt /1 push /2 fire /3 heavy /4 oilflask /5 hook /6 haste。

## 現況(階段 C 第一刀)

- ✅ 同一份 sim 編 WASM(88 KB,遠低於 Poki 8MB),Node 驗證邊界 step/pick/room 流程正常。
- ✅ 可玩殼:Canvas 渲染、tap 移動 / 選法術再 tap 目標(觸控友善 ≥46px)、順序鏈、
  三選一 / 勝敗 overlay、回血瓶、boss 砸擊預告紅格、系統字型(無外部請求)。
- ✅ Poki SDK 薄殼(有則用、無則 no-op):`gameLoadingFinished` / `gameplayStart`(首次輸入)/ `gameplayStop`。
- ✅ events→動畫:`mr_events()` 過 ABI,殼端逐格重放移動補間 / 命中閃光+飄字 / 死亡淡出 / 回血飄字,
  連點可跳過。**火蔓延 view-diff(A 案)**:比對 step 前後 `fire` 格,新點燃格按離火源 BFS 距離漣漪亮起
  (純表現層、不碰 sim,確定性不受影響)。
- ✅ 觸控預覽→確認(點一格瞄準、點同格提交)、16:9/橫向版面、**JS↔Rust 對拍**(`diff/`,已接 CI)、
  **丟牌 UI**(欄位滿時玩家選丟哪張:`mr_pick`→1 → `mr_drop`,不再自動丟最舊)。
- ✅ Poki 插頁廣告:`commercialBreak` 放在 run 結束斷點(死亡→Restart、通關→Play Again),
  廣告前 `gameplayStop`、新場 `firstInput` 再 `gameplayStart`;無 SDK(本機/Pages)直接放行。
  ⚠ **正式提交 Poki 時才在 index.html 加 SDK script**(現在加會違反 Pages 的無外部請求;Poki 平台會注入)。
- ✅ 全英文 UI(玩家可見字串;註解保留中文)。
- ⏳ 待辦:法術投射物 / `stun`·`haste`·`intr` 視覺(需 cast event 帶 target,屬 sim)、
  上架前移除 dev log、localStorage try/catch(無痕)、讀 Poki「Working With AI」政策。
