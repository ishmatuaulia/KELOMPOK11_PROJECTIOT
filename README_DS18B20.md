# 🛰 IoT DS18B20 (Rust + ESP-IDF, ESP32-S3)

Proyek ini terdiri dari dua modul utama yang ditulis dalam *Rust* dan dijalankan di *ESP32-S3*:

1. **IOT_DS18B20STREAM** — membaca sensor suhu *DS18B20* dan mengirim data ke *ThingsBoard* melalui *MQTT*.  
2. **IOT_DS18B20OTA** — versi dengan dukungan *Over-The-Air (OTA)* update menggunakan partisi *dual-app*.

---

## 📂 Struktur Folder

```
IOT_DS18B20OTA/
├─ .cargo/
│  └─ config.toml
├─ src/
│  └─ main.rs
├─ build.rs
├─ Cargo.toml
├─ partition_table.csv
├─ rust-toolchain.toml
└─ sdkconfig.defaults

IOT_DS18B20STREAM/
├─ .cargo/
│  └─ config.toml
├─ src/
│  └─ main.rs
├─ build.rs
├─ Cargo.toml
├─ OTA2.bin
├─ rust-toolchain.toml
└─ sdkconfig.defaults
```

Kedua proyek dapat dibangun dan dijalankan secara terpisah.

---

## ⚙ Persiapan & Instalasi

### 1️⃣ Instal Rust
```bash
sudo apt update
sudo apt install curl -y
curl https://sh.rustup.rs -sSf | sh
source $HOME/.cargo/env
rustc --version
cargo --version
```

### 2️⃣ Clone Repositori
```bash
git clone https://github.com/WildanAuzay/IoT-DS18B20.git
cd IoT-DS18B20
```

### 3️⃣ Siapkan ESP-IDF
```bash
sudo apt install -y git wget flex bison gperf python3 python3-pip cmake ninja-build ccache libffi-dev libssl-dev dfu-util libusb-1.0-0
git clone -b v5.1.1 --recursive https://github.com/espressif/esp-idf.git
cd esp-idf
./install.sh
. ./export.sh
```

### 4️⃣ Instal Toolchain & Flasher
```bash
cargo install espup --locked
espup install
cargo install espflash --locked
```

---

## 🧠 Konfigurasi Proyek

Buka `src/main.rs` di masing-masing modul, lalu ubah kredensial dan konfigurasi berikut:
```rust
const WIFI_SSID: &str = "SSID_WIFI";
const WIFI_PASS: &str = "PASSWORD_WIFI";
const TB_HOST: &str  = "thingsboard.cloud";
const TB_PORT: u16   = 1883;
const TB_TOKEN: &str = "ACCESS_TOKEN";
```

**TOPIK MQTT:**  
```
v1/devices/me/telemetry
```

**Contoh payload:**  
```json
{"temperature": 27.8}
```

---

## ⚡ Wiring Sensor DS18B20 ke ESP32-S3

| DS18B20 Pin | ESP32-S3 Pin | Keterangan |
|--------------|--------------|-------------|
| VCC          | 3.3V         | Tegangan suplai |
| GND          | GND          | Ground |
| DATA         | GPIO5        | Jalur data ke ESP32 |
| —            | —            | Tambahkan resistor **4.7kΩ** antara DATA dan VCC (pull-up) |

💡 Tips:
- Gunakan kabel pendek agar pembacaan stabil.  
- Pastikan daya 3.3V tidak drop.

---

## 🧱 Diagram Sistem

```bash
flowchart LR
  DS18B20 --> ESP32S3 --> WiFi --> MQTT --> TB[(ThingsBoard Cloud)] --> User
```

---

## 🔹 1) Menjalankan Modul STREAM

### Build Firmware
```bash
cd IOT_DS18B20STREAM
cargo build --release --target xtensa-esp32s3-espidf
```

### Flash ke ESP32-S3
```bash
espflash flash /dev/ttyUSB0 target/xtensa-esp32s3-espidf/release/iot_ds18b20stream
```

### Monitoring Serial
```bash
espflash monitor /dev/ttyUSB0 115200
```

