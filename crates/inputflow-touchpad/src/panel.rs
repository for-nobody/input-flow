use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::contact::{Frame, Point, Tracker, Transition};

pub type Vertex = [f64; 2];
pub type Face = Vec<Vertex>;
const EPSILON: f64 = 1e-8;
const MAX_REGIONS: usize = 64;
const MAX_FACES: usize = 128;
const MAX_VERTICES: usize = 32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region<A> {
    pub id: String,
    pub name: String,
    pub action: Option<A>,
    /// Adjacent convex faces with disjoint interiors form one region.
    pub faces: Vec<Face>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Geometry<A> {
    /// Half-width of a dividing seam in normalized board coordinates.
    pub gap: Vertex,
    pub regions: Vec<Region<A>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Region(usize),
    Gap,
    Background,
}

#[derive(Default)]
pub struct Surface {
    seams: Vec<[Vertex; 2]>,
}

impl Surface {
    pub fn seams(&self) -> &[[Vertex; 2]] {
        &self.seams
    }

    pub fn compile<A>(geometry: &Geometry<A>) -> Self {
        let mut seams = Vec::new();
        for (index, region) in geometry.regions.iter().enumerate() {
            for other in &geometry.regions[..index] {
                for edge in region.faces.iter().flat_map(|face| edges(face)) {
                    for neighbor in other.faces.iter().flat_map(|face| edges(face)) {
                        if let Some([start, end]) = shared_interval(edge, neighbor) {
                            seams.push([along(edge, start), along(edge, end)]);
                        }
                    }
                }
            }
        }
        Self { seams }
    }

    pub fn hit<A>(&self, geometry: &Geometry<A>, vertex: Vertex) -> Hit {
        if vertex
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Hit::Background;
        }
        if self
            .seams
            .iter()
            .any(|&edge| near(edge, vertex, geometry.gap))
        {
            return Hit::Gap;
        }
        geometry
            .regions
            .iter()
            .position(|region| region.faces.iter().any(|face| contains(face, vertex)))
            .map_or(Hit::Background, Hit::Region)
    }
}

impl<A> Geometry<A> {
    pub fn validate(&self) -> Result<(), String> {
        if self.gap.iter().any(|v| !(0.0..=0.04).contains(v)) {
            return Err("invalid seam width".into());
        }
        if self.regions.len() > MAX_REGIONS
            || self
                .regions
                .iter()
                .map(|region| region.faces.len())
                .sum::<usize>()
                > MAX_FACES
        {
            return Err("too many regions or faces".into());
        }
        let mut faces: Vec<(&str, &Face)> = Vec::new();
        let mut ids = std::collections::BTreeSet::new();
        for region in &self.regions {
            if !valid_label(&region.id, 64) || !ids.insert(&region.id) {
                return Err(format!("invalid or duplicate region ID {:?}", region.id));
            }
            if !valid_label(&region.name, 80) || region.faces.is_empty() {
                return Err(format!("region {} needs a name and outline", region.id));
            }
            for face in &region.faces {
                if !valid_face(face) {
                    return Err(format!(
                        "region {} has an invalid or out-of-bounds polygon",
                        region.id
                    ));
                }
                if let Some((id, _)) = faces.iter().find(|(_, other)| overlap(face, other)) {
                    return Err(format!("region {} overlaps region {id}", region.id));
                }
                faces.push((&region.id, face));
            }
        }
        Ok(())
    }
}

pub fn valid_label(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}

pub fn rectangle(left: f64, top: f64, right: f64, bottom: f64) -> Face {
    vec![[left, top], [right, top], [right, bottom], [left, bottom]]
}

pub fn normalize(point: Point, bounds: [(i32, i32); 2]) -> Option<Vertex> {
    let [(x_min, x_max), (y_min, y_max)] = bounds;
    if x_min >= x_max || y_min >= y_max {
        return None;
    }
    let scale =
        |value, min, max| (f64::from(value) - f64::from(min)) / (f64::from(max) - f64::from(min));
    Some([scale(point.x, x_min, x_max), scale(point.y, y_min, y_max)])
}

