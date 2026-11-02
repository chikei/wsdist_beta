//! Independent mathematics over isolated sampler contracts. Expectations do not
//! call production formulas and are never used to replace legacy output.
use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};
use wsdist_core::actions::{evaluate_action, sample_round, sample_ws, resolve_spell_name};
use wsdist_core::math::{PdifSkill, get_avg_pdif_melee, get_avg_phys_damage, get_ma_rate3, get_pdif_melee, get_pdif_ranged, get_phys_damage, get_resist_state};
use wsdist_core::model::{Ability, AttackKind, AttackProc, ActionRequest, Enemy, EvaluationContext, Job, Metric, Player, Slot, SpellRequest, SpellType, WsRequest, WsType};
use wsdist_core::rng::{RandomSources, ScriptedTape, TapeRecord};
use wsdist_core::stats::Stat;
use wsdist_core::weaponskills::resolve_name;
use wsdist_core::Catalog;

const BATCHES: usize = 40;
const PER_BATCH: usize = 25_000;
const N: usize = BATCHES * PER_BATCH;

/// Integral of clamp(x,0,cap) under a uniform distribution, including clipping
/// atoms. This is an antiderivative, not the analytical engine's midpoint.
pub fn clipped_uniform_mean(low: f64, high: f64, cap: f64) -> f64 {
    if low == high { return low.max(0.0).min(cap); }
    let (a, b) = if low < high { (low, high) } else { (high, low) };
    let linear_low = a.max(0.0).min(cap);
    let linear_high = b.max(0.0).min(cap);
    let linear_integral = (linear_high * linear_high - linear_low * linear_low) / 2.0;
    let clipped_integral = cap * (b - a.max(cap)).max(0.0);
    (linear_integral + clipped_integral) / (b - a)
}

fn melee_window(ratio: f64) -> (f64, f64) {
    // Each tuple is (exclusive endpoint, slope, intercept). Negative ratios
    // take the legacy final branch; supported boundary cases are not clamped.
    let lower_segments = [(0.38, 0.0, 0.0), (1.25, 1176.0 / 1024.0, -448.0 / 1024.0),
        (1.51, 0.0, 1.0), (2.44, 1176.0 / 1024.0, -755.0 / 1024.0)];
    let upper_segments = [(0.5, 1.0, 0.5), (0.7, 0.0, 1.0), (1.2, 1.0, 0.3), (1.5, 1.25, 0.0)];
    fn piecewise(ratio: f64, segments: &[(f64, f64, f64)], final_intercept: f64) -> f64 {
        if ratio >= 0.0 {
            for &(end, slope, intercept) in segments { if ratio < end { return slope * ratio + intercept; } }
        }
        ratio + final_intercept
    }
    (piecewise(ratio, &lower_segments, -0.375), piecewise(ratio, &upper_segments, 0.375))
}

pub fn melee_conditional_mean(ratio: f64, cap: f64, critical: bool) -> f64 {
    let shifted = ratio + if critical { 1.0 } else { 0.0 };
    let (low, high) = melee_window(shifted);
    (clipped_uniform_mean(low, high, cap) + if critical { 1.0 } else { 0.0 }) * 1.025
}

pub fn melee_mean(ratio: f64, cap: f64, crit: f64) -> f64 {
    (1.0 - crit) * melee_conditional_mean(ratio, cap, false) + crit * melee_conditional_mean(ratio, cap, true)
}

fn ranged_mean(ratio: f64, cap: f64, crit: f64) -> f64 {
    let (low, high) = if (0.0..0.9).contains(&ratio) { (ratio, ratio * 10.0 / 9.0) }
        else if (0.9..1.1).contains(&ratio) { (1.0, 1.0) }
        else { (ratio * 20.0 / 19.0 - 3.0 / 19.0, ratio) };
    clipped_uniform_mean(low, high, cap) * ((1.0 - crit) + 1.25 * crit)
}

pub fn resist_probabilities(probability: f64) -> [f64; 4] {
    let miss = 1.0 - probability;
    [probability, miss * probability, miss * miss * probability, miss * miss * miss]
}

fn staged(value: f64, multipliers: &[f64]) -> f64 {
    multipliers.iter().fold(value.trunc(), |damage, multiplier| (damage * multiplier).trunc())
}

