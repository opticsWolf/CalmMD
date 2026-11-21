use crate::diagrams::{DiagramType, Direction, Graph, Shape};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    base::StyledExt,
    component::{label::Label, theme::Theme},
    *,
};
use std::collections::VecDeque;

type XY = (f32, f32);

struct BoxLabel {
    position: XY,
    size: XY,
    text: String,
    shape: Shape,
}

struct Line {
    points: Vec<XY>,
    dashed: bool,
    thick: bool,
    arrow: bool,
}

struct Scene {
    size: XY,
    boxes: Vec<BoxLabel>,
    groups: Vec<(XY, XY, String)>,
    lines: Vec<Line>,
    captions: Vec<(XY, String)>,
}

const CELL: XY = (180., 114.);
const BOX: XY = (124., 52.);

fn ranks(graph: &Graph, links: &[(usize, usize)]) -> Vec<usize> {
    let n = graph.nodes.len();
    let mut incoming = vec![0; n];
    let mut outgoing = vec![Vec::new(); n];
    for &(a, b) in links {
        if a != b {
            outgoing[a].push(b);
            incoming[b] += 1;
        }
    }
    let mut queue: VecDeque<_> = (0..n).filter(|&i| incoming[i] == 0).collect();
    if queue.is_empty() && n > 0 {
        queue.push_back(0);
    }
    let mut depth = vec![0; n];
    let mut visited = vec![false; n];
    while let Some(i) = queue.pop_front() {
        if visited[i] {
            continue;
        }
        visited[i] = true;
        for &next in &outgoing[i] {
            if !visited[next] {
                depth[next] = depth[next].max(depth[i] + 1);
                incoming[next] -= 1;
                if incoming[next] == 0 {
                    queue.push_back(next);
                }
            }
        }
    }
    depth
}

fn pairs(graph: &Graph) -> Vec<(usize, usize)> {
    let indices: std::collections::HashMap<_, _> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), index))
        .collect();
    graph
        .edges
        .iter()
        .map(|edge| (indices[edge.from.as_str()], indices[edge.to.as_str()]))
        .collect()
}

fn flowchart(graph: &Graph) -> Scene {
    let links = pairs(graph);
    let depth = ranks(graph, &links);
    let levels = depth.iter().copied().max().unwrap_or(0) + 1;
    let mut row_counts = vec![0; levels];
    for &rank in &depth {
        row_counts[rank] += 1;
    }
    let widest = row_counts.iter().copied().max().unwrap_or(1);
    let horizontal = matches!(graph.direction, Direction::Right | Direction::Left);
    let reversed = matches!(graph.direction, Direction::Left | Direction::Up);
    let mut placed = vec![0; levels];
    let boxes: Vec<_> = graph
        .nodes
        .iter()
        .zip(&depth)
        .map(|(node, &rank)| {
            let column = if reversed { levels - 1 - rank } else { rank };
            let slot = placed[rank] as f32 + (widest - row_counts[rank]) as f32 / 2.;
            placed[rank] += 1;
            let (x, y) = if horizontal {
                (column as f32, slot)
            } else {
                (slot, column as f32)
            };
            BoxLabel {
                position: (x * CELL.0 + CELL.0 / 2., y * CELL.1 + CELL.1 / 2.),
                size: BOX,
                text: node.label.clone(),
                shape: node.shape,
            }
        })
        .collect();
    let groups = graph
        .groups
        .iter()
        .filter_map(|group| {
            let members: Vec<_> = graph
                .nodes
                .iter()
                .zip(&boxes)
                .filter_map(|(node, placed)| group.members.contains(&node.id).then_some(placed))
                .collect();
            if members.is_empty() {
                return None;
            }
            let left = members
                .iter()
                .map(|node| node.position.0 - node.size.0 / 2.)
                .fold(f32::INFINITY, f32::min);
            let top = members
                .iter()
                .map(|node| node.position.1 - node.size.1 / 2.)
                .fold(f32::INFINITY, f32::min);
            let right = members
                .iter()
                .map(|node| node.position.0 + node.size.0 / 2.)
                .fold(f32::NEG_INFINITY, f32::max);
            let bottom = members
                .iter()
                .map(|node| node.position.1 + node.size.1 / 2.)
                .fold(f32::NEG_INFINITY, f32::max);
            Some((
                (left - 9., (top - 23.).max(0.)),
                (right - left + 18., bottom - top + 32.),
                group.label.clone(),
            ))
        })
        .collect();
    let mut scene = Scene {
        size: if horizontal {
            (levels as f32 * CELL.0, widest as f32 * CELL.1)
        } else {
            (widest as f32 * CELL.0, levels as f32 * CELL.1)
        },
        boxes,
        groups,
        lines: Vec::new(),
        captions: Vec::new(),
    };
    for ((a, b), edge) in links.into_iter().zip(&graph.edges) {
        let from = &scene.boxes[a];
        let to = &scene.boxes[b];
        let start = border_point(from, to.position);
        let end = border_point(to, from.position);
        let points = if a == b {
            let (x, y) = from.position;
            vec![
                (x + BOX.0 / 2., y - 10.),
                (x + BOX.0 / 2. + 18., y - 28.),
                (x + BOX.0 / 2. + 18., y + 30.),
                (x + BOX.0 / 2., y + 10.),
            ]
        } else {
            vec![start, end]
        };
        if !edge.label.is_empty() {
            let midpoint = if a == b {
                (from.position.0, from.position.1 - 40.)
            } else {
                ((start.0 + end.0) / 2., (start.1 + end.1) / 2.)
            };
            scene.captions.push((midpoint, edge.label.clone()));
        }
        scene.lines.push(Line {
            points,
            dashed: edge.dashed,
            thick: edge.strong,
            arrow: edge.points_to_target,
        });
    }
    scene
}

