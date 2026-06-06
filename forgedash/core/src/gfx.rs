use glow::HasContext;

pub struct Texture {
    pub id: glow::Texture,
    pub w: u32,
    pub h: u32,
}

pub struct Renderer {
    program: glow::Program,
    vbo: glow::Buffer,
    a_pos: u32,
    u_screen: glow::UniformLocation,
    u_rect: glow::UniformLocation,
    u_color: glow::UniformLocation,
    u_use_tex: glow::UniformLocation,
    font: fontdue::Font,
    screen_w: f32,
    screen_h: f32,
}

const VERT: &str = r#"#version 100
attribute vec2 a_pos;
uniform vec2 u_screen;
uniform vec4 u_rect;
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
                0.0, 0.0, 1.0, 0.0, 1.0, 1.0,
                0.0, 0.0, 1.0, 1.0, 0.0, 1.0,
            ];
            let vbo = gl.create_buffer().unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes_of(&verts), glow::STATIC_DRAW);
            let a_pos = gl.get_attrib_location(program, "a_pos").unwrap();

            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

            let u_screen = gl.get_uniform_location(program, "u_screen").unwrap();
            let u_rect = gl.get_uniform_location(program, "u_rect").unwrap();
            let u_color = gl.get_uniform_location(program, "u_color").unwrap();
            let u_use_tex = gl.get_uniform_location(program, "u_use_tex").unwrap();

            let font = fontdue::Font::from_bytes(font_bytes, fontdue::FontSettings::default())
                .expect("font load");

            Renderer {
                program, vbo, a_pos, u_screen, u_rect, u_color, u_use_tex,
                font, screen_w: screen_w as f32, screen_h: screen_h as f32,
            }
        }
    }

    pub fn begin(&self, gl: &glow::Context) {
        unsafe {
            gl.use_program(Some(self.program));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.enable_vertex_attrib_array(self.a_pos);
            gl.vertex_attrib_pointer_f32(self.a_pos, 2, glow::FLOAT, false, 0, 0);
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
            glyphs.push((pen_x + m.xmin, m.width, m.height, bmp, m.ymin));
            pen_x += m.advance_width.ceil() as i32;
            total_w = pen_x;
            max_h = max_h.max((px * 1.3) as i32);
        }
        let (tw, th) = (total_w.max(1) as u32, max_h.max(1) as u32);
        let mut buf = vec![0u8; (tw * th * 4) as usize];
        let baseline = (px * 1.0) as i32;
        for (gx, gw, gh, bmp, ymin) in glyphs {
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
                glow::RGBA, glow::UNSIGNED_BYTE, Some(rgba),
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

// reinterpret an f32 slice as bytes for glow without adding a crate dep.
fn bytes_of(v: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}
