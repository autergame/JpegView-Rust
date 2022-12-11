#![allow(clippy::needless_range_loop)]

use crate::{my_image, Vec2d, Vec3d};
use std::{
    f32,
    sync::{Arc, Mutex},
    thread,
};

type FunctionDct = fn(&[f32], &mut [f32]);

const FUNCTIONS_FAST_DCT: [FunctionDct; 9] = [
    fast_generated_dct::dct2::fast_dct,
    fast_generated_dct::dct4::fast_dct,
    fast_generated_dct::dct8::fast_dct,
    fast_generated_dct::dct16::fast_dct,
    fast_generated_dct::dct32::fast_dct,
    fast_generated_dct::dct64::fast_dct,
    fast_generated_dct::dct128::fast_dct,
    fast_generated_dct::dct256::fast_dct,
    fast_generated_dct::dct512::fast_dct,
];
const FUNCTIONS_FAST_IDCT: [FunctionDct; 9] = [
    fast_generated_dct::dct2::fast_idct,
    fast_generated_dct::dct4::fast_idct,
    fast_generated_dct::dct8::fast_idct,
    fast_generated_dct::dct16::fast_idct,
    fast_generated_dct::dct32::fast_idct,
    fast_generated_dct::dct64::fast_idct,
    fast_generated_dct::dct128::fast_idct,
    fast_generated_dct::dct256::fast_idct,
    fast_generated_dct::dct512::fast_idct,
];

#[rustfmt::skip]
pub const Q_MATRIX_LUMA_CONST: [f32; 64] = [
	16.0f32, 11.0f32, 10.0f32, 16.0f32,  24.0f32,  40.0f32,  51.0f32,  61.0f32,
	12.0f32, 12.0f32, 14.0f32, 19.0f32,  26.0f32,  58.0f32,  60.0f32,  55.0f32,
	14.0f32, 13.0f32, 16.0f32, 24.0f32,  40.0f32,  57.0f32,  69.0f32,  56.0f32,
	14.0f32, 17.0f32, 22.0f32, 29.0f32,  51.0f32,  87.0f32,  80.0f32,  62.0f32,
	18.0f32, 22.0f32, 37.0f32, 56.0f32,  68.0f32, 109.0f32, 103.0f32,  77.0f32,
	24.0f32, 35.0f32, 55.0f32, 64.0f32,  81.0f32, 104.0f32, 113.0f32,  92.0f32,
	49.0f32, 64.0f32, 78.0f32, 87.0f32, 103.0f32, 121.0f32, 120.0f32, 101.0f32,
	72.0f32, 92.0f32, 95.0f32, 98.0f32, 112.0f32, 100.0f32, 103.0f32,  99.0f32
];
#[rustfmt::skip]
pub const Q_MATRIX_CHROMA_CONST: [f32; 64] = [
	17.0f32, 18.0f32, 24.0f32, 47.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32,
	18.0f32, 21.0f32, 26.0f32, 66.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32,
	24.0f32, 26.0f32, 56.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32,
	47.0f32, 66.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32,
	99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32,
	99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32,
	99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32,
	99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32, 99.0f32
];

pub struct Jpeg {
    pub block_size: usize,
    pub block_size_index: usize,

    pub quality: f32,
    pub quality_start: f32,

    pub use_threads: bool,
    pub use_fast_dct: bool,
    pub use_gen_qtable: bool,
    pub use_compression_rate: bool,
}