#[derive(Clone, Copy, Default)]
struct Procs { qa: f64, ta: f64, da: f64, oa: [f64; 7] }

fn proc_outcomes(rates: Procs) -> Vec<(usize, f64)> {
    let mut outcomes = Vec::new();
    let mut remaining = 1.0;
    for (hits, probability) in [(3, rates.qa), (2, rates.ta), (1, rates.da)].into_iter()
        .chain(rates.oa.into_iter().enumerate().map(|(index, probability)| (7 - index, probability)))
    {
        outcomes.push((hits, remaining * probability));
        remaining *= 1.0 - probability;
    }
    outcomes.push((0, remaining));
    outcomes
}

/// Enumerate mutually exclusive priority-proc branches, then every landed/miss
/// tree with at most eight *attempts*. Misses consume an attempt. The sampled
/// normal-round order is initial main/sub, main extras, then offhand extras.
fn landed_distribution(main: Procs, sub: Procs, dual: bool, hit_rate: [f64; 2]) -> [f64; 9] {
    let mut distribution = [0.0; 9];
    let main_branches = proc_outcomes(main);
    let sub_branches = if dual { proc_outcomes(sub) } else { vec![(0, 1.0)] };
    for &(main_extra, main_probability) in &main_branches {
        for &(sub_extra, sub_probability) in &sub_branches {
            let weight = main_probability * sub_probability;
            if weight == 0.0 { continue; }
            let mut rates = Vec::with_capacity(8);
            rates.push(hit_rate[0]);
            if dual { rates.push(hit_rate[1]); }
            for _ in 0..main_extra { if rates.len() < 8 { rates.push(hit_rate[0]); } }
            for _ in 0..sub_extra { if rates.len() < 8 { rates.push(hit_rate[1]); } }
            for outcomes in 0..(1_usize << rates.len()) {
                let mut probability = weight;
                let mut landed = 0;
                for (index, &rate) in rates.iter().enumerate() {
                    if outcomes & (1 << index) != 0 { probability *= rate; landed += 1; }
                    else { probability *= 1.0 - rate; }
                }
                distribution[landed] += probability;
            }
        }
    }
    distribution
}

fn mean_discrete(distribution: &[f64]) -> f64 {
    distribution.iter().enumerate().map(|(value, probability)| value as f64 * probability).sum()
}

fn base_boundary(root: &Path, path: &str, catalog: &Arc<Catalog>) -> (Player, Enemy) {
    let case = wsdist_fixtures::read_value(&root.join(path), None).unwrap();
    let mut context = EvaluationContext::new(Arc::clone(catalog), RandomSources::from_seed(0));
    let player = crate::combat_support::build_boundary(&case, &mut context).unwrap();
    let enemy = crate::combat_support::boundary_enemy(&case);
    (player, enemy)
}

fn isolate(player: &mut Player) {
    for stat in Stat::ALL {
        if stat.name().starts_with("OA") || stat.name().starts_with("FUA") { player.stats.set(stat, 0.0); }
    }
    for stat in [Stat::Qa, Stat::Ta, Stat::Da, Stat::Daken, Stat::KickAttacks, Stat::Zanshin,
        Stat::Zanhasso, Stat::ZanshinOa2, Stat::ConserveTp, Stat::CritRate, Stat::CritDamage,
        Stat::DaDamagePct, Stat::TaDamagePct, Stat::StoreTp, Stat::TpBonus, Stat::Ftp] { player.stats.set(stat, 0.0); }
    for &ability in Ability::ALL { player.abilities.set(ability, false); }
    player.abilities.set_storm(Some(Arc::from("None")));
}

fn assert_close(actual: f64, expected: f64, path: &str) {
    crate::player_support::assert_number(actual, &wsdist_fixtures::Value::from(expected), path);
}

