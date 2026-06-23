//! roguelite 層:確定性 PRNG、三選一撿取、房間載入、整場 run 狀態。
//!
//! 對應 `prototype/demo1.html`:`hash32`/`mulberry32`/`rngFor`(783–785)、`genOffers`(788–796)、
//! `choosePick`/`showDrop`/`finishPick`(815–837)、`loadRoom` 的 sim 半邊(206–230)。
//!
//! ⚠ 確定性(B0 §D-5):**種子外傳**。`run_seed` 由殼產生後傳入(JS 原本 `Date.now()^Math.random()`
//! 已搬出 sim)。`hash32` 吃 ASCII 字串、`mulberry32` 全 u32 wrapping → 與 JS bit 對齊。
//! 洗牌索引用整數運算,與 JS `floor(rand()*n)` 的浮點結果 bit 一致(見 `Mulberry32::below`)。

use crate::config::{self, POTION_COUNT};
use crate::damage::TierTable;
use crate::grid::slam_area;
use crate::spells::{pickable, Spell};
use crate::state::{Entity, GameState, Kind, Tile};

// ─────────────────────────── 確定性 PRNG ───────────────────────────

/// FNV-1a 變體(對應 JS `hash32`)。輸入為 ASCII(`seed|tag|idx`),逐 byte = JS charCodeAt。
pub fn hash32(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

/// mulberry32 PRNG(對應 JS `mulberry32` 閉包)。全 u32 wrapping 算術。
pub struct Mulberry32 {
    state: u32,
}

impl Mulberry32 {
    pub fn new(seed: u32) -> Self {
        Mulberry32 { state: seed }
    }

    /// 下一個 u32(= JS 回傳前的 `(t ^ t>>>14) >>> 0`)。
    pub fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_add(0x6D2B79F5);
        let a = self.state;
        let mut t = (a ^ (a >> 15)).wrapping_mul(a | 1);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61)) ^ t;
        t ^ (t >> 14)
    }

    /// 均勻取 `[0, n)` 的整數(n>0)。
    ///
    /// 等同 JS `Math.floor(rand()*n)`,其中 `rand()=X/2^32`:
    /// `floor(X/2^32 * n) = floor(X*n/2^32) = (X*n) >> 32`。因 `X<2^32`、`n` 小,
    /// `X*n < 2^53` 在 f64 可精確表示 → 整數結果與 JS 浮點結果 **bit 一致**,且無 float。
    pub fn below(&mut self, n: usize) -> usize {
        ((self.next_u32() as u64 * n as u64) >> 32) as usize
    }
}

/// `rngFor(tag, idx)`:由 run_seed + 用途 + 索引 開一條獨立序列(與抽過幾次無關 → 任何路徑可重現)。
pub fn rng_for(run_seed: u32, tag: &str, idx: usize) -> Mulberry32 {
    Mulberry32::new(hash32(&format!("{run_seed}|{tag}|{idx}")))
}

// ─────────────────────────── 整場 run 狀態 ───────────────────────────

/// 操作記錄(回放/突襲快照用)。對應 JS `opLog` 的條目。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Pick {
        room: usize,
        take: Spell,
        drop: Option<Spell>,
    },
    Upgrade {
        room: usize,
        id: Spell,
    },
}

/// 跨房持續的 run 狀態(對應 JS 全域 acquired/tier/potions/RUN_SEED/opLog)。
/// 與 `GameState`(單房戰鬥)分離。
#[derive(Clone, Debug)]
pub struct RunState {
    pub run_seed: u32,
    /// 已撿法術(不含 baseline),維持撿取序。
    pub acquired: Vec<Spell>,
    /// 各法術等級(1 / 2)。戰鬥時複製進 `StepCtx.tiers`。
    pub tiers: TierTable,
    pub potions: u32,
    pub room_idx: usize,
    pub op_log: Vec<Op>,
}

impl RunState {
    /// 開新一場(對應 JS loadRoom idx===0 的重置)。**種子外傳**。
    pub fn new(run_seed: u32) -> Self {
        RunState {
            run_seed,
            acquired: Vec::new(),
            tiers: TierTable::default(),
            potions: POTION_COUNT,
            room_idx: 0,
            op_log: Vec::new(),
        }
    }
}

// ─────────────────────────── 三選一撿取 ───────────────────────────

