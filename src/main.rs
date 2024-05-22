use color_eyre::Result;

use three_body_tui::simulations::Simulatable;
use three_body_tui::simulations::NBody;
use three_body_tui::{tui, errors};

fn main() -> Result<()> {
    errors::install_hooks()?;
    let mut terminal = tui::init()?;

    let mut simulation = NBody::init();
    let app_result = simulation.run(&mut terminal);

    tui::restore()?;
    app_result
}