pub fn diagnostics(root: &Path) -> Vec<Value> {
    let catalog = Arc::new(Catalog::load(&super::resource_root()).unwrap());
    let capped = melee_mean(3.25, 3.25, 0.0);
    assert_close(capped, 3.23515625, "independent.integrated_capped_sword");
    let analytical = get_avg_pdif_melee(3.25, PdifSkill::from_name("Sword"), 0.0, 0.0, 1.0, 0.0).unwrap();
    assert_close(analytical, 3.33125, "legacy.analytical_capped_sword");
    let mixed = melee_mean(1.0, 3.25, 0.5);
    let mixed_analytical = get_avg_pdif_melee(1.0, PdifSkill::from_name("Sword"), 0.0, 0.0, 1.0, 0.5).unwrap();
    assert_close(mixed_analytical, 1.9859375, "legacy.correlated_critical_midpoint");
    let probabilities = resist_probabilities(0.5);
    assert_eq!(probabilities, [0.5, 0.25, 0.125, 0.125]);
    let expected_resist: f64 = probabilities.into_iter().zip([1.0, 0.5, 0.25, 0.125]).map(|(p, damage)| p * damage).sum();
    assert_eq!(expected_resist, 0.671875);
    let conserve: f64 = (10..200).map(f64::from).sum::<f64>() / 190.0;
    assert_eq!(conserve, 104.5);

    let (mut retention_player, retention_enemy) = base_boundary(root, "combat/000001.bitcode.zst", &catalog);
    isolate(&mut retention_player);
    let retention_request = ActionRequest::Ws(WsRequest { skill: resolve_name("Fast Blade").unwrap(),
        ws_type: WsType::Melee, tp: 1000.0, metric: Metric::new("TP return") });
    let mut retention_context = EvaluationContext::new(Arc::clone(&catalog), crate::player_support::RejectRandom);
    let baseline = evaluate_action(&retention_player, &retention_enemy, &retention_request, &mut retention_context).unwrap().tp_return;
    retention_player.stats.set(Stat::ConserveTp, 100.0);
    let analytical_conserve = evaluate_action(&retention_player, &retention_enemy, &retention_request, &mut retention_context).unwrap().tp_return - baseline;
    assert_close(analytical_conserve, 95.0, "legacy.analytical_conserve_tp");
    retention_player.stats.set(Stat::ConserveTp, 0.0);
    retention_player.gearset[Slot::Neck.index()] = catalog.by_name2("Fotia Gorget").unwrap();
    let analytical_fotia_one = evaluate_action(&retention_player, &retention_enemy, &retention_request, &mut retention_context).unwrap().tp_return - baseline;
    assert_eq!(analytical_fotia_one, 0.0, "legacy product with one Fotia piece");
    retention_player.gearset[Slot::Waist.index()] = catalog.by_name2("Fotia Belt").unwrap();
    let analytical_fotia_two = evaluate_action(&retention_player, &retention_enemy, &retention_request, &mut retention_context).unwrap().tp_return - baseline;
    assert_close(analytical_fotia_two, 0.1, "legacy product with both Fotia pieces");

    // Independent first-hit staged integer anchors for each discrete critical
    // outcome. Truncating after pDIF, or replacing conditional crit damage by a
    // correlated average, changes this consumer-visible result.
    for (critical, pdif, n, expected) in [(false, 1.125, 0.0, 411.75), (true, 2.0, 0.0, 878.4),
        (false, 1.125, 1.0, 274.5), (true, 2.0, 1.0, 585.6)] {
        let actual = get_phys_damage(100.0, 2.0, 20.0, pdif, 2.0, critical, 0.2, 0.5,
            0.0, 0.0, n, 0.0, 0.0, 0.0, 0.0, 0.0).unwrap();
        assert_close(actual, expected, "independent.first_hit_integer_stage");
    }
    let independent_physical = (1.0 - 0.5) * 366.0 * melee_conditional_mean(1.0, 3.25, false)
        + 0.5 * 366.0 * melee_conditional_mean(1.0, 3.25, true) * 1.2;
    let analytical_physical = get_avg_phys_damage(100.0, 2.0, 20.0, mixed_analytical, 2.0, 0.5,
        0.2, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0).unwrap();

    let mut main = Procs::default(); main.oa[5] = 1.0; // main OA3
    let mut sub = Procs::default(); sub.oa[0] = 1.0; // offhand OA8
    let distribution = landed_distribution(main, sub, true, [0.5, 0.5]);
    let sampled_hits = mean_discrete(&distribution);
    assert_close(sampled_hits, 4.0, "independent.eight_attempts");
    let mut oa = [0.0; 9]; oa[0] = 1.0; oa[2] = 1.0;
    let hits = get_ma_rate3(Job::Nin, 1.0, 0.0, 0.0, 0.0, &oa, &[0.0; 9], true,
        &[[0.5, 0.5]; 2], 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, false, false, true);
    assert_close(hits.main + hits.sub, 5.0, "legacy.expected_landed_cap");

    // Execute a real analytical Ninjutsu branch with an independently staged
    // calculation. The four discrete resist states are staged separately for
    // the sampler expectation; legacy analytical staging is not equated to it.
    let (mut player, mut enemy) = base_boundary(root, "combat/018467.bitcode.zst", &catalog);
    isolate(&mut player);
    for (stat, value) in [(Stat::Int, 124.0), (Stat::NinjutsuSkill, 152.0), (Stat::MagicDamage, 3.7),
        (Stat::NinjutsuMagicDamage, 0.0), (Stat::MagicAttack, 19.0), (Stat::NinjutsuMagicAttack, 0.0),
        (Stat::MagicAccuracy, 1000.0), (Stat::MainMagicAccuracySkill, 0.0), (Stat::NinjutsuMagicAccuracy, 0.0),
        (Stat::FireElementalBonus, 11.0), (Stat::ElementalBonus, 2.7), (Stat::FutaeBonus, 17.0),
        (Stat::NinjutsuDamagePct, 13.0)] { player.stats.set(stat, value); }
    player.abilities.set(Ability::Futae, true); player.abilities.set(Ability::Innin, true);
    for (stat, value) in [(Stat::Int, 100.0), (Stat::MagicEvasion, 1152.0), (Stat::MagicDefense, 5.0), (Stat::MagicDamageTaken, -13.0)] { enemy.stats.set(stat, value); }
    let prefix = [1.51, 1.67, 1.137];
    let suffix = [1.0, 1.0, 1.0, 1.0, 1.13, 119.0 / 105.0, 1.24, 0.87];
    let pre_resist = staged(43.7, &prefix);
    let analytical_staged = staged((pre_resist * expected_resist).trunc(), &suffix);
    let discrete_staged: Vec<_> = [1.0, 0.5, 0.25, 0.125].into_iter().map(|resist| staged((pre_resist * resist).trunc(), &suffix)).collect();
    let discrete_mean = discrete_staged.iter().zip(probabilities).map(|(damage, probability)| damage * probability).sum::<f64>();
    let mut ctx = EvaluationContext::new(catalog, crate::player_support::RejectRandom);
    let action = ActionRequest::Spell(SpellRequest { spell: resolve_spell_name("Katon: Ichi").unwrap(), spell_type: SpellType::Ninjutsu, metric: Metric::new("Damage dealt") });
    let actual = evaluate_action(&player, &enemy, &action, &mut ctx).unwrap();
    assert_eq!(actual.damage, analytical_staged, "Ninjutsu truncation at every independent stage");

    // Endpoint tapes directly condition on critical/noncritical branches and
    // prove that the clipped upper atom is integrated rather than midpointed.
    for (critical, ratio, q, expected) in [(false, 3.25, 3.625, 3.25), (true, 1.0, 2.375, 3.375)] {
        let (low, high) = melee_window(ratio + if critical { 1.0 } else { 0.0 });
        let mut tape = ScriptedTape::new(vec![TapeRecord::uniform(wsdist_core::model::RandomStream::Pdif, 0.0, 1.0, if critical { 0.0 } else { 0.9 }),
            TapeRecord::uniform(wsdist_core::model::RandomStream::Pdif, low, high, q),
            TapeRecord::uniform(wsdist_core::model::RandomStream::Pdif, 1.0, 1.05, 1.0)]);
        let result = get_pdif_melee(ratio, PdifSkill::from_name("Sword"), 0.0, 0.0, 1.0, 0.5, &mut tape).unwrap();
        assert_eq!(result.1, critical); assert_close(result.0, expected, "conditional clipped pDIF endpoint"); tape.assert_finished().unwrap();
    }

    vec![
        json!({"id": "pdif.capped_sword", "source_anchors": ["get_pdif.py:10-149"], "legacy_analytical": analytical, "independent_sampled_expectation": capped, "difference": analytical - capped}),
        json!({"id": "pdif.critical_correlation", "source_anchors": ["get_pdif.py:10-149", "get_phys_damage.py:9-29"], "legacy_analytical_pdif": mixed_analytical, "independent_sampled_pdif": mixed,
            "legacy_analytical_physical": analytical_physical, "independent_conditioned_physical": independent_physical}),
        json!({"id": "resist.four_states", "source_anchors": ["nuking.py:270-308"], "probabilities": probabilities, "expectation": expected_resist}),
        json!({"id": "conserve_tp", "source_anchors": ["actions.py:1565-1569", "actions.py:1670-1681"], "legacy_analytical": analytical_conserve, "independent_sampled_expectation": conserve, "integer_support": [10, 199]}),
        json!({"id": "fotia.one_piece", "source_anchors": ["actions.py:1565-1569", "actions.py:1670-1681"], "tp": 1000, "legacy_analytical": analytical_fotia_one, "independent_sampled_expectation": 10.0}),
        json!({"id": "fotia.two_pieces", "source_anchors": ["actions.py:1565-1569", "actions.py:1670-1681"], "tp": 1000, "legacy_analytical": analytical_fotia_two, "independent_sampled_expectation": 20.0}),
        json!({"id": "multiattack.eight_attempts", "source_anchors": ["get_ma_rate.py:183-284", "actions.py:597-820"], "legacy_analytical_landed_hits": hits.main + hits.sub, "independent_sampled_landed_hits": sampled_hits, "landed_probabilities": distribution}),
        json!({"id": "ninjutsu.staged_truncation", "source_anchors": ["actions.py:973-1059"], "legacy_analytical": actual.damage, "independent_analytical_stages": analytical_staged,
            "independent_discrete_resist_damage": discrete_staged, "independent_discrete_expectation": discrete_mean, "note": "Diagnostic discrete model; casting has no production stochastic Ninjutsu path."}),
    ]
}