impl Jpeg {
    pub fn new(
        block_size: usize,
        quality: f32,
        quality_start: f32,
        block_size_index: usize,
        use_gen_qtable: bool,
        use_threads: bool,
        use_fast_dct: bool,
        use_compression_rate: bool,
    ) -> Jpeg {
        Jpeg {
            block_size,
            block_size_index,

            quality,
            quality_start,

            use_threads,
            use_fast_dct,
            use_gen_qtable,
            use_compression_rate,
        }
    }
    pub fn render_jpeg(
        &mut self,
        my_image: &mut my_image::MyImage,
        use_ycbcr: bool,
        use_threads: bool,
        subsampling_index: usize,
    ) {
        self.use_threads = use_threads;

        my_image.round_up_size(self.block_size);

        if use_ycbcr {
            my_image.image_to_ycbcr();
            my_image.fill_outbound();
            my_image.sub_sampling(true, subsampling_index);
        } else {
            my_image.image_to_rgb();
            my_image.fill_outbound();
            my_image.sub_sampling(false, subsampling_index);
        }

        let mut q_matrix_luma =
            generate_q_matrix(&Q_MATRIX_LUMA_CONST, self.block_size, self.use_gen_qtable);
        let mut q_matrix_chroma =
            generate_q_matrix(&Q_MATRIX_CHROMA_CONST, self.block_size, self.use_gen_qtable);

        if !self.use_compression_rate {
            let factor = if self.quality >= 50.0f32 {
                200.0f32 - (self.quality * 2.0f32)
            } else {
                5000.0f32 / self.quality
            };

            apply_q_matrix_factor(&mut q_matrix_luma, self.block_size, factor);
            apply_q_matrix_factor(&mut q_matrix_chroma, self.block_size, factor);
        }

        self.encode(my_image, q_matrix_luma, q_matrix_chroma);

        if use_ycbcr {
            my_image.ycbcr_to_image();
        } else {
            my_image.rgb_to_image();
        }
    }
    pub fn encode(
        &mut self,
        my_image: &mut my_image::MyImage,
        q_matrix_luma: Vec<f32>,
        q_matrix_chroma: Vec<f32>,
    ) {
        let mut jpeg_steps = JpegSteps::new(self, my_image.mwidth);

        if !self.use_fast_dct {
            jpeg_steps.dct_table = Some(Arc::new(generate_dct_table(self.block_size)));
            jpeg_steps.alpha_table = Some(Arc::new(generate_alpha_table(self.block_size)));
        }

        let block_width_count = my_image.mwidth / self.block_size;
        let block_height_count = my_image.mheight / self.block_size;

        let final_result_block = if self.use_threads {
            let jpeg_steps = Arc::new(jpeg_steps);

            let mut image_block = Vec::with_capacity(block_width_count * block_height_count);

            for by in 0..block_height_count {
                for bx in 0..block_width_count {
                    let mut solo_image_block: Vec2d<f32> =
                        vec![vec![0.0f32; self.block_size * self.block_size]; 3];

                    for i in 0..3 {
                        for y in 0..self.block_size {
                            for x in 0..self.block_size {
                                let index_image_block = y * self.block_size + x;
                                let index_image_converted = ((by * self.block_size) + y)
                                    * my_image.mwidth
                                    + ((bx * self.block_size) + x);

                                solo_image_block[i][index_image_block] =
                                    my_image.image_converted[i][index_image_converted] as f32
                                        - 128.0f32;
                            }
                        }
                    }

                    image_block.push(solo_image_block);
                }
            }

            let mut result_block = Vec::with_capacity(block_width_count * block_height_count);

            for _ in 0..(block_width_count * block_height_count) {
                result_block.push(Arc::new(Mutex::new(vec![
                    vec![
                        0u8;
                        self.block_size
                            * self.block_size
                    ];
                    3
                ])));
            }

            let image_block = Arc::new(image_block);

            let q_matrix_luma = Arc::new(q_matrix_luma);
            let q_matrix_chroma = Arc::new(q_matrix_chroma);

            let cpu_threads = thread::available_parallelism().unwrap().get();
            let pool = threadpool::ThreadPool::with_name("worker".to_string(), cpu_threads);

            for by in 0..block_height_count {
                for bx in 0..block_width_count {
                    let start_x = bx * self.block_size;
                    let index = by * block_width_count + bx;

                    let arc_jpeg_steps = Arc::clone(&jpeg_steps);
                    let arc_image_block = Arc::clone(&image_block);
                    let arc_result_block = Arc::clone(&result_block[index]);
                    let arc_q_matrix_luma = Arc::clone(&q_matrix_luma);
                    let arc_q_matrix_chroma = Arc::clone(&q_matrix_chroma);

                    pool.execute(move || {
                        let arc_locked_result_block = &mut arc_result_block.lock().unwrap();
                        arc_locked_result_block[0] = arc_jpeg_steps.jpeg_steps(
                            start_x,
                            &arc_image_block[index][0],
                            &arc_q_matrix_luma,
                        );
                        arc_locked_result_block[1] = arc_jpeg_steps.jpeg_steps(
                            start_x,
                            &arc_image_block[index][1],
                            &arc_q_matrix_chroma,
                        );
                        arc_locked_result_block[2] = arc_jpeg_steps.jpeg_steps(
                            start_x,
                            &arc_image_block[index][2],
                            &arc_q_matrix_chroma,
                        );
                    });
                }
            }
            pool.join();

            result_block
                .iter()
                .map(|i| i.lock().unwrap().to_vec())
                .collect()
        } else {
            let mut image_block: Vec3d<f32> =
                vec![
                    vec![vec![0.0f32; self.block_size * self.block_size]; 3];
                    block_width_count * block_height_count
                ];

            for i in 0..3 {
                for by in 0..block_height_count {
                    for bx in 0..block_width_count {
                        for y in 0..self.block_size {
                            for x in 0..self.block_size {
                                let index_image = by * block_width_count + bx;
                                let index_image_block = y * self.block_size + x;
                                let index_image_converted = ((by * self.block_size) + y)
                                    * my_image.mwidth
                                    + ((bx * self.block_size) + x);

                                image_block[index_image][i][index_image_block] =
                                    my_image.image_converted[i][index_image_converted] as f32
                                        - 128.0f32;
                            }
                        }
                    }
                }
            }

            let mut result_block: Vec3d<u8> =
                vec![
                    vec![vec![0u8; self.block_size * self.block_size]; 3];
                    block_width_count * block_height_count
                ];

            for by in 0..block_height_count {
                for bx in 0..block_width_count {
                    let index = by * block_width_count + bx;

                    result_block[index][0] =
                        jpeg_steps.jpeg_steps(bx, &image_block[index][0], &q_matrix_luma);
                    result_block[index][1] =
                        jpeg_steps.jpeg_steps(bx, &image_block[index][1], &q_matrix_chroma);
                    result_block[index][2] =
                        jpeg_steps.jpeg_steps(bx, &image_block[index][2], &q_matrix_chroma);
                }
            }

            result_block
        };

        let mut result: Vec2d<u8> = vec![vec![0u8; my_image.mheight * my_image.mwidth]; 3];

        for i in 0..3 {
            for by in 0..block_height_count {
                for bx in 0..block_width_count {
                    for y in 0..self.block_size {
                        for x in 0..self.block_size {
                            let index_final = by * block_width_count + bx;
                            let index_result_block = y * self.block_size + x;
                            let index_result = ((by * self.block_size) + y) * my_image.mwidth
                                + ((bx * self.block_size) + x);

                            result[i][index_result] =
                                final_result_block[index_final][i][index_result_block];
                        }
                    }
                }
            }
        }

        my_image.image_converted = result;
    }
}

