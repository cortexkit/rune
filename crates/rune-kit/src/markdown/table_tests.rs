use rune::{Color, Cx, ElementInfo, Env, Runtime};

use super::table::auto_layout;
use super::*;

/// A short column of two-word cells before a long one. Its content is wider than its longest
/// word, so a rule that narrows it can break it onto two lines.
const SHORT_FIRST: &str = concat!(
    "| Run id | What happened |\n|---|---|\n",
    "| run 7 | The build server refused the job because another build held its only slot for half an hour |\n",
);

/// The same columns the other way round, so the short cell is not the first in its row.
const SHORT_LAST: &str = concat!(
    "| What happened | Run id |\n|---|---|\n",
    "| The build server refused the job because another build held its only slot for half an hour | run 7 |\n",
);

struct Case {
    doc: &'static str,
    style: MarkdownStyle,
    width: f32,
}

fn view(c: &Case, _: &mut Cx<Case>) -> El {
    col().w(c.width).child(Markdown::new(c.doc).render(&c.style).key("md"))
}

/// `doc` drawn with `style` in a column `width` points wide.
fn drawn(doc: &'static str, style: MarkdownStyle, width: f32) -> Runtime<Case> {
    let mut rt = Runtime::new(Case { doc, style, width }, view, (width, 600.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

/// The element whose path ends with `suffix`: `"/r0"` is the first body row, `"/th"` the header
/// row, `"r0/c1"` the first body row's second cell and `"r0/c0>"` the gap after its first.
fn part(rt: &Runtime<Case>, suffix: &str) -> ElementInfo {
    rt.elements().into_iter().find(|e| e.path.ends_with(suffix)).unwrap_or_else(|| panic!("no element ends with {suffix:?}"))
}

/// `frames` frames a sixtieth of a second apart, for a hover to fade in or out.
fn settle(rt: &mut Runtime<Case>, frames: usize) {
    for _ in 0..frames {
        rt.frame(1.0 / 60.0);
    }
}

#[test]
fn widest_first_keeps_a_short_column_whole_where_proportional_narrows_it() {
    let neutral = || MarkdownStyle::neutral(true);
    // The short column's content width: what it takes when everything fits.
    let whole = part(&drawn(SHORT_FIRST, neutral(), 2000.0), "r0/c0").rect[2];

    let proportional = drawn(SHORT_FIRST, neutral(), 300.0);
    let narrowed = part(&proportional, "r0/c0").rect[2];
    assert!(narrowed < whole - 1.0, "proportional narrows the short column: {narrowed} of {whole}");

    let widest = drawn(SHORT_FIRST, neutral().table_shrink(TableShrink::WidestFirst), 300.0);
    let (short, long, row) = (part(&widest, "r0/c0").rect, part(&widest, "r0/c1").rect, part(&widest, "/r0").rect);
    assert!((short[2] - whole).abs() < 0.5, "widest first keeps the short column whole: {} of {whole}", short[2]);
    // The long column takes the rest: the cells still fill the row, in the header as in the body.
    assert!((long[0] + long[2] - (row[0] + row[2])).abs() < 0.5, "{long:?} ends the row {row:?}");
    assert!((part(&widest, "th/c1").rect[0] - long[0]).abs() < 0.5, "the header's columns line up with the body's");
}

#[test]
fn widest_first_shares_a_full_width_table_s_spare_room_by_content() {
    let full = || MarkdownStyle::neutral(true).table_full_width(true);
    let whole = part(&drawn(SHORT_FIRST, MarkdownStyle::neutral(true), 2000.0), "r0/c0").rect[2];

    let proportional = drawn(SHORT_FIRST, full(), 1200.0);
    let widest = drawn(SHORT_FIRST, full().table_shrink(TableShrink::WidestFirst), 1200.0);
    // Both rules start the second column at the same place and end the last at the row's end.
    let (second, same) = (part(&widest, "r0/c1").rect, part(&proportional, "r0/c1").rect);
    assert!((second[0] - same[0]).abs() < 0.5, "second column starts at {} and {}", second[0], same[0]);
    // Under widest first the cell itself keeps its content's width, and the column's spare room
    // is a gap after it, since the column's text starts at its leading edge.
    let (cell, gap) = (part(&widest, "r0/c0").rect, part(&widest, "r0/c0>").rect);
    assert!((cell[2] - whole).abs() < 0.5, "the cell is {} wide, its content {whole}", cell[2]);
    assert!((gap[0] - (cell[0] + cell[2])).abs() < 0.5 && (gap[0] + gap[2] - second[0]).abs() < 0.5, "{gap:?} fills {cell:?} to {second:?}");
    let (last, row) = (part(&widest, "r0/c1>").rect, part(&widest, "/r0").rect);
    assert!((last[0] + last[2] - (row[0] + row[2])).abs() < 0.5, "{last:?} ends the row {row:?}");
}

#[test]
fn a_column_rule_starts_every_column_but_the_first_at_the_row_s_height() {
    let rule = Color::BLACK;
    let ruled = drawn(SHORT_LAST, MarkdownStyle::neutral(true).table_column_rules(Some(rule)), 300.0);
    for row in ["th", "r0"] {
        let (first, second, whole) = (part(&ruled, &format!("{row}/c0")), part(&ruled, &format!("{row}/c1")), part(&ruled, &format!("/{row}")));
        assert_eq!(first.border_color, None, "{row}: no rule before the first column");
        assert_eq!(second.border_color, Some(rule), "{row}: a rule starts the second column");
        // The long first cell wraps to several lines. A row stretches its cells, so the short
        // cell's box, and the rule on it, still run the row's height.
        assert!((second.rect[3] - whole.rect[3]).abs() < 0.5, "{row}: the rule {:?} runs the row's height {:?}", second.rect, whole.rect);
    }
    // The rule takes no room: the columns are where they are without it.
    let plain = drawn(SHORT_LAST, MarkdownStyle::neutral(true), 300.0);
    assert_eq!(part(&plain, "r0/c1").border_color, None);
    assert!((part(&plain, "r0/c1").rect[0] - part(&ruled, "r0/c1").rect[0]).abs() < 0.01);

    // In a full-width table under widest first, a right-aligned column starts with its gap, so
    // the rule stands on the gap and not on the cell.
    const RIGHT: &str = "| Name | Count |\n|---|--:|\n| alpha | 12 |\n";
    let style = MarkdownStyle::neutral(true).table_full_width(true).table_shrink(TableShrink::WidestFirst).table_column_rules(Some(rule));
    let right = drawn(RIGHT, style, 900.0);
    assert_eq!(part(&right, "r0/c1<").border_color, Some(rule));
    assert_eq!(part(&right, "r0/c1").border_color, None);
}

#[test]
fn a_body_row_under_the_pointer_takes_the_hover_overlay_and_the_header_row_never() {
    let over = |rt: &mut Runtime<Case>, suffix: &str| {
        let r = part(rt, suffix).rect;
        rt.pointer_move(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        settle(rt, 30);
        part(rt, suffix).hover
    };
    let mut hovering = drawn(SHORT_FIRST, MarkdownStyle::neutral(true).table_row_hover(Some(Color::BLACK.alpha(0.04))), 600.0);
    assert!(over(&mut hovering, "/r0") > 0.9, "the body row is hovered");
    assert_eq!(over(&mut hovering, "/th"), 0.0, "the header row takes no overlay");
    assert!(part(&hovering, "/r0").hover < 0.05, "the overlay leaves with the pointer");

    let mut plain = drawn(SHORT_FIRST, MarkdownStyle::neutral(true), 600.0);
    assert_eq!(over(&mut plain, "/r0"), 0.0, "no overlay without the style");
}

#[test]
fn widest_first_in_a_known_text_width_hands_out_equal_shares_between_the_bounds() {
    // Content widths 60, 200 and 600; longest words 40, 50 and 80.
    let (natural, fit) = ([60.0, 200.0, 600.0], [40.0, 50.0, 80.0]);
    let widths = |avail: f32| auto_layout(&natural, &fit, avail, false, TableShrink::WidestFirst);
    // 460 points: the first column is whole, and the other two share the rest equally.
    let w = widths(460.0);
    assert!((w[0] - 60.0).abs() < 0.01 && (w[1] - 200.0).abs() < 0.01 && (w[2] - 200.0).abs() < 0.01, "{w:?}");
    // 200 points: an equal share is 60, which the first column needs whole; the last is not
    // pushed under its longest word.
    let w = widths(200.0);
    assert!((w[0] - 60.0).abs() < 0.01 && (w[1] - 60.0).abs() < 0.01 && (w[2] - 80.0).abs() < 0.01, "{w:?}");
    // Room for everything, and for nothing but the longest words, are as for the other rule.
    assert_eq!(widths(1000.0), natural.to_vec());
    assert_eq!(widths(100.0), fit.to_vec());
    assert_eq!(auto_layout(&natural, &fit, 460.0, false, TableShrink::Proportional), {
        let k = (460.0 - 170.0) / (860.0 - 170.0);
        natural.iter().zip(fit).map(|(w, f)| f + (w - f) * k).collect::<Vec<f32>>()
    });
}
