use crate::{
    Color, Cx, El, Env, GrowthSpec, Ime, Runtime, ScrollSpec, TextEdit, TextStyle, col, el,
    text_input,
};

const SIBLINGS: usize = 300;
const INK: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.5,
    a: 1.0,
};
const OTHER_INK: Color = Color {
    r: 0.0,
    g: 1.0,
    b: 0.5,
    a: 1.0,
};

struct Fields {
    first: TextEdit,
    second: TextEdit,
}

fn fields() -> Fields {
    let mut first = TextEdit::new("first");
    first.focused = true;
    Fields {
        first,
        second: TextEdit::new("second"),
    }
}

fn editor(edit: &TextEdit, key: &'static str, ink: Color) -> El {
    text_input(edit, TextStyle::new(14.0, 400.0).lh(20.0))
        .key(key)
        .size(120.0, 24.0)
        .label(key)
        .caret(ink, ink.alpha(0.3))
}

fn first(s: &Fields) -> El {
    editor(&s.first, "first", INK).on_edit(|s: &mut Fields| &mut s.first)
}

fn mounted(s: &Fields, _: &mut Cx<Fields>) -> El {
    col()
        .w_full()
        .h_full()
        .child(first(s))
        .children((0..SIBLINGS).map(|i| {
            el().key(format!("sibling-{i}"))
                .size(120.0, 1.0)
                .fill(Color::WHITE)
        }))
}

fn two_fields(s: &Fields, _: &mut Cx<Fields>) -> El {
    col()
        .child(first(s))
        .child(editor(&s.second, "second", OTHER_INK).on_edit(|s: &mut Fields| &mut s.second))
}

fn runtime(view: fn(&Fields, &mut Cx<Fields>) -> El) -> Runtime<Fields> {
    let mut rt = Runtime::new(fields(), view, (200.0, 360.0), Env::new(true, 1.0));
    rt.set_srgb_blend(false);
    rt
}

fn drawn(rt: &Runtime<Fields>, ink: Color) -> bool {
    let want = ink.premul(1.0);
    rt.instances
        .iter()
        .any(|i| i.fill.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-3))
}

fn wake_after(rt: &mut Runtime<Fields>, now: f64, due: f64) {
    let wake = rt
        .wake_deadline()
        .expect("the caret should schedule its next blink");
    assert!(
        (wake - due).abs() < 1e-6,
        "expected wake at {due}, got {wake}"
    );
    rt.frame(wake - now);
}

#[test]
fn blink_repaints_the_field_but_keeps_its_siblings() {
    let mut rt = runtime(mounted);
    rt.core_mut().paint.keep.force_copy = true;
    rt.window_focus(true);
    rt.frame(0.0);
    rt.frame(0.25);
    assert!(drawn(&rt, INK));
    let still_kept = rt.core().paint.keep.last_kept;
    let commits = rt.stats.commits;

    rt.frame(0.25);
    assert!(
        !drawn(&rt, INK),
        "the caret must disappear even when copying kept paint"
    );
    assert_eq!(
        still_kept, SIBLINGS,
        "the unchanged frame keeps every sibling but not the field"
    );
    assert_eq!(rt.core().paint.keep.last_kept, still_kept);
    wake_after(&mut rt, 0.5, 1.0);
    assert!(drawn(&rt, INK), "the caret comes back on the second wake");
    assert_eq!(rt.core().paint.keep.last_kept, still_kept);
    assert_eq!(
        rt.stats.commits, commits,
        "blink frames do not rebuild the tree"
    );
}

