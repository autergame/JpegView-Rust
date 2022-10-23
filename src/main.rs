extern crate image;
extern crate native_dialog;
extern crate threadpool;

extern crate gl;
extern crate glfw;

extern crate imgui;

extern crate fast_generated_dct;

use std::{cell::RefCell, env, path::Path, rc::Rc, thread};

use gl::types::GLuint;
use glfw::{Action, Context, Key};
use native_dialog::FileDialog;

mod imgui_glfw;

mod jpeg;
mod my_image;
mod quad_mind;
mod quad_tree;

fn main() {
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
            format!("JpegView-Rust v{}", env!("CARGO_PKG_VERSION")).as_str(),
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

    style.colors[imgui::StyleColor::FrameBg as usize] = [0.2f32, 0.2f32, 0.2f32, 1.0f32];
    style.colors[imgui::StyleColor::Header as usize] = [0.2f32, 0.2f32, 0.2f32, 1.0f32];
    style.colors[imgui::StyleColor::Button as usize] = [0.2f32, 0.2f32, 0.2f32, 1.0f32];
    style.colors[imgui::StyleColor::FrameBgHovered as usize] = [0.3f32, 0.3f32, 0.3f32, 1.0f32];
    style.colors[imgui::StyleColor::HeaderHovered as usize] = [0.3f32, 0.3f32, 0.3f32, 1.0f32];
    style.colors[imgui::StyleColor::ButtonHovered as usize] = [0.3f32, 0.3f32, 0.3f32, 1.0f32];
    style.colors[imgui::StyleColor::FrameBgActive as usize] = [0.4f32, 0.4f32, 0.4f32, 1.0f32];
    style.colors[imgui::StyleColor::HeaderActive as usize] = [0.4f32, 0.4f32, 0.4f32, 1.0f32];
    style.colors[imgui::StyleColor::ButtonActive as usize] = [0.4f32, 0.4f32, 0.4f32, 1.0f32];
    style.colors[imgui::StyleColor::TextSelectedBg as usize] = [0.4f32, 0.4f32, 0.4f32, 1.0f32];
    style.colors[imgui::StyleColor::MenuBarBg as usize] = [0.2f32, 0.2f32, 0.2f32, 1.0f32];

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

    let working_dir = env::current_dir().expect("Could not get current dir");

    let threads_available = match thread::available_parallelism() {
        Ok(ok) => ok.get() > 1,
        Err(error) => {
            println!("{}", error);
            false
        }
    };
    let mut use_threads = threads_available;

    let mut zoomv = 2.0f32;
    let mut zoomv_max = 100.0f32;
    let mut magnifier_size = 200.0f32;
    let mut magnifier_size_max = 1000.0f32;

    let mut use_zoom = true;
    let mut use_jpeg = true;
    let mut use_quad_tree = false;

    let mut use_draw_line = true;
    let mut use_quad_tree_pow_2 = false;
    let mut min_size = 8usize;
    let mut min_size_index = 2usize;
    let mut max_size = 32usize;
    let mut max_size_index = 4usize;
    let mut max_depth = 50u32;
    let mut max_depth_max = 100u32;
    let mut threshold_error = 5.0f32;
    let mut threshold_error_max = 100.0f32;

    let mut use_ycbcr = true;
    let mut subsampling_index = 0usize;

    let block_size_items: [&str; 9] = ["2", "4", "8", "16", "32", "64", "128", "256", "512"];
    let subsampling_items: [&str; 6] = ["4:4:4", "4:4:0", "4:2:2", "4:2:0", "4:1:1", "4:1:0"];

    let mut image_texture_final: GLuint = 0;
    let mut image_texture_final_zoom: GLuint = 0;
    let mut image_texture_original: GLuint = 0;
    let mut image_texture_original_zoom: GLuint = 0;

    let uv_min: [f32; 2] = [0.0f32, 0.0f32];
    let uv_max: [f32; 2] = [1.0f32, 1.0f32];

    let mut use_vsync = false;
    let mut use_scroll = false;
    let mut close_file = false;

    let mut opt_jpeg: Option<jpeg::Jpeg> = None;
    let mut opt_my_image: Option<my_image::MyImage> = None;
    let mut opt_open_file: Option<String> = None;

    let mut root_quad_list: Vec<Rc<RefCell<quad_tree::QuadNode>>> = vec![];

    if cfg!(debug_assertions) {
        let path = format!("{}/testpattern.png", working_dir.to_str().unwrap());

        let image = image::io::Reader::open(Path::new(&path))
            .expect("Could not open image")
            .decode()
            .expect("Could not decode image");

        let image_width = image.width() as i32;
        let image_height = image.height() as i32;

        let mut my_image = my_image::MyImage::new(
            image.into_rgb8().into_vec(),
            image_width as usize,
            image_height as usize,
        );

        let jpeg = jpeg::Jpeg::new(
            &mut my_image,
            8,
            90,
            1,
            2,
            subsampling_index,
            false,
            use_ycbcr,
            use_threads,
            true,
            false,
        );

        image_texture_original = my_image.create_opengl_image(false, true);
        image_texture_original_zoom = my_image.create_opengl_image(false, false);
        image_texture_final = my_image.create_opengl_image(true, true);
        image_texture_final_zoom = my_image.create_opengl_image(true, false);

        opt_jpeg = Some(jpeg);
        opt_my_image = Some(my_image);
        opt_open_file = Some(path);
    }

    while !window.should_close() {
        let current_time = glfw.get_time() as f32;
        let delta_time_fps = current_time - last_time_fps;

        frames += 1.0f32;
        if delta_time_fps >= 1.0f32 {
            window.set_title(
                format!(
                    "JpegView-Rust v{} - Fps: {:1.0} / Ms: {:1.3}",
                    env!("CARGO_PKG_VERSION"),
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
            .position([0.0f32, 0.0f32], imgui::Condition::Once)
            .always_vertical_scrollbar(true)
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
                        let file_dialog_path = FileDialog::new()
                            .set_location(&working_dir)
                            .add_filter("Image Files", &["jpg", "jpeg", "png", "bmp", "qmi"])
                            .add_filter("JPG JPEG Image", &["jpg", "jpeg"])
                            .add_filter("PNG Image", &["png"])
                            .add_filter("BMP Image", &["bmp"])
                            .add_filter("QUADMIND Image", &["qmi"])
                            .show_open_single_file()
                            .expect("Could not open file dialog");
                        if let Some(path) = file_dialog_path {
                            let image = image::io::Reader::open(&path)
                                .expect("Could not open image")
                                .decode()
                                .expect("Could not decode image");

                            let image_width = image.width() as i32;
                            let image_height = image.height() as i32;

                            let mut my_image = my_image::MyImage::new(
                                image.into_rgb8().into_vec(),
                                image_width as usize,
                                image_height as usize,
                            );

                            let jpeg = match &opt_jpeg {
                                Some(jpeg) => {
                                    unsafe {
                                        gl::DeleteTextures(1, &image_texture_original);
                                        gl::DeleteTextures(1, &image_texture_original_zoom);
                                        gl::DeleteTextures(1, &image_texture_final);
                                        gl::DeleteTextures(1, &image_texture_final_zoom);
                                    }
                                    jpeg::Jpeg::new(
                                        &mut my_image,
                                        jpeg.block_size,
                                        jpeg.quality,
                                        jpeg.quality_start,
                                        jpeg.block_size_index,
                                        subsampling_index,
                                        jpeg.use_gen_qtable,
                                        use_ycbcr,
                                        use_threads,
                                        jpeg.use_fast_dct,
                                        jpeg.use_compression_rate,
                                    )
                                }
                                None => jpeg::Jpeg::new(
                                    &mut my_image,
                                    8,
                                    90,
                                    1,
                                    2,
                                    subsampling_index,
                                    false,
                                    use_ycbcr,
                                    use_threads,
                                    true,
                                    false,
                                ),
                            };

                            image_texture_original = my_image.create_opengl_image(false, true);
                            image_texture_original_zoom =
                                my_image.create_opengl_image(false, false);
                            image_texture_final = my_image.create_opengl_image(true, true);
                            image_texture_final_zoom = my_image.create_opengl_image(true, false);

                            opt_jpeg = Some(jpeg);
                            opt_my_image = Some(my_image);
                            opt_open_file = Some(path.to_str().unwrap().to_string());
                        }
                    }
                    if opt_open_file.is_some() {
                        if let Some(my_image) = &mut opt_my_image {
                            if ui.menu_item("Save image") {
                                let file_dialog_path = FileDialog::new()
                                    .set_location(&working_dir)
                                    .add_filter("PNG Image", &["png"])
                                    .add_filter("QUADMIND Image", &["qmi"])
                                    .show_save_single_file()
                                    .expect("Could not open save file dialog");
                                if let Some(path) = file_dialog_path {
                                    match path.extension() {
                                        Some(ext) => match ext.to_str().unwrap() {
                                            "png" => image::save_buffer(
                                                path,
                                                &my_image.final_image,
                                                my_image.width as u32,
                                                my_image.height as u32,
                                                image::ColorType::Rgb8,
                                            )
                                            .expect("Could not save image"),
                                            "qmi" => {}
                                            _ => {}
                                        },
                                        None => {}
                                    }
                                }
                            }
                        }
                    }
                    ui.same_line();
                    if ui.checkbox("Enable Vsync", &mut use_vsync) {
                        glfw.set_swap_interval(match use_vsync {
                            true => glfw::SwapInterval::Sync(1),
                            false => glfw::SwapInterval::None,
                        });
                    }
                });
                if close_file {
                    close_file = false;
                    opt_jpeg = None;
                    opt_my_image = None;
                    opt_open_file = None;
                }
                if let Some(open_file) = &opt_open_file {
                    if let Some(my_image) = &mut opt_my_image {
                        if let Some(jpeg) = &mut opt_jpeg {
                            ui.align_text_to_frame_padding();
                            ui.text(format!("File: {}", open_file));
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
                            ui.align_text_to_frame_padding();
                            ui.checkbox("Use Jpeg", &mut use_jpeg);
                            ui.indent();
                            ui.align_text_to_frame_padding();
                            ui.bullet_text("Quality Factor:");
                            ui.same_line();
                            ui.set_next_item_width(first_column - ui.cursor_pos()[0]);
                            ui.slider("##quality", 1, 100, &mut jpeg.quality);
                            ui.align_text_to_frame_padding();
                            ui.bullet_text("Block Size:");
                            ui.same_line();
                            ui.set_next_item_width(first_column - ui.cursor_pos()[0]);
                            if ui.combo_simple_string(
                                "##block_size",
                                &mut jpeg.block_size_index,
                                &block_size_items,
                            ) {
                                jpeg.block_size = 1 << (jpeg.block_size_index + 1);
                            }
                            ui.align_text_to_frame_padding();
                            ui.checkbox(
                                "Use Generated Quantization Table",
                                &mut jpeg.use_gen_qtable,
                            );
                            ui.checkbox("Show Compression Rate", &mut jpeg.use_compression_rate);
                            ui.indent();
                            ui.align_text_to_frame_padding();
                            ui.bullet_text("Quality Start:");
                            ui.same_line();
                            ui.set_next_item_width(first_column - ui.cursor_pos()[0]);
                            ui.slider("##quality_start", 1, 100, &mut jpeg.quality_start);
                            ui.unindent();
                            ui.align_text_to_frame_padding();
                            ui.checkbox("Use Fast DCT Algorithm", &mut jpeg.use_fast_dct);
                            ui.disabled(!threads_available, || {
                                ui.align_text_to_frame_padding();
                                ui.checkbox("Use Multi-Threading", &mut use_threads);
                            });
                            ui.unindent();
                            separator();
                            ui.align_text_to_frame_padding();
                            ui.checkbox("Use YCbCr Colors", &mut use_ycbcr);
                            ui.align_text_to_frame_padding();
                            ui.bullet_text("Chroma Subsampling:");
                            ui.same_line();
                            ui.set_next_item_width(first_column - ui.cursor_pos()[0]);
                            ui.combo_simple_string(
                                "##subsampling",
                                &mut subsampling_index,
                                &subsampling_items,
                            );
                            ui.next_column();
                            let second_column = ui.column_width(0) + (ui.column_width(1) * 0.90f32);
                            ui.align_text_to_frame_padding();
                            ui.checkbox("Use QuadTree", &mut use_quad_tree);
                            ui.indent();
                            ui.align_text_to_frame_padding();
                            ui.bullet_text("Max Depth:");
                            ui.same_line();
                            ui.set_next_item_width(second_column - ui.cursor_pos()[0]);
                            ui.slider("##max_depth", 1, max_depth_max, &mut max_depth);
                            if max_depth >= max_depth_max {
                                max_depth_max += 10;
                                max_depth -= 1;
                            }
                            ui.align_text_to_frame_padding();
                            ui.bullet_text("Error Threshold:");
                            ui.same_line();
                            ui.set_next_item_width(second_column - ui.cursor_pos()[0]);
                            ui.slider(
                                "##threshold_error",
                                0.0f32,
                                threshold_error_max,
                                &mut threshold_error,
                            );
                            if threshold_error >= threshold_error_max {
                                threshold_error_max += 10.0f32;
                                threshold_error -= 1.0f32;
                            }
                            ui.align_text_to_frame_padding();
                            ui.bullet_text("Min Quad Size:");
                            ui.same_line();
                            ui.set_next_item_width(second_column - ui.cursor_pos()[0]);
                            if ui.combo_simple_string(
                                "##min_size",
                                &mut min_size_index,
                                &block_size_items,
                            ) {
                                min_size = 1 << (min_size_index + 1);
                            }
                            ui.align_text_to_frame_padding();
                            ui.bullet_text("Max Quad Size:");
                            ui.same_line();
                            ui.set_next_item_width(second_column - ui.cursor_pos()[0]);
                            if ui.combo_simple_string(
                                "##max_size",
                                &mut max_size_index,
                                &block_size_items,
                            ) {
                                max_size = 1 << (max_size_index + 1);
                            }
                            ui.align_text_to_frame_padding();
                            ui.checkbox("Use Quad Size Power Of 2", &mut use_quad_tree_pow_2);
                            ui.align_text_to_frame_padding();
                            ui.checkbox("Draw Quadrant Line", &mut use_draw_line);
                            ui.unindent();
                            separator();
                            ui.align_text_to_frame_padding();
                            ui.checkbox("Use Zoom", &mut use_zoom);
                            ui.indent();
                            ui.align_text_to_frame_padding();
                            ui.bullet_text("Zoom:");
                            ui.same_line();
                            ui.set_next_item_width(second_column - ui.cursor_pos()[0]);
                            ui.slider("##zoomv", 1.0f32, zoomv_max, &mut zoomv);
                            if zoomv >= zoomv_max {
                                zoomv_max += 10.0f32;
                                zoomv -= 1.0f32;
                            }
                            ui.align_text_to_frame_padding();
                            ui.bullet_text("Lupe Size:");
                            ui.same_line();
                            ui.set_next_item_width(second_column - ui.cursor_pos()[0]);
                            ui.slider(
                                "##magnifier_size",
                                10.0f32,
                                magnifier_size_max,
                                &mut magnifier_size,
                            );
                            if magnifier_size >= magnifier_size_max {
                                magnifier_size_max += 100.0f32;
                                magnifier_size -= 10.0f32;
                            }
                            ui.unindent();
                            ui.columns(1, "columns", true);
                            ui.separator();
                            if ui.button_with_size(
                                "Compress",
                                [ui.content_region_avail()[0], 0.0f32],
                            ) {
                                if use_quad_tree && use_jpeg {
                                    root_quad_list = quad_mind::render_quad_mind(
                                        jpeg,
                                        my_image,
                                        min_size,
                                        max_size,
                                        max_depth,
                                        use_draw_line,
                                        threshold_error,
                                        use_ycbcr,
                                        use_threads,
                                        subsampling_index,
                                    );
                                } else if !use_quad_tree && !use_jpeg {
                                    match use_ycbcr {
                                        true => {
                                            my_image.image_to_ycbcr();
                                            my_image.fill_outbound();
                                            my_image.subsampling(true, subsampling_index);
                                            my_image.ycbcr_to_image();
                                        }
                                        false => {
                                            my_image.image_to_rgb();
                                            my_image.fill_outbound();
                                            my_image.subsampling(false, subsampling_index);
                                            my_image.rgb_to_image();
                                        }
                                    }
                                } else if use_quad_tree {
                                    root_quad_list = quad_tree::render_quad_tree(
                                        my_image,
                                        use_ycbcr,
                                        min_size,
                                        max_size,
                                        max_depth,
                                        use_draw_line,
                                        threshold_error,
                                        subsampling_index,
                                        use_quad_tree_pow_2,
                                    );
                                } else if use_jpeg {
                                    jpeg.render_jpeg(
                                        my_image,
                                        use_ycbcr,
                                        use_threads,
                                        subsampling_index,
                                    );
                                }
                                my_image.update_opengl_image(image_texture_final, true);
                                my_image.update_opengl_image(image_texture_final_zoom, true);
                            }
                            let new_width = ui.content_region_avail()[0] / 2.0f32 - item_spacing;
                            let new_height =
                                new_width * (my_image.height as f32 / my_image.width as f32);
                            let mut dont_use_sroll = 0;
                            imgui::Image::new(
                                imgui::TextureId::new(image_texture_original as usize),
                                [new_width, new_height],
                            )
                            .tint_col(TINT_COL)
                            .border_col(BORDER_COL)
                            .uv0(uv_min)
                            .uv1(uv_max)
                            .build(ui);
                            if use_zoom {
                                dont_use_sroll += zoom_layer(
                                    image_texture_original_zoom,
                                    my_image,
                                    ui,
                                    &mut zoomv,
                                    magnifier_size,
                                    window_width,
                                    window_height,
                                );
                            }
                            ui.same_line();
                            imgui::Image::new(
                                imgui::TextureId::new(image_texture_final as usize),
                                [new_width, new_height],
                            )
                            .tint_col(TINT_COL)
                            .border_col(BORDER_COL)
                            .uv0(uv_min)
                            .uv1(uv_max)
                            .build(ui);
                            if use_zoom {
                                dont_use_sroll += zoom_layer(
                                    image_texture_final_zoom,
                                    my_image,
                                    ui,
                                    &mut zoomv,
                                    magnifier_size,
                                    window_width,
                                    window_height,
                                );
                            }
                            if dont_use_sroll > 0 {
                                use_scroll = false;
                            } else {
                                use_scroll = true;
                            }
                        }
                    }
                }
            });

        unsafe {
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        imgui_glfw.draw(&mut imgui_ctx, &mut window);

        window.swap_buffers();
    }

    unsafe {
        gl::DeleteTextures(1, &image_texture_original);
        gl::DeleteTextures(1, &image_texture_original_zoom);
        gl::DeleteTextures(1, &image_texture_final);
        gl::DeleteTextures(1, &image_texture_final_zoom);
    }
}

