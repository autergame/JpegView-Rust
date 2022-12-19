use crate::{jpeg::Jpeg, my_image::MyImage, quad_tree::QuadTree};

pub fn jpeg(
    ui: &imgui::Ui,
    column: f32,
    jpeg: &mut Jpeg,
    use_jpeg: &mut bool,
    use_threads: &mut bool,
    threads_available: bool,
) {
    ui.align_text_to_frame_padding();
    ui.checkbox("Use Jpeg", use_jpeg);

    indent_block(ui, || {
        ui.align_text_to_frame_padding();
        ui.bullet_text("Quality Factor:");
        ui.same_line();
        ui.set_next_item_width(column - ui.cursor_pos()[0]);
        ui.slider("##quality", 1.0f32, 100.0f32, &mut jpeg.quality);

        ui.align_text_to_frame_padding();
        ui.bullet_text("Block Size:");
        ui.same_line();
        ui.set_next_item_width(column - ui.cursor_pos()[0]);
        if ui.combo_simple_string(
            "##block_size",
            &mut jpeg.block_size_index,
            &BLOCK_SIZE_ITEMS,
        ) {
            jpeg.block_size = 1 << (jpeg.block_size_index + 1);
        }

        ui.align_text_to_frame_padding();
        ui.checkbox("Use Generated Quantization Table", &mut jpeg.use_gen_qtable);

        ui.align_text_to_frame_padding();
        ui.checkbox("Show Compression Rate", &mut jpeg.use_compression_rate);

        indent_block(ui, || {
            ui.align_text_to_frame_padding();
            ui.bullet_text("Quality Start:");
            ui.same_line();
            ui.set_next_item_width(column - ui.cursor_pos()[0]);
            ui.slider("##quality_start", 1.0f32, 100.0f32, &mut jpeg.quality_start);
        });

        ui.align_text_to_frame_padding();
        ui.checkbox("Use Fast DCT Algorithm", &mut jpeg.use_fast_dct);

        ui.disabled(!threads_available, || {
            ui.align_text_to_frame_padding();
            ui.checkbox("Use Multi-Threading", use_threads);
        });
    });
}

pub fn ycbcr(ui: &imgui::Ui, column: f32, use_ycbcr: &mut bool, subsampling_index: &mut usize) {
    ui.align_text_to_frame_padding();
    ui.checkbox("Use YCbCr Colors", use_ycbcr);

    ui.align_text_to_frame_padding();
    ui.bullet_text("Chroma Subsampling:");
    ui.same_line();
    ui.set_next_item_width(column - ui.cursor_pos()[0]);
    ui.combo_simple_string("##subsampling", subsampling_index, &SUBSAMPLING_ITEMS);
}

pub fn quad_tree(
    ui: &imgui::Ui,
    column: f32,
    quad_tree: &mut QuadTree,
    use_quad_tree: &mut bool,
    min_size_index: &mut usize,
    max_size_index: &mut usize,
    max_depth_max: &mut u32,
    threshold_error_max: &mut f32,
) {
    ui.align_text_to_frame_padding();
    ui.checkbox("Use QuadTree", use_quad_tree);

    indent_block(ui, || {
        ui.align_text_to_frame_padding();
        ui.bullet_text("Max Depth:");
        ui.same_line();
        ui.set_next_item_width(column - ui.cursor_pos()[0]);
        ui.slider("##max_depth", 1, *max_depth_max, &mut quad_tree.max_depth);

        ui.align_text_to_frame_padding();
        ui.bullet_text("Error Threshold:");
        ui.same_line();
        ui.set_next_item_width(column - ui.cursor_pos()[0]);
        ui.slider(
            "##threshold_error",
            0.0f32,
            *threshold_error_max,
            &mut quad_tree.threshold_error,
        );

        ui.align_text_to_frame_padding();
        ui.bullet_text("Min Quad Size:");
        ui.same_line();
        ui.set_next_item_width(column - ui.cursor_pos()[0]);
        if ui.combo_simple_string("##min_size", min_size_index, &BLOCK_SIZE_ITEMS) {
            quad_tree.min_size = 1 << (*min_size_index + 1);
        }

        ui.align_text_to_frame_padding();
        ui.bullet_text("Max Quad Size:");
        ui.same_line();
        ui.set_next_item_width(column - ui.cursor_pos()[0]);
        if ui.combo_simple_string("##max_size", max_size_index, &BLOCK_SIZE_ITEMS) {
            quad_tree.max_size = 1 << (*max_size_index + 1);
        }

        ui.align_text_to_frame_padding();
        ui.checkbox("Use Quad Size Power Of 2", &mut quad_tree.use_pow_2);

        ui.align_text_to_frame_padding();
        ui.checkbox("Draw Quadrant Line", &mut quad_tree.use_draw_line);
    });

    increase_max(&mut quad_tree.max_depth, max_depth_max, 2, 1);
    increase_max(
        &mut quad_tree.threshold_error,
        threshold_error_max,
        2.0f32,
        1.0f32,
    );
}

pub fn zoom(
    ui: &imgui::Ui,
    column: f32,
    use_zoom: &mut bool,
    zoom: &mut f32,
    zoom_max: &mut f32,
    magnifier_size: &mut f32,
    magnifier_size_max: &mut f32,
) {
    ui.align_text_to_frame_padding();
    ui.checkbox("Use Zoom", use_zoom);

    indent_block(ui, || {
        ui.align_text_to_frame_padding();
        ui.bullet_text("Zoom:");
        ui.same_line();
        ui.set_next_item_width(column - ui.cursor_pos()[0]);
        ui.slider("##zoomv", 1.0f32, *zoom_max, zoom);

        ui.align_text_to_frame_padding();
        ui.bullet_text("Lupe Size:");
        ui.same_line();
        ui.set_next_item_width(column - ui.cursor_pos()[0]);
        ui.slider(
            "##magnifier_size",
            10.0f32,
            *magnifier_size_max,
            magnifier_size,
        );
    });

    increase_max(zoom, zoom_max, 2.0f32, 1.0f32);
    increase_max(magnifier_size, magnifier_size_max, 2.0f32, 1.0f32);
}

