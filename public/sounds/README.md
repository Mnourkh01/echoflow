# Custom notification sounds

Select **Sound → Custom** in Settings to play real recorded start/stop cues
instead of the built-in synthesized ones. Real recordings are louder and cleaner.

Two files ship here by default (see CREDITS.txt):

- `start.ogg` — plays when recording starts
- `stop.ogg` — plays when recording stops

To use your own, replace those two files (keep the same names). `.ogg`, `.wav`,
and `.mp3` all play; if you use a different extension, update `SAMPLE_URLS` in
`src/lib/sound.ts`.

Use **royalty-free** audio only. Do NOT use copyrighted system sounds
(iPhone / Samsung / Android). Legal, free sources:

- https://pixabay.com/sound-effects/ (search "notification", "pop", "click")
- https://mixkit.co/free-sound-effects/notification/
- https://freesound.org (filter by CC0)
- https://kenney.nl/assets/interface-sounds (CC0, the pack the defaults came from)

Files here are bundled into the app at build time, so they work fully offline.
