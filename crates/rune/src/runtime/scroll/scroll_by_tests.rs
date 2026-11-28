use super::*;
use crate::el::{col, el};
use crate::{GrowthSpec, Key, KeyEvent, ScrollPos, ScrollSource};
use std::{cell::Cell, rc::Rc};

/// Points, with a two-pixel-per-point device scale.
const WIDTH: f32 = 180.0;
const HEIGHT: f32 = 160.0;
const SCALE: f32 = 2.0;
const HALF_PIXEL: f32 = 0.5 / SCALE;
const ESTIMATE: f32 = 44.0;
const STEP: f64 = 1.0 / 60.0;

/// A request to run in the list's Up handler without changing the view's state.
type Request = Box<dyn FnOnce(&mut Tx)>;

/// A virtual feed, or a two-axis plain pane for the horizontal scroll tests.
struct Feed {
    /// Number of rows in the virtual feed.
    rows: usize,
    /// Whether the virtual viewport follows its tail.
    sticky: bool,
    /// Whether to build the plain pane instead of the feed.
    plain: bool,
    /// Whether the first row opts out of being a scroll anchor.
    unanchored_head: bool,
    /// Most recent offset report from the viewport.
    pos: Rc<Cell<ScrollPos>>,
    /// The next request to issue from the focused viewport.
    request: Option<Request>,
}

/// Build rows with unequal heights, all different from the estimate.
fn view(s: &Feed, _: &mut Cx<Feed>) -> El {
    let pos = Rc::clone(&s.pos);
    let pane = col()
        .key("scroll")
        .size(WIDTH, HEIGHT)
        .focusable(true)
        .on_key(|s: &mut Feed, ev, tx| {
            if ev.key != Key::Up {
                return false;
            }
            let Some(request) = s.request.take() else {
                return false;
            };
            tx.live_only();
            request(tx);
            true
        })
        .on_scroll_offset(move |p| pos.set(p));
    if s.plain {
        pane.scroll_xy()
            .child(el().key("content").size(1000.0, 1000.0))
    } else {
        pane.scroll(ScrollSpec {
            stick_bottom: s.sticky,
        })
        .child(
            col()
                .key("rows")
                .w_full()
                .growth(GrowthSpec::default())
                .virtual_overscan(0.5, 0.25)
                .virtual_rows(
                    s.rows,
                    ESTIMATE,
                    |_: &Feed, i| format!("r{i}").into(),
                    |s: &Feed, i| {
                        let row = el().w_full().h([54.0, 61.0, 47.0, 55.0, 54.0][i % 5]);
                        if i == 0 && s.unanchored_head {
                            row.no_scroll_anchor()
                        } else {
                            row
                        }
                    },
                ),
        )
    }
}

/// Focus the viewport using the same keyboard dispatch as an app.
fn runtime(plain: bool, sticky: bool) -> Runtime<Feed> {
    let s = Feed {
        rows: 180,
        sticky,
        plain,
        unanchored_head: false,
        pos: Rc::default(),
        request: None,
    };
    let mut rt = Runtime::new(s, view, (WIDTH, HEIGHT), Env::new(true, SCALE));
    rt.frame(0.0);
    press(&mut rt, Key::Tab);
    rt
}

/// Press one key and draw the frame it requests.
fn press(rt: &mut Runtime<Feed>, key: Key) -> bool {
    rt.key(KeyEvent {
        key,
        mods: Default::default(),
    });
    rt.frame(STEP)
}

/// Issue a request from the element's Up handler and advance exactly one frame.
fn request(rt: &mut Runtime<Feed>, ask: impl FnOnce(&mut Tx) + 'static) -> bool {
    rt.state.request = Some(Box::new(ask));
    let changed = press(rt, Key::Up);
    assert!(
        rt.state.request.is_none(),
        "the focused viewport must handle Up"
    );
    changed
}

/// Look up the viewport's scroll state independently of the offset watcher.
fn scroll(rt: &Runtime<Feed>) -> &ScrollState {
    let viewport = rt
        .tree
        .nodes
        .iter()
        .find(|n| n.key.as_deref() == Some("scroll"))
        .unwrap();
    &rt.scroll.scrolls[&viewport.id]
}

/// The one virtual list, with its measured heights and mounted range.
fn list(rt: &Runtime<Feed>) -> &crate::el::virtual_list::VirtualList {
    rt.commit.virtuals.values().next().unwrap()
}

/// IDs and drawn vertical bounds of mounted rows, sorted in content order.
fn rows(rt: &Runtime<Feed>) -> Vec<(usize, f32, f32)> {
    let mut rows: Vec<_> = rt
        .elements()
        .into_iter()
        .filter_map(|e| {
            let i = e.key.as_deref()?.strip_prefix('r')?.parse().ok()?;
            (!e.exiting).then_some((i, e.rect[1], e.rect[1] + e.rect[3]))
        })
        .collect();
    rows.sort_unstable_by_key(|&(i, _, _)| i);
    rows
}

