use anyhow::Result;
use esp_idf_hal::delay::Delay;
use esp_idf_hal::gpio::*;
use esp_idf_hal::prelude::*;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use log::*;
use onewire::{Device, DeviceSearch, OneWire};
use serde_json::json;
use alloc::string::ToString;
use alloc::ffi::CString;

extern crate alloc;

// Structure to represent temperature in Celsius
#[derive(Debug)]
struct Celsius(f32);

impl Celsius {
    pub fn as_f32(&self) -> f32 {
        self.0
    }
}

// Convert milliseconds to FreeRTOS ticks
#[inline(always)]
fn ms_to_ticks(ms: u32) -> u32 {
    (ms as u64 * esp_idf_sys::configTICK_RATE_HZ as u64 / 1000) as u32
}

// MQTT client wrapper
struct SimpleMqttClient {
    client: *mut esp_idf_sys::esp_mqtt_client,
}

impl SimpleMqttClient {
    fn new(broker_url: &str, username: &str, password: &str, client_id: &str) -> Result<Self> {
        unsafe {
            let broker_url_cstr = CString::new(broker_url)?;
            let username_cstr = CString::new(username)?;
            let password_cstr = CString::new(password)?;
            let client_id_cstr = CString::new(client_id)?;

            let config = esp_idf_sys::esp_mqtt_client_config_t {
                broker: esp_idf_sys::esp_mqtt_client_config_t_broker_t {
                    address: esp_idf_sys::esp_mqtt_client_config_t_broker_t_address_t {
                        uri: broker_url_cstr.as_ptr() as *const u8,
                        ..core::mem::zeroed()
                    },
                    ..core::mem::zeroed()
                },
                credentials: esp_idf_sys::esp_mqtt_client_config_t_credentials_t {
                    username: username_cstr.as_ptr() as *const u8,
                    client_id: client_id_cstr.as_ptr() as *const u8,
                    authentication: esp_idf_sys::esp_mqtt_client_config_t_credentials_t_authentication_t {
                        password: password_cstr.as_ptr() as *const u8,
                        ..core::mem::zeroed()
                    },
                    ..core::mem::zeroed()
                },
                ..core::mem::zeroed()
            };

            let client = esp_idf_sys::esp_mqtt_client_init(&config);
            if client.is_null() {
                return Err(anyhow::anyhow!("Failed to initialize MQTT client"));
            }

            let err = esp_idf_sys::esp_mqtt_client_start(client);
            if err != esp_idf_sys::ESP_OK {
                return Err(anyhow::anyhow!("Failed to start MQTT client, error code: {}", err));
            }

            esp_idf_sys::vTaskDelay(ms_to_ticks(5000));
            Ok(Self { client })
        }
    }

    fn publish(&self, topic: &str, data: &str) -> Result<()> {
        unsafe {
            let topic_cstr = CString::new(topic)?;
            let data_cstr = CString::new(data)?;

            let msg_id = esp_idf_sys::esp_mqtt_client_publish(
                self.client,
                topic_cstr.as_ptr(),
                data_cstr.as_ptr(),
                data.len() as i32,
                1,
                0,
            );

            if msg_id < 0 {
                Err(anyhow::anyhow!(
                    "Failed to publish message, error code: {}",
                    msg_id
                ))
            } else {
                info!("Message published with ID: {}", msg_id);
                Ok(())
            }
        }
    }
}

impl Drop for SimpleMqttClient {
    fn drop(&mut self) {
        unsafe {
            esp_idf_sys::esp_mqtt_client_stop(self.client);
            esp_idf_sys::esp_mqtt_client_destroy(self.client);
        }
    }
}

// ====================================================
// 🧩 OTA Update Functions
// ====================================================
fn send_ota_status(mqtt_client: &SimpleMqttClient, title: &str, version: &str, state: &str) -> Result<()> {
    let payload = json!({
        "fw_title": title,
        "fw_version": version,
        "fw_state": state
    })
    .to_string();

    mqtt_client.publish("v1/devices/me/telemetry", &payload)?;
    info!("📡 OTA Status sent -> {}", payload);
    Ok(())
}

