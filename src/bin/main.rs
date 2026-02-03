#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_executor::Spawner;
use embassy_net::Runner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::Priority;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_hub75::Hub75;
use esp_radio::wifi::WifiDevice;
use esp_rtos::embassy::{Executor, InterruptExecutor};
use heapless::Vec;
use log::{debug, error, info};
use static_cell::StaticCell;

// Include generated config
include!(concat!(env!("OUT_DIR"), "/config.rs"));

// Timing constants (in milliseconds)
const DISPLAY_REFRESH_INTERVAL_MS: u64 = 5; // ~200fps for brightness
const SCROLL_UPDATE_INTERVAL_MS: u64 = 66; // ~15Hz scroll speed
const RENDER_UPDATE_INTERVAL_MS: u64 = 66; // Match scroll rate for smooth animation
const API_FETCH_INTERVAL_SECS: u64 = 20; // Fetch new data every 20 seconds

// Logging intervals
const DISPLAY_LOG_INTERVAL: u32 = 5000; // Log every 5000 frames
const SCROLL_LOG_INTERVAL: i32 = 100; // Log every 100 pixels
const RENDER_LOG_INTERVAL: u32 = 100; // Log every 100 frames

// Type alias for departure data: (line, destination, time)
type DepartureData = Vec<
    (
        heapless::String<8>,
        heapless::String<32>,
        heapless::String<16>,
    ),
    10,
>;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    error!("PANIC: {}", info);
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

