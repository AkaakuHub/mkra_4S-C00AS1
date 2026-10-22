/* SPDX-License-Identifier: GPL-2.0-only */
#include "target_abi.h"

extern void get_task_comm(char *, struct task_prefix *);

static volatile u32 *lock_entry;
static volatile u32 *unlock_entry;
static volatile u32 *suspend_call;
static unsigned long suspend_start;
static u32 original_branch;
static u32 traced_branch;
static u32 original_unlock;
static u32 traced_unlock_branch;
static u32 original_call;
static u32 traced_call;
static int enabled;
static int (*native_down)(void *);
static void (*native_up)(void *);
static int (*native_suspend)(void *);
static void (*emit_log)(void);
static struct task_prefix *suspend_task;
static void *hdmi_semaphore;
static struct task_prefix *volatile hdmi_owner;
static char owner_name[16];
static unsigned long owner_caller;

static int traced_suspend(void *device)
{
    int result;

    suspend_task = as1_current();
    printk("\0014as1_display_trace: entering display device suspend\n");
    emit_log();
    result = native_suspend(device);
    suspend_task = 0;
    printk("\0014as1_display_trace: leaving display device suspend result=%d\n",
           result);
    emit_log();
    return result;
}

static int traced_lock(void *semaphore)
{
    unsigned long caller = (unsigned long)__builtin_return_address(0);
    int tracing = as1_current() == suspend_task;
    int result;

    if (tracing) {
        printk("\0014as1_display_trace: before lock caller=%lx sem=%p\n",
               caller, semaphore);
        if (semaphore == hdmi_semaphore) {
            as1_barrier();
            printk("\0014as1_display_trace: HDMI last observed owner=%p name=%s caller=%lx\n",
                   (void *)hdmi_owner, owner_name, owner_caller);
        }
        emit_log();
    }
    result = native_down(semaphore);
    if (!result && semaphore == hdmi_semaphore) {
        get_task_comm(owner_name, as1_current());
        owner_caller = caller;
        as1_barrier();
        hdmi_owner = as1_current();
    }
    if (tracing) {
        printk("\0014as1_display_trace: after lock caller=%lx result=%d\n",
               caller, result);
        emit_log();
    }
    return result;
}

static void traced_unlock(void *semaphore)
{
    if (semaphore == hdmi_semaphore) {
        hdmi_owner = 0;
        as1_barrier();
    }
    native_up(semaphore);
}

static void sync_code(void *unused)
{
    (void)unused;
    v7_coherent_kern_range((unsigned long)lock_entry,
                          (unsigned long)(unlock_entry + 1));
    v7_coherent_kern_range((unsigned long)suspend_call,
                          (unsigned long)(suspend_call + 1));
}

static int change_branch(void *value)
{
    int next = *(int *)value;

    if (*lock_entry != (enabled ? traced_branch : original_branch) ||
        *unlock_entry != (enabled ? traced_unlock_branch : original_unlock) ||
        *suspend_call != (enabled ? traced_call : original_call))
        return -22;
    *lock_entry = next ? traced_branch : original_branch;
    *unlock_entry = next ? traced_unlock_branch : original_unlock;
    *suspend_call = next ? traced_call : original_call;
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
    if (next != 0 && next != 1)
        return -22;
    if (next == enabled)
        return 0;
    error = stop_machine(change_branch, &next, 0);
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
                     "enabled=%d hdmi_sem=%p owner=%p name=%s caller=%lx\n",
                     enabled, hdmi_semaphore, (void *)hdmi_owner,
                     owner_name, owner_caller);
}

AS1_PARAM(enabled, 0200, set_enabled, 0);
AS1_PARAM(stats, 0400, 0, get_stats);

int init_module(void)
{
    unsigned long call;
    unsigned long callback;
    unsigned long target;
    u32 instruction;
    long delta;
    u32 *device_slot;

    suspend_start = kallsyms_lookup_name("xcfb_disp_dev_suspend");
    callback = kallsyms_lookup_name("xcfb_disp_suspend");
    native_down = (void *)kallsyms_lookup_name("down_interruptible");
    native_up = (void *)kallsyms_lookup_name("up");
    emit_log = (void *)kallsyms_lookup_name("as1_diag_emit");
    if (!suspend_start || !callback || !native_down || !native_up || !emit_log)
        return -2;
    if (*(u32 *)(callback + 0x14) != 0xe59f3040U)
        return -22;
    device_slot = *(u32 **)(callback + 0x5c);
    if (!device_slot || !*device_slot)
        return -22;
    hdmi_semaphore = *(void **)(*device_slot + 0x4a884);
    if (!hdmi_semaphore)
        return -22;
    native_suspend = (void *)suspend_start;
    suspend_call = (void *)(callback + 0x1c);
    original_call = *suspend_call;
    target = (unsigned long)suspend_call + 8 +
             ((long)(original_call << 8) >> 6);
    if ((original_call & 0xff000000U) != 0xeb000000U ||
        target != suspend_start)
        return -22;
    delta = (unsigned long)traced_suspend - (unsigned long)suspend_call - 8;
    if ((delta & 3) || delta < -33554432L || delta > 33554428L)
        return -34;
    traced_call = 0xeb000000U | ((delta >> 2) & 0x00ffffffU);
    call = suspend_start + 0x24;
    instruction = *(u32 *)call;
    if ((instruction & 0xff000000U) != 0xeb000000U)
        return -22;
    lock_entry = (void *)(call + 8 + ((long)(instruction << 8) >> 6));
    call = suspend_start + 0x64;
    instruction = *(u32 *)call;
    target = call + 8 + ((long)(instruction << 8) >> 6);
    if ((instruction & 0xff000000U) != 0xeb000000U ||
        target != (unsigned long)lock_entry)
        return -22;
    original_branch = *lock_entry;
    target = (unsigned long)lock_entry + 8 +
             ((long)(original_branch << 8) >> 6);
    if ((original_branch & 0xff000000U) != 0xea000000U ||
        target != (unsigned long)native_down)
        return -22;
    delta = (unsigned long)traced_lock - (unsigned long)lock_entry - 8;
    if ((delta & 3) || delta < -33554432L || delta > 33554428L)
        return -34;
    traced_branch = 0xea000000U | ((delta >> 2) & 0x00ffffffU);
    if (lock_entry[1] != 0xe92d4008U)
        return -22;
    unlock_entry = lock_entry + 2;
    original_unlock = *unlock_entry;
    target = (unsigned long)unlock_entry + 8 +
             ((long)(original_unlock << 8) >> 6);
    if ((original_unlock & 0xff000000U) != 0xeb000000U ||
        target != (unsigned long)native_up)
        return -22;
    delta = (unsigned long)traced_unlock - (unsigned long)unlock_entry - 8;
    if ((delta & 3) || delta < -33554432L || delta > 33554428L)
        return -34;
    traced_unlock_branch = 0xeb000000U | ((delta >> 2) & 0x00ffffffU);
    printk("\0016as1_display_trace: verified display lock branch; disabled\n");
    return 0;
}
