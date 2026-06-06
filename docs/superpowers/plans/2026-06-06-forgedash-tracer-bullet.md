# ForgeDash Tracer-Bullet Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build ForgeDash, a custom on-device launcher that renders a coverflow shelf via Mali GLES2, navigates by controller, and launches ROMs into RetroArch — proven end-to-end on the NES Classic with zero NAND writes.

**Architecture:** A standalone Rust binary using SDL2 (window + GLES2 context + game-controller input) and `glow` (GLES2 draw calls). Pure logic (library model, shelf navigation, coverflow layout math) is unit-tested off-device; rendering and platform glue are verified by a desktop dev build and a hardware-acceptance runbook. A shell-loop supervisor (`forge-loop.sh`) runs ForgeDash and RetroArch one at a time (one GLES app holds the Mali EGL surface at any moment). The device build dynamically links the device's own `libSDL2`/`libMali`, mirroring RetroArch's proven dependency chain.

**Tech Stack:** Rust (`armv7-unknown-linux-gnueabihf` for device, host triple for dev), `sdl2`, `glow` (GLES2), `image` (PNG), `fontdue` (glyph rasterization), `serde`/`serde_json`. GLSL ES 1.00 shaders.

**Spec:** `docs/superpowers/specs/2026-06-06-forgedash-tracer-bullet-design.md`

---

## File Structure (Cargo workspace)

A two-crate workspace cleanly separates pure, host-testable logic from the
SDL2/GLES2 layer that only builds with a C toolchain + (for the device) the
device's own libs. This means the pure tests run anywhere — including a headless
dev box with only rustc + the MSVC linker (no CMake / C compiler).

```
forgedash/
  Cargo.toml              # [workspace] members = ["core", "app"]
  core/                   # forgedash-core — PURE Rust, no SDL2/GL. Fully unit-tested.
    Cargo.toml            #   deps: serde, serde_json
    src/lib.rs            #   pub mod model; pub mod ui; pub mod launch;
    src/model.rs          #   Game, Library, library.json parsing
    src/ui.rs             #   Shelf nav + coverflow slot math + easing
    src/launch.rs         #   handoff file format + write
  app/                    # forgedash — the binary. Needs SDL2 + GL toolchain.
    Cargo.toml            #   deps: forgedash-core (path), sdl2, glow, image, fontdue
    .cargo/config.toml    #   armv7 cross linker (added in Task 8)
    assets/font.ttf       #   OFL TTF, embedded via include_bytes!
    library.json          #   dev fixture (Task 7)
    art/placeholder.png   #   dev fixture (Task 7)
    src/main.rs           #   entry: load library, run loop, draw shelf
    src/platform.rs       #   SDL2 window + GLES2 ctx + input + frame loop
    src/gfx.rs            #   GLES2 renderer: quads, textures, text
  scripts/
    forge-loop.sh         #   supervisor: run dashboard -> read handoff -> run RA -> loop
    stage.sh              #   host: tar-over-ssh staging into device tmpfs
docs/HARDWARE-ACCEPTANCE-FORGEDASH.md   # hardware proof runbook (Task 9)
```

`core` = pure logic (testable everywhere). `app` = all SDL/GL/device interaction.
Files that change together live together.

### Crate map & verification commands (applies to all later tasks)

| Task | Crate / files | Verify with |
|---|---|---|
| 1 model, 2–3 ui, 4 launch | `core/src/*` (declared in `core/src/lib.rs`) | `cargo test -p forgedash-core <filter>` — **runs in this env** |
| 5 platform, 6 gfx, 7 main | `app/src/*` | `cargo build -p forgedash --features bundled` then run — **needs CMake + C compiler (VS Build Tools); deferred if absent** |
| 8 cross + scripts | `app/.cargo/`, `scripts/` | `cargo build -p forgedash --release --target armv7-unknown-linux-gnueabihf` — **needs ARM toolchain + device sysroot; deferred** |
| 9 runbook | `docs/` | doc only |

- In `app/src/main.rs`, import core via `use forgedash_core::{model, ui, launch};`
  (crate name `forgedash-core` → path `forgedash_core`). Where later task code
  says `mod model;` etc., that declaration lives in `core/src/lib.rs`; the app
  uses the `use forgedash_core::...` import instead.
- Where later tasks say a path like `forgedash/src/model.rs`, read it as
  `forgedash/core/src/model.rs` (pure modules) or `forgedash/app/src/...`
  (platform/gfx/main) per the table above.
- Where later tasks say `--manifest-path forgedash/Cargo.toml`, use the
  `-p forgedash-core` (tests) or `-p forgedash` (app) form above instead.

---

## Task 0: Scaffold the workspace

**Files:**
- Create: `forgedash/Cargo.toml` (workspace)
- Create: `forgedash/core/Cargo.toml`, `forgedash/core/src/lib.rs`
- Create: `forgedash/app/Cargo.toml`, `forgedash/app/src/main.rs`

- [ ] **Step 1: Create the workspace manifest**

Create `forgedash/Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["core", "app"]
```

- [ ] **Step 2: Create the core crate**

Create `forgedash/core/Cargo.toml`:

```toml
[package]
name = "forgedash-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

Create `forgedash/core/src/lib.rs`:

```rust
pub mod model;
pub mod ui;
pub mod launch;
```

> The three modules are added in Tasks 1–4. Until then, comment out the lines
> for modules that don't exist yet, or create empty `model.rs`/`ui.rs`/`launch.rs`
> stubs so the crate compiles. Simplest: create the three files empty now.

Create empty `forgedash/core/src/model.rs`, `forgedash/core/src/ui.rs`,
`forgedash/core/src/launch.rs` (Tasks 1–4 fill them).

- [ ] **Step 3: Create the app crate**

Create `forgedash/app/Cargo.toml`:

```toml
[package]
name = "forgedash"
version = "0.1.0"
edition = "2021"

[dependencies]
forgedash-core = { path = "../core" }
sdl2 = "0.37"
glow = "0.14"
image = { version = "0.25", default-features = false, features = ["png"] }
fontdue = "0.9"

[features]
default = []
# Desktop dev build (needs CMake + a C compiler): `cargo run -p forgedash --features bundled`
# (builds SDL2 from source + ships ANGLE for a GLES2 context).
# Device build omits this and dynamically links the device's libSDL2.
bundled = ["sdl2/bundled", "sdl2/static-link"]
```

Create `forgedash/app/src/main.rs`:

```rust
fn main() {
    println!("ForgeDash 0.1.0");
}
```

- [ ] **Step 4: Verify the core crate builds (works in any env)**

Run: `cargo build -p forgedash-core`
Expected: compiles clean (only serde/serde_json — pure Rust, no C toolchain).

> The `app` crate is NOT built here: `sdl2` needs CMake + a C compiler. If those
> are absent, skip building `app` until a suitably equipped box / the device.
> Confirm with `cargo metadata --no-deps` that both crates are recognized.

- [ ] **Step 5: Commit**

```bash
git add forgedash/Cargo.toml forgedash/core forgedash/app
git commit -m "feat(forgedash): scaffold core+app workspace"
```

---

## Task 1: Library model + JSON parsing (PURE, TDD)

**Files:**
- Create: `forgedash/src/model.rs`
- Modify: `forgedash/src/main.rs` (declare `mod model;`)

- [ ] **Step 1: Declare the module**

In `forgedash/src/main.rs`, add at the top (above `fn main`):

```rust
mod model;
```

- [ ] **Step 2: Write the failing tests**

Create `forgedash/src/model.rs`:

```rust
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Game {
    pub title: String,
    pub year: u32,
    pub publisher: String,
    pub players: String,
    pub cover_path: String,
    pub rom_path: String,
    pub core: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Library {
    pub games: Vec<Game>,
}

#[derive(Debug, PartialEq)]
pub enum LibraryError {
    Parse(String),
    Empty,
}

impl Library {
    pub fn from_json(_s: &str) -> Result<Library, LibraryError> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
      {"title":"Super Mario Bros. 3","year":1988,"publisher":"Nintendo",
       "players":"1-2P","cover_path":"art/smb3.png","rom_path":"roms/smb3.nes","core":"fceumm"}
    ]"#;

    #[test]
    fn parse_valid_library() {
        let lib = Library::from_json(SAMPLE).unwrap();
        assert_eq!(lib.games.len(), 1);
        assert_eq!(lib.games[0].title, "Super Mario Bros. 3");
        assert_eq!(lib.games[0].year, 1988);
        assert_eq!(lib.games[0].core, "fceumm");
    }

    #[test]
    fn empty_array_is_error() {
        assert_eq!(Library::from_json("[]"), Err(LibraryError::Empty));
    }

    #[test]
    fn malformed_json_is_parse_error() {
        match Library::from_json("not json") {
            Err(LibraryError::Parse(_)) => {}
            other => panic!("expected Parse error, got {:?}", other),
        }
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path forgedash/Cargo.toml model::`
Expected: tests fail / panic at `unimplemented!()`.

- [ ] **Step 4: Implement `from_json`**

Replace the `from_json` body in `forgedash/src/model.rs`:

```rust
    pub fn from_json(s: &str) -> Result<Library, LibraryError> {
        let games: Vec<Game> =
            serde_json::from_str(s).map_err(|e| LibraryError::Parse(e.to_string()))?;
        if games.is_empty() {
            return Err(LibraryError::Empty);
        }
        Ok(Library { games })
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path forgedash/Cargo.toml model::`
Expected: 3 passed.

- [ ] **Step 6: Commit**

```bash
git add forgedash/src/model.rs forgedash/src/main.rs
git commit -m "feat(forgedash): library model + json parsing"
```

---

## Task 2: Shelf navigation (PURE, TDD)

**Files:**
- Create: `forgedash/src/ui.rs`
- Modify: `forgedash/src/main.rs` (declare `mod ui;`)

- [ ] **Step 1: Declare the module**

In `forgedash/src/main.rs`, add `mod ui;` near the other `mod` lines.

- [ ] **Step 2: Write the failing tests**

Create `forgedash/src/ui.rs`:

```rust
/// A single horizontal coverflow row. Selection is clamped (no wrap) for the slice.
#[derive(Debug, Clone, PartialEq)]
pub struct Shelf {
    pub len: usize,
    pub selected: usize,
}

impl Shelf {
    pub fn new(_len: usize) -> Self {
        unimplemented!()
    }
    pub fn move_left(&mut self) {
        unimplemented!()
    }
    pub fn move_right(&mut self) {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_at_zero() {
        let s = Shelf::new(5);
        assert_eq!(s.selected, 0);
        assert_eq!(s.len, 5);
    }

    #[test]
    fn move_right_advances_then_clamps() {
        let mut s = Shelf::new(3);
        s.move_right();
        assert_eq!(s.selected, 1);
        s.move_right();
        assert_eq!(s.selected, 2);
        s.move_right(); // clamp at len-1
        assert_eq!(s.selected, 2);
    }

    #[test]
    fn move_left_clamps_at_zero() {
        let mut s = Shelf::new(3);
        s.move_left();
        assert_eq!(s.selected, 0);
        s.move_right();
        s.move_left();
        assert_eq!(s.selected, 0);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path forgedash/Cargo.toml ui::tests::`
Expected: fail at `unimplemented!()`.

- [ ] **Step 4: Implement `Shelf`**

Replace the three method bodies in `forgedash/src/ui.rs`:

```rust
    pub fn new(len: usize) -> Self {
        Shelf { len, selected: 0 }
    }
    pub fn move_left(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
    pub fn move_right(&mut self) {
        if self.len > 0 && self.selected + 1 < self.len {
            self.selected += 1;
        }
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path forgedash/Cargo.toml ui::tests::`
Expected: 3 passed.

- [ ] **Step 6: Commit**

```bash
git add forgedash/src/ui.rs forgedash/src/main.rs
git commit -m "feat(forgedash): shelf navigation"
```

---

## Task 3: Coverflow layout math + easing (PURE, TDD)

**Files:**
- Modify: `forgedash/src/ui.rs`

- [ ] **Step 1: Write the failing tests**

Append to `forgedash/src/ui.rs` (above the existing `#[cfg(test)]` module, add the public items; then add the new tests inside the existing `tests` module):

Add these public items after the `impl Shelf` block:

```rust
/// Where a cover sits relative to screen center, given its distance (in cards)
/// from the selected card. `x` is pixel offset from center; `scale` and
/// `opacity` fall off with distance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoverSlot {
    pub x: f32,
    pub scale: f32,
    pub opacity: f32,
}

pub const CARD_SPACING: f32 = 220.0;

pub fn slot_for(offset: i32) -> CoverSlot {
    unimplemented!()
}

/// Cubic ease-out for selection slide. t in [0,1] -> [0,1].
pub fn ease_out_cubic(t: f32) -> f32 {
    unimplemented!()
}
```

Add these tests inside the existing `mod tests { ... }` block:

```rust
    #[test]
    fn center_slot_is_full() {
        let s = slot_for(0);
        assert_eq!(s.x, 0.0);
        assert_eq!(s.scale, 1.0);
        assert_eq!(s.opacity, 1.0);
    }

    #[test]
    fn neighbor_slots_are_symmetric_and_smaller() {
        let r = slot_for(1);
        let l = slot_for(-1);
        assert_eq!(r.x, CARD_SPACING);
        assert_eq!(l.x, -CARD_SPACING);
        assert_eq!(r.scale, l.scale);
        assert!(r.scale < 1.0);
        assert!(r.opacity < 1.0 && r.opacity > 0.0);
    }

    #[test]
    fn far_slots_fade_to_zero() {
        assert_eq!(slot_for(5).opacity, 0.0);
    }

    #[test]
    fn easing_endpoints_and_monotonic() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        assert!(ease_out_cubic(0.5) > 0.5); // ease-out is above the diagonal mid-way
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path forgedash/Cargo.toml ui::tests::`
Expected: fail at `unimplemented!()` for the new tests.

- [ ] **Step 3: Implement `slot_for` and `ease_out_cubic`**

Replace the two `unimplemented!()` bodies:

```rust
pub fn slot_for(offset: i32) -> CoverSlot {
    let dist = offset.abs() as f32;
    let scale = if offset == 0 { 1.0 } else { 0.7 };
    let opacity = (1.0 - dist * 0.35).max(0.0);
    CoverSlot {
        x: offset as f32 * CARD_SPACING,
        scale,
        opacity,
    }
}

pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let p = 1.0 - t;
    1.0 - p * p * p
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path forgedash/Cargo.toml ui::tests::`
Expected: 7 passed (3 from Task 2 + 4 new).

- [ ] **Step 5: Commit**

```bash
git add forgedash/src/ui.rs
git commit -m "feat(forgedash): coverflow slot math + easing"
```

---

## Task 4: Launch handoff (PURE format TDD + file write)

**Files:**
- Create: `forgedash/src/launch.rs`
- Modify: `forgedash/src/main.rs` (declare `mod launch;`)

- [ ] **Step 1: Declare the module**

In `forgedash/src/main.rs`, add `mod launch;`.

- [ ] **Step 2: Write the failing test**

Create `forgedash/src/launch.rs`:

```rust
use std::io;
use std::path::Path;

/// One handoff line consumed by forge-loop.sh: `<core>\t<rom_path>\n`.
pub fn handoff_line(rom_path: &str, core: &str) -> String {
    unimplemented!()
}

/// Write the handoff line to `<dir>/handoff`.
pub fn write_handoff(dir: &Path, rom_path: &str, core: &str) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("handoff"), handoff_line(rom_path, core))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_line_is_tab_separated() {
        assert_eq!(handoff_line("roms/smb3.nes", "fceumm"), "fceumm\troms/smb3.nes\n");
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --manifest-path forgedash/Cargo.toml launch::`
Expected: fail at `unimplemented!()`.

- [ ] **Step 4: Implement `handoff_line`**

Replace its body:

```rust
pub fn handoff_line(rom_path: &str, core: &str) -> String {
    format!("{}\t{}\n", core, rom_path)
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --manifest-path forgedash/Cargo.toml launch::`
Expected: 1 passed.

- [ ] **Step 6: Commit**

```bash
git add forgedash/src/launch.rs forgedash/src/main.rs
git commit -m "feat(forgedash): launch handoff format + write"
```

---

## Task 5: Platform layer — SDL2 window + GLES2 context + input

> Not unit-tested (owns SDL/GL/device state). Verified by building and running the desktop dev build: a fullscreen window opens and logs nav events.

**Files:**
- Create: `forgedash/src/platform.rs`
- Modify: `forgedash/src/main.rs`

- [ ] **Step 1: Declare the module**

In `forgedash/src/main.rs`, add `mod platform;`.

- [ ] **Step 2: Implement the platform layer**

Create `forgedash/src/platform.rs`:

```rust
/// Abstract navigation events, mapped from both keyboard (dev) and controller (device).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Nav {
    None,
    Left,
    Right,
    Confirm,
    Quit,
}

pub struct Platform {
    _sdl: sdl2::Sdl,
    video: sdl2::VideoSubsystem,
    window: sdl2::video::Window,
    _gl_ctx: sdl2::video::GLContext,
    pub gl: glow::Context,
    event_pump: sdl2::EventPump,
    _controller_sys: sdl2::GameControllerSubsystem,
    _controller: Option<sdl2::controller::GameController>,
    pub width: u32,
    pub height: u32,
}

impl Platform {
    pub fn new(width: u32, height: u32) -> Result<Platform, String> {
        let sdl = sdl2::init()?;
        let video = sdl.video()?;

        let gl_attr = video.gl_attr();
        gl_attr.set_context_profile(sdl2::video::GLProfile::GLES);
        gl_attr.set_context_version(2, 0);
        gl_attr.set_double_buffer(true);

        let window = video
            .window("ForgeDash", width, height)
            .opengl()
            .fullscreen_desktop()
            .build()
            .map_err(|e| e.to_string())?;

        let gl_ctx = window.gl_create_context()?;
        window.gl_make_current(&gl_ctx)?;

        let gl = unsafe {
            glow::Context::from_loader_function(|s| video.gl_get_proc_address(s) as *const _)
        };

        let controller_sys = sdl.game_controller()?;
        let n = controller_sys.num_joysticks().unwrap_or(0);
        let controller = (0..n).find_map(|i| controller_sys.open(i).ok());
        if controller.is_none() {
            eprintln!("forgedash: no controller detected (keyboard nav only)");
        }

        let event_pump = sdl.event_pump()?;

        Ok(Platform {
            _sdl: sdl,
            video,
            window,
            _gl_ctx: gl_ctx,
            gl,
            event_pump,
            _controller_sys: controller_sys,
            _controller: controller,
            width,
            height,
        })
    }

    /// Drain the event queue, returning the first meaningful nav event this frame.
    pub fn poll(&mut self) -> Nav {
        use sdl2::controller::Button;
        use sdl2::event::Event;
        use sdl2::keyboard::Keycode;

        let mut result = Nav::None;
        for event in self.event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => return Nav::Quit,
                Event::KeyDown { keycode: Some(k), .. } => match k {
                    Keycode::Left => result = Nav::Left,
                    Keycode::Right => result = Nav::Right,
                    Keycode::Return | Keycode::Space => result = Nav::Confirm,
                    Keycode::Escape => return Nav::Quit,
                    _ => {}
                },
                Event::ControllerButtonDown { button, .. } => match button {
                    Button::DPadLeft => result = Nav::Left,
                    Button::DPadRight => result = Nav::Right,
                    Button::A => result = Nav::Confirm,
                    _ => {}
                },
                _ => {}
            }
        }
        result
    }

    pub fn present(&self) {
        self.window.gl_swap_window();
    }
}
```

- [ ] **Step 3: Temporarily exercise it from main**

Replace `fn main` in `forgedash/src/main.rs` with:

```rust
fn main() {
    let mut plat = platform::Platform::new(1280, 720).expect("platform init");
    use glow::HasContext;
    loop {
        let nav = plat.poll();
        if nav == platform::Nav::Quit {
            break;
        }
        if nav != platform::Nav::None {
            println!("nav: {:?}", nav);
        }
        unsafe {
            plat.gl.clear_color(0.05, 0.07, 0.12, 1.0);
            plat.gl.clear(glow::COLOR_BUFFER_BIT);
        }
        plat.present();
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
```

- [ ] **Step 4: Build and run the desktop dev build**

Run: `cargo run --features bundled --manifest-path forgedash/Cargo.toml`
Expected: a dark-blue fullscreen window opens. Arrow keys / Enter print `nav: Left/Right/Confirm`. Esc closes it. No panic.

- [ ] **Step 5: Commit**

```bash
git add forgedash/src/platform.rs forgedash/src/main.rs
git commit -m "feat(forgedash): SDL2 window + GLES2 context + input"
```

---

## Task 6: GFX renderer — quads, textures, text

> Verified by the desktop dev build (visual). Coordinates are pixel-space, top-left origin, mapped to clip space in the vertex shader.

**Files:**
- Create: `forgedash/src/gfx.rs`
- Create: `forgedash/assets/font.ttf`
- Modify: `forgedash/src/main.rs`

- [ ] **Step 1: Add an embeddable font**

Place an OFL-licensed TrueType font at `forgedash/assets/font.ttf` (e.g. copy `DejaVuSans.ttf`, or download Inter/Roboto Regular). It is embedded at compile time via `include_bytes!`, so any single `.ttf` works.

- [ ] **Step 2: Implement the renderer**

Create `forgedash/src/gfx.rs`:

```rust
use glow::HasContext;

pub struct Texture {
    pub id: glow::Texture,
    pub w: u32,
    pub h: u32,
}

pub struct Renderer {
    program: glow::Program,
    vbo: glow::Buffer,
    vao: glow::VertexArray,
    u_screen: glow::UniformLocation,
    u_rect: glow::UniformLocation,
    u_color: glow::UniformLocation,
    u_use_tex: glow::UniformLocation,
    font: fontdue::Font,
    screen_w: f32,
    screen_h: f32,
}

const VERT: &str = r#"#version 100
attribute vec2 a_pos;     // unit quad 0..1
uniform vec2 u_screen;    // pixels
uniform vec4 u_rect;      // x, y, w, h in pixels (top-left origin)
varying vec2 v_uv;
void main() {
    v_uv = a_pos;
    vec2 px = u_rect.xy + a_pos * u_rect.zw;
    vec2 ndc = vec2(px.x / u_screen.x * 2.0 - 1.0,
                    1.0 - px.y / u_screen.y * 2.0);
    gl_Position = vec4(ndc, 0.0, 1.0);
}
"#;

const FRAG: &str = r#"#version 100
precision mediump float;
varying vec2 v_uv;
uniform vec4 u_color;
uniform sampler2D u_tex;
uniform float u_use_tex;
void main() {
    if (u_use_tex > 0.5) {
        gl_FragColor = texture2D(u_tex, v_uv) * u_color;
    } else {
        gl_FragColor = u_color;
    }
}
"#;

impl Renderer {
    pub fn new(gl: &glow::Context, screen_w: u32, screen_h: u32, font_bytes: &[u8]) -> Renderer {
        unsafe {
            let program = gl.create_program().unwrap();
            for (ty, src) in [(glow::VERTEX_SHADER, VERT), (glow::FRAGMENT_SHADER, FRAG)] {
                let sh = gl.create_shader(ty).unwrap();
                gl.shader_source(sh, src);
                gl.compile_shader(sh);
                if !gl.get_shader_compile_status(sh) {
                    panic!("shader compile error: {}", gl.get_shader_info_log(sh));
                }
                gl.attach_shader(program, sh);
            }
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                panic!("program link error: {}", gl.get_program_info_log(program));
            }

            let verts: [f32; 12] = [
                0.0, 0.0, 1.0, 0.0, 1.0, 1.0, // tri 1
                0.0, 0.0, 1.0, 1.0, 0.0, 1.0, // tri 2
            ];
            let vao = gl.create_vertex_array().unwrap();
            gl.bind_vertex_array(Some(vao));
            let vbo = gl.create_buffer().unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck_cast(&verts),
                glow::STATIC_DRAW,
            );
            let loc = gl.get_attrib_location(program, "a_pos").unwrap();
            gl.enable_vertex_attrib_array(loc);
            gl.vertex_attrib_pointer_f32(loc, 2, glow::FLOAT, false, 0, 0);

            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

            let u_screen = gl.get_uniform_location(program, "u_screen").unwrap();
            let u_rect = gl.get_uniform_location(program, "u_rect").unwrap();
            let u_color = gl.get_uniform_location(program, "u_color").unwrap();
            let u_use_tex = gl.get_uniform_location(program, "u_use_tex").unwrap();

            let font = fontdue::Font::from_bytes(font_bytes, fontdue::FontSettings::default())
                .expect("font load");

            Renderer {
                program, vbo, vao, u_screen, u_rect, u_color, u_use_tex,
                font, screen_w: screen_w as f32, screen_h: screen_h as f32,
            }
        }
    }

    pub fn begin(&self, gl: &glow::Context) {
        unsafe {
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.uniform_2_f32(Some(&self.u_screen), self.screen_w, self.screen_h);
        }
    }

    pub fn fill_rect(&self, gl: &glow::Context, x: f32, y: f32, w: f32, h: f32, rgba: [f32; 4]) {
        unsafe {
            gl.uniform_4_f32(Some(&self.u_rect), x, y, w, h);
            gl.uniform_4_f32(Some(&self.u_color), rgba[0], rgba[1], rgba[2], rgba[3]);
            gl.uniform_1_f32(Some(&self.u_use_tex), 0.0);
            gl.draw_arrays(glow::TRIANGLES, 0, 6);
        }
    }

    pub fn draw_texture(&self, gl: &glow::Context, tex: &Texture, x: f32, y: f32, w: f32, h: f32, opacity: f32) {
        unsafe {
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex.id));
            gl.uniform_4_f32(Some(&self.u_rect), x, y, w, h);
            gl.uniform_4_f32(Some(&self.u_color), 1.0, 1.0, 1.0, opacity);
            gl.uniform_1_f32(Some(&self.u_use_tex), 1.0);
            gl.draw_arrays(glow::TRIANGLES, 0, 6);
        }
    }

    /// Rasterize a string to an RGBA texture (white glyphs, alpha = coverage).
    pub fn text_texture(&self, gl: &glow::Context, text: &str, px: f32) -> Texture {
        let mut glyphs = Vec::new();
        let mut pen_x = 0i32;
        let mut max_h = 0i32;
        let mut total_w = 0i32;
        for ch in text.chars() {
            let (m, bmp) = self.font.rasterize(ch, px);
            glyphs.push((pen_x + m.xmin, m.height as i32 - m.ymin, m.width, m.height, bmp, m.ymin));
            pen_x += m.advance_width.ceil() as i32;
            total_w = pen_x;
            max_h = max_h.max((px * 1.3) as i32);
        }
        let (tw, th) = (total_w.max(1) as u32, max_h.max(1) as u32);
        let mut buf = vec![0u8; (tw * th * 4) as usize];
        let baseline = (px * 1.0) as i32;
        for (gx, _gy, gw, gh, bmp, ymin) in glyphs {
            for yy in 0..gh {
                for xx in 0..gw {
                    let cov = bmp[yy * gw + xx];
                    let px_x = gx + xx as i32;
                    let px_y = baseline - ymin - gh as i32 + yy as i32;
                    if px_x < 0 || px_y < 0 || px_x >= tw as i32 || px_y >= th as i32 {
                        continue;
                    }
                    let idx = ((px_y as u32 * tw + px_x as u32) * 4) as usize;
                    buf[idx] = 255;
                    buf[idx + 1] = 255;
                    buf[idx + 2] = 255;
                    buf[idx + 3] = cov;
                }
            }
        }
        Texture::from_rgba(gl, tw, th, &buf)
    }
}

impl Texture {
    pub fn from_rgba(gl: &glow::Context, w: u32, h: u32, rgba: &[u8]) -> Texture {
        unsafe {
            let id = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(id));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::RGBA as i32, w as i32, h as i32, 0,
                glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(rgba)),
            );
            Texture { id, w, h }
        }
    }

    /// Load a PNG file into an RGBA texture; returns None if the file is missing/bad.
    pub fn from_png(gl: &glow::Context, path: &str) -> Option<Texture> {
        let img = image::open(path).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        Some(Texture::from_rgba(gl, w, h, &img))
    }
}

// glow wants &[u8]; reinterpret the f32 vertex slice without adding a crate dep.
fn bytemuck_cast(v: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}
```

> NOTE for `tex_image_2d`: the exact `PixelUnpackData` variant/signature varies by `glow` minor version. If it fails to compile, check the installed `glow` docs and adjust this single call — the rest of the file is version-stable.

- [ ] **Step 3: Declare the module and smoke-test from main**

In `forgedash/src/main.rs` add `mod gfx;`, then replace `fn main` with:

```rust
fn main() {
    let mut plat = platform::Platform::new(1280, 720).expect("platform init");
    let font = include_bytes!("../assets/font.ttf");
    let r = gfx::Renderer::new(&plat.gl, plat.width, plat.height, font);
    let title = r.text_texture(&plat.gl, "ForgeDash", 64.0);
    use glow::HasContext;
    loop {
        if plat.poll() == platform::Nav::Quit {
            break;
        }
        unsafe {
            plat.gl.clear_color(0.04, 0.06, 0.11, 1.0);
            plat.gl.clear(glow::COLOR_BUFFER_BIT);
        }
        r.begin(&plat.gl);
        r.fill_rect(&plat.gl, 440.0, 280.0, 400.0, 160.0, [0.23, 0.51, 0.96, 1.0]);
        r.draw_texture(&plat.gl, &title, 460.0, 80.0, title.w as f32, title.h as f32, 1.0);
        plat.present();
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
```

- [ ] **Step 4: Build and run the desktop dev build**

Run: `cargo run --features bundled --manifest-path forgedash/Cargo.toml`
Expected: dark window with a blue rectangle and white "ForgeDash" text. No shader compile/link panic. Esc closes.

- [ ] **Step 5: Commit**

```bash
git add forgedash/src/gfx.rs forgedash/src/main.rs forgedash/assets/font.ttf
git commit -m "feat(forgedash): GLES2 renderer (quads, textures, text)"
```

---

## Task 7: Wire the coverflow shelf in main

> Brings model + ui + gfx + platform + launch together: load `library.json`, render an animated coverflow in the modern-dark theme, navigate by D-pad/keys, and on Confirm write the handoff and exit. Verified by the desktop dev build.

**Files:**
- Modify: `forgedash/src/main.rs`
- Create: `forgedash/library.json` (dev fixture)
- Create: `forgedash/art/placeholder.png` (any small PNG, optional)

- [ ] **Step 1: Create a dev fixture library**

Create `forgedash/library.json`:

```json
[
  {"title":"Super Mario Bros. 3","year":1988,"publisher":"Nintendo","players":"1-2P","cover_path":"art/placeholder.png","rom_path":"roms/smb3.nes","core":"fceumm"},
  {"title":"The Legend of Zelda","year":1986,"publisher":"Nintendo","players":"1P","cover_path":"art/placeholder.png","rom_path":"roms/zelda.nes","core":"fceumm"},
  {"title":"Mega Man 2","year":1988,"publisher":"Capcom","players":"1P","cover_path":"art/placeholder.png","rom_path":"roms/mm2.nes","core":"fceumm"},
  {"title":"Contra","year":1987,"publisher":"Konami","players":"1-2P","cover_path":"art/placeholder.png","rom_path":"roms/contra.nes","core":"fceumm"}
]
```

(Place any small PNG at `forgedash/art/placeholder.png`; covers fall back to a solid card if the file is missing.)

- [ ] **Step 2: Implement the full main loop**

Replace the entire contents of `forgedash/src/main.rs`:

```rust
mod model;
mod ui;
mod launch;
mod platform;
mod gfx;

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
            // Render an error panel instead of a black screen.
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
            // continuous x using the fractional animated offset
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
```

- [ ] **Step 3: Run the full unit-test suite (regression)**

Run: `cargo test --manifest-path forgedash/Cargo.toml`
Expected: all tests from Tasks 1–4 still pass (11 total).

- [ ] **Step 4: Run the desktop dev build**

Run: `cargo run --features bundled --manifest-path forgedash/Cargo.toml`
Expected: coverflow of 4 cards on a dark background; center card large with a blue glow; Left/Right (or D-pad) slides the selection with easing; title + metadata update; Enter/A prints `launching ...`, writes `/tmp/forge/handoff` (on Windows: a `\tmp\forge\handoff` relative to drive root — fine for dev), and exits.

- [ ] **Step 5: Commit**

```bash
git add forgedash/src/main.rs forgedash/library.json forgedash/art/placeholder.png
git commit -m "feat(forgedash): coverflow shelf wired end-to-end (desktop)"
```

---

## Task 8: Device build — cross-compile + supervisor + staging

> Produces the armv7 binary and the on-device scripts. The cross-compile + dynamic link against the device's libs is the slice's central risk.

**Files:**
- Create: `forgedash/.cargo/config.toml`
- Create: `forgedash/scripts/forge-loop.sh`
- Create: `forgedash/scripts/stage.sh`

- [ ] **Step 1: Add the armv7 target + cross linker**

Install the Rust target:

Run: `rustup target add armv7-unknown-linux-gnueabihf`

Create `forgedash/.cargo/config.toml`:

```toml
[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"

[env]
# Point at a sysroot populated from the device's own /usr/lib (see Step 2),
# so we link the exact libSDL2/libMali the device runs.
SDL2_NO_PKG_CONFIG = "1"
```

> You need an `arm-linux-gnueabihf` cross toolchain (gcc + binutils). On the Windows dev box this is easiest inside WSL or a Linux build container; install via the distro's `gcc-arm-linux-gnueabihf` package.

- [ ] **Step 2: Pull a device sysroot**

The device build links against the device's libraries (no `bundled` feature). Pull them from the device into a local sysroot (device reachable over the RNDIS SSH link, host `169.254.x`, device `169.254.13.37`, SSH `none` auth):

```bash
mkdir -p sysroot/usr/lib sysroot/usr/include
# device libs RetroArch already proves load (libSDL2, libMali/libEGL/libGLESv2, libudev, libasound, ...)
ssh DEV 'cd /usr/lib && tar -cf - libSDL2*.so* libMali.so* libEGL.so* libGLESv2.so* libudev.so* libasound.so* libc.so* libm.so* libdl.so* libpthread.so*' | tar -xf - -C sysroot/usr/lib
```

Add the sysroot to the linker by extending `forgedash/.cargo/config.toml`:

```toml
[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
rustflags = ["-C", "link-arg=--sysroot=sysroot", "-L", "sysroot/usr/lib"]
```

- [ ] **Step 3: Cross-compile the device binary**

Run: `cargo build --release --target armv7-unknown-linux-gnueabihf --manifest-path forgedash/Cargo.toml`
Expected: produces `forgedash/target/armv7-unknown-linux-gnueabihf/release/forgedash` (ARM ELF32). If the SDL2 crate cannot find headers, set `SDL2_INCLUDE` / copy `SDL2/*.h` into `sysroot/usr/include`. This step is the integration crux — resolve link errors here, not on-device.

- [ ] **Step 4: Write the supervisor script**

Create `forgedash/scripts/forge-loop.sh`:

```sh
#!/bin/sh
# ForgeDash supervisor: one GLES app holds the Mali surface at a time.
# Runs the dashboard; if it writes a handoff, launches RetroArch with that ROM,
# then loops back to the dashboard.
set -u
FORGE_ROOT="${FORGE_ROOT:-/tmp/forge}"
RA_ROOT="${RA_ROOT:-/tmp/ra}"
HANDOFF="$FORGE_ROOT/handoff"

cd "$FORGE_ROOT" || exit 1

while true; do
    rm -f "$HANDOFF"
    # Dashboard reads library.json from the current dir.
    ./forgedash "$FORGE_ROOT/library.json" >"$FORGE_ROOT/forge.log" 2>&1

    if [ -f "$HANDOFF" ]; then
        CORE=$(cut -f1 "$HANDOFF")
        ROM=$(cut -f2 "$HANDOFF")
        setsid env HOME="$RA_ROOT/etc/libretro" LD_LIBRARY_PATH=/usr/lib \
            "$RA_ROOT/bin/retroarch" \
            -c "$RA_ROOT/etc/libretro/retroarch.cfg" \
            --appendconfig /tmp/ra-input.cfg \
            -L "$RA_ROOT/etc/libretro/core/${CORE}_libretro.so" \
            "$FORGE_ROOT/$ROM" \
            </dev/null >"$FORGE_ROOT/ra.log" 2>&1
        # RA exits (user picked Quit RetroArch); loop relaunches the dashboard.
    else
        # Dashboard quit without a selection -> exit the loop.
        break
    fi
done
```

- [ ] **Step 5: Write the staging script**

Create `forgedash/scripts/stage.sh`:

```sh
#!/bin/sh
# Host-side: stage ForgeDash + assets into device tmpfs over SSH (tar-over-stdin;
# dropbear has no sftp-server). Run from the forgedash/ directory.
set -eu
DEV="${DEV:-169.254.13.37}"
BIN=target/armv7-unknown-linux-gnueabihf/release/forgedash

tar -cf - --format ustar \
    -C "$(dirname "$BIN")" forgedash \
    -C "$OLDPWD/forgedash" library.json art \
    -C "$OLDPWD/forgedash/scripts" forge-loop.sh \
  | ssh "root@$DEV" 'mkdir -p /tmp/forge && tar -xf - -C /tmp/forge && chmod +x /tmp/forge/forgedash /tmp/forge/forge-loop.sh'

echo "staged to /tmp/forge on $DEV"
echo "on device: bring up GPU+pad (controller-memboot recipe), then: sh /tmp/forge/forge-loop.sh"
```

> Place real `.nes` ROMs + matching cover PNGs under `forgedash/roms/` and `forgedash/art/`, and reference them in `library.json`, before staging for a real run.

- [ ] **Step 6: Commit**

```bash
git add forgedash/.cargo/config.toml forgedash/scripts/forge-loop.sh forgedash/scripts/stage.sh
git commit -m "feat(forgedash): armv7 cross-build + supervisor + staging"
```

---

## Task 9: Hardware-acceptance runbook

**Files:**
- Create: `docs/HARDWARE-ACCEPTANCE-FORGEDASH.md`

- [ ] **Step 1: Write the runbook**

Create `docs/HARDWARE-ACCEPTANCE-FORGEDASH.md`:

```markdown
# Hardware Acceptance — ForgeDash Tracer Bullet

Device: NES Classic (`dp-nes`). All steps RAM-only (brick-safe). Power-cycle = bone stock.

## Preconditions
1. Memboot the GPU+pad bring-up per
   `docs/superpowers/findings/2026-06-03-controller-memboot-spike.md`
   (insmod mali/clovercon/evdev, clover-mcp kick, udevd input-tag). Confirm
   RetroArch RGUI renders + is pad-navigable first.
2. Stage RetroArch into `/tmp/ra` (ClusterM retroarch-clover 1.1d) per
   `docs/superpowers/findings/2026-06-02-retroarch-rgui-spike.md`.
3. From the host `forgedash/` dir: `sh scripts/stage.sh` (stages binary +
   library.json + art + forge-loop.sh + real ROMs into /tmp/forge).

## Acceptance steps + expected results
- [ ] `sh /tmp/forge/forge-loop.sh` → ForgeDash window appears on HDMI.
      Expect: Mali GLES2 surface @1280x720, **no `EGL_BAD_ALLOC`** in
      `/tmp/forge/forge.log`.
- [ ] Coverflow shows >=3 real covers; center cover large with glow; title +
      year/publisher/players render in the modern-dark theme.
- [ ] D-pad Left/Right slides the selection with easing animation (pad seen via
      the udev path).
- [ ] Press A on a game → ForgeDash exits → RetroArch launches that ROM and the
      game runs.
- [ ] In RA: Hotkey+Start → "Quit RetroArch" → ForgeDash relaunches at the same
      shelf.
- [ ] Power-cycle → device boots bone stock (no NAND writes occurred).

## Result
Record PASS/FAIL per step + log excerpts. On PASS, the Rust-GLES + pad + launch
spine is proven; later slices add shelves, sync data contract, theming, and the
NAND-install boot path.
```

- [ ] **Step 2: Commit**

```bash
git add docs/HARDWARE-ACCEPTANCE-FORGEDASH.md
git commit -m "docs(forgedash): hardware-acceptance runbook"
```

---

## Notes for the implementer

- **TDD discipline:** Tasks 1–4 are pure logic — write the test, watch it fail, implement, watch it pass. Tasks 5–7 are GL/platform and are verified visually by the desktop dev build; Tasks 8–9 are device integration verified by the hardware runbook.
- **Desktop first:** Get the whole UX right on the desktop dev build (`--features bundled`) before fighting the cross-compile. The only thing the device adds is the real Mali surface + real pad — both already proven for RetroArch.
- **glow version drift:** the single most likely compile snag is `tex_image_2d`'s `PixelUnpackData` signature; pin `glow` and adjust that one call if needed.
- **Brick safety:** nothing in this slice writes to NAND. Keep it that way — the NAND-install boot path is a separate, backup-gated slice.
```