fn edges(face: &[Vertex]) -> impl Iterator<Item = [Vertex; 2]> + '_ {
    face.iter()
        .copied()
        .zip(face.iter().copied().cycle().skip(1))
        .map(|(a, b)| [a, b])
}

fn cross([a, b]: [Vertex; 2], p: Vertex) -> f64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

fn length([a, b]: [Vertex; 2]) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

fn valid_face(face: &[Vertex]) -> bool {
    if !(3..=MAX_VERTICES).contains(&face.len())
        || face.iter().flatten().any(|v| !(0.0..=1.0).contains(v))
    {
        return false;
    }
    let area = edges(face)
        .map(|[[ax, ay], [bx, by]]| ax * by - ay * bx)
        .sum::<f64>();
    area > EPSILON
        && edges(face)
            .all(|edge| length(edge) > EPSILON && face.iter().all(|&p| cross(edge, p) >= -EPSILON))
        && face
            .iter()
            .enumerate()
            .all(|(i, p)| !face[..i].iter().any(|q| length([*p, *q]) <= EPSILON))
}

fn overlap(a: &[Vertex], b: &[Vertex]) -> bool {
    let outside = |face: &[Vertex], other: &[Vertex]| {
        edges(face).any(|edge| other.iter().all(|&p| cross(edge, p) <= EPSILON))
    };
    !outside(a, b) && !outside(b, a)
}

fn contains(face: &[Vertex], point: Vertex) -> bool {
    edges(face).all(|edge| cross(edge, point) >= -EPSILON)
}

fn along([a, b]: [Vertex; 2], fraction: f64) -> Vertex {
    [
        a[0] + (b[0] - a[0]) * fraction,
        a[1] + (b[1] - a[1]) * fraction,
    ]
}

fn shared_interval(edge: [Vertex; 2], [c, d]: [Vertex; 2]) -> Option<[f64; 2]> {
    let [a, b] = edge;
    if [c, d]
        .iter()
        .any(|p| cross(edge, *p).abs() > EPSILON * length(edge))
    {
        return None;
    }
    let axis = usize::from((b[1] - a[1]).abs() > (b[0] - a[0]).abs());
    let fraction = |p: Vertex| (p[axis] - a[axis]) / (b[axis] - a[axis]);
    let c = fraction(c);
    let d = fraction(d);
    let start = c.min(d).max(0.0);
    let end = c.max(d).min(1.0);
    (end - start > EPSILON).then_some([start, end])
}

fn near([a, b]: [Vertex; 2], p: Vertex, gap: Vertex) -> bool {
    // Scale each axis independently so legacy rectangular grid seams keep their width.
    let scale = |v: Vertex| [v[0] / gap[0].max(EPSILON), v[1] / gap[1].max(EPSILON)];
    let [a, b, p] = [scale(a), scale(b), scale(p)];
    let [dx, dy] = [b[0] - a[0], b[1] - a[1]];
    let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy);
    // No extension beyond the shared edge. A nearby unpartitioned area remains background.
    (-EPSILON..=1.0 + EPSILON).contains(&t)
        && cross([a, b], p).abs() <= dx.hypot(dy) * (1.0 + EPSILON)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Highlight(Option<usize>),
    Confirmed(usize),
    Cancelled(&'static str),
}

#[derive(Default)]
pub struct Recognizer {
    tracker: Tracker,
    selected: Option<usize>,
    highlight: Option<usize>,
}

impl Recognizer {
    pub fn highlight(&self) -> Option<usize> {
        self.highlight
    }

    pub fn interrupt(&mut self, reason: &'static str) -> Option<Event> {
        self.tracker.interrupt(reason);
        self.cancel(reason)
    }

    fn cancel(&mut self, reason: &'static str) -> Option<Event> {
        self.highlight = None;
        self.selected.take().map(|_| Event::Cancelled(reason))
    }