// Function to send temperature data to ThingsBoard
fn send_telemetry(mqtt_client: &SimpleMqttClient, temperature: f32) -> Result<()> {
    let payload = json!({
        "temperature": temperature
    })
    .to_string();

    mqtt_client.publish("v1/devices/me/telemetry", &payload)?;
    info!("Data sent to ThingsBoard: {}", payload);
    Ok(())
}

// ====================================================
// 🕒 Function to send telemetry with timestamp and client_ts
// ====================================================
fn send_telemetry_with_timestamp(mqtt_client: &SimpleMqttClient, temperature: f32) -> Result<()> {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Dapatkan waktu sekarang (timestamp dalam milidetik)
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();

    // Buat payload JSON dengan timestamp, temperature, dan client_ts
    let payload = json!({
        "ts": now,             // timestamp epoch (ms)
        "values": {
            "temperature": temperature,
            "client_ts": now   // client timestamp sama dengan waktu lokal
        }
    })
    .to_string();

    // Kirim payload ke ThingsBoard
    mqtt_client.publish("v1/devices/me/telemetry", &payload)?;
    info!("📡 Data sent with timestamp -> {}", payload);
    Ok(())
}

// ====================================================
// 🕒 Tambahan: Kirim format langsung timestamp, temperature, client_ts
// ====================================================
fn send_telemetry_full_format(mqtt_client: &SimpleMqttClient, temperature: f32) -> Result<()> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();

    let payload = json!({
        "timestamp": now,
        "temperature": temperature,
        "client_ts": now
    })
    .to_string();

    mqtt_client.publish("v1/devices/me/telemetry", &payload)?;
    info!("📡 Full-format data sent to ThingsBoard -> {}", payload);
    Ok(())
}

// Function to connect to Wi-Fi
fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> Result<()> {
    let ssid = "~SkyNet~";
    let password = "20201908";
    let wifi_config = Configuration::Client(ClientConfiguration {
        ssid: heapless::String::try_from(ssid).unwrap(),
        password: heapless::String::try_from(password).unwrap(),
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    });
    wifi.set_configuration(&wifi_config)?;
    wifi.start()?;
    wifi.connect()?;
    wifi.wait_netif_up()?;
    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;
    info!("WiFi Connected, IP: {}", ip_info.ip);
    Ok(())
}

