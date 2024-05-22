use color_eyre::{eyre::WrapErr, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use std::{cell::RefCell, collections::VecDeque, rc::Rc, time::Instant};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Stylize},
    text::{Line, Text},
    widgets::{canvas::Context, Widget},
    Frame,
};
use ratatui::{
    prelude::*,
    symbols::border,
    widgets::{block::*, canvas::*, Borders, Paragraph},
};

use crate::tui;
mod nbody;

pub use nbody::NBody;

pub trait Simulatable {
    fn init() -> Simulation
    where
        Self: Sized;
    fn reset(&mut self);
    fn handle_key_events(&mut self, key_event: KeyEvent);
    fn update(&mut self);

    fn canvas_title(&self) -> &str;
    fn canvas_bounds(&self) -> (f64, f64, f64, f64);
    fn canvas_render(&self, ctx: &mut Context);

    fn info_title(&self) -> &str;
    fn info_text(&self) -> Text;

    fn settings(&self) -> &SettingsBlock;
    fn settings_mut(&mut self) -> &mut SettingsBlock;
}

pub trait Settings {
    fn new() -> Rc<RefCell<Self>>
    where
        Self: Sized;
    fn text(&self) -> &str;
    fn value(&self) -> String;
    fn increment(&mut self);
    fn decrement(&mut self);
}

pub struct SettingsBlock {
    settings: Vec<Rc<RefCell<dyn Settings>>>,
    selected: usize,
}

impl SettingsBlock {
    fn up(&mut self) {
        if self.selected != 0 {
            self.selected -= 1;
        }
    }

    fn down(&mut self) {
        if self.selected < self.settings.len() - 1 {
            self.selected += 1;
        }
    }

    fn left(&mut self) {
        self.settings[self.selected].borrow_mut().decrement();
    }
    fn right(&mut self) {
        self.settings[self.selected].borrow_mut().increment();
    }
    fn render(&self) -> Text {
        Text::from(
            self.settings
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    if i == self.selected {
                        Line::from(format!("{}\t{}", s.borrow().text(), s.borrow().value()))
                            .bg(Color::Green)
                            .fg(Color::Black)
                    } else {
                        Line::from(format!("{}\t{}", s.borrow().text(), s.borrow().value()))
                    }
                })
                .collect::<Vec<_>>(),
        )
    }
}

#[derive(Clone)]
struct Logger {
    logs: Rc<RefCell<VecDeque<String>>>,
}

impl Logger {
    fn new() -> Logger {
        Logger {
            logs: Rc::new(RefCell::new(VecDeque::new())),
        }
    }
    fn len(&self) -> usize {
        self.logs.borrow().len()
    }
    fn log(&mut self, log_text: &str) {
        self.logs.borrow_mut().push_back(log_text.to_string());
        if self.len() > 100 {
            self.logs.borrow_mut().pop_front();
        }
    }

    fn get_logs(&self, n: usize) -> String {
        let mut s = String::new();
        for i in self.len() - n.min(self.len())..self.len() {
            s.push_str(&self.logs.borrow()[i]);
            s.push('\n');
        }
        s
    }
}

pub struct Simulation {
    exit: bool,
    reset: bool,
    pause: bool,
    logger: Logger,
    simulation: Box<dyn Simulatable>,
    fps: u64,
}

impl Simulation {
    pub fn run(&mut self, terminal: &mut tui::Tui) -> Result<()> {
        while !self.exit {
            if self.reset {
                self.simulation.reset();
                self.reset = false;
            }

            let begin_time = Instant::now();
            // Update bodies
            if !self.pause {
                self.simulation.update();
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
            KeyCode::Char(' ') => self.pause = !self.pause,
            KeyCode::Char('r') => self.reset = true,
            KeyCode::Left => self.simulation.settings_mut().left(),
            KeyCode::Right => self.simulation.settings_mut().right(),
            KeyCode::Up => self.simulation.settings_mut().up(),
            KeyCode::Down => self.simulation.settings_mut().down(),
            _ => self.simulation.handle_key_events(key_event),
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }
}

impl Widget for &Simulation {
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
            .title(Title::from(self.simulation.canvas_title().bold()).alignment(Alignment::Center))
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

        let (x1, x2, y1, y2) = self.simulation.canvas_bounds();
        Canvas::default()
            .block(simulation_block)
            .x_bounds([x1, x2])
            .y_bounds([y1, y2])
            .paint(|ctx| self.simulation.canvas_render(ctx))
            .render(layout[0], buf);

        // Entity Info
        let entity_block = Block::default()
            .title(Title::from(self.simulation.info_title().bold()).alignment(Alignment::Left))
            .borders(Borders::ALL)
            .border_set(border::THICK);
        Paragraph::new(self.simulation.info_text())
            .block(entity_block)
            .render(entity_layout[0], buf);

        // Simulation Settings
        let params_block = Block::default()
            .title(Title::from(" Simulation Settings ".bold()).alignment(Alignment::Left))
            .borders(Borders::ALL)
            .border_set(border::THICK);
        Paragraph::new(self.simulation.settings().render())
            .block(params_block)
            .render(entity_layout[1], buf);

        // Logs
        let log_block = Block::default()
            .title(Title::from(" Logs ".bold()).alignment(Alignment::Left))
            .title(
                Title::from(format!(" {}fps ", self.fps).bold())
                    .alignment(Alignment::Right)
                    .position(Position::Bottom),
            )
            .borders(Borders::ALL)
            .border_set(border::THICK);
        Paragraph::new(self.logger.get_logs(10))
            .block(log_block)
            .render(dbg_layout[1], buf);
    }
}
