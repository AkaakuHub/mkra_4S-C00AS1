/* SPDX-License-Identifier: GPL-2.0-only */
#include "target_abi.h"

struct irqaction_prefix {
    int (*handler)(int, void *);
    void *device;
    void *percpu_device;
    struct irqaction_prefix *next;
    int (*thread_fn)(int, void *);
    void *thread;
    unsigned int irq;
    unsigned int flags;
};

static struct irqaction_prefix *action;
static int (*native_handler)(int, void *);
static u32 (*read_register)(void *, u32);
static unsigned int native_flags;
static int enabled;
static volatile int suspending;
static unsigned int pmt_events;
static unsigned int wake_events;

static int wake_irq(int irq, void *device)
{
    u32 dma = *(u32 *)((char *)device + 0x614);
    u32 status = read_register((char *)device + 0x610, dma + 0x14);
    int result;

    result = native_handler(irq, device);
    if (status & 0x10000000U) {
        __sync_fetch_and_add(&pmt_events, 1);
        if (suspending) {
            __sync_fetch_and_add(&wake_events, 1);
            freeze_wake();
            printk("\0016as1_wake: PMT interrupt woke freeze\n");
        }
    }
    return result;
}

static int change_irq(void *value)
{
    int next = *(int *)value;

    if (action->handler != (enabled ? wake_irq : native_handler))
        return -22;
    action->flags = next ? native_flags | 0x4000U : native_flags;
    action->handler = next ? wake_irq : native_handler;
    as1_barrier();
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
    if (!action || (next != 0 && next != 1) || suspending)
        return -22;
    if (next == enabled)
        return 0;
    error = stop_machine(change_irq, &next, 0);
    if (!error)
        enabled = next;
    return error;
}

static int get_stats(char *buffer, const struct kernel_param *parameter)
{
    (void)parameter;
    return scnprintf(buffer, 4096,
                     "enabled=%d suspending=%d pmt=%u wakes=%u flags=%08x\n",
                     enabled, suspending, pmt_events, wake_events,
                     action ? action->flags : 0);
}

static int power_event(struct notifier_block *block, unsigned long event,
                       void *unused)
{
    (void)block;
    (void)unused;
    if (event == 3 && enabled)
        suspending = 1;
    else if (event == 4)
        suspending = 0;
    return 0;
}

static struct notifier_block notifier = {.call = power_event};

AS1_PARAM(enabled, 0200, set_enabled, 0);
AS1_PARAM(stats, 0400, 0, get_stats);

int init_module(void)
{
    const char *banner = (const char *)kallsyms_lookup_name("linux_banner");
    void *descriptor;

    _Static_assert(sizeof(struct irqaction_prefix) == 32, "ARM32 irqaction ABI");
    if (!banner || strncmp(banner, "Linux version 3.10.24 ", 22))
        return -22;
    descriptor = irq_to_desc(83);
    if (!descriptor || *(u32 *)descriptor != 83)
        return -22;
    action = *(struct irqaction_prefix **)((char *)descriptor + 0x30);
    native_handler = (void *)kallsyms_lookup_name("synopGMAC_intr_handler");
    read_register = (void *)kallsyms_lookup_name("plat_ReadReg");
    if (!action || !native_handler || !read_register ||
        action->handler != native_handler ||
        action->irq != 83 || action->flags != 0x80 || !action->device ||
        action->next || action->thread_fn)
        return -22;
    {
        unsigned long call = (unsigned long)native_handler + 0x24;
        u32 instruction = *(u32 *)call;
        unsigned long target = call + 8 + ((long)(instruction << 8) >> 6);

        if ((instruction & 0xff000000U) != 0xeb000000U ||
            target != (unsigned long)read_register)
            return -22;
    }
    native_flags = action->flags;
    printk("\0016as1_wake: verified IRQ83 native handler=%p ready\n",
           (void *)native_handler);
    return register_pm_notifier(&notifier);
}
