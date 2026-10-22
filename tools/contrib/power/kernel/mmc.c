/* SPDX-License-Identifier: GPL-2.0-only */
#include "target_abi.h"

extern void *idr_find(void *, int);
extern int __mmc_claim_host(void *, void *);
extern void mmc_release_host(void *);

static char *host;
static u32 *removable_ops;
static u32 *fixed_ops;
static u32 saved_caps;
static int enabled;

static int card_matches(void)
{
    char *card = *(char **)(host + 0x2d0);

    return card && *(void **)card == host && *(u32 *)(card + 0x234) == 0;
}

static int set_enabled(const char *value, const struct kernel_param *parameter)
{
    int next;
    int error;
    u32 caps;

    (void)parameter;
    error = kstrtoint(value, 0, &next);
    if (error)
        return error;
    if (next != 0 && next != 1)
        return -22;
    if (next == enabled)
        return 0;
    if (__mmc_claim_host(host, 0))
        return -16;
    if (!card_matches() || *(u32 **)(host + 0x47c) !=
        (enabled ? fixed_ops : removable_ops)) {
        mmc_release_host(host);
        return -22;
    }
    caps = *(u32 *)(host + 0x26c);
    *(u32 *)(host + 0x26c) = (caps & ~0x100U) |
                             (next ? 0x100U : saved_caps & 0x100U);
    *(u32 **)(host + 0x47c) = next ? fixed_ops : removable_ops;
    as1_barrier();
    enabled = next;
    mmc_release_host(host);
    printk("\0016as1_mmc: fixed eMMC resume enabled=%d\n", enabled);
    return 0;
}

static int get_stats(char *buffer, const struct kernel_param *parameter)
{
    (void)parameter;
    return scnprintf(buffer, 4096,
                     "enabled=%d host=%p caps=%08x bus_ops=%p card=%p\n",
                     enabled, host, *(u32 *)(host + 0x26c),
                     *(void **)(host + 0x47c), *(void **)(host + 0x2d0));
}

AS1_PARAM(enabled, 0200, set_enabled, 0);
AS1_PARAM(stats, 0400, 0, get_stats);

int init_module(void)
{
    void *hosts = (void *)kallsyms_lookup_name("mmc_host_idr");
    void *host_ops = (void *)kallsyms_lookup_name("xcsdh_mmc_ops");
    u32 *notify = (void *)kallsyms_lookup_name("mmc_pm_notify");
    u32 *attach = (void *)kallsyms_lookup_name("mmc_attach_mmc");
    unsigned long suspend = kallsyms_lookup_name("mmc_suspend");
    unsigned long resume = kallsyms_lookup_name("mmc_resume");
    unsigned int i;

    removable_ops = (void *)kallsyms_lookup_name("mmc_ops");
    fixed_ops = (void *)kallsyms_lookup_name("mmc_ops_unsafe");
    if (!hosts || !host_ops || !notify || !attach || !suspend || !resume ||
        !removable_ops || !fixed_ops || notify[0] != 0xe3510006U ||
        notify[3] != 0xe2406f95U || attach[7] != 0xe594326cU ||
        removable_ops[4] || removable_ops[5] ||
        fixed_ops[4] != suspend || fixed_ops[5] != resume)
        return -22;
    for (i = 0; i < 9; i++)
        if (i != 4 && i != 5 && removable_ops[i] != fixed_ops[i])
            return -22;
    __rcu_read_lock();
    host = idr_find(hosts, 0);
    __rcu_read_unlock();
    if (!host || *(void **)(host + 0x230) != host_ops || !card_matches() ||
        *(u32 **)(host + 0x47c) != removable_ops)
        return -22;
    saved_caps = *(u32 *)(host + 0x26c);
    printk("\0016as1_mmc: verified mmc0 native eMMC; repair disabled\n");
    return 0;
}
