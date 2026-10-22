/* SPDX-License-Identifier: GPL-2.0-only */
#include "target_abi.h"

extern void get_task_comm(char *, struct task_prefix *);
extern void __put_task_struct(struct task_prefix *);
extern int send_sig(int, struct task_prefix *, int);

static struct task_prefix *tasks[2];
static unsigned int saved_flags[2];
static u32 *uart_action;
static u32 uart_flags;
static struct task_prefix *(*find_task)(int);
static int active;

int as1_front_ready(void)
{
    return tasks[0] && tasks[1];
}

int as1_front_failstop(void)
{
    if (!tasks[1])
        return -3;
    return send_sig(19, tasks[1], 1);
}

static int change_tasks(void *value)
{
    int next = *(int *)value;
    unsigned int i;

    for (i = 0; i < 2; i++) {
        if (!tasks[i])
            continue;
        tasks[i]->flags = (tasks[i]->flags & ~0x8000U) |
                          (next ? 0x8000U : saved_flags[i]);
    }
    uart_action[7] = next ? uart_flags | 0x4000U : uart_flags;
    as1_barrier();
    return 0;
}

static int set_keep_tid(const char *value, const struct kernel_param *parameter)
{
    struct task_prefix *task;
    char name[16];
    unsigned int i;
    int tid;
    int error;

    (void)parameter;
    error = kstrtoint(value, 0, &tid);
    if (error)
        return error;
    if (tid <= 0 || active)
        return -22;
    __rcu_read_lock();
    task = find_task(tid);
    if (task)
        __sync_add_and_fetch(&task->usage, 1);
    __rcu_read_unlock();
    if (!task)
        return -3;
    get_task_comm(name, task);
    if (!strncmp(name, "abs.T_FPD", 16))
        i = 0;
    else if (!strncmp(name, "abs.T_FPD3", 16))
        i = 1;
    else {
        error = -22;
        goto release;
    }
    if (tasks[i]) {
        error = -16;
        goto release;
    }
    tasks[i] = task;
    saved_flags[i] = task->flags & 0x8000U;
    return 0;

release:
    if (!__sync_sub_and_fetch(&task->usage, 1))
        __put_task_struct(task);
    return error;
}

static int set_enabled(const char *value, const struct kernel_param *parameter)
{
    int next;
    int error;

    (void)parameter;
    error = kstrtoint(value, 0, &next);
    if (error)
        return error;
    if ((next != 0 && next != 1) || !tasks[0] || !tasks[1])
        return -22;
    if (next == active)
        return 0;
    error = stop_machine(change_tasks, &next, 0);
    if (!error)
        active = next;
    return error;
}

static int get_stats(char *buffer, const struct kernel_param *parameter)
{
    (void)parameter;
    return scnprintf(buffer, 4096,
                     "enabled=%d receiver=%08x watchdog=%08x irq86=%08x\n",
                     active, tasks[0] ? tasks[0]->flags : 0,
                     tasks[1] ? tasks[1]->flags : 0, uart_action[7]);
}

AS1_PARAM(keep_tid, 0200, set_keep_tid, 0);
AS1_PARAM(enabled, 0200, set_enabled, 0);
AS1_PARAM(stats, 0400, 0, get_stats);

int init_module(void)
{
    void *descriptor = irq_to_desc(86);

    find_task = (void *)kallsyms_lookup_name("find_task_by_vpid");
    if (!descriptor || !find_task)
        return -22;
    uart_action = *(u32 **)((char *)descriptor + 0x30);
    if (!uart_action || uart_action[6] != 86 || uart_action[3] ||
        uart_action[4] || !uart_action[1])
        return -22;
    uart_flags = uart_action[7];
    printk("\0016as1_front: front-controller comparison ready; IRQ86 flags=%08x\n",
           uart_flags);
    return 0;
}
