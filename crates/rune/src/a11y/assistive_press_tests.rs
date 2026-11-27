use accesskit::{Action as Ak, ActionRequest, Node, TreeId, TreeUpdate};
use serde_json::json;

use crate::{Cx, El, Env, Key, KeyEvent, Link, Mods, Runtime, TouchPhase, col, el, rich};

const DOCS_LINK: &str = "/docs";

type Events = Vec<(&'static str, bool)>;

fn scene() -> Runtime<Events> {
    fn view(_: &Events, _: &mut Cx<Events>) -> El {
        col()
            .size(240.0, 160.0)
            .gap(8.0)
            .child(
                el().key("button")
                    .role("button")
                    .label("Details")
                    .size(120.0, 30.0)
                    .focusable(true)
                    .press_on_return()
                    .expanded(false)
                    .on_press_tx(|events: &mut Events, tx| {
                        events.push(("button", tx.assistive_press()))
                    })
                    .on_secondary_press(|events: &mut Events, _, tx| {
                        events.push(("secondary", tx.assistive_press()))
                    }),
            )
            .child(
                el().key("default")
                    .size(120.0, 30.0)
                    .default_button()
                    .on_press_tx(|events: &mut Events, tx| {
                        events.push(("default", tx.assistive_press()))
                    }),
            )
            .child(
                rich("Read the docs", vec![])
                    .w(180.0)
                    .link(Link::new(9..13, DOCS_LINK))
                    .on_link(|events: &mut Events, id, tx| {
                        assert_eq!(id, DOCS_LINK);
                        events.push(("link", tx.assistive_press()));
                    }),
            )
    }

    let mut rt = Runtime::new(Events::new(), view, (240.0, 160.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

fn request<'a>(tree: &'a TreeUpdate, key: &str, action: Ak) -> (ActionRequest, &'a Node) {
    let (id, node) = tree
        .nodes
        .iter()
        .find(|(_, node)| node.author_id() == Some(key))
        .expect("target node");
    (
        ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: *id,
            data: None,
        },
        node,
    )
}

fn pointer_click(rt: &mut Runtime<Events>, x: f32, y: f32) {
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
}

#[test]
fn accesskit_click_marks_only_its_press() {
    let mut rt = scene();
    let tree = rt.accessibility_tree();
    assert!(rt.a11y_action(&request(&tree, "button", Ak::Click).0));
    rt.frame(0.016);
    assert_eq!(
        rt.state,
        [("button", true)],
        "AccessKit Click marks on_press as assistive"
    );

    let (x, y) = rt.find("button").expect("button box");
    pointer_click(&mut rt, x, y);
    rt.frame(0.032);
    assert_eq!(
        rt.state,
        [("button", true), ("button", false)],
        "the flag does not reach a later pointer press"
    );

    rt.touch(1, TouchPhase::Started, x, y, 0.04);
    rt.touch(1, TouchPhase::Ended, x, y, 0.09);
    rt.frame(0.096);
    assert_eq!(
        rt.state,
        [("button", true), ("button", false), ("button", false)],
        "a touch tap is not assistive"
    );
}

#[test]
fn accesskit_expand_marks_the_press_but_not_a_secondary_press_in_the_same_frame() {
    let mut rt = scene();
    let tree = rt.accessibility_tree();
    assert!(rt.a11y_action(&request(&tree, "button", Ak::Expand).0));
    assert!(rt.a11y_action(&request(&tree, "button", Ak::ShowContextMenu).0));
    rt.frame(0.016);
    assert_eq!(rt.state, [("button", true), ("secondary", false)]);
}

#[test]
fn keyboard_presses_are_not_assistive() {
    let mut rt = scene();
    rt.update(|_, tx| tx.focus("button", true));
    rt.frame(0.0);
    for key in [Key::Text(" ".into()), Key::Enter] {
        rt.key(KeyEvent {
            key,
            mods: Mods::default(),
        });
        rt.frame(0.0);
    }
    rt.update(|_, tx| tx.blur());
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    assert_eq!(
        rt.state,
        [("button", false), ("button", false), ("default", false)]
    );
}

#[test]
fn only_the_accesskit_click_on_a_link_is_assistive() {
    let mut rt = scene();
    let tree = rt.accessibility_tree();
    let (click, node) = request(&tree, DOCS_LINK, Ak::Click);
    let bounds = node.bounds().expect("link bounds");
    assert!(rt.a11y_action(&click));
    rt.frame(0.016);
    assert_eq!(rt.state, [("link", true)]);

    pointer_click(
        &mut rt,
        ((bounds.x0 + bounds.x1) / 2.0) as f32,
        ((bounds.y0 + bounds.y1) / 2.0) as f32,
    );
    rt.frame(0.032);
    assert_eq!(rt.state, [("link", true), ("link", false)]);
}

#[test]
fn web_mirror_button_activation_is_assistive() {
    let mut rt = scene();
    let item = rt
        .web_a11y_items()
        .into_iter()
        .find(|item| item.kind == "button" && item.text == "Details")
        .expect("mirrored button");
    assert!(rt.web_a11y_action(&json!({"kind": "a11y_press", "path": item.path})));
    rt.frame(0.016);
    assert_eq!(rt.state, [("button", true)]);
}

#[test]
fn web_mirror_link_activation_is_assistive() {
    let mut rt = scene();
    let item = rt
        .web_a11y_items()
        .into_iter()
        .find(|item| {
            item.runs
                .iter()
                .any(|run| run.href.as_deref() == Some(DOCS_LINK))
        })
        .expect("mirrored link");
    assert!(rt.web_a11y_action(&json!({"kind": "a11y_link", "path": item.path, "link": 0})));
    rt.frame(0.016);
    assert_eq!(rt.state, [("link", true)]);
}
