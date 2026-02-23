//! Interactive 5-axis radar chart for optimization weights and build comparison.
//! Uses ImGui DrawList for custom rendering with dual-polygon overlay support.

use nexus::imgui::Ui;
use gw2_optimizer::scoring::{
    OptimizationWeights, AXIS_LABELS,
    STRIKE_DPS_NORM, CONDI_DPS_NORM, EFFECTIVE_HEALTH_NORM, HEALING_NORM,
};

/// Axis colors (RGBA) for each of the 5 axes.
const AXIS_COLORS: [[f32; 4]; 5] = [
    [1.0, 0.3, 0.3, 1.0], // Power — red
    [1.0, 0.8, 0.2, 1.0], // Disable — yellow
    [0.4, 1.0, 0.4, 1.0], // Condition — green
    [0.3, 0.7, 1.0, 1.0], // Heal — blue
    [0.8, 0.4, 1.0, 1.0], // Sustain — purple
];

/// Color for the user's weight polygon (semi-transparent cyan).
const WEIGHTS_FILL: [f32; 4] = [0.2, 0.7, 1.0, 0.2];
const WEIGHTS_OUTLINE: [f32; 4] = [0.3, 0.8, 1.0, 0.9];

/// Color for the current build overlay (semi-transparent amber).
const CURRENT_FILL: [f32; 4] = [1.0, 0.7, 0.2, 0.15];
const CURRENT_OUTLINE: [f32; 4] = [1.0, 0.7, 0.2, 0.7];

/// Color for the optimized build overlay (semi-transparent green).
const OPTIMIZED_FILL: [f32; 4] = [0.2, 1.0, 0.4, 0.15];
const OPTIMIZED_OUTLINE: [f32; 4] = [0.2, 1.0, 0.4, 0.7];

/// Convert RGBA [f32;4] to ImGui packed u32 color.
fn color_u32(c: [f32; 4]) -> u32 {
    let r = (c[0] * 255.0) as u32;
    let g = (c[1] * 255.0) as u32;
    let b = (c[2] * 255.0) as u32;
    let a = (c[3] * 255.0) as u32;
    (a << 24) | (b << 16) | (g << 8) | r
}

/// Calculate the position of a point on axis `i` at `value` (0.0-1.0).
/// Axis 0 = top (12 o'clock), going clockwise.
fn axis_point(center: [f32; 2], radius: f32, axis_index: usize, value: f32) -> [f32; 2] {
    let angle = std::f32::consts::PI * 2.0 * (axis_index as f32) / 5.0 - std::f32::consts::FRAC_PI_2;
    [
        center[0] + radius * value * angle.cos(),
        center[1] + radius * value * angle.sin(),
    ]
}

