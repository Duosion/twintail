#[allow(dead_code)]
mod constants;
#[allow(dead_code)]
mod error;
mod subcommands;

use anstyle::{AnsiColor, Color};
use clap::{builder::Styles, Parser, Subcommand};
use constants::color;
use subcommands::{
    app_info,
    crypt::{decrypt, encrypt},
    fetch,
};

#[derive(Debug, Subcommand)]
enum Commands {
    /// Commands that fetch assets from the game
    Fetch(fetch::FetchArgs),
    /// Commands that decrypt files related to the game
    Decrypt(decrypt::DecryptArgs),
    /// Commands that encrypt files related to the game
    Encrypt(encrypt::EncryptArgs),
    /// Extract app version & hash from an apk file
    AppInfo(app_info::AppInfoArgs),
}

#[derive(Debug, Parser)]
#[command(version, about, long_about = None, styles=get_clap_styles())]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() -> Result<(), twintail::Error> {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Fetch(args) => fetch::fetch(args).await,
        Commands::Decrypt(args) => decrypt::decrypt(args).await,
        Commands::Encrypt(args) => encrypt::encrypt(args).await,
        Commands::AppInfo(args) => app_info::app_info(args),
    };

    // print error if result is an error
    if let Err(err) = result {
        println!(
            "{}{}{}",
            color::ERROR.render_fg(),
            err,
            color::TEXT.render_fg()
        )
    }

    Ok(())
}

/// Get styles for ``clap``.\
const USAGE: Color = Color::Ansi(AnsiColor::BrightBlue);
const HEADER: Color = Color::Ansi(AnsiColor::BrightBlue);
const LITERAL: Color = Color::Ansi(AnsiColor::BrightCyan);
const INVALID: Color = Color::Ansi(AnsiColor::Red);
const VALID: Color = Color::Ansi(AnsiColor::BrightCyan);
const PLACEHOLDER: Color = Color::Ansi(AnsiColor::White);
pub fn get_clap_styles() -> Styles {
    Styles::styled()
        .usage(anstyle::Style::new().bold().fg_color(Some(USAGE)))
        .header(anstyle::Style::new().bold().fg_color(Some(HEADER)))
        .literal(anstyle::Style::new().fg_color(Some(LITERAL)))
        .invalid(anstyle::Style::new().bold().fg_color(Some(INVALID)))
        .error(
            anstyle::Style::new()
                .bold()
                .fg_color(Some(constants::color::ERROR)),
        )
        .valid(
            anstyle::Style::new()
                .bold()
                .underline()
                .fg_color(Some(VALID)),
        )
        .placeholder(anstyle::Style::new().fg_color(Some(PLACEHOLDER)))
}
