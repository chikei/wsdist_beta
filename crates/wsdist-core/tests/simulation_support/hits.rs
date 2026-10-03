use serde_json::Value;
use wsdist_core::model::{AttackKind, AttackProc, HitEvent, python_truth};

use crate::support::{assert_number, number};

fn proc_name(name: &str) -> AttackProc {
    match name {
        "Quad. Attack" => AttackProc::QuadAttack,
        "Triple Attack" => AttackProc::TripleAttack,
        "Double Attack" => AttackProc::DoubleAttack,
        "OA2" => AttackProc::Occasionally(2), "OA3" => AttackProc::Occasionally(3),
        "OA4" => AttackProc::Occasionally(4), "OA5" => AttackProc::Occasionally(5),
        "OA6" => AttackProc::Occasionally(6), "OA7" => AttackProc::Occasionally(7),
        "OA8" => AttackProc::Occasionally(8),
        other => panic!("uncaptured proc {other}"),
    }
}

fn first_round_proc(locals: &Value, offhand: bool) -> AttackProc {
    let keys = if offhand { ["qa_proc_sub", "ta_proc_sub", "da_proc_sub"] }
        else { ["qa_proc_main", "ta_proc_main", "da_proc_main"] };
    for (key, proc) in keys.into_iter().zip([AttackProc::QuadAttack, AttackProc::TripleAttack, AttackProc::DoubleAttack]) {
        if python_truth(&locals[key]) { return proc; }
    }
    AttackProc::Normal
}

fn assert_hit(actual: &HitEvent, expected: HitEvent, label: &str) {
    assert_eq!(actual.kind, expected.kind, "{label}: attempted hit kind");
    assert_eq!(actual.proc, expected.proc, "{label}: selected proc precedence");
    assert_eq!(actual.attempt, expected.attempt, "{label}: attempt (misses included)");
    assert_eq!(actual.landed, expected.landed, "{label}: landed/missed");
    assert_eq!(actual.critical, expected.critical, "{label}: crit decision");
    super::assert_close(actual.physical_damage, expected.physical_damage, label);
    super::assert_close(actual.magical_damage, expected.magical_damage, label);
    super::assert_close(actual.tp_return, expected.tp_return, label);
}

pub fn assert_sample_hits(sample: &Value, actual: &[HitEvent], cursor: &mut usize, label: &str) {
    let is_round = sample["kind"] == "round";
    let locals = &sample["locals"];
    let mut current_kind = AttackKind::Main;
    let mut current_proc = AttackProc::Normal;
    let start = *cursor;
    let mut attempts = 0;
    for event in sample["events"].as_array().unwrap() {
        let line = event["source_line"].as_u64().unwrap();
        let mut retention = None;
        if event["kind"] == "proc" {
            let values = &event["values"];
            match line {
                599 => { current_kind = AttackKind::Main; current_proc = proc_name(values["main_proc"][0].as_str().unwrap()); }
                621 => { current_kind = AttackKind::Offhand; current_proc = proc_name(values["sub_proc"][0].as_str().unwrap()); }
                633 => { current_kind = AttackKind::Zanshin; current_proc = AttackProc::Occasionally(2); }
                1657 => { current_kind = AttackKind::Main; current_proc = proc_name(values["ma_label"].as_str().unwrap()); }
                1666 => { current_kind = AttackKind::Offhand; current_proc = proc_name(values["ma_label"].as_str().unwrap()); }
                1673 => retention = Some((AttackProc::Fotia, number(&values["base_tp"]))),
                1679 => retention = Some((AttackProc::ConserveTp, number(&values["conserve_tp_return"]))),
                other => panic!("{label}: unhandled captured proc observation {other}"),
            }
            if retention.is_none() { continue; }
        }
        let expected = if let Some((proc, tp_return)) = retention {
            HitEvent { kind: AttackKind::TpRetention, proc, attempt: 0, landed: true,
                critical: false, physical_damage: 0.0, magical_damage: 0.0, tp_return }
        } else {
            let tag = event["tag"].as_str().unwrap();
            let landed = event["kind"] == "landed";
            let (kind, proc) = match tag {
                "main" => (AttackKind::Main, if is_round { first_round_proc(locals, false) } else { AttackProc::Normal }),
                "sub" => (AttackKind::Offhand, if is_round { first_round_proc(locals, true) } else { AttackProc::Normal }),
                "other" => (current_kind, current_proc),
                "zanshin" => (AttackKind::Zanshin, AttackProc::Normal),
                "kick" => (AttackKind::Kick, AttackProc::Normal),
                "daken" => (AttackKind::Daken, AttackProc::Normal),
                "ranged" => (AttackKind::Ranged, AttackProc::Normal),
                "magical" => (AttackKind::Magical, AttackProc::Normal),
                other => panic!("{label}: unknown captured hit tag {other}"),
            };
            let magical = kind == AttackKind::Magical;
            let attempt = if magical {
                if locals["magical"].as_bool().unwrap() { 1 } else { 0 }
            } else {
                event["attempted_hits"].as_u64().unwrap() as u8
            };
            if !magical { attempts = attempts.max(attempt); }
            let physical_damage = if !landed || magical { 0.0 }
                // The source logs the unboosted per-hit value, but its return
                // and kick total include the independently rolled kick bonus.
                else if kind == AttackKind::Kick { number(&locals["kickattacks_damage"]) }
                else { number(&event["phys_dmg_ph"]) };
            let magical_damage = if !landed || kind == AttackKind::Daken { 0.0 }
                else if magical { number(&event["magical_damage"]) }
                else { event.get("magic_dmg_ph").map_or(0.0, number) };
            let critical = if !landed { false }
                else if magical { event["magic_crit2"].as_bool().unwrap() }
                else { event["crit"].as_bool().unwrap() };
            let tp_return = if !landed { 0.0 }
                else if magical { number(&event["magic_tp_return"]) }
                else { number(&event["tp_ph"]) };
            HitEvent { kind, proc, attempt, landed, critical, physical_damage, magical_damage, tp_return }
        };
        let hit = actual.get(*cursor).unwrap_or_else(|| panic!("{label}: missing observed hit {}", *cursor - start));
        assert_hit(hit, expected, label);
        *cursor += 1;
    }
    if let Some(expected) = locals.get("attempted_hits") {
        assert_eq!(usize::from(attempts), expected.as_u64().unwrap() as usize, "{label}: complete sampled attempt count");
    }
    if is_round {
        let mut physical = 0.0;
        let mut magical = 0.0;
        let mut tp = 0.0;
        for hit in &actual[start..*cursor] {
            physical += hit.physical_damage;
            magical += hit.magical_damage;
            tp += hit.tp_return;
        }
        assert_number(physical, &sample["result"][0], label);
        assert_number(tp, &sample["result"][1], label);
        assert_number(magical, &sample["result"][2], label);
    }
}
