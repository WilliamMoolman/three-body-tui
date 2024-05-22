use color_eyre::Result;

mod errors;
mod tui;

use three_body_tui::simulations::NBody;
use three_body_tui::simulations::Simulatable;

fn main() -> Result<()> {
    errors::install_hooks()?;
    let mut terminal = tui::init()?;

    let mut simulation = NBody::init();
    let app_result = simulation.run(&mut terminal);

    tui::restore()?;
    app_result
}
