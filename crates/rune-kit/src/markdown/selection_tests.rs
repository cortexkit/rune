use rune::{Cx, Env, Key, KeyEvent, Mods, Runtime};

use super::*;

const COPY_DOC: &str = "# Title\n\nSome **bold** text.\n\n- first\n- second\n\n```\ncode\n```\n";

#[test]
fn markdown_selection_preserves_paths_layout_and_copy() {
    fn view(&(copy, track): &(bool, bool), _: &mut Cx<(bool, bool)>) -> El {
        col().padding(10.0).child(Markdown::new(COPY_DOC).selectable(true).track_source(track).copy_source(copy)
            .render(&MarkdownStyle::neutral(true)).key("md"))
    }
    for copy_source in [false, true] {
        let mut rt = Runtime::new((copy_source, true), view, (480.0, 600.0), Env::new(true, 1.0));
        let mut plain = Runtime::new((copy_source, false), view, (480.0, 600.0), Env::new(true, 1.0));
        let clipboard = rune::clipboard::memory();
        rt.use_clipboard(&clipboard);
        rt.frame(0.0);
        plain.frame(0.0);
        assert_eq!(rt.instances.len(), plain.instances.len());
        for (a, b) in rt.instances.iter().zip(plain.instances.iter()) {
            assert_eq!((a.rect, a.fill, a.border, a.params), (b.rect, b.fill, b.border, b.params));
        }
        assert_eq!(rt.elements().iter().map(|e| e.rect).collect::<Vec<_>>(), plain.elements().iter().map(|e| e.rect).collect::<Vec<_>>());
        rt.update(|_, tx| tx.select_all_text("md"));
        rt.frame(1.0 / 60.0);
        rt.key(KeyEvent { key: Key::Command('c'), mods: Mods { cmd: true, ..Mods::default() } });
        rt.frame(1.0 / 60.0);
        assert_eq!(clipboard.text().as_deref().map(String::as_str), Some(if copy_source {
            COPY_DOC
        } else {
            "Title\n\nSome bold text.\n\n- first\n- second\n\ncode"
        }));
        let paths: Vec<_> = rt.elements().into_iter().map(|e| e.path).collect();
        assert_eq!(paths, [
            "#0", "#0/md", "#0/md/b0", "#0/md/b1", "#0/md/b2", "#0/md/b2/i0",
            "#0/md/b2/i0/#0", "#0/md/b2/i0/#1", "#0/md/b2/i0/#1/b0", "#0/md/b2/i1",
            "#0/md/b2/i1/#0", "#0/md/b2/i1/#1", "#0/md/b2/i1/#1/b0", "#0/md/b3", "#0/md/b3/#0",
        ]);
    }
}

fn md(src: &str) -> Markdown {
    Markdown::new(src).selectable(true).track_source(true)
}