pub fn zoom_layer(
    ui: &imgui::Ui,
    image_texture: gl::types::GLuint,
    my_image: &MyImage,
    zoom: &mut f32,
    magnifier_size: f32,
    width: i32,
    height: i32,
) {
    let ui_io = ui.io();

    if ui_io.mouse_wheel > 0.0f32 {
        *zoom *= 1.1f32;
    } else if ui_io.mouse_wheel < 0.0f32 {
        *zoom *= 0.9f32;
    }

    if *zoom < 1.0f32 {
        *zoom = 1.0f32;
    }

    let last_rect = unsafe { (*imgui::sys::igGetCurrentContext()).LastItemData.Rect };

    let half_magnifier = magnifier_size / 2.0f32;
    let magnifier_zoom = half_magnifier / *zoom;

    let last_rect_size_fixed_x = (last_rect.Max.x - 1.0f32) - (last_rect.Min.x + 1.0f32);
    let last_rect_size_fixed_y = (last_rect.Max.y - 1.0f32) - (last_rect.Min.y + 1.0f32);

    let last_rect_fixed_x = last_rect_size_fixed_x / my_image.width as f32;
    let last_rect_fixed_y = last_rect_size_fixed_y / my_image.height as f32;

    let center_x =
        my_image.width as f32 * ((ui_io.mouse_pos[0] - last_rect.Min.x) / last_rect_size_fixed_x);
    let center_y =
        my_image.height as f32 * ((ui_io.mouse_pos[1] - last_rect.Min.y) / last_rect_size_fixed_y);

    let uv0_x = (center_x - (magnifier_zoom / last_rect_fixed_x)) / my_image.width as f32;
    let uv0_y = (center_y - (magnifier_zoom / last_rect_fixed_y)) / my_image.height as f32;
    let uv1_x = (center_x + (magnifier_zoom / last_rect_fixed_x)) / my_image.width as f32;
    let uv1_y = (center_y + (magnifier_zoom / last_rect_fixed_y)) / my_image.height as f32;

    let mut cursor_box_pos = [
        ui_io.mouse_pos[0] - half_magnifier,
        ui_io.mouse_pos[1] - half_magnifier,
    ];

    if cursor_box_pos[0] < 0.0f32 {
        cursor_box_pos[0] = 0.0f32;
    } else if (ui_io.mouse_pos[0] + half_magnifier) > width as f32 {
        cursor_box_pos[0] = width as f32 - magnifier_size;
    }

    if cursor_box_pos[1] < 0.0f32 {
        cursor_box_pos[1] = 0.0f32;
    } else if (ui_io.mouse_pos[1] + half_magnifier) > height as f32 {
        cursor_box_pos[1] = height as f32 - magnifier_size;
    }

    set_next_window_pos(cursor_box_pos);

    let style = ui.push_style_var(imgui::StyleVar::WindowPadding([0.0f32, 0.0f32]));
    let tooltip = ui.begin_tooltip();

    image(
        ui,
        image_texture,
        [magnifier_size, magnifier_size],
        [uv0_x, uv0_y],
        [uv1_x, uv1_y],
    );

    tooltip.end();
    style.pop();
}

pub fn image(ui: &imgui::Ui, image_texture: u32, size: [f32; 2], uv0: [f32; 2], uv1: [f32; 2]) {
    imgui::Image::new(imgui::TextureId::new(image_texture as usize), size)
        .tint_col(TINT_COL)
        .border_col(BORDER_COL)
        .uv0(uv0)
        .uv1(uv1)
        .build(ui);
}

pub fn set_next_window_pos(pos: [f32; 2]) {
    unsafe {
        imgui::sys::igSetNextWindowPos(
            pos.into(),
            imgui::Condition::Always as i32,
            [0.0f32, 0.0f32].into(),
        )
    };
}

pub fn separator() {
    unsafe {
        imgui::sys::igSeparatorEx(
            imgui::sys::ImGuiSeparatorFlags_Horizontal as imgui::sys::ImGuiSeparatorFlags,
        )
    }
}

fn indent_block<F>(ui: &imgui::Ui, f: F)
where
    F: FnOnce(),
{
    ui.indent();
    f();
    ui.unindent();
}

fn increase_max<T>(value: &mut T, max: &mut T, inc: T, dec: T)
where
    T: PartialOrd + std::ops::AddAssign + std::ops::SubAssign,
{
    if *value >= *max {
        *max += inc;
        *value -= dec;
    }
}

pub const UV_MIN: [f32; 2] = [0.0f32, 0.0f32];
pub const UV_MAX: [f32; 2] = [1.0f32, 1.0f32];

const TINT_COL: [f32; 4] = [1.0f32, 1.0f32, 1.0f32, 1.0f32];
const BORDER_COL: [f32; 4] = [0.5f32, 0.5f32, 0.5f32, 1.0f32];

const BLOCK_SIZE_ITEMS: [&str; 9] = ["2", "4", "8", "16", "32", "64", "128", "256", "512"];
const SUBSAMPLING_ITEMS: [&str; 6] = ["4:4:4", "4:4:0", "4:2:2", "4:2:0", "4:1:1", "4:1:0"];
