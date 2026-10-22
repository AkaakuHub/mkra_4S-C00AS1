/* SPDX-License-Identifier: GPL-2.0-only */
#include "target_abi.h"

extern void get_task_comm(char *, struct task_prefix *);
extern unsigned long __copy_from_user(void *, const void *, unsigned long);

static volatile u32 *site;
static u32 original, patched;
static int enabled, queue_id = -1;
static volatile int held;
static unsigned int denied;
static int (*native_send)(int, long, const void *, unsigned int, int);

static int transfer(int queue, long type, const void *text, unsigned int size, int flags)
{
    u32 header[8];
    if (held && queue == queue_id && size >= sizeof(header) &&
        !__copy_from_user(header, text, sizeof(header)) &&
        header[4] == 0x6004 && header[7] == 0) {
        char name[16];
        get_task_comm(name, as1_current());
        if (!strncmp(name, "abs.T_UIM", sizeof(name))) {
            __sync_fetch_and_add(&denied, 1);
            return -16;
        }
    }
    return native_send(queue, type, text, size, flags);
}

static int get_stats(char *buffer, const struct kernel_param *parameter)
{
    (void)parameter;
    return scnprintf(buffer, 4096, "enabled=%d held=%d queue=%d denied=%u\n",
                     enabled, held, queue_id, denied);
}

static void sync_code(void *unused)
{
    (void)unused;
    v7_coherent_kern_range((unsigned long)site, (unsigned long)(site + 1));
}

static int change(void *value)
{
    int next = *(int *)value;
    if (*site != (enabled ? patched : original))
        return -22;
    *site = next ? patched : original;
    sync_code(0);
    return 0;
}

static int set_enabled(const char *value, const struct kernel_param *parameter)
{
    int next, result;
    (void)parameter;
    result = kstrtoint(value, 0, &next);
    if (result)
        return result;
    if ((next != 0 && next != 1) || (next && queue_id < 0))
        return -22;
    if (next == enabled)
        return 0;
    if (held)
        return -16;
    result = stop_machine(change, &next, 0);
    if (!result) {
        on_each_cpu(sync_code, 0, 1);
        enabled = next;
    }
    return result;
}

static int set_queue(const char *value, const struct kernel_param *parameter)
{
    int next, result;
    (void)parameter;
    if (enabled)
        return -16;
    result = kstrtoint(value, 0, &next);
    if (result)
        return result;
    if (next < 0)
        return -22;
    queue_id = next;
    return 0;
}

static int set_held(const char *value, const struct kernel_param *parameter)
{
    int next, result;
    (void)parameter;
    result = kstrtoint(value, 0, &next);
    if (result)
        return result;
    if ((next != 0 && next != 1) || !enabled)
        return -22;
    held = next;
    as1_barrier();
    return 0;
}

AS1_PARAM(enabled, 0200, set_enabled, 0);
AS1_PARAM(queue, 0200, set_queue, 0);
AS1_PARAM(held, 0200, set_held, 0);
AS1_PARAM(stats, 0400, 0, get_stats);

int init_module(void)
{
    unsigned long entry = kallsyms_lookup_name("sys_msgsnd");
    long delta;
    native_send = (void *)kallsyms_lookup_name("do_msgsnd");
    if (!entry || !native_send || *(u32 *)entry != 0xe92d40f0U)
        return -22;
    site = (void *)(entry + 0x54);
    original = *site;
    if ((original & 0xff000000U) != 0xeb000000U ||
        entry + 0x5c + ((long)(original << 8) >> 6) != (unsigned long)native_send)
        return -22;
    delta = (unsigned long)transfer - (unsigned long)site - 8;
    if ((delta & 3) || delta < -33554432L || delta > 33554428L)
        return -34;
    patched = 0xeb000000U | ((delta >> 2) & 0xffffffU);
    printk("\0016as1_tuner: verified native UI selection call; disabled\n");
    return 0;
}