fn drawn(src: &'static str) -> Runtime<&'static str> {
    let mut rt = Runtime::new(src, |&src, _: &mut Cx<&str>| {
        let style = MarkdownStyle::neutral(true).selection(Color { r: 1.0, g: 0.0, b: 1.0, a: 1.0 });
        col().padding(10.0).child(md(src).render(&style).key("md"))
    }, (480.0, 600.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

fn text_piece<T: 'static>(rt: &Runtime<T>, within: &str, needle: &str) -> (String, Range<usize>) {
    let e = rt.elements().into_iter().find(|e| {
        e.path.contains(within) && e.text.as_deref().is_some_and(|t| t.contains(needle))
    }).unwrap_or_else(|| panic!("no {needle:?} in {within:?}"));
    let at = e.text.unwrap().find(needle).unwrap();
    (e.path, at..at + needle.len())
}

fn select<T: 'static>(rt: &mut Runtime<T>, (path, range): (String, Range<usize>)) -> rune::TextSelection {
    rt.update(|_, tx| tx.select_text(path, range));
    rt.frame(1.0 / 60.0);
    rt.selection().unwrap()
}

fn assert_source(src: &str, selected: &rune::TextSelection, root: &str, start: usize, want: &str) {
    let range = selection_source(src, selected, root).unwrap();
    assert_eq!(range, start..start + want.len());
    assert_eq!(&src[range], want);
}

#[test]
fn selection_source_maps_inline_syntax_without_searching_for_words() {
    for (src, shown, want) in [
        ("Find a word here", "word", "word"),
        ("Stay **quite bold** at noon", "bold at", "bold** at"),
        ("A [green path](https://example.com) here", "een pa", "een pa"),
        ("A link to [home](https://example.com) here", "home here", "home](https://example.com) here"),
        ("A ~~gone~~ word", "gone", "gone"),
        ("Say `cargo test` then stop", "cargo", "cargo"),
        ("A &amp; B", "&", "&amp;"),
        ("A \\* B", "*", "\\*"),
        ("שלום `code` עולם", "עולם", "עולם"),
    ] {
        let mut rt = drawn(src);
        let part = text_piece(&rt, "/md/", shown);
        let selected = select(&mut rt, part);
        assert_source(src, &selected, "md", src.find(want).unwrap(), want);
    }
}

#[test]
fn selection_source_maps_repeated_list_items_and_table_cells_by_occurrence() {
    let src = "- same\n- same\n  - same\n\n| left | right |\n|------|-------|\n| same | same  |";
    let mut rt = drawn(src);
    let starts: Vec<_> = src.match_indices("same").map(|(a, _)| a).collect();
    for (within, at) in [
        ("/i0/#1/b0", starts[0]),
        ("/i1/#1/b0", starts[1]),
        ("/i1/#1/b1/i0/#1/b0", starts[2]),
        ("/r0/c0", starts[3]),
        ("/r0/c1", starts[4]),
    ] {
        let part = text_piece(&rt, within, "same");
        let selected = select(&mut rt, part);
        assert_source(src, &selected, "md", at, "same");
    }
    let part = text_piece(&rt, "/th/c1", "right");
    let selected = select(&mut rt, part);
    assert_source(src, &selected, "md", src.find("right").unwrap(), "right");
}

fn drag_between(rt: &mut Runtime<&'static str>, a: (String, Range<usize>), b: (String, Range<usize>)) -> rune::TextSelection {
    let caret_side = |rt: &mut Runtime<&'static str>, part, fraction| {
        select(rt, part);
        let rects: Vec<_> = rt.instances.iter().filter(|i| i.fill == [1.0, 0.0, 1.0, 1.0]).map(|i| i.rect).collect();
        assert_eq!(rects.len(), 1);
        let [x, y, w, h] = rects[0];
        (x + fraction * w, y + h / 2.0)
    };
    let (x0, y0) = caret_side(rt, a, 0.2);
    let (x1, y1) = caret_side(rt, b, 0.8);
    rt.update(|_, tx| tx.clear_text_selection());
    rt.frame(1.0 / 60.0);
    rt.pointer_move(x0, y0);
    rt.pointer_down(x0, y0);
    rt.frame(1.0 / 60.0);
    for i in 1..=6 {
        let f = i as f32 / 6.0;
        rt.pointer_move(x0 + (x1 - x0) * f, y0 + (y1 - y0) * f);
        rt.frame(1.0 / 60.0);
    }
    rt.pointer_up(x1, y1);
    rt.frame(1.0 / 60.0);
    rt.selection().unwrap()
}

#[test]
fn selection_source_spans_paragraphs_and_cells() {
    for (src, left, right, from, to, want) in [
        ("Start here.\n\nEnd over there.", "/b0", "/b1", "h", "v", "here.\n\nEnd ov"),
        ("| A | B |\n|---|---|\n| left | right |", "/r0/c0", "/r0/c1", "e", "g", "eft | rig"),
    ] {
        let mut rt = drawn(src);
        let a = text_piece(&rt, left, from);
        let b = text_piece(&rt, right, to);
        let selected = drag_between(&mut rt, a, b);
        assert_eq!(selected.texts().len(), 2);
        assert_source(src, &selected, "md", src.find(want).unwrap(), want);
    }
}

#[test]
fn selection_source_maps_fences_and_ignores_inline_code_padding() {
    let src = "Run `cargo` here.\n\n```\nold\nword\n```\n\n```diff\n-old\n+new\n```";
    let mut rt = drawn(src);
    let code = text_piece(&rt, "/b1/", "word");
    let selected = select(&mut rt, code);
    assert_source(src, &selected, "md", src.find("word").unwrap(), "word");
    let diff = text_piece(&rt, "/l1/", "new");
    let selected = select(&mut rt, diff);
    assert_source(src, &selected, "md", src.find("new").unwrap(), "new");
    let (path, pad) = text_piece(&rt, "/b0", PAD);
    rt.update(|_, tx| tx.select_text(path, pad));
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.selection().and_then(|s| selection_source(src, &s, "md")), None);
    let (path, mut whole) = text_piece(&rt, "/b0", "cargo");
    whole.end += PAD.len();
    let selected = select(&mut rt, (path, whole));
    assert_source(src, &selected, "md", src.find("cargo").unwrap(), "cargo");
}

