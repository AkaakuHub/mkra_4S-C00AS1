CROSS_COMPILE ?= arm-linux-gnueabihf-
BUILD_DIR ?= build
CC := $(CROSS_COMPILE)gcc
LD := $(CROSS_COMPILE)ld
CFLAGS := -Os -Wall -Wextra -Werror -std=gnu11 -ffreestanding -fno-builtin \
    -fno-pic -fno-pie -fno-common -fno-stack-protector -fno-unwind-tables \
    -fno-asynchronous-unwind-tables -marm -march=armv7-a -mfloat-abi=soft

.PHONY: all clean
all: $(BUILD_DIR)/as1_freezer.ko $(BUILD_DIR)/as1_diag.ko $(BUILD_DIR)/as1_wake.ko

$(BUILD_DIR):
	mkdir -p $@

$(BUILD_DIR)/freezer.o: freezer.c target_abi.h | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD_DIR)/module.o: module.S | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD_DIR)/as1_freezer.ko: $(BUILD_DIR)/freezer.o $(BUILD_DIR)/module.o
	$(LD) -r $^ -o $@

$(BUILD_DIR)/diag.o: diag.c network_abi.h target_abi.h | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD_DIR)/diag_module.o: module.S | $(BUILD_DIR)
	$(CC) $(CFLAGS) -DAS1_MODULE_NAME='"as1_diag"' -c $< -o $@

$(BUILD_DIR)/as1_diag.ko: $(BUILD_DIR)/diag.o $(BUILD_DIR)/diag_module.o
	$(LD) -r $^ -o $@

$(BUILD_DIR)/wake.o: wake.c target_abi.h | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD_DIR)/wake_module.o: module.S | $(BUILD_DIR)
	$(CC) $(CFLAGS) -DAS1_MODULE_NAME='"as1_wake"' -c $< -o $@

$(BUILD_DIR)/as1_wake.ko: $(BUILD_DIR)/wake.o $(BUILD_DIR)/wake_module.o
	$(LD) -r $^ -o $@

clean:
	rm -f $(BUILD_DIR)/freezer.o $(BUILD_DIR)/module.o $(BUILD_DIR)/as1_freezer.ko \
	    $(BUILD_DIR)/diag.o $(BUILD_DIR)/diag_module.o $(BUILD_DIR)/as1_diag.ko \
	    $(BUILD_DIR)/wake.o $(BUILD_DIR)/wake_module.o $(BUILD_DIR)/as1_wake.ko