fn border_point(node: &BoxLabel, target: XY) -> XY {
    let (x, y) = node.position;
    let dx = target.0 - x;
    let dy = target.1 - y;
    let fraction = (dx.abs() / (node.size.0 / 2.)).max(dy.abs() / (node.size.1 / 2.));
    if fraction == 0. {
        return node.position;
    }
    (x + dx / fraction, y + dy / fraction)
}

fn sequence(graph: &Graph) -> Scene {
    let width = 180.;
    let height = 115. + graph.edges.len() as f32 * 55.;
    let boxes: Vec<_> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| BoxLabel {
            position: (i as f32 * width + width / 2., 31.),
            size: (130., 42.),
            text: node.label.clone(),
            shape: Shape::Rounded,
        })
        .collect();
    let lines = boxes
        .iter()
        .map(|node| Line {
            points: vec![(node.position.0, 52.), (node.position.0, height - 15.)],
            dashed: true,
            thick: false,
            arrow: false,
        })
        .collect();
    let mut scene = Scene {
        size: (width * graph.nodes.len() as f32, height),
        boxes,
        groups: Vec::new(),
        lines,
        captions: Vec::new(),
    };
    for (i, ((a, b), edge)) in pairs(graph).into_iter().zip(&graph.edges).enumerate() {
        let y = 97. + i as f32 * 55.;
        let x1 = scene.boxes[a].position.0;
        let x2 = scene.boxes[b].position.0;
        let points = if a == b {
            vec![(x1, y), (x1 + 42., y), (x1 + 42., y + 18.), (x1, y + 18.)]
        } else {
            vec![(x1, y), (x2, y)]
        };
        scene.lines.push(Line {
            points,
            dashed: edge.dashed,
            thick: false,
            arrow: true,
        });
        if !edge.label.is_empty() {
            scene
                .captions
                .push((((x1 + x2) * 0.5, y - 14.), edge.label.clone()));
        }
    }
    scene
}

