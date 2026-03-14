//! GW2-style visual specialization & trait lock panel.
//! Renders hexagons (specs) + 3×3 circle grids (traits) in the content area.
//! Click to lock/unlock — locked items are preserved by the optimizer.

use gw2_core::types::BuildLocks;
use gw2_optimizer::gamedb::GameDb;
use nexus::imgui::Ui;

// ─── Colors ───

const LOCKED_COLOR: [f32; 4] = [1.0, 0.85, 0.2, 1.0]; // Gold — locked
const SELECTED_COLOR: [f32; 4] = [0.5, 0.8, 1.0, 1.0]; // Cyan — selected but unlocked
const DIM_COLOR: [f32; 4] = [0.35, 0.35, 0.35, 0.8]; // Gray — unselected
const AVAILABLE_COLOR: [f32; 4] = [0.5, 0.5, 0.5, 0.6]; // Dim — available but not selected
const LABEL_COLOR: [f32; 4] = [0.75, 0.75, 0.8, 1.0]; // Light gray text
const ELITE_COLOR: [f32; 4] = [1.0, 0.6, 0.2, 1.0]; // Orange for elite marker
const HEADER_COLOR: [f32; 4] = [0.85, 0.72, 0.3, 1.0]; // Gold header
const LOCK_ICON_COLOR: [f32; 4] = [1.0, 0.85, 0.2, 0.8]; // Lock indicator ring

fn color_u32(c: [f32; 4]) -> u32 {
    let r = (c[0] * 255.0).clamp(0.0, 255.0) as u32;
    let g = (c[1] * 255.0).clamp(0.0, 255.0) as u32;
    let b = (c[2] * 255.0).clamp(0.0, 255.0) as u32;
    let a = (c[3] * 255.0).clamp(0.0, 255.0) as u32;
    (a << 24) | (b << 16) | (g << 8) | r
}

// ─── Geometry helpers ───

/// Draw a hexagon at center position with given radius.
fn draw_hexagon(
    draw_list: &nexus::imgui::DrawListMut,
    center: [f32; 2],
    radius: f32,
    color: u32,
    filled: bool,
) {
    let mut points = Vec::with_capacity(6);
    for i in 0..6 {
        let angle = std::f32::consts::PI / 3.0 * i as f32 - std::f32::consts::FRAC_PI_6;
        points.push([
            center[0] + radius * angle.cos(),
            center[1] + radius * angle.sin(),
        ]);
    }
    if filled {
        // Draw as triangles from center
        for i in 0..6 {
            let next = (i + 1) % 6;
            draw_list
                .add_triangle(center, points[i], points[next], color)
                .filled(true)
                .build();
        }
    }
    // Outline
    for i in 0..6 {
        let next = (i + 1) % 6;
        draw_list
            .add_line(points[i], points[next], color)
            .thickness(2.0)
            .build();
    }
}

/// Check if mouse position is within a circle of given radius around center.
fn is_in_circle(mouse: [f32; 2], center: [f32; 2], radius: f32) -> bool {
    let dx = mouse[0] - center[0];
    let dy = mouse[1] - center[1];
    dx * dx + dy * dy <= radius * radius
}

/// Check if mouse position is within a hexagon of given radius around center.
fn is_in_hexagon(mouse: [f32; 2], center: [f32; 2], radius: f32) -> bool {
    // Approximate with circle (close enough for click detection)
    is_in_circle(mouse, center, radius)
}

// ─── Main render function ───

