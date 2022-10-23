use crate::my_image;
use std::{cell::RefCell, rc::Rc};

fn weighted_average(histogram: &Vec<i32>) -> (u8, f32) {
    let mut value = 0i32;
    let mut error = 0.0f32;

    let mut total = 0i32;
    for hist in histogram {
        total += hist;
    }
    if total > 0 {
        for i in 0..histogram.len() {
            value += i as i32 * histogram[i];
        }
        value /= total;

        for i in 0..histogram.len() {
            error += (histogram[i] * (value - i as i32) * (value - i as i32)) as f32;
        }
        error = (error / total as f32).sqrt();
    }

    (value as u8, error)
}

fn color_from_histogram(histogram: &[Vec<i32>]) -> (f32, [u8; 3]) {
    let (r, r_error) = weighted_average(&histogram[0]);
    let (g, g_error) = weighted_average(&histogram[1]);
    let (b, b_error) = weighted_average(&histogram[2]);
    let error_calc = (0.299f32 * r_error) + (0.587f32 * g_error) + (0.114f32 * b_error);
    (error_calc, [r, g, b])
}

pub struct QuadNode {
    pub image: Rc<Vec<u8>>,

    pub rgb: [u8; 3],
    pub error: f32,
    pub depth: u32,
    pub boxl: usize,
    pub boxt: usize,
    pub boxr: usize,
    pub boxb: usize,

    pub width_block_size: usize,
    pub height_block_size: usize,

    pub children_tl: Option<Rc<RefCell<QuadNode>>>,
    pub children_tr: Option<Rc<RefCell<QuadNode>>>,
    pub children_bl: Option<Rc<RefCell<QuadNode>>>,
    pub children_br: Option<Rc<RefCell<QuadNode>>>,
}

impl QuadNode {
    pub fn new(
        image: Rc<Vec<u8>>,
        width: usize,
        height: usize,
        boxl: usize,
        boxt: usize,
        boxr: usize,
        boxb: usize,
        depth: u32,
    ) -> Option<Rc<RefCell<QuadNode>>> {
        if boxl < width && boxt < height {
            let boxr_limited = if boxr >= width { width - 1 } else { boxr };
            let boxb_limited = if boxb >= height { height - 1 } else { boxb };

            let mut histogram = vec![vec![0i32; 256]; 3];

            for y in boxt..boxb_limited {
                for x in boxl..boxr_limited {
                    let index = (y * width + x) * 3;
                    histogram[0][image[index] as usize] += 1;
                    histogram[1][image[index + 1] as usize] += 1;
                    histogram[2][image[index + 2] as usize] += 1;
                }
            }

            let (error, rgb) = color_from_histogram(&histogram);

            Some(Rc::new(RefCell::new(QuadNode {
                image,

                rgb,
                error,
                depth,

                boxl,
                boxt,
                boxr,
                boxb,
                width_block_size: boxr - boxl,
                height_block_size: boxb - boxt,

                children_tl: None,
                children_tr: None,
                children_bl: None,
                children_br: None,
            })))
        } else {
            None
        }
    }
}

