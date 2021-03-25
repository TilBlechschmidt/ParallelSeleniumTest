use anyhow::{bail, Result};
use std::time::Duration;
use thirtyfour::prelude::*;
use thirtyfour_query::{ElementPoller, ElementQueryable};
use tokio::spawn;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let endpoint = &args[1];
    let count: i32 = args[2].parse::<i32>().unwrap();

    println!("Running {} tests against '{}'", count, endpoint);

    let mut failed = 0;
    let mut handles = Vec::new();

    for _ in 0..count {
        let endpoint = endpoint.clone();
        let handle = spawn(async move { run_test(&endpoint.clone()).await });
        handles.push(handle);
    }

    for (i, handle) in handles.into_iter().enumerate() {
        if let Err(e) = handle.await? {
            println!("Test failed: {}", e);
            failed += 1;
        } else {
            println!("Test #{} finished.", i);
        }
    }

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

async fn run_test(endpoint: &str) -> Result<()> {
    let caps = DesiredCapabilities::firefox();
    let mut driver = WebDriver::new(endpoint, &caps).await?;
    let session_id = driver.session_id().to_string();

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
        driver.quit().await?;
        bail!("Element not found on session {} :(", session_id);
    } else {
        send_message(&driver, "Found result!").await?;
        driver.quit().await?;
    }

    Ok(())
}

async fn send_message(driver: &WebDriver, message: &str) -> Result<()> {
    let cookie = Cookie::new("webgrid:message", serde_json::json!(message));
    driver.add_cookie(cookie).await?;
    // println!("{} ({})", message, driver.session_id());
    Ok(())
}