/// Render the interactive radar chart for setting optimization weights.
/// Returns true if weights were modified by user interaction.
pub fn render_radar_chart(
    ui: &Ui,
    weights: &mut OptimizationWeights,
    dragging: &mut Option<usize>,
    current_build_perf: Option<&[f64; 5]>,
    optimized_perf: Option<&[f64; 5]>,
) -> bool {
    let mut modified = false;
    let size = 200.0_f32;
    let radius = size * 0.38;
    let cursor_pos = ui.cursor_screen_pos();
    let center = [cursor_pos[0] + size / 2.0, cursor_pos[1] + size / 2.0];

    // Reserve space in layout
    ui.invisible_button("##radar_area", [size, size]);

    let draw_list = ui.get_window_draw_list();

    // Background circle
    draw_list.add_circle(center, radius + 2.0, color_u32([0.15, 0.15, 0.15, 0.8]))
        .filled(true).build();

    // Concentric pentagons (grid lines at 25%, 50%, 75%, 100%)
    for level in &[0.25_f32, 0.5, 0.75, 1.0] {
        let grid_color = color_u32([0.3, 0.3, 0.3, 0.4]);
        for i in 0..5 {
            let p1 = axis_point(center, radius, i, *level);
            let p2 = axis_point(center, radius, (i + 1) % 5, *level);
            draw_list.add_line(p1, p2, grid_color).build();
        }
    }

    // Axis lines from center
    for i in 0..5 {
        let p = axis_point(center, radius, i, 1.0);
        draw_list.add_line(center, p, color_u32([0.4, 0.4, 0.4, 0.5])).build();
    }

    // Current build performance overlay (amber)
    if let Some(perf) = current_build_perf {
        draw_filled_polygon(&draw_list, center, radius, perf, CURRENT_FILL, CURRENT_OUTLINE);
    }

    // Optimized build performance overlay (green)
    if let Some(perf) = optimized_perf {
        draw_filled_polygon(&draw_list, center, radius, perf, OPTIMIZED_FILL, OPTIMIZED_OUTLINE);
    }

    // Weights polygon (interactive, cyan)
    let w = weights.as_array();
    let w_f32: [f64; 5] = w;
    draw_filled_polygon(&draw_list, center, radius, &w_f32, WEIGHTS_FILL, WEIGHTS_OUTLINE);

    // Handle dragging
    let mouse_pos = ui.io().mouse_pos;
    let mouse_down = ui.io().mouse_down[0];

    if mouse_down {
        if dragging.is_none() {
            // Check if mouse is near a handle (start drag)
            for i in 0..5 {
                let handle_pos = axis_point(center, radius, i, w[i] as f32);
                let dx = mouse_pos[0] - handle_pos[0];
                let dy = mouse_pos[1] - handle_pos[1];
                if dx * dx + dy * dy < 144.0 {
                    // 12px radius
                    *dragging = Some(i);
                    break;
                }
            }
        }

        if let Some(axis) = *dragging {
            // Project mouse position onto axis
            let dx = mouse_pos[0] - center[0];
            let dy = mouse_pos[1] - center[1];
            let angle = std::f32::consts::PI * 2.0 * (axis as f32) / 5.0
                - std::f32::consts::FRAC_PI_2;
            let axis_dx = angle.cos();
            let axis_dy = angle.sin();
            let proj = (dx * axis_dx + dy * axis_dy) / radius;
            let new_val = proj.clamp(0.0, 1.0) as f64;
            weights.set_constrained(axis, new_val);
            modified = true;
        }
    } else {
        *dragging = None;
    }

    // Draw handles (filled circles at each weight point)
    for i in 0..5 {
        let handle_pos = axis_point(center, radius, i, w[i] as f32);
        let is_dragged = *dragging == Some(i);
        let handle_radius = if is_dragged { 7.0 } else { 5.0 };
        draw_list
            .add_circle(handle_pos, handle_radius, color_u32(AXIS_COLORS[i]))
            .filled(true)
            .build();
        draw_list
            .add_circle(handle_pos, handle_radius, color_u32([1.0, 1.0, 1.0, 0.8]))
            .build();
    }

    // Axis labels + percentage
    for i in 0..5 {
        let label_pos = axis_point(center, radius + 16.0, i, 1.0);
        let label = format!("{} {:.0}%", AXIS_LABELS[i], w[i] * 100.0);
        let text_size = ui.calc_text_size(&label);
        draw_list.add_text(
            [label_pos[0] - text_size[0] / 2.0, label_pos[1] - text_size[1] / 2.0],
            color_u32(AXIS_COLORS[i]),
            &label,
        );
    }

    // Budget indicator at bottom of chart
    let budget_used = weights.total();
    let budget_pct = (budget_used / gw2_optimizer::scoring::WEIGHT_BUDGET).min(1.0);
    let bar_width = size * 0.8;
    let bar_height = 4.0;
    let bar_x = cursor_pos[0] + (size - bar_width) / 2.0;
    let bar_y = cursor_pos[1] + size + 2.0;

    // Background
    draw_list.add_rect(
        [bar_x, bar_y],
        [bar_x + bar_width, bar_y + bar_height],
        color_u32([0.2, 0.2, 0.2, 0.6]),
    ).filled(true).build();

    // Fill
    let bar_color = if budget_pct > 0.95 {
        [1.0, 0.4, 0.2, 0.9] // near max — orange
    } else {
        [0.3, 0.8, 1.0, 0.7] // normal — cyan
    };
    draw_list.add_rect(
        [bar_x, bar_y],
        [bar_x + bar_width * budget_pct as f32, bar_y + bar_height],
        color_u32(bar_color),
    ).filled(true).build();

    modified
}

/// Draw a filled polygon with outline for 5 axis values.
fn draw_filled_polygon(
    draw_list: &nexus::imgui::DrawListMut,
    center: [f32; 2],
    radius: f32,
    values: &[f64; 5],
    fill_color: [f32; 4],
    outline_color: [f32; 4],
) {
    let fill = color_u32(fill_color);
    let outline = color_u32(outline_color);

    // Filled triangles (fan from center)
    for i in 0..5 {
        let p1 = axis_point(center, radius, i, values[i] as f32);
        let p2 = axis_point(center, radius, (i + 1) % 5, values[(i + 1) % 5] as f32);
        draw_list.add_triangle(center, p1, p2, fill).filled(true).build();
    }

    // Outline
    for i in 0..5 {
        let p1 = axis_point(center, radius, i, values[i] as f32);
        let p2 = axis_point(center, radius, (i + 1) % 5, values[(i + 1) % 5] as f32);
        draw_list.add_line(p1, p2, outline).thickness(2.0).build();
    }
}

/// Render compact preset buttons in a row.
/// Returns Some(preset_weights) if a preset was clicked.
pub fn render_presets(ui: &Ui) -> Option<OptimizationWeights> {
    let mut result = None;
    for (i, (name, preset_fn)) in OptimizationWeights::PRESETS.iter().enumerate() {
        if i > 0 {
            ui.same_line();
        }
        if ui.small_button(name) {
            result = Some(preset_fn());
        }
    }
    result
}

