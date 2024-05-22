use core::fmt;
use crossterm::event::KeyEvent;
use rand::Rng;
use ratatui::{prelude::*, text::Span, widgets::canvas::Context};
use std::{cell::RefCell, collections::VecDeque, fmt::Display, rc::Rc};

use super::{Logger, Settings, SettingsBlock, Simulatable, Simulation};

#[derive(Debug)]
struct Icon {
    text: String,
    color: Color,
}

impl Icon {
    fn new(text: &str, color: Color) -> Icon {
        Icon {
            text: text.to_string(),
            color,
        }
    }

    fn print<'a>(&self) -> Span<'a> {
        self.text.clone().set_style(self.color)
    }
}

#[derive(Debug)]
struct Body {
    mass: f64,
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    icon: Icon,
}

impl Display for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:.0}kg pos: ({:.2}, {:.2}) vel: ({:.2}, {:.2})",
            self.mass, self.x, self.y, self.dx, self.dy
        )
    }
}

impl Body {
    const COLOURS: [Color; 8] = [
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::Gray,
        Color::White,
    ];

    fn rand(id: usize) -> Body {
        let mut rng = rand::thread_rng();
        Body {
            mass: 1.,
            x: rng.gen_range(-50.0..=50.0),
            y: rng.gen_range(-50.0..=50.0),
            dx: rng.gen_range(-0.1..=0.1),
            dy: rng.gen_range(-0.1..=0.1),
            icon: Icon::new("☼", Body::COLOURS[id % 8]),
        }
    }

    fn get_trail(&self) -> Body {
        Body {
            mass: 0.0,
            x: self.x,
            y: self.y,
            dx: 0.0,
            dy: 0.0,
            icon: Icon::new("·", self.icon.color),
        }
    }

    fn to_line(&self) -> Line {
        Line::from(vec![self.icon.print(), format!("{}\n", self).into()])
    }

    fn step(&mut self, force: (f64, f64), time: f64, drag: f64) {
        let (ddx, ddy) = (force.0 / self.mass, force.1 / self.mass);
        let (ddx, ddy) = (ddx.clamp(-0.1, 0.1), ddy.clamp(-0.1, 0.1));

        self.x += self.dx * time + 0.5 * ddx * time.powi(2);
        self.y += self.dy * time + 0.5 * ddy * time.powi(2);
        self.dx += ddx * time;
        self.dy += ddy * time;
        // Space Drag
        self.dx *= drag;
        self.dy *= drag;
    }
}

fn ransac_centroid(points: &VecDeque<Body>) -> (f64, f64) {
    let mut center = (0., 0.);
    let mut highest_inliers = 0;
    for i in 0..points.len() {
        let candidate_center = (points[i].x, points[i].y);

        let inliers = points
            .iter()
            .map(|e| {
                ((candidate_center.0 - e.x).powi(2) + (candidate_center.1 - e.y).powi(2)).powf(0.5)
            })
            .filter(|&dist| dist < 150.)
            .count();
        if inliers > highest_inliers {
            highest_inliers = inliers;
            center = points
                .iter()
                .filter(|e| {
                    ((candidate_center.0 - e.x).powi(2) + (candidate_center.1 - e.y).powi(2))
                        .powf(0.5)
                        < 150.
                })
                .fold((0., 0.), |acc, e| (acc.0 + e.x, acc.1 + e.y));
            center.0 /= points.len() as f64;
            center.1 /= points.len() as f64;
        }
    }
    return center;
}

pub struct NBody {
    logger: Logger,
    entities: VecDeque<Body>,
    trail: VecDeque<Body>,
    id_counter: usize,
    settings: SettingsBlock,
    speed: Rc<RefCell<Speed>>,
    drag: Rc<RefCell<Drag>>,
    gravity: Rc<RefCell<Gravity>>,
}

impl Simulatable for NBody {
    fn init() -> Simulation
    where
        Self: Sized,
    {
        let speed = Speed::new();
        let gravity = Gravity::new();
        let drag = Drag::new();

        let logger = Logger::new();

        Simulation {
            exit: false,
            pause: true,
            reset: false,
            logger: logger.clone(),
            fps: 60,
            simulation: Box::new(NBody {
                logger: logger.clone(),
                entities: vec![Body::rand(0), Body::rand(1), Body::rand(2)].into(),
                trail: VecDeque::new(),
                id_counter: 3,
                speed: speed.clone(),
                gravity: gravity.clone(),
                drag: drag.clone(),
                settings: SettingsBlock {
                    selected: 0,
                    settings: vec![speed.clone(), gravity.clone(), drag.clone()],
                },
            }),
        }
    }

