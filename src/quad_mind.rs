#![allow(clippy::needless_range_loop)]

use crate::{
    jpeg::{self, Jpeg, JpegSteps},
    my_image::{self, MyImage},
    quad_tree::{self, QuadNode, QuadNodeRef, QuadTree},
    Vec2d, Vec3d,
};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex},
    thread,
};

use serde::{Deserialize, Serialize};
use sha2::Digest;

pub fn render_quad_mind(
    jpeg: &mut Jpeg,
    my_image: &mut MyImage,
    quad_tree: &QuadTree,
    use_ycbcr: bool,
    use_threads: bool,
    subsampling_index: usize,
) -> (Vec<QuadNodeRef>, Vec3d<i32>) {
    let square_size = if my_image.width > my_image.height {
        my_image.width.next_power_of_two()
    } else {
        my_image.height.next_power_of_two()
    };

    my_image.final_image = my_image.original_image.to_vec();

    let mut quad_root = QuadNode::new(my_image, 0, 0, square_size, square_size, 0);

    let mut quad_node_list: Vec<QuadNodeRef> = Vec::new();
    quad_tree.build(&mut quad_root, &mut quad_node_list, my_image);

    my_image.mwidth = my_image.width;
    my_image.mheight = my_image.height;

    for quad in &quad_node_list {
        let quad = quad.borrow();
        if quad.box_right > my_image.mwidth {
            my_image.mwidth = quad.box_right;
        }
        if quad.box_bottom > my_image.mheight {
            my_image.mheight = quad.box_bottom;
        }
    }

    jpeg.use_threads = use_threads;

    my_image.round_up_size(quad_tree.max_size);

    if use_ycbcr {
        my_image.image_to_ycbcr();
        my_image.fill_outbound();
        my_image.sub_sampling(true, subsampling_index);
    } else {
        my_image.image_to_rgb();
        my_image.fill_outbound();
        my_image.sub_sampling(false, subsampling_index);
    }

    let table_size = (quad_tree.max_size as f32).log2().ceil() as usize;

    let (dct_table, alpha_table) = if !jpeg.use_fast_dct {
        let mut dct_table = Vec::with_capacity(table_size);
        let mut alpha_table = Vec::with_capacity(table_size);

        for i in 0..table_size {
            let block_size = 1 << (i + 1);

            dct_table.push(Arc::new(jpeg::generate_dct_table(block_size)));
            alpha_table.push(Arc::new(jpeg::generate_alpha_table(block_size)));
        }

        (dct_table, alpha_table)
    } else {
        (Vec::new(), Vec::new())
    };

    let mut q_matrix_luma = jpeg::generate_q_matrix(
        &jpeg::Q_MATRIX_LUMA_CONST,
        quad_tree.max_size,
        jpeg.use_gen_qtable,
    );
    let mut q_matrix_chroma = jpeg::generate_q_matrix(
        &jpeg::Q_MATRIX_CHROMA_CONST,
        quad_tree.max_size,
        jpeg.use_gen_qtable,
    );

    if !jpeg.use_compression_rate {
        let factor = if jpeg.use_gen_qtable {
            if jpeg.quality >= 50.0f32 {
                200.0f32 - (jpeg.quality as f32 * 2.0f32)
            } else {
                5000.0f32 / jpeg.quality as f32
            }
        } else {
            25.0f32 * ((101.0f32 - jpeg.quality as f32) * 0.01f32)
        };

        jpeg::apply_q_matrix_factor(&mut q_matrix_luma, quad_tree.max_size, factor);
        jpeg::apply_q_matrix_factor(&mut q_matrix_chroma, quad_tree.max_size, factor);
    }

    let mut jpeg_steps = JpegSteps::new(jpeg, my_image.mwidth);

    let (final_result_block, final_dct_zig_zag_block) = if jpeg.use_threads {
        let mut image_block = Vec::with_capacity(quad_node_list.len());
        let mut result_block = Vec::with_capacity(quad_node_list.len());
        let mut jpeg_steps_list = Vec::with_capacity(quad_node_list.len());
        let mut dct_zig_zag_block = Vec::with_capacity(quad_node_list.len());

        let mut zig_zag_table = Vec::with_capacity(table_size);

        for i in 0..table_size {
            let block_size = 1 << (i + 1);

            zig_zag_table.push(Arc::new(generate_zig_zag_table(block_size)));
        }

        for quad in &quad_node_list {
            let quad = quad.borrow();

            jpeg_steps_list.push(Arc::new({
                let mut jpeg_steps = jpeg_steps.clone();

                let table_index = (quad.width_block_size as f32).log2().ceil() as usize - 1;

                if !jpeg.use_fast_dct {
                    jpeg_steps.dct_table = Some(Arc::clone(&dct_table[table_index]));
                    jpeg_steps.alpha_table = Some(Arc::clone(&alpha_table[table_index]));
                    jpeg_steps.two_block_size = 2.0f32 / quad.width_block_size as f32;
                }

                jpeg_steps.block_size_index = table_index;
                jpeg_steps.block_size = quad.width_block_size;

                jpeg_steps
            }));

            let mut solo_image_block: Vec2d<f32> =
                vec![vec![0.0f32; quad.width_block_size * quad.width_block_size]; 3];

            for j in 0..3 {
                for y in 0..quad.width_block_size {
                    for x in 0..quad.width_block_size {
                        let index_image_block = y * quad.width_block_size + x;
                        let index_image_converted =
                            (quad.box_top + y) * my_image.mwidth + (quad.box_left + x);

                        solo_image_block[j][index_image_block] =
                            my_image.image_converted[j][index_image_converted] as f32 - 128.0f32;
                    }
                }
            }

            image_block.push(solo_image_block);

            result_block.push(Arc::new(Mutex::new(vec![
                vec![
                    0.0f32;
                    quad.width_block_size
                        * quad.width_block_size
                ];
                3
            ])));

            dct_zig_zag_block.push(Arc::new(Mutex::new(vec![
                vec![
                    0i32;
                    quad.width_block_size
                        * quad.width_block_size
                ];
                3
            ])));
        }

        let image_block = Arc::new(image_block);

        let q_matrix_luma = Arc::new(q_matrix_luma);
        let q_matrix_chroma = Arc::new(q_matrix_chroma);

        let cpu_threads = thread::available_parallelism().unwrap().get();
        let pool = threadpool::ThreadPool::with_name("worker".to_string(), cpu_threads);

        for i in 0..quad_node_list.len() {
            let quad = quad_node_list[i].borrow();

            let quad_box_left = quad.box_left;
            let table_index = (quad.width_block_size as f32).log2().ceil() as usize - 1;

            let arc_jpeg_steps = Arc::clone(&jpeg_steps_list[i]);
            let arc_image_block = Arc::clone(&image_block);
            let arc_result_block = Arc::clone(&result_block[i]);
            let arc_zig_zag_table = Arc::clone(&zig_zag_table[table_index]);
            let arc_q_matrix_luma = Arc::clone(&q_matrix_luma);
            let arc_q_matrix_chroma = Arc::clone(&q_matrix_chroma);
            let arc_dct_zig_zag_block = Arc::clone(&dct_zig_zag_block[i]);

            pool.execute(move || {
                let arc_locked_result_block = &mut arc_result_block.lock().unwrap();
                let arc_locked_dct_zig_zag_block = &mut arc_dct_zig_zag_block.lock().unwrap();
                (arc_locked_result_block[0], arc_locked_dct_zig_zag_block[0]) = quad_mind_steps(
                    quad_box_left,
                    &arc_jpeg_steps,
                    &arc_image_block[i][0],
                    &arc_q_matrix_luma,
                    &arc_zig_zag_table,
                );
                (arc_locked_result_block[1], arc_locked_dct_zig_zag_block[1]) = quad_mind_steps(
                    quad_box_left,
                    &arc_jpeg_steps,
                    &arc_image_block[i][1],
                    &arc_q_matrix_chroma,
                    &arc_zig_zag_table,
                );
                (arc_locked_result_block[2], arc_locked_dct_zig_zag_block[2]) = quad_mind_steps(
                    quad_box_left,
                    &arc_jpeg_steps,
                    &arc_image_block[i][2],
                    &arc_q_matrix_chroma,
                    &arc_zig_zag_table,
                );
            });
        }
        pool.join();

        (
            result_block
                .iter()
                .map(|i| i.lock().unwrap().to_vec())
                .collect(),
            dct_zig_zag_block
                .iter()
                .map(|i| i.lock().unwrap().to_vec())
                .collect(),
        )
    } else {
        let mut image_block = Vec::with_capacity(quad_node_list.len());
        let mut result_block = Vec::with_capacity(quad_node_list.len());
        let mut dct_zig_zag_block = Vec::with_capacity(quad_node_list.len());

        let mut zig_zag_table = Vec::with_capacity(table_size);

        for i in 0..table_size {
            let block_size = 1 << (i + 1);

            zig_zag_table.push(generate_zig_zag_table(block_size));
        }

        for quad in &quad_node_list {
            let quad = quad.borrow();

            let mut solo_image_block: Vec2d<f32> =
                vec![vec![0.0f32; quad.width_block_size * quad.width_block_size]; 3];

            for j in 0..3 {
                for y in 0..quad.width_block_size {
                    for x in 0..quad.width_block_size {
                        let index_image_block = y * quad.width_block_size + x;
                        let index_image_converted =
                            (quad.box_top + y) * my_image.mwidth + (quad.box_left + x);

                        solo_image_block[j][index_image_block] =
                            my_image.image_converted[j][index_image_converted] as f32 - 128.0f32;
                    }
                }
            }

            image_block.push(solo_image_block);

            result_block.push(vec![
                vec![
                    0.0f32;
                    quad.width_block_size * quad.width_block_size
                ];
                3
            ]);

            dct_zig_zag_block.push(vec![
                vec![
                    0i32;
                    quad.width_block_size * quad.width_block_size
                ];
                3
            ]);
        }

        for i in 0..quad_node_list.len() {
            let quad = quad_node_list[i].borrow();

            let table_index = (quad.width_block_size as f32).log2().ceil() as usize - 1;

            if !jpeg.use_fast_dct {
                jpeg_steps.dct_table = Some(Arc::clone(&dct_table[table_index]));
                jpeg_steps.alpha_table = Some(Arc::clone(&alpha_table[table_index]));
                jpeg_steps.two_block_size = 2.0f32 / quad.width_block_size as f32;
            }

            jpeg_steps.block_size_index = table_index;
            jpeg_steps.block_size = quad.width_block_size;

            (result_block[i][0], dct_zig_zag_block[i][0]) = quad_mind_steps(
                quad.box_left,
                &jpeg_steps,
                &image_block[i][0],
                &q_matrix_luma,
                &zig_zag_table[table_index],
            );
            (result_block[i][1], dct_zig_zag_block[i][1]) = quad_mind_steps(
                quad.box_left,
                &jpeg_steps,
                &image_block[i][1],
                &q_matrix_chroma,
                &zig_zag_table[table_index],
            );
            (result_block[i][2], dct_zig_zag_block[i][2]) = quad_mind_steps(
                quad.box_left,
                &jpeg_steps,
                &image_block[i][2],
                &q_matrix_chroma,
                &zig_zag_table[table_index],
            );
        }

        (result_block, dct_zig_zag_block)
    };

    let mut result: Vec2d<u8> = vec![vec![0u8; my_image.mheight * my_image.mwidth]; 3];

    for i in 0..quad_node_list.len() {
        let quad = quad_node_list[i].borrow();

        for j in 0..3 {
            for y in 0..quad.width_block_size {
                for x in 0..quad.width_block_size {
                    let index_result_block = y * quad.width_block_size + x;
                    let index_result = (quad.box_top + y) * my_image.mwidth + (quad.box_left + x);

                    result[j][index_result] = my_image::min_max_color(
                        final_result_block[i][j][index_result_block] + 128.0f32,
                    );
                }
            }
        }
    }

    my_image.image_converted = result;

    if use_ycbcr {
        my_image.ycbcr_to_image()
    } else {
        my_image.rgb_to_image()
    }

    if quad_tree.use_draw_line {
        for quad in &quad_node_list {
            let quad = quad.borrow();
            let quad_boxr = if quad.box_right >= my_image.width {
                my_image.width - 1
            } else {
                quad.box_right
            };
            let quad_boxb = if quad.box_bottom >= my_image.height {
                my_image.height - 1
            } else {
                quad.box_bottom
            };
            quad_tree::draw_rect(
                &mut my_image.final_image,
                my_image.width,
                quad.box_left,
                quad.box_top,
                quad_boxr,
                quad_boxb,
            );
        }
    }

    (quad_node_list, final_dct_zig_zag_block)
}

