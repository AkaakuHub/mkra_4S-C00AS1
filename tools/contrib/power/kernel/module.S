/* SPDX-License-Identifier: GPL-2.0-only */
.syntax unified
.section .gnu.linkonce.this_module,"aw",%progbits
.balign 4
.global __this_module
.type __this_module,%object
__this_module:
    .zero 12
    .asciz "as1_freezer"
    .zero 0xbc - (. - __this_module)
    .word init_module
    .zero 0x168 - (. - __this_module)
.size __this_module, . - __this_module

.section .modinfo,"a",%progbits
.asciz "license=GPL"
.asciz "description=AS1 xcdrv freezable wait repair"
.asciz "vermagic=3.10.24 SMP preempt mod_unload ARMv7 p2v8 "
.section .note.GNU-stack,"",%progbits
