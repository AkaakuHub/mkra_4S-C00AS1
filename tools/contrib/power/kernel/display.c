/* SPDX-License-Identifier: GPL-2.0-only */
#include "target_abi.h"

struct branch_patch {
    volatile u32 *address;
    u32 original;
    u32 repaired;
};

static struct branch_patch patches[2];
static void *hdmi_semaphore;
static int (*native_down)(void *);
static void (*native_up)(void *);
static int (*native_suspend)(void *);
static int enabled;
static int preparing;
static int held;
static unsigned int retries;
static unsigned int suspends;

static int freezable_lock(void *semaphore)
{
    int result;

    for (;;) {
        result = native_down(semaphore);
        if (!result || semaphore != hdmi_semaphore ||
            !system_freezing_cnt || !freezing_slow_path(as1_current()))
            return result;
        retries++;
        __refrigerator(0);
    }
}

static int suspend_display(void *device)
{
    if (held) {
        held = 0;
        native_up(hdmi_semaphore);
    }
    suspends++;
    return native_suspend(device);
}

static void sync_code(void *unused)
{
    unsigned int i;

    (void)unused;
    for (i = 0; i < 2; i++)
        v7_coherent_kern_range((unsigned long)patches[i].address,
                              (unsigned long)(patches[i].address + 1));
}

static int change_branches(void *value)
{
    int next = *(int *)value;
    unsigned int i;

    for (i = 0; i < 2; i++)
        if (*patches[i].address !=
            (enabled ? patches[i].repaired : patches[i].original))
            return -22;
    for (i = 0; i < 2; i++)
        *patches[i].address = next ? patches[i].repaired : patches[i].original;
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
    if (preparing || held || (next != 0 && next != 1))
        return -22;
    if (next == enabled)
        return 0;
    error = stop_machine(change_branches, &next, 0);
    if (!error) {
        on_each_cpu(sync_code, 0, 1);
        enabled = next;
    }
    return error;
}

static int get_stats(char *buffer, const struct kernel_param *parameter)
{
    (void)parameter;
    return scnprintf(buffer, 4096,
                     "enabled=%d preparing=%d held=%d retries=%u suspends=%u hdmi_sem=%p\n",
                     enabled, preparing, held, retries, suspends, hdmi_semaphore);
}

static int power_event(struct notifier_block *block, unsigned long event,
                       void *unused)
{
    (void)block;
    (void)unused;
    if (event == 3 && enabled) {
        int result;

        preparing = 1;
        result = native_down(hdmi_semaphore);
        if (result) {
            preparing = 0;
            return 0x8002;
        }
        held = 1;
    } else if (event == 4) {
        if (held) {
            held = 0;
            native_up(hdmi_semaphore);
        }
        preparing = 0;
    }
    return 0;
}

static int prepare_branch(struct branch_patch *patch, volatile u32 *address,
                          u32 opcode, unsigned long target,
                          unsigned long replacement)
{
    u32 instruction = *address;
    unsigned long destination = (unsigned long)address + 8 +
                                ((long)(instruction << 8) >> 6);
    long delta = replacement - (unsigned long)address - 8;

    if ((instruction & 0xff000000U) != opcode || destination != target)
        return -22;
    if ((delta & 3) || delta < -33554432L || delta > 33554428L)
        return -34;
    patch->address = address;
    patch->original = instruction;
    patch->repaired = opcode | ((delta >> 2) & 0x00ffffffU);
    return 0;
}

static struct notifier_block notifier = {.call = power_event};

AS1_PARAM(enabled, 0200, set_enabled, 0);
AS1_PARAM(stats, 0400, 0, get_stats);

int init_module(void)
{
    unsigned long callback = kallsyms_lookup_name("xcfb_disp_suspend");
    unsigned long device_suspend = kallsyms_lookup_name("xcfb_disp_dev_suspend");
    unsigned long lock;
    unsigned long call;
    u32 instruction;
    u32 *device_slot;
    int error;

    native_down = (void *)kallsyms_lookup_name("down_interruptible");
    native_up = (void *)kallsyms_lookup_name("up");
    native_suspend = (void *)device_suspend;
    if (!callback || !device_suspend || !native_down || !native_up ||
        *(u32 *)(callback + 0x14) != 0xe59f3040U)
        return -22;
    device_slot = *(u32 **)(callback + 0x5c);
    if (!device_slot || !*device_slot)
        return -22;
    hdmi_semaphore = *(void **)(*device_slot + 0x4a884);
    if (!hdmi_semaphore)
        return -22;
    call = device_suspend + 0x24;
    instruction = *(u32 *)call;
    if ((instruction & 0xff000000U) != 0xeb000000U)
        return -22;
    lock = call + 8 + ((long)(instruction << 8) >> 6);
    error = prepare_branch(&patches[0], (void *)lock, 0xea000000U,
                           (unsigned long)native_down, (unsigned long)freezable_lock);
    if (error)
        return error;
    error = prepare_branch(&patches[1], (void *)(callback + 0x1c), 0xeb000000U,
                           device_suspend, (unsigned long)suspend_display);
    if (error)
        return error;
    printk("\0016as1_display: verified native HDMI lock handoff; disabled\n");
    return register_pm_notifier(&notifier);
}
