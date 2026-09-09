# Quinn

A local-only AI command assistant for GNOME. Quinn keeps the UI in a thin GNOME Shell extension and runs Needle v2 inference plus command execution in a Rust daemon over the session D-Bus.

## Current architecture

```text
GNOME Shell extension (GJS)
        │ D-Bus session bus
        ▼
Rust quinn-daemon
  ├─ Needle v2 / needle-infer
  ├─ deterministic multi-command splitting
  ├─ confidence gate
  └─ local desktop/audio tools
```

## Implemented now

- Rust workspace and `quinn-daemon` crate.
- Needle v2 `V2Engine` integration using `needle-infer 0.2.1`.
- Resident model loaded once at daemon startup.
- D-Bus service `org.quinn.Assistant` at `/org/quinn/Assistant`.
- `Ask(s)` returning `(response, tool_calls)`.
- `Ready` property.
- `ToolExecuted` signal.
- Tools: `open_application`, `open_terminal`, `set_volume`.
- Deterministic splitting of multi-command requests (`and`, `then`, `+`, comma).
- Confidence threshold of 0.70 before execution.
- GNOME Shell panel button, popup entry, and `Super+Space` shortcut.
- Per-user systemd service.
- Installer with one-time Needle v2 model download.
- Rust CI for formatting, checking, and clippy.

## Model

Needle v2 is a single `needle2.cact` file containing weights and tokenizer. Quinn expects it at:

`~/.local/share/quinn/weights/needle2.cact`

Override the path with `QUINN_MODEL=/path/to/needle2.cact`.

## Build / install

Install prerequisites including Rust, `hf` from Hugging Face Hub, GNOME Shell development/runtime pieces, and the usual D-Bus desktop session. Then run:

```bash
bash ./install.sh
```

The installer builds the daemon, installs the GNOME extension, installs the systemd user unit, downloads Needle v2 when absent, and starts Quinn.

## Manual daemon test

```bash
busctl --user introspect org.quinn.Assistant /org/quinn/Assistant
busctl --user call org.quinn.Assistant /org/quinn/Assistant org.quinn.Assistant Ask s "open terminal"
```

## Roadmap

### Stage 1 — foundation
- [x] Rust workspace
- [x] Needle v2 loading/inference
- [x] D-Bus service
- [x] Core local tools
- [x] Confidence gate
- [x] Multi-command splitting

### Stage 2 — GNOME integration
- [x] Extension metadata/API skeleton
- [x] Panel indicator
- [x] Popup command entry
- [x] D-Bus client
- [x] Global hotkey
- [ ] Verify against installed GNOME Shell version on target machine
- [ ] Add live tool-execution state to the popup
- [ ] Add polished error/ready states

### Stage 3 — voice and richer actions
- [ ] Add prerecorded `.ogg` acknowledgements with `rodio`
- [ ] Add failure/low-confidence voice feedback
- [ ] Add `set_brightness(percent)`
- [ ] Add `search_files(query)`
- [ ] Add `create_reminder(text, when)`
- [ ] Expand desktop-app resolution edge cases

### Stage 4 — performance / reliability
- [ ] Benchmark cold load and warm inference on low-end CPUs
- [ ] Reuse inference state where safe
- [ ] Avoid unnecessary full-causal confidence-cache allocations
- [ ] Add structured logging and diagnostics
- [ ] Add integration tests for D-Bus and tool execution
- [ ] Verify clean startup/restart behavior under systemd --user

### Stage 5 — optional conversational layer
- [ ] Decide whether Quinn needs a second local conversational model
- [ ] Keep Needle focused on routing/tool calls
- [ ] Preserve command-only mode as the low-resource default

### Stage 6 — packaging
- [ ] Finalize extension compatibility range
- [ ] Package/install for Debian-based GNOME desktops
- [ ] Add uninstall script
- [ ] Add release artifacts

See `Ai Mem.txt` for the cross-chat project handoff state.
