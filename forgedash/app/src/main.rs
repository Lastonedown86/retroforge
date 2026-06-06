mod platform;

use forgedash_core::{gfx, launch, model, ui};
use glow::HasContext;
use std::path::Path;

const CARD_W: f32 = 240.0;
const CARD_H: f32 = 320.0;
const CENTER_X: f32 = 640.0;
const CENTER_Y: f32 = 360.0;

fn main() {
    // Library path: first CLI arg, else ./library.json
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

    let mut plat = platform::Platform::new(1280, 720).expect("platform init");
    let font = include_bytes!("../assets/font.ttf");
    let r = gfx::Renderer::new(&plat.gl, plat.width, plat.height, font);

    // Pre-rasterize per-game text + load covers once.
    let mut titles = Vec::new();
    let mut metas = Vec::new();
    let mut covers: Vec<Option<gfx::Texture>> = Vec::new();
    for g in &lib.games {
        titles.push(r.text_texture(&plat.gl, &g.title, 40.0));
        metas.push(r.text_texture(&plat.gl, &format!("{} · {} · {}", g.year, g.publisher, g.players), 22.0));
        covers.push(gfx::Texture::from_png(&plat.gl, &g.cover_path));
    }

    let mut shelf = ui::Shelf::new(lib.games.len());
    let mut anim_pos = 0.0f32; // animated selected index
    let handoff_dir = Path::new("/tmp/forge");

    loop {
        match plat.poll() {
            platform::Nav::Quit => break,
            platform::Nav::Left => shelf.move_left(),
            platform::Nav::Right => shelf.move_right(),
            platform::Nav::Confirm => {
                let g = &lib.games[shelf.selected];
                let _ = launch::write_handoff(handoff_dir, &g.rom_path, &g.core);
                println!("launching {} ({})", g.title, g.core);
                break; // forge-loop.sh takes over from here
            }
            platform::Nav::None => {}
        }

        // ease the animated index toward the selection
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

        // glow behind the center cover (modern-dark theme accent)
        r.fill_rect(&plat.gl, CENTER_X - 200.0, CENTER_Y - 240.0, 400.0, 480.0, [0.23, 0.51, 0.96, 0.18]);

        // draw covers far-to-near so the center lands on top
        let mut order: Vec<usize> = (0..lib.games.len()).collect();
        order.sort_by(|&a, &b| {
            let da = (a as f32 - anim_pos).abs();
            let db = (b as f32 - anim_pos).abs();
            db.partial_cmp(&da).unwrap()
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

        // title + metadata for the selected game
        let sel = shelf.selected;
        let t = &titles[sel];
        r.draw_texture(&plat.gl, t, CENTER_X - t.w as f32 / 2.0, CENTER_Y + 200.0, t.w as f32, t.h as f32, 1.0);
        let m = &metas[sel];
        r.draw_texture(&plat.gl, m, CENTER_X - m.w as f32 / 2.0, CENTER_Y + 250.0, m.w as f32, m.h as f32, 0.8);

        plat.present();
        std::thread::sleep(std::time::Duration::from_millis(16));
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
