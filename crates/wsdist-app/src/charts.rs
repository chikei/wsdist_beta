//! Plotters draws directly into Slint's reusable, implicitly shared RGB buffer.

use std::fmt;

use plotters::prelude::*;
use slint::{Image, Rgb8Pixel, SharedPixelBuffer};
use wsdist_core::model::{DistributionResult, DpsResult};

use crate::reports::HistogramPresentation;

#[derive(Debug, thiserror::Error)]
pub enum ChartError {
    #[error("Chart dimensions must be nonzero and fit in addressable RGB storage ({width} × {height})")]
    Dimensions { width: u32, height: u32 },
    #[error("{chart}: {message}")]
    Data { chart: &'static str, message: String },
    #[error("{chart}: unable to render plot: {message}")]
    Render { chart: &'static str, message: String },
}

pub struct ChartBuffer {
    pixels: SharedPixelBuffer<Rgb8Pixel>,
}

impl ChartBuffer {
    pub fn new(width: u32, height: u32) -> Result<Self, ChartError> {
        check_dimensions(width, height)?;
        Ok(Self { pixels: SharedPixelBuffer::new(width, height) })
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), ChartError> {
        check_dimensions(width, height)?;
        if self.pixels.width() != width || self.pixels.height() != height {
            self.pixels = SharedPixelBuffer::new(width, height);
        }
        Ok(())
    }

    pub fn dimensions(&self) -> (u32, u32) { (self.pixels.width(), self.pixels.height()) }

    /// Image construction only shares the raster, never copies its bytes.
    /// Drop the previous Image before redrawing to reuse the same allocation;
    /// if an old window still displays it, Slint's COW protects that window.
    pub fn render_dps(&mut self, result: &DpsResult) -> Result<Image, ChartError> {
        const CHART: &str = "DPS plot";
        let (width, height) = self.dimensions();
        let series = [&result.total_series[..], &result.tp_series[..], &result.ws_series[..]];
        let mut x_min = f64::INFINITY;
        let mut x_max = f64::NEG_INFINITY;
        let mut y_min = f64::INFINITY;
        let mut y_max = f64::NEG_INFINITY;
        for points in series {
            for &[time, cumulative] in points {
                let dps = cumulative / time;
                // Matplotlib masks nonfinite points (e.g. a progressing zero-
                // duration round); retain gaps rather than inventing a value.
                if time.is_finite() && dps.is_finite() {
                    x_min = x_min.min(time);
                    x_max = x_max.max(time);
                    y_min = y_min.min(dps);
                    y_max = y_max.max(dps);
                }
            }
        }
        if !x_min.is_finite() || !y_min.is_finite() {
            return Err(ChartError::Data { chart: CHART, message: "No finite timestamped DPS points can be plotted".into() });
        }
        let (x_min, x_max) = padded_range(x_min, x_max, 0.05);
        let (y_min, y_max) = padded_range(y_min, y_max, 0.05);
        let font_size = font_size(width);
        let root = BitMapBackend::with_buffer(self.pixels.make_mut_bytes(), (width, height)).into_drawing_area();
        root.fill(&WHITE).map_err(|error| render_error(CHART, error))?;
        let mut chart = ChartBuilder::on(&root)
            .margin(font_size)
            .x_label_area_size(font_size * 3)
            .y_label_area_size(font_size * 4)
            .build_cartesian_2d(x_min..x_max, y_min..y_max)
            .map_err(|error| render_error(CHART, error))?;
        chart.configure_mesh()
            .x_desc("Time (s)")
            .y_desc("DPS")
            .axis_desc_style(("sans-serif", font_size + 2))
            .label_style(("sans-serif", font_size))
            .disable_mesh()
            .draw().map_err(|error| render_error(CHART, error))?;
        let labels = [
            format!("Total={:7.1}", result.damage / result.final_time),
            format!("TP={:7.1} ({:5.1}%)", result.tp_damage / result.final_time, result.tp_damage / result.damage * 100.0),
            format!("WS={:7.1} ({:5.1}%)", result.ws_damage / result.final_time, result.ws_damage / result.damage * 100.0),
        ];
        // Matplotlib's original three default line colors, in source order.
        let colors = [RGBColor(31, 119, 180), RGBColor(255, 127, 14), RGBColor(44, 160, 44)];
        for ((points, label), color) in series.into_iter().zip(labels).zip(colors) {
            let mut position = 0;
            let mut label = Some(label);
            while position < points.len() {
                while position < points.len() && !finite_dps(points[position]) { position += 1; }
                let start = position;
                while position < points.len() && finite_dps(points[position]) { position += 1; }
                if start == position { continue; }
                let drawn = chart.draw_series(LineSeries::new(
                    points[start..position].iter().map(|&[time, cumulative]| (time, cumulative / time)),
                    color.stroke_width(2),
                )).map_err(|error| render_error(CHART, error))?;
                if let Some(label) = label.take() {
                    drawn.label(label).legend(move |(x, y)| PathElement::new([(x, y), (x + 24, y)], color.stroke_width(2)));
                }
            }
            if let Some(label) = label {
                // Even an entirely nonfinite series retains its source legend;
                // drawing an empty series does not fabricate chart samples.
                chart.draw_series(LineSeries::new(std::iter::empty::<(f64, f64)>(), color.stroke_width(2)))
                    .map_err(|error| render_error(CHART, error))?
                    .label(label).legend(move |(x, y)| PathElement::new([(x, y), (x + 24, y)], color.stroke_width(2)));
            }
        }
        chart.configure_series_labels()
            .background_style(WHITE.mix(0.9))
            .border_style(BLACK)
            .label_font(("sans-serif", font_size))
            .position(SeriesLabelPosition::UpperRight)
            .draw().map_err(|error| render_error(CHART, error))?;
        root.present().map_err(|error| render_error(CHART, error))?;
        drop(chart);
        drop(root);
        Ok(Image::from_rgb8(self.pixels.clone()))
    }

    pub fn render_distribution(&mut self, result: &DistributionResult,
        presentation: &HistogramPresentation) -> Result<Image, ChartError> {
        const CHART: &str = "WS distribution plot";
        let histogram = &result.histogram;
        if histogram.counts.len() != 300 || histogram.density.len() != 300 || histogram.edges.len() != 301 {
            return Err(ChartError::Data { chart: CHART,
                message: "The engine histogram must contain exactly 300 bins and 301 edges".into() });
        }
        if !result.mean.is_finite() || histogram.edges.iter().any(|value| !value.is_finite())
            || histogram.density.iter().any(|value| !value.is_finite() || *value < 0.0)
            || histogram.edges.windows(2).any(|edge| edge[0] >= edge[1]) {
            return Err(ChartError::Data { chart: CHART,
                message: "The engine returned nonfinite or unordered histogram geometry".into() });
        }
        let maximum_density = histogram.density.iter().copied().fold(0.0_f64, f64::max);
        if maximum_density <= 0.0 {
            return Err(ChartError::Data { chart: CHART, message: "The histogram has no positive density".into() });
        }
        let x_min = histogram.edges[0];
        let x_max = histogram.edges[300];
        let (x_min, x_max) = padded_range(x_min, x_max, 0.05);
        let y_max = maximum_density * 1.05;
        let (width, height) = self.dimensions();
        let font_size = font_size(width);
        let root = BitMapBackend::with_buffer(self.pixels.make_mut_bytes(), (width, height)).into_drawing_area();
        root.fill(&WHITE).map_err(|error| render_error(CHART, error))?;
        let content = root.margin(font_size / 2, font_size / 2, font_size / 2, font_size / 2);
        let sidebar_width = width / 5;
        let title_height = (font_size * 5).min(height / 3);
        let (header, body) = content.split_vertically(title_height);
        for (index, line) in presentation.title.iter().enumerate() {
            header.draw(&Text::new(line.as_str(),
                (sidebar_width as i32, (font_size + index as u32 * (font_size + 4)) as i32),
                ("monospace", font_size).into_font().color(&BLACK)))
                .map_err(|error| render_error(CHART, error))?;
        }
        let (annotations, plot) = body.split_horizontally(sidebar_width);
        let annotation_font = (font_size * 7 / 8).max(8);
        let annotation_height = (presentation.annotations.len() as u32 + 1) * (annotation_font + 5);
        let (_, body_height) = annotations.dim_in_pixel();
        let annotation_y = body_height.saturating_sub(annotation_height) / 2;
        for (index, row) in presentation.annotations.iter().enumerate() {
            let text = format!("{:>13} = {:>4}", row.label, row.value);
            annotations.draw(&Text::new(text,
                (0, (annotation_y + (index as u32 + 1) * (annotation_font + 5)) as i32),
                ("monospace", annotation_font).into_font().color(&BLACK)))
                .map_err(|error| render_error(CHART, error))?;
        }
        let mut chart = ChartBuilder::on(&plot)
            .margin(font_size / 2)
            .x_label_area_size(font_size * 3)
            .y_label_area_size(font_size)
            .build_cartesian_2d(x_min..x_max, 0.0..y_max)
            .map_err(|error| render_error(CHART, error))?;
        chart.configure_mesh()
            .x_desc("Damage")
            .y_labels(0)
            .axis_desc_style(("sans-serif", font_size + 2))
            .label_style(("sans-serif", font_size))
            .disable_mesh()
            .draw().map_err(|error| render_error(CHART, error))?;
        // Use the engine's density and all of its exact edges, including the
        // inclusive last edge and already-expanded constant-data range.
        chart.draw_series(histogram.density.iter().enumerate().map(|(index, &density)| {
            Rectangle::new([(histogram.edges[index], 0.0), (histogram.edges[index + 1], density)],
                RGBColor(224, 224, 224).filled())
        })).map_err(|error| render_error(CHART, error))?;
        let outline = std::iter::once((histogram.edges[0], 0.0))
            .chain(histogram.density.iter().enumerate().flat_map(|(index, &density)| {
                [(histogram.edges[index], density), (histogram.edges[index + 1], density)]
            }))
            .chain(std::iter::once((histogram.edges[300], 0.0)));
        chart.draw_series(LineSeries::new(outline, BLACK.stroke_width(1)))
            .map_err(|error| render_error(CHART, error))?;
        // The marker uses the raw mean, not the title's integer truncation.
        chart.draw_series(DashedLineSeries::new([(result.mean, 0.0), (result.mean, y_max)],
            6, 5, BLACK.stroke_width(1)))
            .map_err(|error| render_error(CHART, error))?;
        root.present().map_err(|error| render_error(CHART, error))?;
        drop(chart);
        drop(plot);
        drop(annotations);
        drop(header);
        drop(body);
        drop(content);
        drop(root);
        Ok(Image::from_rgb8(self.pixels.clone()))
    }
}

fn check_dimensions(width: u32, height: u32) -> Result<(), ChartError> {
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32
        || (width as usize).checked_mul(height as usize).and_then(|pixels| pixels.checked_mul(3)).is_none() {
        Err(ChartError::Dimensions { width, height })
    } else { Ok(()) }
}

fn font_size(width: u32) -> u32 { (width / 65).clamp(8, 24) }

fn finite_dps([time, cumulative]: [f64; 2]) -> bool {
    time.is_finite() && (cumulative / time).is_finite()
}

fn padded_range(minimum: f64, maximum: f64, fraction: f64) -> (f64, f64) {
    let span = maximum - minimum;
    let padding = if span == 0.0 { 0.5 } else { span * fraction };
    (minimum - padding, maximum + padding)
}

fn render_error(chart: &'static str, error: impl fmt::Display) -> ChartError {
    ChartError::Render { chart, message: error.to_string() }
}
