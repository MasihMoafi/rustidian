use rustidian::{
    graph::{Graph, Options},
    graph_layout::{Camera, Forces, Geometry},
    vault::Vault,
};

#[test]
fn layout_is_deterministic_finite_and_keeps_local_root_fixed() {
    let folder = tempfile::tempdir().unwrap();
    for (name, text) in [
        ("Home.md", "[[Alpha]] [[Beta]]"),
        ("Alpha.md", "[[Beta]]"),
        ("Beta.md", "# Beta"),
    ] {
        std::fs::write(folder.path().join(name), text).unwrap();
    }
    let vault = Vault::open(folder.path()).unwrap();
    let graph = Graph::from_vault(&vault);
    let root = graph.node(std::path::Path::new("Home.md")).unwrap();
    let view = graph
        .visible(
            &vault,
            &Options::default(),
            Some(std::path::Path::new("Home.md")),
        )
        .unwrap();
    let first = Geometry::compute(&graph, &view, Forces::default(), Some(root));
    assert_eq!(
        first,
        Geometry::compute(&graph, &view, Forces::default(), Some(root))
    );
    assert_eq!(first.points[root], [0.0, 0.0]);
    assert!(first.iterations <= 96);
    assert!(first.points.iter().flatten().all(|value| value.is_finite()));
    assert_ne!(first.points[view.nodes[0]], first.points[view.nodes[1]]);
    let changed = Geometry::compute(
        &graph,
        &view,
        Forces {
            distance: 80.0,
            ..Forces::default()
        },
        Some(root),
    );
    assert_ne!(first.points, changed.points);
}

#[test]
fn empty_single_node_and_nonfinite_forces_are_bounded() {
    let graph = Graph::default();
    let empty = Geometry::compute(&graph, &Default::default(), Forces::default(), None);
    assert_eq!(empty.iterations, 0);
    assert!(empty.bounds.iter().flatten().all(|value| value.is_finite()));
    let invalid = Forces {
        center: f32::NAN,
        repel: f32::INFINITY,
        link: f32::NEG_INFINITY,
        distance: f32::NAN,
    };
    assert_eq!(invalid.normalized(), Forces::default());
    let graph = Graph {
        nodes: vec![rustidian::graph::Node {
            path: "Only.md".into(),
            label: "Only".into(),
            kind: rustidian::graph::Kind::Note,
        }],
        edges: vec![],
    };
    let view = rustidian::graph::View {
        nodes: vec![0],
        ..Default::default()
    };
    let single = Geometry::compute(&graph, &view, invalid, Some(0));
    assert_eq!(single.points, [[0.0, 0.0]]);
    assert!(single.bounds[1][0] > single.bounds[0][0]);
}

#[test]
fn fit_pan_and_pointer_anchored_zoom_keep_content_inside_the_view() {
    let mut camera = Camera::default();
    camera.fit([[-300.0, -150.0], [300.0, 150.0]], [800.0, 600.0]);
    let bounds = camera.project([300.0, 150.0]);
    assert!(bounds[0] <= 370.0 && bounds[1] <= 270.0);
    let pivot = [75.0, -25.0];
    let world = [
        (pivot[0] - camera.pan[0]) / camera.zoom,
        (pivot[1] - camera.pan[1]) / camera.zoom,
    ];
    camera.zoom_at(2.0, pivot);
    let projected = camera.project(world);
    assert!((projected[0] - pivot[0]).abs() < 0.001);
    assert!((projected[1] - pivot[1]).abs() < 0.001);
    camera.pan[0] -= 100.0;
    camera.pan[1] -= 60.0;
    let moved = camera.project(world);
    assert!((moved[0] - (pivot[0] - 100.0)).abs() < 0.001);
    assert!((moved[1] - (pivot[1] - 60.0)).abs() < 0.001);
    let before = camera;
    camera.zoom_at(f32::NAN, pivot);
    assert_eq!(camera, before);
}
