use crate::catalog::Catalog;
use crate::math::python_int;
use crate::stats::Stat;

use super::{
    Ability, CalcError, EvaluationContext, Job, ModelState, PieceKind, Player,
    PlayerInput, PlayerStage, PlayerStageSnapshot, RandomSource, SkillType, Slot,
};

#[path = "player_tables.rs"]
mod tables;

/// The literal skill-to-accuracy windows in create_player.py:348–363.
pub fn get_skill_accuracy(skill: f64) -> Result<f64, CalcError> {
    let mut accuracy = 0.0;
    if skill > 200.0 {
        let capped = if 400.0 < skill { 400.0 } else { skill };
        accuracy += python_int((capped - 200.0) * 0.9, "get_skill_accuracy", "skill_level")? + 200.0;
    } else {
        accuracy += skill;
    }
    if skill > 400.0 {
        let capped = if 600.0 < skill { 600.0 } else { skill };
        accuracy += python_int((capped - 400.0) * 0.8, "get_skill_accuracy", "skill_level")?;
    }
    if skill > 600.0 {
        accuracy += python_int((skill - 600.0) * 0.9, "get_skill_accuracy", "skill_level")?;
    }
    Ok(accuracy)
}

/// Rebuild caller-owned storage, retaining the source's ordered sparse sheet.
/// No random draws or shared-catalog mutations occur during player construction.
pub fn build_player(
    input: &PlayerInput,
    ctx: &mut EvaluationContext<impl RandomSource>,
    out: &mut Player,
) -> Result<(), CalcError> {
    out.main_job = input.main_job;
    out.sub_job = input.sub_job;
    out.master_level = input.master_level.clamp(0, 50);
    out.main_job_level = 99;
    out.sub_job_level = 49 + out.master_level / 5;
    out.gearset = input.gearset;
    out.stats.clear();
    out.wsc.clear();
    out.wyvern_bonus_attack = false;
    out.abilities.clone_from(&input.abilities);
    // Keep existing snapshot/WSC allocations for repeated requested traces.
    if let Some(stages) = &mut ctx.player_stages {
        for stage in stages {
            stage.stats.clear();
            stage.wsc.clear();
        }
    }
    let mut builder = Builder {
        catalog: &ctx.catalog,
        state: &mut ctx.state,
        input,
        out,
        stages: &mut ctx.player_stages,
        stage_count: 0,
    };
    let result = builder.run();
    if let Some(stages) = builder.stages {
        stages.truncate(builder.stage_count);
    }
    result
}

struct Builder<'a> {
    catalog: &'a Catalog,
    state: &'a mut ModelState,
    input: &'a PlayerInput,
    out: &'a mut Player,
    stages: &'a mut Option<Vec<PlayerStageSnapshot>>,
    stage_count: usize,
}

