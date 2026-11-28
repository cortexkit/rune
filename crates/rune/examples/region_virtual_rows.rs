//! Count indexed row builds while a neighboring region's counter changes.
//!
//! `cargo run --locked --profile perf -p rune --example region_virtual_rows`
//!
//! Each case starts a fresh headless runtime. The counter starts ticking after the list fills
//! its overscan and any focus or selection has been applied.

use std::cell::Cell;
use std::ops::Range;

use rune::{
    App, AppSpec, Cx, El, Env, GrowthSpec, ScrollSpec, TextEdit, TextStyle, Tracked, col, memo,
    row, text, text_input,
};

const SIZE: (f32, f32) = (560.0, 480.0);
const ROWS: usize = 3_000;
const FRAMES: usize = 8;
const ROW_TARGET: &str = "label-3";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Case {
    #[default]
    Baseline,
    StickBottom,
    Overscan,
    LowEstimate,
    UniformExactEstimate,
    TrackedRowRead,
    RowFocus,
    RowSelection,
    RegionWith,
    ListAtRoot,
    TwoWrappers,
    UnfocusedTrackedField,
    FocusedPlainField,
    FocusedTrackedField,
}

#[derive(Default)]
struct State {
    case: Case,
    counter: Tracked<usize>,
    row_value: Tracked<usize>,
    plain_field: TextEdit,
    tracked_field: Tracked<TextEdit>,
    list_builds: Cell<usize>,
    counter_builds: Cell<usize>,
    row_builds: Cell<usize>,
    visible: Range<usize>,
}

fn build_row(s: &State, i: usize) -> El {
    s.row_builds.set(s.row_builds.get() + 1);
    let height = if s.case == Case::UniformExactEstimate {
        24.0
    } else {
        18.0 + 6.0 * (i % 3) as f32
    };
    let label = if s.case == Case::TrackedRowRead && i == 0 {
        format!("Row {i}: {}", *s.row_value)
    } else {
        format!("Row {i}")
    };
    col().w_full().h(height).child(
        text(label)
            .key(format!("label-{i}"))
            .font(TextStyle::new(14.0, 400.0))
            .selectable(true)
            .focusable(true),
    )
}

fn list(s: &State) -> El {
    s.list_builds.set(s.list_builds.get() + 1);
    let estimate = if s.case == Case::LowEstimate {
        12.0
    } else {
        24.0
    };
    let mut rows = col()
        .key("rows")
        .w_full()
        .selection_group(true)
        .growth(GrowthSpec::default())
        .virtual_rows(
            ROWS,
            estimate,
            |_: &State, i| format!("item-{i}").into(),
            build_row,
        )
        .on_visible_rows(|s: &mut State, range| {
            s.visible = range;
            false
        });
    if s.case == Case::Overscan {
        rows = rows.virtual_overscan(0.5, 0.25);
    }
    let font = TextStyle::new(14.0, 400.0);
    match s.case {
        Case::ListAtRoot => rows,
        Case::TwoWrappers => col().w_full().child(col().w_full().child(rows)),
        Case::UnfocusedTrackedField | Case::FocusedTrackedField | Case::FocusedPlainField => {
            let field = if s.case == Case::FocusedPlainField {
                text_input(&s.plain_field, font).on_edit(|s: &mut State| &mut s.plain_field)
            } else {
                text_input(&s.tracked_field, font).on_edit(|s: &mut State| &mut s.tracked_field)
            };
            col()
                .w_full()
                .child(field.key("field").w_full().h(24.0).min_h(24.0))
                .child(rows)
        }
        _ => col().w_full().child(rows),
    }
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let list = if s.case == Case::RegionWith {
        memo::region_with("list", ROWS, || list(s))
    } else {
        memo::region("list", || list(s))
    };
    row()
        .w_full()
        .h_full()
        .gap(20.0)
        .child(
            col()
                .key("viewport")
                .size(360.0, SIZE.1)
                .scroll(ScrollSpec {
                    stick_bottom: s.case == Case::StickBottom,
                })
                .child(list),
        )
        .child(memo::region("counter", || {
            s.counter_builds.set(s.counter_builds.get() + 1);
            text(format!("Counter: {}", *s.counter)).font(TextStyle::new(14.0, 400.0))
        }))
}

fn run(case: Case) {
    let spec = AppSpec::new("Region virtual rows", SIZE, State::default, view);
    let mut rt = App::new(spec)
        .init(move || {
            let mut plain_field = TextEdit::new("A text field");
            plain_field.focused = case == Case::FocusedPlainField;
            let mut tracked_field = TextEdit::new("A text field");
            tracked_field.focused = case == Case::FocusedTrackedField;
            State {
                case,
                plain_field,
                tracked_field: Tracked::new(tracked_field),
                ..State::default()
            }
        })
        .headless_runtime(SIZE, Env::new(true, 1.0));

    rt.frame(0.0);
    let first_rows = rt.state.row_builds.get();
    let focus = match case {
        Case::RowFocus => Some(ROW_TARGET),
        Case::FocusedPlainField | Case::FocusedTrackedField => Some("field"),
        _ => None,
    };
    if case == Case::RowFocus {
        rt.update(|_, tx| tx.focus(ROW_TARGET, false));
    } else if case == Case::RowSelection {
        rt.update(|_, tx| tx.select_text(ROW_TARGET, 0..5));
    }
    for _ in 0..60 {
        rt.frame(1.0 / 120.0);
    }

    let visible = rt.state.visible.clone();
    assert!((18..=23).contains(&visible.len()), "{case:?}: {visible:?}");
    if case == Case::StickBottom {
        assert_eq!(rt.scroll_at_bottom("viewport"), Some(true));
        assert_eq!(visible.end, ROWS);
    }
    let mut row_builds = [0; FRAMES];
    rt.state.list_builds.set(0);
    rt.state.counter_builds.set(0);
    for builds in &mut row_builds {
        let commits = rt.stats.commits;
        rt.state.row_builds.set(0);
        rt.update(|s, _| *s.counter += 1);
        rt.frame(1.0 / 120.0);
        *builds = rt.state.row_builds.get();
        assert_eq!(rt.stats.commits - commits, 1, "{case:?}");
        assert_eq!(rt.state.visible, visible, "{case:?}");
        assert_eq!(rt.presented_focus_key(), focus, "{case:?}");
        if case == Case::RowSelection {
            assert_eq!(rt.selected_text().as_deref(), Some("Row 3"));
        }
    }
    println!(
        "{case:?}: visible={visible:?} first_rows={first_rows} rows/frame={row_builds:?} list_region={} counter_region={}",
        rt.state.list_builds.get(),
        rt.state.counter_builds.get(),
    );
}

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("run with --profile perf; debug builds re-run kept regions");
        std::process::exit(1);
    }
    for case in [
        Case::Baseline,
        Case::StickBottom,
        Case::Overscan,
        Case::LowEstimate,
        Case::UniformExactEstimate,
        Case::TrackedRowRead,
        Case::RowFocus,
        Case::RowSelection,
        Case::RegionWith,
        Case::ListAtRoot,
        Case::TwoWrappers,
        Case::UnfocusedTrackedField,
        Case::FocusedPlainField,
        Case::FocusedTrackedField,
    ] {
        run(case);
    }
}
