use std::io::{self, Write};

use drywet::limits::{DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE};
use drywet::{run, BufferSink, PipeWireSink};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let result = if std::env::args().skip(1).eq(["--buffer"]) {
        run(
            stdin.lock(),
            stdout.lock(),
            BufferSink::new(DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS),
        )
        .map(|_| ())
    } else {
        run(
            stdin.lock(),
            stdout.lock(),
            PipeWireSink::new(DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS),
        )
        .map(|_| ())
    };
    if let Err(err) = result {
        let _ = writeln!(io::stderr(), "{err}");
        std::process::exit(1);
    }
}
