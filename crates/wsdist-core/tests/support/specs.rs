use wsdist_fixtures::{Value, to_value};
use wsdist_core::math::{WeaponBonusSkill, WeaponBonusWeapon, get_weapon_bonus};
use wsdist_core::stats::Stat;
use wsdist_core::weaponskills::{CritScaling, DStatFormula, TpScaling, WS_TABLE, WsHook, resolve_name};

fn exact(actual: f64, expected: &Value, path: &str) {
    crate::kernel_support::assert_value(&to_value(actual).unwrap(), expected, true, path);
}

fn points(actual: [f64; 3], expected: &Value, path: &str) {
    assert_eq!(expected.as_array().unwrap().len(), 3, "{path}: TP anchors");
    for (index, actual) in actual.into_iter().enumerate() { exact(actual, &expected[index], &format!("{path}[{index}]")); }
}

pub fn weapon_bonus(case: &Value) {
    let input = &case["input"];
    let actual = get_weapon_bonus(WeaponBonusWeapon::from_name(input["main_name2"].as_str().unwrap()),
        WeaponBonusWeapon::from_name(input["ranged_name2"].as_str().unwrap()),
        WeaponBonusSkill::from_name(input["ws_name"].as_str().unwrap()));
    exact(actual, &case["expected"]["value"]["bonus"], case["id"].as_str().unwrap());
}

/// Validate inert reference specifications through the public production table,
/// not the spelling of the Rust source. All hooks are additionally executed by
/// the complete action-boundary corpus.
pub fn metadata(case: &Value) {
    let expected = case["expected"]["value"]["actual_reference_tables"]["ws_table"].as_object().unwrap();
    assert_eq!(WS_TABLE.len(), expected.len(), "all static weapon skills");
    for (position, (actual, (name, expected))) in WS_TABLE.iter().zip(expected).enumerate() {
        let path = format!("{}.WS_TABLE[{position}].{name}", case["id"].as_str().unwrap());
        assert_eq!(actual.name, name, "{path}: ordered identity");
        assert_eq!(resolve_name(name).unwrap().index(), position, "{path}: lookup");
        assert_eq!(actual.skill_type.name(), expected["skill"].as_str().unwrap(), "{path}: skill classification");
        let fields = &expected["fields"];
        assert_eq!(actual.nhits as u64, fields["nhits"].as_u64().unwrap(), "{path}: attempted hits");
        match actual.ftp {
            TpScaling::Fixed(value) => { exact(value, &fields["ftp"], &path); assert!(fields.get("base_ftp").is_none(), "{path}: fixed fTP"); }
            TpScaling::Interpolated(values) => { points(values, &fields["base_ftp"], &path); assert!(fields.get("ftp").is_none(), "{path}: interpolated fTP"); }
        }
        let terms = fields["wsc"]["parameters"]["pairs"].as_array().unwrap();
        assert_eq!(actual.wsc.terms().len(), terms.len(), "{path}: WSC terms");
        for (index, ((coefficient, stat), expected)) in actual.wsc.terms().iter().zip(terms).enumerate() {
            exact(*coefficient, &expected[0], &format!("{path}.wsc[{index}]"));
            let attribute = match stat {
                Stat::Str => "player_str", Stat::Dex => "player_dex", Stat::Vit => "player_vit",
                Stat::Agi => "player_agi", Stat::Int => "player_int", Stat::Mnd => "player_mnd", Stat::Chr => "player_chr",
                other => panic!("{path}: unsupported primary WSC {other:?}"),
            };
            assert_eq!(attribute, expected[1].as_str().unwrap(), "{path}: WSC stat/order");
        }
        assert_eq!(actual.ftp_rep, fields["ftp_rep"].as_bool().unwrap_or(false), "{path}: fTP replication");
        assert_eq!(actual.magical, fields["magical"].as_bool().unwrap_or(false), "{path}: magical");
        assert_eq!(actual.hybrid, fields["hybrid"].as_bool().unwrap_or(false), "{path}: hybrid");
        assert_eq!(actual.element.name(), fields["element"].as_str().unwrap_or("None"), "{path}: element");
        match actual.dstat {
            DStatFormula::Zero => {
                if actual.magical { exact(0.0, &fields["dSTAT"], &path); }
                else { assert!(fields.get("dSTAT").is_none(), "{path}: physical dSTAT absence"); }
            }
            formula => assert_eq!(formula.source_name().unwrap(), fields["dSTAT"]["name"].as_str().unwrap(), "{path}: dSTAT formula"),
        }
        match actual.ftp_hybrid {
            Some(values) => points(values, &fields["base_ftp_hybrid"], &path),
            None => assert!(fields.get("base_ftp_hybrid").is_none(), "{path}: hybrid anchors absence"),
        }
        match actual.crit {
            CritScaling::None => assert!(fields.get("crit").is_none() && fields.get("crit_rate").is_none(), "{path}: noncritical classification"),
            CritScaling::Fixed(value) => exact(value, &fields["crit_rate"], &path),
            CritScaling::Melee(values) | CritScaling::Ranged(values) => {
                let name = if matches!(actual.crit, CritScaling::Melee(_)) { "crit_hook" } else { "ranged_crit_hook" };
                assert_eq!(name, fields["crit"]["name"].as_str().unwrap(), "{path}: critical classification");
                points(values, &fields["crit"]["parameters"]["crit_boost"], &path);
            }
        }
        let hooks = fields["hooks"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        assert_eq!(actual.hooks.len(), hooks.len(), "{path}: ordered hook count");
        for (index, (hook, expected)) in actual.hooks.iter().zip(hooks).enumerate() {
            let path = format!("{path}.hooks[{index}]");
            assert_eq!(hook.source_name(), expected["name"].as_str().unwrap(), "{path}: hook identity/order");
            let args = &expected["parameters"];
            match *hook {
                WsHook::Accuracy { points: values, both_hands } => { points(values, &args["acc_boost"], &path); assert_eq!(both_hands, args["both_hands"].as_bool().unwrap(), "{path}: both hands"); }
                WsHook::RangedAccuracy { points: values } => points(values, &args["acc_boost"], &path),
                WsHook::Attack { modifier, both_hands } => { exact(modifier, &args["modifier"], &path); assert_eq!(both_hands, args["both_hands"].as_bool().unwrap(), "{path}: both hands"); }
                WsHook::AttackUnguarded { modifier } | WsHook::RangedAttack { modifier } => exact(modifier, &args["modifier"], &path),
                WsHook::InterpolatedAttack { points: values, both_hands } => { points(values, &args["atk_boost"], &path); assert_eq!(both_hands, args["both_hands"].as_bool().unwrap(), "{path}: both hands"); }
                WsHook::KickAttack => {} // Its behavior is checked against every captured Footwork/kick boundary.
                WsHook::DefenseDown { points: values } => points(values, &args["scaling"], &path),
            }
        }
    }
}
