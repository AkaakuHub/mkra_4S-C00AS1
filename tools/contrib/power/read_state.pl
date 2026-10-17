#!/usr/bin/perl
$| = 1;

my $expected = '1a3fdfa8379d75a802190b48ea951e9a';
opendir(my $proc, '/proc') or die "proc: $!\n";
my @pids;
for my $pid (readdir $proc) {
    next unless $pid =~ /^\d+$/;
    open(my $comm, '<', "/proc/$pid/comm") or next;
    my $name = <$comm>;
    close $comm;
    push @pids, $pid if defined($name) && $name eq "abs\n";
}
closedir $proc;
@pids == 1 or die "expected one abs process\n";
my $pid = $pids[0];
open(my $hash, '-|', '/usr/bin/md5sum', "/proc/$pid/exe") or die "md5sum: $!\n";
my $digest = <$hash>;
close $hash or die "md5sum failed\n";
defined($digest) && $digest =~ /^$expected\s/ or die "unexpected abs binary\n";

open(my $mem, '<', "/proc/$pid/mem") or die "mem: $!\n";
sub read32 {
    my ($mem, $address) = @_;
    defined(sysseek($mem, $address, 0)) or die "seek: $!\n";
    my $bytes;
    sysread($mem, $bytes, 4) == 4 or die "read: $!\n";
    return unpack('V', $bytes);
}

my $base = 0x0105ea34;
printf "abs.pid=%d\n", $pid;
for my $offset (0x1c8, 0x1d0, 0x1d4, 0x1f0, 0x200, 0x210, 0x258, 0x278) {
    printf "tFpmStatus+0x%03x=0x%08x\n", $offset, read32($mem, $base + $offset);
}
printf "gePowerStatus=0x%08x\n", read32($mem, 0x0188aa60);
printf "uiFpmPowLedStat=0x%08x\n", read32($mem, 0x0188abbc);
printf "eLastPowerStatus=0x%08x\n", read32($mem, 0x0105e92c);
close $mem or die "close: $!\n";
