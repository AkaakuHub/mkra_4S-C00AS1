/* SPDX-License-Identifier: GPL-2.0-only */
#ifndef AS1_NETWORK_ABI_H
#define AS1_NETWORK_ABI_H

#include "target_abi.h"

struct socket;
struct as1_kvec {
    void *base;
    unsigned int length;
};

struct as1_msghdr {
    void *name;
    int name_length;
    void *iov;
    unsigned int iov_length;
    void *control;
    unsigned int control_length;
    unsigned int flags;
};

struct as1_sockaddr_in {
    unsigned short family;
    unsigned short port;
    u32 address;
    unsigned char zero[8];
};

extern int sock_create_kern(int, int, int, struct socket **);
extern void sock_release(struct socket *);
extern int kernel_sendmsg(struct socket *, struct as1_msghdr *,
                          struct as1_kvec *, unsigned int, unsigned int);

#endif