/// 產生本房三選一選項。對應 JS `genOffers`:可撿池(未撿 or 未滿級)→ 種子化洗牌 → 取前 3。
pub fn gen_offers(run: &RunState) -> Vec<Spell> {
    let mut pool: Vec<Spell> = pickable()
        .into_iter()
        .filter(|s| {
            if !run.acquired.contains(s) {
                true // 未撿 → 可獲得
            } else {
                run.tiers.of(s.id()) < s.max_tier() // 已撿但未滿級 → 可升級
            }
        })
        .collect();

    // Fisher-Yates,索引序與 JS 完全一致(i 從 len-1 遞減到 1,j ∈ [0,i])。
    let mut rand = rng_for(run.run_seed, "picks", run.room_idx);
    let mut i = pool.len();
    while i > 1 {
        i -= 1;
        let j = rand.below(i + 1);
        pool.swap(i, j);
    }
    pool.truncate(3);
    pool
}

/// 選牌後是否還需要玩家「丟一張」。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickResult {
    /// 已套用(升級 or 還有空欄位)。
    Done,
    /// 欄位已滿,需呼叫 `apply_drop(new_id, drop_id)`。
    NeedDrop,
}

/// 套用一次選擇。對應 JS `choosePick` 的狀態變更部分。
pub fn apply_pick(run: &mut RunState, id: Spell) -> PickResult {
    if run.acquired.contains(&id) {
        // 已有 → 升級,不占槽。
        let nt = (run.tiers.of(id.id()) + 1).min(id.max_tier());
        run.tiers.set(id.id(), nt);
        run.op_log.push(Op::Upgrade {
            room: run.room_idx,
            id,
        });
        PickResult::Done
    } else if run.acquired.len() < config::SPELL_CAP {
        run.acquired.push(id);
        run.tiers.set(id.id(), 1);
        run.op_log.push(Op::Pick {
            room: run.room_idx,
            take: id,
            drop: None,
        });
        PickResult::Done
    } else {
        PickResult::NeedDrop
    }
}

/// 欄位滿時,丟掉 `drop_id` 換上 `new_id`。對應 JS `showDrop` 的 onclick 狀態變更。
pub fn apply_drop(run: &mut RunState, new_id: Spell, drop_id: Spell) {
    if let Some(i) = run.acquired.iter().position(|&s| s == drop_id) {
        run.op_log.push(Op::Pick {
            room: run.room_idx,
            take: new_id,
            drop: Some(drop_id),
        });
        run.tiers.remove(drop_id.id());
        run.acquired[i] = new_id;
        run.tiers.set(new_id.id(), 1);
    }
}

// ─────────────────────────── 房間載入 ───────────────────────────

/// 把 `ROOMS[room_idx]` 字元地圖解析成 `GameState`。對應 JS `loadRoom` 的 sim 半邊(B0 §C-1)。
///
/// 實體 id:法師 = 0,其餘按掃描序(y,x)1 起跳;最後依 id 排序維持不變式(entities 為 id 序)。
/// 魔像的**初始砸擊預告**在此 arm(對應 JS 行 224 `boss.slam=slamArea(mage)`),補上 ai.rs 標註的缺口。
pub fn init_room(room_idx: usize) -> GameState {
    let rows = config::ROOMS[room_idx].map;
    let h = rows.len() as i32;
    let w = rows[0].chars().count() as i32;

    let mut tiles = Vec::with_capacity(h as usize);
    let mut entities = Vec::new();
    let mut next_id = 1u32;

    for (y, row) in rows.iter().enumerate() {
        let mut trow = Vec::with_capacity(w as usize);
        for (x, c) in row.chars().enumerate() {
            trow.push(match c {
                '#' => Tile::Wall,
                'W' => Tile::Wood,
                '~' => Tile::Oil,
                's' => Tile::Spike,
                'H' => Tile::Rune,
                _ => Tile::Floor,
            });
            let (xi, yi) = (x as i32, y as i32);
            match c {
                '@' => entities.push(Entity::new(0, Kind::Mage, xi, yi)),
                'o' => {
                    entities.push(Entity::new(next_id, Kind::Imp, xi, yi));
                    next_id += 1;
                }
                'e' => {
                    entities.push(Entity::new(next_id, Kind::Eye, xi, yi));
                    next_id += 1;
                }
                'B' => {
                    entities.push(Entity::new(next_id, Kind::Boss, xi, yi));
                    next_id += 1;
                }
                _ => {}
            }
        }
        tiles.push(trow);
    }
    entities.sort_by_key(|e| e.id); // 維持 entities 為 id 序(顯式排序,B0 §D-6)

    let mut g = GameState {
        w,
        h,
        fire: vec![vec![0; w as usize]; h as usize],
        burn_t: vec![vec![0; w as usize]; h as usize],
        tiles,
        entities,
        room_idx,
    };

    // arm 魔像初始預告(在法師起始位置周圍)。
    if let Some(bi) = g.entities.iter().position(|e| e.kind == Kind::Boss) {
        let (mx, my) = {
            let m = g.mage();
            (m.x, m.y)
        };
        g.entities[bi].slam = Some(slam_area(&g, mx, my));
    }
    g
}
