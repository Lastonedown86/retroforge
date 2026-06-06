use std::fs::File;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;

/// Abstract navigation events, mapped from keyboard (dev) and the raw evdev pad (device).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Nav {
    None,
    Left,
    Right,
    Up,
    Down,
    Confirm,
    Back,
    Delete,
    Quit,
}

// Linux evdev: EV_KEY type + Clovercon button codes (learned on-device via evtest).
const EV_KEY: u16 = 1;
const CODE_A: u16 = 304; // BTN_SOUTH   -> Confirm
const CODE_B: u16 = 305; // BTN_EAST    -> Back
const CODE_SELECT: u16 = 314; // BTN_SELECT -> Delete
const CODE_LEFT: u16 = 704; // d-pad left
const CODE_RIGHT: u16 = 705; // d-pad right
const CODE_UP: u16 = 706; // d-pad up
const CODE_DOWN: u16 = 707; // d-pad down
const O_NONBLOCK: i32 = 0o4000;

pub struct Platform {
    _sdl: sdl2::Sdl,
    window: sdl2::video::Window,
    _gl_ctx: sdl2::video::GLContext,
    pub gl: glow::Context,
    event_pump: sdl2::EventPump,
    /// Raw evdev fd for the pad; SDL's joystick layer doesn't enumerate the Clovercon.
    pad: Option<File>,
    pub width: u32,
    pub height: u32,
    pub controller_present: bool,
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

        let event_pump = sdl.event_pump()?;

        // Open the pad as a raw non-blocking evdev device (default event24; override via FORGE_PAD).
        let pad_path =
            std::env::var("FORGE_PAD").unwrap_or_else(|_| "/dev/input/event24".to_string());
        let pad = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(O_NONBLOCK)
            .open(&pad_path)
            .ok();
        if pad.is_none() {
            eprintln!("forgedash: no controller at {pad_path} (keyboard nav only)");
        }
        let controller_present = pad.is_some();

        Ok(Platform {
            _sdl: sdl,
            window,
            _gl_ctx: gl_ctx,
            gl,
            event_pump,
            pad,
            width,
            height,
            controller_present,
        })
    }

    /// Drain SDL events (keyboard/window) + raw evdev pad events; return the first nav this frame.
    pub fn poll(&mut self) -> Nav {
        use sdl2::event::Event;
        use sdl2::keyboard::Keycode;

        let mut result = Nav::None;
        for event in self.event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => return Nav::Quit,
                Event::KeyDown { keycode: Some(k), .. } => match k {
                    Keycode::Left => result = Nav::Left,
                    Keycode::Right => result = Nav::Right,
                    Keycode::Up => result = Nav::Up,
                    Keycode::Down => result = Nav::Down,
                    Keycode::Return | Keycode::Space => result = Nav::Confirm,
                    Keycode::Backspace => result = Nav::Back,
                    Keycode::Delete => result = Nav::Delete,
                    Keycode::Escape => return Nav::Quit,
                    _ => {}
                },
                _ => {}
            }
        }

        // Raw evdev: input_event is 16 bytes on 32-bit ARM (timeval=8, type=2, code=2, value=4).
        if let Some(pad) = self.pad.as_mut() {
            let mut buf = [0u8; 16];
            while let Ok(16) = pad.read(&mut buf) {
                let etype = u16::from_ne_bytes([buf[8], buf[9]]);
                let code = u16::from_ne_bytes([buf[10], buf[11]]);
                let value = i32::from_ne_bytes([buf[12], buf[13], buf[14], buf[15]]);
                if etype == EV_KEY && value == 1 {
                    match code {
                        CODE_LEFT => result = Nav::Left,
                        CODE_RIGHT => result = Nav::Right,
                        CODE_UP => result = Nav::Up,
                        CODE_DOWN => result = Nav::Down,
                        CODE_A => result = Nav::Confirm,
                        CODE_B => result = Nav::Back,
                        CODE_SELECT => result = Nav::Delete,
                        _ => {}
                    }
                }
            }
        }

        result
    }

    pub fn present(&self) {
        self.window.gl_swap_window();
    }
}
