use anyhow::{bail, Result};
use humantime::format_duration;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use thirtyfour::{prelude::*, Capabilities, ExtensionCommand};
use tokio::{spawn, time::sleep};

const DEMO_BODY: &'static str = include_str!("site.html");

struct WebgridMetadataCommand {
    fields: HashMap<String, String>,
}

impl WebgridMetadataCommand {
    pub fn new() -> Self {
        Self {
            fields: HashMap::new(),
        }
    }

    pub fn with_field(key: String, value: String) -> Self {
        let mut instance = Self::new();
        instance.add(key, value);
        instance
    }

    pub fn add(&mut self, key: String, value: String) {
        self.fields.insert(key, value);
    }
}

impl ExtensionCommand for WebgridMetadataCommand {
    fn parameters_json(&self) -> Option<serde_json::Value> {
        serde_json::to_value(self.fields.clone()).ok()
    }

    fn method(&self) -> thirtyfour::RequestMethod {
        thirtyfour::RequestMethod::Post
    }

    fn endpoint(&self) -> String {
        "/webgrid/metadata".into()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::formatted_timed_builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let args: Vec<String> = std::env::args().collect();

    let endpoint = &args[1];
    let count = args[2].parse::<u64>().unwrap();
    let browser = if args.len() > 3 {
        args[3].to_ascii_lowercase()
    } else {
        "firefox".to_owned()
    };

    let timeout_secs = std::env::var("TIMEOUT")
        .unwrap_or("600".into())
        .parse::<u64>()
        .expect("Failed to parse timeout!");
    let timeout = Some(Duration::from_secs(timeout_secs));

    log::info!("Running {} tests against '{}'", count, endpoint);

    let mut handles = Vec::new();

    let failed = Arc::new(AtomicU64::new(0));

    for id in 0..count {
        let failed = failed.clone();
        let endpoint = endpoint.clone();
        let browser = browser.clone();
        let handle = spawn(async move {
            // Wait a tiny bit to stagger the requests
            sleep(Duration::from_millis(id * 25)).await;

            // Run the test
            let start = Instant::now();
            let result = run_test(&endpoint.clone(), &browser.clone(), timeout.clone()).await;
            let duration = Instant::now() - start;

            // Report the result (and duration)
            match result {
                Ok(_) => {
                    log::info!("Test #{} finished in {}.", id, format_duration(duration));
                    Ok(())
                }
                Err(e) => {
                    log::info!("Test #{} failed: {}", id, e);
                    failed.fetch_add(1, Ordering::Relaxed);
                    Err(e)
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles.into_iter() {
        handle.await?.ok();
    }

    let failed = failed.load(Ordering::SeqCst);

    log::info!(
        "All tests finished. {} / {} succeeded.",
        count - failed,
        count
    );

    if failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

async fn run_test(endpoint: &str, browser: &str, timeout: Option<Duration>) -> Result<()> {
    let mut metadata = HashMap::new();
    metadata.insert("name", "test-name");
    metadata.insert("build", "test-build");

    let mut driver = if browser == "firefox" {
        let mut caps = DesiredCapabilities::firefox();
        caps.add_subkey("webgrid:options", "metadata", metadata)?;
        WebDriver::new_with_timeout(endpoint, &caps, timeout).await?
    } else if browser == "chrome" {
        let mut caps = DesiredCapabilities::chrome();
        caps.add_subkey("webgrid:options", "metadata", metadata)?;
        WebDriver::new_with_timeout(endpoint, &caps, timeout).await?
    } else if browser == "safari" {
        let mut caps = DesiredCapabilities::safari();
        caps.add_subkey("webgrid:options", "metadata", metadata)?;
        WebDriver::new_with_timeout(endpoint, &caps, timeout).await?
    } else {
        bail!("Unknown browser!");
    };

    let session_id = driver.session_id().to_string();

    if let Err(e) = run_test_content(&mut driver).await {
        driver.quit().await.ok();
        bail!("{} failed due to {}", session_id, e);
    } else {
        driver.quit().await.ok();
    }

    Ok(())
}

async fn run_test_content(driver: &mut WebDriver) -> Result<()> {
    send_message(&driver, "Visiting demo page").await?;
    let page = format!(
        "data:text/html;charset=utf-8;base64,{}",
        base64::encode(DEMO_BODY)
    );

    driver.get(&page).await?;

    // 0. Set some runtime metadata if the driver supports it
    let metadata_command = WebgridMetadataCommand::with_field("answer".into(), "42".into());
    driver.extension_command(metadata_command).await.ok();

    // 1. Check that the `h1` contains the correct title
    send_message(&driver, "Checking title").await?;
    let title = driver.find_element(By::Tag("h1")).await?.text().await?;
    if !title.eq_ignore_ascii_case("Horrible looking test-page") {
        send_message(&driver, "Title mismatch.").await?;
        set_status(&driver, "failure").await?;
        bail!("Title mismatched :(");
    }

    // 2. Check that pressing the `#increment` button increments the `#counter`
    send_message(&driver, "Checking increment").await?;
    let counter = driver.find_element(By::Id("counter")).await?;
    let value = counter.text().await?.parse::<i32>()?;
    driver
        .find_element(By::Id("increment"))
        .await?
        .click()
        .await?;
    let new_value = counter.text().await?.parse::<i32>()?;
    if (value + 1) != new_value {
        send_message(&driver, "Increment is broken.").await?;
        set_status(&driver, "failure").await?;
        bail!("Increment is broken :(");
    }

    // 3. Check that entering a new hash value actually works
    send_message(&driver, "Checking hash value").await?;
    let expected_hash = "No emojis allowed here :(";
    let hash_input = driver.find_element(By::Id("newHashValue")).await?;
    hash_input.send_keys(expected_hash).await?;
    hash_input.send_keys(Keys::Enter).await?;
    let hash = driver
        .find_element(By::Id("hashValue"))
        .await?
        .text()
        .await?;
    if hash != expected_hash {
        send_message(&driver, "Hash value updating is broken.").await?;
        set_status(&driver, "failure").await?;
        bail!(
            "Hash value updating is broken: {} != {}",
            hash,
            expected_hash
        );
    }

    send_message(&driver, "It worked!").await?;
    set_status(&driver, "success").await?;

    Ok(())
}

async fn send_message(driver: &WebDriver, message: &str) -> Result<()> {
    let cookie = Cookie::new("webgrid:message", serde_json::json!(message));
    driver.add_cookie(cookie).await.ok();
    Ok(())
}

async fn set_status(driver: &WebDriver, status: &str) -> Result<()> {
    let cookie = Cookie::new("webgrid:metadata.session:status", serde_json::json!(status));
    driver.add_cookie(cookie).await.ok();

    Ok(())
}