#[derive(Clone)]
pub struct JpegSteps {
    pub dct_table: Option<Arc<Vec2d<f32>>>,
    pub alpha_table: Option<Arc<Vec<f32>>>,

    pub mwidth: usize,

    pub block_size: usize,
    pub quality_start: f32,
    pub block_size_index: usize,

    pub use_fast_dct: bool,
    pub use_gen_qtable: bool,
    pub use_compression_rate: bool,

    pub q_control: f32,
    pub two_block_size: f32,
}

impl JpegSteps {
    pub fn new(jpeg: &Jpeg, mwidth: usize) -> JpegSteps {
        JpegSteps {
            dct_table: None,
            alpha_table: None,

            mwidth,

            block_size: jpeg.block_size,
            quality_start: jpeg.quality_start,
            block_size_index: jpeg.block_size_index,

            use_fast_dct: jpeg.use_fast_dct,
            use_gen_qtable: jpeg.use_gen_qtable,
            use_compression_rate: jpeg.use_compression_rate,

            q_control: 100.0f32 - jpeg.quality_start as f32,
            two_block_size: 2.0f32 / jpeg.block_size as f32,
        }
    }
    pub fn dct_function(&self, image_block: &[f32]) -> Vec<f32> {
        let mut dct_matrix: Vec<f32> = vec![0.0f32; self.block_size * self.block_size];
        if self.use_fast_dct {
            FUNCTIONS_FAST_DCT[self.block_size_index](image_block, &mut dct_matrix);
        } else if let Some(dct_table) = &self.dct_table {
            if let Some(alpha_table) = &self.alpha_table {
                for v in 0..self.block_size {
                    let vblock = v * self.block_size;

                    for u in 0..self.block_size {
                        let ublock = u * self.block_size;

                        let mut sum = 0.0f32;
                        for y in 0..self.block_size {
                            let yv = dct_table[0][vblock + y];

                            for x in 0..self.block_size {
                                let xu = dct_table[0][ublock + x];

                                let index_image = y * self.block_size + x;
                                sum += image_block[index_image] * xu * yv;
                            }
                        }

                        let index_matrix = vblock + u;
                        dct_matrix[index_matrix] =
                            alpha_table[index_matrix] * sum * self.two_block_size;
                    }
                }
            }
        }
        dct_matrix
    }
    pub fn inverse_dct_function(&self, dct_matrix: &[f32]) -> Vec<u8> {
        let mut image_block: Vec<f32> = vec![0.0f32; self.block_size * self.block_size];
        if self.use_fast_dct {
            FUNCTIONS_FAST_IDCT[self.block_size_index](dct_matrix, &mut image_block);
        } else if let Some(dct_table) = &self.dct_table {
            if let Some(alpha_table) = &self.alpha_table {
                for y in 0..self.block_size {
                    let yblock = y * self.block_size;

                    for x in 0..self.block_size {
                        let xblock = x * self.block_size;

                        let mut sum = 0.0f32;
                        for v in 0..self.block_size {
                            let vblock = v * self.block_size;

                            let yv = dct_table[1][yblock + v];

                            for u in 0..self.block_size {
                                let xu = dct_table[1][xblock + u];

                                let index_matrix = vblock + u;
                                sum +=
                                    alpha_table[index_matrix] * dct_matrix[index_matrix] * xu * yv;
                            }
                        }

                        let index_image = y * self.block_size + x;
                        image_block[index_image] = sum * self.two_block_size;
                    }
                }
            }
        }
        let mut result_block: Vec<u8> = vec![0u8; self.block_size * self.block_size];
        for y in 0..self.block_size {
            for x in 0..self.block_size {
                let index = y * self.block_size + x;
                result_block[index] = my_image::min_max_color(image_block[index] + 128.0f32);
            }
        }
        result_block
    }
    fn quantize_value(
        &self,
        x: f32,
        index: usize,
        q_matrix: &[f32],
        use_compression_rate: bool,
    ) -> f32 {
        if use_compression_rate {
            let mut factor = self.quality_start + (x / self.mwidth as f32) * self.q_control;

            factor = if factor >= 50.0f32 {
                200.0f32 - factor * 2.0f32
            } else {
                5000.0f32 / factor
            };

            let mut q_value = (q_matrix[index] * factor) / 4.0f32;
            if q_value <= 0.0f32 {
                q_value = 1.0f32;
            }
            q_value
        } else {
            q_matrix[index]
        }
    }
    pub fn quantize_function(
        &self,
        start_x: usize,
        q_matrix: &[f32],
        dct_matrix: &mut [f32],
        use_compression_rate: bool,
    ) {
        for y in 0..self.block_size {
            for x in 0..self.block_size {
                let index = y * self.block_size + x;
                let q_matrix_value = self.quantize_value(
                    (start_x + x) as f32,
                    index,
                    q_matrix,
                    use_compression_rate,
                );
                dct_matrix[index] = (dct_matrix[index] / q_matrix_value).round();
            }
        }
    }
    pub fn de_quantize_function(
        &self,
        start_x: usize,
        q_matrix: &[f32],
        dct_matrix: &mut [f32],
        use_compression_rate: bool,
    ) {
        for y in 0..self.block_size {
            for x in 0..self.block_size {
                let index = y * self.block_size + x;
                let q_matrix_value = self.quantize_value(
                    (start_x + x) as f32,
                    index,
                    q_matrix,
                    use_compression_rate,
                );
                dct_matrix[index] *= q_matrix_value;
            }
        }
    }
    pub fn jpeg_steps(&self, start_x: usize, image_block: &[f32], q_matrix: &[f32]) -> Vec<u8> {
        let mut dct_matrix = self.dct_function(image_block);
        self.quantize_function(
            start_x,
            q_matrix,
            &mut dct_matrix,
            self.use_compression_rate,
        );
        self.de_quantize_function(
            start_x,
            q_matrix,
            &mut dct_matrix,
            self.use_compression_rate,
        );
        self.inverse_dct_function(&dct_matrix)
    }
}

