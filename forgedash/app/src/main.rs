mod platform;

use forgedash_core::{gfx, launch, model, savestate, ui};
use glow::HasContext;
use std::path::Path;

const CARD_W: f32 = 240.0;
const CARD_H: f32 = 320.0;
const CENTER_X: f32 = 640.0;
const CENTER_Y: f32 = 360.0;

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Coverflow,
    SaveStates,
}

fn slot_label(s: &savestate::SaveSlot) -> String {
    let name = match s.kind {
        savestate::SlotKind::Auto => "Auto".to_string(),
        savestate::SlotKind::Manual(n) => format!("Slot {n}"),
    };
    if s.exists {
        name
    } else {
        format!("{name}  (empty)")
    }
}

/// Read the ROM's directory and build its slot list.
fn load_slots(rom_path: &str) -> Vec<savestate::SaveSlot> {
    let dir = Path::new(rom_path).parent().unwrap_or_else(|| Path::new("."));
    let listing: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    savestate::slots_for(rom_path, &listing)
}

fn main() {
    let lib_path = std::env::args().nth(1).unwrap_or_else(|| "library.json".to_string());
    let lib = match std::fs::read_to_string(&lib_path)
        .map_err(|e| e.to_string())
        .and_then(|s| model::Library::from_json(&s).map_err(|e| format!("{:?}", e)))
    {
        Ok(l) => l,
        Err(e) => {
            run_error_screen(&format!("library error: {e}"));
            return;
        }
    };

    let mut plat = platform::Platform::new(1280, 720).unwrap_or_else(|e| {
        eprintln!("forgedash: EGL/platform init failed: {e}");
        std::process::exit(1);
    });
    let font = include_bytes!("../assets/font.ttf");
    let r = gfx::Renderer::new(&plat.gl, plat.width, plat.height, font);

    let mut titles = Vec::new();
    let mut metas = Vec::new();
    let mut covers: Vec<Option<gfx::Texture>> = Vec::new();
    for g in &lib.games {
        titles.push(r.text_texture(&plat.gl, &g.title, 40.0));
        metas.push(r.text_texture(&plat.gl, &format!("{} · {} · {}", g.year, g.publisher, g.players), 22.0));
        covers.push(gfx::Texture::from_png(&plat.gl, &g.cover_path));
    }

    let no_pad_hint = if !plat.controller_present {
        Some(r.text_texture(&plat.gl, "No controller detected — keyboard only", 22.0))
    } else {
        None
    };

    let ra_exit_path = Path::new("/tmp/forge/ra_exit");
    let ra_toast = std::fs::read_to_string(ra_exit_path).ok().map(|s| {
        let _ = std::fs::remove_file(ra_exit_path);
        r.text_texture(&plat.gl, &format!("RetroArch exited with code {}", s.trim()), 22.0)
    });
    let mut frame: u32 = 0;

    let mut shelf = ui::Shelf::new(lib.games.len());
    let mut anim_pos = 0.0f32;
    let handoff_dir = Path::new("/tmp/forge");
    let mut screen = Screen::Coverflow;
    let mut ss_sel: usize = 0;
    let mut ss_confirm_delete = false;

    let mut slots: Vec<savestate::SaveSlot> = Vec::new();
    let mut slot_labels: Vec<gfx::Texture> = Vec::new();
    let mut preview: Option<gfx::Texture> = None;
    let mut preview_for: i64 = -1;
    let mut hdr: Option<gfx::Texture> = None;

    loop {
        let nav = plat.poll();
        if nav == platform::Nav::Quit {
            break;
        }
        frame += 1;

        match screen {
            Screen::Coverflow => {
                match nav {
                    platform::Nav::Left => shelf.move_left(),
                    platform::Nav::Right => shelf.move_right(),
                    platform::Nav::Up => {
                        let g = &lib.games[shelf.selected];
                        slots = load_slots(&g.rom_path);
                        slot_labels = slots
                            .iter()
                            .map(|s| r.text_texture(&plat.gl, &slot_label(s), 28.0))
                            .collect();
                        hdr = Some(r.text_texture(&plat.gl, &format!("{} — Save States", g.title), 34.0));
                        preview = None;
                        preview_for = -1;
                        ss_sel = 0;
                        ss_confirm_delete = false;
                        screen = Screen::SaveStates;
                    }
                    platform::Nav::Confirm => {
                        let g = &lib.games[shelf.selected];
                        let _ = launch::write_ra_override(handoff_dir, &launch::PadMap::clovercon());
                        let _ = launch::write_handoff(handoff_dir, &g.rom_path, &g.core);
                        println!("launching {} ({})", g.title, g.core);
                        break;
                    }
                    _ => {}
                }

                let target = shelf.selected as f32;
                anim_pos += (target - anim_pos) * 0.25;
                if (target - anim_pos).abs() < 0.001 {
                    anim_pos = target;
                }

                unsafe {
                    plat.gl.clear_color(0.04, 0.06, 0.11, 1.0);
                    plat.gl.clear(glow::COLOR_BUFFER_BIT);
                }
                r.begin(&plat.gl);

                if let Some(h) = &no_pad_hint {
                    r.draw_texture(&plat.gl, h, 40.0, 40.0, h.w as f32, h.h as f32, 0.85);
                }
                if frame < 180 {
                    if let Some(t) = &ra_toast {
                        r.draw_texture(&plat.gl, t, 40.0, 80.0, t.w as f32, t.h as f32, 0.9);
                    }
                }

                r.fill_rect(&plat.gl, CENTER_X - 200.0, CENTER_Y - 240.0, 400.0, 480.0, [0.23, 0.51, 0.96, 0.18]);

                let mut order: Vec<usize> = (0..lib.games.len()).collect();
                order.sort_by(|&a, &b| {
                    let da = (a as f32 - anim_pos).abs();
                    let db = (b as f32 - anim_pos).abs();
                    db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
                });
                for &i in &order {
                    let offset = i as f32 - anim_pos;
                    let slot = ui::slot_for(offset.round() as i32);
                    let x = CENTER_X + offset * ui::CARD_SPACING;
                    let w = CARD_W * slot.scale;
                    let h = CARD_H * slot.scale;
                    if slot.opacity <= 0.0 {
                        continue;
                    }
                    let cx = x - w / 2.0;
                    let cy = CENTER_Y - h / 2.0;
                    match &covers[i] {
                        Some(tex) => r.draw_texture(&plat.gl, tex, cx, cy, w, h, slot.opacity),
                        None => r.fill_rect(&plat.gl, cx, cy, w, h, [0.27, 0.33, 0.42, slot.opacity]),
                    }
                }

                let sel = shelf.selected;
                let t = &titles[sel];
                r.draw_texture(&plat.gl, t, CENTER_X - t.w as f32 / 2.0, CENTER_Y + 200.0, t.w as f32, t.h as f32, 1.0);
                let m = &metas[sel];
                r.draw_texture(&plat.gl, m, CENTER_X - m.w as f32 / 2.0, CENTER_Y + 250.0, m.w as f32, m.h as f32, 0.8);

                plat.present();
                std::thread::sleep(std::time::Duration::from_millis(16));
            }

            Screen::SaveStates => {
                if ss_confirm_delete {
                    match nav {
                        platform::Nav::Confirm => {
                            if let Some(slot) = slots.get(ss_sel) {
                                if slot.exists {
                                    let _ = std::fs::remove_file(&slot.state_path);
                                    let _ = std::fs::remove_file(&slot.thumb_path);
                                }
                            }
                            let g = &lib.games[shelf.selected];
                            slots = load_slots(&g.rom_path);
                            slot_labels = slots
                                .iter()
                                .map(|s| r.text_texture(&plat.gl, &slot_label(s), 28.0))
                                .collect();
                            if ss_sel >= slots.len() && !slots.is_empty() {
                                ss_sel = slots.len() - 1;
                            }
                            preview_for = -1;
                            ss_confirm_delete = false;
                        }
                        platform::Nav::Back => {
                            ss_confirm_delete = false;
                        }
                        _ => {}
                    }
                } else {
                    match nav {
                        platform::Nav::Back => {
                            screen = Screen::Coverflow;
                            continue;
                        }
                        platform::Nav::Up => {
                            if ss_sel > 0 {
                                ss_sel -= 1;
                            }
                        }
                        platform::Nav::Down => {
                            if ss_sel + 1 < slots.len() {
                                ss_sel += 1;
                            }
                        }
                        platform::Nav::Confirm => {
                            if let Some(slot) = slots.get(ss_sel) {
                                if slot.exists {
                                    let g = &lib.games[shelf.selected];
                                    if slot.kind != savestate::SlotKind::Auto {
                                        let _ = std::fs::copy(&slot.state_path, savestate::auto_path(&g.rom_path));
                                    }
                                    let _ = launch::write_ra_override(handoff_dir, &launch::PadMap::clovercon());
                                    let _ = launch::write_handoff(handoff_dir, &g.rom_path, &g.core);
                                    println!("loading {} -> {}", g.title, slot.state_path);
                                    break;
                                }
                            }
                        }
                        platform::Nav::Delete => {
                            if slots.get(ss_sel).map(|s| s.exists).unwrap_or(false) {
                                ss_confirm_delete = true;
                            }
                        }
                        _ => {}
                    }
                }

                if preview_for != ss_sel as i64 {
                    preview = slots.get(ss_sel).and_then(|s| gfx::Texture::from_png(&plat.gl, &s.thumb_path));
                    preview_for = ss_sel as i64;
                }

                unsafe {
                    plat.gl.clear_color(0.04, 0.06, 0.11, 1.0);
                    plat.gl.clear(glow::COLOR_BUFFER_BIT);
                }
                r.begin(&plat.gl);

                if let Some(h) = &hdr {
                    r.draw_texture(&plat.gl, h, 60.0, 50.0, h.w as f32, h.h as f32, 1.0);
                }

                let list_x = 60.0;
                let row_h = 46.0;
                let list_y0 = 140.0;
                for (i, label) in slot_labels.iter().enumerate() {
                    let y = list_y0 + i as f32 * row_h;
                    if i == ss_sel {
                        r.fill_rect(&plat.gl, list_x - 12.0, y - 6.0, 360.0, row_h - 8.0, [0.23, 0.51, 0.96, 0.85]);
                    } else {
                        r.fill_rect(&plat.gl, list_x - 12.0, y - 6.0, 360.0, row_h - 8.0, [0.12, 0.16, 0.22, 0.7]);
                    }
                    r.draw_texture(&plat.gl, label, list_x, y, label.w as f32, label.h as f32, 1.0);
                }

                let pv_x = 460.0;
                let pv_y = 140.0;
                let pv_w = 720.0;
                let pv_h = 440.0;
                match &preview {
                    Some(tex) => r.draw_texture(&plat.gl, tex, pv_x, pv_y, pv_w, pv_h, 1.0),
                    None => r.fill_rect(&plat.gl, pv_x, pv_y, pv_w, pv_h, [0.16, 0.20, 0.27, 1.0]),
                }

                if ss_confirm_delete {
                    r.fill_rect(&plat.gl, 60.0, 620.0, 1120.0, 60.0, [0.4, 0.06, 0.06, 0.95]);
                }

                plat.present();
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        }
    }
}

fn run_error_screen(msg: &str) {
    eprintln!("forgedash: {msg}");
    let mut plat = match platform::Platform::new(1280, 720) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("forgedash: cannot open window: {e}");
            return;
        }
    };
    let font = include_bytes!("../assets/font.ttf");
    let r = gfx::Renderer::new(&plat.gl, plat.width, plat.height, font);
    let tex = r.text_texture(&plat.gl, msg, 32.0);
    loop {
        if plat.poll() == platform::Nav::Quit {
            break;
        }
        unsafe {
            plat.gl.clear_color(0.15, 0.02, 0.02, 1.0);
            plat.gl.clear(glow::COLOR_BUFFER_BIT);
        }
        r.begin(&plat.gl);
        r.draw_texture(&plat.gl, &tex, 80.0, 320.0, tex.w as f32, tex.h as f32, 1.0);
        plat.present();
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
