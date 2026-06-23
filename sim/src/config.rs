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

// ── 法術數值(CFG.bolt/fire/heavy/hook/oil,行 156–165)。 ──
pub const BOLT_DMG: i32 = 3;
pub const BOLT_RANGE: i32 = 5;
pub const FIRE_DMG: i32 = 3; // 火球直擊傷害
pub const FIRE_RANGE: i32 = 4;
pub const HEAVY_DMG: i32 = 5; // 烈焰術 AoE 傷害
pub const HEAVY_RANGE: i32 = 4;
pub const HOOK_RANGE: i32 = 4;
pub const OIL_RANGE: i32 = 3;

// 敵人(CFG.imp / eye / boss,行 161–163)
pub const IMP_HP: i32 = 5;
pub const IMP_DMG: i32 = 3; // 小鬼貼臉攻擊
pub const EYE_HP: i32 = 4;
pub const EYE_DMG: i32 = 2; // 符文眼射擊
pub const EYE_RANGE: i32 = 4; // 符文眼射程(切比雪夫)
pub const BOSS_HP: i32 = 24;
pub const BOSS_SLAM: i32 = 6; // 魔像砸擊傷害

// ── 位移 / 危險格(CFG.push / spikeDmg,行 159–160)。 ──
/// 推/震/勾撞上阻擋時的撞擊傷害(CFG.push.crash)。
pub const PUSH_CRASH: i32 = 1;
/// 踩到/被推上尖刺格的傷害(CFG.spikeDmg)。
pub const SPIKE_DMG: i32 = 6;

// ── roguelite(CFG.potion + SPELL_CAP,行 166/196)。 ──
/// 法術欄位上限。原型起手 3 槽(《法杖規格》定第 4 槽為天花板);此處照原型 = 3。
pub const SPELL_CAP: usize = 3;
/// 每場攜帶回血瓶數(CFG.potion.count)。
pub const POTION_COUNT: u32 = 2;
/// 每瓶回血量(CFG.potion.heal)。
pub const POTION_HEAL: i32 = 6;

// ── 火/地形(CFG.fire / woodBurnTicks,行 157/167)。 ──
/// 火格起火後的持續 tick 數(CFG.fire.dur)。
pub const FIRE_DUR: i32 = 2;
/// 站在火格/被點燃時每次的 DoT 傷害(CFG.fire.dot)。
pub const FIRE_DOT: i32 = 3;
/// 木牆被點燃後「燃燒中」維持幾 tick 才崩塌成火格(CFG.woodBurnTicks)。
pub const WOOD_BURN_TICKS: i32 = 1;

// ── 房間資料(對應 JS ROOMS,行 173–192)。 ──
// 圖例:# 石牆 / W 木 / . 地板 / ~ 油 / s 尖刺 / H 急速符文 / @ 法師 / o 小鬼 / e 符文眼 / B 魔像。
// hint(教學文案)是 view,不搬;只保留 sim 需要的 name + map。

/// 一間房的定義:名稱(除錯/log 用)+ 字元地圖。
pub struct RoomDef {
    pub name: &'static str,
    pub map: &'static [&'static str],
}

pub const ROOMS: &[RoomDef] = &[
    RoomDef {
        name: "房間 1 · 順序鏈 + 視線",
        map: &[
            "###########",
            "#.........#",
            "#..#....e.#",
            "#.........#",
            "#.@.....o.#",
            "#....#....#",
            "###########",
        ],
    },
    RoomDef {
        // 教學房:玩家已有「推」(基礎包)。小鬼走過來貼你,把它推進身邊的尖刺秒殺。
        // 兩道尖刺夾住法師右側、小鬼從右方逼近 → 走一步卡角度、一推撞刺(對齊 docs/02 §7)。
        name: "房間 2 · 尖刺場",
        map: &[
            "##########",
            "#........#",
            "#..s.....#",
            "#.@....o.#",
            "#..s.....#",
            "#.....o..#",
            "##########",
        ],
    },
    RoomDef {
        name: "房間 3 · 油 + 木牆",
        map: &[
            "############",
            "#..........#",
            "#..~~~..e..#",
            "#..~o~.....#",
            "#..~~~.WWW.#",
            "#......W.H.#",
            "#.@..o.....#",
            "############",
        ],
    },
    RoomDef {
        name: "房間 4 · 開闊場",
        map: &[
            "###########",
            "#....s....#",
            "#.o.....e.#",
            "#....@....#",
            "#.o.......#",
            "#....s....#",
            "###########",
        ],
    },
    RoomDef {
        name: "房間 5 · 油陣",
        map: &[
            "############",
            "#..~~~.....#",
            "#..~o~..e..#",
            "#..~~~.....#",
            "#.@.....o..#",
            "#......s...#",
            "############",
        ],
    },
    RoomDef {
        name: "關主 · 符文魔像",
        map: &[
            "###########",
            "#.........#",
            "#...~~~...#",
            "#...~B~...#",
            "#...~~~...#",
            "#.........#",
            "#.H.......#",
            "#....@....#",
            "###########",
        ],
    },
];
