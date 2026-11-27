use std::borrow::Cow;

use rune::{
    App, AppSpec, Color, Cx, El, Env, GrowthSpec, Key, KeyEvent, LiveText, Runtime, ScrollSpec,
    TextEdit, TextStyle, Tx, col, el, memo, region_with, row, text, text_input,
};

const SIZE: (f32, f32) = (1728.0, 1080.0);
const COLUMNS: usize = 3;
const ROW_HEIGHT: f32 = 174.0;
const CARD_COUNTS: [usize; 4] = [57, 174, 405, 576];
const SAMPLE_COUNT: usize = 200;
const LABEL: &str = "Field notes";
const EMPTY: &str = "Draft";
const FILLED: &str = "Draftx";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Case {
    Plain,
    Memo,
    Region,
    Virtual,
    VirtualNone,
    VirtualVersioned,
    Cache,
    Live,
}

impl Case {
    const ALL: [Self; 8] = [
        Self::Plain,
        Self::Memo,
        Self::Region,
        Self::Virtual,
        Self::VirtualNone,
        Self::VirtualVersioned,
        Self::Cache,
        Self::Live,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Memo => "memo",
            Self::Region => "region",
            Self::Virtual => "virtual",
            Self::VirtualNone => "virtual-none",
            Self::VirtualVersioned => "virtual-versioned",
            Self::Cache => "cache",
            Self::Live => "live-display",
        }
    }

    fn is_virtual(self) -> bool {
        matches!(
            self,
            Self::Virtual | Self::VirtualNone | Self::VirtualVersioned
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    Original,
    NonModal,
    Modal,
    GridInert,
    Closed,
}

struct State {
    case: Case,
    overlay: Overlay,
    row_keys: Vec<String>,
    edit: TextEdit,
    display_value: String,
    live_text: LiveText,
}

impl State {
    fn new(case: Case, cards: usize, overlay: Overlay) -> Self {
        assert!(cards.is_multiple_of(COLUMNS));
        Self {
            case,
            overlay,
            row_keys: (0..cards / COLUMNS).map(|i| format!("row-{i}")).collect(),
            edit: TextEdit::new(EMPTY),
            display_value: EMPTY.into(),
            live_text: LiveText::new(EMPTY),
        }
    }
}

fn chip(label: &str, color: Color) -> El {
    row()
        .h(23.0)
        .px(7.0)
        .gap(5.0)
        .items_center()
        .radius(6.0)
        .fill(Color::WHITE.alpha(0.08))
        .child(el().size(7.0, 7.0).radius(3.5).fill(color))
        .child(
            text(label)
                .font(TextStyle::new(12.0, 500.0))
                .color(Color::WHITE),
        )
}

fn card(index: usize) -> El {
    let muted = Color::WHITE.alpha(0.72);
    let accent = rune::oklch(0.75, 0.13, 177.0);
    let progress = (1 + index * 13 % 30) as f32 * 4.0;
    col()
        .grow(1.0)
        .h(162.0)
        .padding(14.0)
        .gap(5.0)
        .radius(9.0)
        .fill(Color::WHITE.alpha(0.055))
        .border(1.0, Color::WHITE.alpha(0.10))
        .child(
            text(format!("Survey plot {:03}", index + 1))
                .font(TextStyle::new(18.0, 650.0))
                .color(Color::WHITE),
        )
        .children(
            [
                "Transect follows the northern edge.",
                "Soil stays damp under the canopy.",
                "Return after the next rainfall.",
            ]
            .map(|line| text(line).font(TextStyle::new(14.0, 400.0)).color(muted)),
        )
        .child(row().gap(6.0).children([
            chip("North", accent),
            chip("Woodland", rune::oklch(0.76, 0.12, 90.0)),
            chip("Review", rune::oklch(0.77, 0.11, 262.0)),
        ]))
        .child(
            row()
                .size(120.0, 4.0)
                .radius(2.0)
                .fill(Color::WHITE.alpha(0.12))
                .child(el().size(progress, 4.0).radius(2.0).fill(accent)),
        )
}

fn card_row(s: &State, index: usize) -> El {
    row()
        .key(&s.row_keys[index])
        .w_full()
        .h(ROW_HEIGHT)
        .px(12.0)
        .pt(12.0)
        .gap(12.0)
        .children(
            (index * COLUMNS..(index + 1) * COLUMNS).map(|i| match s.case {
                Case::Memo => memo(format!("card-{i}"), 0, || card(i)),
                Case::Cache => card(i).key(format!("card-{i}")).cache(),
                _ => card(i).key(format!("card-{i}")),
            }),
        )
}

fn grid(s: &State) -> El {
    let stack = col().key("rows").w_full();
    let rows = if s.case.is_virtual() {
        let rows = stack.growth(GrowthSpec::default()).virtual_rows(
            s.row_keys.len(),
            ROW_HEIGHT,
            |s: &State, i| Cow::Borrowed(&s.row_keys[i]),
            card_row,
        );
        match s.case {
            Case::VirtualNone => rows.virtual_overscan(0.0, 0.0),
            Case::VirtualVersioned => rows.virtual_rows_version(0),
            _ => rows,
        }
    } else {
        stack.children((0..s.row_keys.len()).map(|i| card_row(s, i)))
    };
    let viewport = col()
        .key("grid")
        .w_full()
        .h_full()
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(rows);
    if s.overlay == Overlay::GridInert {
        viewport.inert()
    } else {
        viewport
    }
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let grid = if s.case == Case::Region {
        // The region depends on the set of rows and the grid's input policy, never the edit.
        region_with(
            "static-grid",
            (s.row_keys.len(), s.overlay == Overlay::GridInert),
            || grid(s),
        )
    } else {
        grid(s)
    };
    let mut root = col()
        .w_full()
        .h_full()
        .fill(rune::oklch(0.22, 0.02, 248.0))
        .child(grid);
    if s.overlay == Overlay::Closed {
        return root;
    }
    if s.overlay != Overlay::Original {
        // The scrim looks the same in every comparison; only the input policy changes.
        root = root.child(
            el().key("scrim")
                .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .fill(Color::BLACK.alpha(0.35))
                .inert(),
        );
    }
    let style = TextStyle::new(16.0, 450.0);
    let field = if s.case == Case::Live {
        text(EMPTY).font(style).live_text(&s.live_text)
    } else {
        text_input(&s.edit, style)
            .single_line()
            .on_edit(|s: &mut State| &mut s.edit)
    };
    let mut panel = col()
        .key("overlay")
        .absolute(48.0, 38.0)
        .w(340.0)
        .padding(12.0)
        .gap(5.0)
        .fill(rune::oklch(0.31, 0.022, 248.0))
        .radius(9.0)
        .child(
            text(LABEL)
                .font(TextStyle::new(13.0, 550.0))
                .color(Color::WHITE),
        )
        .child(
            field
                .key("field")
                .label(LABEL)
                .w_full()
                .h(36.0)
                .px(6.0)
                .fill(Color::BLACK.alpha(0.28))
                .radius(4.0)
                .color(Color::WHITE),
        );
    if s.overlay != Overlay::Original {
        panel = panel.role("dialog").label(LABEL);
    }
    if s.overlay == Overlay::Modal {
        panel = panel.modal(true, |s: &mut State| s.overlay = Overlay::Closed);
    }
    // The last child already paints on top. Explicit layer ordering would prevent paint reuse.
    root.child(panel)
}

fn live_key(s: &mut State, ev: &KeyEvent, tx: &mut Tx) -> bool {
    // This label has no editing behavior. Only the live binding reads the changing text.
    match &ev.key {
        Key::Text(value) => s.display_value.push_str(value),
        Key::Backspace => {
            s.display_value.pop();
        }
        _ => return false,
    }
    s.live_text.set(s.display_value.as_str());
    tx.live_only();
    true
}

fn queue_edit(rt: &mut Runtime<State>, insert: bool) {
    rt.key(KeyEvent::from(if insert {
        Key::Text("x".into())
    } else {
        Key::Backspace
    }));
}

fn clock() -> rune::ThreadCpuMark {
    rune::thread_cpu_mark().expect("benchmark needs a thread CPU clock")
}

fn elapsed_us(start: rune::ThreadCpuMark) -> f64 {
    rune::thread_cpu_since(start).expect("benchmark needs a thread CPU clock") / 1_000.0
}

fn median_p95(samples: &[f64]) -> (f64, f64) {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable_by(f64::total_cmp);
    let n = sorted.len();
    (
        (sorted[(n - 1) / 2] + sorted[n / 2]) * 0.5,
        sorted[(95 * n).div_ceil(100) - 1],
    )
}

fn mounted_and_visible(rt: &Runtime<State>) -> (usize, usize) {
    let elements = rt.elements();
    let visible = elements
        .iter()
        .filter(|e| {
            let [x, y, w, h] = e.visible;
            w > 0.0 && h > 0.0 && x < SIZE.0 && y < SIZE.1 && x + w > 0.0 && y + h > 0.0
        })
        .count();
    (elements.len(), visible)
}

fn bench(case: Case, cards: usize, overlay: Overlay, print_samples: bool) {
    let mut rt = Runtime::new(
        State::new(case, cards, overlay),
        view,
        SIZE,
        Env::new(true, 1.0),
    );
    let mark_frames = std::env::var_os("RUNE_PAINT_KEEP_LOG").is_some_and(|v| v != "0");
    let live = case == Case::Live;
    rt.on_key = live.then_some(live_key);
    rt.frame(0.0);
    if overlay != Overlay::Original {
        let elements = rt.elements();
        let scrim = elements
            .iter()
            .find(|e| e.key.as_deref() == Some("scrim"))
            .expect("mounted scrim");
        assert_eq!(
            scrim.rect,
            [0.0, 0.0, SIZE.0, SIZE.1],
            "scrim must cover the viewport"
        );
        let grid = elements
            .iter()
            .find(|e| matches!(e.key.as_deref(), Some("grid" | "static-grid")))
            .expect("mounted grid");
        assert_eq!(
            grid.inert,
            matches!(overlay, Overlay::Modal | Overlay::GridInert)
        );
    }
    if !live {
        // Put the caret after the initial text. Time neither mounting nor focusing.
        let [x, y, w, h] = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("field"))
            .expect("mounted field")
            .rect;
        rt.pointer_down(x + w - 8.0, y + h * 0.5);
        rt.pointer_up(x + w - 8.0, y + h * 0.5);
        rt.frame(0.0);
        assert!(rt.has_focus(), "controlled field did not take focus");
    }
    for _ in 0..48 {
        rt.frame(1.0 / 60.0);
    }
    for i in 0..24 {
        queue_edit(&mut rt, i % 2 == 0);
        rt.frame(0.0);
        rt.frame(0.0);
    }

    let (mounted, visible) = mounted_and_visible(&rt);
    let mut edit_us = [0.0; SAMPLE_COUNT];
    let mut idle_us = [0.0; SAMPLE_COUNT];
    #[cfg(feature = "stages")]
    let mut phases_us = [[0.0; rune::PHASES]; SAMPLE_COUNT];
    let commits_before = rt.stats.commits;
    for i in 0..SAMPLE_COUNT {
        let insert = i % 2 == 0;
        let commits = rt.stats.commits;
        queue_edit(&mut rt, insert);
        #[cfg(feature = "stages")]
        rune::take_frame_phases();

        // Freeze simulated time so neither caret blink nor animation wakes count as idle.
        // The key is already queued; routing it and applying the edit are in Runtime::frame.
        if mark_frames {
            eprintln!("commit-cost frame: edit {i}");
        }
        let start = clock();
        rt.frame(0.0);
        edit_us[i] = elapsed_us(start);
        #[cfg(feature = "stages")]
        {
            let phases = rune::take_frame_phases();
            for (us, &ms) in phases_us[i].iter_mut().zip(&phases[rune::PHASES..]) {
                *us = ms * 1_000.0;
            }
        }

        let expected = if insert { FILLED } else { EMPTY };
        let value = if live {
            &rt.state.display_value
        } else {
            &rt.state.edit.text
        };
        assert_eq!(value, expected, "one edit per frame");
        if live {
            assert_eq!(rt.stats.commits, commits, "live label committed the view");
        } else {
            assert!(
                rt.stats.commits > commits,
                "controlled field did not commit"
            );
        }
        // Check the mounted text too, without walking the tree in every timed pair.
        if i == 0 || i == SAMPLE_COUNT - 1 {
            let field = rt
                .elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some("field"))
                .unwrap();
            assert_eq!(
                field.text.as_deref(),
                Some(expected),
                "rendered field text is stale"
            );
        }

        let commits = rt.stats.commits;
        if mark_frames {
            eprintln!("commit-cost frame: idle {i}");
        }
        let start = clock();
        let animating = rt.frame(0.0);
        idle_us[i] = elapsed_us(start);
        assert!(!animating, "idle frame still animating");
        assert_eq!(rt.stats.commits, commits, "idle frame committed the view");
    }
    assert_eq!(
        mounted_and_visible(&rt),
        (mounted, visible),
        "grid changed during edits"
    );

    let (edit50, edit95) = median_p95(&edit_us);
    let (idle50, idle95) = median_p95(&idle_us);
    let commits = rt.stats.commits - commits_before;
    let name = case.name();
    let per_element = edit50 / mounted as f64;
    println!(
        "{name},{cards},{mounted},{visible},{commits},{edit50:.3},{edit95:.3},{idle50:.3},{idle95:.3},{per_element:.6}"
    );

    #[cfg(feature = "stages")]
    {
        let names = [
            "view",
            "commit_inclusive",
            "layout_inclusive",
            "text",
            "paint",
            "apply",
        ];
        for (column, phase) in names.into_iter().enumerate() {
            let values: Vec<_> = phases_us.iter().map(|row| row[column]).collect();
            let (p50, p95) = median_p95(&values);
            println!("stage,{name},{cards},{phase},{p50:.3},{p95:.3}");
        }
        for (phase, values) in [
            (
                "commit_other",
                phases_us
                    .iter()
                    .map(|p| (p[1] - p[0] - p[2] - p[5]).max(0.0))
                    .collect::<Vec<_>>(),
            ),
            (
                "frame_other",
                phases_us
                    .iter()
                    .zip(edit_us)
                    .map(|(p, total)| (total - p[1] - p[4]).max(0.0))
                    .collect(),
            ),
        ] {
            let (p50, p95) = median_p95(&values);
            println!("stage,{name},{cards},{phase},{p50:.3},{p95:.3}");
        }
    }

    if print_samples {
        for (i, (edit, idle)) in edit_us.into_iter().zip(idle_us).enumerate() {
            println!("sample,{name},{cards},{i},{edit:.3},{idle:.3}");
        }
    }
}

