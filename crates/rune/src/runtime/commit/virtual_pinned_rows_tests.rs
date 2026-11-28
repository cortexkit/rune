use std::cell::Cell;

use super::memo_reuse::same_root_el;
use crate::runtime::ElementInfo;
use crate::{Color, Cx, El, Env, GrowthSpec, Runtime, ScrollSpec, TextStyle, col, memo, text};

const ROWS: usize = 500;
const MIN_ROW: f32 = 20.0;
const ESTIMATE: f32 = 24.0;
const WIDTH: f32 = 240.0;
const VIEWPORT: f32 = 192.0;
const STEP: f64 = 1.0 / 120.0;

struct Chat {
    rows: usize,
    tail: &'static str,
    minimum: Option<usize>,
    overscan: f32,
    view_builds: Cell<usize>,
    layout_reports: usize,
    eager: bool,
}

fn message(s: &Chat, i: usize) -> El {
    let body = if i + 1 == s.rows {
        s.tail.to_owned()
    } else {
        format!("message {i}")
    };
    col()
        .w_full()
        .min_h(MIN_ROW + 4.0 * (i % 3) as f32)
        .fill(Color::WHITE)
        .child(text(body).font(TextStyle::new(12.0, 400.0)).wrap().w_full())
}

fn list(s: &Chat) -> El {
    let stack = col()
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_overscan(s.overscan, (s.overscan - 0.25).max(0.0));
    if s.eager {
        stack
            .virtualize(ESTIMATE)
            .children((0..s.rows).map(|i| message(s, i).key(format!("r{i}"))))
    } else {
        stack.virtual_rows(
            s.rows,
            ESTIMATE,
            |_: &Chat, i| format!("r{i}").into(),
            message,
        )
    }
}

fn viewport(list: El) -> El {
    col()
        .key("scroll")
        .size(WIDTH, VIEWPORT)
        .scroll(ScrollSpec::stick_bottom())
        .on_layout_tx(|s: &mut Chat, _, _| s.layout_reports += 1)
        .child(list)
}

fn runtime(minimum: Option<usize>, overscan: f32, eager: bool, memoized: bool) -> Runtime<Chat> {
    fn view(s: &Chat, cx: &mut Cx<Chat>) -> El {
        s.view_builds.set(s.view_builds.get() + 1);
        cx.reduced_motion(true);
        viewport(with_minimum(list(s).key("messages"), s.minimum))
    }
    fn memo_view(s: &Chat, cx: &mut Cx<Chat>) -> El {
        cx.reduced_motion(true);
        let list = memo("messages", (), || {
            s.view_builds.set(s.view_builds.get() + 1);
            list(s)
        });
        let built = list
            .memo_built()
            .expect("view builds a memo placeholder")
            .clone();
        let list = with_minimum(list, s.minimum);
        // Windowing can still pick a new range even if the root comparison misses a change.
        // Check that the cached root is rejected on the setting alone, before windowing.
        assert_eq!(
            same_root_el(&list, &built, false),
            s.minimum.is_none(),
            "memo root comparison must notice the pinned row setting"
        );
        viewport(list)
    }
    let s = Chat {
        rows: ROWS,
        tail: "latest message",
        minimum,
        overscan,
        view_builds: Cell::new(0),
        layout_reports: 0,
        eager,
    };
    let mut rt = Runtime::new(
        s,
        if memoized { memo_view } else { view },
        (WIDTH, VIEWPORT),
        Env::new(true, 1.0),
    );
    rt.frame(0.0);
    assert_at_foot(&rt);
    rt
}

fn with_minimum(list: El, minimum: Option<usize>) -> El {
    if let Some(rows) = minimum {
        list.virtual_pinned_rows(rows)
    } else {
        list
    }
}

fn row_index(el: &ElementInfo) -> Option<usize> {
    el.key.as_deref()?.strip_prefix('r')?.parse().ok()
}