pub(super) fn render(graph: &Graph, id: impl Into<ElementId>, cx: &App) -> Option<AnyElement> {
    if graph.nodes.is_empty() || graph.nodes.len() > 100 || graph.edges.len() > 300 {
        return None;
    }
    let scene = match graph.kind {
        DiagramType::Flowchart => flowchart(graph),
        DiagramType::Sequence => sequence(graph),
    };
    let theme = Theme::global(cx);
    let (line, surface, node_background, border) = (
        theme.muted_foreground,
        theme.background,
        theme.muted,
        theme.link,
    );
    let (w, h) = scene.size;
    let paint = canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let position = |p: XY| bounds.origin + point(px(p.0), px(p.1));
            for edge in scene.lines {
                let mut points = edge.points.iter().copied();
                let Some(first) = points.next() else { continue };
                let mut path = PathBuilder::stroke(px(if edge.thick { 2.4 } else { 1.3 }));
                if edge.dashed {
                    path = path.dash_array(&[px(4.), px(4.)]);
                }
                path.move_to(position(first));
                for next in points {
                    path.line_to(position(next));
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, line);
                }
                if edge.arrow && edge.points.len() >= 2 {
                    let end = edge.points[edge.points.len() - 1];
                    let before = edge.points[edge.points.len() - 2];
                    let dx = end.0 - before.0;
                    let dy = end.1 - before.1;
                    let length = dx.hypot(dy);
                    if length <= 0.1 {
                        continue;
                    }
                    let (ux, uy) = (dx / length, dy / length);
                    let mut arrow = PathBuilder::fill();
                    arrow.move_to(position(end));
                    arrow.line_to(position((
                        end.0 - 9. * ux + 4. * uy,
                        end.1 - 9. * uy - 4. * ux,
                    )));
                    arrow.line_to(position((
                        end.0 - 9. * ux - 4. * uy,
                        end.1 - 9. * uy + 4. * ux,
                    )));
                    arrow.close();
                    if let Ok(arrow) = arrow.build() {
                        window.paint_path(arrow, line);
                    }
                }
            }
        },
    )
    .absolute()
    .size_full();
    let mut content = div().relative().w(px(w)).h(px(h)).flex_shrink_0();
    for ((x, y), (w, h), label) in scene.groups {
        content = content.child(
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w))
                .h(px(h))
                .border_1()
                .border_color(border.opacity(0.55))
                .rounded_md()
                .child(div().px_2().text_xs().child(label)),
        );
    }
    content = content.child(paint);
    for (index, node) in scene.boxes.into_iter().enumerate() {
        let (x, y) = node.position;
        let (width, height) = node.size;
        let diamond = node.shape == Shape::Diamond;
        let label = div()
            .h_flex()
            .items_center()
            .justify_center()
            .size_full()
            .px_2()
            .child(
                div()
                    .min_w(px(0.))
                    .text_center()
                    .text_ellipsis()
                    .overflow_hidden()
                    .child(Label::new(node.text)),
            );
        let mut box_element = div()
            .absolute()
            .left(px(x - width / 2.))
            .top(px(y - height / 2.))
            .w(px(width))
            .h(px(height));
        if diamond {
            box_element = box_element.child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let coords = [
                            (width / 2., 1.),
                            (width - 1., height / 2.),
                            (width / 2., height - 1.),
                            (1., height / 2.),
                        ];
                        for (mut path, color) in [
                            (PathBuilder::fill(), node_background),
                            (PathBuilder::stroke(px(1.)), border),
                        ] {
                            for (i, (x, y)) in coords.iter().enumerate() {
                                let position = bounds.origin + point(px(*x), px(*y));
                                if i == 0 {
                                    path.move_to(position);
                                } else {
                                    path.line_to(position);
                                }
                            }
                            path.close();
                            if let Ok(path) = path.build() {
                                window.paint_path(path, color);
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            );
        } else {
            box_element = box_element
                .border_1()
                .border_color(border)
                .bg(node_background)
                .when(matches!(node.shape, Shape::Rounded), |element| {
                    element.rounded_md()
                })
                .when(matches!(node.shape, Shape::Circle), |element| {
                    element.rounded_full()
                });
        }
        content = content.child(box_element.id(("diagram-node", index)).child(label));
    }
    for ((x, y), text) in scene.captions {
        content = content.child(
            div()
                .absolute()
                .left(px(x - 66.))
                .top(px(y - 10.))
                .w(px(132.))
                .h_flex()
                .justify_center()
                .child(div().px_1().bg(surface).text_xs().child(text)),
        );
    }
    Some(
        div()
            .id(id)
            .w_full()
            .overflow_x_scroll()
            .bg(surface)
            .child(content)
            .into_any_element(),
    )
}