/// The downward movement of the first row visible in both frames.
fn shared_distance(
    before: &[(usize, f32, f32)],
    after: &[(usize, f32, f32)],
) -> Option<(usize, f32)> {
    before
        .iter()
        .filter(|&&(_, y, end)| y < HEIGHT && end > 0.0)
        .find_map(|&(i, y, _)| {
            after
                .iter()
                .find(|&&(j, y, end)| j == i && y < HEIGHT && end > 0.0)
                .map(|&(_, to, _)| (i, to - y))
        })
}

/// Jump to the middle without scrolling through the still-unmeasured rows above it.
fn middle() -> Runtime<Feed> {
    let mut rt = runtime(false, false);
    request(&mut rt, |tx| tx.scroll_to_y("scroll", 3500.0));
    for _ in 0..4 {
        rt.frame(STEP);
    }
    rt
}

/// Move up ten times, observing the top edge of a row visible on both sides of each move.
fn up_ten(absolute: bool, mut check: impl FnMut(usize, f32)) -> (Vec<f32>, usize) {
    let mut rt = middle();
    let mut distances = Vec::new();
    let mut measured_above = 0;
    for step in 1..=10 {
        let before = rows(&rt);
        let pos = rt.state.pos.get();
        let unmeasured: Vec<_> = (0..rt.state.rows)
            .filter(|i| list(&rt).height(&format!("r{i}")) == ESTIMATE)
            .collect();
        assert!(
            pos.offset[1] > HEIGHT && pos.offset[1] < pos.max[1] - HEIGHT,
            "stay away from either clamp"
        );
        request(&mut rt, move |tx| {
            if absolute {
                tx.scroll_to_y("scroll", pos.offset[1] - 60.0);
            } else {
                tx.scroll_by("scroll", 0.0, -60.0);
            }
        });
        assert_eq!(rt.state.pos.get().source, ScrollSource::Programmatic);
        let (row, dy) = shared_distance(&before, &rows(&rt))
            .expect("a row must stay in sight across each 60-point move");
        check(step, dy);
        distances.push(dy);
        measured_above += unmeasured
            .into_iter()
            .filter(|&i| i < row && list(&rt).height(&format!("r{i}")) != ESTIMATE)
            .count();
    }
    (distances, measured_above)
}

/// The shared row moves by the requested distance despite first measurements above it.
#[test]
fn scroll_by_preserves_the_distance_through_ten_up_moves() {
    let (distances, measured) = up_ten(false, |step, dy| {
        assert!(
            (dy - 60.0).abs() <= HALF_PIXEL,
            "move {step}: row moved {dy} points, expected 60"
        );
    });
    assert!(
        measured > 0,
        "rows above the tracked row must measure for the first time"
    );
    println!("scroll_by distances: {distances:?}; new measurements above tracked rows: {measured}");
}

/// The absolute offset remains a place in the content, not an anchored relative move.
#[test]
fn scroll_to_y_still_shifts_a_visible_row_when_estimates_change_above_it() {
    let (distances, measured) = up_ten(true, |_, _| {});
    assert!(
        measured > 0,
        "the absolute jump must encounter unmeasured rows too"
    );
    assert!(
        distances.iter().any(|dy| (dy - 60.0).abs() > HALF_PIXEL),
        "{distances:?}"
    );
    println!(
        "scroll_to_y distances: {distances:?}; new measurements above tracked rows: {measured}"
    );
}

/// Project even a culled row's top using the measured prefix and the drawn scroll offset.
fn projected_top(rt: &Runtime<Feed>, row: usize) -> f32 {
    list(rt).prefix(row) - rt.state.pos.get().offset[1]
}

/// Large moves remount their destination without leaving blank space for a frame.
#[test]
fn scroll_by_more_than_a_viewport_lands_and_covers_it_in_both_directions() {
    let mut rt = middle();
    // Measure the stretch first, then return to the starting point. Across a gap of unknown
    // heights, rows between two disjoint windows don't have a known distance yet.
    for _ in 0..6 {
        request(&mut rt, |tx| tx.scroll_by("scroll", 0.0, 60.0));
    }
    for _ in 0..6 {
        request(&mut rt, |tx| tx.scroll_by("scroll", 0.0, -60.0));
    }
    for dy in [235.0, -235.0] {
        let before = rows(&rt);
        let &(top_row, y, _) = before
            .iter()
            .find(|&&(_, y, end)| y <= 0.0 && end > 0.0)
            .unwrap();
        assert!((projected_top(&rt, top_row) - y).abs() <= HALF_PIXEL);
        request(&mut rt, move |tx| tx.scroll_by("scroll", 0.0, dy));
        let mut covered = 0.0_f32;
        let mut newly_mounted = false;
        for (i, y, end) in rows(&rt)
            .into_iter()
            .filter(|&(_, y, end)| end > 0.0 && y < HEIGHT)
        {
            assert!(
                y <= covered + HALF_PIXEL,
                "gap at {covered} before row {i} at {y}"
            );
            covered = covered.max(end);
            newly_mounted |= !before.iter().any(|&(j, _, _)| j == i);
        }
        assert!(
            covered >= HEIGHT - HALF_PIXEL,
            "only covered {covered} points"
        );
        assert!(
            newly_mounted,
            "the destination must include newly mounted rows"
        );
        let distance = projected_top(&rt, top_row) - y;
        assert!(
            (distance + dy).abs() <= HALF_PIXEL,
            "old top moved {distance}, expected {}",
            -dy
        );
    }
}

