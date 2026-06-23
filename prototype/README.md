# prototype/

## `demo1.html` — 已驗證好玩的 JS 原型(= Rust port 規格)

單檔 HTML/JS 原型,已含 **Demo 0 + 大半 Demo 1** 玩法(速度制戰鬥、推/尖刺/油火、三選一法術收集、元素=機制)。直接用瀏覽器開即可玩。

**它是真相來源**:後續把玩法 port 成確定性 Rust sim 核時,**規格就是這支原型的行為**——不是重寫玩法,是把已驗證邏輯搬語言。逐項拆法(sim 核 vs view、確定性風險、行號對應)見 [`../docs/05-b0-migration.md`](../docs/05-b0-migration.md);技術選型見 [`../docs/04-tech-stack.md`](../docs/04-tech-stack.md)。

### 注意(保持原貌,問題已在 backlog 追蹤)

- 本檔**刻意不修改**,保留為最初版參考基準。
- 它用了 `fonts.googleapis.com` 的 Fredoka 字型 → Poki 預設封鎖外部請求,正式客戶端**必須改打包本地字型**(見 [`../docs/07-master-backlog.md`](../docs/07-master-backlog.md) 橫切約束)。
- hover 預覽、固定窄欄版面等也都是 view 層待改造項,與此原型玩法邏輯無關。
