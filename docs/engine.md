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

The supported top-level schedule fields are `sequence`, `part`, and `loop`. A boolean `loop` enables transport looping; an object describes a repeating loop event:

```json
{"cmd":"start","bpm":90,"loop":{"interval":"4n","note":"kick","duration":"16n"}}
```

## Commands

- `warmup` initializes the engine.
- `start` schedules the supplied phrase and starts the transport.
- `stop`, `pause`, and `resume` control transport state.
- `bpm` changes tempo.
- `play-midi` triggers a MIDI note for a duration.
- `note-on`/`trigger_attack` and `note-off`/`trigger_release` control a voice.
- `shutdown` ends the process.

Responses are JSON objects such as `{"ok":true}`. Starting transport also emits a `started` event with latency and position fields. Invalid commands or payloads return `{"error":"..."}`.

## Process lifetime

The native engine owns a `DeviceSink`, so it must remain running while audio is playing. For an offline or test harness, use `--buffer`; the engine then renders into memory without opening the default audio device.
