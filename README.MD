# Molto-TTS

**Molto-TTS** is a lightweight Windows application that lets you mix and combine installed Windows TTS (SAPI) voices into new, unified custom voices.

With Molto-TTS, you can create a single voice that automatically switches between different backing voices based on what is being read—such as using an Arabic voice for Arabic text, an English voice for English text, and dedicated voices for numbers or punctuation marks. Once created, your custom voices are registered system-wide and work in any application or screen reader that supports Windows SAPI voices.

---

## Features

- **Automatic Language Switching**: Automatically detects the language of incoming text and routes it to the voice you assigned for that language.
- **Numbers & Punctuation Routing**: Choose dedicated voices to read numbers and punctuation marks, either across all languages or with distinct voices for Arabic and English.
- **System-Wide Availability**: Created voices appear as standard Windows voices, compatible with screen readers (NVDA, JAWS, Narrator) and other TTS clients across both 32-bit and 64-bit applications.
- **Low Latency**: Fast, smooth playback with no awkward pauses or lagging when switching between languages and numerals mid-sentence.
- **GUI and CLI**: Configure voices through a simple desktop interface or automate them using command-line arguments.

---

## Building from Source

### Prerequisites

- Windows 10 or 11
- [Rust](https://www.rust-lang.org/) (MSVC toolchain)
- Optional (for 32-bit client support):
  ```powershell
  rustup target add i686-pc-windows-msvc
  ```

### Build

Run the included build script in PowerShell:

```powershell
# Build debug binaries
powershell -ExecutionPolicy Bypass -File .\build.ps1

# Build release binaries
powershell -ExecutionPolicy Bypass -File .\build.ps1 -Release
```

The output files will be in `target\debug\` (or `target\release\`):
- `molto-tts.exe`
- `molto_tts_engine.dll` (64-bit engine)
- `molto_tts_engine_x86.dll` (32-bit engine)

---

## Testing

Run the workspace test suite:

```powershell
cargo test --workspace
```

---

## Usage

### GUI Mode

Launch `molto-tts.exe`:

1. Click **+ Create New Voice...**.
2. Give your voice a name and choose a default fallback voice.
3. Add one or more language rules (e.g. English `en`, Arabic `ar`) and assign a backing voice to each.
4. Set your preferences for how numbers and punctuation should be read.
5. Click **Create & Register Voice**.

You can also test any voice directly from the main window, edit existing configurations, delete voices, or click **Clear All Voices** to remove all custom profiles and clean up registered tokens.

### CLI Mode

You can also manage voices via the command line:

```powershell
# List custom voices and installed system voices
molto-tts list

# Create a voice
molto-tts create --name "MyVoice" --default "<VOICE_ID>" --lang en="<VOICE_ID>" --lang ar="<VOICE_ID>" --unified "<VOICE_ID>"

# Test speak text
molto-tts test --name "MyVoice" --text "Hello world! 123. مرحباً بكم."

# Delete a voice
molto-tts delete --name "MyVoice"

# Clear all custom voices
molto-tts clear-all
```

*Note: Running `molto-tts` in an elevated (Administrator) command prompt registers voices system-wide for all users and applications.*

---

## Contributing

Contributions, feedback, and bug reports are welcome. Please ensure that all tests pass (`cargo test --workspace`) before submitting a pull request.

---

## License

This project is licensed under the **Personal Use and Upstream Contribution License (PUCL) 1.0**. See [`LICENSE.MD`](LICENSE.MD) for terms and conditions.
