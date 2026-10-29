#[allow(dead_code)]
mod combat_support;
#[path = "numerical_boundaries/support.rs"]
#[allow(dead_code)]
mod kernel_support;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};
use wsdist_core::Catalog;

#[test]
fn formula_boundaries_match_python_at_every_threshold() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/boundaries");
    let manifest = wsdist_fixtures::read_value(&root.join("manifest.json"), None).unwrap();
    let groups = manifest["groups"].as_array().unwrap();
    assert_eq!(groups.len() + manifest["rejected_groups"].as_array().unwrap().len(), 89);
    let catalog = Arc::new(Catalog::load(&combat_support::resource_root()).unwrap());
    let mut observations: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    let mut replay_failures = Vec::new();

    for index_name in ["combat.json", "kernels.json"] {
        let index = wsdist_fixtures::read_value(&root.join(index_name), None).unwrap();
        for entry in index["cases"].as_array().unwrap() {
            let case = wsdist_fixtures::read_value(
                &root.join(entry["path"].as_str().unwrap()),
                Some(entry["content_xxh3_128"].as_str().unwrap()),
            ).unwrap();
            assert_eq!(case["id"], entry["id"]);
            assert_eq!(case["operation"], entry["operation"]);
            let group = case["boundary"]["group"].as_str().unwrap();
            let sides = observations.entry(group.to_owned()).or_default();
            if index_name == "combat.json" {
                assert!(case["expected"]["value"].get("scripted").is_some(), "{group}: missing scripted Python execution");
                let counts = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let counts = combat_support::replay_case(&case, &catalog);
                    let fields = counts.last_fields.as_ref().expect("successful scripted boundary");
                    for (name, expected) in case["expected"]["value"]["scripted"]["fields"].as_object().unwrap() {
                        kernel_support::assert_value(&fields[name], expected, true, &format!("{group}.{name}"));
                    }
                    counts
                })) {
                    Ok(counts) => counts,
                    Err(_) => {
                        replay_failures.push(group.to_owned());
                        continue;
                    }
                };
                let observation = json!({"fields": counts.last_fields, "hits": counts.scripted_trace});
                let side = case["boundary"]["side"].as_str().unwrap();
                assert!(sides.insert(side.to_owned(), observation).is_none(), "{group}: duplicate {side}");
            } else {
                assert_eq!(case["expected"]["outcome"], "result", "{group}: kernel capture failed");
                for row in case["expected"]["value"]["rows"].as_array().unwrap() {
                    let side = row["label"].as_str().unwrap();
                    let observation = kernel_support::replay_row(&case, row);
                    kernel_support::assert_value(&observation, &row["result"], true, &format!("{group}.{side}"));
                    assert!(sides.insert(side.to_owned(), observation).is_none(), "{group}: duplicate {side}");
                }
            }
        }
    }

    assert!(replay_failures.is_empty(), "Rust replays diverged from Python: {}", replay_failures.join(", "));
    assert_eq!(observations.len(), groups.len(), "every captured group must be declared");
    for entry in groups {
        let group = entry["group"].as_str().unwrap();
        let pattern = entry["pattern"].as_str().unwrap();
        let python_site = entry["python_site"].as_str().unwrap();
        let rust_site = entry["rust_site"].as_str().unwrap();
        let sides = observations.get(group).unwrap_or_else(|| panic!("{group}: missing observations"));
        assert_eq!(sides.len(), 3, "{group}: expected exactly three sides");
        let below = &sides["below"];
        let equal = &sides["equal"];
        let above = &sides["above"];
        let valid = match pattern {
            "below_differs" => below != equal && equal == above,
            "above_differs" => below == equal && equal != above,
            other => panic!("{group}: unknown boundary pattern {other}"),
        };
        assert!(valid, "{group}: {pattern} violated at {python_site} / {rust_site}");
    }
}