fn main() {
    let mut selected_case = None;
    let mut cards = None;
    let mut overlay = Overlay::Original;
    let mut measure = false;
    let mut samples = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bench" => measure = true,
            "--samples" => samples = true,
            "--overlay" => {
                overlay = match args.next().as_deref() {
                    Some("nonmodal") => Overlay::NonModal,
                    Some("modal") => Overlay::Modal,
                    Some("grid-inert") => Overlay::GridInert,
                    _ => panic!("--overlay needs nonmodal, modal, or grid-inert"),
                };
            }
            "--case" => {
                let name = args.next().expect("--case needs a case name");
                selected_case = Some(
                    Case::ALL
                        .into_iter()
                        .find(|c| c.name() == name)
                        .expect("unknown case"),
                );
            }
            "--cards" => {
                cards = Some(
                    args.next()
                        .expect("--cards needs a count")
                        .parse::<usize>()
                        .expect("invalid count"),
                )
            }
            "--help" | "-h" => {
                println!(
                    "commit-cost [--bench [--samples]] [--case NAME] [--cards N] [--overlay MODE]\n\
                    Without --bench, opens a window. N must be a positive multiple of {COLUMNS}.\n\
                    Cases: plain, memo, region, virtual, virtual-none, virtual-versioned, cache, live-display.\n\
                    Overlay modes add a backdrop: nonmodal, modal, grid-inert. Default: no backdrop.\n\
                    Build benchmark with --profile perf, optionally with --features stages."
                );
                return;
            }
            _ => panic!("unrecognized argument {arg}; see --help"),
        }
    }
    if let Some(n) = cards {
        assert!(n > 0 && n.is_multiple_of(COLUMNS));
    }

    if measure {
        assert!(
            !cfg!(debug_assertions),
            "build the benchmark with --profile perf"
        );
        #[cfg(windows)]
        while rune::thread_cpu_ns().is_none() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        println!(
            "case,cards,mounted,visible,edit_commits,edit_p50_us,edit_p95_us,idle_p50_us,idle_p95_us,edit_p50_us_per_mounted"
        );
        let counts = if cards.is_some() {
            cards.as_slice()
        } else {
            &CARD_COUNTS
        };
        for n in counts {
            for case in Case::ALL
                .into_iter()
                .filter(|c| selected_case.is_none_or(|one| *c == one))
            {
                bench(case, *n, overlay, samples);
            }
        }
    } else {
        assert!(!samples, "--samples requires --bench");
        let case = selected_case.unwrap_or(Case::Plain);
        let mut spec = AppSpec::new(
            "Commit cost",
            SIZE,
            || State::new(Case::Plain, 57, Overlay::Original),
            view,
        );
        spec.on_key = (case == Case::Live).then_some(live_key);
        App::new(spec)
            .init(move || State::new(case, cards.unwrap_or(174), overlay))
            .args([])
            .run();
    }
}