/// Render the legend showing what each polygon color represents.
pub fn render_legend(ui: &Ui, show_current: bool, show_optimized: bool) {
    if show_current {
        ui.text_colored(CURRENT_OUTLINE, "-- Current Build");
    }
    if show_optimized {
        ui.text_colored(OPTIMIZED_OUTLINE, "-- Optimized Build");
    }
    ui.text_colored(WEIGHTS_OUTLINE, "-- Target Weights");
}

/// Compute 5-axis performance values from CombatPerformance, normalized 0.0-1.0.
/// Normalization is relative to reasonable per-class baselines.
/// `profession` is used to adjust expectations per class.
pub fn compute_performance_axes(
    perf: &gw2_optimizer::combat::CombatPerformance,
    _profession: &str,
) -> [f64; 5] {
    // Normalize each axis to 0.0-1.0 using the same constants as score_with_weights()
    // so the radar chart visual matches actual scoring behavior.
    let power = (perf.strike_dps_index / STRIKE_DPS_NORM).min(1.0);
    let disable = ((perf.boon_duration_pct / 100.0) * 0.5
        + (perf.condi_duration_pct / 100.0) * 0.5)
        .min(1.0);
    let condition = ((perf.condition_dps_index / CONDI_DPS_NORM)
        + (perf.condi_duration_pct / 100.0).min(1.0) * 0.15)
        .min(1.0);
    let heal = (perf.healing_power_index / HEALING_NORM).min(1.0);
    let sustain = ((perf.effective_health / EFFECTIVE_HEALTH_NORM)
        + perf.damage_reduction_pct / 100.0)
        .min(1.0);

    [power, disable, condition, heal, sustain]
}

/// Compute 5-axis performance values from CombatMetrics (i32 UI type).
/// Same normalization as `compute_performance_axes` but works with the
/// rounded integer metrics stored on BuildSuggestion and ComparisonState.
pub fn compute_axes_from_metrics(m: &gw2_core::types::CombatMetrics) -> [f64; 5] {
    let power = (m.strike_dps_index as f64 / STRIKE_DPS_NORM).min(1.0);
    let disable = ((m.boon_duration_pct / 100.0) * 0.5
        + (m.condi_duration_pct / 100.0) * 0.5)
        .min(1.0);
    let condition = ((m.condition_dps_index as f64 / CONDI_DPS_NORM)
        + (m.condi_duration_pct / 100.0).min(1.0) * 0.15)
        .min(1.0);
    let heal = (m.healing_index as f64 / HEALING_NORM).min(1.0);
    let sustain = ((m.effective_health as f64 / EFFECTIVE_HEALTH_NORM)
        + m.damage_reduction_pct / 100.0)
        .min(1.0);

    [power, disable, condition, heal, sustain]
}

/// Infer optimization weights from current build stats.
/// Maps stat investment patterns to the 5-axis weight space.
pub fn infer_weights_from_stats(stats: Option<&gw2_core::types::StatBlock>) -> OptimizationWeights {
    let Some(stats) = stats else {
        return OptimizationWeights::preset_balanced();
    };

    // Investment above base values (Power & Precision base = 1000, rest = 0)
    let power_inv = (stats.power - 1000).max(0) as f64;
    let prec_inv = (stats.precision - 1000).max(0) as f64;
    let fer_inv = stats.ferocity as f64;
    let cd_inv = stats.condition_damage as f64;
    let exp_inv = stats.expertise as f64;
    let conc_inv = stats.concentration as f64;
    let hp_inv = stats.healing_power as f64;
    let tough_inv = stats.toughness as f64;
    let vit_inv = stats.vitality as f64;

    // Max possible investment per slot (Ascended Berserker = ~1200 for primary stat)
    let max_inv = 1500.0;

    let power = ((power_inv + prec_inv + fer_inv) / (max_inv * 3.0)).clamp(0.0, 1.0);
    let condition = ((cd_inv * 2.0 + exp_inv) / (max_inv * 3.0)).clamp(0.0, 1.0);
    let sustain = ((tough_inv + vit_inv) / (max_inv * 2.0)).clamp(0.0, 1.0);
    let healing = (hp_inv / max_inv).clamp(0.0, 1.0);
    let disable = ((conc_inv + exp_inv * 0.5) / (max_inv * 1.5)).clamp(0.0, 1.0);

    // Normalize to respect weight budget — real gear can't max everything
    let mut w = OptimizationWeights { power, disable, condition, healing, sustain };
    let total = w.total();
    if total > gw2_optimizer::scoring::WEIGHT_BUDGET {
        let scale = gw2_optimizer::scoring::WEIGHT_BUDGET / total;
        w.power *= scale;
        w.disable *= scale;
        w.condition *= scale;
        w.healing *= scale;
        w.sustain *= scale;
    }
    w
}