#[test]
fn caret_blinks_in_a_mounted_virtual_row() {
    let mut rt = runtime(|_, _| {
        col()
            .size(180.0, 160.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .clip_soft(0.0)
            .child(
                col()
                    .key("rows")
                    .w_full()
                    .growth(GrowthSpec::default())
                    .virtual_rows(
                        100,
                        24.0,
                        |_: &Fields, i| format!("row-{i}").into(),
                        |s: &Fields, i| {
                            if i == 0 {
                                col().size(120.0, 24.0).child(first(s))
                            } else {
                                el().size(120.0, 24.0)
                            }
                        },
                    ),
            )
    });
    rt.window_focus(true);
    rt.frame(0.0);
    assert!(
        rt.find("row-0/first").is_some(),
        "the input is inside an index-built row"
    );
    assert!(drawn(&rt, INK));
    wake_after(&mut rt, 0.0, 0.5);
    assert!(!drawn(&rt, INK));
    wake_after(&mut rt, 0.5, 1.0);
    assert!(drawn(&rt, INK));
}

#[test]
fn caret_blinks_in_a_modal_without_paint_keep() {
    let mut rt = runtime(|s, _| col().child(col().key("modal").modal_trap().child(first(s))));
    rt.window_focus(true);
    rt.frame(0.0);
    assert!(
        rt.core().kbd.modal_scope.is_some(),
        "this field is in the active modal"
    );
    assert!(drawn(&rt, INK));
    wake_after(&mut rt, 0.0, 0.5);
    assert!(!drawn(&rt, INK));
    assert_eq!(rt.core().paint.keep.last_kept, 0);
    wake_after(&mut rt, 0.5, 1.0);
    assert!(drawn(&rt, INK));
    assert_eq!(rt.core().paint.keep.last_kept, 0);
}

#[test]
fn a_blurred_hidden_caret_leaves_no_wake_and_refocus_starts_shown() {
    for next in ["first", "second"] {
        let mut rt = runtime(two_fields);
        rt.window_focus(true);
        rt.frame(0.0);
        wake_after(&mut rt, 0.0, 0.5);
        assert!(!drawn(&rt, INK));
        rt.update(|_, tx| tx.blur());
        rt.frame(0.125);
        assert!(!drawn(&rt, INK) && !drawn(&rt, OTHER_INK));
        assert_eq!(
            rt.wake_deadline(),
            None,
            "a window with no focused field rests"
        );

        rt.update(move |_, tx| tx.focus(next, false));
        rt.frame(0.125);
        assert_eq!(
            drawn(&rt, INK),
            next == "first",
            "the first field paints only if refocused"
        );
        assert_eq!(
            drawn(&rt, OTHER_INK),
            next == "second",
            "another field starts shown"
        );
        assert_eq!(rt.wake_deadline(), Some(1.25));
    }
}

#[test]
fn each_window_has_its_own_blink_clock_and_focus() {
    let (mut left, mut right) = (runtime(two_fields), runtime(two_fields));
    left.window_focus(true);
    right.window_focus(false);
    left.frame(0.0);
    right.frame(0.0);
    wake_after(&mut left, 0.0, 0.5);
    right.frame(0.5);
    assert!(!drawn(&left, INK));
    assert!(
        drawn(&right, INK),
        "the inactive window's field holds still"
    );
    assert_eq!(right.wake_deadline(), None);
    assert!(!left.core().stale() && !right.core().stale());

    left.window_focus(false);
    assert!(
        left.core().stale(),
        "blur needs to repaint the hidden caret"
    );
    left.frame(0.125);
    right.window_focus(true);
    assert!(
        right.core().stale(),
        "focus alone must wake the resting window for a paint"
    );
    right.frame(0.125);
    assert!(drawn(&left, INK) && drawn(&right, INK));
    assert_eq!(left.wake_deadline(), None);
    wake_after(&mut right, 0.625, 1.125);
    assert!(!drawn(&right, INK));
    assert!(drawn(&left, INK));
}

#[test]
fn window_focus_does_not_invalidate_a_window_without_a_caret_that_can_blink() {
    for reduced in [false, true] {
        let mut rt = runtime(two_fields);
        if reduced {
            rt.set_system_reduced_motion(true);
        } else {
            rt.state.first.focused = false;
        }
        rt.window_focus(false);
        rt.frame(0.0);
        assert!(!rt.core().stale());
        rt.window_focus(true);
        assert!(!rt.core().stale());
        assert_eq!(rt.wake_deadline(), None);
    }
}

#[test]
fn an_unfocused_window_keeps_the_caret_on_without_a_wake() {
    let mut rt = runtime(two_fields);
    rt.window_focus(false);
    rt.frame(0.0);
    assert_eq!(
        rt.wake_deadline(),
        None,
        "an unfocused window must not ask for a caret wake"
    );
    rt.frame(0.5);
    assert!(drawn(&rt, INK));
    assert_eq!(rt.wake_deadline(), None);
}

#[test]
fn a_window_without_a_focus_report_keeps_the_caret_on_without_a_wake() {
    let mut rt = runtime(two_fields);
    rt.frame(0.0);
    rt.frame(0.5);
    assert!(drawn(&rt, INK));
    assert_eq!(rt.wake_deadline(), None);
}

#[test]
fn reduced_motion_keeps_the_caret_on_without_a_wake() {
    let mut rt = runtime(two_fields);
    rt.window_focus(true);
    rt.frame(0.0);
    wake_after(&mut rt, 0.0, 0.5);
    assert!(!drawn(&rt, INK));

    rt.set_system_reduced_motion(true);
    rt.frame(0.125);
    assert!(drawn(&rt, INK));
    assert_eq!(rt.wake_deadline(), None);
}

#[test]
fn the_idle_limit_holds_the_caret_on_without_a_wake() {
    let mut rt = runtime(two_fields);
    rt.window_focus(true);
    rt.frame(0.0);
    rt.frame(59.5);
    assert!(!drawn(&rt, INK));
    wake_after(&mut rt, 59.5, 60.0);
    assert!(drawn(&rt, INK));
    assert_eq!(rt.wake_deadline(), None);
}

#[test]
fn selection_and_ime_restart_the_caret_without_changing_blink_geometry() {
    thread_local! {
        static VIEW_CARET: std::cell::Cell<Option<[f32; 4]>> = const { std::cell::Cell::new(None) };
    }
    let mut rt = runtime(|s, cx| {
        VIEW_CARET.set(cx.caret("first"));
        two_fields(s, cx)
    });
    rt.window_focus(true);
    rt.frame(0.0);
    let caret = |rt: &Runtime<Fields>| {
        rt.elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("first"))
            .unwrap()
            .caret
    };
    let shown = caret(&rt).expect("the focused field reports its caret");
    wake_after(&mut rt, 0.0, 0.5);
    assert!(!drawn(&rt, INK));
    rt.update(|_, _| {});
    rt.frame(0.0);
    assert_eq!(
        caret(&rt),
        Some(shown),
        "ElementInfo still reports a hidden caret"
    );
    assert_eq!(
        VIEW_CARET.get(),
        Some(shown),
        "Cx still reports a hidden caret"
    );

    let at = rt.state.first.focus;
    rt.update(|s, _| s.first.set_selection(0, 1));
    rt.frame(0.125);
    assert!(!drawn(&rt, INK), "range selection takes the caret's place");
    assert_eq!(rt.wake_deadline(), None);
    rt.update(move |s, _| s.first.set_caret(at));
    rt.frame(0.125);
    assert!(
        drawn(&rt, INK),
        "collapsing a selection restarts even at the old position"
    );
    wake_after(&mut rt, 0.75, 1.25);
    assert!(!drawn(&rt, INK));
    rt.ime(Ime::Preedit("かな".into(), Some((6, 6)), Vec::new()));
    rt.frame(0.125);
    assert!(drawn(&rt, INK));
    assert_eq!(rt.wake_deadline(), Some(1.875));
}