fn mounted(rt: &Runtime<Chat>) -> Vec<ElementInfo> {
    rt.elements()
        .into_iter()
        .filter(|el| row_index(el).is_some())
        .collect()
}

fn last_row(rt: &Runtime<Chat>) -> ElementInfo {
    let last = rt.state.rows - 1;
    mounted(rt)
        .into_iter()
        .find(|el| row_index(el) == Some(last))
        .expect("tail row must be mounted")
}

fn assert_at_foot(rt: &Runtime<Chat>) -> ElementInfo {
    let last = last_row(rt);
    assert_eq!(rt.scroll_at_bottom("scroll"), Some(true));
    assert!(last.visible[3] > 0.0, "last row must be visible");
    assert!(
        (last.rect[1] + last.rect[3] - VIEWPORT).abs() < 0.05,
        "last row must meet viewport foot: {:?}",
        last.rect
    );
    last
}

fn assert_viewport_covered(rt: &Runtime<Chat>) {
    let mut end = 0.0;
    for el in mounted(rt).into_iter().filter(|e| e.visible[3] > 0.0) {
        assert!(
            (el.visible[1] - end).abs() < 0.05,
            "blank band before {:?}: {end} to {}",
            el.key,
            el.visible[1]
        );
        end = el.visible[1] + el.visible[3];
    }
    assert!(
        (end - VIEWPORT).abs() < 0.05,
        "blank band at viewport foot: {end} to {VIEWPORT}"
    );
}

fn assert_pinned_bound(rt: &Runtime<Chat>) {
    // At the end, there is overscan on only one side. One row may straddle its upper edge.
    let bound = (VIEWPORT * (1.0 + rt.state.overscan) / MIN_ROW).ceil() as usize + 1;
    let count = mounted(rt).len();
    assert!(
        count <= bound,
        "pinned viewport and overscan need at most {bound} rows, got {count}"
    );
}

#[test]
fn pinned_rows_zero_mounts_only_the_viewport_and_overscan() {
    for eager in [false, true] {
        let rt = runtime(Some(0), 0.5, eager, false);
        assert_pinned_bound(&rt);
        assert_viewport_covered(&rt);
    }
}

#[test]
fn pinned_rows_append_lands_at_the_viewport_foot_in_one_frame() {
    let mut rt = runtime(Some(0), 0.5, false, false);
    rt.update(|s, _| s.rows += 1);
    rt.frame(STEP);
    assert_at_foot(&rt);
    assert_viewport_covered(&rt);
}

#[test]
fn pinned_rows_last_message_wraps_and_stays_at_the_viewport_foot_in_one_frame() {
    let mut rt = runtime(Some(0), 0.5, false, false);
    let before = last_row(&rt);
    rt.update(|s, _| {
        s.tail =
            "A longer message that wraps onto several lines in the chat pane as more words arrive."
    });
    rt.frame(STEP);
    let after = assert_at_foot(&rt);
    assert!(
        after.rect[3] > before.rect[3],
        "the same row must actually grow"
    );
    assert_viewport_covered(&rt);
}