// ====================================================
// 🧠 MAIN PROGRAM
// ====================================================
fn main() -> Result<()> {
    esp_idf_sys::link_patches();
    EspLogger::initialize_default();
    info!("🚀 DS18B20 temperature reading program with Wi-Fi and MQTT started...");

    let peripherals = Peripherals::take().unwrap();
    let sys_loop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;
   loop {
    match connect_wifi(&mut wifi) {
        Ok(_) => {
            info!("✅ WiFi connected successfully");
            break;
        }
        Err(e) => {
            error!("❌ WiFi connection failed: {:?}. Retrying in 5s...", e);
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    }
}

    let pin_ds18b20 = peripherals.pins.gpio6;
    let pin = PinDriver::input_output_od(pin_ds18b20)?;
    info!("GPIO6 configured for 1-Wire");

    let mut one_wire = OneWire::new(pin, false);
    let mut delay = Delay::new(100);

    info!("Connecting to MQTT broker...");
    let mqtt_client = match SimpleMqttClient::new(
        "mqtt://mqtt.thingsboard.cloud:1883",
        "riskalaena",
        "riskalaena",
        "ybo47mjx9o7vcziv9i6p",
    ) {
        Ok(client) => {
            info!("Connected to ThingsBoard MQTT broker");
            client
        }
        Err(e) => {
            error!("Failed to connect to MQTT: {:?}", e);
            return Err(e);
        }
    };

    if let Err(e) = send_ota_status(&mqtt_client, "myFirmware", "1.0.0", "UPDATED") {
        error!("Failed to send OTA status: {:?}", e);
    }

    info!("🔍 Attempting direct temperature reading (skip ROM)...");
    if let Ok(_) = one_wire.reset(&mut delay) {
        info!("1-Wire bus reset successful for skip ROM");
        if let Err(e) = one_wire.write_bytes(&mut delay, &[0xCC, 0x44]) {
            error!("❌ Failed to send skip ROM/convert command: {:?}", e);
        } else {
            delay.delay_ms(750);
            if let Ok(_) = one_wire.reset(&mut delay) {
                if let Err(e) = one_wire.write_bytes(&mut delay, &[0xCC, 0xBE]) {
                    error!("❌ Failed to send read scratchpad command: {:?}", e);
                } else {
                    let mut data = [0u8; 9];
                    if let Err(e) = one_wire.read_bytes(&mut delay, &mut data) {
                        error!("❌ Failed to read scratchpad: {:?}", e);
                    } else {
                        info!("Scratchpad data: {:?}", data);
                        let temp_raw = (data[1] as i16) << 8 | data[0] as i16;
                        let temp_celsius = Celsius(temp_raw as f32 / 16.0);
                        info!("🌡️ Temperature: {:.1} °C", temp_celsius.as_f32());
                        if let Err(e) = send_telemetry(&mqtt_client, temp_celsius.as_f32()) {
                            error!("Failed to send telemetry: {:?}", e);
                        }
                        if let Err(e) = send_telemetry_with_timestamp(&mqtt_client, temp_celsius.as_f32()) {
                            error!("Failed to send telemetry with timestamp: {:?}", e);
                        }
                        // 🧩 Tambahan kirim format lengkap timestamp, temperature, client_ts
                        if let Err(e) = send_telemetry_full_format(&mqtt_client, temp_celsius.as_f32()) {
                            error!("Failed to send full-format telemetry: {:?}", e);
                        }
                    }
                }
            } else {
                error!("❌ Failed to reset bus for reading");
            }
        }
    } else {
        error!("❌ Failed to reset 1-Wire bus for skip ROM");
    }

    info!("📊 Starting temperature readings every 3 seconds...");
    loop {
        match one_wire.reset(&mut delay) {
            Ok(_) => {
                let _ = one_wire.write_bytes(&mut delay, &[0xCC, 0x44]);
                delay.delay_ms(750);
                let _ = one_wire.reset(&mut delay);
                let _ = one_wire.write_bytes(&mut delay, &[0xCC, 0xBE]);

                let mut data = [0u8; 9];
                if let Ok(_) = one_wire.read_bytes(&mut delay, &mut data) {
                    let temp_raw = (data[1] as i16) << 8 | data[0] as i16;
                    let temp_celsius = Celsius(temp_raw as f32 / 16.0);
                    info!("🌡️ Temperature: {:.1} °C", temp_celsius.as_f32());
                    if let Err(e) = send_telemetry(&mqtt_client, temp_celsius.as_f32()) {
                        error!("Failed to send telemetry: {:?}", e);
                    }
                    if let Err(e) = send_telemetry_with_timestamp(&mqtt_client, temp_celsius.as_f32()) {
                        error!("Failed to send telemetry with timestamp: {:?}", e);
                    }
                    // 🧩 Tambahan kirim format lengkap timestamp, temperature, client_ts
                    if let Err(e) = send_telemetry_full_format(&mqtt_client, temp_celsius.as_f32()) {
                        error!("Failed to send full-format telemetry: {:?}", e);
                    }
                }
            }
            Err(e) => {
                error!("❌ Error during temperature read: {:?}", e);
            }
        }
        delay.delay_ms(3000);
    }
}
