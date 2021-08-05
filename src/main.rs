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
use thirtyfour::prelude::ElementQueryable;
use thirtyfour::{prelude::*, Capabilities};
use thirtyfour_query::ElementPoller;
use tokio::{spawn, time::sleep};

#[tokio::main]
async fn main() -> Result<()> {
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

    println!("Running {} tests against '{}'", count, endpoint);

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
                    println!("Test #{} finished in {}.", id, format_duration(duration));
                    Ok(())
                }
                Err(e) => {
                    println!("Test #{} failed: {}", id, e);
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

    println!(
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
    driver.get("https://duckduckgo.com").await?;
    send_message(&driver, "Visiting DuckDuckGo").await?;

    let form = driver
        .find_element(By::Id("search_form_input_homepage"))
        .await?;
    send_message(&driver, "Searching for webgrid.dev").await?;
    form.send_keys("webgrid.dev").await?;
    form.send_keys(Keys::Enter).await?;

    // Set the element polling
    driver
        .set_implicit_wait_timeout(Duration::new(0, 0))
        .await?;
    let poller =
        ElementPoller::TimeoutWithInterval(Duration::new(20, 0), Duration::from_millis(500));
    driver.config_mut().set("ElementPoller", poller)?;

    send_message(&driver, "Looking at results").await?;
    let results = driver.query(By::ClassName("result__a")).all().await?;

    let mut found = false;
    for result in results {
        let text = result.text().await?;
        if text.contains("WebGrid") {
            found = true;
            break;
        }
    }

    if !found {
        send_message(&driver, "No result.").await?;
        set_status(&driver, "failure").await?;
        bail!("Element not found :(");
    } else {
        send_message(&driver, "Found result!").await?;
        set_status(&driver, "success").await?;
    }

    Ok(())
}

async fn send_message(driver: &WebDriver, message: &str) -> Result<()> {
    let cookie = Cookie::new("webgrid:message", serde_json::json!(message));
    driver.add_cookie(cookie).await?;
    // println!("{} ({})", message, driver.session_id());
    Ok(())
}

async fn set_status(driver: &WebDriver, status: &str) -> Result<()> {
    let cookie = Cookie::new("webgrid:metadata.session:status", serde_json::json!(status));
    driver.add_cookie(cookie).await?;

    Ok(())
}
