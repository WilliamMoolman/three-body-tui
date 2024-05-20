use core::fmt;
use rand::Rng;
use std::{collections::VecDeque, fmt::Display, time::Instant};

use color_eyre::{eyre::WrapErr, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    prelude::*,
    symbols::border,
    text::Line,
    widgets::{block::*, canvas::*, Borders, Paragraph},
};

mod errors;
mod tui;

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
            // icon: Icon::new("✹", Body::COLOURS[id % 8]),
        }
    }

    fn get_trail(&self) -> Body {
        Body { mass: 0.0, x: self.x, y: self.y, dx: 0.0, dy: 0.0, icon: Icon::new("·",self.icon.color) }
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

        let inliers = points.iter().map(|e| ((candidate_center.0 - e.x).powi(2) + (candidate_center.1 - e.y).powi(2)).powf(0.5)).filter(|&dist| dist < 150.).count();
        if inliers > highest_inliers {
            highest_inliers = inliers;
            center = points.iter().filter(|e| ((candidate_center.0 - e.x).powi(2) + (candidate_center.1 - e.y).powi(2)).powf(0.5) < 150.).fold((0.,0.), |acc, e| (acc.0 + e.x, acc.1 + e.y));
            center.0 /= points.len() as f64;
            center.1 /= points.len() as f64;
        }
    }
    return center;
}

#[derive(Debug)]
pub struct App {
    entities: VecDeque<Body>,
    trail: VecDeque<Body>,
    logs: VecDeque<String>,
    speed: i64,
    force_g: f64,
    drag: f64,
    exit: bool,
    reset: bool,
    pause: bool,
    id_counter: usize,
    zoom: usize,
    fps: u64,
}

impl App {
    pub fn new() -> App {
        App {
            entities: vec![Body::rand(0), Body::rand(1), Body::rand(2)].into(), //Body::new(0.,0.), Body::new(40.,40.), Body::new(100.,0.)],
            trail: VecDeque::new(),
            logs: VecDeque::new(), 
            speed: 5,
            force_g: 1e2,
            drag: 0.99,
            exit: false,
            reset: false,
            pause: false,
            id_counter: 3,
            zoom: 10,
            fps: 60,
        }
    }

    pub fn run(&mut self, terminal: &mut tui::Tui) -> Result<()> {
        while !self.exit {
            if self.reset {
                self.entities = vec![Body::rand(0), Body::rand(1), Body::rand(2)].into();
                self.trail = VecDeque::new();
                self.reset = false;
                self.id_counter = 3;
            }

            let begin_time = Instant::now();
            // Update bodies
            if !self.pause {
                self.update_entities();
            }
            terminal.draw(|frame| self.render_frame(frame))?;
            if event::poll(std::time::Duration::from_millis(16))? {
                self.handle_events().wrap_err("handle events failed")?;
            }
            let delta_time = (Instant::now() - begin_time).as_millis();
            self.fps = ((self.fps as f64 * 0.99) + (1000. / delta_time as f64 * 0.01)) as u64;
        }
        Ok(())
    }

    fn zoom_in(&mut self) {
        if self.zoom > 1 {
            self.zoom -= 1;
        }
    }

    fn zoom_out(&mut self) {
        if self.zoom < 20 {
            self.zoom += 1;
        }
    }

    fn log(&mut self, log_text: &str) {
        self.logs.push_back(log_text.to_string());
        if self.logs.len() > 100 {
            self.logs.pop_front();
        }
    }

    fn nlogs(&self, n: usize) -> String {
        let mut s = String::new();
        for i in self.logs.len() - n.min(self.logs.len())..self.logs.len() {
            s.push_str(&self.logs[i]);
            s.push('\n');
        }
        s
    }

    fn add_entity(&mut self) {
        self.entities.push_back(Body::rand(self.id_counter));
        self.id_counter += 1;
    }

    fn remove_entity(&mut self) {
        // TODO: program freezes when removing last entity
        self.entities.pop_front();
    }

