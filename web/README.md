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

## 邊界設計(免 wasm-bindgen)

只用已安裝的 `wasm32-unknown-unknown` target(無需 wasm-bindgen/wasm-pack CLI)。
匯出 C 函式;狀態以 JSON 字串寫進 wasm 記憶體,JS 讀 `ptr`(`mr_buf_len()` 取長度)後 `JSON.parse`。

| 匯出 | 作用 |
|---|---|
| `mr_new(seed)` | 開新一場(種子外傳) |
| `mr_step(act,x,y,spell)` | 套用一手(0 待機 /1 回血瓶 /2 移動 /3 施法 /4 釋放)→ status code |
| `mr_rejected()` | 上一手是否非法(無時間流逝) |
| `mr_status()` | 0 等輸入 /1 等釋放 /2 三選一 /3 通關 /4 陣亡 |
| `mr_offers()` / `mr_pick(code)` / `mr_next_room()` | 三選一與換房 |
| `mr_render()` + `mr_buf_len()` | 完整可渲染狀態 JSON(tiles/fire/ents/chain/slamCells/acquired/tiers) |

法術 `code` 對齊 sim `SPELL_ORDER`:0 bolt /1 push /2 fire /3 heavy /4 oilflask /5 hook /6 haste。

## 現況(階段 C 第一刀)

- ✅ 同一份 sim 編 WASM(88 KB,遠低於 Poki 8MB),Node 驗證邊界 step/pick/room 流程正常。
- ✅ 可玩殼:Canvas 渲染、tap 移動 / 選法術再 tap 目標(觸控友善 ≥46px)、順序鏈、
  三選一 / 勝敗 overlay、回血瓶、boss 砸擊預告紅格、系統字型(無外部請求)。
- ✅ Poki SDK 薄殼(有則用、無則 no-op):`gameLoadingFinished` / `gameplayStart`(首次輸入)/ `gameplayStop`。
- ⏳ 待辦:events→動畫(目前每手整盤重繪,無逐格動畫)、16:9 縮放填滿打磨、丟牌 UI、
  Poki SDK 廣告點(死亡重來 / 兩局之間)、**JS↔Rust 對拍**(demo1.html JS sim vs 這顆 WASM)。