impl Builder<'_> {
    fn run(&mut self) -> Result<(), CalcError> {
        // The capture adapter snapshots completed and exceptional outer helpers.
        let result = self.add_base_stats();
        self.snapshot(PlayerStage::Base);
        result?;
        self.add_gear_stats();
        self.snapshot(PlayerStage::Gear);
        let result = self.add_buffs();
        self.snapshot(PlayerStage::Buffs);
        result?;
        let result = self.finalize_stats();
        self.snapshot(PlayerStage::FinalizeBeforeSort);
        result?;
        self.out.stats.sort_alphabetically();
        self.snapshot(PlayerStage::Final);
        Ok(())
    }

    fn snapshot(&mut self, stage: PlayerStage) {
        let Some(stages) = self.stages.as_mut() else { return; };
        let skills = std::array::from_fn(|slot| self.state.skill(self.catalog.piece(self.out.gearset[slot])));
        if let Some(snapshot) = stages.get_mut(self.stage_count) {
            snapshot.stage = stage;
            snapshot.stats.clone_from(&self.out.stats);
            snapshot.wsc.clone_from(&self.out.wsc);
            snapshot.wyvern_bonus_attack = self.out.wyvern_bonus_attack;
            snapshot.skills = skills;
        } else {
            stages.push(PlayerStageSnapshot {
                stage,
                stats: self.out.stats.clone(),
                wsc: self.out.wsc.clone(),
                wyvern_bonus_attack: self.out.wyvern_bonus_attack,
                skills,
            });
        }
        self.stage_count += 1;
    }

    #[inline]
    fn get(&self, stat: Stat) -> f64 { self.out.stats.get(stat) }
    #[inline]
    fn add(&mut self, stat: Stat, value: f64) { self.out.stats.add(stat, value); }
    #[inline]
    fn set(&mut self, stat: Stat, value: f64) { self.out.stats.set(stat, value); }
    #[inline]
    fn enabled(&self, ability: Ability) -> bool { self.input.abilities.enabled(ability) }
    #[inline]
    fn has_job(&self, job: Job) -> bool { self.out.main_job == job || self.out.sub_job == Some(job) }
    #[inline]
    fn main_skill(&self) -> SkillType { self.state.skill(self.catalog.piece(self.out.gearset[Slot::Main.index()])) }

    fn add_base_stats(&mut self) -> Result<(), CalcError> {
        let main = self.out.main_job as usize;
        let sub = self.out.sub_job.map(|job| job as usize);
        for (index, stat) in tables::PARAMETERS.iter().copied().enumerate() {
            let sub_value = sub.map_or(0.0, |sub| tables::SUB_PARAMETERS[sub][index]);
            self.set(stat, self.get(stat) + tables::BASE_PARAMETERS[main][index] + sub_value + f64::from(self.out.master_level));
        }
        for trait_spec in tables::TRAITS {
            if let Some((_, tiers)) = trait_spec.jobs.iter().find(|(job, _)| *job == self.out.main_job) {
                for &(level, value) in *tiers {
                    if self.out.main_job_level >= level {
                        self.add(trait_spec.stat, value);
                        break;
                    }
                }
            }
            if let Some((_, tiers)) = trait_spec.jobs.iter().find(|(job, _)| Some(*job) == self.out.sub_job) {
                for &(level, value) in *tiers {
                    if trait_spec.stat == Stat::Da && self.out.main_job == Job::Dnc
                        && self.out.sub_job == Some(Job::War) && self.enabled(Ability::SaberDance) {
                        continue;
                    }
                    if self.out.sub_job_level >= level {
                        let old = self.get(trait_spec.stat);
                        self.set(trait_spec.stat, if value > old { value } else { old });
                        break;
                    }
                }
            }
        }
        if self.out.main_job == Job::Bst { self.add(Stat::Accuracy, 50.0); }
        self.add(Stat::RangedAttack, 0.0);
        for &(stat, value) in tables::COMBAT_SKILLS[main] {
            self.set(stat, self.get(stat) + value + f64::from(self.out.master_level));
        }
        for &(stat, value) in tables::MERIT_STATS[main] { self.add(stat, value); }
        if self.out.main_job == Job::Sam {
            self.add(Stat::WeaponSkillDamage, 19.0 * flag(self.enabled(Ability::Overwhelm)));
        }
        if self.out.main_job == Job::Dnc {
            self.add(Stat::Accuracy, 15.0 * flag(self.enabled(Ability::ClosedPosition)));
            self.add(Stat::Evasion, 15.0 * flag(self.enabled(Ability::ClosedPosition)));
        }
        self.add(Stat::CritRate, 5.0 + 5.0);
        for &stat in tables::ALL_COMBAT_SKILLS { self.add(stat, 16.0); }
        for &(stat, value) in tables::MASTERY_STATS[main] { self.add(stat, value); }

        if self.out.main_job == Job::Rdm {
            let temper = python_int((self.input.abilities.enhancing_skill - 300.0) / 10.0, "build_player", "Enhancing Skill")?;
            let temper = if temper > 0.0 { temper } else { 0.0 };
            let temper = if temper < 40.0 { temper } else { 40.0 };
            self.add(Stat::Ta, temper * flag(self.enabled(Ability::TemperIi)));
        }
        if self.out.main_job == Job::Run {
            let temper = python_int((self.input.abilities.enhancing_skill - 300.0) / 10.0, "build_player", "Enhancing Skill")?;
            let temper = if temper > 0.0 { temper } else { 0.0 };
            let temper = if temper < 5.0 { 5.0 } else { temper };
            self.add(Stat::Da, temper * flag(self.enabled(Ability::Temper)));
        }
        if self.has_job(Job::Sch) {
            for stat in [Stat::ElementalMagicSkill, Stat::DarkMagicSkill, Stat::DivineMagicSkill] {
                let old = self.get(stat);
                self.set(stat, if 420.0 > old { 420.0 } else { old });
            }
        }
        if self.out.main_job == Job::Blu {
            for (stat, value) in [
                (Stat::Accuracy, 48.0), (Stat::RangedAccuracy, 48.0), (Stat::MagicAccuracy, 36.0),
                (Stat::StoreTp, 30.0), (Stat::DualWield, 25.0), (Stat::Ta, 5.0), (Stat::CritDamage, 11.0),
                (Stat::SkillchainBonus, 16.0), (Stat::Str, 5.0 + 4.0 + 3.0 + 2.0 - 3.0),
                (Stat::Dex, 8.0 + 6.0 + 2.0 + 4.0 + 4.0 + 1.0 + 8.0 + 4.0), (Stat::Vit, 4.0 + 7.0 + 4.0),
                (Stat::Agi, 5.0 + 2.0 + 1.0), (Stat::Mnd, 4.0 + 2.0), (Stat::Int, 4.0 - 1.0),
                (Stat::Chr, 1.0 + 5.0 - 2.0),
            ] { self.add(stat, value); }
        }
        if self.out.main_job == Job::Drg {
            self.out.wyvern_bonus_attack = true;
            self.add(Stat::WeaponSkillDamageTrait, 10.0);
            self.add(Stat::JaHaste, 10.0);
            self.add(Stat::Da, 15.0);
            self.add(Stat::Attack, 40.0);
        }
        if self.out.main_job == Job::Nin {
            self.add(Stat::StoreTp, 10.0);
            self.add(Stat::SubtleBlow, 10.0);
        }
        Ok(())
    }

    fn add_gear_stats(&mut self) {
        let catalog = self.catalog;
        for slot in Slot::ALL.iter().copied() {
            let piece = catalog.piece(self.out.gearset[slot.index()]);
            for entry in &piece.stats {
                if entry.stat == Stat::TripleShot && self.out.main_job == Job::Rng { continue; }
                if entry.stat == Stat::DoubleShot && self.out.main_job == Job::Cor { continue; }
                let stat = slot_stat(entry.stat, slot).unwrap_or(entry.stat);
                self.add(stat, entry.value);
            }
            if let Some(wsc) = piece.wsc { self.out.wsc.push(wsc); }
        }
        for bonus in tables::ARMOR_BONUSES {
            let mut count = 0;
            for slot in Slot::ALL.iter().copied() {
                let name = &catalog.piece(self.out.gearset[slot.index()]).name;
                if !name.contains(bonus.name) { continue; }
                if bonus.plus1 && !name.contains("+1") { continue; }
                if bonus.slots.is_some_and(|slots| !slots.contains(&slot)) { continue; }
                count += 1;
            }
            let count = count.min(bonus.max_pieces);
            let value = if count >= 2 {
                let pieces = if bonus.counts_first { count } else { count - 1 };
                bonus.per_piece * pieces as f64
            } else { 0.0 };
            for &stat in bonus.stats { self.add(stat, value); }
        }
        let prefix = tables::AF_PREFIX[self.out.main_job as usize].as_bytes();
        let mut af_count = 0;
        for slot in [Slot::Head, Slot::Body, Slot::Hands, Slot::Legs, Slot::Feet] {
            let name = catalog.piece(self.out.gearset[slot.index()]).name.as_bytes();
            if name.windows(prefix.len()).any(|window| window.eq_ignore_ascii_case(prefix)) { af_count += 1; }
        }
        let af_count = af_count.min(5);
        let mut regal_count = 0;
        if [Slot::Ring1, Slot::Ring2].iter().any(|slot| catalog.piece(self.out.gearset[slot.index()]).name.as_ref() == "Regal Ring") {
            regal_count += af_count;
        }
        if [Slot::Ear1, Slot::Ear2].iter().any(|slot| catalog.piece(self.out.gearset[slot.index()]).name.as_ref() == "Regal Earring") {
            regal_count += af_count;
        }
        self.add(Stat::Accuracy, f64::from(regal_count * 15));
        self.add(Stat::RangedAccuracy, f64::from(regal_count * 15));
        self.add(Stat::MagicAccuracy, f64::from(regal_count * 15));
    }

    fn add_buffs(&mut self) -> Result<(), CalcError> {
        let input = self.input;
        for bucket in &input.effects.buckets {
            for (stat, value) in bucket.stats.ordered() {
                if !tables::METADATA.contains(&stat) { self.add(stat, value); }
            }
        }
        self.add_war_mnk_ability_buffs();
        self.add_caster_ability_buffs();
        self.add_thf_pld_drk_ability_buffs();
        self.add_rng_sam_nin_cor_ability_buffs()?;
        self.add_dnc_sch_geo_run_bst_ability_buffs();
        if self.out.main_job == Job::Pup && self.catalog.piece(self.out.gearset[Slot::Main.index()]).name.as_ref() == "Dragon Fangs" {
            self.add(Stat::KickAttacks, 14.0);
        }
        self.add_smite_and_fencer()?;
        self.add_aftermath_buffs()?;
        self.add_universal_party_member_buffs()?;
        Ok(())
    }

    fn add_war_mnk_ability_buffs(&mut self) {
        let catalog = self.catalog;
        let main = catalog.piece(self.out.gearset[Slot::Main.index()]);
        let feet = catalog.piece(self.out.gearset[Slot::Feet.index()]);
        let body = catalog.piece(self.out.gearset[Slot::Body.index()]);
        if self.has_job(Job::War) {
            if self.enabled(Ability::Berserk) {
                self.add(Stat::AttackPct, 0.25 + 0.085 * flag(main.name.as_ref() == "Conqueror") + (100.0 / 1024.0) * flag(self.out.main_job == Job::War));
                self.add(Stat::Attack, 40.0 * flag(self.out.main_job == Job::War));
                self.add(Stat::CritRate, 14.0 * flag(main.name.as_ref() == "Conqueror"));
            }
            if self.enabled(Ability::Aggressor) {
                self.add(Stat::Accuracy, 25.0 + 20.0 * flag(self.out.main_job == Job::War));
            }
            if self.out.main_job == Job::War && self.enabled(Ability::MightyStrikes) {
                self.set(Stat::CritRate, 100.0);
                self.add(Stat::Accuracy, 40.0);
            }
        }
        if self.has_job(Job::Mnk) {
            if self.out.main_job == Job::Mnk {
                if self.enabled(Ability::Focus) {
                    self.add(Stat::CritRate, 20.0);
                    self.add(Stat::Accuracy, 100.0 + 20.0);
                }
                if self.enabled(Ability::Footwork) {
                    self.add(Stat::KickAttacks, 20.0);
                    self.set(Stat::KickAttacksAttackPct, self.get(Stat::KickAttacksAttackPct) + 100.0 / 1024.0 + if feet.name.as_ref() == "Bhikku Gaiters +2" { 130.0 / 1024.0 } else { 160.0 / 1024.0 });
                    self.add(Stat::KickAttacksDmg, 20.0 + 20.0);
                }
                if self.enabled(Ability::Impetus) {
                    self.add(Stat::CritRate, 50.0 * 0.9);
                    self.add(Stat::Attack, (100.0 + 40.0) * 0.9);
                    self.add(Stat::CritDamage, 50.0 * 0.9 * flag(body.name.contains("Bhikku Cyclas")));
                    self.add(Stat::Accuracy, 100.0 * 0.9 * flag(body.name.contains("Bhikku Cyclas")));
                }
            }
            if self.out.sub_job == Some(Job::Mnk) && self.enabled(Ability::Focus) {
                self.add(Stat::CritRate, 20.0 * (1.0 - f64::from(99 - self.out.sub_job_level) / 100.0));
                self.add(Stat::Accuracy, 100.0 * (1.0 - f64::from(99 - self.out.sub_job_level) / 100.0));
            }
        }
    }

    fn add_caster_ability_buffs(&mut self) {
        if self.out.main_job == Job::Blm {
            if self.enabled(Ability::Manafont) { self.add(Stat::MagicDamage, 60.0); }
            if self.enabled(Ability::Manawell) { self.add(Stat::MagicDamage, 20.0); }
        }
        if self.out.main_job == Job::Rdm {
            if self.enabled(Ability::Chainspell) { self.add(Stat::MagicDamage, 40.0); }
            if self.enabled(Ability::Composure) {
                self.add(Stat::Accuracy, 20.0 + 50.0);
                self.add(Stat::EnSpellDamagePct, 200.0);
                self.set(Stat::EnSpellDamage, self.get(Stat::EnSpellDamage));
            }
        }
    }

    fn add_thf_pld_drk_ability_buffs(&mut self) {
        let catalog = self.catalog;
        let body = catalog.piece(self.out.gearset[Slot::Body.index()]);
        if self.out.main_job == Job::Thf && self.enabled(Ability::Conspirator) {
            self.add(Stat::Accuracy, 25.0 + 20.0);
            self.add(Stat::SubtleBlow, 50.0);
            self.add(Stat::Attack, 25.0 * flag(body.name.contains("Skulker's Vest")));
        }
        if self.out.main_job == Job::Pld {
            if self.enabled(Ability::DivineEmblem) { self.add(Stat::MagicDamage, 40.0); }
            if self.enabled(Ability::EnlightIi) {
                let skill = self.input.abilities.enhancing_skill;
                let skill = if skill < 0.0 { 0.0 } else { skill };
                let accuracy = if skill <= 500.0 {
                    2.0 * ((skill + 85.0) / 13.0) + ((skill + 85.0) / 26.0)
                } else {
                    2.0 * ((skill + 400.0) / 20.0) + ((skill + 400.0) / 40.0)
                };
                self.add(Stat::Accuracy, accuracy * 0.80 + 20.0);
            }
        }
        if self.has_job(Job::Drk) {
            if self.enabled(Ability::LastResort) {
                self.add(Stat::AttackPct, 256.0 / 1024.0 + 100.0 / 1024.0 * flag(self.out.main_job == Job::Drk));
                self.add(Stat::Attack, 40.0 * flag(self.out.main_job == Job::Drk));
                let haste = if two_handed(self.main_skill()) {
                    self.get(Stat::JaHaste) + 15.0 + 10.0 * flag(self.out.main_job == Job::Drk)
                } else { self.get(Stat::JaHaste) };
                self.set(Stat::JaHaste, haste);
            }
            if self.out.main_job == Job::Drk && self.enabled(Ability::EndarkIi) {
                let skill = self.input.abilities.enhancing_skill;
                self.add(Stat::Accuracy, 20.0);
                self.add(Stat::Attack, (((skill + 20.0) / 13.0 + 5.0) * 2.5) * 0.80 + 20.0);
            }
        }
    }

    fn add_rng_sam_nin_cor_ability_buffs(&mut self) -> Result<(), CalcError> {
        let catalog = self.catalog;
        let body = catalog.piece(self.out.gearset[Slot::Body.index()]);
        let back = catalog.piece(self.out.gearset[Slot::Back.index()]);
        let ammo = catalog.piece(self.out.gearset[Slot::Ammo.index()]);
        let hands = catalog.piece(self.out.gearset[Slot::Hands.index()]);
        if self.has_job(Job::Rng) {
            if self.enabled(Ability::Sharpshot) {
                self.add(Stat::RangedAccuracy, 40.0);
                self.add(Stat::RangedAttack, 40.0 * flag(self.out.main_job == Job::Rng));
            }
            if self.out.main_job == Job::Rng {
                if self.enabled(Ability::Barrage) { self.add(Stat::RangedAttack, 60.0); }
                if self.enabled(Ability::VelocityShot) {
                    self.add(Stat::RangedAttack, 40.0);
                    self.set(Stat::JaHaste, self.get(Stat::JaHaste) - 15.0);
                    self.add(Stat::RangedAttackPct, 152.0 / 1024.0 + 112.0 / 1024.0 * flag(body.name.as_ref() == "Amini Caban +3") + 92.0 / 1024.0 * flag(body.name.as_ref() == "Amini Caban +2") + 20.0 / 1024.0 * flag(back.name.contains("Belenus")));
                }
                if self.enabled(Ability::DoubleShot) {
                    self.add(Stat::DoubleShot, 40.0 + 5.0 * flag(body.name.contains("Arcadian Jerkin")));
                    if body.name.contains("Arcadian Jerkin") {
                        self.set(Stat::TripleShot, self.get(Stat::DoubleShot) / 2.0);
                        self.set(Stat::DoubleShot, self.get(Stat::DoubleShot) / 2.0);
                    }
                } else {
                    self.set(Stat::DoubleShot, 0.0 + 5.0 * flag(body.name.contains("Arcadian Jerkin")));
                    self.set(Stat::TripleShot, 0.0);
                }
                if self.enabled(Ability::HoverShot) { self.add(Stat::RangedAccuracy, 100.0); }
            }
        }
        if self.has_job(Job::Sam) && self.enabled(Ability::Hasso) && two_handed(self.main_skill()) {
            if self.out.main_job == Job::Sam {
                self.add(Stat::Str, 14.0 + 20.0);
                let zanshin = self.get(Stat::Zanshin);
                self.set(Stat::Zanshin, if zanshin > 100.0 { 100.0 } else { zanshin });
                self.set(Stat::Zanhasso, self.get(Stat::Zanshin) * 0.35);
            } else {
                self.add(Stat::Str, python_int(f64::from(self.out.sub_job_level) / 7.0, "build_player", "sub_job_level")?);
            }
            self.add(Stat::JaHaste, 10.0 + self.get(Stat::HassoPlusJaHaste));
            self.add(Stat::Accuracy, 10.0);
        }
        if self.out.main_job == Job::Nin {
            if self.enabled(Ability::Sange) && ammo.kind == PieceKind::Shuriken {
                self.add(Stat::RangedAccuracy, 100.0);
                self.set(Stat::Daken, 100.0);
            }
            if self.enabled(Ability::Innin) {
                self.add(Stat::Accuracy, 20.0);
                self.add(Stat::SkillchainBonus, 5.0);
                self.add(Stat::MagicBurstDamageTrait, 5.0);
                self.add(Stat::CritRate, 0.7 * (30.0 - 10.0) + 10.0);
                self.add(Stat::Evasion, 0.7 * (-30.0 - -10.0) + -10.0);
                self.add(Stat::Da, self.get(Stat::InninDaPct));
            }
            if self.enabled(Ability::Futae) { self.add(Stat::NinjutsuMagicDamage, 100.0); }
        }
        if self.out.main_job == Job::Cor {
            if self.enabled(Ability::TripleShot) {
                self.add(Stat::TripleShot, 40.0);
                if hands.name.contains("Lanun Gants") {
                    self.set(Stat::QuadShot, self.get(Stat::TripleShot) / 2.0);
                    self.set(Stat::TripleShot, self.get(Stat::TripleShot) / 2.0);
                }
            } else {
                self.set(Stat::TripleShot, 0.0);
                self.set(Stat::QuadShot, 0.0);
            }
        }
        Ok(())
    }

    fn add_dnc_sch_geo_run_bst_ability_buffs(&mut self) {
        let catalog = self.catalog;
        let feet = catalog.piece(self.out.gearset[Slot::Feet.index()]);
        if self.out.main_job == Job::Dnc {
            if self.enabled(Ability::BuildingFlourish) {
                self.add(Stat::CritRate, 10.0);
                self.add(Stat::Accuracy, 40.0);
                self.add(Stat::AttackPct, 0.25);
                self.add(Stat::WeaponSkillDamage, self.get(Stat::BuildingFlourishWsd));
            }
            if self.enabled(Ability::SaberDance) { self.add(Stat::Da, 25.0); }
            if self.enabled(Ability::ClosedPosition) {
                self.add(Stat::StoreTp, (3.0 * 5.0) * flag(feet.name.as_ref() == "Horos Toe Shoes +3" || feet.name.as_ref() == "Horos Toe Shoes +4"));
            }
        }
        if self.out.main_job == Job::Sch {
            if self.enabled(Ability::Ebullience) { self.set(Stat::MagicDamageLegacyLower, self.get(Stat::MagicDamage) + 40.0); }
            if self.enabled(Ability::Enlightenment) {
                self.add(Stat::Int, 20.0);
                self.add(Stat::Mnd, 20.0);
            }
        }
        if self.has_job(Job::Sch) && self.enabled(Ability::Klimaform) { self.add(Stat::MagicAccuracy, 15.0); }
        if self.out.main_job == Job::Geo && self.enabled(Ability::TheurgicFocus) {
            self.add(Stat::RaMagicAttack, 50.0);
            self.set(Stat::RaMagicDamageLegacyLower, self.get(Stat::RaMagicDamage) + 60.0);
        }
        if self.has_job(Job::Run) && self.enabled(Ability::Swordplay) {
            self.add(Stat::Accuracy, 60.0 * 0.9);
            self.add(Stat::Evasion, 60.0 * 0.9);
        }
        if self.out.main_job == Job::Bst {
            if self.enabled(Ability::Rage) { self.add(Stat::AttackPct, 0.50); }
            if self.enabled(Ability::FrenziedRage) { self.add(Stat::AttackPct, 0.25); }
        }
    }

    fn add_smite_and_fencer(&mut self) -> Result<(), CalcError> {
        let skill = self.main_skill();
        if two_handed(skill) || skill == SkillType::HandToHand {
            let level = python_int(self.get(Stat::Smite), "build_player", "Smite")?;
            let index = checked_tier(level, tables::SMITE.len(), "Smite")?;
            self.add(Stat::AttackPct, tables::SMITE[index]);
        }
        let sub = self.catalog.piece(self.out.gearset[Slot::Sub.index()]);
        if matches!(sub.kind, PieceKind::Shield | PieceKind::None)
            && skill != SkillType::HandToHand && !two_handed(skill) {
            let fencer = self.get(Stat::Fencer);
            let level = if fencer > 8.0 { 8.0 } else { python_int(fencer, "build_player", "Fencer")? };
            let index = checked_tier(level, tables::FENCER.len(), "Fencer")?;
            let (tp_bonus, crit) = tables::FENCER[index];
            self.add(Stat::TpBonus, tp_bonus + self.get(Stat::FencerTpBonus));
            self.add(Stat::CritRate, crit);
        }
        Ok(())
    }

    fn add_aftermath_buffs(&mut self) -> Result<(), CalcError> {
        let level = self.input.abilities.aftermath();
        if level <= 0.0 || level.is_nan() { return Ok(()); }
        let catalog = self.catalog;
        let main = catalog.piece(self.out.gearset[Slot::Main.index()]);
        let ranged = catalog.piece(self.out.gearset[Slot::Ranged.index()]);
        if let Some((_, stats)) = tables::RELIC_MAIN.iter().find(|(name, _)| *name == main.name.as_ref()) {
            for &(stat, value) in *stats { self.add(stat, value); }
        } else if let Some((_, stats)) = tables::RELIC_RANGED.iter().find(|(name, _)| *name == ranged.name.as_ref()) {
            for &(stat, value) in *stats { self.add(stat, value); }
        }
        if let Some((_, tiers)) = tables::MYTHIC.iter().find(|(name, _)| *name == main.name.as_ref()) {
            if level == 3.0 {
                self.add(Stat::Oa2Main, 40.0);
                self.add(Stat::Oa3Main, 20.0);
            } else {
                let (stat, value) = tiers[mythic_index(level, self.input.abilities.aftermath_is_integer())?];
                self.add(stat, value);
            }
        }
        if level == 1.0 || level == 2.0 {
            if let Some((_, tiers)) = tables::MYTHIC.iter().find(|(name, _)| *name == ranged.name.as_ref()) {
                let (stat, value) = tiers[mythic_index(level, self.input.abilities.aftermath_is_integer())?];
                self.add(stat, value);
            }
        }
        if let Some((_, stats)) = tables::PRIME_STATS.iter().find(|(name, _)| *name == main.name.as_ref()) {
            let stage = main.name2.split_whitespace().last().ok_or_else(||
                CalcError::legacy("build_player", "Prime Stage", "IndexError", "list index out of range"))?;
            let stage_index = match stage {
                "IV" => 0,
                "V" => 1,
                _ => return Err(CalcError::legacy("build_player", "Prime Stage", "KeyError", format!("'{stage}'"))),
            };
            for &stat in *stats {
                let (_, tiers) = tables::PRIME_TIERS.iter().find(|(candidate, _)| *candidate == stat).expect("prime stat has tiers");
                let tiers = tiers[stage_index];
                let value = if level == 1.0 { (tiers[1] - tiers[0]) * 0.6 + tiers[0] }
                    else if level == 2.0 { (tiers[2] - tiers[1]) * 0.6 + tiers[1] }
                    else { tiers[2] };
                self.add(stat, value);
            }
        }
        Ok(())
    }

    fn add_universal_party_member_buffs(&mut self) -> Result<(), CalcError> {
        if self.enabled(Ability::BloodRage) {
            self.add(Stat::CritRate, 20.0);
            if self.out.main_job == Job::War { self.add(Stat::CritRate, 20.0); }
        }
        if self.enabled(Ability::Warcry) {
            self.add(Stat::AttackPct, python_int(99.0 / 4.0 + 4.75, "build_player", "Warcry")? / 256.0);
            self.add(Stat::TpBonus, 500.0 + 200.0);
            if self.out.main_job == Job::War { self.add(Stat::Attack, 60.0); }
        }
        if self.enabled(Ability::WarcrySub) {
            self.add(Stat::AttackPct, python_int(f64::from(self.out.sub_job_level) / 4.0 + 4.75, "build_player", "Warcry (sub)")? / 256.0);
        }
        if self.enabled(Ability::CrimsonHowl) {
            self.add(Stat::AttackPct, python_int(99.0 / 4.0 + 4.75, "build_player", "Crimson Howl")? / 256.0);
        }
        if self.enabled(Ability::CrystalBlessing) { self.add(Stat::TpBonus, 250.0); }
        if self.enabled(Ability::IfritsFavor) { self.add(Stat::Da, 25.0); }
        if self.enabled(Ability::ShivasFavor) { self.add(Stat::MagicAttack, 39.0); }
        if self.enabled(Ability::RamuhsFavor) { self.add(Stat::CritRate, 23.0); }
        if self.enabled(Ability::HasteSamba) { self.add(Stat::JaHaste, 10.1); }
        else if self.enabled(Ability::HasteSambaSub) { self.add(Stat::JaHaste, 5.1); }
        if self.enabled(Ability::NaturesMeditation) { self.add(Stat::AttackPct, 52.0 / 256.0); }
        if self.enabled(Ability::MightyGuard) {
            self.add(Stat::MagicHaste, 0.15);
            self.add(Stat::MagicDefense, 15.0);
        }
        Ok(())
    }

    fn finalize_stats(&mut self) -> Result<(), CalcError> {
        let dual_wield = self.catalog.piece(self.out.gearset[Slot::Sub.index()]).kind == PieceKind::Weapon
            || self.main_skill() == SkillType::HandToHand;
        self.finalize_offensive_stats()?;
        self.finalize_weapon_dmg_delay(dual_wield);
        self.finalize_haste_and_delay()?;
        Ok(())
    }

    fn finalize_offensive_stats(&mut self) -> Result<(), CalcError> {
        let evasion_skill = self.get(Stat::EvasionSkill);
        self.set(Stat::Evasion, self.get(Stat::Evasion) + python_int(0.5 * self.get(Stat::Agi), "build_player", "AGI")?
            + if evasion_skill > 300.0 { 300.0 + 0.8 * (evasion_skill - 300.0) } else { evasion_skill });
        let main_skill = self.main_skill().skill_stat();
        let sub_skill = self.state.skill(self.catalog.piece(self.out.gearset[Slot::Sub.index()])).skill_stat();
        let ranged_skill = self.state.skill(self.catalog.piece(self.out.gearset[Slot::Ranged.index()])).skill_stat();
        let ammo_skill = self.state.skill(self.catalog.piece(self.out.gearset[Slot::Ammo.index()])).skill_stat();
        self.set(Stat::Attack1, 8.0 + self.get(main_skill) + self.out.stats.require(Stat::Str, "build_player")?
            + self.get(Stat::Attack) + self.slot_skill(main_skill, Slot::Main));
        self.set(Stat::Attack2, 8.0 + self.get(sub_skill) + python_int(0.5 * self.out.stats.require(Stat::Str, "build_player")?, "build_player", "STR")?
            + self.get(Stat::Attack) + self.slot_skill(sub_skill, Slot::Sub));
        if ammo_skill == Stat::ThrowingSkill {
            self.add(Stat::RangedAttack, 8.0 + self.get(ammo_skill) + self.get(Stat::Str));
        } else if matches!(ranged_skill, Stat::MarksmanshipSkill | Stat::ArcherySkill) {
            self.add(Stat::RangedAttack, 8.0 + self.get(ranged_skill) + self.get(Stat::Str));
        } else { self.set(Stat::RangedAttack, 0.0); }
        self.set(Stat::Attack1, self.get(Stat::Attack1) * (1.0 + self.get(Stat::AttackPct)));
        self.set(Stat::Attack2, self.get(Stat::Attack2) * (1.0 + self.get(Stat::AttackPct)));
        self.set(Stat::RangedAttack, self.get(Stat::RangedAttack) * (1.0 + self.get(Stat::RangedAttackPct)));
        self.add(Stat::Attack1, self.get(Stat::FoodAttack));
        self.add(Stat::Attack2, self.get(Stat::FoodAttack));
        self.add(Stat::RangedAttack, self.get(Stat::FoodRangedAttack));
        let main_level = self.get(main_skill) + self.slot_skill(main_skill, Slot::Main);
        let sub_level = self.get(sub_skill) + self.slot_skill(sub_skill, Slot::Sub);
        let ranged_level = self.get(ranged_skill);
        let ammo_level = self.get(ammo_skill);
        self.set(Stat::Accuracy1, python_int(0.75 * self.get(Stat::Dex), "build_player", "DEX")? + self.get(Stat::Accuracy) + get_skill_accuracy(main_level)?);
        self.set(Stat::Accuracy2, python_int(0.75 * self.get(Stat::Dex), "build_player", "DEX")? + self.get(Stat::Accuracy) + get_skill_accuracy(sub_level)?);
        if ammo_skill == Stat::ThrowingSkill {
            self.add(Stat::RangedAccuracy, python_int(0.75 * self.get(Stat::Agi), "build_player", "AGI")? + get_skill_accuracy(ammo_level)?);
        } else if matches!(ranged_skill, Stat::MarksmanshipSkill | Stat::ArcherySkill) {
            self.add(Stat::RangedAccuracy, python_int(0.75 * self.get(Stat::Agi), "build_player", "AGI")? + get_skill_accuracy(ranged_level)?);
        }
        Ok(())
    }

    fn slot_skill(&self, skill: Stat, slot: Slot) -> f64 {
        slot_stat(skill, slot).map_or(0.0, |stat| self.get(stat))
    }

    fn finalize_weapon_dmg_delay(&mut self, dual_wield: bool) {
        let catalog = self.catalog;
        let main = catalog.piece(self.out.gearset[Slot::Main.index()]);
        let sub = catalog.piece(self.out.gearset[Slot::Sub.index()]);
        let ranged = catalog.piece(self.out.gearset[Slot::Ranged.index()]);
        let ammo = catalog.piece(self.out.gearset[Slot::Ammo.index()]);
        self.set(Stat::Delay1, if main.delay != 0.0 { main.delay } else { 480.0 - self.get(Stat::MartialArts) });
        self.set(Stat::Delay2, if sub.delay != 0.0 { sub.delay } else { self.get(Stat::Delay1) });
        self.set(Stat::RangedDelay, ranged.delay);
        self.set(Stat::AmmoDelay, ammo.delay);
        self.set(Stat::Dmg1, main.dmg);
        self.set(Stat::Dmg2, sub.dmg);
        self.set(Stat::RangedDmg, ranged.dmg);
        self.set(Stat::AmmoDmg, ammo.dmg);
        if ammo.kind != PieceKind::Shuriken || self.out.main_job != Job::Nin { self.set(Stat::Daken, 0.0); }
        if self.main_skill() != SkillType::HandToHand {
            self.set(Stat::KickAttacks, 0.0);
            self.set(Stat::MartialArts, 0.0);
        }
        if !two_handed(self.main_skill()) { self.set(Stat::Zanshin, 0.0); }
        if !matches!(ranged.kind, PieceKind::Gun | PieceKind::Bow | PieceKind::Crossbow)
            && !matches!(ammo.kind, PieceKind::Bullet | PieceKind::Arrow | PieceKind::Bolt | PieceKind::Shuriken) {
            self.set(Stat::RangedAttack, 0.0);
            self.set(Stat::RangedAccuracy, 0.0);
        }
        if !self.enabled(Ability::TrueShot) { self.set(Stat::TrueShot, 0.0); }
        if !dual_wield {
            self.set(Stat::Attack2, 0.0);
            self.set(Stat::Accuracy2, 0.0);
            self.set(Stat::Delay2, self.get(Stat::Delay1));
            self.set(Stat::DualWield, 0.0);
        }
    }

    fn finalize_haste_and_delay(&mut self) -> Result<(), CalcError> {
        self.set(Stat::GearHaste, self.get(Stat::GearHaste) / 102.4);
        self.set(Stat::JaHaste, self.get(Stat::JaHaste) / 102.4);
        self.set(Stat::MagicHaste, self.get(Stat::MagicHaste));
        let gear = self.get(Stat::GearHaste);
        let ja = self.get(Stat::JaHaste);
        let magic = self.get(Stat::MagicHaste);
        let haste = (if gear < 0.25 { gear } else { 0.25 }) + (if ja < 0.25 { ja } else { 0.25 })
            + (if magic < 448.0 / 1024.0 { magic } else { 448.0 / 1024.0 });
        let base_delay;
        let reduced_delay;
        if self.main_skill() == SkillType::HandToHand {
            self.set(Stat::Attack2, self.get(Stat::Attack1) - 0.5 * self.out.stats.require(Stat::Str, "build_player")? * (1.0 + self.get(Stat::AttackPct)) * 0.0);
            self.set(Stat::Accuracy2, self.get(Stat::Accuracy1));
            self.state.set_skill(self.out.gearset[Slot::Sub.index()], self.main_skill());
            let base_dmg = 3.0 + python_int((self.get(Stat::HandToHandSkill) + self.get(Stat::MainHandToHandSkill)) * 0.11, "build_player", "Hand-to-Hand Skill")?;
            self.set(Stat::Dmg1, base_dmg + self.get(Stat::Dmg1));
            self.set(Stat::Dmg2, self.get(Stat::Dmg1));
            if self.enabled(Ability::Footwork) { self.set(Stat::KickDmg, self.get(Stat::Dmg1) + self.get(Stat::KickAttacksDmg)); }
            else { self.set(Stat::KickDmg, base_dmg + self.get(Stat::KickAttacksDmg)); }
            self.add(Stat::Delay1, 480.0);
            self.set(Stat::Delay2, self.get(Stat::Delay1));
            base_delay = self.get(Stat::Delay1);
            reduced_delay = (base_delay - self.get(Stat::MartialArts)) * (1.0 - haste);
            self.set(Stat::DualWield, 0.0);
        } else {
            base_delay = (self.get(Stat::Delay1) + self.get(Stat::Delay2)) / 2.0;
            reduced_delay = base_delay * (1.0 - self.get(Stat::DualWield) / 100.0) * (1.0 - haste);
            self.set(Stat::KickDmg, 0.0);
        }
        if base_delay == 0.0 {
            return Err(CalcError::legacy("build_player", "Delay Reduction", "ZeroDivisionError", "float division by zero"));
        }
        let reduction = 1.0 - reduced_delay / base_delay;
        self.set(Stat::DelayReduction, if reduction < 0.8 { reduction } else { 0.8 });
        Ok(())
    }
}

