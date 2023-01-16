extern crate bincode;
extern crate image;
extern crate miniz_oxide;
extern crate native_dialog;
extern crate serde;
extern crate sha2;
extern crate threadpool;

extern crate gl;
extern crate glfw;

extern crate imgui;

extern crate fast_generated_dct;

use std::{
    env,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

use gl::types::GLuint;
use glfw::{Action, Context, Key};
use native_dialog::FileDialog;

mod imgui_glfw;

mod imgui_layout;
mod jpeg;
mod my_image;
mod quad_mind;
mod quad_tree;

use jpeg::Jpeg;
use my_image::MyImage;
use quad_tree::{QuadNodeRef, QuadTree};

fn main() {
    let cargo_pkg_version = env!("CARGO_PKG_VERSION");
    let working_dir = env::current_dir().expect("Could not get current dir");

    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS).expect("Could not init GLFW");

    glfw.window_hint(glfw::WindowHint::Samples(None));
    glfw.window_hint(glfw::WindowHint::DoubleBuffer(true));
    glfw.window_hint(glfw::WindowHint::ContextVersion(3, 3));
    glfw.window_hint(glfw::WindowHint::OpenGlProfile(
        glfw::OpenGlProfileHint::Core,
    ));
    #[cfg(target_os = "macos")]
    glfw.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));

    let (mut window_width, mut window_height) = (1024i32, 576i32);

    let (mut window, events) = glfw
        .create_window(
            window_width as u32,
            window_height as u32,
            format!("JpegView-Rust v{}", cargo_pkg_version).as_str(),
            glfw::WindowMode::Windowed,
        )
        .expect("Could not create GLFW window");

    glfw.with_primary_monitor(|_, monitor| {
        let (xpos, ypos, monitor_width, monitor_height) =
            monitor.expect("Could not get GLFW monitor").get_workarea();
        window.set_pos(
            (monitor_width - xpos) / 2 - window_width / 2,
            (monitor_height - ypos) / 2 - window_height / 2,
        );
    });

    window.make_current();

    window.set_key_polling(true);
    window.set_char_polling(true);
    window.set_scroll_polling(true);
    window.set_cursor_pos_polling(true);
    window.set_mouse_button_polling(true);
    window.set_framebuffer_size_polling(true);

    glfw.set_swap_interval(glfw::SwapInterval::None);

    gl::load_with(|symbol| window.get_proc_address(symbol));

    unsafe {
        gl::ClearColor(0.5f32, 0.5f32, 0.5f32, 1.0f32);
    }

    let mut imgui_ctx = imgui::Context::create();

    imgui_ctx.set_ini_filename(None);

    let style = imgui_ctx.style_mut();
    style.use_dark_colors();

    let item_spacing = style.item_spacing[0];

    style[imgui::StyleColor::FrameBg] = [0.2f32, 0.2f32, 0.2f32, 1.0f32];
    style[imgui::StyleColor::Header] = [0.2f32, 0.2f32, 0.2f32, 1.0f32];
    style[imgui::StyleColor::Button] = [0.2f32, 0.2f32, 0.2f32, 1.0f32];

    style[imgui::StyleColor::FrameBgHovered] = [0.3f32, 0.3f32, 0.3f32, 1.0f32];
    style[imgui::StyleColor::HeaderHovered] = [0.3f32, 0.3f32, 0.3f32, 1.0f32];
    style[imgui::StyleColor::ButtonHovered] = [0.3f32, 0.3f32, 0.3f32, 1.0f32];

    style[imgui::StyleColor::FrameBgActive] = [0.4f32, 0.4f32, 0.4f32, 1.0f32];
    style[imgui::StyleColor::HeaderActive] = [0.4f32, 0.4f32, 0.4f32, 1.0f32];
    style[imgui::StyleColor::ButtonActive] = [0.4f32, 0.4f32, 0.4f32, 1.0f32];

    style[imgui::StyleColor::TextSelectedBg] = [0.4f32, 0.4f32, 0.4f32, 1.0f32];
    style[imgui::StyleColor::MenuBarBg] = [0.2f32, 0.2f32, 0.2f32, 1.0f32];

    style.grab_rounding = 6.0f32;
    style.frame_rounding = 8.0f32;
    style.window_rounding = 0.0f32;
    style.frame_border_size = 1.0f32;
    style.window_border_size = 2.0f32;
    style.item_spacing = [item_spacing, item_spacing];

    imgui_ctx.fonts().add_font(&[imgui::FontSource::TtfData {
        data: include_bytes!("../assets/fonts/consola.ttf"),
        size_pixels: 13.0f32,
        config: None,
    }]);

    let mut imgui_glfw = imgui_glfw::ImguiGLFW::new(&mut imgui_ctx);

    let mut frames = 0.0f32;
    let mut last_time = 0.0f32;
    let mut last_time_fps = 0.0f32;

    let threads_available = match thread::available_parallelism() {
        Ok(count) => count.get() > 1,
        Err(_) => false,
    };
    let mut use_threads = threads_available;

    let mut zoom = 2.0f32;
    let mut zoom_max = 100.0f32;
    let mut magnifier_size = 200.0f32;
    let mut magnifier_size_max = 1000.0f32;

    let mut use_jpeg = true;
    let mut use_quad_tree = false;

    let mut min_size_index = 1;
    let mut max_size_index = 5;

    let mut max_depth_max = 100;
    let mut threshold_error_max = 100.0f32;

    let mut jpeg = Jpeg::new(8, 90.0f32, 1.0f32, 2, false, use_threads, true, false);
    let mut quad_tree = QuadTree::new(50, 4, 64, false, true, 10.0f32);

    let mut use_ycbcr = true;
    let mut subsampling_index = 0;

    let mut image_textures = OpenglImages::new();

    let mut use_zoom = true;
    let mut use_vsync = false;
    let mut use_scroll = false;

    let mut close_file = false;

    let mut opt_my_image: Option<MyImage> = None;

    let mut quad_mind_list: Vec<QuadNodeRef> = Vec::new();
    let mut quad_mind_dct_zig_zag: Vec3d<i32> = Vec::new();

    if cfg!(debug_assertions) {
        let path = format!("{}/assets/test_pattern.png", working_dir.to_str().unwrap());

        let image = image::io::Reader::open(Path::new(&path))
            .expect("Could not open image")
            .decode()
            .expect("Could not decode image");

        let image_width = image.width() as usize;
        let image_height = image.height() as usize;

        let mut my_image = MyImage::new(
            image.into_rgb8().into_vec(),
            image_width,
            image_height,
            path,
        );

        my_image.apply_transform(use_ycbcr, subsampling_index);

        image_textures.my_image_to_opengl(&my_image);

        opt_my_image = Some(my_image);
    }

    while !window.should_close() {
        let current_time = glfw.get_time() as f32;
        let delta_time_fps = current_time - last_time_fps;

        frames += 1.0f32;
        if delta_time_fps >= 1.0f32 {
            window.set_title(
                format!(
                    "JpegView-Rust v{} - Fps: {:1.0} / Ms: {:1.3}",
                    cargo_pkg_version,
                    frames / delta_time_fps,
                    1000.0f32 / frames
                )
                .as_str(),
            );
            frames = 0.0f32;
            last_time_fps = current_time;
        }

        let delta_time = current_time - last_time;
        last_time = current_time;

        glfw.poll_events();

        for (_, event) in glfw::flush_messages(&events) {
            imgui_glfw.handle_event(&mut imgui_ctx, &event);
            match event {
                glfw::WindowEvent::FramebufferSize(frame_width, frame_height) => unsafe {
                    gl::Viewport(0, 0, frame_width, frame_height);
                    window_width = frame_width;
                    window_height = frame_height;
                },
                glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
                    window.set_should_close(true)
                }
                glfw::WindowEvent::Close => window.set_should_close(true),
                _ => {}
            }
        }

        imgui_glfw.update_imgui(delta_time, &window, &mut imgui_ctx);

        let ui = imgui_ctx.new_frame();

        ui.window("Main")
            .size(
                [window_width as f32, window_height as f32],
                imgui::Condition::Always,
            )
            .position([0.0f32, 0.0f32], imgui::Condition::Always)
            .always_vertical_scrollbar(opt_my_image.is_some())
            .always_auto_resize(true)
            .scrollable(use_scroll)
            .collapsible(false)
            .title_bar(false)
            .resizable(false)
            .movable(false)
            .menu_bar(true)
            .build(|| {
                ui.menu_bar(|| {
                    if ui.menu_item("Open image") {
                        if let Some((my_image, jpeg_)) =
                            open_image(&working_dir, use_ycbcr, subsampling_index)
                        {
                            if let Some(jpeg_) = jpeg_ {
                                jpeg = jpeg_;
                            }

                            if opt_my_image.is_some() {
                                image_textures.destroy();
                            }

                            image_textures.my_image_to_opengl(&my_image);

                            opt_my_image = Some(my_image);
                        }
                    }
                    if opt_my_image.is_some() {
                        if let Some(my_image) = &opt_my_image {
                            if ui.menu_item("Save image") {
                                save_image(
                                    &working_dir,
                                    use_jpeg,
                                    use_ycbcr,
                                    use_threads,
                                    use_quad_tree,
                                    &jpeg,
                                    my_image,
                                    &quad_mind_list,
                                    &quad_mind_dct_zig_zag,
                                )
                            }
                        }
                    }
                    ui.same_line();
                    if ui.checkbox("Enable Vsync", &mut use_vsync) {
                        glfw.set_swap_interval(if use_vsync {
                            glfw::SwapInterval::Sync(1)
                        } else {
                            glfw::SwapInterval::None
                        });
                    }
                });
                if let Some(my_image) = &mut opt_my_image {
                    ui.align_text_to_frame_padding();
                    ui.text(format!("File: {}", my_image.file_path));
                    ui.same_line();
                    ui.text(format!(
                        "Image Size: {} {}",
                        my_image.width, my_image.height
                    ));
                    ui.same_line();
                    if ui.button("Close Image") {
                        close_file = true;
                    }

                    ui.separator();

                    ui.columns(2, "columns", true);
                    let first_column = ui.column_width(0) * 0.90f32;

                    imgui_layout::jpeg(
                        ui,
                        first_column,
                        &mut jpeg,
                        &mut use_jpeg,
                        &mut use_threads,
                        threads_available,
                    );

                    imgui_layout::separator();

                    imgui_layout::ycbcr(ui, first_column, &mut use_ycbcr, &mut subsampling_index);

                    ui.next_column();
                    let second_column = ui.column_width(0) + (ui.column_width(1) * 0.90f32);

                    imgui_layout::quad_tree(
                        ui,
                        second_column,
                        &mut quad_tree,
                        &mut use_quad_tree,
                        &mut min_size_index,
                        &mut max_size_index,
                        &mut max_depth_max,
                        &mut threshold_error_max,
                    );

                    imgui_layout::separator();

                    imgui_layout::zoom(
                        ui,
                        second_column,
                        &mut use_zoom,
                        &mut zoom,
                        &mut zoom_max,
                        &mut magnifier_size,
                        &mut magnifier_size_max,
                    );

                    ui.columns(1, "columns", true);

                    ui.separator();

                    if ui.button_with_size("Compress", [ui.content_region_avail()[0], 0.0f32]) {
                        if use_quad_tree && use_jpeg {
                            (quad_mind_list, quad_mind_dct_zig_zag) = quad_mind::render_quad_mind(
                                &mut jpeg,
                                my_image,
                                &quad_tree,
                                use_ycbcr,
                                use_threads,
                                subsampling_index,
                            );
                        } else if !use_quad_tree && !use_jpeg {
                            my_image.apply_transform(use_ycbcr, subsampling_index);
                        } else if use_quad_tree {
                            quad_tree.render(my_image, use_ycbcr, subsampling_index);
                        } else if use_jpeg {
                            jpeg.render(my_image, use_ycbcr, use_threads, subsampling_index);
                        }
                        my_image.update_opengl_image(image_textures.final_result, true);
                        my_image.update_opengl_image(image_textures.final_result_zoom, true);
                    }

                    use_scroll = true;

                    let new_width = ui.content_region_avail()[0] / 2.0f32 - item_spacing;
                    let new_height = new_width * (my_image.height as f32 / my_image.width as f32);

                    imgui_layout::image(
                        ui,
                        image_textures.original,
                        [new_width, new_height],
                        imgui_layout::UV_MIN,
                        imgui_layout::UV_MAX,
                    );

                    if use_zoom && ui.is_item_hovered() {
                        use_scroll = false;
                        imgui_layout::zoom_layer(
                            ui,
                            image_textures.original_zoom,
                            my_image,
                            &mut zoom,
                            magnifier_size,
                            window_width,
                            window_height,
                        );
                    }

                    ui.same_line();

                    imgui_layout::image(
                        ui,
                        image_textures.final_result,
                        [new_width, new_height],
                        imgui_layout::UV_MIN,
                        imgui_layout::UV_MAX,
                    );

                    if use_zoom && ui.is_item_hovered() {
                        use_scroll = false;
                        imgui_layout::zoom_layer(
                            ui,
                            image_textures.final_result_zoom,
                            my_image,
                            &mut zoom,
                            magnifier_size,
                            window_width,
                            window_height,
                        );
                    }

                    if close_file {
                        close_file = false;

                        opt_my_image = None;
                        image_textures.destroy();
                    }
                }
            });

        unsafe {
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        imgui_glfw.draw(&mut imgui_ctx, &mut window);

        window.swap_buffers();
    }

    image_textures.destroy();
}

