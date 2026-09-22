# Engine protocol

<!-- kicker: JSON lines -->

Run the engine with native output:

```sh
cargo run --bin drywet-engine
```

Use `--buffer` when a caller needs deterministic in-memory rendering instead of a system device. The process reads one JSON object per line and writes one JSON response per command.

## Start a phrase

```json
{"cmd":"start","bpm":120,"loop":true,"sequence":{"events":["C4","E4","G4"],"subdivision":"8n"}}
```

`start` + `sequence` (or `part` / a `loop` object) mixes the phrase before the engine replies `started`, and keeps ticking while Transport is started. Stdin is not the clock — an idle host still hears the arrangement. A boolean `loop` enables transport looping and sets loop points from the phrase length so the sequence repeats; an object describes a repeating loop event:

```json
{"cmd":"start","bpm":90,"loop":{"interval":"4n","note":"kick","duration":"16n"}}
```

## Commands

- `warmup` initializes the engine.
- `start` schedules the supplied phrase, starts the transport, and mixes due events. Boolean `loop` repeats the phrase; `play-midi` is live hits only.
- `stop`, `pause`, and `resume` control transport state. `pause` stops ticking; `resume` starts the pump again.
- `bpm` changes tempo.
- `play-midi` triggers a MIDI note at the write cursor. Do not use it as the arrangement clock.
- `note-on`/`trigger_attack` and `note-off`/`trigger_release` control a voice.
- `shutdown` ends the process.

Responses are JSON objects such as `{"ok":true}`. Starting transport also emits a `started` event with latency and position fields. Invalid commands or payloads return `{"error":"..."}`.

## Process lifetime

The native engine owns a `DeviceSink`, so it must remain running while audio is playing. For an offline or test harness, use `--buffer`; the engine then renders into memory without opening the default audio device.