#[derive(Default)]
struct Moments { count: usize, mean: f64, m2: f64 }
impl Moments {
    fn add(&mut self, value: f64) {
        self.count += 1;
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        self.m2 += delta * (value - self.mean);
    }
    fn variance(&self) -> f64 { if self.count > 1 { self.m2 / (self.count - 1) as f64 } else { 0.0 } }
}

fn statistic<F>(id: &str, anchors: &[&str], expected_mean: f64, categories: Vec<(String, f64)>, sample: F) -> Value
where F: FnMut(usize, usize, &mut RandomSources, &mut [u64]) -> f64 {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| measure_statistic(id, anchors, expected_mean, categories, sample))) {
        Ok(result) => result,
        Err(error) => json!({"id": id, "source_anchors": anchors, "passed": false, "completed": false,
            "planned_n": N, "failure": super::panic_message(error)}),
    }
}

fn measure_statistic<F>(id: &str, anchors: &[&str], expected_mean: f64, categories: Vec<(String, f64)>, mut sample: F) -> Value
where F: FnMut(usize, usize, &mut RandomSources, &mut [u64]) -> f64 {
    let mut all = Moments::default();
    let mut batch_means = Moments::default();
    let mut counts = vec![0_u64; categories.len()];
    let mut batches = Vec::with_capacity(BATCHES);
    for batch in 0..BATCHES {
        let seed = 20_240_101_u64 + batch as u64;
        let mut random = RandomSources::from_seed(seed);
        let mut moments = Moments::default();
        for index in 0..PER_BATCH {
            let value = sample(batch, index, &mut random, &mut counts);
            assert!(value.is_finite(), "{id}: nonfinite isolated sample");
            moments.add(value); all.add(value);
        }
        assert!(moments.mean.is_finite() && moments.variance().is_finite(), "{id}: nonfinite batch reduction");
        batch_means.add(moments.mean);
        batches.push(json!({"batch": batch, "seed": seed,
            "n": PER_BATCH, "mean": moments.mean, "sample_variance": moments.variance()}));
    }
    let standard_error = (batch_means.variance() / BATCHES as f64).sqrt();
    let tolerance = (6.0 * standard_error).max(1e-9);
    let error = all.mean - expected_mean;
    assert!(all.mean.is_finite() && all.variance().is_finite() && standard_error.is_finite() && error.is_finite(), "{id}: nonfinite statistical reduction");
    let mut passed = error.abs() <= tolerance;
    let categories = categories.into_iter().zip(counts).map(|((name, expected), count)| {
        let observed = count as f64 / N as f64;
        let gate = 6.0 * (expected * (1.0 - expected) / N as f64).sqrt();
        let passed_category = if expected == 0.0 || expected == 1.0 { observed == expected }
            else { (observed - expected).abs() <= gate };
        passed &= passed_category;
        json!({"category": name, "count": count, "expected_probability": expected, "observed_probability": observed,
            "error": observed - expected, "six_sigma_tolerance": gate, "passed": passed_category})
    }).collect::<Vec<_>>();
    json!({"id": id, "source_anchors": anchors, "completed": true, "n": N, "batch_count": BATCHES, "samples_per_batch": PER_BATCH,
        "expected_mean": expected_mean, "observed_mean": all.mean, "sample_variance": all.variance(),
        "batch_mean_variance": batch_means.variance(), "standard_error_of_batch_means": standard_error,
        "error": error, "six_standard_error_tolerance": tolerance, "passed": passed, "categories": categories, "batches": batches})
}

