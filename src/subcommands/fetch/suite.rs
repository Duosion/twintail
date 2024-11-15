use std::{path::Path, time::Duration};

use crate::{
    api::{sekai_client::SekaiClient, url::UrlProvider},
    constants::{color, strings},
    error::CommandError,
    models::enums::{Platform, Server},
    utils::{
        fs::{extract_suitemaster_file, write_file},
        progress::{ProgressBar, WithProgress},
    },
};
use clap::Args;
use futures::{stream, StreamExt};
use tokio::time::Instant;
use tokio_retry::{strategy::FixedInterval, Retry};

#[derive(Debug, Args)]
pub struct SuiteArgs {
    /// The version of the game app get the suitemaster files for
    #[arg(short, long)]
    pub version: String,

    /// The app hash to get the suitemaster files for
    #[arg(long)]
    pub hash: String,

    /// The device platform to get the suitemaster files for
    #[arg(short, long, value_enum, default_value_t = Platform::Android)]
    pub platform: Platform,

    /// The server to get the suitemaster files from
    #[arg(short, long, value_enum, default_value_t = Server::Japan)]
    pub server: Server,

    /// The maximum number of files to download simultaneously
    #[arg(long, short, default_value_t = crate::utils::available_parallelism())]
    pub concurrent: usize,

    /// The maximum number of times to retry a download if it fails
    #[arg(long, short, default_value_t = 3)]
    pub retry: usize,

    /// If set, the downloaded suitemaster files will not be decrypted.
    #[arg(long, short, default_value_t = false)]
    pub encrypt: bool,

    /// The directory to output the suitemaster files to
    pub out_dir: String,
}

/// Downloads a suitemasterfile at the provided path using the given SekaiClient.
///
/// This will unpack each suitemasterfile and save the contents to the provided out_path.
///
/// If encrypt is true, the suitemaster file will not be unpacked.
async fn download_suitemasterfile<T: UrlProvider>(
    client: &SekaiClient<T>,
    api_file_path: &str,
    out_path: &Path,
    encrypt: bool,
) -> Result<(), CommandError> {
    if encrypt {
        let file_bytes = client.get_suitemasterfile(api_file_path).await?;
        if let Some(file_name) = Path::new(api_file_path).file_name() {
            write_file(&out_path.join(file_name), &file_bytes).await?;
            Ok(())
        } else {
            Err(CommandError::NotFound(format!(
                "file name not found for api file path: {}",
                api_file_path
            )))
        }
    } else {
        let value = client.get_suitemasterfile_as_value(api_file_path).await?;
        extract_suitemaster_file(value, out_path).await
    }
}

pub async fn fetch_suite(args: SuiteArgs) -> Result<(), CommandError> {
    let mut client = SekaiClient::new(args.version, args.hash, args.platform, args.server).await?;

    // create communication spinner
    println!(
        "{}[1/2] {}{}",
        color::TEXT_VARIANT.render_fg(),
        color::TEXT.render_fg(),
        strings::command::COMMUNICATING,
    );
    let login_spinner = ProgressBar::spinner();

    // see what suite master split files are available for download
    let user_signup = client.user_signup().await?;
    let user_login = client
        .user_login(
            user_signup.user_registration.user_id,
            user_signup.credential,
        )
        .await?;

    login_spinner.finish();

    // create download progress bar
    let suitemaster_split_paths = user_login.suite_master_split_path;
    let split_count = suitemaster_split_paths.len();

    println!(
        "{}[2/2] {}{}",
        color::TEXT_VARIANT.render_fg(),
        color::TEXT.render_fg(),
        strings::command::DOWNLOADING,
    );
    let download_progress = ProgressBar::progress(split_count as u64);
    download_progress.enable_steady_tick(Duration::from_millis(100));

    // download suite master split files
    let out_path = Path::new(&args.out_dir);
    let retry_strat = FixedInterval::from_millis(200).take(args.retry);
    let download_start = Instant::now();
    let do_encrypt = args.encrypt;

    let download_results: Vec<Result<(), CommandError>> = stream::iter(&suitemaster_split_paths)
        .map(|api_path| async {
            Retry::spawn(retry_strat.clone(), || {
                download_suitemasterfile(&client, api_path, out_path, do_encrypt)
                    .with_progress(&download_progress)
            })
            .await
        })
        .buffer_unordered(args.concurrent)
        .collect()
        .await;

    // stop progress bar
    download_progress.finish_and_clear();

    // print result
    let success_count = download_results
        .iter()
        .filter(|&result| result.is_ok())
        .count();

    println!(
        "{}Successfully {} {} / {} files in {:?}{}",
        color::SUCCESS.render_fg(),
        strings::command::DOWNLOADED,
        success_count,
        split_count,
        Instant::now().duration_since(download_start),
        color::TEXT.render_fg(),
    );

    Ok(())
}
