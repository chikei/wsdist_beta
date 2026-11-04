//! Optional candidate pruning: drop armor and accessories whose stats cannot change the score.
//!
//! Stat relevance mirrors what the round, weapon-skill and spell evaluators and player
//! construction read for one optimize target. Stats not classified here count as relevant.

use std::sync::Arc;

use crate::actions::optimizer_spell_type;
use crate::catalog::Catalog;
use crate::model::{
    Ability, ActionRequest, CandidatePools, GearId, Job, MINIMUM_STATS, MetricKind, OptimizeRequest, Slot, SpellType,
    WsType, is_artifact_armor, set_bonus_stats,
};
use crate::stats::{STAT_COUNT, Stat};
use crate::weaponskills::DStatFormula;

use super::helpers::{dt_sum, dt_vector, piece_dt, stat_requirements};
use super::legality::{jse_ear_names, optimizer_ws_type};

/// Weapon slots stay whole: kind, skill, damage and delay drive legality and the action
/// itself, and many weapon effects are keyed by name rather than by stats.
const PRUNED_SLOTS: [Slot; 12] = [Slot::Head, Slot::Neck, Slot::Ear1, Slot::Ear2, Slot::Body, Slot::Hands,
    Slot::Ring1, Slot::Ring2, Slot::Back, Slot::Waist, Slot::Legs, Slot::Feet];

/// Armor and accessories that player construction, the evaluators or legality check by name.
const NAMED_EFFECTS: &[&str] = &[
    "Hachirin-no-Obi", "Fotia Gorget", "Fotia Belt", "Regal Ring", "Regal Earring", "Balder Earring",
    "Arcadian Beret", "Arcadian Jerkin", "Amini Caban", "Belenus", "Bhikku Gaiters", "Bhikku Cyclas",
    "Skulker's Vest", "Lanun Gants", "Horos Toe Shoes", "Cohort Cloak", "Crepuscular Cloak", "Twilight Cloak",
];

/// Candidate pools without armor and accessories that cannot affect `request`'s score,
/// damage-taken requirements or required minimum stats.
///
/// A slot that loses candidates keeps one of them as a score-neutral choice unless Empty is
/// already a candidate, so every set of the original pools has an equally scored legal set in
/// the pruned pools. Weapon, ranged and ammo pools are returned unchanged.
pub fn prune_candidates(request: &OptimizeRequest, catalog: &Catalog) -> CandidatePools {
    let relevance = Relevance::new(request, catalog);
    let main_job = request.player.main_job;
    let mut pools = request.candidates.clone();
    let mut neutral = [None; Slot::COUNT];
    for slot in PRUNED_SLOTS {
        let pool = &mut pools[slot.index()];
        let kept: Vec<bool> = pool.iter().map(|&id| relevance.keeps(catalog, id, slot)).collect();
        // A paired slot avoids its partner's choice so both may hold neutral pieces.
        let partner = match slot {
            Slot::Ring2 => neutral[Slot::Ring1.index()],
            Slot::Ear2 => neutral[Slot::Ear1.index()],
            _ => None,
        };
        let stand_in = if pool.contains(&catalog.empty) { None } else {
            let mut pruned = pool.iter().zip(&kept)
                .filter(|&(&id, &keep)| !keep && catalog.piece(id).eligible_for(main_job))
                .map(|(&id, _)| id);
            let first = pruned.next();
            if first.is_some() && first == partner { pruned.find(|&id| Some(id) != partner).or(first) } else { first }
        };
        neutral[slot.index()] = stand_in;
        let mut index = 0;
        pool.retain(|&id| {
            let keep = kept[index] || Some(id) == stand_in;
            index += 1;
            keep
        });
    }
    pools
}

struct Relevance {
    stats: [bool; STAT_COUNT],
    weapon_skill: bool,
    main_job: Job,
    /// Main-job artifact armor can raise accuracy through a Regal Ring/Earring candidate.
    artifact_counts: bool,
    jse_ears: Vec<Arc<str>>,
}

