pub mod ab;
pub mod suite;

use ab::DecryptAbArgs;
use clap::{Args, Subcommand};
use suite::DecryptSuiteArgs;

use crate::error::CommandError;

#[derive(Debug, Subcommand)]
enum Commands {
    /// Decrypt assetbundles.
    Ab(DecryptAbArgs),
    /// Decrypt suitemaster files.
    Suite(DecryptSuiteArgs),
}

#[derive(Debug, Args)]
pub struct DecryptArgs {
    #[command(subcommand)]
    command: Commands,
}

/// Command handler for the decrypt subcommand.
pub async fn decrypt(args: DecryptArgs) -> Result<(), CommandError> {
    match args.command {
        Commands::Ab(args) => ab::decrypt_ab(&args).await,
        Commands::Suite(args) => suite::decrypt_suite(args).await,
    }
}
