fn main() {
    linker_be_nice();
    generate_config();
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

fn generate_config() {
    use serde::Deserialize;
    use std::fs;
    use std::path::Path;

    #[derive(Deserialize)]
    struct Config {
        wifi: WifiConfig,
        network: NetworkConfig,
        api: ApiConfig,
    }

    #[derive(Deserialize)]
    struct WifiConfig {
        ssid: String,
        password: String,
    }

    #[derive(Deserialize)]
    struct NetworkConfig {
        dhcp_timeout_ms: u32,
        link_check_interval_ms: u64,
        ip_check_interval_ms: u64,
    }

    #[derive(Deserialize)]
    struct ApiConfig {
        site_id: u32,
        mode: String,
        transport: String,
        direction: u8,
        line: Option<u16>,
        forecast: u16,
        request_interval_secs: u64,
        max_retries: u32,
        retry_delay_secs: u64,
        min_response_size: usize,
        #[serde(default = "default_buffer_size")]
        buffer_size: usize,
    }

    fn default_buffer_size() -> usize {
        8192
    }

    // Read and parse config.toml
    let config_path = Path::new("config.toml");
    println!("cargo:rerun-if-changed=config.toml");

    let config_content = fs::read_to_string(config_path).expect("Failed to read config.toml");

    let config: Config = toml::from_str(&config_content).expect("Failed to parse config.toml");

    // Generate config.rs
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let config_rs_path = Path::new(&out_dir).join("config.rs");

    let line_const = match config.api.line {
        Some(line) => format!("Some({})", line),
        None => "None".to_string(),
    };

    let config_rs_content = format!(
        r#"// Auto-generated from config.toml - DO NOT EDIT

pub mod wifi {{
    pub const SSID: &str = "{}";
    pub const PASSWORD: &str = "{}";
}}

pub mod network {{
    pub const DHCP_TIMEOUT_MS: u32 = {};
    pub const LINK_CHECK_INTERVAL_MS: u64 = {};
    pub const IP_CHECK_INTERVAL_MS: u64 = {};
}}

pub mod api {{
    pub const SITE_ID: u32 = {};
    pub const MODE: &str = "{}";
    pub const TRANSPORT: &str = "{}";
    pub const DIRECTION: u8 = {};
    pub const LINE: Option<u16> = {};
    pub const FORECAST: u16 = {};
    pub const REQUEST_INTERVAL_SECS: u64 = {};
    pub const MAX_RETRIES: u32 = {};
    pub const RETRY_DELAY_SECS: u64 = {};
    pub const MIN_RESPONSE_SIZE: usize = {};
    pub const BUFFER_SIZE: usize = {};

    // Build full URL at compile time
    pub fn build_url() -> heapless::String<256> {{
        use core::fmt::Write;
        let mut url = heapless::String::new();
        write!(&mut url, "https://transport.integration.sl.se/v1/sites/{{}}/{{}}?transport={{}}&direction={{}}",
            SITE_ID, MODE, TRANSPORT, DIRECTION).unwrap();

        // Add line parameter only if present
        if let Some(line) = LINE {{
            write!(&mut url, "&line={{}}", line).unwrap();
        }}

        write!(&mut url, "&forecast={{}}", FORECAST).unwrap();
        url
    }}
}}
"#,
        config.wifi.ssid,
        config.wifi.password,
        config.network.dhcp_timeout_ms,
        config.network.link_check_interval_ms,
        config.network.ip_check_interval_ms,
        config.api.site_id,
        config.api.mode,
        config.api.transport,
        config.api.direction,
        line_const,
        config.api.forecast,
        config.api.request_interval_secs,
        config.api.max_retries,
        config.api.retry_delay_secs,
        config.api.min_response_size,
        config.api.buffer_size,
    );

    fs::write(config_rs_path, config_rs_content).expect("Failed to write generated config.rs");
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                what if what.starts_with("_defmt_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`"
                    );
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                what if what.starts_with("esp_rtos_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `esp-radio` has no scheduler enabled. Make sure you have initialized `esp-rtos` or provided an external scheduler."
                    );
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!(
                        "💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests"
                    );
                    eprintln!();
                }
                "free"
                | "malloc"
                | "calloc"
                | "get_free_internal_heap_size"
                | "malloc_internal"
                | "realloc_internal"
                | "calloc_internal"
                | "free_internal" => {
                    eprintln!();
                    eprintln!(
                        "💡 Did you forget the `esp-alloc` dependency or didn't enable the `compat` feature on it?"
                    );
                    eprintln!();
                }
                _ => (),
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=-Wl,--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