/// Render the spec & trait lock panel in the left menu.
/// Returns true if any lock state was modified.
pub fn render_lock_panel(
    ui: &Ui,
    locks: &mut BuildLocks,
    expanded: &mut bool,
    db: Option<&GameDb>,
    profession_name: &str,
    current_specs: &[(u32, Vec<u32>)], // (spec_id, selected_trait_ids) from current build
) -> bool {
    let mut modified = false;
    let spacing = 4.0_f32;

    // Collapsible header
    ui.dummy([0.0, spacing]);
    {
        let pos = ui.cursor_screen_pos();
        let width = ui.content_region_avail()[0];
        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_rect(
                [pos[0], pos[1]],
                [pos[0] + width, pos[1] + 18.0],
                [0.22, 0.19, 0.10, 0.9],
            )
            .filled(true)
            .build();
        draw_list.add_text(
            [pos[0] + 6.0, pos[1] + 2.0],
            HEADER_COLOR,
            "SPEC & TRAIT LOCKS",
        );
        // Collapse indicator
        let indicator = if *expanded { "v" } else { ">" };
        let iw = ui.calc_text_size(indicator)[0];
        draw_list.add_text(
            [pos[0] + width - iw - 6.0, pos[1] + 2.0],
            HEADER_COLOR,
            indicator,
        );
    }
    // Invisible button for click detection on the header
    if ui.invisible_button("##locks_header", [ui.content_region_avail()[0], 18.0]) {
        *expanded = !*expanded;
    }
    ui.dummy([0.0, spacing * 0.5]);

    if !*expanded {
        return modified;
    }

    let Some(db) = db else {
        ui.text_colored(DIM_COLOR, "  (Load game data first)");
        return modified;
    };

    if profession_name.is_empty() {
        ui.text_colored(DIM_COLOR, "  (Select a character first)");
        return modified;
    }

    let Some(profession) = db.professions.get(profession_name) else {
        return modified;
    };

    // Gather available specs for this profession (reserved for future spec picker)
    let _core_specs: Vec<(u32, &str)> = profession
        .specializations
        .iter()
        .filter_map(|&id| db.specializations.get(&id))
        .filter(|s| !s.elite)
        .map(|s| (s.id, s.name.as_str()))
        .collect();
    let _elite_specs: Vec<(u32, &str)> = profession
        .specializations
        .iter()
        .filter_map(|&id| db.specializations.get(&id))
        .filter(|s| s.elite)
        .map(|s| (s.id, s.name.as_str()))
        .collect();

    let mouse_pos = ui.io().mouse_pos;
    let mouse_clicked = ui.is_mouse_clicked(nexus::imgui::MouseButton::Left);
    let right_clicked = ui.is_mouse_clicked(nexus::imgui::MouseButton::Right);

    // Render 3 spec rows — sized for content area (wide), scale-aware
    let font_size = ui.current_font_size();
    let s = (font_size / 13.0).max(0.5); // derive scale from font size (13px baseline)
    let avail_width = ui.content_region_avail()[0];
    let hex_radius = (22.0 * s).round();
    let hex_area_width = hex_radius * 2.0 + 16.0 * s;
    let trait_area_width = avail_width - hex_area_width - 8.0 * s;
    let circle_radius = (10.0 * s).round();
    let col_spacing = trait_area_width / 3.0;
    let row_height = (28.0 * s).round();

    for slot in 0..3_usize {
        let row_start = ui.cursor_screen_pos();
        let total_row_height = hex_radius * 2.0 + 22.0 * s; // hex + name below
                                                            // Reserve enough height for the grid (max of hex height or 3 trait rows)
        let grid_height = row_height * 3.0 + 8.0 * s;
        let section_height = total_row_height.max(grid_height);

        // ── Hexagon (spec identity) ──
        let hex_center = [
            row_start[0] + hex_area_width / 2.0,
            row_start[1] + section_height / 2.0,
        ];

        let spec_id = locks.specs[slot];
        let spec_locked = spec_id.is_some();
        let spec_name = spec_id
            .and_then(|id| db.specializations.get(&id))
            .map(|s| s.name.as_str())
            .or_else(|| {
                // Fall back to current build spec for this slot
                current_specs
                    .get(slot)
                    .and_then(|(id, _)| db.specializations.get(id).map(|s| s.name.as_str()))
            })
            .unwrap_or("(empty)");
        let is_elite = spec_id
            .and_then(|id| db.specializations.get(&id))
            .is_some_and(|s| s.elite);

        {
            let draw_list = ui.get_window_draw_list();

            // Draw hexagon
            let hex_color = if spec_locked { LOCKED_COLOR } else { DIM_COLOR };
            if spec_locked {
                draw_hexagon(
                    &draw_list,
                    hex_center,
                    hex_radius,
                    color_u32([0.3, 0.25, 0.05, 0.6]),
                    true,
                );
            }
            draw_hexagon(
                &draw_list,
                hex_center,
                hex_radius,
                color_u32(hex_color),
                false,
            );

            // Lock ring around hexagon when locked
            if spec_locked {
                draw_list
                    .add_circle(hex_center, hex_radius + 3.0, color_u32(LOCK_ICON_COLOR))
                    .thickness(2.0)
                    .build();
            }

            // Elite marker
            if is_elite {
                let e_pos = [hex_center[0] - 3.0, hex_center[1] - 5.0];
                draw_list.add_text(e_pos, color_u32(ELITE_COLOR), "E");
            } else if !spec_locked {
                // Slot number
                let num_text = format!("{}", slot + 1);
                let tw = ui.calc_text_size(&num_text)[0];
                draw_list.add_text(
                    [hex_center[0] - tw / 2.0, hex_center[1] - 5.0],
                    color_u32(DIM_COLOR),
                    &num_text,
                );
            }

            // Spec name below hexagon (full name — content area has room)
            let nw = ui.calc_text_size(spec_name)[0];
            draw_list.add_text(
                [hex_center[0] - nw / 2.0, hex_center[1] + hex_radius + 3.0],
                color_u32(LABEL_COLOR),
                spec_name,
            );
        } // DrawListMut dropped

        // Hexagon click detection — toggle lock
        if is_in_hexagon(mouse_pos, hex_center, hex_radius + 4.0) {
            // Tooltip
            ui.tooltip(|| {
                if spec_locked {
                    ui.text(&format!("{} (LOCKED)", spec_name));
                    ui.text_colored(DIM_COLOR, "Click to unlock");
                } else {
                    ui.text(spec_name);
                    ui.text_colored(DIM_COLOR, "Click to lock");
                }
            });

            if mouse_clicked {
                if spec_locked {
                    // Unlock: clear spec lock and associated trait locks
                    if let Some(old_id) = locks.specs[slot] {
                        locks.trait_locks.remove(&old_id);
                    }
                    locks.specs[slot] = None;
                } else {
                    // Lock: use current build's spec for this slot
                    if let Some((cur_id, _)) = current_specs.get(slot) {
                        locks.specs[slot] = Some(*cur_id);
                    }
                }
                modified = true;
            }
        }

        // ── Trait grid (3 columns × 3 rows) ──
        // Only show traits when a spec is set (locked or from current build)
        let active_spec_id = spec_id.or_else(|| current_specs.get(slot).map(|(id, _)| *id));
        let selected_traits: Vec<u32> = current_specs
            .get(slot)
            .map(|(_, traits)| traits.clone())
            .unwrap_or_default();

        if let Some(sid) = active_spec_id {
            if let Some(spec) = db.specializations.get(&sid) {
                if spec.major_traits.len() == 9 {
                    let grid_x = row_start[0] + hex_area_width + 4.0;
                    let grid_y = row_start[1] + 2.0;

                    for col in 0..3_usize {
                        for row in 0..3_usize {
                            let trait_idx = col * 3 + row;
                            let trait_id = spec.major_traits[trait_idx];
                            let trait_info = db.traits.get(&trait_id);
                            let trait_name = trait_info.map(|t| t.name.as_str()).unwrap_or("?");

                            let cx = grid_x + col as f32 * col_spacing + col_spacing / 2.0;
                            let cy = grid_y + row as f32 * row_height + row_height / 2.0;

                            let is_selected = selected_traits.contains(&trait_id);
                            let is_locked = locks
                                .locked_trait(sid, col)
                                .is_some_and(|id| id == trait_id);

                            {
                                let draw_list = ui.get_window_draw_list();

                                // Circle
                                let (fill_color, outline_color) = if is_locked {
                                    (LOCKED_COLOR, LOCKED_COLOR)
                                } else if is_selected {
                                    (SELECTED_COLOR, SELECTED_COLOR)
                                } else {
                                    (AVAILABLE_COLOR, DIM_COLOR)
                                };

                                if is_selected || is_locked {
                                    draw_list
                                        .add_circle([cx, cy], circle_radius, color_u32(fill_color))
                                        .filled(true)
                                        .build();
                                }
                                draw_list
                                    .add_circle([cx, cy], circle_radius, color_u32(outline_color))
                                    .build();

                                // Lock ring
                                if is_locked {
                                    draw_list
                                        .add_circle(
                                            [cx, cy],
                                            circle_radius + 3.0,
                                            color_u32(LOCK_ICON_COLOR),
                                        )
                                        .thickness(1.5)
                                        .build();
                                }

                                // Trait name to the right of circle
                                let text_color = if is_locked {
                                    LOCKED_COLOR
                                } else if is_selected {
                                    SELECTED_COLOR
                                } else {
                                    DIM_COLOR
                                };
                                draw_list.add_text(
                                    [cx + circle_radius + 6.0, cy - 6.0],
                                    color_u32(text_color),
                                    trait_name,
                                );
                            } // DrawListMut dropped

                            // Click/hover detection
                            if is_in_circle(mouse_pos, [cx, cy], circle_radius + 4.0) {
                                // Tooltip with full trait info
                                ui.tooltip(|| {
                                    ui.text(trait_name);
                                    if let Some(t) = trait_info {
                                        if let Some(ref desc) = t.description {
                                            ui.text_wrapped(desc);
                                        }
                                    }
                                    if is_locked {
                                        ui.text_colored(LOCKED_COLOR, "LOCKED");
                                        ui.text_colored(DIM_COLOR, "Click to unlock");
                                    } else {
                                        ui.text_colored(DIM_COLOR, "Click to lock");
                                    }
                                });

                                if mouse_clicked {
                                    if is_locked {
                                        // Unlock this trait
                                        if let Some(cols) = locks.trait_locks.get_mut(&sid) {
                                            cols[col] = None;
                                        }
                                    } else {
                                        // Lock this trait (and select it)
                                        let entry =
                                            locks.trait_locks.entry(sid).or_insert([None; 3]);
                                        entry[col] = Some(trait_id);
                                        // Also lock the spec if not already
                                        if locks.specs[slot].is_none() {
                                            locks.specs[slot] = Some(sid);
                                        }
                                    }
                                    modified = true;
                                }

                                if right_clicked && is_locked {
                                    // Right-click to unlock
                                    if let Some(cols) = locks.trait_locks.get_mut(&sid) {
                                        cols[col] = None;
                                    }
                                    modified = true;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Reserve layout space
        ui.dummy([avail_width, section_height]);
        ui.dummy([0.0, 4.0]); // Gap between spec rows

        // Separator between rows
        {
            let sep_pos = ui.cursor_screen_pos();
            let draw_list = ui.get_window_draw_list();
            draw_list
                .add_line(
                    [sep_pos[0], sep_pos[1] - 2.0],
                    [sep_pos[0] + avail_width, sep_pos[1] - 2.0],
                    color_u32([0.3, 0.25, 0.1, 0.3]),
                )
                .build();
        }
    }

    // ── Lock All / Unlock All buttons ──
    ui.dummy([0.0, 4.0]);
    let btn_width = (avail_width - 6.0) / 2.0;
    if ui.button_with_size("Lock All", [btn_width, 0.0]) {
        // Lock all current build specs and traits
        for (slot, (spec_id, trait_ids)) in current_specs.iter().enumerate() {
            locks.specs[slot] = Some(*spec_id);
            if let Some(spec) = db.specializations.get(spec_id) {
                if spec.major_traits.len() == 9 {
                    let mut cols = [None; 3];
                    for &tid in trait_ids {
                        // Find which column this trait belongs to
                        for col in 0..3 {
                            if spec.major_traits[col * 3..col * 3 + 3].contains(&tid) {
                                cols[col] = Some(tid);
                            }
                        }
                    }
                    locks.trait_locks.insert(*spec_id, cols);
                }
            }
        }
        modified = true;
    }
    ui.same_line();
    if ui.button_with_size("Unlock All", [btn_width, 0.0]) {
        locks.specs = [None; 3];
        locks.trait_locks.clear();
        modified = true;
    }

    // Lock count indicator
    let lock_count = locks.specs.iter().filter(|s| s.is_some()).count()
        + locks
            .trait_locks
            .values()
            .flat_map(|c| c.iter())
            .filter(|t| t.is_some())
            .count();
    if lock_count > 0 {
        ui.text_colored(LOCKED_COLOR, &format!("  {} locks active", lock_count));
    }

    modified
}

/// Render the optimized build's specs & traits in the same visual style as the lock panel.
/// Read-only — no click interactions. Matches the lock panel layout for side-by-side comparison.
pub fn render_optimized_specs_panel(
    ui: &Ui,
    db: Option<&GameDb>,
    suggestion_specs: &[(String, Vec<String>)], // (spec_name, [trait1, trait2, trait3])
) {
    let spacing = 4.0_f32;

    // Header — matches lock panel header layout exactly (4px + 18px header + 2px)
    ui.dummy([0.0, spacing]);
    {
        let pos = ui.cursor_screen_pos();
        let width = ui.content_region_avail()[0];
        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_rect(
                [pos[0], pos[1]],
                [pos[0] + width, pos[1] + 18.0],
                [0.10, 0.19, 0.12, 0.9],
            )
            .filled(true)
            .build();
        draw_list.add_text(
            [pos[0] + 6.0, pos[1] + 2.0],
            color_u32([0.3, 1.0, 0.5, 1.0]),
            "OPTIMIZED SPECS & TRAITS",
        );
    }
    ui.dummy([ui.content_region_avail()[0], 18.0]);
    ui.dummy([0.0, spacing * 0.5]);

    if suggestion_specs.is_empty() {
        ui.text_colored(DIM_COLOR, "  (No optimization result yet)");
        return;
    }

    // Match sizing from lock panel
    let font_size = ui.current_font_size();
    let s = (font_size / 13.0).max(0.5);
    let avail_width = ui.content_region_avail()[0];
    let hex_radius = (22.0 * s).round();
    let hex_area_width = hex_radius * 2.0 + 16.0 * s;
    let trait_area_width = avail_width - hex_area_width - 8.0 * s;
    let circle_radius = (10.0 * s).round();
    let col_spacing = trait_area_width / 3.0;
    let row_height = (28.0 * s).round();

    // Look up spec info from DB for visual rendering
    // Build name→spec map from DB
    let spec_by_name: std::collections::HashMap<&str, &gw2_api::models::Specialization> = db
        .map(|db| {
            db.specializations
                .values()
                .map(|sp| (sp.name.as_str(), sp))
                .collect()
        })
        .unwrap_or_default();

    for (slot, (spec_name, trait_names)) in suggestion_specs.iter().enumerate() {
        let row_start = ui.cursor_screen_pos();
        let total_row_height = hex_radius * 2.0 + 22.0 * s;
        let grid_height = row_height * 3.0 + 8.0 * s;
        let section_height = total_row_height.max(grid_height);

        // Strip " [E]" suffix for DB lookup (suggestion names include elite marker)
        let lookup_name = spec_name.strip_suffix(" [E]").unwrap_or(spec_name.as_str());
        let spec_info = spec_by_name.get(lookup_name);
        let is_elite = spec_info.is_some_and(|s| s.elite) || spec_name.ends_with(" [E]");

        // ── Hexagon (spec identity) ──
        let hex_center = [
            row_start[0] + hex_area_width / 2.0,
            row_start[1] + section_height / 2.0,
        ];

        let optimized_color: [f32; 4] = [0.3, 1.0, 0.5, 1.0]; // Green for optimized

        {
            let draw_list = ui.get_window_draw_list();
            // Filled hexagon
            draw_hexagon(
                &draw_list,
                hex_center,
                hex_radius,
                color_u32([0.05, 0.2, 0.1, 0.6]),
                true,
            );
            draw_hexagon(
                &draw_list,
                hex_center,
                hex_radius,
                color_u32(optimized_color),
                false,
            );

            // Elite marker
            if is_elite {
                let e_pos = [hex_center[0] - 3.0, hex_center[1] - 5.0];
                draw_list.add_text(e_pos, color_u32(ELITE_COLOR), "E");
            } else {
                let num_text = format!("{}", slot + 1);
                let tw = ui.calc_text_size(&num_text)[0];
                draw_list.add_text(
                    [hex_center[0] - tw / 2.0, hex_center[1] - 5.0],
                    color_u32(DIM_COLOR),
                    &num_text,
                );
            }

            // Spec name below hexagon (strip " [E]" — hexagon already has elite marker)
            let display_name = lookup_name;
            let nw = ui.calc_text_size(display_name)[0];
            draw_list.add_text(
                [hex_center[0] - nw / 2.0, hex_center[1] + hex_radius + 3.0],
                color_u32(LABEL_COLOR),
                display_name,
            );
        }

        // ── Trait display (3 columns, 3 rows each) ──
        if let Some(spec) = spec_info {
            if spec.major_traits.len() == 9 {
                let grid_x = row_start[0] + hex_area_width + 4.0;
                let grid_y = row_start[1] + 2.0;

                for col in 0..3_usize {
                    for row in 0..3_usize {
                        let trait_idx = col * 3 + row;
                        let trait_id = spec.major_traits[trait_idx];
                        let trait_info = db.and_then(|d| d.traits.get(&trait_id));
                        let trait_name = trait_info.map(|t| t.name.as_str()).unwrap_or("?");

                        let cx = grid_x + col as f32 * col_spacing + col_spacing / 2.0;
                        let cy = grid_y + row as f32 * row_height + row_height / 2.0;

                        // Check if this trait was selected by the optimizer
                        let is_selected = trait_names.iter().any(|tn| tn == trait_name);

                        {
                            let draw_list = ui.get_window_draw_list();
                            let (fill_color, outline_color) = if is_selected {
                                (optimized_color, optimized_color)
                            } else {
                                (AVAILABLE_COLOR, DIM_COLOR)
                            };

                            if is_selected {
                                draw_list
                                    .add_circle([cx, cy], circle_radius, color_u32(fill_color))
                                    .filled(true)
                                    .build();
                            }
                            draw_list
                                .add_circle([cx, cy], circle_radius, color_u32(outline_color))
                                .build();

                            // Trait name
                            let text_color = if is_selected {
                                optimized_color
                            } else {
                                DIM_COLOR
                            };
                            draw_list.add_text(
                                [cx + circle_radius + 6.0, cy - 6.0],
                                color_u32(text_color),
                                trait_name,
                            );
                        }

                        // Tooltip on hover
                        let mouse_pos = ui.io().mouse_pos;
                        if is_in_circle(mouse_pos, [cx, cy], circle_radius + 4.0) {
                            ui.tooltip(|| {
                                ui.text(trait_name);
                                if let Some(t) = trait_info {
                                    if let Some(ref desc) = t.description {
                                        ui.text_wrapped(desc);
                                    }
                                }
                                if is_selected {
                                    ui.text_colored(optimized_color, "OPTIMIZER SELECTED");
                                }
                            });
                        }
                    }
                }
            }
        } else {
            // No DB data — just show trait names as text
            let grid_x = row_start[0] + hex_area_width + 4.0;
            let grid_y = row_start[1] + 2.0;
            let draw_list = ui.get_window_draw_list();
            for (i, tn) in trait_names.iter().enumerate() {
                draw_list.add_text(
                    [grid_x, grid_y + i as f32 * row_height],
                    color_u32(optimized_color),
                    tn,
                );
            }
        }

        ui.dummy([avail_width, section_height]);
        ui.dummy([0.0, 4.0]);

        // Separator
        {
            let sep_pos = ui.cursor_screen_pos();
            let draw_list = ui.get_window_draw_list();
            draw_list
                .add_line(
                    [sep_pos[0], sep_pos[1] - 2.0],
                    [sep_pos[0] + avail_width, sep_pos[1] - 2.0],
                    color_u32([0.1, 0.25, 0.1, 0.3]),
                )
                .build();
        }
    }
}