pub fn generate_q_matrix(
    q_matrix_base: &[f32],
    block_size: usize,
    use_gen_qtable: bool,
) -> Vec<f32> {
    let mut q_matrix: Vec<f32> = vec![0.0f32; block_size * block_size];
    if use_gen_qtable {
        for y in 0..block_size {
            for x in 0..block_size {
                q_matrix[y * block_size + x] = (x + y + 1) as f32;
            }
        }
    } else {
        for y in 0..block_size {
            for x in 0..block_size {
                q_matrix[y * block_size + x] = q_matrix_base[(y % 8) * 8 + (x % 8)];
            }
        }
    }
    q_matrix
}

pub fn apply_q_matrix_factor(q_matrix: &mut [f32], block_size: usize, factor: f32) {
    for y in 0..block_size {
        for x in 0..block_size {
            let index = y * block_size + x;
            let mut q_value = ((q_matrix[index] * factor) + 50.0f32) / 100.0f32;
            if q_value <= 0.0f32 {
                q_value = 1.0f32;
            }
            q_matrix[index] = q_value;
        }
    }
}

pub fn generate_dct_table(block_size: usize) -> Vec2d<f32> {
    let mut dct_table: Vec2d<f32> = vec![vec![0.0f32; block_size * block_size]; 2];
    for y in 0..block_size {
        for x in 0..block_size {
            let cos_calc =
                (((2 * y + 1) as f32 * x as f32 * f32::consts::PI) / (2 * block_size) as f32).cos();
            dct_table[0][x * block_size + y] = cos_calc;
            dct_table[1][y * block_size + x] = cos_calc;
        }
    }
    dct_table
}

pub fn generate_alpha_table(block_size: usize) -> Vec<f32> {
    let mut alpha_table: Vec<f32> = vec![0.0f32; block_size * block_size];
    for y in 0..block_size {
        for x in 0..block_size {
            alpha_table[y * block_size + x] = if y == 0 {
                f32::consts::FRAC_1_SQRT_2
            } else {
                1.0f32
            } * if x == 0 {
                f32::consts::FRAC_1_SQRT_2
            } else {
                1.0f32
            };
        }
    }
    alpha_table
}
