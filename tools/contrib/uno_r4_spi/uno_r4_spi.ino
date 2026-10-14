#include <Arduino.h>

constexpr uint8_t AS1_ROM_CS_PIN = 8;
constexpr uint8_t AS1_ROM_MOSI_PIN = 9;
constexpr uint8_t AS1_ROM_CLK_PIN = 7;
constexpr uint8_t AS1_ROM_MISO_ADC_PIN = A0;
constexpr uint16_t ADC_HIGH_THRESHOLD = 410; // 10-bit ADC: ~2.0V at 5V reference
constexpr uint32_t FLASH_SIZE = 2UL * 1024UL * 1024UL;
constexpr uint32_t ENV_START = 0x1FF000UL;

static uint8_t chunk[256];

// Pre-set each output latch to LOW in setup(). Never drive HIGH.
static inline void pullLow(uint8_t pin) {
  pinMode(pin, OUTPUT);
}
static inline void releaseTo33(uint8_t pin) {
  pinMode(pin, INPUT); // External resistor provides 3.3V; never INPUT_PULLUP!
}

static uint8_t bitbangTransfer(uint8_t transmit) {
  uint8_t received = 0;
  for (int bit = 7; bit >= 0; --bit) {
    if (transmit & (1U << bit)) releaseTo33(AS1_ROM_MOSI_PIN);
    else pullLow(AS1_ROM_MOSI_PIN);
    delayMicroseconds(4);

    releaseTo33(AS1_ROM_CLK_PIN); // Rising edge: ROM samples MOSI
    delayMicroseconds(8);

    received = (uint8_t)((received << 1) |
      (analogRead(AS1_ROM_MISO_ADC_PIN) >= ADC_HIGH_THRESHOLD ? 1 : 0));

    pullLow(AS1_ROM_CLK_PIN); // Falling edge
    delayMicroseconds(3);
  }
  return received;
}

static void selectFlash() {
  pullLow(AS1_ROM_CLK_PIN);
  releaseTo33(AS1_ROM_MOSI_PIN);
  delayMicroseconds(3);
  pullLow(AS1_ROM_CS_PIN);
  delayMicroseconds(3);
}
static void deselectFlash() {
  releaseTo33(AS1_ROM_CS_PIN);
  releaseTo33(AS1_ROM_MOSI_PIN);
  pullLow(AS1_ROM_CLK_PIN);
  delayMicroseconds(3);
}

static void readId() {
  uint8_t id[3];
  selectFlash();
  bitbangTransfer(0x9F);
  for (int i = 0; i < 3; ++i) id[i] = bitbangTransfer(0x00);
  deselectFlash();
  Serial.write(id, sizeof(id));
}

static uint8_t readRegister(uint8_t command) {
  selectFlash();
  bitbangTransfer(command);
  uint8_t value = bitbangTransfer(0);
  deselectFlash();
  return value;
}

static bool waitReady(uint32_t timeoutMs) {
  uint32_t started = millis();
  while (readRegister(0x05) & 1U) {
    if (millis() - started >= timeoutMs) return false;
    delay(2);
  }
  return true;
}

static bool enableWrite() {
  if (!waitReady(1000)) return false;
  selectFlash();
  bitbangTransfer(0x9F);
  uint8_t manufacturer = bitbangTransfer(0);
  uint8_t type = bitbangTransfer(0);
  uint8_t capacity = bitbangTransfer(0);
  deselectFlash();
  if (manufacturer != 0xEF || type != 0x70 || capacity != 0x15) return false;
  selectFlash();
  bitbangTransfer(0x06);
  deselectFlash();
  return (readRegister(0x05) & 2U) != 0;
}

static void sendAddress(uint32_t address) {
  bitbangTransfer((uint8_t)(address >> 16));
  bitbangTransfer((uint8_t)(address >> 8));
  bitbangTransfer((uint8_t)address);
}

static void eraseEnvironment(uint32_t address) {
  bool ok = address == ENV_START && enableWrite();
  if (ok) {
    selectFlash();
    bitbangTransfer(0x20);
    sendAddress(address);
    deselectFlash();
    ok = waitReady(15000);
  }
  Serial.write((uint8_t)(ok ? 'K' : 'X'));
}

static void programEnvironment(uint32_t address) {
  if (Serial.readBytes(chunk, sizeof(chunk)) != sizeof(chunk)) {
    Serial.write((uint8_t)'X');
    return;
  }
  bool ok = address >= ENV_START && address <= FLASH_SIZE - sizeof(chunk) &&
            (address & 0xFFUL) == 0 && enableWrite();
  if (ok) {
    selectFlash();
    bitbangTransfer(0x02);
    sendAddress(address);
    for (unsigned i = 0; i < sizeof(chunk); ++i) bitbangTransfer(chunk[i]);
    deselectFlash();
    ok = waitReady(4000);
  }
  Serial.write((uint8_t)(ok ? 'K' : 'X'));
}

static void read256(uint32_t addr) {
  if (addr > FLASH_SIZE - sizeof(chunk)) return; // PC validates addresses.
  selectFlash();
  bitbangTransfer(0x03); // standard read (no dummy cycles)
  sendAddress(addr);
  for (unsigned i = 0; i < sizeof(chunk); ++i) {
    chunk[i] = bitbangTransfer(0x00);
  }
  deselectFlash();
  Serial.write(chunk, sizeof(chunk));
}

void setup() {
  // Keep output register latched LOW before ever enabling output mode.
  pinMode(AS1_ROM_CS_PIN, INPUT);
  pinMode(AS1_ROM_MOSI_PIN, INPUT);
  pinMode(AS1_ROM_CLK_PIN, INPUT);
  digitalWrite(AS1_ROM_CS_PIN, LOW);
  digitalWrite(AS1_ROM_MOSI_PIN, LOW);
  digitalWrite(AS1_ROM_CLK_PIN, LOW);
  releaseTo33(AS1_ROM_CS_PIN);
  releaseTo33(AS1_ROM_MOSI_PIN);
  pullLow(AS1_ROM_CLK_PIN);
  pinMode(AS1_ROM_MISO_ADC_PIN, INPUT);
  analogReadResolution(10);
  Serial.begin(115200);
  Serial.setTimeout(3000);
}

void loop() {
  if (!Serial.available()) return;
  int cmd = Serial.read();
  if (cmd == 'I') {
    readId();
  } else if (cmd == 'S') {
    uint8_t status[] = {readRegister(0x05), readRegister(0x35), readRegister(0x15)};
    Serial.write(status, sizeof(status));
  } else if (cmd == 'R' || cmd == 'E' || cmd == 'P') {
    uint8_t address[3];
    if (Serial.readBytes(address, 3) != 3) return;
    uint32_t offset = (uint32_t)address[0] |
                      ((uint32_t)address[1] << 8) |
                      ((uint32_t)address[2] << 16);
    if (cmd == 'R') read256(offset);
    else if (cmd == 'E') eraseEnvironment(offset);
    else programEnvironment(offset);
  }
}