    fn reset(&mut self) {
        self.entities = vec![Body::rand(0), Body::rand(1), Body::rand(2)].into();
        self.trail = VecDeque::new();
        self.id_counter = 3;
    }
    fn handle_key_events(&mut self, key_event: KeyEvent) {
        match key_event {
            _ => {}
        };
    }
    fn update(&mut self) {
        // Create Trail
        for e in &self.entities {
            self.trail.push_back(e.get_trail());
        }
        // Calculate forces
        let mut forces = vec![(0., 0.); self.entities.len()];
        for i in 0..self.entities.len() - 1 {
            for j in (i + 1)..self.entities.len() {
                let a = &self.entities[i];
                let b = &self.entities[j];
                let r = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).powf(0.5);
                let force = self.gravity.borrow().0 * a.mass * b.mass / r.powi(2);
                let ab_hat = ((a.x - b.x) / r, (a.y - b.y) / r);
                forces[i] = (
                    forces[i].0 - ab_hat.0 * force,
                    forces[i].1 - ab_hat.1 * force,
                );
                forces[j] = (
                    forces[j].0 + ab_hat.0 * force,
                    forces[j].1 + ab_hat.1 * force,
                );
                self.logger.log(&format!("[{i},{j}] r: {r:?}"));
            }
        }

        // Apply forces
        for i in 0..self.entities.len() {
            self.entities[i].step(
                forces[i],
                self.speed.borrow().0 as f64,
                self.drag.borrow().0,
            );
            self.logger.log(&format!("[{i}] force: {:?}", forces[i]));
        }

        let centroid = ransac_centroid(&self.entities);

        self.entities
            .iter_mut()
            .for_each(|e| e.x -= 0.1 * centroid.0);
        self.entities
            .iter_mut()
            .for_each(|e| e.y -= 0.1 * centroid.1);
        self.trail.iter_mut().for_each(|e| e.x -= 0.1 * centroid.0);
        self.trail.iter_mut().for_each(|e| e.x -= 0.1 * centroid.0);
    }
    fn canvas_title(&self) -> &str {
        " N-Body Simulation "
    }
    fn canvas_bounds(&self) -> (f64, f64, f64, f64) {
        (-100., 100., -100., 100.)
    }
    fn canvas_render(&self, ctx: &mut Context) {
        self.entities
            .iter()
            .for_each(|e| ctx.print(e.x, e.y, e.icon.print()));
    }
    fn info_title(&self) -> &str {
        " Entity Info "
    }
    fn info_text(&self) -> Text {
        let mut entity_info = String::new();
        for e in &self.entities {
            entity_info.push_str(&e.icon.print().to_string());
            entity_info.push_str(&format!("{e}\n"));
        }
        Text::from(
            self.entities
                .iter()
                .map(|e| e.to_line())
                .collect::<Vec<_>>(),
        )
    }
    fn settings(&self) -> &SettingsBlock {
        &self.settings
    }
    fn settings_mut(&mut self) -> &mut SettingsBlock {
        &mut self.settings
    }
}

struct Gravity(f64);
impl Settings for Gravity {
    fn new() -> Rc<RefCell<Self>>
    where
        Self: Sized,
    {
        Rc::new(RefCell::new(Gravity(1e2)))
    }
    fn decrement(&mut self) {
        self.0 *= 0.1;
    }
    fn increment(&mut self) {
        self.0 *= 10.0;
    }
    fn text(&self) -> &str {
        "Force (G):"
    }
    fn value(&self) -> String {
        format!("{:.0}", self.0)
    }
}
struct Speed(i64);
impl Settings for Speed {
    fn new() -> Rc<RefCell<Self>>
    where
        Self: Sized,
    {
        Rc::new(RefCell::new(Speed(3)))
    }
    fn decrement(&mut self) {
        self.0 -= 1;
    }
    fn increment(&mut self) {
        self.0 += 1;
    }
    fn text(&self) -> &str {
        "Speed:"
    }
    fn value(&self) -> String {
        format!("{}", self.0)
    }
}
struct Drag(f64);
impl Settings for Drag {
    fn new() -> Rc<RefCell<Self>>
    where
        Self: Sized,
    {
        Rc::new(RefCell::new(Drag(0.99)))
    }
    fn decrement(&mut self) {
        self.0 -= 0.01
    }
    fn increment(&mut self) {
        self.0 += 0.01
    }
    fn text(&self) -> &str {
        "Drag:"
    }
    fn value(&self) -> String {
        format!("{:.2}", self.0)
    }
}