/// Each isolated scenario uses 40 fixed batch seeds with independent Primary/Pdif streams.
/// No comparison equates a sampled mean to a legacy analytical approximation.
pub fn statistics(root: &Path) -> Vec<Value> {
    let mut results = Vec::new();
    for probability in [0.0, 0.5, 0.95, 1.0] {
        let probabilities = resist_probabilities(probability);
        let expected = probabilities.into_iter().zip([1.0, 0.5, 0.25, 0.125]).map(|(p, state)| p * state).sum();
        results.push(statistic(&format!("resist.p{probability}"), &["nuking.py:270-291"], expected,
            ["full", "half", "quarter", "eighth"].into_iter().zip(probabilities).map(|(name, p)| (name.to_owned(), p)).collect(),
            |_, _, random, counts| {
                let value = get_resist_state(probability, random);
                let index = match value { 1.0 => 0, 0.5 => 1, 0.25 => 2, 0.125 => 3, other => panic!("invalid resist state {other}") };
                counts[index] += 1; value
            }));
    }
    for (id, ratio, crit, ranged, cap, skill) in [
        ("pdif.melee_zero", -1.0, 0.0, false, 3.25, "Sword"),
        ("pdif.melee_critical_mixture", 1.0, 0.5, false, 3.25, "Sword"),
        ("pdif.melee_capped", 3.25, 0.0, false, 3.25, "Sword"),
        ("pdif.melee_always_critical", 2.0, 1.0, false, 3.25, "Sword"),
        ("pdif.ranged_capped_critical_mixture", 3.5, 0.25, true, 3.5, "Marksmanship"),
    ] {
        let expected = if ranged { ranged_mean(ratio, cap, crit) } else { melee_mean(ratio, cap, crit) };
        results.push(statistic(id, &["get_pdif.py:10-195"], expected, vec![("critical".to_owned(), crit), ("noncritical".to_owned(), 1.0 - crit)],
            |_, _, random, counts| {
                let (value, critical) = if ranged { get_pdif_ranged(ratio, PdifSkill::from_name(skill), 0.0, 0.0, 1.0, crit, random) }
                    else { get_pdif_melee(ratio, PdifSkill::from_name(skill), 0.0, 0.0, 1.0, crit, random) }.unwrap();
                counts[usize::from(!critical)] += 1; value
            }));
    }
    let catalog = Arc::new(Catalog::load(&super::resource_root()).unwrap());
    for (id, dual, qa, ta, da, capped) in [
        ("proc.zero", false, 0.0, 0.0, 0.0, false),
        ("proc.certain_qa_precedence", true, 1.0, 1.0, 1.0, false),
        ("proc.certain_ta_precedence", false, 0.0, 1.0, 1.0, false),
        ("proc.certain_da", false, 0.0, 0.0, 1.0, false),
        ("proc.dual_priority_mixture", true, 0.17, 0.31, 0.43, false),
        ("proc.oa_eight_attempt_cap", true, 0.0, 0.0, 0.0, true),
    ] {
        let (mut player, mut enemy) = base_boundary(root, if dual { "combat/018467.bitcode.zst" } else { "combat/018452.bitcode.zst" }, &catalog);
        isolate(&mut player);
        player.stats.set(Stat::Qa, qa * 100.0); player.stats.set(Stat::Ta, ta * 100.0); player.stats.set(Stat::Da, da * 100.0);
        player.stats.set(Stat::Accuracy1, 950.0); player.stats.set(Stat::Accuracy2, 950.0); enemy.stats.set(Stat::Evasion, 1000.0);
        let mut main = Procs { qa, ta, da, ..Procs::default() }; let mut sub = main;
        if capped { main.oa[5] = 1.0; sub.oa[0] = 1.0; player.stats.set(Stat::Oa3Main, 100.0); player.stats.set(Stat::Oa8Sub, 100.0); }
        let distribution = landed_distribution(main, sub, dual, [0.5, 0.5]);
        let mut categories = (0..=8).map(|hits| (format!("landed_hits_{hits}"), distribution[hits])).collect::<Vec<_>>();
        let normal = (1.0 - qa) * (1.0 - ta) * (1.0 - da);
        categories.extend([("main_qa", qa), ("main_ta", (1.0 - qa) * ta), ("main_da", (1.0 - qa) * (1.0 - ta) * da), ("main_normal", normal)].map(|(name, probability)| (name.to_owned(), probability)));
        let mut context = EvaluationContext::new(Arc::clone(&catalog), RandomSources::from_seed(0));
        context.action_trace = Some(Vec::with_capacity(8));
        results.push(statistic(id, &["actions.py:597-820", "get_ma_rate.py:183-284"], mean_discrete(&distribution), categories,
            |_, index, random, counts| {
                if index == 0 { context.random = random.clone(); }
                context.action_trace.as_mut().unwrap().clear();
                sample_round(&player, &enemy, 0.0, 1000.0, &mut context).unwrap();
                let trace = context.action_trace.as_ref().unwrap();
                assert!(trace.len() <= 8, "{id}: eight attempted hits, including misses");
                assert!(trace.iter().all(|hit| matches!(hit.kind, AttackKind::Main | AttackKind::Offhand)), "{id}: isolated normal hands");
                let landed = trace.iter().filter(|hit| hit.landed).count(); counts[landed] += 1;
                let category = match trace[0].proc { AttackProc::QuadAttack => 9, AttackProc::TripleAttack => 10, AttackProc::DoubleAttack => 11, AttackProc::Normal => 12, other => panic!("unexpected first proc {other:?}") };
                counts[category] += 1; landed as f64
            }));
    }
    for (id, fotia, conserve) in [("retention.conserve", 0, 0.37), ("retention.fotia_one", 1, 0.0), ("retention.fotia_two", 2, 0.0)] {
        let (mut player, enemy) = base_boundary(root, "combat/000001.bitcode.zst", &catalog);
        isolate(&mut player); player.stats.set(Stat::ConserveTp, conserve * 100.0);
        if fotia > 0 { player.gearset[Slot::Neck.index()] = catalog.by_name2("Fotia Gorget").unwrap(); }
        if fotia > 1 { player.gearset[Slot::Waist.index()] = catalog.by_name2("Fotia Belt").unwrap(); }
        let probability = if fotia > 0 { 0.01 * fotia as f64 } else { conserve };
        let expected = if fotia > 0 { 1000.0 * probability } else { 104.5 * conserve };
        let mut categories = vec![("proc".to_owned(), probability), ("no_proc".to_owned(), 1.0 - probability)];
        if conserve > 0.0 { categories.extend((10..200).map(|tp| (format!("integer_tp_{tp}"), conserve / 190.0))); }
        let request = WsRequest { skill: resolve_name("Fast Blade").unwrap(), ws_type: WsType::Melee, tp: 1000.0, metric: Metric::new("Damage dealt") };
        let mut context = EvaluationContext::new(Arc::clone(&catalog), RandomSources::from_seed(0));
        context.action_trace = Some(Vec::with_capacity(10));
        results.push(statistic(id, &["actions.py:1565-1569", "actions.py:1670-1681"], expected, categories,
            |_, index, random, counts| {
                if index == 0 { context.random = random.clone(); }
                context.action_trace.as_mut().unwrap().clear();
                sample_ws(&player, &enemy, &request, true, &mut context).unwrap();
                let mut retained = 0.0;
                for hit in context.action_trace.as_ref().unwrap().iter().filter(|hit| hit.kind == AttackKind::TpRetention) { retained += hit.tp_return; }
                let occurred = retained != 0.0; counts[usize::from(!occurred)] += 1;
                if conserve > 0.0 && occurred {
                    assert!(retained.fract() == 0.0 && (10.0..200.0).contains(&retained), "Conserve TP integer support 10..199");
                    counts[2 + retained as usize - 10] += 1;
                }
                retained
            }));
    }
    results
}
