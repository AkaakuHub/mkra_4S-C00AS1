/* SPDX-License-Identifier: GPL-2.0-only */
#ifndef AS1_TARGET_ABI_H
#define AS1_TARGET_ABI_H

typedef unsigned int u32;

struct kernel_param;
struct kernel_param_ops {
    int (*set)(const char *, const struct kernel_param *);
    int (*get)(char *, const struct kernel_param *);
    void (*free)(void *);
};

struct kernel_param {
    const char *name;
    const struct kernel_param_ops *ops;
    unsigned short perm;
    short level;
    void *arg;
};

struct task_prefix {
    volatile long state;
    void *stack;
    int usage;
    volatile u32 flags;
};

extern unsigned long kallsyms_lookup_name(const char *);
extern void schedule(void);
extern int system_freezing_cnt;
extern _Bool freezing_slow_path(struct task_prefix *);
extern _Bool __refrigerator(_Bool);
extern int stop_machine(int (*)(void *), void *, const void *);
extern void on_each_cpu(void (*)(void *), void *, int);
extern void v7_coherent_kern_range(unsigned long, unsigned long);
extern void __rcu_read_lock(void);
extern void __rcu_read_unlock(void);
extern int wake_up_process(struct task_prefix *);
extern int kstrtoint(const char *, unsigned int, int *);
extern int scnprintf(char *, unsigned int, const char *, ...);
extern int printk(const char *, ...);
extern int strncmp(const char *, const char *, unsigned int);

#define AS1_PARAM(name, permissions, setter, getter) \
    static const struct kernel_param_ops name##_ops = {setter, getter, 0}; \
    static const struct kernel_param name##_param \
        __attribute__((used, section("__param"), aligned(4))) = { \
            #name, &name##_ops, permissions, -1, 0 \
        }

static inline struct task_prefix *as1_current(void)
{
    unsigned long sp;
    __asm__ volatile("mov %0, sp" : "=r"(sp));
    return *(struct task_prefix **)((sp & ~8191UL) + 12);
}

static inline void as1_barrier(void)
{
    __asm__ volatile("dmb ish" ::: "memory");
}

#endif