fn open_image(
    working_dir: &PathBuf,
    use_ycbcr: bool,
    subsampling_index: usize,
) -> Option<(MyImage, Option<Jpeg>)> {
    let file_dialog_path = FileDialog::new()
        .set_location(working_dir)
        .add_filter("Image Files", &["jpg", "jpeg", "png", "bmp", "qmi"])
        .add_filter("JPG JPEG Image", &["jpg", "jpeg"])
        .add_filter("PNG Image", &["png"])
        .add_filter("BMP Image", &["bmp"])
        .add_filter("QUADMIND Image", &["qmi"])
        .show_open_single_file()
        .expect("Could not open file dialog");

    if let Some(path) = file_dialog_path {
        let ext = path
            .extension()
            .expect("Could not get open file dialog path ext")
            .to_str()
            .unwrap();

        let (my_image, jpeg) = match ext {
            "jpg" | "jpeg" | "png" | "bmp" => {
                let image = image::io::Reader::open(&path)
                    .expect("Could not open image")
                    .decode()
                    .expect("Could not decode image");

                let image_width = image.width() as usize;
                let image_height = image.height() as usize;

                let mut my_image = MyImage::new(
                    image.into_rgb8().into_vec(),
                    image_width,
                    image_height,
                    path.to_str().unwrap().to_string(),
                );

                my_image.apply_transform(use_ycbcr, subsampling_index);

                (my_image, None)
            }
            "qmi" => {
                let quad_mind =
                    quad_mind::load_quad_mind(&path).expect("Could not load quad mind image");
                (quad_mind.0, Some(quad_mind.1))
            }
            _ => {
                return None;
            }
        };

        Some((my_image, jpeg))
    } else {
        None
    }
}