fn quad_mind_steps(
    start_x: usize,
    jpeg_steps: &JpegSteps,
    image_block: &[f32],
    q_matrix: &[f32],
    zig_zag_table: &[usize],
) -> (Vec<f32>, Vec<i32>) {
    let mut dct_matrix = jpeg_steps.dct_function(image_block);
    jpeg_steps.quantize_function(start_x, q_matrix, &mut dct_matrix, false);
    let dct_matrix_zig_zag = zig_zag_function(zig_zag_table, jpeg_steps.block_size, &dct_matrix);

    if jpeg_steps.use_compression_rate {
        dct_matrix = jpeg_steps.dct_function(image_block);
        jpeg_steps.quantize_function(start_x, q_matrix, &mut dct_matrix, true);
        jpeg_steps.de_quantize_function(start_x, q_matrix, &mut dct_matrix, true);
    } else {
        jpeg_steps.de_quantize_function(start_x, q_matrix, &mut dct_matrix, false);
    };

    (
        jpeg_steps.inverse_dct_function(&dct_matrix),
        dct_matrix_zig_zag,
    )
}

fn quad_mind_steps_decompress_load(
    jpeg_steps: &JpegSteps,
    dct_matrix_zig_zag: &[i32],
    q_matrix: &[f32],
    zig_zag_table: &[usize],
) -> Vec<f32> {
    let mut dct_matrix =
        un_zig_zag_function(zig_zag_table, jpeg_steps.block_size, dct_matrix_zig_zag);
    jpeg_steps.de_quantize_function(0, q_matrix, &mut dct_matrix, false);
    jpeg_steps.inverse_dct_function(&dct_matrix)
}

