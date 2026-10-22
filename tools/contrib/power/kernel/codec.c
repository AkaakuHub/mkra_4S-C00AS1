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
    if ((next != 0 && next != 1) || !callbacks[0] || !callbacks[1])
        return -22;
    if (next == bypass)
        return 0;
    error = stop_machine(change_callbacks, &next, 0);
    if (!error) {
        on_each_cpu(sync_code, 0, 1);
        bypass = next;
        printk("\0014as1_codec: codec suspend/resume bypass=%d\n", bypass);
    }
    return error;
}

AS1_PARAM(bypass, 0200, set_bypass, 0);

int init_module(void)
{
    unsigned long context = kallsyms_lookup_name("gps_XCode");
    unsigned int i;

    callbacks[0] = (void *)kallsyms_lookup_name("xc_device_suspend");
    callbacks[1] = (void *)kallsyms_lookup_name("xc_device_resume");
    if (!context || !callbacks[0] || !callbacks[1] ||
        callbacks[0][0] != 0xe92d4038U ||
        callbacks[0][1] != (0xe3003000U | ((context & 0xf000U) << 4) |
                           (context & 0xfffU)) ||
        callbacks[1][0] != 0xe92d4038U || callbacks[1][1] != 0xe59f3084U)
        return -22;
    for (i = 0; i < 2; i++) {
        saved[i][0] = callbacks[i][0];
        saved[i][1] = callbacks[i][1];
    }
    printk("\0016as1_codec: codec callbacks verified; bypass disabled\n");
    return 0;
}
