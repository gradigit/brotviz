use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cfg = tui_visualizer::config::Config::parse();
    if cfg.list_devices {
        tui_visualizer::audio::list_input_devices()?;
        return Ok(());
    }

    tui_visualizer::app::run(cfg)
}
