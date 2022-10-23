use crate::my_image;
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

    pub quality: u32,
    pub quality_start: u32,

    pub use_gen_qtable: bool,
    pub use_threads: bool,
    pub use_fast_dct: bool,
    pub use_compression_rate: bool,
}

impl Jpeg {
    pub fn new(
        my_image: &mut my_image::MyImage,
        block_size: usize,
        quality: u32,
        quality_start: u32,
        block_size_index: usize,
        subsampling_index: usize,
        use_gen_qtable: bool,
        use_ycbcr: bool,
        use_threads: bool,
        use_fast_dct: bool,
        use_compression_rate: bool,
    ) -> Jpeg {
        my_image.mwidth = round_up_block_size(my_image.width, block_size);
        my_image.mheight = round_up_block_size(my_image.height, block_size);

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

        Jpeg {
            block_size,
            block_size_index,
            quality,
            quality_start,
            use_gen_qtable,
            use_threads,
            use_fast_dct,
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

        my_image.mwidth = round_up_block_size(my_image.width, self.block_size);
        my_image.mheight = round_up_block_size(my_image.height, self.block_size);

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

        let mut q_matrix_luma =
            generate_q_matrix(&Q_MATRIX_LUMA_CONST, self.block_size, self.use_gen_qtable);
        let mut q_matrix_chroma =
            generate_q_matrix(&Q_MATRIX_CHROMA_CONST, self.block_size, self.use_gen_qtable);

        if !self.use_compression_rate {
            let factor = match self.use_gen_qtable {
                true => {
                    if self.quality >= 50 {
                        200.0f32 - (self.quality as f32 * 2.0f32)
                    } else {
                        5000.0f32 / self.quality as f32
                    }
                }
                false => 25.0f32 * ((101.0f32 - self.quality as f32) * 0.01f32),
            };
            apply_q_matrix_factor(&mut q_matrix_luma, self.block_size, factor);
            apply_q_matrix_factor(&mut q_matrix_chroma, self.block_size, factor);
        }

        self.encode(my_image, q_matrix_luma, q_matrix_chroma);

        match use_ycbcr {
            true => my_image.ycbcr_to_image(),
            false => my_image.rgb_to_image(),
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
            jpeg_steps.dct_table = generate_dct_table(self.block_size);
            jpeg_steps.alpha_table = generate_alpha_table(self.block_size);
        }
        jpeg_steps.zig_zag_table = generate_zig_zag_table(self.block_size);

        if self.use_threads {
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

            let cpu_threads = thread::available_parallelism().unwrap().get();
            let pool = threadpool::ThreadPool::with_name("worker".to_string(), cpu_threads);

            for by in (0..my_image.mheight).step_by(self.block_size) {
                for bx in (0..my_image.mwidth).step_by(self.block_size) {
                    let arc_result = Arc::clone(&result);
                    let arc_jpeg_steps = Arc::clone(&jpeg_steps);
                    let arc_q_matrix_luma = Arc::clone(&q_matrix_luma);
                    let arc_q_matrix_chroma = Arc::clone(&q_matrix_chroma);
                    let arc_image_converted = Arc::clone(&image_converted);

                    pool.execute(move || {
                        let result = &mut *arc_result.lock().expect("Could not lock result");
                        let jpeg_steps =
                            &mut *arc_jpeg_steps.lock().expect("Could not lock jpeg_steps");
                        let q_matrix_luma = &*arc_q_matrix_luma;
                        let q_matrix_chroma = &*arc_q_matrix_chroma;
                        let image_converted = &*arc_image_converted;

                        jpeg_steps.start_x = bx;
                        jpeg_steps.start_y = by;

                        jpeg_steps.jpeg_steps(&mut result[0], &image_converted[0], q_matrix_luma);
                        jpeg_steps.jpeg_steps(&mut result[1], &image_converted[1], q_matrix_chroma);
                        jpeg_steps.jpeg_steps(&mut result[2], &image_converted[2], q_matrix_chroma);
                    });
                }
            }

            pool.join();
            my_image.image_converted = (*result.lock().expect("Could not lock result")).to_vec();
        } else {
            let mut result = vec![vec![0u8; my_image.mheight * my_image.mwidth]; 3];

            for by in (0..my_image.mheight).step_by(self.block_size) {
                for bx in (0..my_image.mwidth).step_by(self.block_size) {
                    jpeg_steps.start_x = bx;
                    jpeg_steps.start_y = by;

                    jpeg_steps.jpeg_steps(
                        &mut result[0],
                        &my_image.image_converted[0],
                        &q_matrix_luma,
                    );
                    jpeg_steps.jpeg_steps(
                        &mut result[1],
                        &my_image.image_converted[1],
                        &q_matrix_chroma,
                    );
                    jpeg_steps.jpeg_steps(
                        &mut result[2],
                        &my_image.image_converted[2],
                        &q_matrix_chroma,
                    );
                }
            }

            my_image.image_converted = result;
        }
    }
}

pub struct JpegSteps {
    pub dct_table: Vec<Vec<f32>>,
    pub alpha_table: Vec<f32>,
    pub zig_zag_table: Vec<usize>,

