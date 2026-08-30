#[tokio::main]
async fn main() {
    let config = match garive_tui::parse_launch_config(std::env::args_os()) {
        Ok(config) => config,
        Err(garive_tui::LaunchParseError::Display(text)) => {
            print!("{text}");
            return;
        }
        Err(error) => {
            eprintln!("garive-tui: {error}");
            std::process::exit(2);
        }
    };
    if let Err(error) = garive_tui::run(config).await {
        if let garive_tui::TuiError::Interrupted(signal) = error {
            std::process::exit(128 + signal);
        }
        eprintln!("garive-tui: {error}");
        std::process::exit(1);
    }
}