impl Relevance {
    fn new(request: &OptimizeRequest, catalog: &Catalog) -> Self {
        let scope = Scope::new(request, catalog);
        let stats = std::array::from_fn(|index| scope.relevant(Stat::ALL[index]));
        let accuracy = [Stat::Accuracy, Stat::RangedAccuracy, Stat::MagicAccuracy].iter().any(|stat| stats[stat.index()]);
        let regal = [Slot::Ring1, Slot::Ring2, Slot::Ear1, Slot::Ear2].iter().any(|slot| {
            request.candidates[slot.index()].iter().chain([&request.player.gearset[slot.index()]])
                .any(|&id| matches!(catalog.piece(id).name.as_ref(), "Regal Ring" | "Regal Earring"))
        });
        Self { stats, weapon_skill: scope.ws, main_job: request.player.main_job,
            artifact_counts: regal && accuracy, jse_ears: jse_ear_names() }
    }

    fn keeps(&self, catalog: &Catalog, id: GearId, slot: Slot) -> bool {
        let piece = catalog.piece(id);
        id == catalog.empty
            || piece.stats.iter().any(|entry| entry.value != 0.0 && self.stats[entry.stat.index()])
            || (self.weapon_skill && piece.wsc.is_some())
            || NAMED_EFFECTS.iter().any(|name| piece.name.contains(name))
            || self.jse_ears.contains(&piece.name)
            || (self.artifact_counts && is_artifact_armor(&piece.name, slot, self.main_job))
            || set_bonus_stats(&piece.name, slot).any(|stat| self.stats[stat.index()])
    }
}

/// Which mechanics of the optimize target can move its score.
#[derive(Default)]
struct Scope {
    round: bool,
    ws: bool,
    /// Physical melee swings: rounds and non-magical melee weapon skills.
    melee: bool,
    /// Physical ranged shots: non-magical ranged weapon skills, Ranged Attack and NIN Daken.
    ranged: bool,
    ranged_ws: bool,
    ranged_spell: bool,
    quick_draw: bool,
    /// Magical or hybrid weapon-skill damage.
    ws_magic: bool,
    /// Every non-ranged spell, including Ninjutsu and Quick Draw.
    spell_magic: bool,
    /// Any magic damage or accuracy term, including round EnSpells and the magic-accuracy metric.
    magic: bool,
    /// The metric includes TP return.
    tp: bool,
    trick: bool,
    flourish: bool,
    /// Attributes read by weapon-skill WSC, dSTAT or gear WSC.
    scaling: Vec<Stat>,
    /// PDT and MDT requirements some candidate set could miss.
    dt: [bool; 2],
    /// Sheet stats read by [`MINIMUM_STATS`] with a requested minimum.
    required: Vec<Stat>,
}

impl Scope {
    fn new(request: &OptimizeRequest, catalog: &Catalog) -> Self {
        let abilities = &request.player.abilities;
        let mut scope = Self {
            trick: abilities.enabled(Ability::TrickAttack),
            flourish: [Ability::ClimacticFlourish, Ability::StrikingFlourish, Ability::TernaryFlourish]
                .into_iter().any(|ability| abilities.enabled(ability)),
            dt: dt_binding(request, catalog),
            required: MINIMUM_STATS.into_iter().zip(stat_requirements(request.min_stats))
                .filter(|(_, minimum)| minimum.is_finite()).flat_map(|(stat, _)| stat.stats().iter().copied()).collect(),
            ..Self::default()
        };
        match &request.action {
            ActionRequest::Round { .. } => {
                scope.round = true;
                scope.melee = true;
                scope.tp = true;
                scope.ranged = request.player.main_job == Job::Nin;
                scope.magic = abilities.enabled(Ability::EnSpell);
            }
            ActionRequest::Ws(ws) => {
                // The search classifies the weapon skill by name, exactly like this.
                let ws_type = optimizer_ws_type(&request.ws_name);
                let spec = ws.skill.spec();
                scope.ws = true;
                scope.melee = ws_type != WsType::Ranged && !spec.magical;
                scope.ranged = ws_type != WsType::Melee && !spec.magical;
                scope.ranged_ws = ws_type != WsType::Melee;
                scope.ws_magic = spec.magical || spec.hybrid;
                scope.magic = scope.ws_magic || ws.metric.kind == MetricKind::MagicAccuracy;
                scope.tp = ws.metric.kind == MetricKind::TpReturn;
                scope.scaling.extend(spec.wsc.terms().iter().map(|&(_, stat)| stat));
                if spec.magical {
                    scope.scaling.extend(match spec.dstat {
                        DStatFormula::Zero => None,
                        DStatFormula::IntCapped | DStatFormula::IntDouble => Some(Stat::Int),
                        DStatFormula::AgiDouble | DStatFormula::Wildfire => Some(Stat::Agi),
                        DStatFormula::MndDouble => Some(Stat::Mnd),
                        DStatFormula::PrimalRend => Some(Stat::Chr),
                    });
                }
                for (pool, &equipped) in request.candidates.iter().zip(&request.player.gearset) {
                    scope.scaling.extend(pool.iter().chain([&equipped]).filter_map(|&id| catalog.piece(id).wsc).map(|wsc| wsc.stat));
                }
            }
            ActionRequest::Spell(spell) => {
                // The search classifies the spell by name, exactly like this.
                let spell_type = optimizer_spell_type(&request.spell_name);
                scope.ranged_spell = spell_type == SpellType::RangedAttack;
                scope.ranged = scope.ranged_spell;
                scope.quick_draw = spell_type == SpellType::QuickDraw;
                scope.spell_magic = !scope.ranged_spell;
                scope.magic = scope.spell_magic;
                scope.tp = matches!(spell.metric.kind, MetricKind::TpReturn | MetricKind::DamageThenTp | MetricKind::TpThenDamage);
            }
        }
        scope
    }

