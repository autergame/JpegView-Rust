#![allow(clippy::needless_range_loop)]

use crate::{my_image, Vec2d};
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

fn color_from_histogram(histogram: &Vec2d<i32>) -> (f32, [u8; 3]) {
    let (r, r_error) = weighted_average(&histogram[0]);
    let (g, g_error) = weighted_average(&histogram[1]);
    let (b, b_error) = weighted_average(&histogram[2]);
    let error_calc = (0.299f32 * r_error) + (0.587f32 * g_error) + (0.114f32 * b_error);
    (error_calc, [r, g, b])
}

pub struct QuadTree {
    pub max_depth: u32,

    pub min_size: usize,
    pub max_size: usize,

    pub use_pow_2: bool,
    pub use_draw_line: bool,

    pub threshold_error: f32,
}

impl QuadTree {
    pub fn new(
        max_depth: u32,
        min_size: usize,
        max_size: usize,
        use_pow_2: bool,
        use_draw_line: bool,
        threshold_error: f32,
    ) -> QuadTree {
        QuadTree {
            max_depth,

            min_size,
            max_size,

            use_pow_2,
            use_draw_line,

            threshold_error,
        }
    }
}

pub type QuadNodeRef = Rc<RefCell<QuadNode>>;

pub struct QuadNode {
    pub rgb: [u8; 3],
    pub error: f32,
    pub depth: u32,

    pub box_top: usize,
    pub box_left: usize,
    pub box_right: usize,
    pub box_bottom: usize,

    pub width_block_size: usize,
    pub height_block_size: usize,

    pub children_tl: Option<QuadNodeRef>,
    pub children_tr: Option<QuadNodeRef>,
    pub children_bl: Option<QuadNodeRef>,
    pub children_br: Option<QuadNodeRef>,
}

