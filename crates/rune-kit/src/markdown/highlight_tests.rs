use rune::{Cx, Env, Runtime};

use super::*;

const SOURCE: &str = "first second third fourth fifth sixth seventh";
const INK: Color = Color { r: 1.0, g: 0.0, b: 1.0, a: 1.0 };
const BACKGROUND: Color = Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };

#[derive(Default)]
struct Case {
    highlight: Option<Highlight>,
    bounds: Cell<Option<[f32; 4]>>,
}

fn drawn(highlight: Option<Highlight>) -> Runtime<Case> {
    fn view(case: &Case, cx: &mut Cx<Case>) -> El {
        case.bounds.set(cx.layout("note"));
        let style = MarkdownStyle::neutral(true).highlight_bg(BACKGROUND)
            .highlight_current_bg(BACKGROUND).highlight_current_fg(Some(BACKGROUND))
            .highlight_current_ring(Some((BACKGROUND, 1.0)));
        col().w(110.0).child(Markdown::new(SOURCE)
            .highlights(case.highlight.clone().map(|h| (0..SOURCE.len(), h.key("note"))))
            .render(&style).key("md"))
    }
    let mut rt = Runtime::new(Case { highlight, bounds: Cell::new(None) }, view, (130.0, 260.0), Env::new(true, 1.0));
    rt.set_srgb_blend(false);
    rt.frame(0.0);
    rt.invalidate();
    rt.frame(0.0);
    rt
}

fn underline_rects(rt: &Runtime<Case>) -> Vec<[f32; 4]> {
    rt.instances.iter().filter(|q| q.fill == [INK.r, INK.g, INK.b, INK.a]).map(|q| q.rect).collect()
}

#[test]
fn markdown_highlight_underline_paints_only_the_line_and_keeps_its_key() {
    let plain = drawn(None);
    let solid = drawn(Some(Highlight::current().underline(INK)));
    let lines = underline_rects(&solid);
    assert!(lines.len() > 1, "an underline follows the wraps: {lines:?}");
    let [x, y, w, h] = solid.state.bounds.get().expect("Cx::layout sees the underline's key");
    assert!(w > 0.0 && h > 0.0);
    for [l, top, width, thickness] in lines.iter().copied() {
        assert_eq!(thickness, 1.0);
        assert!(l >= x - 1.0 && top >= y && l + width <= x + w + 1.0 && top <= y + h);
    }
    let other: Vec<_> = solid.instances.iter().filter(|q| q.fill != [INK.r, INK.g, INK.b, INK.a]).collect();
    assert_eq!(other.len(), plain.instances.len(), "an underline adds no background or ring draws");
    for (a, b) in other.iter().zip(plain.instances.iter()) {
        assert_eq!((a.rect, a.fill, a.border, a.params), (b.rect, b.fill, b.border, b.params));
    }
}

#[test]
fn markdown_highlight_dashes_have_gaps_on_each_wrapped_line() {
    let solid = drawn(Some(Highlight::new().underline(INK)));
    let dashed = drawn(Some(Highlight::new().underline_dashed(INK)));
    let solid_lines = underline_rects(&solid);
    let dashed_lines = underline_rects(&dashed);
    assert_eq!(solid.state.bounds.get(), dashed.state.bounds.get());
    assert!(dashed_lines.len() > solid_lines.len());
    for [left, y, width, height] in solid_lines {
        let dashes: Vec<_> = dashed_lines.iter().filter(|d| d[1] == y).collect();
        assert!(dashes.len() > 1);
        assert_eq!(dashes[0][0], left);
        assert!(dashes.iter().all(|d| d[3] == height && d[0] + d[2] <= left + width));
        assert!(dashes.windows(2).all(|d| d[0][0] + d[0][2] < d[1][0]), "dashes leave unpainted gaps");
    }
}

#[test]
fn markdown_highlight_without_underline_keeps_the_default_match_fill() {
    let default = drawn(Some(Highlight::new()));
    let explicit = drawn(Some(Highlight::new().bg(BACKGROUND)));
    assert!(default.instances.iter().any(|q| q.fill == [BACKGROUND.r, BACKGROUND.g, BACKGROUND.b, BACKGROUND.a]));
    assert_eq!(default.instances.len(), explicit.instances.len());
    for (a, b) in default.instances.iter().zip(explicit.instances.iter()) {
        assert_eq!((a.rect, a.fill, a.border, a.params), (b.rect, b.fill, b.border, b.params));
    }
}
