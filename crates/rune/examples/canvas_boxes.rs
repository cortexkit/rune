//! Compare independently snapped box fills with one canvas, with and without window coordinates.
//! Run with `cargo run --locked -p rune --example canvas_boxes`. No window or GPU is created.
//! Each table compares four byte channels across the full frames at all nine origins.

use rune::render::cpu_boot::{CpuPainter, NeedsGpu, Scene, Target};
use rune::{Color, Cx, El, Env, LineCap, Runtime, Shape, canvas, el};

const FRAME: (u32, u32) = (128, 34);
const STRIP: (f32, f32) = (120.0, 26.0);
const BAR_WIDTH: f32 = 2.0;
const GAP: f32 = 1.5;
const RADIUS: f32 = 1.0;
const TOP: Color = Color::WHITE;
const FOOT: Color = Color {
    r: 0.0,
    g: 0.25,
    b: 1.0,
    a: 1.0,
};

// (Bar height, foot height) in points, from left to right.
const BARS: [(f32, f32); 34] = [
    (23.25, 7.5),
    (11.5, 3.25),
    (6.25, 2.0),
    (19.0, 12.25),
    (3.5, 0.0),
    (0.75, 0.0),
    (14.5, 4.5),
    (4.75, 4.75),
    (21.0, 3.75),
    (18.25, 8.5),
    (8.5, 0.75),
    (2.75, 1.25),
    (15.0, 6.25),
    (25.5, 16.25),
    (6.75, 3.5),
    (17.25, 2.75),
    (1.25, 0.75),
    (13.5, 10.0),
    (5.75, 0.0),
    (11.25, 3.25),
    (4.0, 1.5),
    (22.25, 11.5),
    (9.0, 4.75),
    (12.75, 7.25),
    (7.5, 2.25),
    (0.875, 0.875),
    (16.5, 6.25),
    (2.5, 0.0),
    (24.0, 5.5),
    (10.75, 8.25),
    (3.25, 2.5),
    (20.5, 0.0),
    (8.25, 3.5),
    (14.75, 9.25),
];

#[derive(Clone, Copy)]
enum Drawing {
    Boxes,
    LocalCanvas,
    SnappedCanvas(f32),
}

struct Sample {
    origin: [f32; 2],
    drawing: Drawing,
}

fn bars() -> impl Iterator<Item = (f32, f32, f32)> {
    BARS.into_iter().enumerate().map(|(i, (height, foot))| {
        let from_right = (BARS.len() - 1 - i) as f32;
        let x = STRIP.0 - BAR_WIDTH - from_right * (BAR_WIDTH + GAP);
        (x, height, foot)
    })
}

fn boxes() -> El {
    el().children(bars().map(|(x, height, foot)| {
        let bar = el()
            .absolute(x, STRIP.1 - height)
            .size(BAR_WIDTH, height)
            .corners([RADIUS, RADIUS, 0.0, 0.0])
            .fill(TOP);
        if foot > 0.0 {
            bar.child(
                el().absolute(0.0, height - foot)
                    .size(BAR_WIDTH, foot)
                    .fill(FOOT),
            )
        } else {
            bar
        }
    }))
}

// The strip is positioned directly in the window, without scrolling or a transform. Snap
// each pair of window-space edges, then express it relative to the canvas's rounded origin.
fn snap_axis(lo: f32, hi: f32, origin: f32, scale: f32) -> [f32; 2] {
    let round = |v: f32| (v + 0.5).floor();
    let to_window = |v: f32| ((origin + v) * scale * 64.0).round() / 64.0;
    let (a, b) = (to_window(lo), to_window(hi));
    if b - a < 0.5 {
        return [a, b].map(|v| (v - round(to_window(0.0))) / scale);
    }
    [round(a), round(b).max(round(a) + 1.0)].map(|v| (v - round(to_window(0.0))) / scale)
}

fn flat(shape: Shape, x: [f32; 2], y: [f32; 2], color: Color) -> Shape {
    if y[0] >= y[1] {
        return shape;
    }
    let cx = (x[0] + x[1]) * 0.5;
    shape.line([cx, y[0]], [cx, y[1]], x[1] - x[0], LineCap::Butt, color)
}