    fn scales(&self, stat: Stat) -> bool { self.scaling.contains(&stat) }

    fn relevant(&self, stat: Stat) -> bool {
        use Stat::*;
        let physical = self.melee || self.ranged;
        // A required minimum counts whatever the target reads.
        if self.required.contains(&stat) { return true; }
        match stat {
            // No optimizer path reads pets, the player's own defenses, Subtle Blow or skillchains.
            PetAgi | PetAccuracy | PetAttack | PetChr | PetCritRate | PetDa | PetDex | PetDt | PetDamageDealtPct
            | PetEvasion | PetGearHaste | PetInt | PetLevel | PetMdt | PetMnd | PetMagicAccuracy | PetMagicAttack
            | PetMagicDefense | PetMagicEvasion | PetPdt | PetRangedAccuracy | PetRangedAttack | PetStr | PetStoreTp
            | PetSubtleBlow | PetTpBonus | PetVit | AutomatonSkill | BloodPactDamage
            | Evasion | EvasionSkill | MagicEvasion | MagicDefense | SubtleBlow | SubtleBlowIi | SkillchainBonus => false,
            Pdt | Pdt2 => self.dt[0],
            Mdt | Mdt2 => self.dt[1],
            Dt | Dt2 => self.dt[0] || self.dt[1],
            GearHaste | JaHaste | HassoPlusJaHaste | Regain | Zanshin | ZanshinOa2 | Daken | KickAttacks | Fua
            | TaDamagePct | DaDamagePct => self.round,
            WeaponSkillDamage | TpBonus | Ftp | WeaponSkillAccuracy | ElementalWsDamagePct | Wsc => self.ws,
            ConserveTp => self.ws && self.tp,
            // Fencer grants TP Bonus and Crit Rate.
            Fencer => self.round || self.ws || self.ranged_spell,
            // Accuracy, Attack and combat skills only feed the melee Accuracy1/2 and Attack1/2.
            Accuracy | Attack | Da | Ta | Qa | Oa2 | Oa3 | Oa4 | Oa5 | Oa6 | Oa7 | Oa8 | InninDaPct | KickAttacksDmg
            | Smite | SneakAttackBonus | TrickAttackBonus | ClimacticCritDamage | StrikingCritRate
            | HandToHandSkill | DaggerSkill | SwordSkill | GreatSwordSkill | AxeSkill | GreatAxeSkill | ScytheSkill
            | PolearmSkill | KatanaSkill | GreatKatanaSkill | ClubSkill | StaffSkill => self.melee,
            CritRate | CritDamage | Pdl => physical,
            // Weapon-skill and spell damage never reads TP gain.
            StoreTp | DualWield | MartialArts => self.tp,
            OccultAcumen => self.spell_magic && self.tp,
            Recycle => self.ranged_spell && self.tp,
            RangedAccuracy | RangedAttack | ArcherySkill | MarksmanshipSkill | ThrowingSkill => self.ranged,
            TrueShot | RangedCritRate | RangedCritDamage => self.ranged || self.ranged_ws,
            Barrage | BarrageRangedAccuracy | BarrageRangedAttack | DoubleShot | DoubleShotDamagePct | TripleShot
            | TripleShotDamagePct | QuadShot => self.ranged_spell,
            MagicAttack | MagicDamage | MagicAccuracy | MagicAccuracySkill | MagicCritRateIi | KlimaformDamagePct
            | EnSpellDamage | EnSpellDamagePct | ElementalBonus | DarkElementalBonus | EarthElementalBonus
            | FireElementalBonus | IceElementalBonus | LightElementalBonus | LightningElementalBonus
            | ThunderElementalBonus | WaterElementalBonus | WindElementalBonus | DarkAffinity | EarthAffinity
            | FireAffinity | IceAffinity | LightAffinity | ThunderAffinity | WaterAffinity | WindAffinity
            | DarkAffinityLower | EarthAffinityLower | FireAffinityLower | IceAffinityLower | LightAffinityLower
            | ThunderAffinityLower | WaterAffinityLower | WindAffinityLower => self.magic,
            MagicBurstDamage | MagicBurstDamageIi | MagicBurstAccuracy | ElementalMagicSkill | DarkMagicSkill
            | DivineMagicSkill | NinjutsuSkill | BlueMagicSkill | BlueMagciSkill | SummoningMagicSkill
            | HelixMagicAccuracy | HelixMagicAttack | EbullienceBonus | FutaeBonus | NinjutsuDamagePct
            | NinjutsuMagicAccuracy | NinjutsuMagicAttack | NinjutsuMagicDamage | QuickDrawDamage
            | QuickDrawDamagePct | QuickDrawMagicAccuracy => self.spell_magic,
            Str => physical || self.scales(Str),
            Dex => self.melee || self.scales(Dex),
            Agi => self.ranged || self.ranged_ws || self.quick_draw || (self.melee && self.trick) || self.scales(Agi),
            Int => self.ws_magic || self.spell_magic || self.scales(Int),
            Chr => (self.melee && self.flourish) || self.scales(Chr),
            Vit | Mnd => self.scales(stat),
            _ => true,
        }
    }
}