fn assert_first_wheel_moves_by_input(minimum: Option<usize>) {
    const WHEEL: f32 = 150.0;
    let mut rt = runtime(minimum, 0.5, false, false);
    let before = mounted(&rt);
    let list = rt.commit.virtuals.values().next().unwrap();
    let unmeasured = (0..ROWS)
        .filter(|i| {
            let min_height = MIN_ROW + 4.0 * (i % 3) as f32;
            min_height != ESTIMATE && list.height(&format!("r{i}")) == ESTIMATE
        })
        .collect::<Vec<_>>();

    rt.wheel(WIDTH / 2.0, VIEWPORT / 2.0, WHEEL);
    for frame in 1..=5 {
        rt.frame(STEP);
        assert_eq!(rt.scroll_at_bottom("scroll"), Some(false));
        let after = mounted(&rt);
        if minimum == Some(0) && frame == 1 {
            assert!(
                after.iter().any(|el| el.visible[3] > 0.0
                    && el.rect[3] != ESTIMATE
                    && unmeasured.contains(&row_index(el).unwrap())),
                "the wheel must expose rows whose measured heights differ from the estimate"
            );
        }
        let mut compared = 0;
        for row in before.iter().filter(|el| el.visible[3] > 0.0) {
            if let Some(now) = after
                .iter()
                .find(|el| el.key == row.key && el.visible[3] > 0.0)
            {
                compared += 1;
                let moved = now.rect[1] - row.rect[1];
                assert!(
                    (moved - WHEEL).abs() < 0.05,
                    "frame {frame}, {:?}: wheel moved {WHEEL} pt, row moved {moved} pt",
                    row.key
                );
            }
        }
        assert!(compared > 0, "both frames must show a common row");
        assert_viewport_covered(&rt);
    }
}

#[test]
fn pinned_rows_zero_first_wheel_move_preserves_the_reading_position() {
    assert_first_wheel_moves_by_input(Some(0));
}

#[test]
fn pinned_rows_default_first_wheel_move_preserves_the_reading_position() {
    assert_first_wheel_moves_by_input(None);
}

#[test]
fn pinned_rows_large_wheel_move_fills_the_viewport_in_one_frame() {
    let mut rt = runtime(Some(0), 0.5, false, false);
    rt.wheel(WIDTH / 2.0, VIEWPORT / 2.0, VIEWPORT * 2.0);
    rt.frame(STEP);
    assert_eq!(rt.scroll_at_bottom("scroll"), Some(false));
    assert_viewport_covered(&rt);
}

#[test]
fn pinned_rows_return_to_the_end_restores_the_small_window() {
    let mut rt = runtime(Some(0), 0.5, false, false);
    rt.wheel(WIDTH / 2.0, VIEWPORT / 2.0, VIEWPORT * 2.0);
    rt.frame(STEP);
    assert_eq!(rt.scroll_at_bottom("scroll"), Some(false));
    rt.update(|_, tx| tx.scroll_to_bottom_instant("scroll"));
    rt.frame(STEP);
    assert_at_foot(&rt);
    assert_pinned_bound(&rt);
    assert_viewport_covered(&rt);
}

#[test]
fn pinned_rows_adds_no_commits_layout_work_or_wakes_at_rest() {
    for minimum in [None, Some(0)] {
        let mut rt = runtime(minimum, 0.5, false, false);
        rt.frame(STEP); // Deliver the first layout report before measuring idle frames.
        let commits = rt.stats.commits;
        let layout_nodes = rt.stats.layout_nodes;
        let view_builds = rt.state.view_builds.get();
        let layout_reports = rt.state.layout_reports;
        assert!(view_builds > 0 && layout_reports > 0);
        for _ in 0..5 {
            assert!(
                !rt.frame(STEP),
                "a resting viewport should not draw a frame"
            );
            assert_eq!(rt.wake_deadline(), None);
        }
        assert_eq!(rt.stats.commits, commits);
        assert_eq!(rt.stats.layout_nodes, layout_nodes);
        assert_eq!(rt.state.view_builds.get(), view_builds);
        assert_eq!(rt.state.layout_reports, layout_reports);
    }
}

#[test]
fn pinned_rows_memo_root_applies_only_setting_change_on_next_commit() {
    let mut rt = runtime(None, 0.5, false, true);
    assert_eq!(mounted(&rt).len(), 32);
    rt.update(|s, _| s.minimum = Some(0));
    rt.frame(STEP);
    assert_pinned_bound(&rt);
    assert_at_foot(&rt);

    rt.update(|s, _| s.minimum = None);
    rt.frame(STEP);
    assert_eq!(mounted(&rt).len(), 32);
    assert_eq!(
        rt.state.view_builds.get(),
        1,
        "the memo builder stayed cached"
    );
}