    pub fn observe<A>(
        &mut self,
        frame: Frame,
        bounds: [(i32, i32); 2],
        geometry: &Geometry<A>,
        surface: &Surface,
        now: Instant,
    ) -> Option<Event> {
        let hit =
            |point| normalize(point, bounds).map_or(Hit::Background, |p| surface.hit(geometry, p));
        match self.tracker.observe(frame, now) {
            Some(Transition::Down { start, .. }) => {
                let Hit::Region(index) = hit(start) else {
                    return Some(Event::Cancelled("down_outside_region"));
                };
                if geometry.regions[index].action.is_none() {
                    return Some(Event::Cancelled("unbound_region"));
                }
                self.selected = Some(index);
                self.highlight = Some(index);
                Some(Event::Highlight(self.highlight))
            }
            Some(Transition::Up { end, .. }) => {
                let index = self.selected.take()?;
                self.highlight = None;
                Some(if hit(end) == Hit::Region(index) {
                    Event::Confirmed(index)
                } else {
                    Event::Cancelled("up_outside_origin")
                })
            }
            Some(Transition::Incomplete { reason, .. }) => self.cancel(reason),
            None => {
                let index = self.selected?;
                let Frame::Single(contact) = frame else {
                    return None;
                };
                let highlight = match hit(contact.point) {
                    Hit::Region(current) if current == index => Some(index),
                    Hit::Region(_) => return self.cancel("crossed_region"),
                    Hit::Background => return self.cancel("entered_background"),
                    Hit::Gap => None,
                };
                if highlight == self.highlight {
                    return None;
                }
                self.highlight = highlight;
                Some(Event::Highlight(highlight))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contact::Contact;

    fn geometry() -> Geometry<u8> {
        serde_json::from_str(include_str!("../../../fixtures/config/panel-geometry.json")).unwrap()
    }

    #[test]
    fn common_geometry_distinguishes_seams_concave_notches_and_edges() {
        let layout = geometry();
        layout.validate().unwrap();
        let surface = Surface::compile(&layout);
        for (point, hit) in [
            ([0.0, 0.0], Hit::Region(0)),
            ([1.0, 1.0], Hit::Region(0)),
            ([0.25, 0.5], Hit::Region(0)),
            ([0.75, 0.25], Hit::Region(1)),
            ([0.8, 0.1], Hit::Background),
            ([0.5, 0.25], Hit::Gap),
            ([0.75, 0.503], Hit::Gap),
        ] {
            assert_eq!(surface.hit(&layout, point), hit, "{point:?}");
        }
        let mut invalid = layout.clone();
        invalid.regions[1].faces = vec![rectangle(0.25, 0.25, 0.75, 0.75)];
        assert!(invalid.validate().unwrap_err().contains("overlap"));
        invalid.regions[1].faces[0].swap(1, 2);
        assert!(invalid.validate().unwrap_err().contains("invalid"));
    }

    #[test]
    fn diagonal_confirms_only_an_explicit_matching_lift_and_cancels_on_crossing() {
        let mut layout = geometry();
        layout.regions[0].faces = vec![vec![[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]]];
        layout.regions[1].faces = vec![vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]];
        layout.validate().unwrap();
        let surface = Surface::compile(&layout);
        let mut recognizer = Recognizer::default();
        let mut sample = |x, y, tip| {
            recognizer.observe(
                Frame::Single(Contact {
                    id: 1,
                    tip,
                    point: Point { x, y },
                }),
                [(0, 1000); 2],
                &layout,
                &surface,
                Instant::now(),
            )
        };
        assert_eq!(sample(300, 700, true), Some(Event::Highlight(Some(0))));
        assert_eq!(sample(500, 500, true), Some(Event::Highlight(None)));
        assert_eq!(sample(300, 700, true), Some(Event::Highlight(Some(0))));
        assert_eq!(sample(300, 700, false), Some(Event::Confirmed(0)));
        assert_eq!(sample(300, 700, false), None);
        assert_eq!(sample(300, 700, true), Some(Event::Highlight(Some(0))));
        assert_eq!(
            sample(700, 300, true),
            Some(Event::Cancelled("crossed_region"))
        );
        assert_eq!(sample(300, 700, true), None);
        assert_eq!(sample(300, 700, false), None);
        assert_eq!(sample(700, 300, true), Some(Event::Highlight(Some(1))));
        assert_eq!(sample(700, 300, false), Some(Event::Confirmed(1)));
    }
}
