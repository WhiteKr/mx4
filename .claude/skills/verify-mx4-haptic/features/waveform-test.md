# Waveform test

`mx4d --test` is the user's quick check after install: it plays all 16 built-in waveforms on the mouse, 1.5 s apart, printing `OK` or `FAIL` for each.

## Sub-features

- `test-all` 16 packets, waveforms in ascending index order 0x00…0x0E then 0x1B, each line `OK`, exit code 0.
- `test-config` uses `path`, `number` and `feature_index` from the `[device]` section of the config file.

## How to get to it (user POV)

- Run `mx4d --test` in a terminal (README "Quick Test").

## Driving it with verify.sh

Preconditions:

- cargo and a Python with the two dependencies are available (see `../SKILL.md` Launch). No `launch` is needed; test-mode makes its own fake device and needs no bus.

- **Run the test.** Run `$V test-mode` (about 24 s). Output shows `MX Master 4 Haptic Test — playing all waveforms`, 16 `[0x..] NAME... OK` lines, `Done.`, `exit=0`, then 16 decoded packets ending with `wave=0x1B WHISPER_COLLISION`, none `MALFORMED`.
- **Proof.** Evidence is written automatically to `$HOME/mx4-verify-evidence/test-<timestamp>/` (`test-mode.out`, `packets-test-mode.txt`); the command prints the path.

## Gotchas

- `OK` means the write did not error; a regular file always accepts writes. Only the decoded packets show the bytes are right, and only a human holding the mouse can confirm it vibrates.
- test-mode reads the repo `config.toml`; `MX4_CONFIG` does not apply here.
- On the real device a failed write triggers the `/dev/hidraw*` scan described in `../SKILL.md`. Never run `--test` against real hardware unattended.