const TINT_COL: [f32; 4] = [1.0f32, 1.0f32, 1.0f32, 1.0f32];
const BORDER_COL: [f32; 4] = [0.5f32, 0.5f32, 0.5f32, 1.0f32];

fn zoom_layer(
    image_texture: GLuint,
    my_image: &my_image::MyImage,
    ui: &imgui::Ui,
    zoom: &mut f32,
    magnifier_size: f32,
    width: i32,
    height: i32,
) -> i32 {
    if ui.is_item_hovered() {
        if ui.io().mouse_wheel > 0.0f32 {
            *zoom *= 1.1f32;
        } else if ui.io().mouse_wheel < 0.0f32 {
            *zoom *= 1.0f32 / 1.1f32;
        }
        if *zoom < 1.0f32 {
            *zoom = 1.0f32;
        }

        let cursor = ui.io().mouse_pos;

        let last_rect = unsafe { (*imgui::sys::igGetCurrentContext()).LastItemData.Rect };

        let half_magnifier = magnifier_size / 2.0f32;
        let magnifier_zoom = half_magnifier / *zoom;

        let last_rect_size_fixed_x = (last_rect.Max.x - 1.0f32) - (last_rect.Min.x + 1.0f32);
        let last_rect_size_fixed_y = (last_rect.Max.y - 1.0f32) - (last_rect.Min.y + 1.0f32);
        let last_rect_fixed_x = last_rect_size_fixed_x / my_image.width as f32;
        let last_rect_fixed_y = last_rect_size_fixed_y / my_image.height as f32;

        let center_x =
            my_image.width as f32 * ((cursor[0] - last_rect.Min.x) / last_rect_size_fixed_x);
        let center_y =
            my_image.height as f32 * ((cursor[1] - last_rect.Min.y) / last_rect_size_fixed_y);
        let uv0_x = (center_x - (magnifier_zoom / last_rect_fixed_x)) / my_image.width as f32;
        let uv0_y = (center_y - (magnifier_zoom / last_rect_fixed_y)) / my_image.height as f32;
        let uv1_x = (center_x + (magnifier_zoom / last_rect_fixed_x)) / my_image.width as f32;
        let uv1_y = (center_y + (magnifier_zoom / last_rect_fixed_y)) / my_image.height as f32;

        let mut cursor_box_pos_x = cursor[0] - half_magnifier;
        if cursor_box_pos_x < 0.0f32 {
            cursor_box_pos_x = 0.0f32;
        }
        if cursor[0] + half_magnifier > width as f32 {
            cursor_box_pos_x = width as f32 - magnifier_size;
        }
        let mut cursor_box_pos_y = cursor[1] - half_magnifier;
        if cursor_box_pos_y < 0.0f32 {
            cursor_box_pos_y = 0.0f32;
        }
        if cursor[1] + half_magnifier > height as f32 {
            cursor_box_pos_y = height as f32 - magnifier_size;
        }

        set_next_window_pos([cursor_box_pos_x, cursor_box_pos_y]);
        let style = ui.push_style_var(imgui::StyleVar::WindowPadding([0.0f32, 0.0f32]));
        let tooltip = ui.begin_tooltip();
        imgui::Image::new(
            imgui::TextureId::new(image_texture as usize),
            [magnifier_size, magnifier_size],
        )
        .tint_col(TINT_COL)
        .border_col(BORDER_COL)
        .uv0([uv0_x, uv0_y])
        .uv1([uv1_x, uv1_y])
        .build(ui);
        tooltip.end();
        style.pop();

        1
    } else {
        0
    }
}

fn set_next_window_pos(pos: [f32; 2]) {
    unsafe {
        imgui::sys::igSetNextWindowPos(
            pos.into(),
            imgui::Condition::Always as i32,
            [0.0f32, 0.0f32].into(),
        )
    };
}

fn separator() {
    unsafe {
        imgui::sys::igSeparatorEx(
            imgui::sys::ImGuiSeparatorFlags_Horizontal as imgui::sys::ImGuiSeparatorFlags,
        )
    }
}
