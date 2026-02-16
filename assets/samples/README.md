# Sample audio and lyrics for Brotviz review

## Files

- `yankee_doodle_choral.ogg`
- `yankee_doodle_choral.lrc`

## Source and rights

Audio source:
- https://commons.wikimedia.org/wiki/File:Yankee_Doodle_(choral).ogg
- Direct media URL: https://upload.wikimedia.org/wikipedia/commons/c/c4/Yankee_Doodle_%28choral%29.ogg

Lyrics source reference:
- https://en.wikipedia.org/wiki/Yankee_Doodle
- Song is traditional/public-domain; this LRC is a timing adaptation for local testing.

## Quick review

```sh
afplay assets/samples/yankee_doodle_choral.ogg &
cargo run --release --bin tui_visualizer -- \
  --source system --engine metal --renderer kitty \
  --lyrics-file assets/samples/yankee_doodle_choral.lrc \
  --lyrics-loop true \
  --system-data subtle
```

If lyric timing is early/late, adjust:

```sh
cargo run --release --bin tui_visualizer -- \
  --source system --engine metal --renderer kitty \
  --lyrics-file assets/samples/yankee_doodle_choral.lrc \
  --lyrics-offset-ms 120
```