**Verifikasi:**
- Log menampilkan “Connected to Wi-Fi” dan “Data sent to ThingsBoard”.  
- Di ThingsBoard → *Latest Telemetry* → muncul nilai `temperature`.

---

## 🔹 2) Menjalankan Modul OTA

### Build Firmware OTA
```bash
cd ../IOT_DS18B20OTA
cargo build --release --target xtensa-esp32s3-espidf
```

### Flash Firmware Awal (Partisi A)
```bash
espflash flash --partition-table partition_table.csv /dev/ttyUSB0 target/xtensa-esp32s3-espidf/release/iot_ds18b20ota
```

Jika muncul error `unexpected argument '--partition-table'`, cek urutan argumen dengan:
```bash
espflash flash --help
```

### Update Firmware OTA
1. Buat perubahan kecil di kode (misalnya ubah versi firmware).  
2. Build ulang → hasilkan file baru `OTA2.bin`.  
3. Unggah ke ThingsBoard atau server OTA sesuai implementasi.  
4. Device otomatis mendownload → flash → reboot → tampil versi baru.

---

## 📊 Hasil & Analisis

- Sensor DS18B20 membaca suhu dengan akurasi ±0.5°C.  
- Data dikirim setiap ±10 detik ke ThingsBoard via MQTT.  
- Dashboard ThingsBoard menampilkan grafik perubahan suhu.  
- OTA sukses jika perangkat reboot otomatis ke firmware baru dan telemetry tetap aktif.

---

## 🛠 Troubleshooting

| Masalah | Solusi |
|----------|---------|
| ❌ `unwrap() on Err: environment variable not found` | Jalankan `. ./export.sh` sebelum `cargo build` |
| ⚠️ Telemetri tidak muncul di ThingsBoard | Periksa `TB_TOKEN`, `TB_HOST`, dan koneksi Wi-Fi |
| 🌡 Nilai suhu tidak terbaca / acak | Pastikan resistor pull-up 4.7kΩ terpasang dan kabel DATA tidak terlalu panjang |
| 🔄 OTA gagal boot | Pastikan file `partition_table.csv` sesuai layout dan firmware valid |
| ⚙️ Target xtensa tidak ditemukan | Jalankan `cargo install espup` lalu `espup install` untuk menambahkan toolchain |

---

## ✅ Checklist Uji Coba

- [ ] ESP-IDF sudah diinstal dan `. ./export.sh` dijalankan  
- [ ] Target `xtensa-esp32s3-espidf` tersedia  
- [ ] `espflash` sudah terinstal  
- [ ] Kredensial Wi-Fi dan Token ThingsBoard terisi  
- [ ] Wiring DS18B20 benar (dengan pull-up 4.7kΩ)  
- [ ] Telemetri muncul di dashboard ThingsBoard  
- [ ] OTA berhasil update firmware dan reboot otomatis  

---

## 📘 Contoh Cepat

```bash
# Setup environment
cd ~/esp-idf
. ./export.sh

# Build & flash modul STREAM
cd ~/IoT-DS18B20/IOT_DS18B20STREAM
cargo build --release --target xtensa-esp32s3-espidf
espflash flash /dev/ttyUSB0 target/xtensa-esp32s3-espidf/release/iot_ds18b20stream
espflash monitor /dev/ttyUSB0 115200

# Build & flash modul OTA
cd ../IOT_DS18B20OTA
cargo build --release --target xtensa-esp32s3-espidf
espflash flash --partition-table partition_table.csv /dev/ttyUSB0 target/xtensa-esp32s3-espidf/release/iot_ds18b20ota
espflash monitor /dev/ttyUSB0 115200
```

---

## 📎 Catatan Akhir

- DS18B20 menggunakan protokol **1-Wire**, pastikan timing komunikasi dan resistor pull-up benar.  
- OTA update memerlukan partisi ganda (`factory`, `ota_0`, `ota_1`) di `partition_table.csv`.  
- Semua data dikirim dalam format JSON ke topik MQTT:  
  ```
  v1/devices/me/telemetry
  ```
- Gunakan Access Token dari ThingsBoard sebagai username MQTT.