    fn update_entities(&mut self) {
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
                let force = self.force_g * a.mass * b.mass / r.powi(2);
                let ab_hat = ((a.x - b.x) / r, (a.y - b.y) / r);
                forces[i] = (
                    forces[i].0 - ab_hat.0 * force,
                    forces[i].1 - ab_hat.1 * force,
                );
                forces[j] = (
                    forces[j].0 + ab_hat.0 * force,
                    forces[j].1 + ab_hat.1 * force,
                );
                self.log(&format!("[{i},{j}] r: {r:?}"));
            }
        }

        // Apply forces
        for i in 0..self.entities.len() {
            self.entities[i].step(forces[i], self.speed as f64, self.drag);
            self.log(&format!("[{i}] force: {:?}", forces[i]));
        }

        let centroid = ransac_centroid(&self.entities);

        self.entities.iter_mut().for_each(|e| e.x -= 0.1*centroid.0);
        self.entities.iter_mut().for_each(|e| e.y -= 0.1*centroid.1);
        self.trail.iter_mut().for_each(|e| e.x -= 0.1*centroid.0);
        self.trail.iter_mut().for_each(|e| e.x -= 0.1*centroid.0);
    }

    fn render_frame(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.size());
    }

    fn handle_events(&mut self) -> Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event)
            }
            _ => {}
        };
        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            KeyCode::Char('a') => self.add_entity(),
            KeyCode::Char('x') => self.remove_entity(),
            KeyCode::Char(' ') => self.pause = !self.pause,
            KeyCode::Char('r') => self.reset = true,
            KeyCode::Left => self.speed -= 1,
            KeyCode::Right => self.speed += 1,
            // KeyCode::Char('-') => self.force_g *= 0.1,
            // KeyCode::Char('+') => self.force_g *= 10.,
            KeyCode::Char('-') => self.zoom_in(),
            KeyCode::Char('+') => self.zoom_out(),
            KeyCode::Up => self.drag += 0.01,
            KeyCode::Down => self.drag -= 0.01,
            _ => {}
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(75), Constraint::Percentage(25)])
            .split(area);
        let dbg_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(layout[1]);
        let entity_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(dbg_layout[0]);

        let simulation_block = Block::default()
            .title(Title::from(" Three Body Simulation ".bold()).alignment(Alignment::Center))
            .title(
                Title::from(Line::from(vec![
                    " Quit ".into(),
                    "<Q> ".blue().bold(),
                    " Reset ".into(),
                    "<R> ".blue().bold(),
                    " Pause ".into(),
                    "<Spacebar> ".blue().bold(),
                ]))
                .alignment(Alignment::Center)
                .position(Position::Bottom),
            )
            .borders(Borders::ALL)
            .border_set(border::THICK);

        Canvas::default()
            .block(simulation_block)
            .x_bounds([-10.0 * self.zoom as f64, 10.0 * self.zoom as f64])
            .y_bounds([-10.0 * self.zoom as f64, 10.0 * self.zoom as f64])
            .paint(|ctx| {
                // self.trail
                //     .iter()
                //     .for_each(|e| ctx.print(e.x, e.y, e.icon.print()));
                self.entities
                    .iter()
                    .for_each(|e| ctx.print(e.x, e.y, e.icon.print()));
            })
            .render(layout[0], buf);

        // Entity Info
        let entity_block = Block::default()
            .title(Title::from(" Entity Info ".bold()).alignment(Alignment::Left))
            .title(Title::from(format!(" {} entities! ", self.entities.len()).bold()).alignment(Alignment::Right))
            .title(
                Title::from(Line::from(vec![
                    " Add ".into(),
                    "<A> ".blue().bold(),
                    " Remove ".into(),
                    "<X> ".blue().bold(),
                ]))
                .alignment(Alignment::Center)
                .position(Position::Bottom),
            )
            .borders(Borders::ALL)
            .border_set(border::THICK);
        let mut entity_info = String::new();
        for e in &self.entities {
            entity_info.push_str(&e.icon.print().to_string());
            entity_info.push_str(&format!("{e}\n"));
        }
        Paragraph::new(Text::from(
            self.entities
                .iter()
                .map(|e| e.to_line())
                .collect::<Vec<_>>(),
        ))
        .block(entity_block)
        .render(entity_layout[0], buf);

        // Simulation Settings
        let params_block = Block::default()
            .title(Title::from(" Simulation Settings ".bold()).alignment(Alignment::Left))
            .borders(Borders::ALL)
            .border_set(border::THICK);
        Paragraph::new(Text::from(vec![
            format!("Speed (DT): {}\t <Left/Right>", self.speed).into(),
            format!("Force (G):  {}\t <-/+>", self.force_g).into(),
            format!("Drag:       {:.2}\t <Up/Down>", self.drag).into(),
            format!("Zoom:       {:.2}\t <-/+>", self.zoom).into(),
        ]))
        .block(params_block)
        .render(entity_layout[1], buf);

        let log_block = Block::default()
            .title(Title::from(" Logs ".bold()).alignment(Alignment::Left))
            .title(Title::from(format!(" {}fps ", self.fps).bold()).alignment(Alignment::Right).position(Position::Bottom))
            .borders(Borders::ALL)
            .border_set(border::THICK);
        Paragraph::new(self.nlogs(10))
            .block(log_block)
            .render(dbg_layout[1], buf);
    }
}

fn main() -> Result<()> {
    errors::install_hooks()?;
    let mut terminal = tui::init()?;
    let app_result = App::new().run(&mut terminal);
    tui::restore()?;
    app_result
}

