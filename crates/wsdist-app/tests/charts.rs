use wsdist_app::charts::{ChartBuffer, ChartError};
use wsdist_app::reports::HistogramPresentation;
use wsdist_core::model::{DistributionResult, DpsResult};
use wsdist_core::simulation::statistics::summarize_distribution;

fn distribution() -> DistributionResult {
    summarize_distribution(vec![12.5, 13.5, 14.5], vec![75.0; 3]).unwrap()
}
fn presentation() -> HistogramPresentation {
    HistogramPresentation { title: [String::new(), String::new(), String::new()],
        annotations: Vec::new(), mean_label: String::new() }
}
fn assert_histogram_domain_error(buffer: &mut ChartBuffer, result: &DistributionResult) {
    match buffer.render_distribution(result, &presentation()) {
        Err(ChartError::Data { chart, .. }) => assert_eq!(chart, "WS distribution plot"),
        Err(error) => panic!("invalid histogram geometry reached rendering: {error}"),
        Ok(_) => panic!("invalid histogram geometry must be a visible data error, not a plot"),
    }
}

#[test]
fn histogram_rejects_missing_density_counts_or_endpoint_instead_of_plotting_a_different_bin_domain() {
    let mut buffer = ChartBuffer::new(640, 480).unwrap();
    let mut missing_counts = distribution();
    missing_counts.histogram.counts.pop();
    assert_histogram_domain_error(&mut buffer, &missing_counts);
    let mut missing_density = distribution();
    missing_density.histogram.density.pop();
    assert_histogram_domain_error(&mut buffer, &missing_density);
    let mut missing_last_edge = distribution();
    missing_last_edge.histogram.edges.pop();
    assert_histogram_domain_error(&mut buffer, &missing_last_edge);
}

#[test]
fn histogram_requires_finite_strictly_increasing_edges_nonnegative_density_and_a_raw_mean() {
    let mut buffer = ChartBuffer::new(640, 480).unwrap();
    let mut zero_width = distribution();
    zero_width.histogram.edges[1] = zero_width.histogram.edges[0];
    assert_histogram_domain_error(&mut buffer, &zero_width);
    let mut backwards = distribution();
    backwards.histogram.edges[300] = backwards.histogram.edges[299] - 1.0;
    assert_histogram_domain_error(&mut buffer, &backwards);
    let mut nonfinite_edge = distribution();
    nonfinite_edge.histogram.edges[299] = f64::INFINITY;
    assert_histogram_domain_error(&mut buffer, &nonfinite_edge);
    let mut negative_density = distribution();
    negative_density.histogram.density[0] = -0.25;
    assert_histogram_domain_error(&mut buffer, &negative_density);
    let mut nonfinite_density = distribution();
    nonfinite_density.histogram.density[0] = f64::NAN;
    assert_histogram_domain_error(&mut buffer, &nonfinite_density);
    let mut nonfinite_mean = distribution();
    nonfinite_mean.mean = f64::NAN;
    // The finite title's truncated mean is not a substitute for its raw marker.
    assert_histogram_domain_error(&mut buffer, &nonfinite_mean);
    let mut zero_density = distribution();
    zero_density.histogram.density.fill(0.0);
    assert_histogram_domain_error(&mut buffer, &zero_density);
}

#[test]
fn dps_rejects_a_domain_without_any_finite_timestamped_damage_rate() {
    let result = DpsResult {
        final_time: 7200.0, final_tp: 0.0, damage: 0.0, tp_damage: 0.0, ws_damage: 0.0,
        seconds_per_round: 0.0, round_count: 0, ws_count: 0, average_tp_damage: 0.0,
        average_physical_round_damage: 0.0, average_magical_round_damage: 0.0,
        average_ws_damage: 0.0, average_pre_bonus_tp: 0.0, ws_tp_bonus: 0.0,
        total_series: vec![[0.0, 0.0], [f64::NAN, 10.0]],
        tp_series: vec![[1.0, f64::INFINITY]], ws_series: vec![[f64::INFINITY, 20.0]],
        events: Vec::new(),
    };
    let mut buffer = ChartBuffer::new(640, 480).unwrap();
    match buffer.render_dps(&result) {
        Err(ChartError::Data { chart, .. }) => assert_eq!(chart, "DPS plot"),
        Err(error) => panic!("nonfinite DPS domain reached rendering: {error}"),
        Ok(_) => panic!("nonfinite DPS samples must not be clamped into a fabricated finite curve"),
    }
}

#[test]
fn rejected_resize_keeps_the_existing_raster_storage_geometry() {
    let mut buffer = ChartBuffer::new(640, 480).unwrap();
    let before = buffer.dimensions();
    for (width, height) in [(0, 480), (640, 0), (i32::MAX as u32 + 1, 480)] {
        assert!(matches!(buffer.resize(width, height), Err(ChartError::Dimensions { .. })),
            "unaddressable drawing geometry must fail before replacing the current raster");
        assert_eq!(buffer.dimensions(), before, "failed resize must leave the current chart usable");
    }
}