#[inline]
fn flag(value: bool) -> f64 { if value { 1.0 } else { 0.0 } }

#[inline]
fn two_handed(skill: SkillType) -> bool {
    matches!(skill, SkillType::GreatSword | SkillType::GreatKatana | SkillType::GreatAxe
        | SkillType::Polearm | SkillType::Scythe | SkillType::Staff)
}

fn checked_tier(level: f64, len: usize, field: &'static str) -> Result<usize, CalcError> {
    if level < 0.0 || level >= len as f64 {
        return Err(CalcError::legacy("build_player", field, "KeyError", format!("{level:.0}")));
    }
    // python_int checked finiteness and range before this bounded conversion.
    Ok(level as usize)
}

fn mythic_index(level: f64, integer_kind: bool) -> Result<usize, CalcError> {
    if !integer_kind || level.fract() != 0.0 || !level.is_finite() {
        return Err(CalcError::legacy("build_player", "Aftermath", "TypeError", "tuple indices must be integers or slices, not float"));
    }
    let index = level - 1.0;
    if !(0.0..2.0).contains(&index) {
        return Err(CalcError::legacy("build_player", "Aftermath", "IndexError", "tuple index out of range"));
    }
    Ok(index as usize)
}

// These names are resolved once into Stat variants, never formatted per candidate.
fn slot_stat(stat: Stat, slot: Slot) -> Option<Stat> {
    match slot {
        Slot::Main => match stat {
            Stat::HandToHandSkill => Some(Stat::MainHandToHandSkill),
            Stat::DaggerSkill => Some(Stat::MainDaggerSkill),
            Stat::SwordSkill => Some(Stat::MainSwordSkill),
            Stat::GreatSwordSkill => Some(Stat::MainGreatSwordSkill),
            Stat::AxeSkill => Some(Stat::MainAxeSkill),
            Stat::GreatAxeSkill => Some(Stat::MainGreatAxeSkill),
            Stat::ScytheSkill => Some(Stat::MainScytheSkill),
            Stat::PolearmSkill => Some(Stat::MainPolearmSkill),
            Stat::KatanaSkill => Some(Stat::MainKatanaSkill),
            Stat::GreatKatanaSkill => Some(Stat::MainGreatKatanaSkill),
            Stat::ClubSkill => Some(Stat::MainClubSkill),
            Stat::StaffSkill => Some(Stat::MainStaffSkill),
            Stat::EvasionSkill => Some(Stat::MainEvasionSkill),
            Stat::DivineMagicSkill => Some(Stat::MainDivineMagicSkill),
            Stat::ElementalMagicSkill => Some(Stat::MainElementalMagicSkill),
            Stat::DarkMagicSkill => Some(Stat::MainDarkMagicSkill),
            Stat::NinjutsuSkill => Some(Stat::MainNinjutsuSkill),
            Stat::SummoningMagicSkill => Some(Stat::MainSummoningMagicSkill),
            Stat::BlueMagicSkill => Some(Stat::MainBlueMagicSkill),
            Stat::MagicAccuracySkill => Some(Stat::MainMagicAccuracySkill),
            Stat::Fua => Some(Stat::FuaMain),
            Stat::Oa2 => Some(Stat::Oa2Main), Stat::Oa3 => Some(Stat::Oa3Main),
            Stat::Oa4 => Some(Stat::Oa4Main), Stat::Oa5 => Some(Stat::Oa5Main),
            Stat::Oa6 => Some(Stat::Oa6Main), Stat::Oa7 => Some(Stat::Oa7Main),
            Stat::Oa8 => Some(Stat::Oa8Main), Stat::EnSpellDamage => Some(Stat::EnSpellDamageMain),
            Stat::EnSpellDamagePct => Some(Stat::EnSpellDamagePctMain),
            _ => None,
        },
        Slot::Sub => match stat {
            Stat::HandToHandSkill => Some(Stat::SubHandToHandSkill),
            Stat::DaggerSkill => Some(Stat::SubDaggerSkill),
            Stat::SwordSkill => Some(Stat::SubSwordSkill),
            Stat::GreatSwordSkill => Some(Stat::SubGreatSwordSkill),
            Stat::AxeSkill => Some(Stat::SubAxeSkill),
            Stat::GreatAxeSkill => Some(Stat::SubGreatAxeSkill),
            Stat::ScytheSkill => Some(Stat::SubScytheSkill),
            Stat::PolearmSkill => Some(Stat::SubPolearmSkill),
            Stat::KatanaSkill => Some(Stat::SubKatanaSkill),
            Stat::GreatKatanaSkill => Some(Stat::SubGreatKatanaSkill),
            Stat::ClubSkill => Some(Stat::SubClubSkill),
            Stat::StaffSkill => Some(Stat::SubStaffSkill),
            Stat::EvasionSkill => Some(Stat::SubEvasionSkill),
            Stat::DivineMagicSkill => Some(Stat::SubDivineMagicSkill),
            Stat::ElementalMagicSkill => Some(Stat::SubElementalMagicSkill),
            Stat::DarkMagicSkill => Some(Stat::SubDarkMagicSkill),
            Stat::NinjutsuSkill => Some(Stat::SubNinjutsuSkill),
            Stat::SummoningMagicSkill => Some(Stat::SubSummoningMagicSkill),
            Stat::BlueMagicSkill => Some(Stat::SubBlueMagicSkill),
            Stat::MagicAccuracySkill => Some(Stat::SubMagicAccuracySkill),
            Stat::Fua => Some(Stat::FuaSub),
            Stat::Oa2 => Some(Stat::Oa2Sub), Stat::Oa3 => Some(Stat::Oa3Sub),
            Stat::Oa4 => Some(Stat::Oa4Sub), Stat::Oa5 => Some(Stat::Oa5Sub),
            Stat::Oa6 => Some(Stat::Oa6Sub), Stat::Oa7 => Some(Stat::Oa7Sub),
            Stat::Oa8 => Some(Stat::Oa8Sub), Stat::EnSpellDamage => Some(Stat::EnSpellDamageSub),
            Stat::EnSpellDamagePct => Some(Stat::EnSpellDamagePctSub),
            _ => None,
        },
        _ => None,
    }
}
