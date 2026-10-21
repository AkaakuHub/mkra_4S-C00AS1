#!/usr/bin/perl
die "usage: keep_awake.pl\n" if @ARGV;

my $firmware = '1a3fdfa8379d75a802190b48ea951e9a';
my $quick_start = 0x0190acfc;
my $auto_off = 0x0190ac24;
sub read_at {
    my ($fh, $address, $length) = @_;
    defined(sysseek($fh, $address, 0)) or die "seek: $!\n";
    my $bytes;
    sysread($fh, $bytes, $length) == $length or die "read: $!\n";
    return $bytes;
}

sub write_at {
    my ($fh, $address, $bytes) = @_;
    defined(sysseek($fh, $address, 0)) or die "seek: $!\n";
    syswrite($fh, $bytes) == length($bytes) or die "write: $!\n";
    read_at($fh, $address, length($bytes)) eq $bytes
        or die "readback mismatch\n";
}

sub process_state {
    my @abs;
    opendir(my $proc, '/proc') or die "proc: $!\n";
    for my $pid (readdir $proc) {
        next unless $pid =~ /^\d+$/;
        open(my $comm, '<', "/proc/$pid/comm") or next;
        my $name = <$comm>;
        close $comm;
        push @abs, $pid if defined($name) && $name eq "abs\n";
    }
    closedir $proc;
    @abs == 1 or die "expected one abs process\n";
    return $abs[0];
}

sub verify_binary {
    my ($path, $expected) = @_;
    open(my $hash, '-|', '/usr/bin/md5sum', $path)
        or die "md5sum: $!\n";
    my $digest = <$hash>;
    close $hash or die "md5sum failed\n";
    defined($digest) && $digest =~ /^$expected\s/
        or die "unexpected binary: $path\n";
}

my $pid = process_state();
verify_binary("/proc/$pid/exe", $firmware);
open(my $mem, '+<', "/proc/$pid/mem") or die "mem: $!\n";
my $config = read_at($mem, $quick_start, 20);
my ($minutes, $type) = unpack 'vv', $config;
($minutes <= 1440 || $minutes == 0xffff) && $type <= 2
    or die "unexpected quick-start configuration\n";
my $off = unpack 'V', read_at($mem, $auto_off, 4);
$off <= 1 or die "unexpected auto-off configuration\n";
write_at($mem, $quick_start, pack('vv', 0xffff, 0))
    if $minutes != 0xffff || $type != 0;
write_at($mem, $auto_off, pack('V', 0)) if $off != 0;
my $after = read_at($mem, $quick_start, 20);
$after eq pack('vv', 0xffff, 0) . substr($config, 4)
    or die "quick-start verification failed\n";
unpack('V', read_at($mem, $auto_off, 4)) == 0
    or die "auto-off verification failed\n";
close $mem or die "close: $!\n";
printf "%d pid=%d quick_start=always auto_off=disabled previous=%u,%u,%u\n",
    time, $pid, $minutes, $type, $off;
