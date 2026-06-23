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
//! - ⏳ 待補(對齊 docs/05-b0-migration.md A 區):
//!   `grid` / `damage` / `terrain` / `spells` / `movement` / `ai` / `roguelite`,
//!   以及 `lib::step` 全契約與 `project_chain` 完整前瞻。

pub mod config;
pub mod events;
pub mod state;
pub mod time_chain;

pub use events::{Event, Status};
pub use state::{Channel, Entity, GameState, Kind, Tile};