/// Whether some candidate set could miss the PDT and MDT requirements.
///
/// Bounds `dt_for_player` from above with each slot's largest candidate or equipped
/// contribution plus party effects; player construction only lowers damage taken.
fn dt_binding(request: &OptimizeRequest, catalog: &Catalog) -> [bool; 2] {
    let buffs = dt_sum(request.player.effects.buckets.iter().map(|bucket| dt_vector(&bucket.stats)));
    // [PDT + DT, MDT + DT] floor at -50; [PDT2 + DT2, MDT2 + DT2] add uncapped.
    let mut capped = [buffs[0] + buffs[2], buffs[1] + buffs[2]];
    let mut uncapped = [buffs[3] + buffs[5], buffs[4] + buffs[5]];
    for (pool, &equipped) in request.candidates.iter().zip(&request.player.gearset) {
        let mut capped_max = [f64::NEG_INFINITY; 2];
        let mut uncapped_max = [f64::NEG_INFINITY; 2];
        for &id in pool.iter().chain([&equipped]) {
            let dt = piece_dt(catalog, id);
            for channel in 0..2 {
                capped_max[channel] = capped_max[channel].max(dt[channel] + dt[2]);
                uncapped_max[channel] = uncapped_max[channel].max(dt[channel + 3] + dt[5]);
            }
        }
        for channel in 0..2 {
            capped[channel] += capped_max[channel];
            uncapped[channel] += uncapped_max[channel];
        }
    }
    let requirements = [request.min_pdt, request.min_mdt];
    std::array::from_fn(|channel| capped[channel].max(-50.0) + uncapped[channel] > requirements[channel])
}
