use crate::{jpeg, my_image, quad_tree};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
};

pub fn render_quad_mind(
    jpeg: &mut jpeg::Jpeg,
    my_image: &mut my_image::MyImage,
    min_size: usize,
    max_size: usize,
    max_depth: u32,
    use_draw_line: bool,
    threshold_error: f32,
    use_ycbcr: bool,
    use_threads: bool,
    subsampling_index: usize,
) -> Vec<Rc<RefCell<quad_tree::QuadNode>>> {
    let square_size = if my_image.width > my_image.height {
        my_image.width.next_power_of_two()
    } else {
        my_image.height.next_power_of_two()
    };

    let mut root_quad = quad_tree::QuadNode::new(
        Rc::new(my_image.original_image.to_vec()),
        my_image.width,
        my_image.height,
        0,
        0,
        square_size,
        square_size,
        0,
    );

    let mut list_quad: Vec<Rc<RefCell<quad_tree::QuadNode>>> = vec![];
    quad_tree::build_tree(
        &mut root_quad,
        &mut list_quad,
        my_image.width,
        my_image.height,
        max_depth,
        threshold_error,
        min_size,
        max_size,
    );

    my_image.mwidth = my_image.width;
    my_image.mheight = my_image.height;

    for quad in &list_quad {
        let quad = quad.borrow();
        if quad.boxr > my_image.mwidth {
            my_image.mwidth = quad.boxr;
        }
        if quad.boxb > my_image.mheight {
            my_image.mheight = quad.boxb;
        }
    }

    jpeg.use_threads = use_threads;

    my_image.mwidth = jpeg::round_up_block_size(my_image.mwidth, max_size);
    my_image.mheight = jpeg::round_up_block_size(my_image.mheight, max_size);

    match use_ycbcr {
        true => {
            my_image.image_to_ycbcr();
            my_image.fill_outbound();
            my_image.subsampling(true, subsampling_index);
        }
        false => {
            my_image.image_to_rgb();
            my_image.fill_outbound();
            my_image.subsampling(false, subsampling_index);
        }
    }

    let table_size = (max_size as f32).log2().ceil() as usize;

    let (dct_table, alpha_table) = if !jpeg.use_fast_dct {
        let mut dct_table: Vec<Vec<Vec<f32>>> = Vec::with_capacity(table_size);
        let mut alpha_table: Vec<Vec<f32>> = Vec::with_capacity(table_size);
        for i in 0..table_size {
            let block_size = 1 << (i + 1);
            dct_table.push(jpeg::generate_dct_table(block_size));
            alpha_table.push(jpeg::generate_alpha_table(block_size));
        }
        (dct_table, alpha_table)
    } else {
        (vec![], vec![])
    };

    let mut zig_zag_table = Vec::with_capacity(table_size);
    for i in 0..table_size {
        let block_size = 1 << (i + 1);
        zig_zag_table.push(jpeg::generate_zig_zag_table(block_size));
    }

    let (image_block, dct_matrix) = {
        let mut image_block: Vec<Vec<f32>> = Vec::with_capacity(table_size);
        let mut dct_matrix: Vec<Vec<f32>> = Vec::with_capacity(table_size);
        for i in 0..table_size {
            let block_size_total = (1 << (i + 1)) * (1 << (i + 1));
            image_block.push(vec![0.0f32; block_size_total]);
            dct_matrix.push(vec![0.0f32; block_size_total]);
        }
        (image_block, dct_matrix)
    };

    let mut q_matrix_luma =
        jpeg::generate_q_matrix(&jpeg::Q_MATRIX_LUMA_CONST, max_size, jpeg.use_gen_qtable);
    let mut q_matrix_chroma =
        jpeg::generate_q_matrix(&jpeg::Q_MATRIX_CHROMA_CONST, max_size, jpeg.use_gen_qtable);

    if !jpeg.use_compression_rate {
        let factor = match jpeg.use_gen_qtable {
            true => {
                if jpeg.quality >= 50 {
                    200.0f32 - (jpeg.quality as f32 * 2.0f32)
                } else {
                    5000.0f32 / jpeg.quality as f32
                }
            }
            false => 25.0f32 * ((101.0f32 - jpeg.quality as f32) * 0.01f32),
        };
        jpeg::apply_q_matrix_factor(&mut q_matrix_luma, max_size, factor);
        jpeg::apply_q_matrix_factor(&mut q_matrix_chroma, max_size, factor);
    }

    let mut jpeg_steps = jpeg::JpegSteps::new(jpeg, my_image.mwidth);

    if jpeg.use_threads {
        let result = Arc::new(Mutex::new(vec![
            vec![
                0u8;
                my_image.mheight * my_image.mwidth
            ];
            3
        ]));
        let jpeg_steps = Arc::new(Mutex::new(jpeg_steps));
        let q_matrix_luma = Arc::new(q_matrix_luma);
        let q_matrix_chroma = Arc::new(q_matrix_chroma);
        let image_converted = Arc::new(my_image.image_converted.to_vec());

        let dct_matrix = Arc::new(dct_matrix);
        let image_block = Arc::new(image_block);
        let zig_zag_table = Arc::new(zig_zag_table);

        let dct_table = Arc::new(dct_table);
        let alpha_table = Arc::new(alpha_table);

        let cpu_threads = thread::available_parallelism().unwrap().get();
        let pool = threadpool::ThreadPool::with_name("worker".to_string(), cpu_threads);

        let jpeg_do_not_use_fast_dct = !jpeg.use_fast_dct;

        for quad in &list_quad {
            let quad = quad.borrow();

            let start_x = quad.boxl;
            let start_y = quad.boxt;
            let quad_width_block_size = quad.width_block_size;
            let table_index = (quad_width_block_size as f32).log2().ceil() as usize - 1;

            let arc_result = Arc::clone(&result);
            let arc_jpeg_steps = Arc::clone(&jpeg_steps);
            let arc_q_matrix_luma = Arc::clone(&q_matrix_luma);
            let arc_q_matrix_chroma = Arc::clone(&q_matrix_chroma);
            let arc_image_converted = Arc::clone(&image_converted);

            let arc_dct_matrix = Arc::clone(&dct_matrix);
            let arc_image_block = Arc::clone(&image_block);
            let arc_zig_zag_table = Arc::clone(&zig_zag_table);

            let arc_dct_table = Arc::clone(&dct_table);
            let arc_alpha_table = Arc::clone(&alpha_table);

            pool.execute(move || {
                let result = &mut *arc_result.lock().expect("Could not lock result");
                let jpeg_steps = &mut *arc_jpeg_steps.lock().expect("Could not lock jpeg_steps");
                let q_matrix_luma = &*arc_q_matrix_luma;
                let q_matrix_chroma = &*arc_q_matrix_chroma;
                let image_converted = &*arc_image_converted;

                let dct_matrix = &*arc_dct_matrix;
                let image_block = &*arc_image_block;
                let zig_zag_table = &*arc_zig_zag_table;

                let dct_table = &*arc_dct_table;
                let alpha_table = &*arc_alpha_table;

                jpeg_steps.dct_matrix = dct_matrix[table_index].to_vec();
                jpeg_steps.image_block = image_block[table_index].to_vec();
                jpeg_steps.zig_zag_table = zig_zag_table[table_index].to_vec();

                if jpeg_do_not_use_fast_dct {
                    jpeg_steps.dct_table = dct_table[table_index].to_vec();
                    jpeg_steps.alpha_table = alpha_table[table_index].to_vec();
                    jpeg_steps.two_block_size = 2.0f32 / quad_width_block_size as f32;
                }

                jpeg_steps.block_size_index = table_index;
                jpeg_steps.block_size = quad_width_block_size;

                jpeg_steps.start_x = start_x;
                jpeg_steps.start_y = start_y;

                quad_mind_steps(
                    jpeg_steps,
                    &mut result[0],
                    &image_converted[0],
                    q_matrix_luma,
                );
                quad_mind_steps(
                    jpeg_steps,
                    &mut result[1],
                    &image_converted[1],
                    q_matrix_chroma,
                );
                quad_mind_steps(
                    jpeg_steps,
                    &mut result[2],
                    &image_converted[2],
                    q_matrix_chroma,
                );
            });
        }

        pool.join();
        my_image.image_converted = (*result.lock().expect("Could not lock result")).to_vec();
    } else {
        let mut result = vec![vec![0u8; my_image.mheight * my_image.mwidth]; 3];

        for quad in &list_quad {
            let quad = quad.borrow();

            let table_index = (quad.width_block_size as f32).log2().ceil() as usize - 1;

            jpeg_steps.dct_matrix = dct_matrix[table_index].to_vec();
            jpeg_steps.image_block = image_block[table_index].to_vec();
            jpeg_steps.zig_zag_table = zig_zag_table[table_index].to_vec();

            if !jpeg.use_fast_dct {
                jpeg_steps.dct_table = dct_table[table_index].to_vec();
                jpeg_steps.alpha_table = alpha_table[table_index].to_vec();
                jpeg_steps.two_block_size = 2.0f32 / quad.width_block_size as f32;
            }

            jpeg_steps.block_size_index = table_index;
            jpeg_steps.block_size = quad.width_block_size;

            jpeg_steps.start_x = quad.boxl;
            jpeg_steps.start_y = quad.boxt;

            quad_mind_steps(
                &mut jpeg_steps,
                &mut result[0],
                &my_image.image_converted[0],
                &q_matrix_luma,
            );
            quad_mind_steps(
                &mut jpeg_steps,
                &mut result[1],
                &my_image.image_converted[1],
                &q_matrix_chroma,
            );
            quad_mind_steps(
                &mut jpeg_steps,
                &mut result[2],
                &my_image.image_converted[2],
                &q_matrix_chroma,
            );
        }

        my_image.image_converted = result;
    }

    match use_ycbcr {
        true => my_image.ycbcr_to_image(),
        false => my_image.rgb_to_image(),
    }

    if use_draw_line {
        let width = my_image.width;
        for quad in &list_quad {
            let quad = quad.borrow();
            let quad_boxr = if quad.boxr >= my_image.width {
                my_image.width - 1
            } else {
                quad.boxr
            };
            let quad_boxb = if quad.boxb >= my_image.height {
                my_image.height - 1
            } else {
                quad.boxb
            };
            quad_tree::draw_rect(
                &mut my_image.final_image,
                width,
                quad.boxl,
                quad.boxt,
                quad_boxr,
                quad_boxb,
            );
        }
    }

    list_quad
}

fn quad_mind_steps(
    jpeg_steps: &mut jpeg::JpegSteps,
    result: &mut [u8],
    image_converted: &[u8],
    q_matrix: &[f32],
) -> Vec<i32> {
    jpeg_steps.dct_function(image_converted);
    jpeg_steps.quantize_function(q_matrix);
    let dct_matrix_zigzag = jpeg_steps.zig_zag_function();
    jpeg_steps.de_quantize_function(q_matrix);
    jpeg_steps.inverse_dct_function(result);
    dct_matrix_zigzag
}

fn quad_mind_steps_decompress_load(
    jpeg_steps: &mut jpeg::JpegSteps,
    result: &mut [u8],
    q_matrix: &[f32],
    dct_matrix_zigzag: &[i32],
) {
    jpeg_steps.un_zig_zag_function(dct_matrix_zigzag);
    jpeg_steps.de_quantize_function(q_matrix);
    jpeg_steps.inverse_dct_function(result);
}
