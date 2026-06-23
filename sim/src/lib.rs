//! 魔法基地突襲 — 確定性 sim 核(headless)。
//!
//! **規格 = `prototype/demo1.html` 的行為**(已驗證好玩),不是重寫玩法。
//! 一份實作同時餵客戶端(WASM)、harness(native)、未來伺服器(cgo)——見 CLAUDE.md 紀律。
//!
//! 目標契約(B0 §E,逐模組長到位後實作):
//! ```text
//! step(state, action, seed) -> { state, events, status }   // 純函式,無 DOM/計時器/外部隨機
//! ```
//!
//! ## 目前進度(階段 B 骨架)
//! - ✅ `config` / `state` / `events` / `time_chain`:時間鏈與確定性地基已可驗。
//! - ✅ `grid` / `damage` / `terrain`:格子謂詞、整數 LoS、傷害+event、火油木牆 CA(確定性已驗)。
//! - ✅ `movement`:推/震/勾/走位落地 + ZOC 煞車 + 整數成本 A\*(D-3)。
//! - ✅ `ai`:小鬼/符文眼/魔像三套確定性行為 + 過熱循環。
//! - ✅ `spells`:7 招 registry(validate/cast/initiate)+ 烈焰術 AoE + 連鎖閃電接口。
//! - ✅ `roguelite`:確定性 PRNG(種子外傳)+ 三選一撿取 + 房間載入 + run 狀態。
//! - ⏳ 待補:`lib::step` 全契約(時間鏈 + action + status)與 `project_chain` 完整前瞻。

pub mod ai;
pub mod config;
pub mod damage;
pub mod events;
pub mod grid;
pub mod movement;
pub mod roguelite;
pub mod spells;
pub mod state;
pub mod terrain;
pub mod time_chain;

pub use damage::{StepCtx, TierTable};
pub use events::{Event, Status};
pub use roguelite::{
    apply_drop, apply_pick, gen_offers, hash32, init_room, rng_for, Mulberry32, Op, PickResult,
    RunState,
};
pub use spells::{Element, Reject, Spell, Target, TargetKind};
pub use state::{Channel, Entity, GameState, Kind, Tile};
