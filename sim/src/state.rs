//! 狀態容器與實體模型。
//!
//! 對應 `prototype/demo1.html`:`G`(行 221)與 entity 欄位(行 215–218)。
//! 確定性紀律:實體存 `Vec`(id 序),禁用會受迭代序影響的 HashMap(B0 §D-6)。

use crate::config;

/// 實體種類(對應 JS entity.kind 字串)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mage,
    Imp,
    Eye,
    Boss,
}

impl Kind {
    #[inline]
    pub fn is_mage(self) -> bool {
        matches!(self, Kind::Mage)
    }
}

/// 前搖通道(強力法術兩格制,速度規格 §4)。對應 JS `mage.channel`。
#[derive(Clone, Debug)]
pub struct Channel {
    /// 此通道對應的法術 id(釋放時據此結算;view 端 metadata 也用)。
    pub spell: &'static str,
    /// 蓄力鎖定的目標格(對應 JS channel.x/y)。
    pub tx: i32,
    pub ty: i32,
    /// 前搖已撐過、停在釋放手(對應 JS mage.channel.ready)。
    pub ready: bool,
    /// 蓄力期間受傷被打斷(對應 JS mage.channel.interrupted)。
    pub interrupted: bool,
}

/// 單一實體。對應 demo1.html entity 物件;`Option` 對應 JS 的 null 欄位。
#[derive(Clone, Debug)]
pub struct Entity {
    pub id: u32,
    pub kind: Kind,
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    pub maxhp: i32,
    /// 速度,半步整數(見 config:1.0=2、1.5=3)。
    pub speed_halves: u8,
    /// 時間鏈上「下次輪到我」的時間值,1/6 整數單位(config::TIME_UNIT)。
    pub time: i64,
    /// 加速剩餘手數(僅法師;CFG.hasteGrant)。
    pub haste_turns: u32,
    /// 眩暈剩餘手數(電擊/震退;對應 JS entity.stun)。
    pub stun: u32,
    pub channel: Option<Channel>,
    /// 法師 auto-walk 剩餘路徑(對應 JS `mage.path`);走到 ZOC 邊緣或被打會清空。
    pub path: Option<Vec<(i32, i32)>>,
    // boss 專用(對應 JS pendingSlam/exhausted/slam)。
    pub pending_slam: bool,
    pub exhausted: bool,
    /// 已預告(telegraph)的砸擊格;`None` = 尚未布告。對應 JS `boss.slam`。
    /// 初始 telegraph 由 `init_room` 在房載入時 arm(對應 JS loadRoom 行 224);未 port 前為 `None`。
    pub slam: Option<Vec<(i32, i32)>>,
}

impl Entity {
    #[inline]
    pub fn alive(&self) -> bool {
        self.hp > 0
    }

    /// 有效速度(半步):法師加速中用 haste,否則基礎速度。對應 JS `effSpeed`(行 245)。
    #[inline]
    pub fn eff_speed_halves(&self) -> u8 {
        if self.kind.is_mage() && self.haste_turns > 0 {
            config::SPEED_HALVES_HASTE
        } else {
            self.speed_halves
        }
    }

    pub fn new(id: u32, kind: Kind, x: i32, y: i32) -> Self {
        let hp = match kind {
            Kind::Mage => config::MAGE_HP,
            Kind::Imp => config::IMP_HP,
            Kind::Eye => config::EYE_HP,
            Kind::Boss => config::BOSS_HP,
        };
        Entity {
            id,
            kind,
            x,
            y,
            hp,
            maxhp: hp,
            speed_halves: config::SPEED_HALVES_BASE,
            time: 0,
            haste_turns: 0,
            stun: 0,
            channel: None,
            path: None,
            pending_slam: kind == Kind::Boss,
            exhausted: false,
            slam: None,
        }
    }
}

/// 格子地形(對應 JS tiles 字串值)。骨架先列已用到的;火/油的計時走 fire/burn_t 平行 grid。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    Floor,
    Wall,
    Wood,
    WoodBurn,
    Oil,
    Spike,
    Rune,
}

/// 全域戰鬥狀態。對應 JS `G = {w,h,tiles,fire,burnT,entities,roomIdx}`。
#[derive(Clone, Debug)]
pub struct GameState {
    pub w: i32,
    pub h: i32,
    pub tiles: Vec<Vec<Tile>>,
    /// 火格剩餘時間(0 = 無火)。
    pub fire: Vec<Vec<i32>>,
    /// 燃燒中木牆剩餘 tick。
    pub burn_t: Vec<Vec<i32>>,
    /// 實體,**永遠以 id 序保存**(確定性,B0 §D-6)。
    pub entities: Vec<Entity>,
    pub room_idx: usize,
    /// 基地突襲的核心目標格(踩到即贏);`None` = 野區 run(無核心)。決定 base-raid 終局(Demo 2)。
    pub core: Option<(i32, i32)>,
}

impl GameState {
    /// 取法師(id 0)。骨架假設法師恆存在於 entities。
    pub fn mage(&self) -> &Entity {
        self.entities
            .iter()
            .find(|e| e.kind.is_mage())
            .expect("法師應恆存在")
    }

    pub fn mage_mut(&mut self) -> &mut Entity {
        self.entities
            .iter_mut()
            .find(|e| e.kind.is_mage())
            .expect("法師應恆存在")
    }

    pub fn alive_enemies(&self) -> usize {
        self.entities
            .iter()
            .filter(|e| e.alive() && !e.kind.is_mage())
            .count()
    }
}
