/* SPDX-License-Identifier: GPL-2.0-only */
#include "target_abi.h"

static volatile u32 *callback;
static u32 saved[2];
static void *gmac;
static u32 (*read_register)(void *, u32);
static void (*powerdown)(void *);
static void (*powerup)(void *);
static void (*disable_powerdown)(void *);
static int enabled;
static unsigned int suspends;
static unsigned int resumes;

static void mac_pm(unsigned int adapter, unsigned int resume)
{
    u32 base = *(u32 *)gmac;
    u32 pmt;

    (void)adapter;
    pmt = read_register(gmac, base + 0x2c);
    if (!resume) {
        suspends++;
        if (!(pmt & 1U))
            powerdown(gmac);
    } else {
        resumes++;
        if (pmt & 1U) {
            disable_powerdown(gmac);
            powerup(gmac);
        }
    }
}

static void sync_code(void *unused)
{
    (void)unused;
    v7_coherent_kern_range((unsigned long)callback,
                          (unsigned long)(callback + 2));
}

static int change_callback(void *value)
{
    int next = *(int *)value;
    u32 replacement[2] = {0xe51ff004U, (u32)mac_pm};
    unsigned int i;

    for (i = 0; i < 2; i++)
        if (callback[i] != (enabled ? replacement[i] : saved[i]))
            return -22;
    for (i = 0; i < 2; i++)
        callback[i] = next ? replacement[i] : saved[i];
    sync_code(0);
    return 0;
}

static int set_enabled(const char *value, const struct kernel_param *parameter)
{
    int next;
    int error;

    (void)parameter;
    error = kstrtoint(value, 0, &next);
    if (error)
        return error;
    if (!callback || (next != 0 && next != 1))
        return -22;
    if (next == enabled)
        return 0;
    error = stop_machine(change_callback, &next, 0);
    if (!error) {
        on_each_cpu(sync_code, 0, 1);
        enabled = next;
    }
    return error;
}

static int get_stats(char *buffer, const struct kernel_param *parameter)
{
    (void)parameter;
    return scnprintf(buffer, 4096, "enabled=%d suspends=%u resumes=%u\n",
                     enabled, suspends, resumes);
}

AS1_PARAM(enabled, 0200, set_enabled, 0);
AS1_PARAM(stats, 0400, 0, get_stats);

int init_module(void)
{
    void *descriptor = irq_to_desc(83);
    u32 *action;
    void *device;
    u32 *native_devices;

    callback = (void *)kallsyms_lookup_name("synopGMAC_pm");
    read_register = (void *)kallsyms_lookup_name("plat_ReadReg");
    powerdown = (void *)kallsyms_lookup_name("synopGMAC_linux_powerdown_mac");
    powerup = (void *)kallsyms_lookup_name("synopGMAC_linux_powerup_mac");
    disable_powerdown = (void *)kallsyms_lookup_name("synopGMAC_power_down_disable");
    if (!descriptor || !callback || !read_register || !powerdown || !powerup ||
        !disable_powerdown || callback[0] != 0xe92d4038U ||
        callback[1] != 0xe3510000U || callback[38] != 0x80f09788U)
        return -22;
    action = *(u32 **)((char *)descriptor + 0x30);
    if (!action || action[6] != 83)
        return -22;
    device = (void *)action[1];
    native_devices = (u32 *)callback[38];
    if (!device || native_devices[0] != (u32)device || native_devices[1])
        return -22;
    gmac = (char *)device + 0x610;
    saved[0] = callback[0];
    saved[1] = callback[1];
    printk("\0016as1_lan: verified MAC-only PM comparison ready\n");
    return 0;
}
