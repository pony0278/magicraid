# diff/ — JS↔Rust 對拍(differential testing)

驗證 Rust sim port **與已驗證好玩的 `prototype/demo1.html` JS sim 行為一致**(CLAUDE.md 紀律 #4:
port = 原型行為,不是重寫)。同一串操作餵兩份 sim,**逐手比對可觀察狀態**,抓出漂移。

## 怎麼跑

```bash
cargo build -p magicraid-harness --release   # 產 target/release/harness
node diff/compare.mjs [seed]                 # 預設 seed 0,比到房 0 結束
node diff/compare.mjs 7 --full               # 比整場(需 tier 同步,見下「限制」)
```

## 架構

- **Rust 出 trace**:`harness trace <seed>` 用 baseline agent 跑一場,**逐手吐 JSON**
  `{op, 該手後狀態}`(`harness/src/lib.rs::trace_json`)。
- **JS 無頭驅動**:`diff/jsim.mjs` 抽 `demo1.html` 的 `<script>`,在 Node `vm` + DOM shim 裡跑,
  stub 掉 view 層(render/present…),吐 `__api`:`newRun` / 套用一手 / 讀狀態。
- **比對器**:`diff/compare.mjs` 把 Rust 的 op 序列重放進 JS,逐手比對,報第一處漂移。

## 比什麼、不比什麼

比:**存活實體 id/kind/x/y/hp + fire + tiles + status + room + potions + acquired**。

**不比 time**:JS 用 float(1.0/1.5…)、Rust 用整數 1/6(6/9…),表示法本就不同(這正是 port 為
確定性做的事)。若時間鏈**順序**漂掉,實體位置/HP 會跟著分歧,照樣抓得到——比可觀察狀態比比 time 更穩。

## 狀態:seed 0..50 整場逐手一致 ✅

`node diff/compare.mjs --all 50` 綠:51 個種子、每場 ~38 手、穿過房 0–5(含撿取/升級/boss)
JS 與 Rust 可觀察狀態 bit 一致。

### 對拍抓到並已修的漂移(建構過程)

1. **投射物攔截**(seed 0 第 3 手):Rust port 的魔法彈與符文眼用 `first_unit_on_ray`(射線第一個單位
   中彈=身體當掩體),原型則直接命中、單位不擋。**決議:採納增強**——攔截已同步進 `demo1.html`
   (`firstUnitOnRay` + bolt/eye,整數取格對齊 sim `round_div`)。
2. **房間資料**(換房時):`demo1.html` 房 1(尖刺場)是舊佈局,`config.rs` 是 playtest 重畫版。
   已把重畫版回填原型。
3. **tier(★★)**:升級會改法術行為(火★★ 留火),trace 現在帶 `tiers`、JS 換房時 `setTiers` 同步。

## 限制 / 下一步

- 對的是 **native Rust**(== WASM:全整數、無 float、同源;native↔WASM 確定性由 harness 500 場已證)。
  若要直接驗 WASM,可把 trace 改走 `web/` 的 wasm(Node 載入,如 `web/smoke.mjs`)。
- pick 的三張(`genOffers`)目前由 trace 記錄、JS 直接套用,未獨立對拍;JS/Rust 的 PRNG 同式
  (hash32/mulberry32 + `seed|tag|idx`),可另立 offers 對拍補上。
- 可接 CI:`node diff/compare.mjs --all 50`(非零退出=有漂移)。
