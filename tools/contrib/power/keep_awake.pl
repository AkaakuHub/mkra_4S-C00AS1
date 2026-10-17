#!/usr/bin/perl
$| = 1;
my $once = @ARGV == 1 && $ARGV[0] eq '--once';
die "usage: keep_awake.pl [--once]\n" if @ARGV && !$once;

my $firmware = '1a3fdfa8379d75a802190b48ea951e9a';
my $tuner_firmware = 'b94fc6a8f2e506dc66336c5d0d031b69';
my $receiver = '239b5332de3a6394fd6ee6f5af798419';
my $quick_start = 0x0190acfc;
my $auto_off = 0x0190ac24;
my $tuner_system = 0x0003c9e4;

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
    my (@abs, @tuner, $av);
    $av = 0;
    opendir(my $proc, '/proc') or die "proc: $!\n";
    for my $pid (readdir $proc) {
        next unless $pid =~ /^\d+$/;
        open(my $comm, '<', "/proc/$pid/comm") or next;
        my $name = <$comm>;
        close $comm;
        push @abs, $pid if defined($name) && $name eq "abs\n";
        push @tuner, $pid if defined($name) && $name eq "tuner\n";
        $av++ if defined($name) && $name =~ /^(?:MIXER|buf_buf|Sink.*|Gate.*)\n$/;
    }
    closedir $proc;
    @abs == 1 && @tuner == 1 or die "expected one abs and tuner process\n";
    return ($abs[0], $tuner[0], $av);
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

my $missing = 0;
my $last_wake = 0;
while (1) {
    my ($pid) = process_state();
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
    last if $once;
    for (1..15) {
        sleep 2;
        my ($current_abs, $tuner, $av) = process_state();
        if ($av) {
            $missing = 0;
            next;
        }
        $missing++;
        next if $missing < 2 || time - $last_wake < 30;
        $last_wake = time;
        verify_binary("/proc/$tuner/exe", $tuner_firmware);
        open(my $tmem, '<', "/proc/$tuner/mem") or die "tuner mem: $!\n";
        my $system = unpack 'V', read_at($tmem, $tuner_system, 4);
        close $tmem;
        next unless $system == 0x0d;
        verify_binary('/tmp/mt/mkra', $receiver);
        print time . " av_pipeline=absent tuner_system=isdb-s3 wake=resume\n";
        my $result = system('/tmp/mt/mkra', 'tune4k', '--sid', '101', '--tune-mode', '3');
        print time . " wake_exit=$result\n";
    }
}
