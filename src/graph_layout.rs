//! Deterministic, bounded graph geometry; computed once per topology/settings change.
use crate::graph::{Graph, View};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Forces {
    pub center: f32,
    pub repel: f32,
    pub link: f32,
    pub distance: f32,
}

impl Default for Forces {
    fn default() -> Self {
        Self {
            center: 0.52,
            repel: 10.0,
            link: 1.0,
            distance: 250.0,
        }
    }
}

fn bounded(value: f32, default: f32, minimum: f32, maximum: f32) -> f32 {
    if value.is_finite() {
        value.clamp(minimum, maximum)
    } else {
        default
    }
}

impl Forces {
    pub fn normalized(self) -> Self {
        let default = Self::default();
        Self {
            center: bounded(self.center, default.center, 0.0, 1.0),
            repel: bounded(self.repel, default.repel, 0.0, 20.0),
            link: bounded(self.link, default.link, 0.0, 1.0),
            distance: bounded(self.distance, default.distance, 30.0, 500.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Geometry {
    /// Indexed by the original graph node index, just like View::nodes.
    pub points: Vec<[f32; 2]>,
    pub bounds: [[f32; 2]; 2],
    pub iterations: usize,
}

impl Geometry {
    pub fn compute(graph: &Graph, view: &View, forces: Forces, root: Option<usize>) -> Self {
        let forces = forces.normalized();
        let mut points = vec![[0.0; 2]; graph.nodes.len()];
        let count = view.nodes.len();
        if count == 0 {
            return Self {
                points,
                bounds: [[-1.0; 2], [1.0; 2]],
                iterations: 0,
            };
        }
        let mut indices = vec![0; graph.nodes.len()];
        for (ordinal, &node) in view.nodes.iter().enumerate() {
            indices[node] = ordinal;
            let angle = ordinal as f32 * 2.399_963_1;
            let radius = forces.distance * 0.32 * ((ordinal + 1) as f32).sqrt();
            points[node] = [angle.cos() * radius, angle.sin() * radius];
        }
        if let Some(root) = root {
            points[root] = [0.0; 2];
        }
        let iterations = if count <= 128 { 96 } else { 64 };
        let mut velocity = vec![[0.0; 2]; count];
        let mut acceleration = vec![[0.0; 2]; count];
        let cell_size = forces.distance.max(60.0);
        for iteration in 0..iterations {
            let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
            for &node in &view.nodes {
                let p = points[node];
                grid.entry((
                    (p[0] / cell_size).floor() as i32,
                    (p[1] / cell_size).floor() as i32,
                ))
                .or_default()
                .push(node);
            }
            acceleration.fill([0.0; 2]);
            for (ordinal, &node) in view.nodes.iter().enumerate() {
                let p = points[node];
                let cell = (
                    (p[0] / cell_size).floor() as i32,
                    (p[1] / cell_size).floor() as i32,
                );
                let mut samples = 0;
                'neighbors: for dy in -1..=1 {
                    for dx in -1..=1 {
                        if let Some(neighbors) = grid.get(&(cell.0 + dx, cell.1 + dy)) {
                            for &other in neighbors {
                                if node == other {
                                    continue;
                                }
                                let delta = [p[0] - points[other][0], p[1] - points[other][1]];
                                let squared = delta[0] * delta[0] + delta[1] * delta[1] + 1.0;
                                let strength = (forces.repel * 180.0 / squared).min(8.0);
                                let distance = squared.sqrt();
                                acceleration[ordinal][0] += delta[0] / distance * strength;
                                acceleration[ordinal][1] += delta[1] / distance * strength;
                                samples += 1;
                                if samples == 48 {
                                    break 'neighbors;
                                }
                            }
                        }
                    }
                }
                acceleration[ordinal][0] -= p[0] * forces.center * 0.002;
                acceleration[ordinal][1] -= p[1] * forces.center * 0.002;
            }
            for edge in &view.edges {
                let delta = [
                    points[edge.to][0] - points[edge.from][0],
                    points[edge.to][1] - points[edge.from][1],
                ];
                let distance = (delta[0] * delta[0] + delta[1] * delta[1]).sqrt().max(1.0);
                let strength =
                    ((distance - forces.distance) * forces.link * 0.025).clamp(-12.0, 12.0);
                for (axis, delta) in delta.iter().enumerate() {
                    let pull = delta / distance * strength;
                    acceleration[indices[edge.from]][axis] += pull;
                    acceleration[indices[edge.to]][axis] -= pull;
                }
            }
            let cooling = 1.0 - 0.75 * iteration as f32 / iterations as f32;
            for (ordinal, &node) in view.nodes.iter().enumerate() {
                if Some(node) == root {
                    continue;
                }
                for axis in 0..2 {
                    velocity[ordinal][axis] = (velocity[ordinal][axis] * 0.72
                        + acceleration[ordinal][axis])
                        .clamp(-16.0, 16.0);
                    points[node][axis] = (points[node][axis] + velocity[ordinal][axis] * cooling)
                        .clamp(-1_000_000.0, 1_000_000.0);
                }
            }
        }
        let mut bounds = [[f32::INFINITY; 2], [f32::NEG_INFINITY; 2]];
        for &node in &view.nodes {
            for axis in 0..2 {
                bounds[0][axis] = bounds[0][axis].min(points[node][axis] - 20.0);
                bounds[1][axis] = bounds[1][axis].max(points[node][axis] + 20.0);
            }
        }
        Self {
            points,
            bounds,
            iterations,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Camera {
    pub zoom: f32,
    pub pan: [f32; 2],
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: [0.0; 2],
        }
    }
}

impl Camera {
    pub fn fit(&mut self, bounds: [[f32; 2]; 2], size: [f32; 2]) {
        self.zoom = ((size[0] - 60.0).max(1.0) / (bounds[1][0] - bounds[0][0]).max(1.0))
            .min((size[1] - 60.0).max(1.0) / (bounds[1][1] - bounds[0][1]).max(1.0))
            .clamp(0.02, 4.0);
        self.pan = [
            -(bounds[0][0] + bounds[1][0]) * 0.5 * self.zoom,
            -(bounds[0][1] + bounds[1][1]) * 0.5 * self.zoom,
        ];
    }

    /// Pivot is relative to the center of the graph canvas.
    pub fn zoom_at(&mut self, factor: f32, pivot: [f32; 2]) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let next = (self.zoom * factor).clamp(0.02, 12.0);
        for (axis, pivot) in pivot.iter().enumerate() {
            let world = (pivot - self.pan[axis]) / self.zoom;
            self.pan[axis] = pivot - world * next;
        }
        self.zoom = next;
    }

    pub fn project(&self, point: [f32; 2]) -> [f32; 2] {
        [
            point[0] * self.zoom + self.pan[0],
            point[1] * self.zoom + self.pan[1],
        ]
    }
}
