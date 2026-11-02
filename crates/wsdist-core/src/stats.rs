use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::model::CalcError;

macro_rules! define_stats {
    ($($variant:ident => $name:literal),+ $(,)?; gear: $($allowed:ident),+ $(,)?) => {
        pub const STAT_COUNT: usize = [$(stringify!($variant)),+].len();
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u16)]
        pub enum Stat { $($variant),+ }
        impl Stat {
            pub const ALL: [Self; STAT_COUNT] = [$(Self::$variant),+];
            pub const fn index(self) -> usize { self as usize }
            pub const fn name(self) -> &'static str { match self { $(Self::$variant => $name),+ } }
            pub fn from_name(name: &str) -> Option<Self> { match name { $($name => Some(Self::$variant)),+, _ => None } }
            pub const fn gear_allowed(self) -> bool { match self { $(Self::$allowed)|+ => true, _ => false } }
        }
    };
}

// Exact strings include legacy misspellings; loading must not normalize them.
define_stats! {
    RaMagicAttack => "-ra Magic Attack",
    RaMagicDamage => "-ra Magic Damage",
    RaMagicDamageLegacyLower => "-ra Magic damage",
    Agi => "AGI",
    Accuracy => "Accuracy",
    Accuracy1 => "Accuracy1",
    Accuracy2 => "Accuracy2",
    AmmoDmg => "Ammo DMG",
    AmmoDelay => "Ammo Delay",
    ArcherySkill => "Archery Skill",
    Attack => "Attack",
    AttackPct => "Attack%",
    Attack1 => "Attack1",
    Attack2 => "Attack2",
    AutomatonSkill => "Automaton Skill",
    AxeSkill => "Axe Skill",
    Barrage => "Barrage",
    BarrageRangedAccuracy => "Barrage Ranged Accuracy",
    BarrageRangedAttack => "Barrage Ranged Attack",
    BaseDefense => "Base Defense",
    BloodPactDamage => "Blood Pact Damage",
    BlueMagciSkill => "Blue Magci Skill",
    BlueMagicSkill => "Blue Magic Skill",
    BuildingFlourishWsd => "Building Flourish WSD",
    Chr => "CHR",
    ClimacticCritDamage => "Climactic Crit Damage",
    ClubSkill => "Club Skill",
    ConserveTp => "Conserve TP",
    ConverveTp => "Converve TP",
    CritDamage => "Crit Damage",
    CritRate => "Crit Rate",
    Da => "DA",
    DaDamagePct => "DA Damage%",
    Dex => "DEX",
    Dmg => "DMG",
    Dmg1 => "DMG1",
    Dmg2 => "DMG2",
    Dt => "DT",
    Dt2 => "DT2",
    DaggerSkill => "Dagger Skill",
    Daken => "Daken",
    DarkAffinity => "Dark Affinity",
    DarkElementalBonus => "Dark Elemental Bonus",
    DarkMagicSkill => "Dark Magic Skill",
    DarkSkill => "Dark Skill",
    Defense => "Defense",
    Delay => "Delay",
    DelayReduction => "Delay Reduction",
    Delay1 => "Delay1",
    Delay2 => "Delay2",
    DivineMagicSkill => "Divine Magic Skill",
    DoubleShot => "Double Shot",
    DoubleShotDamagePct => "Double Shot Damage%",
    DualWield => "Dual Wield",
    EarthAffinity => "Earth Affinity",
    EarthElementalBonus => "Earth Elemental Bonus",
    EbullienceBonus => "Ebullience Bonus",
    ElementalBonus => "Elemental Bonus",
    ElementalMagicSkill => "Elemental Magic Skill",
    ElementalWsDamagePct => "Elemental WS Damage%",
    EnSpellDamage => "EnSpell Damage",
    EnSpellDamageMain => "EnSpell Damage main",
    EnSpellDamageSub => "EnSpell Damage sub",
    EnSpellDamagePct => "EnSpell Damage%",
    EnSpellDamagePctMain => "EnSpell Damage% main",
    EnSpellDamagePctSub => "EnSpell Damage% sub",
    Evasion => "Evasion",
    EvasionSkill => "Evasion Skill",
    Fua => "FUA",
    FuaMain => "FUA main",
    FuaSub => "FUA sub",
    Fencer => "Fencer",
    FencerTpBonus => "Fencer TP Bonus",
    FireAffinity => "Fire Affinity",
    FireElementalBonus => "Fire Elemental Bonus",
    FlourishChrPct => "Flourish CHR%",
    FoodAttack => "Food Attack",
    FoodRangedAttack => "Food Ranged Attack",
    FutaeBonus => "Futae Bonus",
    GearHaste => "Gear Haste",
    GreatAxeSkill => "Great Axe Skill",
    GreatKatanaSkill => "Great Katana Skill",
    GreatSwordSkill => "Great Sword Skill",
    HandToHandSkill => "Hand-to-Hand Skill",
    Hasso => "Hasso",
    HassoPlusJaHaste => "Hasso+ JA Haste",
    HelixMagicAccuracy => "Helix Magic Accuracy",
    HelixMagicAttack => "Helix Magic Attack",
    Int => "INT",
    IceAffinity => "Ice Affinity",
    IceElementalBonus => "Ice Elemental Bonus",
    InninDaPct => "Innin DA%",
    InstrumentSkill => "Instrument Skill",
    JaHaste => "JA Haste",
    Jobs => "Jobs",
    KatanaSkill => "Katana Skill",
    KickAttacks => "Kick Attacks",
    KickAttacksAccuracy => "Kick Attacks Accuracy",
    KickAttacksAttack => "Kick Attacks Attack",
    KickAttacksAttackPct => "Kick Attacks Attack%",
    KickAttacksDmg => "Kick Attacks DMG",
    KickDmg => "Kick DMG",
    KlimaformDamagePct => "Klimaform Damage%",
    Level => "Level",
    LightAffinity => "Light Affinity",
    LightElementalBonus => "Light Elemental Bonus",
    LightningElementalBonus => "Lightning Elemental Bonus",
    LungeBonus => "Lunge Bonus",
    Mdt => "MDT",
    Mdt2 => "MDT2",
    Mnd => "MND",
    MagicAccuracy => "Magic Accuracy",
    MagicAccuracySkill => "Magic Accuracy Skill",
    MagicAttack => "Magic Attack",
    MagicBurstAccuracy => "Magic Burst Accuracy",
    MagicBurstDamage => "Magic Burst Damage",
    MagicBurstDamageIi => "Magic Burst Damage II",
    MagicBurstDamageTrait => "Magic Burst Damage Trait",
    MagicCritRateIi => "Magic Crit Rate II",
    MagicDtPct => "Magic DT%",
    MagicDamage => "Magic Damage",
    MagicDamageTaken => "Magic Damage Taken",
    MagicDefense => "Magic Defense",
    MagicEvasion => "Magic Evasion",
    MagicHaste => "Magic Haste",
    MagicDamageLegacyLower => "Magic damage",
    MarksmanshipSkill => "Marksmanship Skill",
    MartialArts => "Martial Arts",
    Name => "Name",
    Name2 => "Name2",
    NinjutsuDamagePct => "Ninjutsu Damage%",
    NinjutsuMagicAccuracy => "Ninjutsu Magic Accuracy",
    NinjutsuMagicAttack => "Ninjutsu Magic Attack",
    NinjutsuMagicDamage => "Ninjutsu Magic Damage",
    NinjutsuSkill => "Ninjutsu Skill",
    NoneSkill => "None Skill",
    Oa2 => "OA2",
    Oa2Main => "OA2 main",
    Oa2Sub => "OA2 sub",
    Oa3 => "OA3",
    Oa3Main => "OA3 main",
    Oa3Sub => "OA3 sub",
    Oa4 => "OA4",
    Oa4Main => "OA4 main",
    Oa4Sub => "OA4 sub",
    Oa5 => "OA5",
    Oa5Main => "OA5 main",
    Oa5Sub => "OA5 sub",
    Oa6 => "OA6",
    Oa6Main => "OA6 main",
    Oa6Sub => "OA6 sub",
    Oa7 => "OA7",
    Oa7Main => "OA7 main",
    Oa7Sub => "OA7 sub",
    Oa8 => "OA8",
    Oa8Main => "OA8 main",
    Oa8Sub => "OA8 sub",
    OccultAcumen => "Occult Acumen",
    Pdl => "PDL",
    PdlTrait => "PDL Trait",
    Pdt => "PDT",
    Pdt2 => "PDT2",
    PetAgi => "Pet:AGI",
    PetAccuracy => "Pet:Accuracy",
    PetAttack => "Pet:Attack",
    PetChr => "Pet:CHR",
    PetCritRate => "Pet:Crit Rate",
    PetDa => "Pet:DA",
    PetDex => "Pet:DEX",
    PetDt => "Pet:DT",
    PetDamageDealtPct => "Pet:Damage Dealt%",
    PetEvasion => "Pet:Evasion",
    PetGearHaste => "Pet:Gear Haste",
    PetInt => "Pet:INT",
    PetLevel => "Pet:Level",
    PetMdt => "Pet:MDT",
    PetMnd => "Pet:MND",
    PetMagicAccuracy => "Pet:Magic Accuracy",
    PetMagicAttack => "Pet:Magic Attack",
    PetMagicDefense => "Pet:Magic Defense",
    PetMagicEvasion => "Pet:Magic Evasion",
    PetPdt => "Pet:PDT",
    PetRangedAccuracy => "Pet:Ranged Accuracy",
    PetRangedAttack => "Pet:Ranged Attack",
    PetStr => "Pet:STR",
    PetStoreTp => "Pet:Store TP",
    PetSubtleBlow => "Pet:Subtle Blow",
    PetTpBonus => "Pet:TP Bonus",
    PetVit => "Pet:VIT",
    PolearmSkill => "Polearm Skill",
    Qa => "QA",
    QuadShot => "Quad Shot",
    QuickDrawDamage => "Quick Draw Damage",
    QuickDrawDamagePct => "Quick Draw Damage%",
    QuickDrawMagicAccuracy => "Quick Draw Magic Accuracy",
    RangedAccuracy => "Ranged Accuracy",
    RangedAttack => "Ranged Attack",
    RangedAttackPct => "Ranged Attack%",
    RangedCritDamage => "Ranged Crit Damage",
    RangedCritRate => "Ranged Crit Rate",
    RangedDmg => "Ranged DMG",
    RangedDelay => "Ranged Delay",
    Rank => "Rank",
    Recycle => "Recycle",
    Regain => "Regain",
    Str => "STR",
    ScytheSkill => "Scythe Skill",
    ShieldSkill => "Shield Skill",
    SkillType => "Skill Type",
    SkillchainBonus => "Skillchain Bonus",
    Smite => "Smite",
    SneakAttackBonus => "Sneak Attack Bonus",
    StaffSkill => "Staff Skill",
    StoreTp => "Store TP",
    StrikingCritRate => "Striking Crit Rate",
    SubtleBlow => "Subtle Blow",
    SubtleBlowIi => "Subtle Blow II",
    SummoningMagicSkill => "Summoning Magic Skill",
    SwordSkill => "Sword Skill",
    Ta => "TA",
    TaAttack => "TA Attack",
    TaDamagePct => "TA Damage%",
    TpBonus => "TP Bonus",
    ThrowingSkill => "Throwing Skill",
    ThunderAffinity => "Thunder Affinity",
    ThunderElementalBonus => "Thunder Elemental Bonus",
    TrickAttackBonus => "Trick Attack Bonus",
    TripleShot => "Triple Shot",
    TripleShotDamagePct => "Triple Shot Damage%",
    TrueShot => "True Shot",
    Type => "Type",
    Vit => "VIT",
    Wsc => "WSC",
    WaterAffinity => "Water Affinity",
    WaterElementalBonus => "Water Elemental Bonus",
    WeaponSkillAccuracy => "Weapon Skill Accuracy",
    WeaponSkillDamage => "Weapon Skill Damage",
    WeaponSkillDamageTrait => "Weapon Skill Damage Trait",
    Weather => "Weather",
    WindAffinity => "Wind Affinity",
    WindElementalBonus => "Wind Elemental Bonus",
    Zanhasso => "Zanhasso",
    Zanshin => "Zanshin",
    ZanshinAttack => "Zanshin Attack",
    ZanshinOa2 => "Zanshin OA2",
    DarkAffinityLower => "dark Affinity",
    EarthAffinityLower => "earth Affinity",
    FireAffinityLower => "fire Affinity",
    Ftp => "ftp",
    IceAffinityLower => "ice Affinity",
    LightAffinityLower => "light Affinity",
    MainAxeSkill => "main Axe Skill",
    MainBlueMagicSkill => "main Blue Magic Skill",
    MainClubSkill => "main Club Skill",
    MainDaggerSkill => "main Dagger Skill",
    MainDarkMagicSkill => "main Dark Magic Skill",
    MainDivineMagicSkill => "main Divine Magic Skill",
    MainElementalMagicSkill => "main Elemental Magic Skill",
    MainEvasionSkill => "main Evasion Skill",
    MainGreatAxeSkill => "main Great Axe Skill",
    MainGreatKatanaSkill => "main Great Katana Skill",
    MainGreatSwordSkill => "main Great Sword Skill",
    MainHandToHandSkill => "main Hand-to-Hand Skill",
    MainKatanaSkill => "main Katana Skill",
    MainMagicAccuracySkill => "main Magic Accuracy Skill",
    MainNinjutsuSkill => "main Ninjutsu Skill",
    MainPolearmSkill => "main Polearm Skill",
    MainScytheSkill => "main Scythe Skill",
    MainStaffSkill => "main Staff Skill",
    MainSummoningMagicSkill => "main Summoning Magic Skill",
    MainSwordSkill => "main Sword Skill",
    SubAxeSkill => "sub Axe Skill",
    SubBlueMagicSkill => "sub Blue Magic Skill",
    SubClubSkill => "sub Club Skill",
    SubDaggerSkill => "sub Dagger Skill",
    SubDarkMagicSkill => "sub Dark Magic Skill",
    SubDivineMagicSkill => "sub Divine Magic Skill",
    SubElementalMagicSkill => "sub Elemental Magic Skill",
    SubEvasionSkill => "sub Evasion Skill",
    SubGreatAxeSkill => "sub Great Axe Skill",
    SubGreatKatanaSkill => "sub Great Katana Skill",
    SubGreatSwordSkill => "sub Great Sword Skill",
    SubHandToHandSkill => "sub Hand-to-Hand Skill",
    SubKatanaSkill => "sub Katana Skill",
    SubMagicAccuracySkill => "sub Magic Accuracy Skill",
    SubNinjutsuSkill => "sub Ninjutsu Skill",
    SubPolearmSkill => "sub Polearm Skill",
    SubScytheSkill => "sub Scythe Skill",
    SubStaffSkill => "sub Staff Skill",
    SubSummoningMagicSkill => "sub Summoning Magic Skill",
    SubSwordSkill => "sub Sword Skill",
    ThunderAffinityLower => "thunder Affinity",
    WaterAffinityLower => "water Affinity",
    WindAffinityLower => "wind Affinity",
    ; gear: Fua, AutomatonSkill, PetMagicEvasion, PetMagicDefense, PetStr, PetDex, PetAgi, PetVit, PetInt, PetMnd, PetChr, PetLevel, PetDamageDealtPct, PetCritRate, PetTpBonus, PetMagicAttack, PetEvasion, PetPdt, PetSubtleBlow, PetMdt, PetStoreTp, PetDt, PetMagicAccuracy, PetGearHaste, PetRangedAccuracy, PetRangedAttack, PetDa, PetAttack, PetAccuracy, InninDaPct, ElementalWsDamagePct, EnSpellDamagePct, EnSpellDamage, ConserveTp, Regain, Barrage, BarrageRangedAccuracy, BarrageRangedAttack, StrikingCritRate, ClimacticCritDamage, KlimaformDamagePct, EbullienceBonus, OccultAcumen, FutaeBonus, Wsc, ZanshinOa2, Recycle, DoubleShotDamagePct, TripleShotDamagePct, SubtleBlow, SubtleBlowIi, RangedCritDamage, MagicDefense, MagicEvasion, Evasion, Pdt2, Mdt2, Pdt, Mdt, Dt, BloodPactDamage, Rank, KickAttacks, KickAttacksDmg, MartialArts, SneakAttackBonus, TrickAttackBonus, DoubleShot, TrueShot, Zanshin, Hasso, QuickDrawDamage, QuickDrawMagicAccuracy, QuickDrawDamagePct, TripleShot, MagicCritRateIi, MagicBurstAccuracy, Fencer, JaHaste, HassoPlusJaHaste, Accuracy, Agi, Attack, AxeSkill, Chr, ClubSkill, CritDamage, CritRate, Da, DaDamagePct, DaggerSkill, Daken, DarkAffinity, DarkElementalBonus, Delay, Dex, Dmg, DualWield, EarthAffinity, EarthElementalBonus, ElementalBonus, ElementalMagicSkill, FireAffinity, FireElementalBonus, Ftp, GearHaste, GreatAxeSkill, GreatKatanaSkill, GreatSwordSkill, HandToHandSkill, IceAffinity, IceElementalBonus, Int, Jobs, KatanaSkill, LightAffinity, LightElementalBonus, MagicAccuracySkill, MagicAccuracy, MagicAttack, MagicBurstDamageIi, MagicBurstDamage, MagicDamage, Mnd, Name, Name2, NinjutsuDamagePct, NinjutsuMagicAttack, NinjutsuMagicAccuracy, NinjutsuSkill, Oa2, Oa3, Oa4, Oa5, Oa6, Oa7, Oa8, Pdl, PolearmSkill, Qa, RangedAccuracy, RangedAttack, ScytheSkill, SkillType, SkillchainBonus, StaffSkill, StoreTp, Str, SwordSkill, Ta, TaDamagePct, ThrowingSkill, ThunderAffinity, ThunderElementalBonus, TpBonus, Type, Vit, WaterAffinity, WaterElementalBonus, WeaponSkillAccuracy, WeaponSkillDamage, Weather, WindAffinity, WindElementalBonus, MarksmanshipSkill, ArcherySkill
}