// Create a static cell for the given type and value
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // Initialize logger from environment FIRST
    esp_println::logger::init_logger_from_env();

    info!("ESP32-S3 Metro Time Application starting...");

    // Initialize hardware
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    info!("HAL initialized with CPU clock: {:?}", CpuClock::max());

    // Framebuffer is allocated as static data (not heap), size reduced via 2-bit color depth
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_ints = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0);
    info!("RTOS started");

    // Initialize radio and WiFi
    info!("Initializing radio controller...");
    let radio_controller = &*mk_static!(
        esp_radio::Controller<'static>,
        esp_radio::init().unwrap_or_else(|e| {
            error!("Failed to initialize radio controller: {:?}", e);
            panic!("Radio init failed");
        })
    );

    info!("Initializing WiFi controller...");
    let (mut wifi_controller, interfaces) =
        esp_radio::wifi::new(radio_controller, peripherals.WIFI, Default::default())
            .unwrap_or_else(|e| {
                error!("Failed to initialize Wi-Fi controller: {:?}", e);
                panic!("WiFi init failed");
            });

    // Set up network stack
    info!("Setting up network stack...");
    static RESOURCES: StaticCell<embassy_net::StackResources<3>> = StaticCell::new();

    let rng = Rng::new();
    let net_seed = rng.random() as u64 | ((rng.random() as u64) << 32);
    let tls_seed = rng.random() as u64 | ((rng.random() as u64) << 32);

    let dhcp_config = embassy_net::DhcpConfig::default();
    let config = embassy_net::Config::dhcpv4(dhcp_config);

    let (stack, runner) = embassy_net::new(
        interfaces.sta,
        config,
        RESOURCES.init(embassy_net::StackResources::new()),
        net_seed,
    );

    // Spawn network task
    spawner.spawn(net_task(runner)).ok();

    // Connect WiFi (keep controller alive by storing it)
    let station_config = esp_radio::wifi::ModeConfig::Client(
        esp_radio::wifi::ClientConfig::default()
            .with_ssid(wifi::SSID.into())
            .with_password(wifi::PASSWORD.into()),
    );

    info!("Configuring and connecting to WiFi: {}", wifi::SSID);
    wifi_controller.set_config(&station_config).unwrap();
    wifi_controller.start().unwrap();
    wifi_controller.connect().unwrap();

    // Store wifi_controller so it doesn't get dropped
    let _wifi_controller = wifi_controller;

    // Wait for network
    wait_for_network(&stack).await;

    // Give DNS time to fully initialize after getting IP
    info!("Waiting for DNS to initialize...");
    Timer::after(Duration::from_secs(2)).await;

    // Initialize LED matrix display
    info!("Initializing LED matrix display...");
    let display = match metrotimes3::display::init_display(
        peripherals.LCD_CAM,
        peripherals.DMA_CH0,
        peripherals.GPIO42,
        peripherals.GPIO41,
        peripherals.GPIO40,
        peripherals.GPIO38,
        peripherals.GPIO39,
        peripherals.GPIO37,
        peripherals.GPIO45,
        peripherals.GPIO36,
        peripherals.GPIO48,
        peripherals.GPIO35,
        peripherals.GPIO21,
        peripherals.GPIO14,
        peripherals.GPIO2,
        peripherals.GPIO47,
    ) {
        Ok(d) => {
            info!("Display initialized successfully");
            d
        }
        Err(e) => {
            error!("Failed to initialize display: {}", e);
            panic!("Display init failed");
        }
    };

    // Create framebuffer and initialize with content BEFORE starting refresh
    let fb = mk_static!(
        metrotimes3::display::DisplayFrameBuffer,
        metrotimes3::display::DisplayFrameBuffer::new()
    );

    // Fill entire screen black first
    use embedded_graphics::prelude::*;

    for y in 0..metrotimes3::display::DISPLAY_ROWS as i32 {
        for x in 0..metrotimes3::display::DISPLAY_COLS as i32 {
            fb.set_pixel(Point::new(x, y), esp_hub75::Color::BLACK);
        }
    }

    // Now draw "no departures" in GREEN using custom bitmap font
    metrotimes3::display::draw_departures(fb, &[], 0);

    // Wrap framebuffer in mutex for safe sharing
    let fb_mutex: &'static Mutex<
        CriticalSectionRawMutex,
        metrotimes3::display::DisplayFrameBuffer,
    > = mk_static!(
        Mutex<CriticalSectionRawMutex, metrotimes3::display::DisplayFrameBuffer>,
        Mutex::new(*fb)
    );

    // Create shared departure data storage
    let departure_data: &'static Mutex<CriticalSectionRawMutex, DepartureData> = mk_static!(
        Mutex<CriticalSectionRawMutex, DepartureData>,
        Mutex::new(Vec::new())
    );

    // Create shared scroll offset (using AtomicI32 for lock-free updates)
    use core::sync::atomic::AtomicI32;
    let scroll_offset: &'static AtomicI32 = mk_static!(AtomicI32, AtomicI32::new(0));

    // Start second core for display operations (dedicated core)
    // Architecture:
    //   Core 0: Network/API (fetch, parse, scroll calculation) - can have variable latency
    //   Core 1: Display only (refresh + render) - must be consistently fast for smooth visuals
    // This isolation prevents HTTP parsing stutters from affecting display rendering
    info!("Starting display tasks on Core 1 (dedicated)...");

    let cpu1_fn = {
        let display = display;
        let departure_data_core1 = departure_data;
        let scroll_offset_core1 = scroll_offset;
        move || {
            let hp_executor = mk_static!(
                InterruptExecutor<2>,
                InterruptExecutor::new(sw_ints.software_interrupt2)
            );
            let high_pri_spawner = hp_executor.start(Priority::Priority3);

            // Display refresh runs as high priority on dedicated core
            high_pri_spawner
                .spawn(display_refresh_task(display, fb_mutex))
                .ok();

            // Low priority executor for render task (still on Core 1, isolated from network/parsing)
            let lp_executor = mk_static!(Executor, Executor::new());

            // Render task runs on Core 1 to avoid stutters from Core 0 network/parsing work
            lp_executor.run(move |spawner| {
                spawner
                    .spawn(render_task(
                        fb_mutex,
                        departure_data_core1,
                        scroll_offset_core1,
                    ))
                    .ok();
            });
        }
    };

    use esp_hal::system::Stack as CpuStack;
    const DISPLAY_STACK_SIZE: usize = 8192;
    let app_core_stack = mk_static!(CpuStack<DISPLAY_STACK_SIZE>, CpuStack::new());

    esp_rtos::start_second_core(
        peripherals.CPU_CTRL,
        sw_ints.software_interrupt0,
        sw_ints.software_interrupt1,
        app_core_stack,
        cpu1_fn,
    );

    info!("Core 1 running: display refresh (high priority) + render task (low priority)");

    // Make stack static for data fetch task
    let stack = mk_static!(embassy_net::Stack<'static>, stack);

    // Start data fetch task on Core 0 (updates departure data, handles parsing)
    info!("Starting data fetch task on Core 0...");
    spawner
        .spawn(data_fetch_task(stack, tls_seed, departure_data))
        .expect("Failed to spawn data fetch task");

    // Start scroll task on Core 0 (updates scroll offset - lightweight)
    info!("Starting scroll task on Core 0...");
    spawner
        .spawn(scroll_task(departure_data, scroll_offset))
        .expect("Failed to spawn scroll task");

    // Main task just waits
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

/// Display refresh task - continuously refreshes the HUB75 display
#[embassy_executor::task]
async fn display_refresh_task(
    mut hub75: Hub75<'static, esp_hal::Blocking>,
    fb: &'static Mutex<CriticalSectionRawMutex, metrotimes3::display::DisplayFrameBuffer>,
) -> ! {
    info!("Display refresh task started");
    let mut counter = 0u32;
    loop {
        // Lock the framebuffer for the ENTIRE render cycle including DMA transfer
        // This prevents Core 0 from modifying the framebuffer while DMA is reading it
        let fb_locked = fb.lock().await;
        let xfer = hub75
            .render(&*fb_locked)
            .map_err(|(e, _)| e)
            .expect("failed to render");

        // Wait for DMA to complete BEFORE releasing the lock
        let (_result, new_hub75) = xfer.wait();
        hub75 = new_hub75;
        drop(fb_locked); // NOW it's safe to release

        counter += 1;
        if counter % DISPLAY_LOG_INTERVAL == 0 {
            debug!("Display: {} frames rendered", counter);
        }

        // Run at ~200fps to maintain brightness while allowing render task to update
        Timer::after(Duration::from_millis(DISPLAY_REFRESH_INTERVAL_MS)).await;
    }
}

/// Data fetching task - periodically fetches metro data
#[embassy_executor::task]
async fn data_fetch_task(
    stack: &'static embassy_net::Stack<'static>,
    tls_seed: u64,
    departure_data: &'static Mutex<CriticalSectionRawMutex, DepartureData>,
) -> ! {
    info!("Data fetch task started");

    loop {
        // Check network connectivity before making request
        if !stack.is_link_up() {
            error!("WiFi link is down, skipping fetch");
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }

        if stack.config_v4().is_none() {
            error!("No IP address, skipping fetch");
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }

        match metrotimes3::api::fetch_departures(stack, tls_seed).await {
            Ok(departures) => {
                info!("Fetched {} departures", departures.len());

                // Store departure data (scroll task will handle the display update)
                let mut data = departure_data.lock().await;
                *data = departures.clone();
                drop(data);

                info!("Updated departure data");
            }
            Err(e) => {
                error!("Failed to fetch data: {:?}", e);
            }
        }

        info!(
            "Waiting {} seconds until next fetch...",
            API_FETCH_INTERVAL_SECS
        );
        Timer::after(Duration::from_secs(API_FETCH_INTERVAL_SECS)).await;
    }
}

/// Scrolling task - updates scroll offset (lock-free)
#[embassy_executor::task]
async fn scroll_task(
    departure_data: &'static Mutex<CriticalSectionRawMutex, DepartureData>,
    scroll_offset: &'static core::sync::atomic::AtomicI32,
) -> ! {
    use core::sync::atomic::Ordering;

    info!("Scroll task started");
    let mut cached_departures: DepartureData = Vec::new();
    let mut total_width = 0;

    loop {
        // Get current departure data
        let data = departure_data.lock().await;
        let departures = data.clone();
        drop(data);

        // Check if stations/lines changed (ignore time changes)
        let stations_changed = departures.len() != cached_departures.len()
            || departures
                .iter()
                .zip(cached_departures.iter())
                .any(|(a, b)| {
                    // Only compare line and destination, not time
                    a.0 != b.0 || a.1 != b.1
                });

        if stations_changed {
            // Only reset scroll if actual stations changed
            cached_departures = departures.clone();
            scroll_offset.store(0, Ordering::Relaxed);

            if cached_departures.is_empty() || cached_departures.len() <= 1 {
                total_width = 0;
            } else {
                // Calculate total width of scrolling text
                total_width = 0;
                for i in 1..cached_departures.len() {
                    let (line, dest, time) = &cached_departures[i];
                    let entry_len = line.len() + 1 + dest.len() + 1 + time.len();
                    total_width += entry_len * 6;
                    if i > 1 {
                        total_width += 12; // Separator "  " = 2 chars = 12 pixels
                    }
                }
                debug!(
                    "Scroll: stations changed, recalculated total_width={}",
                    total_width
                );
            }
        } else if departures != cached_departures {
            // Times updated but stations are the same - just update cached data without resetting scroll
            cached_departures = departures.clone();
        }

        if total_width > 0 {
            // Update scroll offset (lock-free atomic operation)
            let current = scroll_offset.load(Ordering::Relaxed);
            // Reset after scrolling: text width + screen width
            // This lets the text fully scroll across the display before looping
            let reset_point = total_width as i32 + metrotimes3::display::DISPLAY_COLS as i32;
            let next = if current >= reset_point {
                0
            } else {
                current + 1
            };
            scroll_offset.store(next, Ordering::Relaxed);

            if next % SCROLL_LOG_INTERVAL == 0 {
                debug!("Scroll: offset={} (reset at {})", next, reset_point);
            }
        }

        // Scroll speed
        Timer::after(Duration::from_millis(SCROLL_UPDATE_INTERVAL_MS)).await;
    }
}

/// Render task - redraws the display with current scroll offset
#[embassy_executor::task]
async fn render_task(
    fb: &'static Mutex<CriticalSectionRawMutex, metrotimes3::display::DisplayFrameBuffer>,
    departure_data: &'static Mutex<CriticalSectionRawMutex, DepartureData>,
    scroll_offset: &'static core::sync::atomic::AtomicI32,
) -> ! {
    use core::sync::atomic::Ordering;

    info!("Render task started on Core 1 (isolated from Core 0 network/parsing)");
    let mut frame_count = 0u32;

    loop {
        // Get current departure data and scroll offset
        let data = departure_data.lock().await;
        let departures = data.clone();
        drop(data);

        let offset = scroll_offset.load(Ordering::Relaxed);

        // Redraw the display
        let mut fb_locked = fb.lock().await;

        // Only erase the display area (much faster than full erase)
        use embedded_graphics::prelude::*;
        for y in 0..32 {
            for x in 0..metrotimes3::display::DISPLAY_COLS as i32 {
                fb_locked.set_pixel(Point::new(x, y), esp_hub75::Color::BLACK);
            }
        }

        metrotimes3::display::draw_departures(&mut *fb_locked, &departures, offset);
        drop(fb_locked);

        frame_count += 1;
        if frame_count % RENDER_LOG_INTERVAL == 0 {
            debug!("Render: {} frames", frame_count);
        }

        // Render rate
        Timer::after(Duration::from_millis(RENDER_UPDATE_INTERVAL_MS)).await;
    }
}

/// Network task runner
#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}

/// Wait for network to be ready
async fn wait_for_network(stack: &embassy_net::Stack<'_>) {
    info!("Waiting for link to be up...");
    loop {
        if stack.is_link_up() {
            info!("Link is up!");
            break;
        }
        Timer::after(Duration::from_millis(network::LINK_CHECK_INTERVAL_MS)).await;
    }

    info!("Waiting for DHCP to assign IP address...");
    loop {
        if let Some(config) = stack.config_v4() {
            info!("Got IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(network::IP_CHECK_INTERVAL_MS)).await;
    }
}