    pub dct_matrix: Vec<f32>,
    pub image_block: Vec<f32>,

    pub mwidth: usize,

    pub start_x: usize,
    pub start_y: usize,

    pub block_size: usize,
    pub quality_start: u32,
    pub block_size_index: usize,

    pub use_gen_qtable: bool,
    pub use_fast_dct: bool,
    pub use_compression_rate: bool,

    pub q_control: f32,
    pub two_block_size: f32,
}

impl JpegSteps {
    pub fn new(jpeg: &Jpeg, mwidth: usize) -> JpegSteps {
        JpegSteps {
            dct_table: vec![],
            alpha_table: vec![],
            zig_zag_table: vec![],

            dct_matrix: vec![0.0f32; jpeg.block_size * jpeg.block_size],
            image_block: vec![0.0f32; jpeg.block_size * jpeg.block_size],

            mwidth,

            start_x: 0,
            start_y: 0,

            block_size: jpeg.block_size,
            quality_start: jpeg.quality_start,
            block_size_index: jpeg.block_size_index,

            use_gen_qtable: jpeg.use_gen_qtable,
            use_fast_dct: jpeg.use_fast_dct,
            use_compression_rate: jpeg.use_compression_rate,

            q_control: 100.0f32 - jpeg.quality_start as f32,
            two_block_size: 2.0f32 / jpeg.block_size as f32,
        }
    }
    pub fn zig_zag_function(&self) -> Vec<i32> {
        let mut dct_matrix_zig_zag = vec![0i32; self.block_size * self.block_size];
        for y in 0..self.block_size {
            for x in 0..self.block_size {
                let index = y * self.block_size + x;
                dct_matrix_zig_zag[self.zig_zag_table[index]] = self.dct_matrix[index] as i32;
            }
        }
        dct_matrix_zig_zag
    }
    pub fn un_zig_zag_function(&mut self, dct_matrix_zig_zag: &[i32]) {
        for y in 0..self.block_size {
            for x in 0..self.block_size {
                let index = y * self.block_size + x;
                self.dct_matrix[index] = dct_matrix_zig_zag[self.zig_zag_table[index]] as f32;
            }
        }
    }
    pub fn dct_function(&mut self, image_converted: &[u8]) {
        for y in 0..self.block_size {
            for x in 0..self.block_size {
                let index_image = y * self.block_size + x;
                let index_image_converted = (self.start_y + y) * self.mwidth + (self.start_x + x);
                self.image_block[index_image] =
                    image_converted[index_image_converted] as f32 - 128.0f32;
            }
        }
        match self.use_fast_dct {
            true => {
                FUNCTIONS_FAST_DCT[self.block_size_index](&self.image_block, &mut self.dct_matrix)
            }
            false => {
                for v in 0..self.block_size {
                    let vblock = v * self.block_size;

                    for u in 0..self.block_size {
                        let ublock = u * self.block_size;

                        let mut sum = 0.0f32;
                        for y in 0..self.block_size {
                            let yv = self.dct_table[0][vblock + y];

                            for x in 0..self.block_size {
                                let xu = self.dct_table[0][ublock + x];

                                let index_image = y * self.block_size + x;
                                sum += self.image_block[index_image] * xu * yv;
                            }
                        }

                        let index_matrix = vblock + u;
                        self.dct_matrix[index_matrix] =
                            self.alpha_table[index_matrix] * sum * self.two_block_size;
                    }
                }
            }
        }
    }
    pub fn inverse_dct_function(&mut self, result: &mut [u8]) {
        match self.use_fast_dct {
            true => {
                FUNCTIONS_FAST_IDCT[self.block_size_index](&self.dct_matrix, &mut self.image_block)
            }
            false => {
                for y in 0..self.block_size {
                    let yblock = y * self.block_size;

                    for x in 0..self.block_size {
                        let xblock = x * self.block_size;

                        let mut sum = 0.0f32;
                        for v in 0..self.block_size {
                            let vblock = v * self.block_size;

                            let yv = self.dct_table[1][yblock + v];

                            for u in 0..self.block_size {
                                let xu = self.dct_table[1][xblock + u];

                                let index_matrix = vblock + u;
                                sum += self.alpha_table[index_matrix]
                                    * self.dct_matrix[index_matrix]
                                    * xu
                                    * yv;
                            }
                        }

                        let index_image = y * self.block_size + x;
                        self.image_block[index_image] = sum * self.two_block_size;
                    }
                }
            }
        }
        for y in 0..self.block_size {
            for x in 0..self.block_size {
                let index_image = y * self.block_size + x;
                let index_result = (self.start_y + y) * self.mwidth + (self.start_x + x);
                result[index_result] =
                    my_image::min_max_color(self.image_block[index_image] as f32 + 128.0f32);
            }
        }
    }
    fn quantize_value(&mut self, x: usize, index: usize, q_matrix: &[f32]) -> f32 {
        match self.use_compression_rate {
            true => {
                let mut factor = self.quality_start as f32
                    + ((self.start_x + x) as f32 / self.mwidth as f32) * self.q_control;

                factor = match self.use_gen_qtable {
                    true => {
                        if factor >= 50.0f32 {
                            200.0f32 - factor * 2.0f32
                        } else {
                            5000.0f32 / factor
                        }
                    }
                    false => 25.0f32 * ((101.0f32 - factor) * 0.01f32),
                };

                1.0f32 + (q_matrix[index] - 1.0f32) * factor
            }
            false => q_matrix[index],
        }
    }
    pub fn quantize_function(&mut self, q_matrix: &[f32]) {
        for y in 0..self.block_size {
            for x in 0..self.block_size {
                let index = y * self.block_size + x;
                let q_matrix_value = self.quantize_value(x, index, q_matrix);
                self.dct_matrix[index] = (self.dct_matrix[index] / q_matrix_value).round();
            }
        }
    }
    pub fn de_quantize_function(&mut self, q_matrix: &[f32]) {
        for y in 0..self.block_size {
            for x in 0..self.block_size {
                let index = y * self.block_size + x;
                let q_matrix_value = self.quantize_value(x, index, q_matrix);
                self.dct_matrix[index] = self.dct_matrix[index] * q_matrix_value;
            }
        }
    }
    pub fn jpeg_steps(&mut self, result: &mut [u8], image_converted: &[u8], q_matrix: &[f32]) {
        self.dct_function(image_converted);
        self.quantize_function(q_matrix);
        self.de_quantize_function(q_matrix);
        self.inverse_dct_function(result);
    }
}

pub fn generate_q_matrix(
    q_matrix_base: &[f32],
    block_size: usize,
    use_gen_qtable: bool,
) -> Vec<f32> {
    let mut q_matrix = vec![0.0f32; block_size * block_size];
    match use_gen_qtable {
        true => {
            for y in 0..block_size {
                for x in 0..block_size {
                    q_matrix[y * block_size + x] = (x + y + 1) as f32;
                }
            }
        }
        false => {
            for y in 0..block_size {
                for x in 0..block_size {
                    q_matrix[y * block_size + x] = q_matrix_base[(y % 8) * 8 + (x % 8)];
                }
            }
        }
    }
    q_matrix
}

pub fn apply_q_matrix_factor(q_matrix: &mut [f32], block_size: usize, factor: f32) {
    for y in 0..block_size {
        for x in 0..block_size {
            q_matrix[y * block_size + x] =
                1.0f32 + (q_matrix[y * block_size + x] - 1.0f32) * factor;
        }
    }
}

pub fn generate_dct_table(block_size: usize) -> Vec<Vec<f32>> {
    let mut dct_table = vec![vec![0.0f32; block_size * block_size]; 2];
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
    let mut alpha_table = vec![0.0f32; block_size * block_size];
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

pub fn round_up_block_size(x: usize, block_size: usize) -> usize {
    let y = x + (block_size - 1);
    y - (y % block_size)
}
