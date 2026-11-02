use std::sync::Arc;

use wsdist_core::Catalog;
use wsdist_core::model::{AttackKind, AttackProc};

#[allow(dead_code)]
mod combat_support;
#[allow(dead_code)]
mod round_support;

use combat_support::{fixture, fixture_root, index, replay_case, resource_root};
use round_support::{assert_enspell_sample, assert_trace_totals, enspell_sample, scripted_sample};

#[test]
fn all_frozen_round_branches_match_compiled_and_scripted_oracles() {
    let catalog = Arc::new(Catalog::load(&resource_root()).expect("real immutable catalog"));
    let root = fixture_root();
    let mut cases = 0;
    let mut recorded = 0;
    let mut scripted = 0;
    for entry in index(&root).cases.into_iter().filter(|entry| entry.operation == "combat.round") {
        let case = fixture(&root, &entry);
        assert_eq!(case["id"], entry.id, "indexed case identity");
        let replay = replay_case(&case, &catalog);
        cases += 1;
        recorded += replay.recorded;
        scripted += replay.scripted;
    }
    assert_eq!(cases, 816, "complete frozen combat.round corpus, without narrowing");
    assert_eq!(recorded, 816, "recorded compiled-draw replays");
    assert_eq!(scripted, 19, "all paired strict endpoint tapes and complete draw consumption");
}

#[test]
fn sampled_trace_keeps_misses_caps_and_special_hit_identity() {
    let catalog = Arc::new(Catalog::load(&resource_root()).expect("real immutable catalog"));
    let root = fixture_root();
    let entries = index(&root).cases;
    for (suffix, kinds, procs, landed) in [
        ("round.tape.dual_eight_attempts_all_miss",
            vec![AttackKind::Main, AttackKind::Offhand, AttackKind::Main, AttackKind::Main,
                AttackKind::Main, AttackKind::Main, AttackKind::Main, AttackKind::Main],
            vec![AttackProc::Normal, AttackProc::Normal, AttackProc::Occasionally(8), AttackProc::Occasionally(8),
                AttackProc::Occasionally(8), AttackProc::Occasionally(8), AttackProc::Occasionally(8), AttackProc::Occasionally(8)],
            vec![false; 8]),
        ("round.tape.kick_footwork",
            vec![AttackKind::Main, AttackKind::Offhand, AttackKind::Kick],
            vec![AttackProc::Normal; 3], vec![true; 3]),
        ("round.tape.daken",
            vec![AttackKind::Main, AttackKind::Offhand, AttackKind::Daken],
            vec![AttackProc::Normal; 3], vec![true; 3]),
        ("round.tape.zanshin_oa2_before_zanshin",
            vec![AttackKind::Main, AttackKind::Zanshin, AttackKind::Zanshin],
            vec![AttackProc::Normal, AttackProc::Occasionally(2), AttackProc::Occasionally(2)],
            vec![false, true, true]),
    ] {
        let entry = entries.iter().find(|entry| entry.id.ends_with(suffix)).expect("frozen edge case");
        let case = fixture(&root, entry);
        let observed = scripted_sample(&case, &catalog);
        let actual = observed.trace.iter().map(|event| (event.kind, event.proc, event.landed)).collect::<Vec<_>>();
        let expected = kinds.into_iter().zip(procs).zip(landed)
            .map(|((kind, proc), landed)| (kind, proc, landed)).collect::<Vec<_>>();
        assert_eq!(actual, expected, "{}: attempted/proc/hit sequence", entry.id);
        assert_trace_totals(&observed);
    }
}

#[test]
fn nonzero_sampled_enspell_uses_main_percent_offhand_and_exposes_total() {
    let catalog = Arc::new(Catalog::load(&resource_root()).expect("real immutable catalog"));
    let root = fixture_root();
    let entry = index(&root).cases.into_iter()
        .find(|entry| entry.id.ends_with("round.tape.dual_qa_over_ta_da")).expect("captured all-hit tape");
    let case = fixture(&root, &entry);
    let observed = enspell_sample(&case, &catalog);
    assert_enspell_sample(&observed, &case);
}