#[test]
fn selection_source_is_scoped_to_the_actual_view_root() {
    let src = "A word here";
    let mut rt = Runtime::new(src, |&src, _: &mut Cx<&str>| {
        let view = || md(src).copy_source(true).render(&MarkdownStyle::neutral(true)).selection_group(false);
        col().key("container").selection_group(true)
            .child(view().key("b0"))
            .child(view().key("other/view"))
            .child(text("outside").selectable(true).key("outside"))
    }, (480.0, 600.0), Env::new(true, 1.0));
    rt.frame(0.0);
    assert!(rt.selection().is_none());
    assert_eq!(selection_source(src, &rune::TextSelection::default(), "b0"), None);
    let part = text_piece(&rt, "/#!other%2Fview/", "word");
    let selected = select(&mut rt, part);
    for root in ["other/view", "container/#!other%2Fview"] {
        assert_source(src, &selected, root, 2, "word");
    }
    for root in ["b0", "container", "view"] {
        assert_eq!(selection_source(src, &selected, root), None);
    }
    assert_eq!(selection_source("A changed page", &selected, "other/view"), None);
    let outside = text_piece(&rt, "outside", "outside");
    let selected = select(&mut rt, outside);
    assert_eq!(selection_source(src, &selected, "b0"), None);
}

#[test]
fn selection_source_uses_the_nearest_root_inside_a_custom_frame() {
    let src = "> Host";
    let mut rt = Runtime::new(src, |&src, _: &mut Cx<&str>| {
        md(src).quote_frame(|q, _| {
            col().child(q).child(md("Child word").copy_source(true).render(&MarkdownStyle::neutral(true))
                .key("inner").selection_group(false))
        }).render(&MarkdownStyle::neutral(true)).key("outer")
    }, (480.0, 600.0), Env::new(true, 1.0));
    rt.frame(0.0);
    let part = text_piece(&rt, "/inner/", "word");
    let selected = select(&mut rt, part);
    assert_source("Child word", &selected, "inner", 6, "word");
    assert_eq!(selection_source(src, &selected, "outer"), None);
    rt.update(|_, tx| tx.select_all_text("outer"));
    rt.frame(1.0 / 60.0);
    let selected = rt.selection().unwrap();
    assert_eq!(selected.text(), "Host\n\nChild word");
    assert_source(src, &selected, "outer", 2, "Host");
    assert_source("Child word", &selected, "inner", 0, "Child word");
}

#[test]
fn selection_source_survives_append_and_uses_the_stream_parse() {
    let open = "Earlier\n\nHere is **bold";
    let closed = "Earlier\n\nHere is **bold text** now";
    let mut rt = Runtime::new((open, true), |&(src, stream), _: &mut Cx<(&str, bool)>| {
        md(src).streaming(stream).render(&MarkdownStyle::neutral(true)).key("stream")
    }, (480.0, 600.0), Env::new(true, 1.0));
    rt.frame(0.0);
    let part = text_piece(&rt, "stream/b1", "bold");
    let s = select(&mut rt, part);
    assert_source(open, &s, "stream", open.find("bold").unwrap(), "bold");
    rt.update(|state, _| *state = (closed, false));
    rt.frame(1.0 / 60.0);
    let s = rt.selection().expect("append keeps the selected content and path");
    assert_source(closed, &s, "stream", closed.find("bold").unwrap(), "bold");
}
