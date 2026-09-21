//! Pipe a short NDJSON session into `drywet-engine --buffer`.
//!
//! ```text
//! cargo run --example engine_session
//! ```

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

const SESSION: &[&str] = &[
    r#"{"cmd":"warmup","instrument":"drum"}"#,
    r#"{"cmd":"start","bpm":100,"loop":true,"schedule":{"type":"loop","interval":"4n","note":"kick","duration":"16n"}}"#,
    r#"{"cmd":"play-midi","note":"hat","duration":"32n"}"#,
    r#"{"cmd":"stop"}"#,
    r#"{"cmd":"shutdown"}"#,
];

fn main() -> std::io::Result<()> {
    let mut child = Command::new("cargo")
        .args(["run", "--quiet", "--bin", "drywet-engine", "--", "--buffer"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    {
        let stdin = child.stdin.as_mut().expect("engine stdin");
        for line in SESSION {
            writeln!(stdin, "{line}")?;
        }
    }

    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines() {
            println!("{}", line?);
        }
    }
    child.wait()?;
    Ok(())
}
