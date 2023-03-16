use std::str::FromStr;

use anyhow::{anyhow, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::StatusCode;
use tokio::io::AsyncWriteExt;

fn style(s: &'static str) -> ProgressStyle {
    ProgressStyle::with_template(s)
        .unwrap()
        .progress_chars("#>-")
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Audible customer id
    #[arg(long)]
    customer_id: String,

    /// SKU of the book to download
    sku: String,

    /// Output file
    #[arg(short, long)]
    output: Option<String>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

struct ContentRange {
    start: u64,
    end: u64,
    total: u64,
}

impl FromStr for ContentRange {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s
            .strip_prefix("bytes ")
            .ok_or_else(|| anyhow!("Invalid Content-Range header"))?;

        let parts = s
            .split(['-', '/'])
            .map(|s| s.parse::<u64>())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            start: parts[0],
            end: parts[1],
            total: parts[2],
        })
    }
}

async fn update_progress_bar(pb: ProgressBar) {
    while !pb.is_finished() {
        pb.tick();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();

    let url = format!(
        "https://cds.audible.com/download?user_id={}&product_id={}&codec=LC_128_44100_Stereo&awtype=AAX&cust_id={}",
        args.customer_id,
        args.sku,
        args.customer_id,
    );

    let output = args.output.unwrap_or_else(|| format!("{}.aax", args.sku));

    let style_downloading =
        style("[{elapsed_precise}] [{bar:35.cyan/blue}] {bytes}/{total_bytes} ({eta})");
    let style_init = style("[{elapsed_precise}] [{bar:35.cyan/blue}] {msg}");

    // Initialize progress bar
    let pb = ProgressBar::new_spinner();
    pb.set_message("Initiating download...");
    pb.set_style(style_init.clone());
    tokio::spawn(update_progress_bar(pb.clone()));

    // Create reqwest client
    let client = reqwest::Client::builder().build()?;

    loop {
        // Get file size of existing file
        let start = match tokio::fs::metadata(&output).await {
            Ok(metadata) => metadata.len(),
            // Ignore if file doesn't exist
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
            // Propagate all other errors
            Err(e) => return Err(e.into()),
        };

        if args.verbose {
            pb.println(format!("Downloading from offset {}", start));
        }

        // Send the request with the range header
        let mut res = client
            .get(&url)
            .header("Range", format!("bytes={}-", start))
            .header(
                "User-Agent",
                "Audible ADM 6.6.0.19;Windows Vista  Build 9200",
            )
            .send()
            .await?;

        match res.status() {
            StatusCode::PARTIAL_CONTENT => {}
            StatusCode::RANGE_NOT_SATISFIABLE => {
                pb.finish();
                eprintln!("Download complete: {}", output);
                return Ok(());
            }
            code => return Err(anyhow!("Invalid status code: {code}")),
        }

        // Parse Content-Range header
        let content_range: ContentRange = res
            .headers()
            .get("Content-Range")
            .ok_or_else(|| anyhow!("Missing Content-Range header"))?
            .to_str()?
            .parse()?;

        if content_range.start != start {
            return Err(anyhow!("Server returned invalid start offset"));
        }

        if content_range.end != content_range.total - 1 {
            return Err(anyhow!("Server returned invalid end offset"));
        }

        pb.set_style(style_downloading.clone());
        pb.set_length(content_range.total);
        pb.set_position(start);
        pb.reset_eta();

        // Open file for appending
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&output)
            .await?;

        // Download data
        loop {
            match res.chunk().await {
                Ok(Some(chunk)) => {
                    file.write_all(&chunk).await?;
                    pb.inc(chunk.len() as u64);
                }
                // The entire file has been downloaded
                Ok(None) => {
                    pb.finish();
                    eprintln!("Download complete: {}", output);
                    return Ok(());
                }
                // Retry on error
                Err(e) => {
                    if args.verbose {
                        pb.println(format!("Error: {}", e));
                    }

                    pb.set_message("Restarting download...");
                    pb.set_style(style_init.clone());

                    // Close and flush file
                    file.shutdown().await?;

                    // Wait a bit before retrying
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

                    break;
                }
            }
        }
    }
}
