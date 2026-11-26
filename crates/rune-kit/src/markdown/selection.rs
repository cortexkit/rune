//! Source byte ranges of selections in drawn Markdown.

use std::ops::Range;
use std::rc::Rc;

use super::{Renderer, SpanMap};
use rune::{El, TextSelection};

/// The source bytes covered by a selection in a Markdown view drawn with
/// [`Markdown::selectable`](super::Markdown::selectable) and
/// [`Markdown::track_source`](super::Markdown::track_source), or `None` if the selection covers
/// no mapped character in that view. Pass the source that was drawn (including the current
/// tail of a stream).
///
/// `root` is the key on the element returned by [`Markdown::render`](super::Markdown::render)
/// or its whole element path. Use the whole path when the view is unkeyed or its key is not unique. The match is
/// against the end of each text's nearest source root path, as whole path segments; a key on
/// an enclosing container or a descendant is not the root. Ambiguous matches return `None`.
///
/// The range is one stretch of source in document order. Formatting and other Markdown syntax
/// between its endpoints are included; bytes with no source, such as padding around inline code,
/// contribute nothing to either endpoint.
pub fn selection_source(src: &str, selection: &TextSelection, root: &str) -> Option<Range<usize>> {
    let root_key = rune::path_segment(root);
    let ends_with = |path: &str, suffix: &str| {
        path == suffix || path.strip_suffix(suffix).is_some_and(|prefix| prefix.ends_with('/'))
    };
    let mut found = None;
    let mut bounds = None;
    for (path, source, range) in selection.texts().iter().filter_map(rune::SelectedText::source) {
        if !ends_with(path, root) && !ends_with(path, &root_key) { continue; }
        match found {
            None if source.as_ref() == src => found = Some((path, source)),
            Some((first_path, first_source)) if path == first_path && Rc::ptr_eq(source, first_source) => {}
            _ => return None,
        }
        src.get(range.clone())?;
        extend(&mut bounds, range);
    }
    bounds
}

fn extend(bounds: &mut Option<Range<usize>>, range: Range<usize>) {
    *bounds = Some(bounds.take().map_or_else(|| range.clone(), |b| b.start.min(range.start)..b.end.max(range.end)));
}

impl Renderer {
    pub(super) fn uses_source_maps(&self) -> bool {
        self.selection_source.is_some() || !self.highlights.is_empty()
    }

    pub(super) fn selection_map(&self, map: Option<&SpanMap>) -> Option<Rc<SpanMap>> {
        self.selection_source.as_ref()?;
        map.filter(|m| !m.0.is_empty()).map(|m| Rc::new(m.clone()))
    }
}

/// Give one element its window onto a block's shared map. `pads` are the shown byte ranges
/// inserted by code tab expansion, sorted and disjoint. An empty line's placeholder has a
/// zero-length window onto the source text.
pub(super) fn attach(el: El, map: Option<Rc<SpanMap>>, window: Range<usize>, pads: Vec<Range<usize>>) -> El {
    let Some(map) = map.filter(|_| !window.is_empty()) else { return el };
    el.selection_source_map(move |r| {
        let unpad = |byte: usize| byte - pads.iter().take_while(|p| p.start < byte)
            .map(|p| p.end.min(byte) - p.start).sum::<usize>();
        let a = unpad(r.start).min(window.len()) + window.start;
        let b = unpad(r.end).min(window.len()) + window.start;
        if a >= b { return None; }
        let mut bounds = None;
        for (segments, atomic) in [(&map.0, false), (&map.1, true)] {
            let first = segments.partition_point(|(_, shown)| shown.end <= a);
            for (src, shown) in segments[first..].iter().take_while(|(_, shown)| shown.start < b) {
                let mapped = if !atomic && src.len() == shown.len() {
                    src.start + a.saturating_sub(shown.start)..src.start + b.min(shown.end) - shown.start
                } else {
                    src.clone()
                };
                extend(&mut bounds, mapped);
            }
        }
        bounds
    })
}