const PRESENCE_WORDS: usize = STAT_COUNT.div_ceil(64);

#[derive(Clone, Debug)]
pub struct Stats {
    values: [f64; STAT_COUNT],
    presence: [u64; PRESENCE_WORDS],
    order: [Stat; STAT_COUNT],
    len: u16,
}

impl Default for Stats {
    fn default() -> Self {
        Self { values: [0.0; STAT_COUNT], presence: [0; PRESENCE_WORDS], order: [Stat::ALL[0]; STAT_COUNT], len: 0 }
    }
}

impl Stats {
    #[inline]
    pub fn contains(&self, stat: Stat) -> bool { self.presence[stat.index() / 64] & (1 << (stat.index() % 64)) != 0 }
    #[inline]
    pub fn get(&self, stat: Stat) -> f64 { self.get_or(stat, 0.0) }
    #[inline]
    pub fn get_or(&self, stat: Stat, default: f64) -> f64 { if self.contains(stat) { self.values[stat.index()] } else { default } }
    pub fn require(&self, stat: Stat, operation: &'static str) -> Result<f64, CalcError> {
        if self.contains(stat) { Ok(self.values[stat.index()]) }
        else { Err(CalcError::legacy(operation, stat.name(), "KeyError", format!("'{}'", stat.name()))) }
    }
    #[inline]
    pub fn set(&mut self, stat: Stat, amount: f64) {
        if !self.contains(stat) {
            self.presence[stat.index() / 64] |= 1 << (stat.index() % 64);
            self.order[self.len as usize] = stat;
            self.len += 1;
        }
        self.values[stat.index()] = amount;
    }
    #[inline]
    pub fn add(&mut self, stat: Stat, amount: f64) { self.set(stat, self.get(stat) + amount); }
    pub fn remove(&mut self, stat: Stat) -> Option<f64> {
        if !self.contains(stat) { return None; }
        let value = self.values[stat.index()];
        self.presence[stat.index() / 64] &= !(1 << (stat.index() % 64));
        let end = self.len as usize;
        let index = self.order[..end].iter().position(|item| *item == stat).expect("present stat has insertion position");
        self.order.copy_within(index + 1..end, index);
        self.len -= 1;
        Some(value)
    }
    pub fn clear(&mut self) { self.presence.fill(0); self.len = 0; }
    pub fn len(&self) -> usize { self.len as usize }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    pub fn ordered(&self) -> impl ExactSizeIterator<Item = (Stat, f64)> + DoubleEndedIterator + '_ {
        self.order[..self.len as usize].iter().map(|stat| (*stat, self.values[stat.index()]))
    }
    pub fn sort_alphabetically(&mut self) { self.order[..self.len as usize].sort_unstable(); }
}

impl PartialEq for Stats {
    fn eq(&self, other: &Self) -> bool {
        self.presence == other.presence && self.ordered().all(|(stat, value)| value == other.values[stat.index()])
    }
}

impl Serialize for Stats {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (stat, value) in self.ordered() { map.serialize_entry(stat.name(), &crate::model::LegacyF64(value))?; }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Stats {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StatsVisitor;
        impl<'de> Visitor<'de> for StatsVisitor {
            type Value = Stats;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result { formatter.write_str("an ordered numeric stat object") }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut stats = Stats::default();
                while let Some((name, value)) = map.next_entry::<String, crate::model::LegacyF64>()? {
                    let stat = Stat::from_name(&name).ok_or_else(|| serde::de::Error::custom(format!("unknown stat {name:?}")))?;
                    stats.set(stat, value.0);
                }
                Ok(stats)
            }
        }
        deserializer.deserialize_map(StatsVisitor)
    }
}
