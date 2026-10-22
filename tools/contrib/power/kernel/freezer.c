/* SPDX-License-Identifier: GPL-2.0-only */
#include "target_abi.h"

#define PF_FREEZER_SKIP 0x40000000U

static volatile u32 *schedule_call;
static u32 original_call;
static u32 repaired_call;
static int enabled;
static unsigned int calls;
static unsigned int freezes;
static struct task_prefix *(*find_task)(int);

static void freezable_wait(void)
{
    struct task_prefix *task = as1_current();

    __sync_fetch_and_add(&calls, 1);
    task->flags |= PF_FREEZER_SKIP;
    schedule();
    task->flags &= ~PF_FREEZER_SKIP;
    as1_barrier();
    if (system_freezing_cnt && freezing_slow_path(task)) {
        __sync_fetch_and_add(&freezes, 1);
        __refrigerator(0);
    }
}

static void sync_code(void *unused)
{
    (void)unused;
    v7_coherent_kern_range((unsigned long)schedule_call,
                          (unsigned long)(schedule_call + 1));
}

static int change_call(void *value)
{
    int next = *(int *)value;
    u32 expected = enabled ? repaired_call : original_call;

    if (*schedule_call != expected)
        return -22;
    *schedule_call = next ? repaired_call : original_call;
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
    if (!schedule_call || (next != 0 && next != 1))
        return -22;
    if (next == enabled)
        return 0;
    error = stop_machine(change_call, &next, 0);
    if (error)
        return error;
    on_each_cpu(sync_code, 0, 1);
    enabled = next;
    printk("\0016as1_freezer: enabled=%d\n", enabled);
    return 0;
}

static int get_enabled(char *buffer, const struct kernel_param *parameter)
{
    (void)parameter;
    return scnprintf(buffer, 4096, "%d\n", enabled);
}

static int wake_tid(const char *value, const struct kernel_param *parameter)
{
    struct task_prefix *task;
    int tid;
    int error;

    (void)parameter;
    error = kstrtoint(value, 0, &tid);
    if (error)
        return error;
    if (!enabled || tid <= 0)
        return -22;
    __rcu_read_lock();
    task = find_task(tid);
    if (task)
        wake_up_process(task);
    __rcu_read_unlock();
    return task ? 0 : -3;
}

static int get_stats(char *buffer, const struct kernel_param *parameter)
{
    (void)parameter;
    return scnprintf(buffer, 4096, "calls=%u freezes=%u\n", calls, freezes);
}

AS1_PARAM(enabled, 0600, set_enabled, get_enabled);
AS1_PARAM(wake_tid, 0200, wake_tid, 0);
AS1_PARAM(stats, 0400, 0, get_stats);

int init_module(void)
{
    unsigned long wait;
    unsigned long destination;
    long delta;
    const volatile u32 *code;
    const char *banner;

    _Static_assert(sizeof(struct kernel_param) == 16, "ARM32 parameter ABI");
    _Static_assert(sizeof(struct task_prefix) == 16, "ARM32 task prefix ABI");
    banner = (const char *)kallsyms_lookup_name("linux_banner");
    if (!banner || strncmp(banner, "Linux version 3.10.24 ", 22))
        return -22;
    wait = kallsyms_lookup_name("sleep_current_thread_on_interrupt");
    find_task = (void *)kallsyms_lookup_name("find_task_by_vpid");
    if (!wait || !find_task)
        return -2;
    code = (const volatile u32 *)wait;
    if (code[0] != 0xe92d4010 || code[1] != 0xe1a04001 ||
        code[2] != 0xe5913028 || code[21] != 0xe3a02002 ||
        code[26] != 0xe1a00004 || code[30] != 0xe8bd8010)
        return -22;
    schedule_call = (volatile u32 *)(wait + 0x48);
    original_call = *schedule_call;
    if ((original_call & 0xff000000U) != 0xeb000000U)
        return -22;
    destination = (unsigned long)schedule_call + 8 +
                  ((long)(original_call << 8) >> 6);
    if (destination != (unsigned long)schedule)
        return -22;
    delta = (unsigned long)freezable_wait - (unsigned long)schedule_call - 8;
    if ((delta & 3) || delta < -33554432L || delta > 33554428L)
        return -34;
    repaired_call = 0xeb000000U | ((u32)(delta >> 2) & 0x00ffffffU);
    printk("\0016as1_freezer: verified xcdrv wait=%p call=%08x ready\n",
           (void *)wait, original_call);
    return 0;
}
