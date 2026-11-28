//! Commit an unrelated counter while an idle text field sits in another region.
//!
//! `cargo run --locked --profile perf -p rune --features frame-profile --example region_focus`
//!
//! Headless: the commit phase includes view, layout and apply, but no GPU upload or presentation.

use std::cell::Cell;

use rune::{
    App, AppSpec, Cx, El, Env, TextEdit, TextStyle, Tracked, col, memo, row, text, text_input,
};

const SIZE: (f32, f32) = (560.0, 480.0);
const FRAMES: usize = 120;
const ROWS: usize = 300;
const RUNS: usize = 5;

enum Editor {
    Tracked(Tracked<TextEdit>),
    Plain(TextEdit),
}

impl Editor {
    fn read(&self) -> &TextEdit {
        match self {
            Self::Tracked(edit) => edit,
            Self::Plain(edit) => edit,
        }
    }

    fn access(&mut self) -> &mut TextEdit {
        match self {
            Self::Tracked(edit) => edit,
            Self::Plain(edit) => edit,
        }
    }

    fn version(&self) -> Option<u64> {
        match self {
            Self::Tracked(edit) => Some(Tracked::version(edit)),
            Self::Plain(_) => None,
        }
    }
}

struct State {
    editor: Editor,
    counter: Tracked<usize>,
    field_builds: Cell<usize>,
    counter_builds: Cell<usize>,
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let font = TextStyle::new(14.0, 400.0);
    row()
        .w_full()
        .h_full()
        .gap(20.0)
        .child(memo::region("field-and-rows", || {
            s.field_builds.set(s.field_builds.get() + 1);
            col()
                .w(360.0)
                .child(
                    text_input(s.editor.read(), font)
                        .key("field")
                        .size(320.0, 28.0)
                        .on_edit(|s: &mut State| s.editor.access()),
                )
                .children((0..ROWS).map(|i| text(format!("Row {i}")).font(font)))
        }))
        .child(memo::region("counter", || {
            s.counter_builds.set(s.counter_builds.get() + 1);
            text(format!("Counter: {}", *s.counter)).font(font)
        }))
}

const CASES: [(&str, bool, bool); 3] = [
    ("tracked, unfocused", true, false),
    ("tracked, focused", true, true),
    ("plain, focused", false, true),
];

fn sample(case: usize) -> (String, f64) {
    let (_, tracked, focused) = CASES[case];
    let spec = AppSpec::new("Region focus", SIZE, State::default, view);
    let mut rt = App::new(spec)
        .init(move || {
            let mut edit = TextEdit::new("A text field");
            edit.focused = focused;
            State {
                editor: if tracked {
                    Editor::Tracked(Tracked::new(edit))
                } else {
                    Editor::Plain(edit)
                },
                counter: Tracked::new(0),
                field_builds: Cell::new(0),
                counter_builds: Cell::new(0),
            }
        })
        .headless_runtime(SIZE, Env::new(true, 1.0));
    rt.frame(0.0);
    let initial = (
        rt.state.field_builds.replace(0),
        rt.state.counter_builds.replace(0),
    );
    let version = rt.state.editor.version();
    let _ = rune::take_frame_phases();
    let mut times = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        let commits = rt.stats.commits;
        rt.update(|s, tx| {
            *s.counter += 1;
            tx.instant();
        });
        rt.frame(1.0 / 120.0);
        // Wall milliseconds for the commit phase, the second of six wall-time columns.
        times.push(rune::take_frame_phases()[1]);
        assert_eq!(rt.stats.commits - commits, 1);
        assert_eq!(rt.presented_focus_key(), focused.then_some("field"));
    }
    let field = rt.state.field_builds.get();
    let counter = rt.state.counter_builds.get();
    let bumps = rt
        .state
        .editor
        .version()
        .zip(version)
        .map(|(after, before)| after - before);
    (
        format!(
            "initial={initial:?} field={field} counter={counter} editor_version_bumps={bumps:?}"
        ),
        median(&mut times),
    )
}

impl Default for State {
    fn default() -> Self {
        Self {
            editor: Editor::Plain(TextEdit::default()),
            counter: Tracked::new(0),
            field_builds: Cell::new(0),
            counter_builds: Cell::new(0),
        }
    }
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    (values[(values.len() - 1) / 2] + values[values.len() / 2]) * 0.5
}

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("run with --profile perf; debug builds re-run kept regions");
        std::process::exit(1);
    }
    let mut times = [Vec::new(), Vec::new(), Vec::new()];
    let mut counts = [String::new(), String::new(), String::new()];
    for run in 0..=RUNS {
        let order = if run % 2 == 0 { [0, 1, 2] } else { [2, 1, 0] };
        for case in order {
            let (count, ms) = sample(case);
            if run != 0 {
                if run == 1 {
                    counts[case] = count;
                } else {
                    assert_eq!(count, counts[case]);
                }
                times[case].push(ms);
            }
        }
    }
    println!(
        "{ROWS} rows, {FRAMES} counter commits per run; {RUNS} alternating runs after warm-up"
    );
    for (case, (name, ..)) in CASES.iter().enumerate() {
        let ms = median(&mut times[case].clone());
        println!(
            "{name}: {}; commit p50={ms:.3} ms (run p50s {:.3?})",
            counts[case], times[case]
        );
    }
}
