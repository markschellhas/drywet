use std::io::{self, Write};

use drywet::limits::{DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE};
use drywet::{run, run_with_config, BufferSink, DeviceSink};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let result: Result<(), String> = if std::env::args().skip(1).eq(["--buffer"]) {
        run(
            stdin.lock(),
            stdout.lock(),
            BufferSink::new(DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS),
        )
        .map(|_| ())
        .map_err(|err| err.to_string())
    } else {
        DeviceSink::new()
            .map_err(|err| err.to_string())
            .and_then(|sink| {
                let sample_rate = sink.sample_rate();
                let channels = sink.channels();
                run_with_config(stdin.lock(), stdout.lock(), sample_rate, channels, sink)
                    .map(|_| ())
                    .map_err(|err| err.to_string())
            })
    };
    if let Err(err) = result {
        let _ = writeln!(io::stderr(), "{err}");
        std::process::exit(1);
    }
}
