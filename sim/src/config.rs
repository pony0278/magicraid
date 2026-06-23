//! 設定常數與時間換算。
//!
//! 對應 `prototype/demo1.html` 的 `CFG`(行 154–171)與《速度制戰鬥規格》。
//! 數字多為 playtest 佔位 → 集中在此,別散寫死(見 CLAUDE.md 開發慣例)。

/// 時間鏈以 1/6 為單位的整數累加(6 = 速度 {0.5,1,1.5} 的最小公分母)。
///
/// 為何整數:`time += 1/速度` 在 float 下,`1/1.5` 二進位非終止 → 加速一開就漂(B0 §D-1)。
/// 改成「每次 += 6/速度」全為整數:速度 0.5→+12、1.0→+6、1.5→+4、2.0→+3。
pub const TIME_UNIT: i64 = 6;

/// 速度以「半步」整數表示,避免浮點:speed_halves = 速度 × 2。
/// 0.5→1、1.0→2、1.5→3、2.0→4。夾在 [0.5, 2.0] = [1, 4]。
pub const SPEED_HALVES_MIN: u8 = 1; // 0.5
pub const SPEED_HALVES_MAX: u8 = 4; // 2.0
pub const SPEED_HALVES_BASE: u8 = 2; // 1.0 — 所有角色基礎速度
pub const SPEED_HALVES_HASTE: u8 = 3; // 1.5 — CFG.hasteSpeed

/// 一次行動後時間值的增量(1/6 單位)= TIME_UNIT × 2 / speed_halves。
///
/// 推導:增量(real) = 1/速度;化成 1/6 單位 = 6/速度 = 6/(speed_halves/2)
///       = 12/speed_halves = TIME_UNIT*2/speed_halves。
/// speed_halves ∈ {1,2,3,4} 時 12 皆可整除 → 永遠整數,無 float。
#[inline]
pub fn time_step(speed_halves: u8) -> i64 {
    let h = clamp_speed(speed_halves);
    debug_assert!((TIME_UNIT * 2) % h as i64 == 0, "速度 {h} 不整除,會引入分數時間");
    (TIME_UNIT * 2) / h as i64
}

/// 夾速度到 [0.5, 2.0](速度規格 §2:防緩速疊到 0、加速疊到無限)。
#[inline]
pub fn clamp_speed(speed_halves: u8) -> u8 {
    speed_halves.clamp(SPEED_HALVES_MIN, SPEED_HALVES_MAX)
}

// ── 法師 / 敵人數值(CFG 行 155–166)。先放會用到的;其餘隨模組逐步補。 ──
pub const MAGE_HP: i32 = 14;
pub const HASTE_GRANT: u32 = 4; // 加速持續手數(CFG.hasteGrant)

// 敵人(CFG.imp / eye / boss)
pub const IMP_HP: i32 = 5;
pub const EYE_HP: i32 = 4;
pub const BOSS_HP: i32 = 24;