impl QuadNode {
    pub fn new(
        my_image: &my_image::MyImage,
        box_left: usize,
        box_top: usize,
        box_right: usize,
        box_bottom: usize,
        depth: u32,
    ) -> Option<QuadNodeRef> {
        if box_left < my_image.width && box_top < my_image.height {
            let box_right_limited = if box_right >= my_image.width {
                my_image.width - 1
            } else {
                box_right
            };
            let box_bottom_limited = if box_bottom >= my_image.height {
                my_image.height - 1
            } else {
                box_bottom
            };

            let mut histogram: Vec2d<i32> = vec![vec![0i32; 256]; 3];

            for y in box_top..box_bottom_limited {
                for x in box_left..box_right_limited {
                    let index = (y * my_image.width + x) * 3;
                    histogram[0][my_image.final_image[index] as usize] += 1;
                    histogram[1][my_image.final_image[index + 1] as usize] += 1;
                    histogram[2][my_image.final_image[index + 2] as usize] += 1;
                }
            }

            let (error, rgb) = color_from_histogram(&histogram);

            Some(Rc::new(RefCell::new(QuadNode {
                rgb,
                error,
                depth,

                box_top,
                box_left,
                box_right,
                box_bottom,

                width_block_size: box_right - box_left,
                height_block_size: box_bottom - box_top,

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
    quad_tree: &QuadTree,
    quad_node: &mut Option<QuadNodeRef>,
    quad_node_list: &mut Vec<QuadNodeRef>,
    my_image: &my_image::MyImage,
) {
    if let Some(quad_node_) = quad_node {
        let mut quad_node = quad_node_.borrow_mut();

        if (quad_node.width_block_size > quad_tree.max_size
            && quad_node.height_block_size > quad_tree.max_size)
            || ((quad_node.depth <= quad_tree.max_depth)
                && (quad_node.error >= quad_tree.threshold_error)
                && (quad_node.width_block_size > quad_tree.min_size
                    && quad_node.height_block_size > quad_tree.min_size))
        {
            let left_right = quad_node.box_left + (quad_node.width_block_size / 2);
            let top_bottom = quad_node.box_top + (quad_node.height_block_size / 2);

            quad_node.children_tl = QuadNode::new(
                my_image,
                quad_node.box_left,
                quad_node.box_top,
                left_right,
                top_bottom,
                quad_node.depth + 1,
            );
            quad_node.children_tr = QuadNode::new(
                my_image,
                left_right,
                quad_node.box_top,
                quad_node.box_right,
                top_bottom,
                quad_node.depth + 1,
            );
            quad_node.children_bl = QuadNode::new(
                my_image,
                quad_node.box_left,
                top_bottom,
                left_right,
                quad_node.box_bottom,
                quad_node.depth + 1,
            );
            quad_node.children_br = QuadNode::new(
                my_image,
                left_right,
                top_bottom,
                quad_node.box_right,
                quad_node.box_bottom,
                quad_node.depth + 1,
            );

            build_tree(
                quad_tree,
                &mut quad_node.children_tl,
                quad_node_list,
                my_image,
            );
            build_tree(
                quad_tree,
                &mut quad_node.children_tr,
                quad_node_list,
                my_image,
            );
            build_tree(
                quad_tree,
                &mut quad_node.children_bl,
                quad_node_list,
                my_image,
            );
            build_tree(
                quad_tree,
                &mut quad_node.children_br,
                quad_node_list,
                my_image,
            );
        } else {
            quad_node_list.push(Rc::clone(quad_node_));
        }
    }
}

pub fn render_quad_tree(
    quad_tree: &QuadTree,
    my_image: &mut my_image::MyImage,
    use_ycbcr: bool,
    subsampling_index: usize,
) {
    my_image.mwidth = my_image.width;
    my_image.mheight = my_image.height;

    if use_ycbcr {
        my_image.image_to_ycbcr();
        my_image.fill_outbound();
        my_image.sub_sampling(true, subsampling_index);
        my_image.ycbcr_to_image();
    } else {
        my_image.image_to_rgb();
        my_image.fill_outbound();
        my_image.sub_sampling(false, subsampling_index);
        my_image.rgb_to_image();
    }

    let mut quad_root = if quad_tree.use_pow_2 {
        let square_size = if my_image.width > my_image.height {
            my_image.width.next_power_of_two()
        } else {
            my_image.height.next_power_of_two()
        };
        QuadNode::new(my_image, 0, 0, square_size, square_size, 0)
    } else {
        QuadNode::new(my_image, 0, 0, my_image.width, my_image.height, 0)
    };

    let mut quad_node_list: Vec<QuadNodeRef> = Vec::new();
    build_tree(quad_tree, &mut quad_root, &mut quad_node_list, my_image);

    my_image.final_image = vec![0u8; my_image.width * my_image.height * 3];

    for quad in &quad_node_list {
        let quad = quad.borrow();

        let quad_box_right = if quad.box_right > my_image.width {
            my_image.width
        } else {
            quad.box_right
        };

        let quad_box_bottom = if quad.box_bottom > my_image.height {
            my_image.height
        } else {
            quad.box_bottom
        };

        for y in quad.box_top..quad_box_bottom {
            for x in quad.box_left..quad_box_right {
                let index = (y * my_image.width + x) * 3;
                my_image.final_image[index] = quad.rgb[0];
                my_image.final_image[index + 1] = quad.rgb[1];
                my_image.final_image[index + 2] = quad.rgb[2];
            }
        }
    }

    if quad_tree.use_draw_line {
        for quad in &quad_node_list {
            let quad = quad.borrow();

            let quad_box_right = if quad.box_right >= my_image.width {
                my_image.width - 1
            } else {
                quad.box_right
            };

            let quad_box_bottom = if quad.box_bottom >= my_image.height {
                my_image.height - 1
            } else {
                quad.box_bottom
            };

            draw_rect(
                &mut my_image.final_image,
                my_image.width,
                quad.box_left,
                quad.box_top,
                quad_box_right,
                quad_box_bottom,
            );
        }
    }
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