fn save_image(
    working_dir: &PathBuf,
    use_jpeg: bool,
    use_ycbcr: bool,
    use_threads: bool,
    use_quad_tree: bool,
    jpeg: &Jpeg,
    my_image: &MyImage,
    quad_node_list: &[QuadNodeRef],
    quad_dct_zig_zag: &Vec3d<i32>,
) {
    let file_dialog_path = FileDialog::new()
        .set_location(working_dir)
        .add_filter("PNG Image", &["png"])
        .add_filter("QUADMIND Image", &["qmi"])
        .show_save_single_file()
        .expect("Could not open save file dialog");

    if let Some(path) = file_dialog_path {
        let ext = path
            .extension()
            .expect("Could not get save file dialog path ext")
            .to_str()
            .unwrap();

        match ext {
            "png" => {
                if use_jpeg {
                    image::save_buffer(
                        path,
                        &my_image.final_image,
                        my_image.width as u32,
                        my_image.height as u32,
                        image::ColorType::Rgb8,
                    )
                    .expect("Could not save image")
                }
            }
            "qmi" => {
                if use_quad_tree && use_jpeg {
                    quad_mind::save_quad_mind(
                        &path,
                        quad_node_list,
                        quad_dct_zig_zag,
                        my_image,
                        jpeg,
                        use_ycbcr,
                        use_threads,
                    )
                }
            }
            _ => {}
        }
    }
}

