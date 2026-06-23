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

## 已知漂移(待設計定奪)

**投射物攔截**:Rust port 的**魔法彈**與**符文眼**都用 `first_unit_on_ray`(射線上第一個單位中彈,
「身體當掩體」),但 `demo1.html` 兩者都是**直接命中目標/法師、單位不擋**。對拍第一發即抓到
(seed 0 第 3 手:眼的射擊在 Rust 打中擋路的小鬼、JS 直接打法師)。這是**港版新增、原型沒有的機制**,
需決定:把攔截同步進原型(採納)或 Rust 改回直接命中(對齊)。**決定前對拍會在此停。**

## 限制(下一步)

- 目前只驗到房 0(第一個 `nextroom` 前)。`--full` 跨房需把 **tier(★★)** 也納入 trace 與 JS 套用
  (升級會改法術行為)。pick 的 RNG(`genOffers`)JS/Rust 同式(hash32/mulberry32),可另立 offers 對拍。
- 對的是 **native Rust**(== WASM:全整數、無 float、同源,確定性由 harness 500 場已證)。
