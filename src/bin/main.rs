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
const DISPLAY_REFRESH_INTERVAL_MS: u64 = 2; // ~500fps for PWM brightness
const API_FETCH_INTERVAL_SECS: u64 = 20; // Fetch new data every 20 seconds

// Logging intervals
const DISPLAY_LOG_INTERVAL: u32 = 5000; // Log every 5000 frames
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

    // Dual heap setup: internal SRAM for WiFi/system, PSRAM for large buffers
    // WiFi needs internal SRAM (fast access), large buffers can use PSRAM
    esp_alloc::heap_allocator!(size: 120_000); // 120KB internal SRAM for WiFi/system
    esp_alloc::psram_allocator!(&peripherals.PSRAM, esp_hal::psram); // 2MB PSRAM for buffers

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

    // Double buffering: 2 separate mutex-wrapped framebuffers (no contention!)
    // Display refresh locks one, render locks the other - different mutexes = no blocking
    let fb0 = mk_static!(
        metrotimes3::display::DisplayFrameBuffer,
        metrotimes3::display::DisplayFrameBuffer::new()
    );
    let fb1 = mk_static!(
        metrotimes3::display::DisplayFrameBuffer,
        metrotimes3::display::DisplayFrameBuffer::new()
    );

    // Initialize both buffers
    use embedded_graphics::prelude::*;
    for y in 0..metrotimes3::display::DISPLAY_ROWS as i32 {
        for x in 0..metrotimes3::display::DISPLAY_COLS as i32 {
            fb0.set_pixel(Point::new(x, y), esp_hub75::Color::BLACK);
            fb1.set_pixel(Point::new(x, y), esp_hub75::Color::BLACK);
        }
    }
    metrotimes3::display::draw_loading(fb0);
    metrotimes3::display::draw_loading(fb1);

    // Wrap each in its own mutex
    let fb0_mutex: &'static Mutex<
        CriticalSectionRawMutex,
        metrotimes3::display::DisplayFrameBuffer,
    > = mk_static!(Mutex<CriticalSectionRawMutex, metrotimes3::display::DisplayFrameBuffer>, Mutex::new(*fb0));
    let fb1_mutex: &'static Mutex<
        CriticalSectionRawMutex,
        metrotimes3::display::DisplayFrameBuffer,
    > = mk_static!(Mutex<CriticalSectionRawMutex, metrotimes3::display::DisplayFrameBuffer>, Mutex::new(*fb1));

    // Atomic flag: which buffer is currently "active" for display (true = fb0, false = fb1)
    use core::sync::atomic::AtomicBool;
    let active_is_zero: &'static AtomicBool = mk_static!(AtomicBool, AtomicBool::new(true));

    // Lock-free channel for passing departure data from Core 0 → Core 1
    // No mutex contention = no stutter!
    use embassy_sync::channel::Channel;
    let departure_channel: &'static Channel<CriticalSectionRawMutex, DepartureData, 1> = mk_static!(
        Channel<CriticalSectionRawMutex, DepartureData, 1>,
        Channel::new()
    );

    // Start second core for display operations (dedicated core)
    // Architecture:
    //   Core 0: Network/API (fetch, parse, scroll calculation) - can have variable latency
    //   Core 1: Display only (refresh + render) - must be consistently fast for smooth visuals
    // This isolation prevents HTTP parsing stutters from affecting display rendering
    info!("Starting display tasks on Core 1 (dedicated)...");

    let cpu1_fn = {
        let display = display;
        let departure_channel_core1 = departure_channel;
        move || {
            let hp_executor = mk_static!(
                InterruptExecutor<2>,
                InterruptExecutor::new(sw_ints.software_interrupt2)
            );
            let high_pri_spawner = hp_executor.start(Priority::Priority3);

            // Display refresh runs as high priority on dedicated core
            high_pri_spawner
                .spawn(display_refresh_task(
                    display,
                    fb0_mutex,
                    fb1_mutex,
                    active_is_zero,
                ))
                .ok();

            // Low priority executor for render task (still on Core 1, isolated from network/parsing)
            let lp_executor = mk_static!(Executor, Executor::new());

            // Render task runs on Core 1, handles both rendering and scrolling
            // This avoids cross-core data contention
            lp_executor.run(move |spawner| {
                spawner
                    .spawn(render_and_scroll_task(
                        fb0_mutex,
                        fb1_mutex,
                        active_is_zero,
                        departure_channel_core1,
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
        .spawn(data_fetch_task(stack, tls_seed, departure_channel))
        .expect("Failed to spawn data fetch task");

    // Scroll logic moved to render_task on Core 1 (avoids cross-core data sharing)

    // Main task just waits
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

/// Display refresh task - continuously refreshes the HUB75 display
/// With double buffering: locks active buffer only (separate mutex from render)
#[embassy_executor::task]
async fn display_refresh_task(
    mut hub75: Hub75<'static, esp_hal::Blocking>,
    fb0: &'static Mutex<CriticalSectionRawMutex, metrotimes3::display::DisplayFrameBuffer>,
    fb1: &'static Mutex<CriticalSectionRawMutex, metrotimes3::display::DisplayFrameBuffer>,
    active_is_zero: &'static core::sync::atomic::AtomicBool,
) -> ! {
    use core::sync::atomic::Ordering;
    info!("Display refresh task started (double buffered, safe Rust!)");
    let mut counter = 0u32;
    loop {
        // Select active buffer to read from
        let active_fb = if active_is_zero.load(Ordering::Acquire) {
            fb0
        } else {
            fb1
        };

        // Lock active buffer (render task locks the OTHER one - no contention!)
        let fb_locked = active_fb.lock().await;
        let xfer = hub75
            .render(&*fb_locked)
            .map_err(|(e, _)| e)
            .expect("failed to render");

        // Wait for DMA to complete
        let (_result, new_hub75) = xfer.wait();
        hub75 = new_hub75;
        drop(fb_locked);

        counter += 1;
        if counter % DISPLAY_LOG_INTERVAL == 0 {
            debug!("Display: {} frames rendered", counter);
        }

        // Run at ~200fps to maintain brightness
        Timer::after(Duration::from_millis(DISPLAY_REFRESH_INTERVAL_MS)).await;
    }
}

/// Data fetching task - periodically fetches metro data
#[embassy_executor::task]
async fn data_fetch_task(
    stack: &'static embassy_net::Stack<'static>,
    tls_seed: u64,
    departure_channel: &'static embassy_sync::channel::Channel<
        CriticalSectionRawMutex,
        DepartureData,
        1,
    >,
) -> ! {
    info!("Data fetch task started (using lock-free channel)");

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

                // Send to channel (no mutex lock needed - lock-free!)
                departure_channel.send(departures).await;

                info!("Sent departure data to channel");
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

/// Combined render and scroll task - runs entirely on Core 1
/// Receives data from Core 0 via lock-free channel (no mutex contention!)
/// With double buffering: locks inactive buffer (separate mutex from display)
#[embassy_executor::task]
async fn render_and_scroll_task(
    fb0: &'static Mutex<CriticalSectionRawMutex, metrotimes3::display::DisplayFrameBuffer>,
    fb1: &'static Mutex<CriticalSectionRawMutex, metrotimes3::display::DisplayFrameBuffer>,
    active_is_zero: &'static core::sync::atomic::AtomicBool,
    departure_channel: &'static embassy_sync::channel::Channel<
        CriticalSectionRawMutex,
        DepartureData,
        1,
    >,
) -> ! {
    use core::sync::atomic::Ordering;

    info!("Render+Scroll task started on Core 1 (double buffered, lock-free channel!)");
    let mut frame_count = 0u32;
    let mut scroll_offset = 0i32;
    let mut cached_departures: DepartureData = Vec::new();
    let mut total_scroll_width = 0i32;
    let mut last_scroll_tick = embassy_time::Instant::now();

    loop {
        // Try to receive new data from channel (non-blocking)
        if let Ok(new_departures) = departure_channel.try_receive() {
            info!("Received {} departures from channel", new_departures.len());

            // Check if stations/lines changed (ignore time changes)
            let stations_changed = new_departures.len() != cached_departures.len()
                || new_departures
                    .iter()
                    .zip(cached_departures.iter())
                    .any(|(a, b)| a.0 != b.0 || a.1 != b.1);

            if stations_changed {
                // Reset scroll when stations change
                scroll_offset = 0;
                debug!("Stations changed, reset scroll");

                // Recalculate scroll width
                if new_departures.len() <= 1 {
                    total_scroll_width = 0;
                } else {
                    total_scroll_width = 0;
                    for i in 1..new_departures.len() {
                        let (line, dest, time) = &new_departures[i];
                        let entry_len = line.len() + 1 + dest.len() + 1 + time.len();
                        total_scroll_width += entry_len as i32 * 6;
                        if i > 1 {
                            total_scroll_width += 12;
                        }
                    }
                }
            }

            cached_departures = new_departures;
        }

        // Update scroll position (only if needed)
        if total_scroll_width > 0 {
            let now = embassy_time::Instant::now();
            if now.duration_since(last_scroll_tick).as_millis() >= display::SCROLL_SPEED_MS {
                last_scroll_tick = now;
                let reset_point = total_scroll_width + metrotimes3::display::DISPLAY_COLS as i32;
                scroll_offset = if scroll_offset >= reset_point {
                    0
                } else {
                    scroll_offset + 1
                };
            }
        }

        // Redraw if needed (when scroll changes or initial draw)
        if !cached_departures.is_empty() {
            // Get the INACTIVE buffer (not currently being displayed)
            let inactive_fb = if active_is_zero.load(Ordering::Acquire) {
                fb1 // Display reads fb0, so we write to fb1
            } else {
                fb0 // Display reads fb1, so we write to fb0
            };

            // Lock inactive buffer - display task locks the OTHER one (no contention!)
            let mut fb_locked = inactive_fb.lock().await;

            // Clear and draw to inactive buffer
            use embedded_graphics::prelude::*;
            use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
            let clear_rect = Rectangle::new(
                Point::new(0, 0),
                Size::new(metrotimes3::display::DISPLAY_COLS as u32, 32),
            );
            let _ = clear_rect
                .into_styled(PrimitiveStyle::with_fill(esp_hub75::Color::BLACK))
                .draw(&mut *fb_locked);

            // Draw appropriate content
            metrotimes3::display::draw_departures(
                &mut *fb_locked,
                &cached_departures,
                scroll_offset,
            );
            drop(fb_locked);

            // Swap buffers atomically - instant operation!
            active_is_zero.fetch_xor(true, Ordering::Release);

            frame_count += 1;
            if frame_count % RENDER_LOG_INTERVAL == 0 {
                debug!("Render: {} frames", frame_count);
            }
        }

        // Poll at 100Hz (10ms) for smooth updates
        Timer::after(Duration::from_millis(10)).await;
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
