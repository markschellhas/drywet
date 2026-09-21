# drywet examples

Runnable sketches for the musician API. Documented in [docs/examples.md](../docs/examples.md).

```text
cargo run --example metronome
cargo run --example chords
cargo run --example drums
cargo run --example sixeight
cargo run --example bassline
cargo run --example piano
cargo run --example jam
cargo run --example render
cargo run --example engine_session
```

Live examples open the system's default device through CPAL (CoreAudio on
macOS). `render` and `engine_session --buffer` stay offline. The `piano`
example additionally needs `samples/piano/*.wav`.