struct OpenglImages {
    original: GLuint,
    original_zoom: GLuint,
    final_result: GLuint,
    final_result_zoom: GLuint,
}

impl OpenglImages {
    fn new() -> OpenglImages {
        OpenglImages {
            original: 0,
            original_zoom: 0,
            final_result: 0,
            final_result_zoom: 0,
        }
    }
    fn my_image_to_opengl(&mut self, my_image: &MyImage) {
        self.original = my_image.create_opengl_image(false, true);
        self.original_zoom = my_image.create_opengl_image(false, false);
        self.final_result = my_image.create_opengl_image(true, true);
        self.final_result_zoom = my_image.create_opengl_image(true, false);
    }
    fn destroy(&self) {
        unsafe {
            gl::DeleteTextures(1, &self.original);
            gl::DeleteTextures(1, &self.original_zoom);
            gl::DeleteTextures(1, &self.final_result);
            gl::DeleteTextures(1, &self.final_result_zoom);
        }
    }
}

pub type Vec2d<T> = Vec<Vec<T>>;
pub type Vec3d<T> = Vec<Vec<Vec<T>>>;

pub fn unwrap_arc_mutex<T>(value: Arc<Mutex<T>>) -> T
where
    T: std::fmt::Debug,
{
    Arc::try_unwrap(value).unwrap().into_inner().unwrap()
}