fn rounded_top(
    shape: Shape,
    [x0, x1]: [f32; 2],
    [y0, y1]: [f32; 2],
    snapped_scale: Option<f32>,
) -> Shape {
    let radius = RADIUS.min((x1 - x0).min(y1 - y0) * 0.5);
    let (a, b) = if y1 - y0 >= x1 - x0 {
        let cx = (x0 + x1) * 0.5;
        ([cx, y0 + radius], [cx, y1 - radius])
    } else {
        ([x0 + radius, y0 + radius], [x1 - radius, y0 + radius])
    };
    // A capsule supplies the circular top corners. Square off its lower end with a box.
    let shape = shape.line(a, b, 2.0 * radius, LineCap::Round, TOP);
    // On the device grid, a row of pixel centres through the capsule's centre is already
    // fully covered. Start the square fill below that row so it doesn't blend there again.
    let join = snapped_scale.map_or(b[1], |scale| (b[1] * scale).ceil() / scale);
    flat(shape, [x0, x1], [join, y1], TOP)
}

fn shape(axis: impl Fn(usize, f32, f32) -> [f32; 2], snapped_scale: Option<f32>) -> Shape {
    bars().fold(Shape::new(), |shape, (x, height, foot)| {
        let x = axis(0, x, x + BAR_WIDTH);
        let y = axis(1, STRIP.1 - height, STRIP.1);
        let shape = rounded_top(shape, x, y, snapped_scale);
        if foot > 0.0 {
            flat(shape, x, axis(1, STRIP.1 - foot, STRIP.1), FOOT)
        } else {
            shape
        }
    })
}

fn view(sample: &Sample, _: &mut Cx<Sample>) -> El {
    let strip = match sample.drawing {
        Drawing::Boxes => boxes(),
        Drawing::LocalCanvas => canvas(shape(|_, lo, hi| [lo, hi], None)),
        Drawing::SnappedCanvas(scale) => canvas(shape(
            |axis, lo, hi| snap_axis(lo, hi, sample.origin[axis], scale),
            Some(scale),
        )),
    };
    el().fill(Color::BLACK).child(
        strip
            .absolute(sample.origin[0], sample.origin[1])
            .size(STRIP.0, STRIP.1),
    )
}

fn render(
    painter: &mut CpuPainter,
    origin: [f32; 2],
    drawing: Drawing,
    scale: u32,
) -> Result<Vec<u8>, NeedsGpu> {
    let mut rt = Runtime::new(
        Sample { origin, drawing },
        view,
        (FRAME.0 as f32, FRAME.1 as f32),
        Env::new(true, scale as f32),
    );
    rt.set_srgb_blend(true);
    rt.frame(0.0);

    let (w, h) = (FRAME.0 * scale, FRAME.1 * scale);
    let scene = Scene::new(
        &rt.instances,
        &rt.text.atlas,
        &rt.images,
        (w, h),
        rt.clear_color(),
    );
    let mut pixels = vec![0; (w * h * 4) as usize];
    painter.forget(); // Each sample paints into a fresh buffer.
    painter.paint(&scene, &mut Target::new(&mut pixels, w, h))?;
    Ok(pixels)
}

#[derive(Default)]
struct Diff {
    pixels: usize,
    changed: usize,
    max: u8,
}

impl Diff {
    fn compare(&mut self, reference: &[u8], sample: &[u8]) {
        assert_eq!(reference.len(), sample.len());
        let (reference, _) = reference.as_chunks::<4>();
        let (sample, _) = sample.as_chunks::<4>();
        for (a, b) in reference.iter().zip(sample) {
            let delta = a
                .iter()
                .zip(b)
                .map(|(&a, &b)| a.abs_diff(b))
                .max()
                .unwrap_or(0);
            self.pixels += 1;
            self.changed += usize::from(delta > 0);
            self.max = self.max.max(delta);
        }
    }
}

fn main() -> Result<(), NeedsGpu> {
    let mut painter = CpuPainter::new(1);
    let offsets = [0.0, 1.0 / 3.0, 0.5];
    for scale in [1, 2] {
        let mut local = Diff::default();
        let mut snapped = Diff::default();
        for dx in offsets {
            for dy in offsets {
                let origin = [4.0 + dx, 4.0 + dy];
                let boxes = render(&mut painter, origin, Drawing::Boxes, scale)?;
                let ordinary = render(&mut painter, origin, Drawing::LocalCanvas, scale)?;
                let adjusted = render(
                    &mut painter,
                    origin,
                    Drawing::SnappedCanvas(scale as f32),
                    scale,
                )?;
                local.compare(&boxes, &ordinary);
                snapped.compare(&boxes, &adjusted);
            }
        }
        println!("Scale {scale}x (all nine origins, compared with box fills)");
        println!(
            "{:<16} {:>14} {:>18} {:>16}",
            "Drawing", "Pixels differ", "Max channel delta", "Pixels compared"
        );
        for (name, diff) in [("Local canvas", local), ("Snapped canvas", snapped)] {
            println!(
                "{name:<16} {:>14} {:>18} {:>16}",
                diff.changed, diff.max, diff.pixels
            );
        }
        println!();
    }
    Ok(())
}