#[repr(packed)]
#[derive(Serialize, Deserialize)]
pub struct QuadNodeJpeg {
    x: u32,
    y: u32,
    block_size: u8,
}

impl QuadNodeJpeg {
    pub fn new(x: u32, y: u32, block_size: u8) -> QuadNodeJpeg {
        QuadNodeJpeg { x, y, block_size }
    }
}

#[derive(Serialize, Deserialize)]
pub struct QuadMindData {
    start_signature: String,
    sha512: Vec<u8>,
    data: Vec<u8>,
    end_signature: String,
}

impl QuadMindData {
    pub fn new(
        start_signature: String,
        sha512: Vec<u8>,
        data: Vec<u8>,
        end_signature: String,
    ) -> QuadMindData {
        QuadMindData {
            start_signature,
            sha512,
            data,
            end_signature,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct QuadMindFile {
    signature: String,
    width: u32,
    height: u32,
    quality: f32,
    use_ycbcr: bool,
    use_threads: bool,
    use_fast_dct: bool,
    use_gen_qtable: bool,
    quad_mind_datas: Vec<QuadMindData>,
}

impl QuadMindFile {
    pub fn new(
        signature: String,
        width: u32,
        height: u32,
        quality: f32,
        use_ycbcr: bool,
        use_threads: bool,
        use_fast_dct: bool,
        use_gen_qtable: bool,
        quad_mind_datas: Vec<QuadMindData>,
    ) -> QuadMindFile {
        QuadMindFile {
            signature,
            width,
            height,
            quality,
            use_ycbcr,
            use_threads,
            use_fast_dct,
            use_gen_qtable,
            quad_mind_datas,
        }
    }
}

pub fn save_quad_mind(
    path: &Path,
    quad_node_list: &[QuadNodeRef],
    quad_dct_zig_zag: &Vec3d<i32>,
    my_image: &MyImage,
    jpeg: &Jpeg,
    use_ycbcr: bool,
    use_threads: bool,
) {
    let mut dct_zig_zag_count = 0;
    let mut quad_node_jpeg = Vec::with_capacity(quad_node_list.len());

    for quad in quad_node_list {
        let quad = quad.borrow();

        quad_node_jpeg.push(QuadNodeJpeg::new(
            quad.box_left as u32,
            quad.box_top as u32,
            (quad.width_block_size as f32).log2().ceil() as u8,
        ));

        dct_zig_zag_count += quad.width_block_size * quad.width_block_size * 3;
    }

    let mut dct_zig_zag = vec![0i32; dct_zig_zag_count];

    let mut dzz_index = 0;
    for i in 0..quad_node_list.len() {
        let quad = quad_node_list[i].borrow();

        for j in 0..3 {
            for k in 0..(quad.width_block_size * quad.width_block_size) {
                dct_zig_zag[dzz_index] = quad_dct_zig_zag[i][j][k];
                dzz_index += 1;
            }
        }
    }

    let serialized_quad_node_jpeg =
        bincode::serialize(&quad_node_jpeg).expect("Could not serialize quad node jpeg");

    let sha512_quad_node_jpeg = sha2::Sha512::digest(&serialized_quad_node_jpeg);

    let compressed_quad_node_jpeg =
        miniz_oxide::deflate::compress_to_vec(&serialized_quad_node_jpeg, 10);

    let quad_node_jpeg_data = QuadMindData::new(
        "SQNJ".to_string(),
        sha512_quad_node_jpeg.to_vec(),
        compressed_quad_node_jpeg,
        "EQNJ".to_string(),
    );

    let serialized_dct_zig_zag =
        bincode::serialize(&dct_zig_zag).expect("Could not serialize dct zig zag");

    let sha512_dct_zig_zag = sha2::Sha512::digest(&serialized_dct_zig_zag);

    let compressed_dct_zig_zag = miniz_oxide::deflate::compress_to_vec(&serialized_dct_zig_zag, 10);

    let dct_zig_zag_data = QuadMindData::new(
        "SDCT".to_string(),
        sha512_dct_zig_zag.to_vec(),
        compressed_dct_zig_zag,
        "EDCT".to_string(),
    );

    let quad_mind_file = QuadMindFile::new(
        "QUADMIND".to_string(),
        my_image.width as u32,
        my_image.height as u32,
        jpeg.quality,
        use_ycbcr,
        use_threads,
        jpeg.use_fast_dct,
        jpeg.use_gen_qtable,
        vec![quad_node_jpeg_data, dct_zig_zag_data],
    );

    let serialized_quad_mind_file =
        bincode::serialize(&quad_mind_file).expect("Could not serialize quad mind file");

    let mut file = File::create(path).expect("Could not create file");
    file.write_all(&serialized_quad_mind_file)
        .expect("Could not write to file");
}

pub fn load_quad_mind(path: &Path) -> Result<(MyImage, Jpeg), &str> {
    let mut contents: Vec<u8> = Vec::new();
    let mut file = File::open(path).expect("Could not open file");
    file.read_to_end(&mut contents)
        .expect("Could not read file");

    let quad_mind_file: QuadMindFile =
        bincode::deserialize(&contents).expect("Could not deserialize quad mind file");

    if quad_mind_file.signature != "QUADMIND" {
        return Err("Wrong QUADMIND signature");
    }

    let quad_node_jpeg_data = &quad_mind_file.quad_mind_datas[0];

    if quad_node_jpeg_data.start_signature != "SQNJ" {
        return Err("Wrong QNJ start signature");
    }
    if quad_node_jpeg_data.end_signature != "EQNJ" {
        return Err("Wrong QNJ end signature");
    }

    let serialized_quad_node_jpeg =
        miniz_oxide::inflate::decompress_to_vec(&quad_node_jpeg_data.data)
            .expect("Could not decompressed quad node jpeg");

    let sha512_quad_node_jpeg_test = sha2::Sha512::digest(&serialized_quad_node_jpeg);

    if quad_node_jpeg_data.sha512[..] != sha512_quad_node_jpeg_test[..] {
        return Err("Wrong QNJ sha512 signature");
    }

    let quad_node_jpeg: Vec<QuadNodeJpeg> = bincode::deserialize(&serialized_quad_node_jpeg)
        .expect("Could not deserialize quad node jpeg");

    let dct_zig_zag_data = &quad_mind_file.quad_mind_datas[1];

    if dct_zig_zag_data.start_signature != "SDCT" {
        return Err("Wrong DCT start signature");
    }
    if dct_zig_zag_data.end_signature != "EDCT" {
        return Err("Wrong DCT end signature");
    }

    let serialized_dct_zig_zag = miniz_oxide::inflate::decompress_to_vec(&dct_zig_zag_data.data)
        .expect("Could not decompressed dct zig zag");

    let sha512_dct_zig_zag_test = sha2::Sha512::digest(&serialized_dct_zig_zag);

    if dct_zig_zag_data.sha512[..] != sha512_dct_zig_zag_test[..] {
        return Err("Wrong DCT sha512 signature");
    }

    let quad_root_dct_zig_zag: Vec<i32> =
        bincode::deserialize(&serialized_dct_zig_zag).expect("Could not deserialize dct zig zag");

    let mut dct_zig_zag: Vec3d<i32> = vec![vec![Vec::new(); 3]; quad_node_jpeg.len()];

    let mut dzz_index = 0;
    for i in 0..quad_node_jpeg.len() {
        let quad = &quad_node_jpeg[i];
        let quad_block_size = 1 << quad.block_size;
        let quad_block_size_max = (quad_block_size * quad_block_size) as usize;

        for j in 0..3 {
            dct_zig_zag[i][j].resize(quad_block_size_max, 0);

            for k in 0..quad_block_size_max {
                dct_zig_zag[i][j][k] = quad_root_dct_zig_zag[dzz_index];
                dzz_index += 1;
            }
        }
    }

    Ok(decode_quad_mind(
        dct_zig_zag,
        quad_mind_file,
        quad_node_jpeg,
        path.to_str().unwrap().to_string(),
    ))
}

pub fn decode_quad_mind(
    dct_zig_zag: Vec3d<i32>,
    quad_mind_file: QuadMindFile,
    quad_node_jpeg: Vec<QuadNodeJpeg>,
    file_path: String,
) -> (MyImage, Jpeg) {
    let mut my_image = MyImage::new(
        Vec::new(),
        quad_mind_file.width as usize,
        quad_mind_file.height as usize,
        file_path,
    );

    let mut max_size = 0;

    for quad in &quad_node_jpeg {
        let block_size = (1 << quad.block_size) as usize;

        let quad_box_right = quad.x as usize + block_size;
        let quad_box_bottom = quad.y as usize + block_size;

        if quad_box_right > my_image.mwidth {
            my_image.mwidth = quad_box_right;
        }
        if quad_box_bottom > my_image.mheight {
            my_image.mheight = quad_box_bottom;
        }

        if block_size > max_size {
            max_size = block_size;
        }
    }

    my_image.round_up_size(max_size);

    let table_size = (max_size as f32).log2().ceil() as usize;

    let (dct_table, alpha_table) = if !quad_mind_file.use_fast_dct {
        let mut dct_table = Vec::with_capacity(table_size);
        let mut alpha_table = Vec::with_capacity(table_size);

        for i in 0..table_size {
            let block_size = 1 << (i + 1);

            dct_table.push(Arc::new(jpeg::generate_dct_table(block_size)));
            alpha_table.push(Arc::new(jpeg::generate_alpha_table(block_size)));
        }

        (dct_table, alpha_table)
    } else {
        (Vec::new(), Vec::new())
    };

    let mut zig_zag_table = Vec::with_capacity(table_size);

    for i in 0..table_size {
        let block_size = 1 << (i + 1);

        zig_zag_table.push(Arc::new(generate_zig_zag_table(block_size)));
    }

    let mut q_matrix_luma = jpeg::generate_q_matrix(
        &jpeg::Q_MATRIX_LUMA_CONST,
        max_size,
        quad_mind_file.use_gen_qtable,
    );
    let mut q_matrix_chroma = jpeg::generate_q_matrix(
        &jpeg::Q_MATRIX_CHROMA_CONST,
        max_size,
        quad_mind_file.use_gen_qtable,
    );

    let factor = if quad_mind_file.use_gen_qtable {
        if quad_mind_file.quality >= 50.0f32 {
            200.0f32 - (quad_mind_file.quality as f32 * 2.0f32)
        } else {
            5000.0f32 / quad_mind_file.quality as f32
        }
    } else {
        25.0f32 * ((101.0f32 - quad_mind_file.quality as f32) * 0.01f32)
    };

    jpeg::apply_q_matrix_factor(&mut q_matrix_luma, max_size, factor);
    jpeg::apply_q_matrix_factor(&mut q_matrix_chroma, max_size, factor);

    let jpeg = Jpeg::new(
        8,
        quad_mind_file.quality,
        1.0f32,
        2,
        quad_mind_file.use_gen_qtable,
        quad_mind_file.use_threads,
        quad_mind_file.use_fast_dct,
        false,
    );

    let mut jpeg_steps = JpegSteps::new(&jpeg, my_image.mwidth);

    let final_result_block = if jpeg.use_threads {
        let mut result_block = Vec::with_capacity(quad_node_jpeg.len());
        let mut jpeg_steps_list = Vec::with_capacity(quad_node_jpeg.len());

        let mut zig_zag_table = Vec::with_capacity(table_size);

        for i in 0..table_size {
            let block_size = 1 << (i + 1);

            zig_zag_table.push(Arc::new(generate_zig_zag_table(block_size)));
        }

        for quad in &quad_node_jpeg {
            let block_size = (1 << quad.block_size) as usize;

            jpeg_steps_list.push(Arc::new({
                let mut jpeg_steps = jpeg_steps.clone();

                let table_index = quad.block_size as usize - 1;

                if !jpeg.use_fast_dct {
                    jpeg_steps.dct_table = Some(Arc::clone(&dct_table[table_index]));
                    jpeg_steps.alpha_table = Some(Arc::clone(&alpha_table[table_index]));
                    jpeg_steps.two_block_size = 2.0f32 / block_size as f32;
                }

                jpeg_steps.block_size = block_size;
                jpeg_steps.block_size_index = table_index;

                jpeg_steps
            }));

            result_block.push(Arc::new(Mutex::new(vec![
                vec![
                    0.0f32;
                    block_size * block_size
                ];
                3
            ])));
        }

        let dct_zig_zag = Arc::new(dct_zig_zag);

        let q_matrix_luma = Arc::new(q_matrix_luma);
        let q_matrix_chroma = Arc::new(q_matrix_chroma);

        let cpu_threads = thread::available_parallelism().unwrap().get();
        let pool = threadpool::ThreadPool::with_name("worker".to_string(), cpu_threads);

        for i in 0..quad_node_jpeg.len() {
            let table_index = quad_node_jpeg[i].block_size as usize - 1;

            let arc_jpeg_steps = Arc::clone(&jpeg_steps_list[i]);
            let arc_dct_zig_zag = Arc::clone(&dct_zig_zag);
            let arc_result_block = Arc::clone(&result_block[i]);
            let arc_zig_zag_table = Arc::clone(&zig_zag_table[table_index]);
            let arc_q_matrix_luma = Arc::clone(&q_matrix_luma);
            let arc_q_matrix_chroma = Arc::clone(&q_matrix_chroma);

            pool.execute(move || {
                let arc_locked_result_block = &mut arc_result_block.lock().unwrap();
                arc_locked_result_block[0] = quad_mind_steps_decompress_load(
                    &arc_jpeg_steps,
                    &arc_dct_zig_zag[i][0],
                    &arc_q_matrix_luma,
                    &arc_zig_zag_table,
                );
                arc_locked_result_block[1] = quad_mind_steps_decompress_load(
                    &arc_jpeg_steps,
                    &arc_dct_zig_zag[i][1],
                    &arc_q_matrix_chroma,
                    &arc_zig_zag_table,
                );
                arc_locked_result_block[2] = quad_mind_steps_decompress_load(
                    &arc_jpeg_steps,
                    &arc_dct_zig_zag[i][2],
                    &arc_q_matrix_chroma,
                    &arc_zig_zag_table,
                );
            });
        }
        pool.join();

        result_block
            .iter()
            .map(|i| i.lock().unwrap().to_vec())
            .collect()
    } else {
        let mut result_block = Vec::with_capacity(quad_node_jpeg.len());

        let mut zig_zag_table = Vec::with_capacity(table_size);

        for i in 0..table_size {
            let block_size = 1 << (i + 1);

            zig_zag_table.push(generate_zig_zag_table(block_size));
        }

        for quad in &quad_node_jpeg {
            let block_size = (1 << quad.block_size) as usize;

            result_block.push(vec![vec![0.0f32; block_size * block_size]; 3]);
        }

        for i in 0..quad_node_jpeg.len() {
            let block_size = (1 << quad_node_jpeg[i].block_size) as usize;

            let table_index = quad_node_jpeg[i].block_size as usize - 1;

            if !jpeg.use_fast_dct {
                jpeg_steps.dct_table = Some(Arc::clone(&dct_table[table_index]));
                jpeg_steps.alpha_table = Some(Arc::clone(&alpha_table[table_index]));
                jpeg_steps.two_block_size = 2.0f32 / block_size as f32;
            }

            jpeg_steps.block_size = block_size;
            jpeg_steps.block_size_index = table_index;

            result_block[i][0] = quad_mind_steps_decompress_load(
                &jpeg_steps,
                &dct_zig_zag[i][0],
                &q_matrix_luma,
                &zig_zag_table[table_index],
            );
            result_block[i][1] = quad_mind_steps_decompress_load(
                &jpeg_steps,
                &dct_zig_zag[i][1],
                &q_matrix_chroma,
                &zig_zag_table[table_index],
            );
            result_block[i][2] = quad_mind_steps_decompress_load(
                &jpeg_steps,
                &dct_zig_zag[i][2],
                &q_matrix_chroma,
                &zig_zag_table[table_index],
            );
        }

        result_block
    };

    let mut result: Vec2d<u8> = vec![vec![0u8; my_image.mheight * my_image.mwidth]; 3];

    for i in 0..quad_node_jpeg.len() {
        let block_size = (1 << quad_node_jpeg[i].block_size) as usize;

        for j in 0..3 {
            for y in 0..block_size {
                for x in 0..block_size {
                    let index_result_block = y * block_size + x;
                    let index_result = (quad_node_jpeg[i].y as usize + y) * my_image.mwidth
                        + (quad_node_jpeg[i].x as usize + x);

                    result[j][index_result] = my_image::min_max_color(
                        final_result_block[i][j][index_result_block] + 128.0f32,
                    );
                }
            }
        }
    }

    my_image.image_converted = result;

    if quad_mind_file.use_ycbcr {
        my_image.ycbcr_to_image()
    } else {
        my_image.rgb_to_image()
    }

    my_image.original_image = my_image.final_image.to_vec();

    (my_image, jpeg)
}

fn zig_zag_function(zig_zag_table: &[usize], block_size: usize, dct_matrix: &[f32]) -> Vec<i32> {
    let mut dct_matrix_zig_zag: Vec<i32> = vec![0i32; block_size * block_size];
    for y in 0..block_size {
        for x in 0..block_size {
            let index = y * block_size + x;
            dct_matrix_zig_zag[zig_zag_table[index]] = dct_matrix[index] as i32;
        }
    }

    dct_matrix_zig_zag
}

fn un_zig_zag_function(
    zig_zag_table: &[usize],
    block_size: usize,
    dct_matrix_zig_zag: &[i32],
) -> Vec<f32> {
    let mut dct_matrix: Vec<f32> = vec![0.0f32; block_size * block_size];
    for y in 0..block_size {
        for x in 0..block_size {
            let index = y * block_size + x;
            dct_matrix[index] = dct_matrix_zig_zag[zig_zag_table[index]] as f32;
        }
    }

    dct_matrix
}

pub fn generate_zig_zag_table(block_size: usize) -> Vec<usize> {
    let mut zig_zag_table = vec![0usize; block_size * block_size];

    let mut n = 0;
    let mut j: usize;
    let mut index: usize;

    for i in 0..(block_size * 2) {
        if i < block_size {
            j = 0;
        } else {
            j = i - block_size + 1;
        }

        while j <= i && j < block_size {
            if (i & 1) == 1 {
                index = j * (block_size - 1) + i;
            } else {
                index = (i - j) * block_size + j;
            }

            zig_zag_table[index] = n;
            n += 1;
            j += 1;
        }
    }

    zig_zag_table
}
