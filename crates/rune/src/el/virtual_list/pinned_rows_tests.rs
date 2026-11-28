use super::*;

fn end_window() -> VirtualList {
    let mut v = VirtualList {
        viewport: 40.0,
        ..Default::default()
    };
    v.set_estimate(20.0);
    v.set_keys((0..100).map(|i| format!("r{i}")).collect(), 0, None);
    v
}

#[test]
fn pinned_rows_default_mounts_the_last_32_rows() {
    let mut v = end_window();
    v.select(0.0, true, None);
    assert_eq!(PINNED_ROWS, 32);
    assert_eq!(v.range, v.keys.len() - PINNED_ROWS..v.keys.len());
}

#[test]
fn pinned_rows_unfilled_counts_the_configured_end_window() {
    let mut v = end_window();
    // Two visible rows and four overscan rows at the end. The window holds the last two.
    v.range = 98..100;
    v.set_pinned_rows(Some(0));
    assert_eq!(
        v.unfilled(0.0, true),
        4,
        "zero minimum leaves only overscan to fill"
    );

    v.set_pinned_rows(Some(9));
    assert_eq!(
        v.unfilled(0.0, true),
        7,
        "the custom minimum extends the end window"
    );
    v.select(0.0, true, None);
    assert_eq!(v.range, 91..100);
    assert_eq!(v.unfilled(0.0, true), 0);

    v.set_pinned_rows(None);
    assert_eq!(
        v.unfilled(0.0, true),
        32 - 9,
        "removing the override restores the default"
    );
}