/// A move beyond a cold tail lands on the measured end but does not start following it.
#[test]
fn scroll_by_clamps_to_the_cold_end_without_following_appends() {
    let mut rt = runtime(false, false);
    let last = format!("r{}", rt.state.rows - 1);
    let estimated_end = scroll(&rt).max;
    assert_eq!(list(&rt).height(&last), ESTIMATE);
    request(&mut rt, |tx| tx.scroll_by("scroll", 0.0, 20_000.0));
    let end = scroll(&rt).max;
    assert_ne!(list(&rt).height(&last), ESTIMATE);
    assert!(
        end > estimated_end,
        "measuring the tail must change the end"
    );
    assert_eq!(scroll(&rt).offset, end);
    assert!(!scroll(&rt).pinned());
    assert_eq!(rt.wake_deadline(), None);
    assert!(!rt.frame(STEP));
    assert_eq!(scroll(&rt).offset, end);
    assert!(!scroll(&rt).moving());

    rt.update(|s, tx| {
        s.rows += 1;
        tx.instant();
    });
    rt.frame(STEP);
    assert!(scroll(&rt).max > end);
    assert_eq!(
        scroll(&rt).offset,
        end,
        "a normal viewport must not follow the new end"
    );
}

/// A move to within ten points of the estimated end is still an anchored distance.
#[test]
fn scroll_by_stopping_short_of_a_cold_end_keeps_its_distance() {
    let mut rt = runtime(false, false);
    let end = scroll(&rt).max as f32;
    request(&mut rt, move |tx| tx.scroll_to_y("scroll", end - 129.0));
    // Move closer using the updated estimate after the first window has measured, while
    // leaving the last row out of overscan.
    let end = scroll(&rt).max as f32;
    request(&mut rt, move |tx| tx.scroll_to_y("scroll", end - 148.0));
    let last = format!("r{}", rt.state.rows - 1);
    assert_eq!(
        list(&rt).height(&last),
        ESTIMATE,
        "the last row must still be cold"
    );
    let [_, offset] = rt.state.pos.get().offset;
    let dy = rt.state.pos.get().max[1] - 10.0 - offset;
    let before = rows(&rt);
    request(&mut rt, move |tx| tx.scroll_by("scroll", 0.0, dy));
    let (_, distance) =
        shared_distance(&before, &rows(&rt)).expect("an already visible row must remain in sight");
    assert_ne!(
        list(&rt).height(&last),
        ESTIMATE,
        "the move must measure the tail"
    );
    assert!(
        (distance + dy).abs() <= HALF_PIXEL,
        "row moved {}, expected {dy}",
        -distance
    );
    assert!(scroll(&rt).offset < scroll(&rt).max);
}

/// A large move up lands on zero even when the first rows have yet to measure.
#[test]
fn scroll_by_clamps_to_the_cold_start_and_leaves_no_motion() {
    // Start at the tail so the initial layout leaves the head at its estimates.
    let mut rt = runtime(false, true);
    assert_eq!(list(&rt).height("r0"), ESTIMATE);
    request(&mut rt, |tx| tx.scroll_by("scroll", 0.0, -20_000.0));
    assert_ne!(list(&rt).height("r0"), ESTIMATE);
    assert_eq!(scroll(&rt).offset, 0.0);
    assert!(!scroll(&rt).pinned());
    assert_eq!(rt.wake_deadline(), None);
    assert!(!rt.frame(STEP));
    assert_eq!(scroll(&rt).offset, 0.0);
    assert!(!scroll(&rt).moving());
}

