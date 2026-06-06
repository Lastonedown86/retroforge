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
