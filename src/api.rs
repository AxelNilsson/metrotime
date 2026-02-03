//! API client for Stockholm metro departures
//!
//! Base URL: https://transport.integration.sl.se/v1/sites/{site_id}/{mode}
//! where:
//! - site_id: The station ID (e.g., 9144 for Hammarbyhöjden)
//! - mode: "departures" or "arrivals"

use embassy_net::{
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use embassy_time::{Duration, with_timeout};
use log::{error, info};
use reqwless::client::{HttpClient, TlsConfig};

use crate::types::ApiResponse;

// Include generated config
include!(concat!(env!("OUT_DIR"), "/config.rs"));

/// Fetch departure information from the API
/// Returns: Vec<(line, destination, time)>
pub async fn fetch_departures(
    stack: &embassy_net::Stack<'_>,
    tls_seed: u64,
) -> Result<
    heapless::Vec<
        (
            heapless::String<8>,
            heapless::String<32>,
            heapless::String<16>,
        ),
        10,
    >,
    &'static str,
> {
    let mut rx_buffer = [0; api::BUFFER_SIZE];
    let mut tx_buffer = [0; 4096];
    let dns = DnsSocket::new(*stack);
    // Increase socket count from 1 to 2 for better resilience
    let tcp_state = TcpClientState::<2, { api::BUFFER_SIZE }, 4096>::new();
    let tcp = TcpClient::new(*stack, &tcp_state);

    let tls = TlsConfig::new(
        tls_seed,
        &mut rx_buffer,
        &mut tx_buffer,
        reqwless::client::TlsVerify::None,
    );

    let mut client = HttpClient::new_with_tls(&tcp, &dns, tls);
    let mut buffer = [0u8; api::BUFFER_SIZE];

    // Build URL from config
    let url = api::build_url();

    info!("Making HTTP request to: {}", url);
    info!("Creating HTTP request...");

    // Add 20 second timeout for creating request (DNS + TCP connect + TLS handshake)
    let mut http_req = with_timeout(
        Duration::from_secs(20),
        client.request(reqwless::request::Method::GET, url.as_str()),
    )
    .await
    .map_err(|_| {
        error!("Timeout creating HTTP request after 20 seconds");
        "Request creation timeout"
    })?
    .map_err(|e| {
        error!("Failed to create HTTP request: {:?}", e);
        "Failed to create HTTP request"
    })?;

    info!("Sending HTTP request...");

    // Add 15 second timeout for sending request and receiving response
    let response = with_timeout(Duration::from_secs(15), http_req.send(&mut buffer))
        .await
        .map_err(|_| {
            error!("Timeout sending HTTP request after 15 seconds");
            "Request send timeout"
        })?
        .map_err(|e| {
            error!("Failed to send HTTP request: {:?}", e);
            "Failed to send HTTP request"
        })?;

    info!("Got response!");

    let status = response.status;
    info!("Got response from server - Status: {:?}", status);

    // Add 10 second timeout for reading response body
    let res = with_timeout(Duration::from_secs(10), response.body().read_to_end())
        .await
        .map_err(|_| {
            error!("Timeout reading response body after 10 seconds");
            "Response read timeout"
        })?
        .map_err(|_| "Failed to read response body")?;

    let content = core::str::from_utf8(res).map_err(|_| "Response is not valid UTF-8")?;

    info!("Response ({} bytes)", res.len());

    // Check if response is suspiciously small
    if res.len() < api::MIN_RESPONSE_SIZE {
        error!("Response too small, might be an error: {}", content);
        return Err("Response too small");
    }

    // Parse and display results
    parse_and_display(content)
}

/// Parse JSON and display departure information
fn parse_and_display(
    content: &str,
) -> Result<
    heapless::Vec<
        (
            heapless::String<8>,
            heapless::String<32>,
            heapless::String<16>,
        ),
        10,
    >,
    &'static str,
> {
    match serde_json_core::from_str::<ApiResponse>(content) {
        Ok((api_response, _)) => {
            info!("============================================================");
            if let Some(line) = api::LINE {
                info!("Metro Departures for Line {}", line);
            } else {
                info!("Metro Departures");
            }
            info!("============================================================");

            let mut departures_list = heapless::Vec::new();

            if api_response.departures.is_empty() {
                info!("No departures found");
            } else {
                for (idx, departure) in api_response.departures.iter().enumerate() {
                    info!("--- Departure {} ---", idx + 1);
                    info!(
                        "  Line: {} ({})",
                        departure.line.designation, departure.line.group_of_lines
                    );
                    info!("  Destination: {}", departure.destination);
                    info!("  Display: {}", departure.display);
                    info!("  State: {}", departure.state);
                    info!("  Scheduled: {}", departure.scheduled);
                    info!("  Expected: {}", departure.expected);
                    info!(
                        "  Stop: {} (Platform {})",
                        departure.stop_point.name, departure.stop_point.designation
                    );

                    // Only add non-cancelled departures to the display list
                    if departure.state != "CANCELLED" {
                        let line = heapless::String::try_from(departure.line.designation)
                            .unwrap_or_default();
                        let dest =
                            heapless::String::try_from(departure.destination).unwrap_or_default();
                        let time =
                            heapless::String::try_from(departure.display).unwrap_or_default();
                        departures_list.push((line, dest, time)).ok();
                    } else {
                        info!("  -> Skipping cancelled departure");
                    }
                }
            }

            info!("============================================================");
            Ok(departures_list)
        }
        Err(e) => {
            error!("Failed to parse JSON: {:?}", e);
            error!("Raw response: {}", content);
            Err("Failed to parse JSON")
        }
    }
}