pub fn build_tree(
    node: &mut Option<Rc<RefCell<QuadNode>>>,
    list: &mut Vec<Rc<RefCell<QuadNode>>>,
    width: usize,
    height: usize,
    max_depth: u32,
    threshold_error: f32,
    min_size: usize,
    max_size: usize,
) {
    if let Some(node_) = node {
        let mut node = node_.borrow_mut();
        if (node.width_block_size > max_size && node.height_block_size > max_size)
            || ((node.depth <= max_depth)
                && (node.error >= threshold_error)
                && (node.width_block_size > min_size && node.height_block_size > min_size))
        {
            let lr = node.boxl + (node.width_block_size / 2);
            let tb = node.boxt + (node.height_block_size / 2);

            node.children_tl = QuadNode::new(
                Rc::clone(&node.image),
                width,
                height,
                node.boxl,
                node.boxt,
                lr,
                tb,
                node.depth + 1,
            );
            node.children_tr = QuadNode::new(
                Rc::clone(&node.image),
                width,
                height,
                lr,
                node.boxt,
                node.boxr,
                tb,
                node.depth + 1,
            );
            node.children_bl = QuadNode::new(
                Rc::clone(&node.image),
                width,
                height,
                node.boxl,
                tb,
                lr,
                node.boxb,
                node.depth + 1,
            );
            node.children_br = QuadNode::new(
                Rc::clone(&node.image),
                width,
                height,
                lr,
                tb,
                node.boxr,
                node.boxb,
                node.depth + 1,
            );

            build_tree(
                &mut node.children_tl,
                list,
                width,
                height,
                max_depth,
                threshold_error,
                min_size,
                max_size,
            );
            build_tree(
                &mut node.children_tr,
                list,
                width,
                height,
                max_depth,
                threshold_error,
                min_size,
                max_size,
            );
            build_tree(
                &mut node.children_bl,
                list,
                width,
                height,
                max_depth,
                threshold_error,
                min_size,
                max_size,
            );
            build_tree(
                &mut node.children_br,
                list,
                width,
                height,
                max_depth,
                threshold_error,
                min_size,
                max_size,
            );
        } else {
            list.push(Rc::clone(node_));
        }
    }
}

pub fn render_quad_tree(
    my_image: &mut my_image::MyImage,
    use_ycbcr: bool,
    min_size: usize,
    max_size: usize,
    max_depth: u32,
    use_draw_line: bool,
    threshold_error: f32,
    subsampling_index: usize,
    use_quad_tree_pow_2: bool,
) -> Vec<Rc<RefCell<QuadNode>>> {
    my_image.mwidth = my_image.width;
    my_image.mheight = my_image.height;

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

    let mut root_quad = match use_quad_tree_pow_2 {
        true => {
            let square_size = if my_image.width > my_image.height {
                my_image.width.next_power_of_two()
            } else {
                my_image.height.next_power_of_two()
            };
            QuadNode::new(
                Rc::new(my_image.final_image.to_vec()),
                my_image.width,
                my_image.height,
                0,
                0,
                square_size,
                square_size,
                0,
            )
        }
        false => QuadNode::new(
            Rc::new(my_image.final_image.to_vec()),
            my_image.width,
            my_image.height,
            0,
            0,
            my_image.width,
            my_image.height,
            0,
        ),
    };

    let mut list_quad: Vec<Rc<RefCell<QuadNode>>> = vec![];
    build_tree(
        &mut root_quad,
        &mut list_quad,
        my_image.width,
        my_image.height,
        max_depth,
        threshold_error,
        min_size,
        max_size,
    );

    my_image.final_image = vec![0u8; my_image.width * my_image.height * 3];

    for quad in &list_quad {
        let quad = quad.borrow();
        let quad_boxr = if quad.boxr > my_image.width {
            my_image.width
        } else {
            quad.boxr
        };
        let quad_boxb = if quad.boxb > my_image.height {
            my_image.height
        } else {
            quad.boxb
        };
        for y in quad.boxt..quad_boxb {
            for x in quad.boxl..quad_boxr {
                let index = (y * my_image.width + x) * 3;
                my_image.final_image[index] = quad.rgb[0];
                my_image.final_image[index + 1] = quad.rgb[1];
                my_image.final_image[index + 2] = quad.rgb[2];
            }
        }
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
            draw_rect(
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

pub fn draw_rect(image: &mut [u8], width: usize, x1: usize, y1: usize, x2: usize, y2: usize) {
    draw_line(image, width, x1, y1, x2, y1);
    draw_line(image, width, x1, y2, x2, y2);
    draw_line(image, width, x1, y1, x1, y2);
    draw_line(image, width, x2, y1, x2, y2);
}

fn draw_line(image: &mut [u8], width: usize, x1: usize, y1: usize, x2: usize, y2: usize) {
    if x2 - x1 == 0 {
        for y in y1..=y2 {
            let index = (y * width + x1) * 3;
            image[index] = 128;
            image[index + 1] = 128;
            image[index + 2] = 128;
        }
    } else if y2 - y1 == 0 {
        for x in x1..=x2 {
            let index = (y1 * width + x) * 3;
            image[index] = 128;
            image[index + 1] = 128;
            image[index + 2] = 128;
        }
    }
}
