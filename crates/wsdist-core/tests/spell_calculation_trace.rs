#[allow(dead_code)]
mod combat_support;

use std::sync::Arc;

use wsdist_core::Catalog;
use wsdist_core::actions::{SPELL_REGISTRY, evaluate_spell, gui_spell_type, resolve_spell_name};
use wsdist_core::model::{ActionResult, CalculationStep, Enemy, EvaluationContext, Metric, Player, SpellRequest, SpellType};
use wsdist_core::rng::ScriptedTape;

use combat_support::{boundary_enemy, build_boundary, document, fixture_root, index, resource_root};

fn fixture(suffix: &str) -> (EvaluationContext<ScriptedTape>, Player, Enemy) {
    let root = fixture_root();
    let entry = index(&root).cases.into_iter().find(|entry| entry.id.ends_with(suffix)).unwrap();
    let case = document(&root.join(entry.path));
    let catalog = Arc::new(Catalog::load(&resource_root()).expect("frozen catalog"));
    let mut ctx = EvaluationContext::new(catalog, ScriptedTape::new(Vec::new()));
    let player = build_boundary(&case, &mut ctx).expect("actual player construction");
    (ctx, player, boundary_enemy(&case))
}

fn value(step: &CalculationStep, name: &str) -> f64 {
    step.values.iter().find(|(key, _)| *key == name).unwrap().1
}

fn step<'a>(trace: &'a [CalculationStep], label: &str) -> &'a CalculationStep {
    trace.iter().find(|step| step.label == label).unwrap()
}

fn result_bits(result: &ActionResult) -> [u64; 5] {
    [result.damage.to_bits(), result.tp_return.to_bits(), result.metric.to_bits(),
        result.physical_damage.to_bits(), result.magical_damage.to_bits()]
}

#[test]
fn tracing_all_exposed_spells_preserves_results_streams_and_integer_stages() {
    let (mut ctx, player, enemy) = fixture(".spell.EnSpell.weatherFirestorm_II");
    for spec in SPELL_REGISTRY.iter().filter(|spec| spec.name != "None") {
        for metric in ["Damage dealt", "TP return", "TP > Damage", "Damage > TP"] {
            let request = SpellRequest { spell: resolve_spell_name(spec.name).unwrap(),
                spell_type: gui_spell_type(spec.name).unwrap(), metric: Metric::new(metric) };
            let baseline = evaluate_spell(&player, &enemy, &request, &mut ctx).unwrap();
            assert!(ctx.calculation_trace.is_none());
            ctx.calculation_trace = Some(Vec::new());
            let traced = evaluate_spell(&player, &enemy, &request, &mut ctx).unwrap();
            let trace = ctx.calculation_trace.take().unwrap();
            assert_eq!(result_bits(&traced), result_bits(&baseline), "{} / {metric}", spec.name);
            assert_eq!(traced.invert, baseline.invert);
            assert_eq!(traced.seconds_per_round, baseline.seconds_per_round);
            assert_eq!(traced.time_to_ws, baseline.time_to_ws);
            assert_eq!(ctx.random.position(), 0, "{}: analytical traces must not consume RNG", spec.name);

            let output = trace.last().expect("a successful spell supplies its calculation result");
            if spec.name == "EnSpell" || request.spell_type == SpellType::Ninjutsu {
                assert_eq!(value(output, "damage").to_bits(), traced.metric.to_bits());
            } else {
                assert_eq!(value(output, "value").to_bits(), traced.metric.to_bits());
            }
            let stages: Vec<_> = trace.iter().filter(|step| step.values.iter().any(|(key, _)| *key == "next")).collect();
            if spec.name != "EnSpell" && matches!(request.spell_type, SpellType::Ninjutsu | SpellType::ElementalMagic) {
                assert_eq!(stages.len(), 12, "{}: retain every legacy integer multiplier stage", spec.name);
                for stage in &stages {
                    assert_eq!((value(stage, "damage") * value(stage, "multiplier")).trunc().to_bits(),
                        value(stage, "next").to_bits(), "{} / {}", spec.name, stage.label);
                }
                assert_eq!(value(stages.last().unwrap(), "next").to_bits(), traced.damage.to_bits());
            }
        }
    }
}

#[test]
fn barrage_shot_and_recycle_operands_reconstruct_authoritative_damage_tp_and_metric() {
    let (mut ctx, player, enemy) = fixture(".spell.ranged.rng.Barrage.Barrage.Damage_dealt");
    let request = SpellRequest { spell: resolve_spell_name("Barrage").unwrap(),
        spell_type: SpellType::RangedAttack, metric: Metric::new("TP > Damage") };
    let baseline = evaluate_spell(&player, &enemy, &request, &mut ctx).unwrap();
    ctx.calculation_trace = Some(Vec::new());
    let result = evaluate_spell(&player, &enemy, &request, &mut ctx).unwrap();
    let trace = ctx.calculation_trace.take().unwrap();
    assert_eq!(result_bits(&result), result_bits(&baseline));
    assert_eq!(ctx.random.position(), 0);

    let barrage = step(&trace, "Expected main/Barrage hits");
    let attempts = (1.0 + value(barrage, "barrage")).ceil() as usize;
    let expected_hits = (0..attempts).fold(0.0, |hits, i| hits + value(barrage, "hit").powf(i as f64 + 1.0));
    assert_eq!(expected_hits.to_bits(), value(barrage, "hits").to_bits());
    assert!(attempts > 1);

    let shots = step(&trace, "Expected additional shot hits");
    assert_eq!((1.0 * value(shots, "hit") * value(shots, "double")).to_bits(), value(shots, "double_hits").to_bits());
    assert_eq!((2.0 * value(shots, "hit") * value(shots, "triple")).to_bits(), value(shots, "triple_hits").to_bits());
    assert_eq!((3.0 * value(shots, "hit") * value(shots, "quad")).to_bits(), value(shots, "quad_hits").to_bits());

    let mut tp = value(step(&trace, "Main/Barrage TP"), "tp");
    tp += value(step(&trace, "Double Shot TP"), "tp");
    tp += value(step(&trace, "Triple Shot TP"), "tp");
    tp += value(step(&trace, "Quad Shot TP"), "tp");
    let recycle = step(&trace, "Expected Arcadian Beret Recycle TP");
    assert_eq!(value(recycle, "recycle"), 90.0, "legacy Recycle cap");
    tp += value(recycle, "main_tp");
    tp += value(recycle, "double_tp");
    tp += value(recycle, "triple_tp");
    tp += value(recycle, "quad_tp");
    assert_eq!(tp.to_bits(), result.tp_return.to_bits());

    let additional = step(&trace, "Expected additional shot damage");
    let mut physical = value(step(&trace, "Expected main/Barrage damage"), "damage");
    physical += value(additional, "double_damage");
    physical += value(additional, "triple_damage");
    physical += value(additional, "quad_damage");
    let final_step = step(&trace, "Ranged result");
    assert_eq!(physical.to_bits(), value(final_step, "physical").to_bits());
    let damage = physical * (1.0 + value(final_step, "true_shot")) * (1.0 + value(final_step, "hover_shot"))
        * value(final_step, "empyrean") * value(final_step, "mythic");
    assert_eq!(damage.to_bits(), result.damage.to_bits());
    assert_eq!((damage * tp * tp / 1e4).to_bits(), result.metric.to_bits());
}
