use wsdist_core::model::CalcError;
use wsdist_core::simulation::statistics::{histogram, numpy_mean, summarize_distribution};

#[test]
fn histogram_edges_keep_neighbors_and_include_the_final_edge() {
    let values = [0.0, 1.0_f64.next_down(), 1.0, 1.0_f64.next_up(), 300.0];
    let histogram = histogram(&values, 300).unwrap();
    let mut expected = vec![0; 300];
    expected[0] = 2;
    expected[1] = 2;
    expected[299] = 1;
    assert_eq!(histogram.counts, expected);
    assert_eq!(histogram.edges, (0..=300).map(f64::from).collect::<Vec<_>>());
    assert_eq!(histogram.density[0], 0.4);
    assert_eq!(histogram.density[1], 0.4);
    assert_eq!(histogram.density[299], 0.2);
}

#[test]
fn constant_samples_expand_range_without_discarding_density() {
    let histogram = histogram(&[7.2; 20], 300).unwrap();
    let mut expected = vec![0; 300];
    expected[150] = 20;
    assert_eq!(histogram.counts, expected);
    assert_eq!(histogram.edges[0], 7.2 - 0.5);
    assert_eq!(histogram.edges[300], 7.2 + 0.5);
    assert_eq!(histogram.density[150], 20.0 / (histogram.edges[151] - histogram.edges[150]) / 20.0);
    assert!(histogram.density.iter().enumerate().all(|(index, &value)| index == 150 || value == 0.0));
}

#[test]
fn ordered_mean_uses_one_pairwise_block_and_preserves_empty_nonfinite_output() {
    // Observed against the pinned NumPy 2.4.6 contiguous binary64 reduction.
    // A sequential sum, reassociation, or 8192-item blocks produce another mean.
    let period = [1e16, 1.0, -1e16, 2.0, -1.0, 0.1, 2e10, -2e10];
    let values: Vec<_> = period.into_iter().cycle().take(20_000).collect();
    assert_eq!(numpy_mean(&values).to_bits(), 0.2971_f64.to_bits());
    assert!(numpy_mean(&[]).is_nan());
    assert_eq!(numpy_mean(&[-0.0; 129]).to_bits(), 0.0_f64.to_bits());
}

#[test]
fn summaries_keep_sample_order_and_truncate_both_middle_value_titles() {
    let damage = vec![9.5, 1.0, 2.0, 3.0];
    let tp = vec![10.0, 20.0, 30.0, 40.0];
    let result = summarize_distribution(damage.clone(), tp.clone()).unwrap();
    assert_eq!(result.damage, damage);
    assert_eq!(result.tp, tp);
    assert_eq!((result.minimum, result.maximum), (1.0, 9.5));
    assert_eq!(result.mean, 3.875);
    assert_eq!(result.median, 2.5);
    assert_eq!((result.title_mean, result.title_median), (3.0, 2.0));
}

#[test]
fn large_finite_damage_titles_do_not_saturate_to_machine_integers() {
    let result = summarize_distribution(vec![3e19, 4e19], vec![0.0; 2]).unwrap();
    assert_eq!(result.mean, 3.5e19);
    assert_eq!(result.median, 3.5e19);
    assert_eq!(format!("{:.0}", result.title_mean), "35000000000000000000");
    assert_eq!(format!("{:.0}", result.title_median), "35000000000000000000");
}

#[test]
fn nonfinite_or_unrepresentable_ranges_are_errors_not_empty_successes() {
    for (values, expected) in [
        (vec![0.0, f64::NAN], "autodetected range of [nan, nan] is not finite"),
        (vec![0.0, f64::INFINITY], "autodetected range of [0.0, inf] is not finite"),
        (vec![1e20; 2], "Too many bins for data range. Cannot create 300 finite-sized bins."),
    ] {
        let error = histogram(&values, 300).unwrap_err();
        let CalcError::Legacy { category, message, .. } = error else { panic!("wrong error: {error}") };
        assert_eq!(category, "ValueError");
        assert_eq!(message, expected);
    }
}
