/* SPDX-License-Identifier: GPL-2.0-only */
#include "target_abi.h"

extern struct task_prefix *kthread_create_on_node(int (*)(void *), void *, int,
                                                const char *, ...);
extern void msleep(unsigned int);
extern int unregister_pm_notifier(struct notifier_block *);

static int (*front_ready)(void);
static int (*stop_heartbeat)(void);
static volatile u32 *wait_next;
static volatile unsigned char *freeze_woken;
static int enabled;
static volatile int active;
static volatile int ticks;
static unsigned int failures;

static int power_event(struct notifier_block *block, unsigned long event,
                       void *unused)
{
    (void)block;
    (void)unused;
    if (event == 3 && enabled) {
        ticks = 180;
        as1_barrier();
        active = 1;
    } else if (event == 4) {
        active = 0;
    }
    return 0;
}

static int monitor(void *unused)
{
    (void)unused;
    for (;;) {
        msleep(250);
        if (!active)
            continue;
        if (*wait_next != (u32)wait_next && !*freeze_woken) {
            ticks = 180;
            continue;
        }
        if (--ticks <= 0) {
            int result;

            active = 0;
            failures++;
            result = stop_heartbeat();
            printk("\0013as1_guard: PM transition exceeded 45 seconds; MCU failstop=%d\n",
                   result);
        }
    }
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
    if ((next != 0 && next != 1) || active || (next && !front_ready()))
        return -22;
    enabled = next;
    return 0;
}

static int get_stats(char *buffer, const struct kernel_param *parameter)
{
    (void)parameter;
    return scnprintf(buffer, 4096,
                     "enabled=%d active=%d ticks=%d waiting=%d woken=%u failures=%u\n",
                     enabled, active, ticks, *wait_next != (u32)wait_next,
                     *freeze_woken, failures);
}

static struct notifier_block notifier = {
    .call = power_event,
    .priority = 2147483647,
};

AS1_PARAM(enabled, 0200, set_enabled, 0);
AS1_PARAM(stats, 0400, 0, get_stats);

int init_module(void)
{
    unsigned long enter = kallsyms_lookup_name("suspend_devices_and_enter");
    unsigned long prepare = kallsyms_lookup_name("prepare_to_wait");
    unsigned long head = kallsyms_lookup_name("suspend_freeze_wait_head");
    struct task_prefix *thread;
    int error;

    freeze_woken = (void *)kallsyms_lookup_name("suspend_freeze_wake");
    front_ready = (void *)kallsyms_lookup_name("as1_front_ready");
    stop_heartbeat = (void *)kallsyms_lookup_name("as1_front_failstop");
    if (!enter || !prepare || !head || !freeze_woken ||
        !front_ready || !stop_heartbeat ||
        *(u32 *)(enter + 0x280) != 0xe5d73004U ||
        *(u32 *)(enter + 0x440) != head ||
        *(u32 *)(prepare + 0x5c) != 0xe5b32020U)
        return -22;
    wait_next = (void *)(head + 0x20);
    if (*wait_next != (u32)wait_next)
        return -16;
    error = register_pm_notifier(&notifier);
    if (error)
        return error;
    thread = kthread_create_on_node(monitor, 0, -1, "as1_pm_guard");
    if ((unsigned long)thread >= (unsigned long)-4095) {
        unregister_pm_notifier(&notifier);
        return (int)thread;
    }
    wake_up_process(thread);
    printk("\0016as1_guard: verified freeze wait queue; PM transition guard ready\n");
    return 0;
}
