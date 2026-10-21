mod console_capture;
mod native_transport;
mod save_quick_start;

use std::fs;
use std::io;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;

fn query() -> io::Result<()> {
    let receiver = native_transport::receiver()?;
    let captured = console_capture::capture(|| {
        native_transport::send(&receiver, native_transport::Request::Description)
    })?;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open("/tmp/mt/native-query-console.log")?
        .write_all(&captured)?;
    let output = String::from_utf8_lossy(&captured);
    if !output.lines().any(|line| line.trim() == "no param") {
        return Err(io::Error::other(
            "native command description was not captured",
        ));
    }
    println!("QuickOff: native command is registered (description only; not executed)");
    Ok(())
}

fn download_info() -> io::Result<()> {
    let receiver = native_transport::receiver()?;
    let description = console_capture::capture(|| {
        native_transport::send(&receiver, native_transport::Request::DownloadDescription)
    })?;
    if !String::from_utf8_lossy(&description).contains("func. eDlmPrintInfo") {
        return Err(io::Error::other("download diagnostic registration was not verified"));
    }
    let captured = console_capture::capture(|| {
        native_transport::send(&receiver, native_transport::Request::DownloadInfo)
    })?;
    if !String::from_utf8_lossy(&captured).contains("Download Information") {
        return Err(io::Error::other("download diagnostic output was not captured"));
    }
    fs::write("/tmp/mt/native-download-info.log", &captured)?;
    println!("Download diagnostic captured {} bytes", captured.len());
    Ok(())
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = if args == ["--describe"] {
        query()
    } else if args == ["--download-info"] {
        download_info()
    } else if args == ["--save-quick-start"] {
        save_quick_start::run()
    } else if args == ["--restore-console"] {
        console_capture::restore()
    } else {
        Err(io::Error::other(
            "usage: native_power --describe | --download-info | --save-quick-start | --restore-console",
        ))
    };
    if let Err(error) = result {
        eprintln!("native power query: {error}");
        std::process::exit(1);
    }
}
