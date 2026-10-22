/* SPDX-License-Identifier: GPL-2.0-only */
#include "target_abi.h"

static volatile u32 *callbacks[2];
static u32 saved[2][2];
static int bypass;
static const u32 return_success[2] = {0xe3a00000U, 0xe12fff1eU};

static void sync_code(void *unused)
{
    unsigned int i;

    (void)unused;
    for (i = 0; i < 2; i++)
        v7_coherent_kern_range((unsigned long)callbacks[i],
                              (unsigned long)(callbacks[i] + 2));
}

static int change_callbacks(void *value)
{
    int next = *(int *)value;
    unsigned int i;
    unsigned int j;

    for (i = 0; i < 2; i++)
        for (j = 0; j < 2; j++)
            if (callbacks[i][j] != (bypass ? return_success[j] : saved[i][j]))
                return -22;
    for (i = 0; i < 2; i++)
        for (j = 0; j < 2; j++)
            callbacks[i][j] = next ? return_success[j] : saved[i][j];
    sync_code(0);
    return 0;
}

static int set_bypass(const char *value, const struct kernel_param *parameter)
{
    int next;
    int error;

    (void)parameter;
    error = kstrtoint(value, 0, &next);
    if (error)
        return error;
    if (!callbacks[0] || !callbacks[1] || (next != 0 && next != 1))
        return -22;
    if (next == bypass)
        return 0;
    error = stop_machine(change_callbacks, &next, 0);
    if (!error) {
        on_each_cpu(sync_code, 0, 1);
        bypass = next;
        printk("\0014as1_display: display suspend/resume bypass=%d\n", bypass);
    }
    return error;
}

static int get_bypass(char *buffer, const struct kernel_param *parameter)
{
    (void)parameter;
    return scnprintf(buffer, 4096, "%d\n", bypass);
}

AS1_PARAM(bypass, 0600, set_bypass, get_bypass);

int init_module(void)
{
    const char *names[2] = {"xcfb_disp_suspend", "xcfb_disp_resume"};
    unsigned int i;

    for (i = 0; i < 2; i++) {
        callbacks[i] = (volatile u32 *)kallsyms_lookup_name(names[i]);
        if (!callbacks[i] || callbacks[i][0] != 0xe92d4008U ||
            callbacks[i][1] != 0xe3a00601U)
            return -22;
        saved[i][0] = callbacks[i][0];
        saved[i][1] = callbacks[i][1];
    }
    printk("\0016as1_display: display callbacks verified; bypass disabled\n");
    return 0;
}
