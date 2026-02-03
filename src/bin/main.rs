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
use log::{error, info};
use static_cell::StaticCell;

// Include generated config
include!(concat!(env!("OUT_DIR"), "/config.rs"));

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

    info!("Drawing black background and green text...");
    for y in 0..32 {
        for x in 0..64 {
            fb.set_pixel(Point::new(x, y), esp_hub75::Color::BLACK);
        }
    }

    // Now draw "no departures" in GREEN using custom bitmap font
    metrotimes3::display::draw_departures(fb, &[]);

    info!("Drew 'no departures' in green");

    // Wrap framebuffer in mutex for safe sharing
    let fb_mutex: &'static Mutex<
        CriticalSectionRawMutex,
        metrotimes3::display::DisplayFrameBuffer,
    > = mk_static!(
        Mutex<CriticalSectionRawMutex, metrotimes3::display::DisplayFrameBuffer>,
        Mutex::new(*fb)
    );

    // Start second core for display refresh (high priority, dedicated core)
    info!("Starting display refresh on Core 1 (dedicated)...");

    let cpu1_fn = {
        let display = display;
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

            // Low priority executor for any other core 1 tasks
            let lp_executor = mk_static!(Executor, Executor::new());
            lp_executor.run(|_spawner| {});
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

    info!("Display running on Core 1 - showing initial placeholder");

    // Make stack static for data fetch task
    let stack = mk_static!(embassy_net::Stack<'static>, stack);

    // Create shared departure data storage
    let departure_data: &'static Mutex<CriticalSectionRawMutex, DepartureData> = mk_static!(
        Mutex<CriticalSectionRawMutex, DepartureData>,
        Mutex::new(Vec::new())
    );

    // Start data fetch task on Core 0 (updates departure data every 30s)
    info!("Starting data fetch task on Core 0...");
    spawner
        .spawn(data_fetch_task(stack, tls_seed, fb_mutex, departure_data))
        .expect("Failed to spawn data fetch task");

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
        if counter % 5000 == 0 {
            info!("Display: {} frames rendered", counter);
        }
    }
}

/// Data fetching task - periodically fetches metro data and updates display
#[embassy_executor::task]
async fn data_fetch_task(
    stack: &'static embassy_net::Stack<'static>,
    tls_seed: u64,
    fb: &'static Mutex<CriticalSectionRawMutex, metrotimes3::display::DisplayFrameBuffer>,
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

                // Store departure data
                let mut data = departure_data.lock().await;
                *data = departures.clone();
                drop(data);

                // Update framebuffer
                let mut fb_locked = fb.lock().await;
                fb_locked.erase();
                metrotimes3::display::draw_departures(&mut *fb_locked, &departures);
                drop(fb_locked);

                info!("Updated display with new data");
            }
            Err(e) => {
                error!("Failed to fetch data: {:?}", e);
            }
        }

        info!("Waiting 30 seconds until next fetch...");
        Timer::after(Duration::from_secs(30)).await;
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