/// A cold leading row excluded from anchoring must not pull the scroll away from zero.
#[test]
fn scroll_by_clamps_to_the_start_even_with_an_unanchored_first_row() {
    let mut rt = runtime(false, true);
    rt.update(|s, tx| {
        s.unanchored_head = true;
        tx.instant();
    });
    rt.frame(STEP);
    assert_eq!(list(&rt).height("r0"), ESTIMATE);
    request(&mut rt, |tx| tx.scroll_by("scroll", 0.0, -20_000.0));
    assert_ne!(list(&rt).height("r0"), ESTIMATE);
    assert_eq!(scroll(&rt).offset, 0.0);
    assert!(!rt.frame(STEP));
    assert_eq!(scroll(&rt).offset, 0.0);
    assert_eq!(rt.wake_deadline(), None);
    assert!(!scroll(&rt).moving());
}

/// Leaving a pinned tail stops following it; landing there again resumes following appends.
#[test]
fn scroll_by_leaves_and_rejoins_stick_bottom() {
    let mut rt = runtime(false, true);
    assert!(scroll(&rt).pinned());
    request(&mut rt, |tx| tx.scroll_by("scroll", 0.0, -100.0));
    assert!(
        !scroll(&rt).pinned(),
        "moving away from the end must stop following"
    );
    assert!(scroll(&rt).offset < scroll(&rt).max - 50.0);
    let away = scroll(&rt).offset;
    rt.update(|s, tx| {
        s.rows += 1;
        tx.instant();
    });
    rt.frame(STEP);
    assert_eq!(
        scroll(&rt).offset,
        away,
        "an append must not pull the reader back"
    );
    request(&mut rt, |tx| tx.scroll_by("scroll", 0.0, 350.0));
    assert!(scroll(&rt).pinned());
    let end = scroll(&rt).offset;
    assert_eq!(end, scroll(&rt).max);
    rt.update(|s, tx| {
        s.rows += 1;
        tx.instant();
    });
    rt.frame(STEP);
    assert_eq!(scroll(&rt).offset, scroll(&rt).max);
    assert!(scroll(&rt).offset > end, "follow the new end");
}

/// Two keys for one viewport accumulate, with one clamp on their total.
#[test]
fn scroll_by_adds_distances_for_the_same_viewport_before_clamping() {
    let mut rt = runtime(true, false);
    request(&mut rt, |tx| {
        tx.scroll_by("scroll", 40.0, -20.0);
        tx.scroll_by("content", 20.0, 70.0);
    });
    assert_eq!(rt.state.pos.get().offset, [60.0, 50.0]);
}

/// Stop a running two-axis glide and move from the offset it had already drawn.
#[test]
fn scroll_by_after_stop_scroll_moves_from_rest_in_the_same_frame() {
    let mut rt = runtime(true, false);
    request(&mut rt, |tx| tx.scroll_to("scroll", 600.0, 600.0));
    for _ in 0..4 {
        rt.frame(STEP);
    }
    assert!(scroll(&rt).glide().is_some() && scroll(&rt).x.carried());
    let before = rt.state.pos.get().offset;
    request(&mut rt, |tx| {
        tx.stop_scroll("content");
        tx.scroll_by("scroll", 12.0, 60.0);
    });
    let after = rt.state.pos.get().offset;
    assert!((after[0] - before[0] - 12.0).abs() <= HALF_PIXEL);
    assert!((after[1] - before[1] - 60.0).abs() <= HALF_PIXEL);
    rt.frame(STEP);
    assert!(!scroll(&rt).moving());
    assert_eq!(rt.state.pos.get().offset, after);
}

/// The request also moves plain scroll content, right and down as reported to the app.
#[test]
fn scroll_by_moves_a_plain_viewport_on_both_axes_and_ignores_disabled_axes() {
    let mut rt = runtime(true, false);
    request(&mut rt, |tx| tx.scroll_by("content", 35.0, 65.0));
    assert_eq!(rt.state.pos.get().offset, [35.0, 65.0]);
    let rect = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("content"))
        .unwrap()
        .rect;
    assert_eq!([rect[0], rect[1]], [-35.0, -65.0]);

    let mut rt = runtime(false, false);
    request(&mut rt, |tx| tx.scroll_by("scroll", 35.0, 65.0));
    assert_eq!(rt.state.pos.get().offset, [0.0, 65.0]);
}

/// A landed request adds no delayed wake or repeated virtual-list work.
#[test]
fn scroll_by_leaves_a_virtual_viewport_idle_with_no_more_commits_or_layout() {
    let mut rt = middle();
    request(&mut rt, |tx| tx.scroll_by("scroll", 0.0, -120.0));
    assert_eq!(rt.wake_deadline(), None);
    let (commits, layouts) = (rt.stats.commits, rt.stats.layout_nodes);
    for _ in 0..5 {
        assert!(!rt.frame(STEP));
        assert!(!scroll(&rt).moving());
        assert_eq!(rt.wake_deadline(), None);
        assert_eq!(
            (rt.stats.commits, rt.stats.layout_nodes),
            (commits, layouts)
        );
    }
}
