/* SPDX-License-Identifier: GPL-2.0-only */
#include "network_abi.h"

extern struct task_prefix *kthread_create_on_node(int (*)(void *), void *, int,
                                                const char *, ...);
extern void msleep(unsigned int);
extern int kthread_stop(struct task_prefix *);
extern void kthread_bind(struct task_prefix *, unsigned int);
extern void emergency_restart(void);
extern void __put_task_struct(struct task_prefix *);

static struct socket *output;
static char log_buffer[16384];
static volatile int ticks;
static volatile int logging = 1;
static struct task_prefix *owner;
static unsigned int sequence;
static int last_send;
static struct task_prefix *log_task;
static struct task_prefix *recovery_task;
static void (*busy_us)(unsigned long);
static int (*read_log)(int, char *, int, _Bool);
static void (*show_task)(struct task_prefix *);
static struct as1_sockaddr_in destination = {
    .family = 2,
    .port = __builtin_bswap16(49001),
    .address = __builtin_bswap32(0xc0a80114U),
};

static void release_task(struct task_prefix *task)
{
    if (task && !__sync_sub_and_fetch(&task->usage, 1))
        __put_task_struct(task);
}

static int set_armed(const char *value, const struct kernel_param *parameter)
{
    struct task_prefix *previous;
    struct task_prefix *task = 0;
    int seconds;
    int error;

    (void)parameter;
    error = kstrtoint(value, 0, &seconds);
    if (error)
        return error;
    if (seconds < 0 || seconds > 60)
        return -22;
    if (seconds) {
        task = as1_current();
        __sync_add_and_fetch(&task->usage, 1);
    }
    ticks = 0;
    as1_barrier();
    previous = __sync_lock_test_and_set(&owner, task);
    release_task(previous);
    ticks = seconds * 4;
    printk("\0016as1_diag: recovery armed for %d seconds\n", seconds);
    return 0;
}

static int get_stats(char *buffer, const struct kernel_param *parameter)
{
    (void)parameter;
    return scnprintf(buffer, 4096,
                     "ticks=%d logging=%d sequence=%u send=%d log_flags=%08x recovery_flags=%08x\n",
                     ticks, logging, sequence, last_send,
                     log_task ? log_task->flags : 0,
                     recovery_task ? recovery_task->flags : 0);
}

static int set_logging(const char *value, const struct kernel_param *parameter)
{
    int next;
    int error;

    (void)parameter;
    error = kstrtoint(value, 0, &next);
    if (error)
        return error;
    if (next != 0 && next != 1)
        return -22;
    logging = next;
    return 0;
}

static void send_log(void)
{
    int length = read_log(3, log_buffer, sizeof(log_buffer), 1);
    int offset;

    sequence++;
    for (offset = 0; offset < length; offset += 1200) {
        unsigned int part = (unsigned int)(length - offset);
        struct as1_kvec vector;
        struct as1_msghdr message = {0};

        if (part > 1200)
            part = 1200;
        vector.base = log_buffer + offset;
        vector.length = part;
        message.name = &destination;
        message.name_length = sizeof(destination);
        message.flags = 0x40;
        last_send = kernel_sendmsg(output, &message, &vector, 1, part);
    }
}

static int monitor(void *unused)
{
    (void)unused;
    for (;;) {
        if (logging)
            send_log();
        msleep(250);
    }
    return 0;
}

static int recovery(void *unused)
{
    (void)unused;
    for (;;) {
        int remaining = ticks;

        if (remaining > 0 &&
            __sync_bool_compare_and_swap(&ticks, remaining, remaining - 1)) {
            if (remaining == 120) {
                u32 *descriptor = irq_to_desc(83);
                u32 *action = *(u32 **)((char *)descriptor + 0x30);

                printk("\0014as1_diag: timed freeze wake; IRQ83 flags=%08x state=%08x depth=%u\n",
                       action[7], descriptor[14], descriptor[15]);
                freeze_wake();
            } else if (remaining == 1) {
                struct task_prefix *task = __sync_lock_test_and_set(&owner, 0);

                printk("\0013as1_diag: deadline reached; suspend caller follows\n");
                if (task)
                    show_task(task);
                release_task(task);
                msleep(500);
                emergency_restart();
            }
        }
        if (ticks > 0) {
            unsigned int i;

            for (i = 0; i < 250; i++)
                busy_us(1000);
        } else {
            msleep(250);
        }
    }
    return 0;
}

AS1_PARAM(armed, 0200, set_armed, 0);
AS1_PARAM(stats, 0400, 0, get_stats);
AS1_PARAM(logging, 0200, set_logging, 0);

int init_module(void)
{
    struct task_prefix *thread;
    struct task_prefix *watcher;
    int error;

    _Static_assert(sizeof(struct as1_msghdr) == 28, "Linux3.10 ARM32 msghdr");
    read_log = (void *)kallsyms_lookup_name("do_syslog");
    show_task = (void *)kallsyms_lookup_name("sched_show_task");
    busy_us = (void *)kallsyms_lookup_name("__loop_udelay");
    if (!read_log || !show_task || !busy_us)
        return -2;
    error = sock_create_kern(2, 2, 17, &output);
    if (error)
        return error;
    watcher = kthread_create_on_node(recovery, 0, -1, "as1_pm_recover");
    if ((unsigned long)watcher >= (unsigned long)-4095) {
        sock_release(output);
        return (int)watcher;
    }
    thread = kthread_create_on_node(monitor, 0, -1, "as1_pm_diag");
    if ((unsigned long)thread >= (unsigned long)-4095) {
        kthread_stop(watcher);
        sock_release(output);
        return (int)thread;
    }
    kthread_bind(watcher, 1);
    recovery_task = watcher;
    log_task = thread;
    wake_up_process(watcher);
    wake_up_process(thread);
    printk("\0016as1_diag: UDP log output to 192.168.1.20:49001 ready\n");
    return 0;
}
