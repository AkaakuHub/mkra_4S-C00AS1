#!/usr/bin/perl
$| = 1;
my $mode = @ARGV ? shift : '';
$mode =~ /\A--(?:check|expire|guard)\z/ && !@ARGV
    or die "usage: watchdog.pl --check|--expire|--guard\n";

open(my $version, '<', '/proc/sys/kernel/osrelease') or die "kernel: $!\n";
my $release = <$version>;
close $version;
defined($release) && $release eq "3.10.24\n" or die "unexpected kernel\n";
my $driver = readlink('/sys/bus/platform/devices/xcode_wdt/driver');
defined($driver) && $driver =~ m{/xcode_wdt\z}
    or die "unexpected watchdog driver\n";

my $watchdog;
END {
    if ($watchdog && defined(fileno($watchdog))) {
        close $watchdog or warn "watchdog close: $!\n";
    }
}
$SIG{TERM} = $SIG{INT} = sub { exit 0 };
sysopen($watchdog, '/dev/watchdog0', 1) or die "watchdog open: $!\n";
my $support = "\0" x 40;
defined(ioctl($watchdog, 0x80285700, $support)) or die "watchdog support: $!\n";
my ($options, $firmware, $identity) = unpack('VVa32', $support);
$identity =~ s/\0.*//s;
$identity eq 'XCODE Watchdog' && $options == 0x8080
    or die "unexpected watchdog: $identity options=$options\n";
my $timeout = pack('i', 25);
defined(ioctl($watchdog, 0xc0045706, $timeout)) or die "watchdog timeout: $!\n";
unpack('i', $timeout) == 25 or die "unexpected watchdog timeout\n";
syswrite($watchdog, "\0") == 1 or die "watchdog keepalive: $!\n";
print "watchdog=$identity timeout=25 mode=$mode pid=$$\n";

if ($mode eq '--expire') {
    sleep 60;
    die "watchdog did not reset within 60 seconds\n";
}
if ($mode eq '--guard') {
    while (1) {
        sleep 5;
        syswrite($watchdog, "\0") == 1 or die "watchdog keepalive: $!\n";
    }
}
