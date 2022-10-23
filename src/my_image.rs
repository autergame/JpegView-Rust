use std::os::raw::c_void;

use gl::types::{GLfloat, GLint, GLsizei, GLuint};

pub struct MyImage {
    pub final_image: Vec<u8>,
    pub original_image: Vec<u8>,
    pub image_converted: Vec<Vec<u8>>,

    pub width: usize,
    pub height: usize,
    pub mwidth: usize,
    pub mheight: usize,
}

impl MyImage {
    pub fn new(original: Vec<u8>, width: usize, height: usize) -> MyImage {
        MyImage {
            final_image: vec![],
            image_converted: vec![],
            original_image: original,

            width,
            height,
            mwidth: width,
            mheight: height,
        }
    }
    pub fn image_to_ycbcr(&mut self) {
        self.image_converted = vec![vec![0u8; self.mheight * self.mwidth]; 3];
        for y in 0..self.height {
            for x in 0..self.width {
                let index_ycbcr = y * self.mwidth + x;
                let index_original = (y * self.width + x) * 3;

                let r = self.original_image[index_original + 0] as f32;
                let g = self.original_image[index_original + 1] as f32;
                let b = self.original_image[index_original + 2] as f32;

                self.image_converted[0][index_ycbcr] =
                    min_max_color((0.299f32 * r) + (0.587f32 * g) + (0.114f32 * b));
                self.image_converted[1][index_ycbcr] =
                    min_max_color(((-0.168f32 * r) + (-0.331f32 * g) + (0.500f32 * b)) + 128.0f32);
                self.image_converted[2][index_ycbcr] =
                    min_max_color(((0.500f32 * r) + (-0.418f32 * g) + (-0.081f32 * b)) + 128.0f32);
            }
        }
    }
    pub fn ycbcr_to_image(&mut self) {
        self.final_image = vec![0u8; self.height * self.width * 3];
        for y in 0..self.height {
            for x in 0..self.width {
                let index_ycbcr = y * self.mwidth + x;
                let index_result = (y * self.width + x) * 3;

                let y = self.image_converted[0][index_ycbcr] as f32;
                let cb = self.image_converted[1][index_ycbcr] as f32 - 128.0f32;
                let cr = self.image_converted[2][index_ycbcr] as f32 - 128.0f32;

                self.final_image[index_result + 0] = min_max_color(y + (1.402f32 * cr));
                self.final_image[index_result + 1] =
                    min_max_color(y + (-0.344f32 * cb) + (-0.714f32 * cr));
                self.final_image[index_result + 2] = min_max_color(y + (1.772f32 * cb));
            }
        }
    }
    pub fn image_to_rgb(&mut self) {
        self.image_converted = vec![vec![0u8; self.mheight * self.mwidth]; 3];
        for y in 0..self.height {
            for x in 0..self.width {
                let index_rgb = y * self.mwidth + x;
                let index_original = (y * self.width + x) * 3;

                self.image_converted[0][index_rgb] = self.original_image[index_original + 0];
                self.image_converted[1][index_rgb] = self.original_image[index_original + 1];
                self.image_converted[2][index_rgb] = self.original_image[index_original + 2];
            }
        }
    }
    pub fn rgb_to_image(&mut self) {
        self.final_image = vec![0u8; self.height * self.width * 3];
        for y in 0..self.height {
            for x in 0..self.width {
                let index_rgb = y * self.mwidth + x;
                let index_result = (y * self.width + x) * 3;

                self.final_image[index_result + 0] = self.image_converted[0][index_rgb];
                self.final_image[index_result + 1] = self.image_converted[1][index_rgb];
                self.final_image[index_result + 2] = self.image_converted[2][index_rgb];
            }
        }
    }
    pub fn subsampling(&mut self, use_ycbcr: bool, subsampling_index: usize) {
        let start_comp = if use_ycbcr { 0 } else { 0 };
        match subsampling_index {
            0 => {} // 4:4:4
            1 =>
            // 4:4:0
            {
                for i in start_comp..3 {
                    for y in (0..self.height).step_by(2) {
                        for x in (0..self.width).step_by(4) {
                            let index = y * self.mwidth + x;
                            let index_two = (y + 1) * self.mwidth + x;
                            self.image_converted[i][index_two + 0] =
                                self.image_converted[i][index + 0];
                            self.image_converted[i][index_two + 1] =
                                self.image_converted[i][index + 1];
                            self.image_converted[i][index_two + 2] =
                                self.image_converted[i][index + 2];
                            self.image_converted[i][index_two + 3] =
                                self.image_converted[i][index + 3];
                        }
                    }
                }
            }
            2 =>
            // 4:2:2
            {
                for i in start_comp..3 {
                    for y in (0..self.height).step_by(2) {
                        for x in (0..self.width).step_by(4) {
                            let index = y * self.mwidth + x;
                            self.image_converted[i][index + 1] = self.image_converted[i][index + 0];
                            self.image_converted[i][index + 3] = self.image_converted[i][index + 2];

                            let index_two = (y + 1) * self.mwidth + x;
                            self.image_converted[i][index_two + 1] =
                                self.image_converted[i][index_two + 0];
                            self.image_converted[i][index_two + 3] =
                                self.image_converted[i][index_two + 2];
                        }
                    }
                }
            }
            3 =>
            // 4:2:0
            {
                for i in start_comp..3 {
                    for y in (0..self.height).step_by(2) {
                        for x in (0..self.width).step_by(4) {
                            let index = y * self.mwidth + x;
                            self.image_converted[i][index + 1] = self.image_converted[i][index + 0];
                            self.image_converted[i][index + 3] = self.image_converted[i][index + 2];

                            let index_two = (y + 1) * self.mwidth + x;
                            self.image_converted[i][index_two + 0] =
                                self.image_converted[i][index + 0];
                            self.image_converted[i][index_two + 1] =
                                self.image_converted[i][index + 0];
                            self.image_converted[i][index_two + 2] =
                                self.image_converted[i][index + 2];
                            self.image_converted[i][index_two + 3] =
                                self.image_converted[i][index + 2];
                        }
                    }
                }
            }
            4 =>
            // 4:1:1
            {
                for i in start_comp..3 {
                    for y in (0..self.height).step_by(2) {
                        for x in (0..self.width).step_by(4) {
                            let index = y * self.mwidth + x;
                            self.image_converted[i][index + 1] = self.image_converted[i][index];
                            self.image_converted[i][index + 2] = self.image_converted[i][index];
                            self.image_converted[i][index + 3] = self.image_converted[i][index];

                            let index_two = (y + 1) * self.mwidth + x;
                            self.image_converted[i][index_two + 1] =
                                self.image_converted[i][index_two];
                            self.image_converted[i][index_two + 2] =
                                self.image_converted[i][index_two];
                            self.image_converted[i][index_two + 3] =
                                self.image_converted[i][index_two];
                        }
                    }
                }
            }
            5 =>
            // 4:1:0
            {
                for i in start_comp..3 {
                    for y in (0..self.height).step_by(2) {
                        for x in (0..self.width).step_by(4) {
                            let index = y * self.mwidth + x;
                            self.image_converted[i][index + 1] = self.image_converted[i][index];
                            self.image_converted[i][index + 2] = self.image_converted[i][index];
                            self.image_converted[i][index + 3] = self.image_converted[i][index];

                            let index_two = (y + 1) * self.mwidth + x;
                            self.image_converted[i][index_two + 0] = self.image_converted[i][index];
                            self.image_converted[i][index_two + 1] = self.image_converted[i][index];
                            self.image_converted[i][index_two + 2] = self.image_converted[i][index];
                            self.image_converted[i][index_two + 3] = self.image_converted[i][index];
                        }
                    }
                }
            }
            _ => {}
        }
    }
    pub fn fill_outbound(&mut self) {
        for y in 0..self.mheight {
            for x in self.width..self.mwidth {
                let index_result = y * self.mwidth + x;
                self.image_converted[0][index_result] = 0x80;
                self.image_converted[1][index_result] = 0x80;
                self.image_converted[2][index_result] = 0x80;
            }
        }
        for y in self.height..self.mheight {
            for x in 0..self.mwidth {
                let index_result = y * self.mwidth + x;
                self.image_converted[0][index_result] = 0x80;
                self.image_converted[1][index_result] = 0x80;
                self.image_converted[2][index_result] = 0x80;
            }
        }
    }
    pub fn create_opengl_image(&self, use_final: bool, use_linear: bool) -> GLuint {
        let color: [GLfloat; 4] = [0.2f32, 0.2f32, 0.2f32, 1.0f32];
        let mut image_texture: GLuint = 0;
        unsafe {
            gl::GenTextures(1, &mut image_texture);
            gl::BindTexture(gl::TEXTURE_2D, image_texture);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            match use_linear {
                true => {
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as GLint);
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as GLint);
                }
                false => {
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as GLint);
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as GLint);
                }
            }
            gl::TexParameteri(
                gl::TEXTURE_2D,
                gl::TEXTURE_WRAP_S,
                gl::CLAMP_TO_BORDER as GLint,
            );
            gl::TexParameteri(
                gl::TEXTURE_2D,
                gl::TEXTURE_WRAP_T,
                gl::CLAMP_TO_BORDER as GLint,
            );
            gl::TexParameterfv(
                gl::TEXTURE_2D,
                gl::TEXTURE_BORDER_COLOR,
                color.as_ptr() as *const f32,
            );
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB as GLint,
                self.width as GLsizei,
                self.height as GLsizei,
                0,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                match use_final {
                    true => self.final_image.as_ptr(),
                    false => self.original_image.as_ptr(),
                } as *const c_void,
            );
        }
        image_texture
    }
    pub fn update_opengl_image(&self, image_texture: GLuint, use_final: bool) {
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, image_texture);
            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                0,
                0,
                self.width as i32,
                self.height as i32,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                match use_final {
                    true => self.final_image.as_ptr(),
                    false => self.original_image.as_ptr(),
                } as *const c_void,
            );
        }
    }
}

pub fn min_max_color(color: f32) -> u8 {
    if color > 255.0f32 {
        255
    } else if color < 0.0f32 {
        0
    } else {
        color as u8
    }
}
